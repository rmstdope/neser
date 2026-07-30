//! BG (background) tile pipeline for Modes 0 and 1.
//!
//! Scroll registers use the SNES shared write-twice "BG_old" latch: a single 8-bit latch is
//! shared across all eight `BGnHOFS`/`BGnVOFS` writes. The 10-bit scroll value is rebuilt on each
//! write per the fullsnes formula.

use super::{CGRAM_SIZE, Ppu, VRAM_SIZE};

/// A front-to-back priority slot in the Background Priority Chart: either a BG layer at a given
/// tile-priority, or an OBJ priority level (0-3).
#[derive(Clone, Copy)]
enum Slot {
    Bg(usize, bool),
    Obj(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenTarget {
    Main,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PixelSource {
    Bg(usize),
    Obj {
        priority: u8,
        palette: u8,
    },
    #[default]
    Backdrop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowLayer {
    Bg(usize),
    Obj,
    Math,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScreenPixel {
    pub color: u16,
    pub source: PixelSource,
}

impl Ppu {
    /// Handle a write to `BGnHOFS` (horizontal scroll, write-twice via the shared BG_old latch):
    /// `hofs = (data << 8) | (bg_old & ~7) | ((hofs >> 8) & 7)`.
    ///
    /// The value is stored unmasked; the self-referential `(hofs >> 8) & 7` term depends on the
    /// full previous value, so masking here would corrupt the hardware write-twice behavior.
    /// Callers mask to the active scroll width when applying it.
    pub(super) fn write_bg_hofs(&mut self, bg: usize, value: u8) {
        let prev = self.bg_old as u16;
        let old_high = (self.bg_hofs[bg] >> 8) & 0x07;
        self.bg_hofs[bg] = ((value as u16) << 8) | (prev & !0x07) | old_high;
        self.bg_old = value;
    }

    /// Handle a write to `BGnVOFS` (vertical scroll, write-twice via the shared BG_old latch):
    /// `vofs = (data << 8) | bg_old`.
    pub(super) fn write_bg_vofs(&mut self, bg: usize, value: u8) {
        let prev = self.bg_old as u16;
        self.bg_vofs[bg] = ((value as u16) << 8) | prev;
        self.bg_old = value;
    }

    /// Resolve the front-most main- and sub-screen pixels at visible screen `(x, y)`.
    ///
    /// Mode 7 has its own layer resolver; hi-res output keeps the standard resolver even in
    /// mode 7 (mode 7 + pseudo-hires renders backdrop only, as before this refactor).
    pub(super) fn resolve_pixel_pair(&self, x: u16, y: u16) -> (ScreenPixel, ScreenPixel) {
        if self.bg_mode == 7 && !self.hires_output_enabled() {
            (
                self.resolve_mode7_screen_pixel(ScreenTarget::Main, x, y),
                self.resolve_mode7_screen_pixel(ScreenTarget::Sub, x, y),
            )
        } else {
            (
                self.resolve_screen_pixel(ScreenTarget::Main, x, y),
                self.resolve_screen_pixel(ScreenTarget::Sub, x, y),
            )
        }
    }

    pub(super) fn resolve_screen_pixel(&self, target: ScreenTarget, x: u16, y: u16) -> ScreenPixel {
        // CGWSEL bit 1 clear means the color-math operand is the fixed color rather
        // than the sub screen; outside hires that makes an un-consumed sub resolve
        // equivalent to backdrop, so it is skipped. In hires output the sub screen is
        // DISPLAYED (even columns) and always renders from TS (Mesen2 renders the sub
        // screen unconditionally).
        let sub_hidden =
            target == ScreenTarget::Sub && self.cgwsel & 0x02 == 0 && !self.hires_output_enabled();
        let obj_pixel = if self.screen_enable_mask(target) & 0x10 == 0 || sub_hidden {
            None
        } else {
            self.obj_pixel_at(x, y)
        };
        for &slot in self.layer_order().iter() {
            match slot {
                Slot::Bg(bg, priority) => {
                    if self.screen_enable_mask(target) & (1 << bg) == 0 {
                        continue;
                    }
                    if sub_hidden {
                        continue;
                    }
                    if self.layer_disabled_by_window(target, WindowLayer::Bg(bg), x, y) {
                        continue;
                    }
                    // In true hires (modes 5/6) the sub screen fetches the even
                    // half-pixel (2x) and the main screen the odd one (2x+1).
                    let hires_half =
                        u16::from(self.true_hires_enabled() && target == ScreenTarget::Main);
                    if let Some((color, pixel_priority)) = self.bg_pixel(bg, x, y, hires_half)
                        && pixel_priority == priority
                    {
                        return ScreenPixel {
                            color,
                            source: PixelSource::Bg(bg),
                        };
                    }
                }
                Slot::Obj(priority) => {
                    if self.layer_disabled_by_window(target, WindowLayer::Obj, x, y) {
                        continue;
                    }
                    if let Some(pixel) = obj_pixel.filter(|pixel| pixel.priority == priority) {
                        return ScreenPixel {
                            color: pixel.color,
                            source: PixelSource::Obj {
                                priority,
                                palette: pixel.palette,
                            },
                        };
                    }
                }
            }
        }
        ScreenPixel {
            color: self.backdrop_color_for(target),
            source: PixelSource::Backdrop,
        }
    }

    pub(super) fn screen_enable_mask(&self, target: ScreenTarget) -> u8 {
        match target {
            ScreenTarget::Main => self.tm,
            ScreenTarget::Sub => self.ts,
        }
    }

    pub(super) fn backdrop_color_for(&self, target: ScreenTarget) -> u16 {
        match target {
            ScreenTarget::Main => self.backdrop_color(),
            ScreenTarget::Sub => self.coldata & 0x7FFF,
        }
    }

    pub(super) fn compose_pixels(
        &self,
        x: u16,
        y: u16,
        main: ScreenPixel,
        sub: ScreenPixel,
    ) -> u16 {
        let (operand, fixed_color_fallback) = self.math_operand(sub);
        self.compose_half(x, y, main.color, main.source, operand, fixed_color_fallback)
    }

    /// The colour-math operand for `sub`, plus whether it is the
    /// **empty-sub-screen fallback**.
    ///
    /// CGWSEL bit 1 asks for the sub screen as the operand; when nothing is on
    /// the sub screen at this dot the fixed colour is substituted *and* halving
    /// is disabled (Mesen2's `_subScreenPriority[x] == 0` branch, #3012). With
    /// bit 1 clear the fixed colour is the operand by design and halving still
    /// applies -- the two cases look alike but behave differently.
    fn math_operand(&self, sub: ScreenPixel) -> (u16, bool) {
        self.math_operand_with(sub, sub.color)
    }

    /// As [`Self::math_operand`], but with the coverage test and the operand given
    /// separately.
    ///
    /// `coverage` decides whether the sub screen has a pixel at the dot being
    /// tested; `covered_operand` is the operand to use when it does. The two
    /// coincide everywhere except the hires even half, where Mesen2 gates a
    /// MAIN-pixel operand on SUB coverage at x-1 (#3035).
    fn math_operand_with(&self, coverage: ScreenPixel, covered_operand: u16) -> (u16, bool) {
        let fixed_color = self.coldata & 0x7FFF;
        if self.cgwsel & 0x02 == 0 {
            (fixed_color, false)
        } else if coverage.source == PixelSource::Backdrop {
            (fixed_color, true)
        } else {
            (covered_operand, false)
        }
    }

    /// Finalize one output pixel: colour-window clip to black, the CGADSUB gate
    /// from `gate_source`, then colour math against `operand`.
    ///
    /// Ordering follows Mesen2 `ApplyColorMathToPixel`: the clip applies first
    /// and unconditionally (it is NOT gated by the per-layer CGADSUB bit), then
    /// the layer gate, then the prevent-window, then the maths.
    ///
    /// Halving (CGADSUB bit 6) is suppressed in two independent cases: when the
    /// main pixel was clipped to black, and when `operand` is the
    /// empty-sub-screen fixed-colour fallback.
    fn compose_half(
        &self,
        x: u16,
        y: u16,
        color: u16,
        gate_source: PixelSource,
        operand: u16,
        fixed_color_fallback: bool,
    ) -> u16 {
        let mut color = color;
        let mut half = self.cgadsub & 0x40 != 0;
        if self.force_main_black_at(x, y) {
            color = 0;
            // A blacked-out main pixel takes the operand at full strength.
            // ares applies this uniformly (`colorHalve && above.colorEnable`),
            // CGWSEL clip mode 3 included; Mesen2 omits it for mode 3 alone,
            // and NESER deliberately follows ares here (#3011).
            half = false;
        }
        if !self.color_math_source_enabled(gate_source) {
            return color;
        }
        if !self.color_math_enabled_at(x, y) {
            return color;
        }
        if fixed_color_fallback {
            half = false;
        }
        self.apply_color_math(color, operand, half)
    }

    /// Finalize the hires half-pixel pair at native `(x, y)` from the line buffers,
    /// returning `(even, odd)` output colors (Mesen2 ApplyColorMath, hires branch).
    pub(super) fn compose_hires_pair(&self, x: u16, y: u16) -> (u16, u16) {
        let xi = x as usize;
        let main = self.line_main[xi];
        let sub = self.line_sub[xi];
        // Odd/main half: operand is the pre-math sub color at the same x, and an
        // empty sub screen there is the halve-disabling fallback (#3012). When
        // CGWSEL bit 1 is clear both halves take the COLDATA fixed colour instead
        // (Mesen2 ApplyColorMathToPixel's !ColorMathAddSubscreen branch is
        // unconditional); `math_operand_with` handles that for either half.
        let (main_operand, main_fallback) = self.math_operand(sub);
        let odd = self.compose_half(x, y, main.color, main.source, main_operand, main_fallback);
        // Even/sub half: operand is the finalized main pixel one dot to the left
        // (black at x = 0), and the math gate follows the main pixel's source at
        // x - 1 (Mesen2 passes prevX for the sub half of the hires math loop).
        let prev_main = if x > 0 {
            self.line_main_final[xi - 1]
        } else {
            0
        };
        // Mesen2's hires loop calls the same helper for both halves and passes
        // `prevX` for this one, and that parameter is what indexes
        // `_subScreenPriority` -- so the even half's operand, a MAIN pixel at x-1,
        // is gated on SUB coverage at x-1. Where the sub screen is empty there the
        // operand becomes COLDATA and halving is dropped, even though the intended
        // operand was a main pixel. Semantically odd but hardware-modelled: NESER
        // follows Mesen2 here, unlike the ares-favouring splits in #3011/#3003,
        // because ares composes hires colour math differently and offers no
        // competing cross-index model -- absence of evidence, not counter-evidence
        // (#3035).
        let prev_sub = self.line_sub[xi.saturating_sub(1)];
        let (even_operand, even_fallback) = self.math_operand_with(prev_sub, prev_main);
        let gate_source = self.line_main[xi.saturating_sub(1)].source;
        // The sub resolve represents its backdrop with the COLDATA fixed color (the
        // color-math operand on hardware), but the DISPLAYED sub backdrop is CGRAM
        // color 0, like the main screen (Mesen2 RenderBgColor).
        let sub_display = if sub.source == PixelSource::Backdrop {
            self.cgram_color(0)
        } else {
            sub.color
        };
        let even = self.compose_half(x, y, sub_display, gate_source, even_operand, even_fallback);
        (even, odd)
    }

    /// Whether colour math runs at this dot (CGWSEL bits 5-4).
    ///
    /// The field names where maths is **prevented**, not where it is enabled:
    /// 1 = prevented outside the colour window (so maths runs INSIDE), 2 =
    /// prevented inside (so maths runs OUTSIDE). NESER had those two the wrong
    /// way round (#3011); see the vendored `CGWSEL::prevent` constants.
    pub(super) fn color_math_enabled_at(&self, x: u16, y: u16) -> bool {
        match (self.cgwsel >> 4) & 0x03 {
            0 => true,
            1 => self.window_area(WindowLayer::Math, x, y),
            2 => !self.window_area(WindowLayer::Math, x, y),
            _ => false,
        }
    }

    /// Whether the main pixel is clipped to black at this dot (CGWSEL bits
    /// 7-6), naming where the clip **applies**: 1 = outside the colour window,
    /// 2 = inside, 3 = always.
    pub(super) fn force_main_black_at(&self, x: u16, y: u16) -> bool {
        match (self.cgwsel >> 6) & 0x03 {
            0 => false,
            1 => !self.window_area(WindowLayer::Math, x, y),
            2 => self.window_area(WindowLayer::Math, x, y),
            _ => true,
        }
    }

    pub(super) fn color_math_source_enabled(&self, source: PixelSource) -> bool {
        match source {
            PixelSource::Bg(bg) => self.cgadsub & (1 << bg) != 0,
            PixelSource::Obj { palette, .. } => palette >= 4 && self.cgadsub & 0x10 != 0,
            PixelSource::Backdrop => self.cgadsub & 0x20 != 0,
        }
    }

    pub(super) fn layer_disabled_by_window(
        &self,
        target: ScreenTarget,
        layer: WindowLayer,
        x: u16,
        y: u16,
    ) -> bool {
        let disable_mask = match target {
            ScreenTarget::Main => self.tmw,
            ScreenTarget::Sub => self.tsw,
        };
        let bit = match layer {
            WindowLayer::Bg(bg) => bg as u8,
            WindowLayer::Obj => 4,
            WindowLayer::Math => return false,
        };
        if disable_mask & (1 << bit) == 0 {
            return false;
        }
        self.window_area(layer, x, y)
    }

    /// Rebuild the decoded window cache from the raw W12SEL/W34SEL/WOBJSEL,
    /// WH0-WH3 and WBGLOG/WOBJLOG registers.
    ///
    /// Called from every one of those register writes and from save-state
    /// restore -- the raw registers are the persisted state, this is a cache.
    ///
    /// Each selector register holds two `EIei` nibbles, one per layer, and
    /// within every 2-bit pair the HIGH bit is the enable and the LOW bit the
    /// invert (see [`WindowConfig`]). The mask-logic bit pair is indexed by
    /// LAYER, independently of which nibble of the selector that layer used:
    /// WBGLOG is `44332211` (BG1 = bits 1-0 ... BG4 = bits 7-6) and WOBJLOG is
    /// `----ccoo` (OBJ = bits 1-0, colour window = bits 3-2). Deriving the two
    /// from a single "high nibble?" flag is what made BG3/BG4 read BG1/BG2's
    /// mask logic (#3011).
    pub(super) fn decode_window_registers(&mut self) {
        for (selector, base_layer) in [(self.w12sel, 0), (self.w34sel, 2), (self.wobjsel, 4)] {
            for nibble in 0..2 {
                let layer = base_layer + nibble;
                let bits = selector >> (nibble * 4);
                self.windows[0].active_layers[layer] = bits & 0x02 != 0;
                self.windows[0].inverted_layers[layer] = bits & 0x01 != 0;
                self.windows[1].active_layers[layer] = bits & 0x08 != 0;
                self.windows[1].inverted_layers[layer] = bits & 0x04 != 0;
            }
        }
        self.windows[0].left = self.wh[0];
        self.windows[0].right = self.wh[1];
        self.windows[1].left = self.wh[2];
        self.windows[1].right = self.wh[3];
        for layer in 0..4 {
            self.mask_logic[layer] = (self.wbglog >> (layer * 2)) & 0x03;
        }
        self.mask_logic[4] = self.wobjlog & 0x03;
        self.mask_logic[5] = (self.wobjlog >> 2) & 0x03;
    }

    /// Whether `x` is in `layer`'s combined window area.
    ///
    /// Mirrors Mesen2 `ProcessMaskWindow`: with both windows enabled the
    /// per-layer mask-logic operator combines them; with exactly ONE enabled
    /// that window's area is used directly and the operator is bypassed (so an
    /// AND setting does not turn a lone window into "masked nowhere"); with
    /// neither enabled nothing is masked, XNOR included.
    pub(super) fn window_area(&self, layer: WindowLayer, x: u16, _y: u16) -> bool {
        let layer = Self::window_layer_index(layer);
        let x = x as u8;
        let one_active = self.windows[0].active_layers[layer];
        let two_active = self.windows[1].active_layers[layer];
        match (one_active, two_active) {
            (false, false) => false,
            (true, false) => self.windows[0].pixel_in_area(layer, x),
            (false, true) => self.windows[1].pixel_in_area(layer, x),
            (true, true) => {
                let one = self.windows[0].pixel_in_area(layer, x);
                let two = self.windows[1].pixel_in_area(layer, x);
                match self.mask_logic[layer] {
                    0 => one || two,
                    1 => one && two,
                    2 => one != two,
                    _ => one == two,
                }
            }
        }
    }

    /// BG1-BG4 = 0-3, OBJ = 4, colour window = 5 (Mesen2's layer indices).
    fn window_layer_index(layer: WindowLayer) -> usize {
        match layer {
            WindowLayer::Bg(bg) => bg,
            WindowLayer::Obj => 4,
            WindowLayer::Math => 5,
        }
    }

    /// CGADSUB add/subtract with an explicit `half`; the caller decides halving
    /// because hardware suppresses it in cases CGADSUB bit 6 alone cannot express
    /// (see [`Self::compose_half`]).
    fn apply_color_math(&self, main: u16, sub: u16, half: bool) -> u16 {
        let subtract = self.cgadsub & 0x80 != 0;
        let mut out = 0u16;
        for shift in [0, 5, 10] {
            let a = ((main >> shift) & 0x1F) as i16;
            let b = ((sub >> shift) & 0x1F) as i16;
            let value = if subtract { a - b } else { a + b };
            let value = if half { value / 2 } else { value };
            let value = value.clamp(0, 0x1F) as u16;
            out |= value << shift;
        }
        out
    }

    /// Front-to-back priority slots for the current mode per the fullsnes Background Priority Chart.
    fn layer_order(&self) -> &'static [Slot] {
        use Slot::{Bg, Obj};
        match self.bg_mode {
            0 => &[
                Obj(3),
                Bg(0, true),
                Bg(1, true),
                Obj(2),
                Bg(0, false),
                Bg(1, false),
                Obj(1),
                Bg(2, true),
                Bg(3, true),
                Obj(0),
                Bg(2, false),
                Bg(3, false),
            ],
            // Mode 1 with BG3 high-priority (BGMODE bit 3): BG3.1 moves to the very front (BG3.1a).
            1 if self.bg3_priority => &[
                Bg(2, true),
                Obj(3),
                Bg(0, true),
                Bg(1, true),
                Obj(2),
                Bg(0, false),
                Bg(1, false),
                Obj(1),
                Obj(0),
                Bg(2, false),
            ],
            1 => &[
                Obj(3),
                Bg(0, true),
                Bg(1, true),
                Obj(2),
                Bg(0, false),
                Bg(1, false),
                Obj(1),
                Bg(2, true),
                Obj(0),
                Bg(2, false),
            ],
            // Modes 2-5 display BG1 + BG2 (BG3 in modes 2/4 is the offset-per-tile source).
            2..=5 => &[
                Obj(3),
                Bg(0, true),
                Obj(2),
                Bg(1, true),
                Obj(1),
                Bg(0, false),
                Obj(0),
                Bg(1, false),
            ],
            // Mode 6 displays BG1 only (BG3 is the offset source).
            6 => &[Obj(3), Bg(0, true), Obj(2), Obj(1), Bg(0, false), Obj(0)],
            _ => &[],
        }
    }

    /// Bits-per-pixel for a BG layer in the current mode.
    fn bg_bpp(&self, bg: usize) -> u8 {
        match self.bg_mode {
            0 => 2,
            1 => {
                if bg < 2 {
                    4
                } else {
                    2
                }
            }
            2 => 4,
            3 => {
                if bg == 0 {
                    8
                } else {
                    4
                }
            }
            4 => {
                if bg == 0 {
                    8
                } else {
                    2
                }
            }
            5 => {
                if bg == 0 {
                    4
                } else {
                    2
                }
            }
            6 => 4,
            _ => 2,
        }
    }

    /// CGRAM base for a BG layer's palettes. In Mode 0 each BG uses a separate 32-entry region
    /// (BG1=0, BG2=32, BG3=64, BG4=96); other modes share the low CGRAM.
    fn bg_palette_base(&self, bg: usize) -> u8 {
        match self.bg_mode {
            0 => (bg as u8) * 32,
            _ => 0,
        }
    }

    /// Resolve `(BGR555 color, priority)` for BG layer `bg` at screen `(x, y)`, or `None` if the
    /// pixel is transparent (color 0). Supports 8x8/16x16 tiles, all four tilemap sizes, 2/4/8 bpp,
    /// direct-color mode, and offset-per-tile (modes 2/4/6).
    ///
    /// `hires_half` selects the half-pixel (0 = even/sub, 1 = odd/main) in true hires
    /// (modes 5/6), where BG1/BG2 fetch at doubled horizontal resolution; it is 0 otherwise.
    fn bg_pixel(&self, bg: usize, x: u16, y: u16, hires_half: u16) -> Option<(u16, bool)> {
        let bpp = self.bg_bpp(bg);
        let size16 = self.bg_tile_size_16[bg];

        // Apply horizontal mosaic: snap x to the left edge of its block when enabled.
        // In hires the block-start sample is the even half-pixel, which mosaic then
        // replicates into both output columns (Mesen2 RenderTilemap mosaic hold).
        let (x, hires_half) = if self.mosaic_bg_enabled(bg) {
            (self.mosaic_apply_x(x), 0)
        } else {
            (x, hires_half)
        };

        if self.true_hires_enabled() {
            return self.bg_pixel_hires(bg, bpp, size16, x, y, hires_half);
        }

        let cell_shift = if size16 { 4 } else { 3 };
        let cell_mask = (1u16 << cell_shift) - 1;

        let (scrolled_x, scrolled_y) = self.effective_offsets(bg, x, y);

        let entry = self.read_bg_map_entry(bg, scrolled_x >> cell_shift, scrolled_y >> cell_shift);
        let (char_num, palette, priority, hflip, vflip) = decode_bg_map_entry(entry);

        // Cell-relative coordinates (0..cell_size), with flip applied to the whole cell.
        let mut within_x = scrolled_x & cell_mask;
        let mut within_y = scrolled_y & cell_mask;
        if hflip {
            within_x = cell_mask - within_x;
        }
        if vflip {
            within_y = cell_mask - within_y;
        }

        // For 16x16 tiles, the cell is a 2x2 block of 8x8 tiles (N, N+1, N+16, N+17).
        let mut tile = char_num;
        if size16 {
            if within_x & 8 != 0 {
                tile += 1;
            }
            if within_y & 8 != 0 {
                tile += 16;
            }
        }
        let fine_x = (within_x & 7) as u8;
        let fine_y = (within_y & 7) as u8;

        self.bg_tile_color(bg, bpp, tile, palette, fine_x, fine_y)
            .map(|color| (color, priority))
    }

    /// True-hires (modes 5/6) BG fetch at doubled horizontal resolution: a tilemap entry
    /// spans 16 half-pixel columns pairing chars N/N+1 (regardless of the BGMODE tile-size
    /// bit, which only pairs vertically), with the horizontal scroll applied in half-pixel
    /// units (Mesen2 RenderTilemap hires fetch / GetChrData largeTileWidth).
    fn bg_pixel_hires(
        &self,
        bg: usize,
        bpp: u8,
        size16: bool,
        x: u16,
        y: u16,
        hires_half: u16,
    ) -> Option<(u16, bool)> {
        let hx = (x << 1) | hires_half;
        let (entry_col, hoffset, voffset_map, voffset_chr) =
            self.effective_offsets_hires(bg, hx, y);

        let v_shift = if size16 { 4 } else { 3 };
        let v_mask = (1u16 << v_shift) - 1;
        let entry = self.read_bg_map_entry(bg, entry_col, voffset_map >> v_shift);
        let (char_num, palette, priority, hflip, vflip) = decode_bg_map_entry(entry);

        // Flips apply across the whole 16-wide pair (and 16-tall block for size16).
        let mut within_x = hoffset & 15;
        if hflip {
            within_x = 15 - within_x;
        }
        let mut within_y = voffset_chr & v_mask;
        if vflip {
            within_y = v_mask - within_y;
        }

        let mut tile = char_num;
        if within_x & 8 != 0 {
            tile += 1;
        }
        if size16 && within_y & 8 != 0 {
            tile += 16;
        }
        let fine_x = (within_x & 7) as u8;
        let fine_y = (within_y & 7) as u8;

        self.bg_tile_color(bg, bpp, tile, palette, fine_x, fine_y)
            .map(|color| (color, priority))
    }

    /// Decode the tile pixel and resolve its CGRAM (or direct-color) BGR555 value, or
    /// `None` when transparent; shared tail of the native and hires BG fetch paths.
    fn bg_tile_color(
        &self,
        bg: usize,
        bpp: u8,
        tile: u16,
        palette: u8,
        fine_x: u8,
        fine_y: u8,
    ) -> Option<u16> {
        let color =
            self.decode_tile_pixel(self.bg_char_base[bg], tile & 0x03FF, bpp, fine_x, fine_y);
        if color == 0 {
            return None;
        }
        // Direct-color mode (CGWSEL.0) resolves 256-color BGs straight to BGR555.
        if bpp == 8 && self.cgwsel & 0x01 != 0 {
            return Some(direct_color(color, palette));
        }
        // 8bpp (256-color) BGs index CGRAM directly; map-entry palette bits are ignored.
        let index = if bpp == 8 {
            color
        } else {
            let colors_per_palette = if bpp == 2 { 4 } else { 16 };
            self.bg_palette_base(bg) + palette * colors_per_palette + color
        };
        Some(self.cgram_color(index))
    }

    /// Compute the effective hires BG coordinates for layer `bg` at half-pixel column
    /// `hx` (0..512): `(tilemap entry column, hoffset in half-pixel units, voffset for
    /// the tilemap row, voffset for the chr row)`, with the horizontal scroll doubled
    /// and mode 6 offset-per-tile applied to the entry column only (fine x and the
    /// char-pair half always follow BGnHOFS). The two vertical offsets differ only
    /// under mosaic + interlace (see below).
    fn effective_offsets_hires(&self, bg: usize, hx: u16, y: u16) -> (u16, u16, u16, u16) {
        let hscroll = (self.bg_hofs[bg] & 0x03FF) << 1;
        let hoffset = hx.wrapping_add(hscroll) & 0x07FF;
        let mut entry_col = hoffset >> 4;
        let screen_y = y.wrapping_add(1);
        // Screen interlace doubles the vertical fetch in modes 5/6 (the only modes
        // that reach this path): Mesen2 GetTilemapData/GetChrData compute
        // realY = (scanline << 1) | oddFrame when IsDoubleHeight, so each field
        // samples a distinct source line.
        let doubled = self.interlace_enabled();
        let field = u16::from(self.interlace_field);
        let base = if doubled {
            (screen_y << 1) | field
        } else {
            screen_y
        };
        // Mosaic holds the block-start line; under the doubled fetch the subtraction
        // doubles, and Mesen2's two fetch steps diverge: GetTilemapData keeps the
        // field term (map row = 2*block_start + field), GetChrData subtracts it too
        // (chr row = 2*block_start, identical in both fields).
        let (mut real_y_map, mut real_y_chr) = (base, base);
        if self.mosaic_bg_enabled(bg) {
            let m = self.mosaic_vcount as u16;
            real_y_map = real_y_map.wrapping_sub(m);
            real_y_chr = real_y_chr.wrapping_sub(m);
            if doubled {
                real_y_map = real_y_map.wrapping_sub(m);
                real_y_chr = real_y_chr.wrapping_sub(m).wrapping_sub(field);
            }
        }
        let vscroll = self.bg_vofs[bg] & 0x03FF;
        let mut voffset_map = real_y_map.wrapping_add(vscroll) & 0x03FF;
        let mut voffset_chr = real_y_chr.wrapping_add(vscroll) & 0x03FF;

        // Offset-per-tile (mode 6 is the only hires OPT mode; mode-2-style dual H/V
        // entries). The BG3 lookup runs in native units, where one tilemap entry spans
        // 8 native pixels regardless of the tile-size bit, and the first entry column
        // is exempt.
        if self.bg_mode == 6 && bg < 2 {
            let valid_bit = 0x2000u16 << bg; // BG1 -> bit13, BG2 -> bit14
            let offset_x = (hx >> 1).wrapping_add(self.bg_hofs[bg] & 7);
            if offset_x >= 8 {
                let lookup_x = (offset_x - 8).wrapping_add(self.bg_hofs[2] & 0x03F8);
                let bg3_vscroll = self.bg_vofs[2] & 0x03FF;
                let hlookup = self.bg3_offset_entry(lookup_x, bg3_vscroll);
                let vlookup = self.bg3_offset_entry(lookup_x, bg3_vscroll.wrapping_add(8));
                if hlookup & valid_bit != 0 {
                    // Mesen2 GetTilemapData ORs the OPT value into the doubled
                    // scroll's bits 3-9 and shifts back down: the entry column moves
                    // by (value & 0x3F0) >> 4 and the value's bit 3 is dropped.
                    entry_col = (offset_x >> 3).wrapping_add((hlookup & 0x03F0) >> 4);
                }
                if vlookup & valid_bit != 0 {
                    let v = vlookup & 0x03FF;
                    voffset_map = real_y_map.wrapping_add(v) & 0x03FF;
                    voffset_chr = real_y_chr.wrapping_add(v) & 0x03FF;
                }
            }
        }
        (entry_col, hoffset, voffset_map, voffset_chr)
    }

    /// The layer's vertical scroll with the mosaic block-hold subtraction applied
    /// (fullsnes: "subtract the vertical index from the vertical scroll register").
    fn bg_vscroll(&self, bg: usize) -> u16 {
        if self.mosaic_bg_enabled(bg) {
            self.bg_vofs[bg].wrapping_sub(self.mosaic_vcount as u16)
        } else {
            self.bg_vofs[bg]
        }
    }

    /// Compute the effective BG pixel coordinates `(hoffset, voffset)` for layer `bg` at screen
    /// `(x, y)`, applying offset-per-tile (modes 2/4/6) where BG3 supplies per-column H/V offsets
    /// to BG1/BG2. Algorithm follows bsnes (non-hires; Mode 5/6 hi-res output is #2766).
    fn effective_offsets(&self, bg: usize, x: u16, y: u16) -> (u16, u16) {
        let hscroll = self.bg_hofs[bg] & 0x03FF;
        let vscroll = self.bg_vscroll(bg) & 0x03FF;
        // Framebuffer row `y` shows display line `y + 1` (line 0 is never rendered), and the BG
        // fetch adds BGnVOFS to the raw display line (ares fetchNameTable: vcounter() + vscroll;
        // Mesen2: realY = _scanline). Same convention as Mode 7's screen_y (mode7.rs).
        let screen_y = y.wrapping_add(1);
        let mut hoffset = x.wrapping_add(hscroll);
        let mut voffset = screen_y.wrapping_add(vscroll);

        if matches!(self.bg_mode, 2 | 4 | 6) && bg < 2 {
            let tile_width = if self.bg_tile_size_16[bg] { 4 } else { 3 };
            let valid_bit = 0x2000u16 << bg; // BG1 -> bit13, BG2 -> bit14
            let offset_x = x.wrapping_add(hscroll & 7);
            // Mosaic adjusts the line BEFORE the OPT vScroll replacement applies
            // (Mesen2 GetTilemapData: realY loses the mosaic offset first), so
            // OPT-shifted rows are held to the block start too.
            let mosaic_y = if self.mosaic_bg_enabled(bg) {
                screen_y.wrapping_sub(self.mosaic_vcount as u16)
            } else {
                screen_y
            };
            // The first tile column is exempt from offset-per-tile.
            if offset_x >= (1u16 << tile_width) {
                let lookup_x =
                    (offset_x - (1u16 << tile_width)).wrapping_add(self.bg_hofs[2] & 0x03F8);
                let bg3_vscroll = self.bg_vofs[2] & 0x03FF;
                let hlookup = self.bg3_offset_entry(lookup_x, bg3_vscroll);
                if self.bg_mode == 4 {
                    if hlookup & valid_bit != 0 {
                        if hlookup & 0x8000 == 0 {
                            // H offset: 10-bit field with the low 3 bits ignored (bits 3-9).
                            hoffset = offset_x.wrapping_add(hlookup & 0x03F8);
                        } else {
                            // V offset: full 10-bit field.
                            voffset = mosaic_y.wrapping_add(hlookup & 0x03FF);
                        }
                    }
                } else {
                    let vlookup = self.bg3_offset_entry(lookup_x, bg3_vscroll.wrapping_add(8));
                    if hlookup & valid_bit != 0 {
                        hoffset = offset_x.wrapping_add(hlookup & 0x03F8);
                    }
                    if vlookup & valid_bit != 0 {
                        voffset = mosaic_y.wrapping_add(vlookup & 0x03FF);
                    }
                }
            }
        }
        (hoffset, voffset)
    }

    /// Fetch a BG3 offset-per-tile map entry at BG-pixel coordinates `(h, v)`.
    fn bg3_offset_entry(&self, h: u16, v: u16) -> u16 {
        let tile_shift = if self.bg_tile_size_16[2] { 4 } else { 3 };
        self.read_bg_map_entry(2, h >> tile_shift, v >> tile_shift)
    }

    /// Read a 16-bit BG map entry, resolving the tilemap size (BGnSC bits 0-1) and the SC0..SC3
    /// sub-screen layout. `entry_col`/`entry_row` are tile indices into the full (up to 64x64) map.
    fn read_bg_map_entry(&self, bg: usize, entry_col: u16, entry_row: u16) -> u16 {
        let size = self.bg_screen_size[bg];
        let (map_w, map_h): (u16, u16) = match size {
            0 => (32, 32),
            1 => (64, 32),
            2 => (32, 64),
            _ => (64, 64),
        };
        let col = entry_col & (map_w - 1);
        let row = entry_row & (map_h - 1);
        let sc_x = (col >> 5) & 1;
        let sc_y = (row >> 5) & 1;
        let sc_index = match size {
            0 => 0,
            1 => sc_x,
            2 => sc_y,
            _ => sc_y * 2 + sc_x,
        };

        let local = (row & 31) * 32 + (col & 31);
        let word_addr = self.bg_tilemap_base[bg]
            .wrapping_add(sc_index * 0x400)
            .wrapping_add(local);
        let byte = (word_addr as usize) << 1;
        let lo = self.vram[byte & (VRAM_SIZE - 1)] as u16;
        let hi = self.vram[(byte + 1) & (VRAM_SIZE - 1)] as u16;
        lo | (hi << 8)
    }

    /// Decode a single pixel's color index (0..2^bpp) from a tile's bit-planes (2/4/8 bpp).
    pub(super) fn decode_tile_pixel(
        &self,
        char_base: u16,
        char_num: u16,
        bpp: u8,
        fine_x: u8,
        fine_y: u8,
    ) -> u8 {
        // Words per tile: 8 (2bpp), 16 (4bpp), 32 (8bpp).
        let words_per_tile = (bpp as u16) * 4;
        let tile_word = char_base.wrapping_add(char_num.wrapping_mul(words_per_tile));
        let row_base = ((tile_word as usize) << 1).wrapping_add((fine_y as usize) * 2);
        let bit = 7 - fine_x;

        // Each plane pair occupies 16 bytes; the second plane of a pair is at +1.
        let mut color = 0u8;
        for plane in 0..bpp {
            let pair = (plane / 2) as usize;
            let within = (plane % 2) as usize;
            let byte = self.vram[(row_base + pair * 16 + within) & (VRAM_SIZE - 1)];
            color |= ((byte >> bit) & 1) << plane;
        }
        color
    }

    /// Read a CGRAM color (BGR555) by palette index.
    ///
    /// Also records the index as the renderer's current palette-fetch address, which is
    /// where CPU/DMA CGRAM writes land when they happen during active rendering (see the
    /// $2122 handler in `registers.rs`).
    pub(super) fn cgram_color(&self, index: u8) -> u16 {
        self.cgram_render_index.set(index);
        let byte = (index as usize) << 1;
        (self.cgram[byte & (CGRAM_SIZE - 1)] as u16
            | ((self.cgram[(byte + 1) & (CGRAM_SIZE - 1)] as u16) << 8))
            & 0x7FFF
    }
}

/// Split a 16-bit BG tilemap entry into `(char_num, palette, priority, hflip, vflip)`.
fn decode_bg_map_entry(entry: u16) -> (u16, u8, bool, bool, bool) {
    (
        entry & 0x03FF,
        ((entry >> 10) & 0x07) as u8,
        entry & 0x2000 != 0,
        entry & 0x4000 != 0,
        entry & 0x8000 != 0,
    )
}

/// Direct-color conversion for 256-color BGs: combine the 8-bit color index (`BBGGGRRR`) with the
/// 3-bit BG-map palette (`bgr`) into a 15-bit BGR555 value (per fullsnes Direct Color).
fn direct_color(color: u8, palette: u8) -> u16 {
    let red = ((color as u16 & 0x07) << 2) | ((palette as u16 & 0x01) << 1);
    let green = (((color as u16 >> 3) & 0x07) << 2) | (((palette as u16 >> 1) & 0x01) << 1);
    let blue = (((color as u16 >> 6) & 0x03) << 3) | (((palette as u16 >> 2) & 0x01) << 2);
    (blue << 10) | (green << 5) | red
}

#[cfg(test)]
mod tests {
    use super::super::{
        DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME, Ppu, SCREEN_WIDTH_MAX,
    };

    fn render_frame(ppu: &mut Ppu) {
        let ticks =
            DOTS_PER_SCANLINE as u32 * NTSC_SCANLINES_PER_FRAME as u32 * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks {
            ppu.tick();
        }
    }

    /// Render up to and including the first visible scanline, leaving the per-line
    /// buffers holding that line's resolve results.
    fn render_first_visible_line(ppu: &mut Ppu) {
        render_lines(ppu, 1);
    }

    fn set_cgram(ppu: &mut Ppu, index: usize, bgr555: u16) {
        ppu.cgram[index * 2] = (bgr555 & 0xFF) as u8;
        ppu.cgram[index * 2 + 1] = (bgr555 >> 8) as u8;
    }

    fn set_vram_word(ppu: &mut Ppu, word_addr: usize, value: u16) {
        ppu.vram[word_addr * 2] = (value & 0xFF) as u8;
        ppu.vram[word_addr * 2 + 1] = (value >> 8) as u8;
    }

    /// Fill an 8x8 tile (at `char_num`, `char_base` words) with a single 2bpp color (1-3).
    fn fill_2bpp_tile(ppu: &mut Ppu, char_base: usize, char_num: usize, color: u8) {
        let base = (char_base + char_num * 8) * 2; // byte address
        for r in 0..8 {
            ppu.vram[base + r * 2] = if color & 1 != 0 { 0xFF } else { 0x00 };
            ppu.vram[base + r * 2 + 1] = if color & 2 != 0 { 0xFF } else { 0x00 };
        }
    }

    /// Fill an 8x8 tile with a single 4bpp color (1-15).
    fn fill_4bpp_tile(ppu: &mut Ppu, char_base: usize, char_num: usize, color: u8) {
        let base = (char_base + char_num * 16) * 2;
        for r in 0..8 {
            ppu.vram[base + r * 2] = if color & 1 != 0 { 0xFF } else { 0x00 };
            ppu.vram[base + r * 2 + 1] = if color & 2 != 0 { 0xFF } else { 0x00 };
            ppu.vram[base + 16 + r * 2] = if color & 4 != 0 { 0xFF } else { 0x00 };
            ppu.vram[base + 16 + r * 2 + 1] = if color & 8 != 0 { 0xFF } else { 0x00 };
        }
    }

    /// Fill an 8x8 tile with a single 8bpp color (0-255). 32 words/tile; plane pairs at +0/+16/+32/+48.
    fn fill_8bpp_tile(ppu: &mut Ppu, char_base: usize, char_num: usize, color: u8) {
        let base = (char_base + char_num * 32) * 2;
        for r in 0..8 {
            for p in 0..8usize {
                let off = base + (p / 2) * 16 + r * 2 + (p % 2);
                ppu.vram[off] = if color & (1 << p) != 0 { 0xFF } else { 0x00 };
            }
        }
    }

    /// Set a single pixel in a 2bpp 8×8 tile at (fine_x, fine_y) to `color` (0-3).
    fn set_2bpp_tile_pixel(
        ppu: &mut Ppu,
        char_base: usize,
        char_num: usize,
        fine_x: u8,
        fine_y: u8,
        color: u8,
    ) {
        let base = (char_base + char_num * 8) * 2;
        let row_offset = base + (fine_y as usize) * 2;
        let bit = 7 - fine_x; // bit 7 is left-most
        if color & 1 != 0 {
            ppu.vram[row_offset] |= 1 << bit;
        } else {
            ppu.vram[row_offset] &= !(1 << bit);
        }
        if color & 2 != 0 {
            ppu.vram[row_offset + 1] |= 1 << bit;
        } else {
            ppu.vram[row_offset + 1] &= !(1 << bit);
        }
    }

    /// Set a single pixel in a 4bpp 8×8 tile at (fine_x, fine_y) to `color` (0-15).
    fn set_4bpp_tile_pixel(
        ppu: &mut Ppu,
        char_base: usize,
        char_num: usize,
        fine_x: u8,
        fine_y: u8,
        color: u8,
    ) {
        let base = (char_base + char_num * 16) * 2;
        let bit = 7 - fine_x; // bit 7 is left-most
        for plane in 0..4usize {
            // Planes 0/1 interleave in the first 16 bytes, planes 2/3 in the next 16.
            let offset = base + (plane / 2) * 16 + (fine_y as usize) * 2 + (plane % 2);
            if color & (1 << plane) != 0 {
                ppu.vram[offset] |= 1 << bit;
            } else {
                ppu.vram[offset] &= !(1 << bit);
            }
        }
    }

    fn pixel(rgb: &[u8], x: usize, y: usize) -> [u8; 3] {
        let i = (y * 256 + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    /// Render `n` scanlines from power-on (framebuffer rows 0..n-1 are completed,
    /// since display line y+1 renders framebuffer row y).
    fn render_lines(ppu: &mut Ppu, n: u32) {
        let ticks = DOTS_PER_SCANLINE as u32 * (n + 1) * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks {
            ppu.tick();
        }
    }

    /// Mode 5 with BG1 on both the main and sub screens (sub display enabled via
    /// CGWSEL bit 1), identity palette (CGRAM word i = i), full brightness.
    /// True-hires tests then read raw BGR555 words from framebuffer row `y`.
    fn setup_mode5_bg1_both_screens(ppu: &mut Ppu) {
        for i in 1..16 {
            set_cgram(ppu, i, i as u16);
        }
        ppu.write_register(0x2105, 0x05); // mode 5 (BG1 4bpp)
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x01); // TS: BG1
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2100, 0x0F); // full brightness
    }

    /// Write a per-column 4bpp pattern (column c = color c+1) on all 8 tile rows.
    fn fill_4bpp_tile_column_pattern(ppu: &mut Ppu, char_base: usize, char_num: usize) {
        for fine_y in 0..8 {
            for fine_x in 0..8u8 {
                set_4bpp_tile_pixel(ppu, char_base, char_num, fine_x, fine_y, fine_x + 1);
            }
        }
    }

    /// Write a per-row 4bpp pattern (row r = color r+1) on all 8 tile columns.
    fn fill_4bpp_tile_row_pattern(ppu: &mut Ppu, char_base: usize, char_num: usize) {
        for fine_y in 0..8u8 {
            for fine_x in 0..8u8 {
                set_4bpp_tile_pixel(ppu, char_base, char_num, fine_x, fine_y, fine_y + 1);
            }
        }
    }

    #[test]
    fn mode5_fetches_distinct_even_and_odd_subpixels() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_vram_word(&mut ppu, 0x400, 1); // BG1 entry -> char 1
        set_bg_map_base(&mut ppu, 0, 0x400);
        fill_4bpp_tile_column_pattern(&mut ppu, 0, 1);
        render_lines(&mut ppu, 1);

        // Modes 5/6 fetch BG1 at doubled horizontal resolution: output column X shows
        // tile hi-res column X (even from the sub fetch, odd from the main fetch),
        // NOT the same native pixel doubled.
        for c in 0..8usize {
            assert_eq!(
                ppu.framebuffer[c],
                (c + 1) as u16,
                "output column {c} shows tile column {c}"
            );
        }
    }

    #[test]
    fn mode5_pairs_char_n_and_n_plus_1_into_a_16_wide_tile() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1); // entry 0 -> char 1 (pairs with char 2)
        set_vram_word(&mut ppu, 0x401, 3); // entry 1 -> char 3 (pairs with char 4)
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_4bpp_tile(&mut ppu, 0, 2, 2);
        fill_4bpp_tile(&mut ppu, 0, 3, 3);
        render_lines(&mut ppu, 1);

        // A map entry spans 16 output columns: char N in the left half, char N+1 in
        // the right half; the next map entry starts at output column 16.
        assert_eq!(ppu.framebuffer[0], 1, "columns 0-7 show char 1");
        assert_eq!(ppu.framebuffer[7], 1, "columns 0-7 show char 1");
        assert_eq!(ppu.framebuffer[8], 2, "columns 8-15 show char 2");
        assert_eq!(ppu.framebuffer[15], 2, "columns 8-15 show char 2");
        assert_eq!(ppu.framebuffer[16], 3, "next map entry starts at column 16");
    }

    #[test]
    fn mode5_hflip_mirrors_the_char_pair() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1 | 0x4000); // entry -> char 1, H-flip
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_4bpp_tile(&mut ppu, 0, 2, 2);
        render_lines(&mut ppu, 1);

        // H-flip mirrors the whole 16-wide pair: char 2 (mirrored) lands in the left
        // half, char 1 (mirrored) in the right half.
        assert_eq!(ppu.framebuffer[0], 2, "flipped pair leads with char 2");
        assert_eq!(ppu.framebuffer[7], 2, "columns 0-7 show mirrored char 2");
        assert_eq!(ppu.framebuffer[8], 1, "columns 8-15 show mirrored char 1");
    }

    #[test]
    fn mode5_hscroll_applies_in_half_pixel_units() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1);
        fill_4bpp_tile_column_pattern(&mut ppu, 0, 1);
        // BG1HOFS = 1 (write-twice: low byte then high byte).
        ppu.write_register(0x210D, 0x01);
        ppu.write_register(0x210D, 0x00);
        render_lines(&mut ppu, 1);

        // A native scroll of 1 shifts the hi-res fetch by 2 half-pixels: output
        // column 0 shows tile hi-res column 2 (pattern color 3).
        assert_eq!(ppu.framebuffer[0], 3, "HOFS=1 shifts by two hi-res columns");
    }

    #[test]
    fn mode5_tile_size_16_pairs_vertically_but_not_horizontally() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        ppu.write_register(0x2105, 0x15); // mode 5 + BG1 16x16 tiles
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1); // entry 0 -> chars 1/2 (top), 17/18 (bottom)
        set_vram_word(&mut ppu, 0x401, 5); // entry 1 -> chars 5/6 (top), 21/22 (bottom)
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_4bpp_tile(&mut ppu, 0, 17, 4); // vertical pair of char 1
        fill_4bpp_tile(&mut ppu, 0, 5, 5);
        fill_4bpp_tile(&mut ppu, 0, 21, 6); // vertical pair of char 5
        render_lines(&mut ppu, 8);

        // 16x16 tile size still spans only 16 output columns horizontally (the
        // width-16 fetch is forced in modes 5/6), but pairs vertically: rows 8-15
        // use char N+16.
        assert_eq!(ppu.framebuffer[0], 1, "top half shows char 1");
        assert_eq!(
            ppu.framebuffer[7 * SCREEN_WIDTH_MAX],
            4,
            "bottom half (display line 8) shows char 17"
        );
        assert_eq!(
            ppu.framebuffer[7 * SCREEN_WIDTH_MAX + 16],
            6,
            "next map entry still starts at column 16"
        );
    }

    /// Mode 5 scene with a row-pattern char for field-content tests: BG1 entry 0 ->
    /// char 1 whose tile row r is solid color r+1, screen interlace enabled.
    fn setup_mode5_interlace_row_pattern(ppu: &mut Ppu) {
        setup_mode5_bg1_both_screens(ppu);
        set_bg_map_base(ppu, 0, 0x400);
        set_vram_word(ppu, 0x400, 1);
        fill_4bpp_tile_row_pattern(ppu, 0, 1);
        ppu.write_register(0x2133, 0x01); // SETINI: screen interlace
    }

    #[test]
    fn mode5_interlace_even_field_fetches_doubled_tile_row() {
        let mut ppu = Ppu::new();
        setup_mode5_interlace_row_pattern(&mut ppu);
        ppu.interlace_field = false;
        render_lines(&mut ppu, 1);

        // Mesen2 GetTilemapData/GetChrData: realY = (scanline << 1) | field in modes
        // 5/6 + interlace; display line 1, even field -> tile row 2 -> color 3.
        // The even field writes framebuffer row 0 (y*2 + 0).
        assert_eq!(
            ppu.framebuffer[0], 3,
            "even field fetches tile row 2 for display line 1"
        );
    }

    #[test]
    fn mode5_interlace_odd_field_fetches_the_next_tile_row() {
        let mut ppu = Ppu::new();
        setup_mode5_interlace_row_pattern(&mut ppu);
        ppu.interlace_field = true;
        render_lines(&mut ppu, 1);

        // Odd field: realY = 3 -> tile row 3 -> color 4, written to framebuffer
        // row 1 (y*2 + 1).
        assert_eq!(
            ppu.framebuffer[SCREEN_WIDTH_MAX], 4,
            "odd field fetches tile row 3 for display line 1"
        );
    }

    #[test]
    fn mode5_interlace_tile_size_16_pairs_vertically_at_doubled_rate() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        ppu.write_register(0x2105, 0x15); // mode 5 + BG1 16x16 tiles
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1); // entry 0 -> chars 1/2 (top), 17/18 (bottom)
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_4bpp_tile(&mut ppu, 0, 17, 4); // vertical pair of char 1
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = false;
        render_lines(&mut ppu, 4);

        // The doubled realY crosses the 8-line half-tile boundary twice as fast:
        // display line 3 (realY 6) still shows the top char, display line 4
        // (realY 8) already shows the vertical pair char 17.
        assert_eq!(
            ppu.framebuffer[4 * SCREEN_WIDTH_MAX],
            1,
            "display line 3 (realY 6) shows the top half"
        );
        assert_eq!(
            ppu.framebuffer[6 * SCREEN_WIDTH_MAX],
            4,
            "display line 4 (realY 8) shows the bottom half"
        );
    }

    #[test]
    fn mode6_interlace_opt_v_replacement_uses_doubled_line() {
        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        set_vram_word(&mut ppu, 0x421, 7); // BG1 map row 1, col 1 -> char 7
        fill_4bpp_tile(&mut ppu, 0, 7, 7);
        set_vram_word(&mut ppu, 0x820, 6 | 0x2000); // OPT V entry, value 6
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = false;
        render_lines(&mut ppu, 1);

        // The OPT V replacement adds the entry to the DOUBLED line: realY 2 + 6 = 8
        // -> map row 1 -> char 7 (undoubled 1 + 6 = 7 would stay on row 0).
        assert_eq!(
            ppu.framebuffer[16], 7,
            "OPT V offset applies to the doubled display line"
        );
    }

    #[test]
    fn mode5_interlace_vflip_mirrors_the_doubled_row() {
        let mut ppu = Ppu::new();
        setup_mode5_interlace_row_pattern(&mut ppu);
        set_vram_word(&mut ppu, 0x400, 1 | 0x8000); // add V-flip
        ppu.interlace_field = false;
        render_lines(&mut ppu, 1);
        assert_eq!(
            ppu.framebuffer[0], 6,
            "even field: realY 2 flips to tile row 5"
        );

        let mut ppu = Ppu::new();
        setup_mode5_interlace_row_pattern(&mut ppu);
        set_vram_word(&mut ppu, 0x400, 1 | 0x8000);
        ppu.interlace_field = true;
        render_lines(&mut ppu, 1);
        assert_eq!(
            ppu.framebuffer[SCREEN_WIDTH_MAX], 5,
            "odd field: realY 3 flips to tile row 4"
        );
    }

    #[test]
    fn mode5_interlace_leaves_horizontal_pairing_and_hflip_unchanged() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1 | 0x4000); // char 1, H-flip
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_4bpp_tile(&mut ppu, 0, 2, 2);
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = false;
        render_lines(&mut ppu, 1);

        // Interlace only doubles the vertical fetch; the 16-wide char pairing and
        // hflip mirroring across the pair are unchanged.
        assert_eq!(
            ppu.framebuffer[0], 2,
            "flipped pair still leads with char 2"
        );
        assert_eq!(
            ppu.framebuffer[8], 1,
            "columns 8-15 still show mirrored char 1"
        );
    }

    #[test]
    fn interlace_outside_modes_5_6_keeps_identical_field_content() {
        // Mesen2's IsDoubleHeight requires mode 5/6: in other modes + interlace both
        // fields fetch the SAME source row and only the output row parity differs.
        let render_mode1_field = |field: bool| {
            let mut ppu = Ppu::new();
            for i in 1..16 {
                set_cgram(&mut ppu, i, i as u16);
            }
            ppu.write_register(0x2105, 0x01); // mode 1 (BG1 4bpp)
            ppu.write_register(0x212C, 0x01); // TM: BG1
            ppu.write_register(0x2100, 0x0F);
            ppu.write_register(0x2133, 0x01);
            set_bg_map_base(&mut ppu, 0, 0x400);
            set_vram_word(&mut ppu, 0x400, 1);
            fill_4bpp_tile_row_pattern(&mut ppu, 0, 1);
            ppu.interlace_field = field;
            render_lines(&mut ppu, 1);
            ppu
        };

        let even = render_mode1_field(false);
        let odd = render_mode1_field(true);
        assert_eq!(even.framebuffer[0], 2, "even field: undoubled tile row 1");
        assert_eq!(
            odd.framebuffer[SCREEN_WIDTH_MAX], 2,
            "odd field: same source row on the odd output row"
        );
    }

    #[test]
    fn mode5_interlace_mosaic_chr_row_is_field_independent() {
        // Mesen2 GetChrData subtracts the mosaic offset twice PLUS the field bit
        // under IsDoubleHeight, so the whole block samples one source row in both
        // fields: chr row = 2 * (line - vcount).
        for field in [false, true] {
            let mut ppu = Ppu::new();
            setup_mode5_interlace_row_pattern(&mut ppu);
            ppu.write_register(0x2106, 0x11); // MOSAIC: size 1 (2-line blocks), BG1
            ppu.interlace_field = field;
            render_lines(&mut ppu, 4);

            let f = field as usize;
            for (y, color) in [(0usize, 3u16), (1, 3), (2, 7), (3, 7)] {
                assert_eq!(
                    ppu.framebuffer[(y * 2 + f) * SCREEN_WIDTH_MAX],
                    color,
                    "field {field}: display line {} shows block chr row color {color}",
                    y + 1
                );
            }
        }
    }

    #[test]
    fn mode5_interlace_mosaic_map_row_keeps_the_field_bit() {
        // Mesen2 GetTilemapData subtracts the mosaic offset twice with NO field
        // term: the map row is 2*block_start + field + vscroll, so the two fields
        // can select different tilemap rows within one mosaic block.
        for (field, expected) in [(false, 1u16), (true, 3)] {
            let mut ppu = Ppu::new();
            setup_mode5_bg1_both_screens(&mut ppu);
            set_bg_map_base(&mut ppu, 0, 0x400);
            set_vram_word(&mut ppu, 0x400, 1); // map row 0 -> char 1
            set_vram_word(&mut ppu, 0x420, 3); // map row 1 -> char 3
            fill_4bpp_tile(&mut ppu, 0, 1, 1);
            fill_4bpp_tile(&mut ppu, 0, 3, 3);
            ppu.write_register(0x210E, 0x05); // BG1VOFS = 5
            ppu.write_register(0x210E, 0x00);
            ppu.write_register(0x2133, 0x01);
            ppu.write_register(0x2106, 0x11); // MOSAIC: size 1 (2-line blocks), BG1
            ppu.interlace_field = field;
            render_lines(&mut ppu, 2);

            // Block lines 1-2: map voffset = 2*1 + field + 5 = 7 + field ->
            // map row 0 (even field) / row 1 (odd field), held across the block.
            let f = field as usize;
            assert_eq!(
                ppu.framebuffer[f * SCREEN_WIDTH_MAX],
                expected,
                "field {field}: line 1 map row"
            );
            assert_eq!(
                ppu.framebuffer[(2 + f) * SCREEN_WIDTH_MAX],
                expected,
                "field {field}: line 2 holds the block's map row"
            );
        }
    }

    #[test]
    fn mode6_opt_v_replacement_applies_mosaic_to_real_y() {
        // Mesen2 adjusts realY for mosaic BEFORE the OPT vScroll replacement adds
        // its entry, so mosaic holds the OPT-shifted rows too (non-interlace).
        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        set_vram_word(&mut ppu, 0x421, 7); // BG1 map row 1, col 1 -> char 7
        set_vram_word(&mut ppu, 0x441, 9); // BG1 map row 2, col 1 -> char 9
        fill_4bpp_tile(&mut ppu, 0, 7, 7);
        fill_4bpp_tile(&mut ppu, 0, 9, 9);
        set_vram_word(&mut ppu, 0x820, 8 | 0x2000); // OPT V entry, value 8
        ppu.write_register(0x2106, 0x31); // MOSAIC: size 3 (4-line blocks), BG1
        render_lines(&mut ppu, 8);

        // Display line 8 has vcount 3: realY = 8 - 3 = 5, voffset = 5 + 8 = 13 ->
        // map row 1 -> char 7. Without the mosaic adjustment: 8 + 8 = 16 -> row 2.
        assert_eq!(
            ppu.framebuffer[7 * SCREEN_WIDTH_MAX + 16],
            7,
            "OPT V rows are mosaic-held to the block start"
        );
    }

    #[test]
    fn mode2_opt_v_replacement_applies_mosaic_to_screen_y() {
        // Native-path equivalent of the above (modes 2/4 OPT).
        let mut ppu = Ppu::new();
        for i in 1..16 {
            set_cgram(&mut ppu, i, i as u16);
        }
        ppu.write_register(0x2105, 0x02); // mode 2 (BG1 4bpp + OPT)
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2100, 0x0F);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_bg_map_base(&mut ppu, 2, 0x800);
        set_vram_word(&mut ppu, 0x421, 7); // BG1 map row 1, col 1 -> char 7
        set_vram_word(&mut ppu, 0x441, 9); // BG1 map row 2, col 1 -> char 9
        fill_4bpp_tile(&mut ppu, 0, 7, 7);
        fill_4bpp_tile(&mut ppu, 0, 9, 9);
        set_vram_word(&mut ppu, 0x820, 8 | 0x2000); // OPT V entry, value 8
        ppu.write_register(0x2106, 0x31); // MOSAIC: size 3 (4-line blocks), BG1
        render_lines(&mut ppu, 8);

        // Display line 8, vcount 3: voffset = (8 - 3) + 8 = 13 -> map row 1 ->
        // char 7 at native x 8 (the first OPT-affected column).
        assert_eq!(
            ppu.framebuffer[7 * SCREEN_WIDTH_MAX + 8],
            7,
            "native OPT V rows are mosaic-held to the block start"
        );
    }

    #[test]
    fn mode5_bg2_uses_2bpp_doubled_fetch() {
        let mut ppu = Ppu::new();
        for i in 1..4 {
            set_cgram(&mut ppu, i, i as u16);
        }
        ppu.write_register(0x2105, 0x05); // mode 5 (BG2 2bpp)
        ppu.write_register(0x212C, 0x02); // TM: BG2
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2100, 0x0F);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x400, 1); // BG2 entry -> char 1
        for fine_y in 0..8 {
            for fine_x in 0..8u8 {
                // Column pattern 1,2,3,1,2,3,...
                set_2bpp_tile_pixel(&mut ppu, 0, 1, fine_x, fine_y, (fine_x % 3) + 1);
            }
        }
        render_lines(&mut ppu, 1);

        assert_eq!(ppu.framebuffer[0], 1, "column 0 shows tile column 0");
        assert_eq!(ppu.framebuffer[1], 2, "column 1 shows tile column 1");
        assert_eq!(ppu.framebuffer[2], 3, "column 2 shows tile column 2");
    }

    /// Mode 5 color-math scene: BG1 (main) solid red 10, BG2 (sub, palette 1) solid
    /// red 5, sub display enabled. Raw BGR555 asserts keep the math chains visible.
    fn setup_mode5_color_math(ppu: &mut Ppu) {
        set_cgram(ppu, 1, 0x000A); // BG1 color 1 = red 10
        set_cgram(ppu, 5, 0x0005); // BG2 palette 1 color 1 = red 5
        set_bg_map_base(ppu, 0, 0x000);
        set_bg_map_base(ppu, 1, 0x400);
        set_vram_word(ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(ppu, 0x400, 2 | (1 << 10)); // BG2 entry -> char 2, palette 1
        fill_4bpp_tile(ppu, 0, 1, 1);
        fill_2bpp_tile(ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x05); // mode 5
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // CGWSEL: add subscreen
        ppu.write_register(0x2100, 0x0F);
    }

    #[test]
    fn hires_color_math_applies_to_odd_main_pixels_using_pre_math_sub() {
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        ppu.write_register(0x2131, 0x01); // CGADSUB: add, BG1 enabled
        render_lines(&mut ppu, 1);

        // Odd (main) columns get main + pre-math sub = 10 + 5 = 15. At native x 1
        // the post-math even value is 20, so a post-math operand would yield 30.
        assert_eq!(ppu.framebuffer[1], 15, "main pixel adds the raw sub color");
        assert_eq!(
            ppu.framebuffer[3], 15,
            "later main pixels still use the pre-math sub color"
        );
    }

    #[test]
    fn hires_color_math_applies_to_even_sub_pixels_using_previous_post_math_main() {
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        ppu.write_register(0x2131, 0x01); // CGADSUB: add, BG1 enabled
        render_lines(&mut ppu, 1);

        // Even (sub) columns add the finalized main pixel one dot to the left:
        // sub 5 + main_final(10 + 5 = 15) = 20. The pre-math main (10) would give 15.
        assert_eq!(
            ppu.framebuffer[2], 20,
            "sub pixel adds the post-math main pixel of the previous dot"
        );
    }

    #[test]
    fn hires_color_math_even_column_at_x0_uses_black_prev_main() {
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        ppu.write_register(0x2131, 0x41); // CGADSUB: add-half, BG1 enabled
        render_lines(&mut ppu, 1);

        // Add-half against the (black) missing previous main pixel: (5 + 0) / 2 = 2.
        assert_eq!(
            ppu.framebuffer[0], 2,
            "the first even column adds black (no previous main pixel)"
        );
    }

    #[test]
    fn hires_color_math_even_gate_uses_previous_main_source() {
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        // Math enabled for BG2 only: the main pixel (BG1) is ungated, and the even
        // half-pixel's gate follows the previous MAIN pixel's source (BG1), not the
        // sub pixel's own layer (BG2) -- so nothing gets math.
        ppu.write_register(0x2131, 0x02);
        render_lines(&mut ppu, 1);

        assert_eq!(ppu.framebuffer[1], 10, "main pixel stays raw");
        assert_eq!(
            ppu.framebuffer[2], 5,
            "sub pixel stays raw: its gate is the main source at x-1"
        );
    }

    #[test]
    fn hires_even_half_falls_back_to_fixed_color_when_sub_screen_is_empty() {
        // Mesen2's hires loop passes `prevX` to the SAME helper that reads
        // `_subScreenPriority`, so the even half's operand -- a MAIN pixel at x-1 --
        // is gated on SUB coverage at x-1. With nothing on the sub screen the fixed
        // colour is substituted and halving is disabled, even though the intended
        // operand was a main pixel (#3035).
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        ppu.write_register(0x212D, 0x00); // TS: nothing -- sub is backdrop everywhere
        ppu.write_register(0x2131, 0x41); // CGADSUB: add + half, BG1 enabled
        ppu.write_register(0x2132, 0x80 | 12); // COLDATA: blue 12
        render_lines(&mut ppu, 1);

        // The main half already had this rule (#3012): 10 + COLDATA at full strength.
        assert_eq!(
            ppu.framebuffer[1],
            (12 << 10) | 10,
            "odd/main half takes the fallback operand unhalved"
        );
        // Even half at x = 1. Three outcomes are distinguishable here:
        //   prev_main halved    = (12298) / 2 per channel = (6 << 10) | 5
        //   prev_main unhalved  = 12298
        //   COLDATA unhalved    = 12 << 10          <- Mesen2
        assert_eq!(
            ppu.framebuffer[2],
            12 << 10,
            "even/sub half falls back to COLDATA at full strength, not the halved prev main"
        );
    }

    #[test]
    fn hires_even_half_reads_sub_coverage_at_the_previous_dot() {
        // The load-bearing index test. BG2 (sub) covers native x 0..3 only, while BG1
        // (main) additionally covers x 8..11 via a second tilemap entry. The even
        // half's coverage lookup must use x-1, not x:
        //   x = 4: sub covered at x-1 = 3  -> operand stays the previous main pixel
        //   x = 9: sub empty   at x-1 = 8  -> operand falls back to COLDATA
        // Reading coverage at x instead would make x = 4 fall back too, so the first
        // assertion pins the index and the second pins the rule.
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        set_vram_word(&mut ppu, 0x001, 1); // BG1 tilemap entry 1 -> covers x 8..11
        ppu.write_register(0x2131, 0x01); // CGADSUB: add (no half), BG1 enabled
        ppu.write_register(0x2132, 0x80 | 12); // COLDATA: blue 12
        render_lines(&mut ppu, 1);

        assert_eq!(
            ppu.framebuffer[8], 15,
            "x=4: sub is covered at x-1=3, so the operand is the previous main pixel"
        );
        assert_eq!(
            ppu.framebuffer[18],
            12 << 10,
            "x=9: sub is empty at x-1=8, so the operand falls back to COLDATA"
        );
    }

    #[test]
    fn hires_color_math_without_sub_enable_uses_fixed_color_operand() {
        let mut ppu = Ppu::new();
        setup_mode5_color_math(&mut ppu);
        ppu.write_register(0x2130, 0x00); // CGWSEL: fixed-color operand
        ppu.write_register(0x2131, 0x01); // CGADSUB: add, BG1 enabled
        ppu.write_register(0x2132, 0x80 | 12); // COLDATA: blue 12
        render_lines(&mut ppu, 1);

        assert_eq!(
            ppu.framebuffer[1],
            (12 << 10) | 10,
            "main pixel adds COLDATA when CGWSEL bit 1 is clear"
        );
        // Mesen2's fixed-color branch applies to BOTH halves (ApplyColorMathToPixel:
        // otherPixel = FixedColor when ColorMathAddSubscreen is clear).
        assert_eq!(
            ppu.framebuffer[2],
            (12 << 10) | 5,
            "sub pixel adds COLDATA when CGWSEL bit 1 is clear"
        );
    }

    #[test]
    fn pseudo_hires_applies_color_math_to_both_half_pixels() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 1, 0x000A); // BG1 color 1 = red 10
        set_cgram(&mut ppu, 33, 0x0005); // BG2 color 1 = red 5 (mode 0 region)
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1);
        set_vram_word(&mut ppu, 0x400, 2);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // CGWSEL: add subscreen
        ppu.write_register(0x2131, 0x01); // CGADSUB: add, BG1 enabled
        ppu.write_register(0x2133, 0x08); // pseudo-hires
        ppu.write_register(0x2100, 0x0F);
        render_lines(&mut ppu, 1);

        // Pseudo-hires shares the hires math path: odd = 10 + 5 = 15, and the even
        // column at native x 1 adds the finalized main to its left: 5 + 15 = 20.
        assert_eq!(ppu.framebuffer[1], 15, "main half-pixel gets color math");
        assert_eq!(ppu.framebuffer[2], 20, "sub half-pixel gets color math");
    }

    #[test]
    fn hires_even_backdrop_shows_cgram_zero_not_coldata() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0842); // backdrop = dark gray (2,2,2)
        set_cgram(&mut ppu, 1, 0x7FFF); // BG1 color 1 = white
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_vram_word(&mut ppu, 0x000, 1);
        fill_4bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x05); // mode 5
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2 (empty -> sub backdrop)
        ppu.write_register(0x2132, 0x80 | 12); // COLDATA: blue 12
        ppu.write_register(0x2100, 0x0F);
        render_lines(&mut ppu, 1);

        // Mesen2 RenderBgColor fills the sub-screen backdrop with CGRAM color 0
        // (like main); COLDATA is only ever a color-math operand, so an empty even
        // column displays the backdrop color, not the fixed color and not black.
        assert_eq!(
            ppu.framebuffer[0], 0x0842,
            "empty even column shows the CGRAM 0 backdrop"
        );
        assert_eq!(ppu.framebuffer[1], 0x7FFF, "odd column shows main");
    }

    #[test]
    fn true_hires_displays_sub_layers_without_cgwsel_sub_enable() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // main BG1 color 1 = white
        set_cgram(&mut ppu, 5, 0x001F); // sub BG2 palette 1 color 1 = red
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2 | (1 << 10)); // BG2 entry -> char 2, palette 1
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x05); // mode 5
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        // CGWSEL stays 0: bit 1 selects the color-math operand, it does NOT gate
        // the sub screen's hires display (Mesen2 renders TS unconditionally).
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            &rgb[0..3],
            &[255, 0, 0],
            "even column shows the TS layer without CGWSEL bit 1"
        );
        assert_eq!(&rgb[3..6], &[255, 255, 255], "odd column shows main");
    }

    #[test]
    fn pseudo_hires_displays_sub_layers_without_cgwsel_sub_enable() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // main BG1 color 1 = white
        set_cgram(&mut ppu, 33, 0x001F); // sub BG2 color 1 = red (mode 0 region)
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2); // BG2 entry -> char 2
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2133, 0x08); // pseudo-hires
        // CGWSEL stays 0, as above.
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            &rgb[0..3],
            &[255, 0, 0],
            "first half-pixel shows the TS layer without CGWSEL bit 1"
        );
        assert_eq!(&rgb[3..6], &[255, 255, 255], "second half-pixel shows main");
    }

    #[test]
    fn mode5_mosaic_size1_collapses_pairs_to_the_even_subpixel() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1);
        fill_4bpp_tile_column_pattern(&mut ppu, 0, 1);
        ppu.write_register(0x2106, 0x01); // MOSAIC: size 1, BG1 enabled
        render_lines(&mut ppu, 1);

        // Mosaic on a hires layer collapses each half-pixel pair to the even sample,
        // even at block size 1 (Mesen2 forces the mosaic path on for modes 5/6).
        assert_eq!(ppu.framebuffer[0], 1, "even column keeps the even sample");
        assert_eq!(ppu.framebuffer[1], 1, "odd column repeats the even sample");
        assert_eq!(ppu.framebuffer[2], 3, "next pair samples hires column 2");
        assert_eq!(ppu.framebuffer[3], 3, "and repeats it");
    }

    #[test]
    fn mode5_mosaic_block_replicates_the_block_start_even_sample() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1);
        fill_4bpp_tile_column_pattern(&mut ppu, 0, 1);
        fill_4bpp_tile(&mut ppu, 0, 2, 9); // paired char: block 2's even sample
        ppu.write_register(0x2106, 0x31); // MOSAIC: size 4, BG1 enabled
        render_lines(&mut ppu, 1);

        // A 4-wide mosaic block spans 8 output columns, all showing the even sample
        // of the block-start native pixel (hires column 0 -> pattern color 1).
        for c in 0..8usize {
            assert_eq!(
                ppu.framebuffer[c], 1,
                "output column {c} shows the block-start even sample"
            );
        }
        // The next block starts at native x 4 -> even sample is hires column 8,
        // the first column of paired char 2.
        assert_eq!(ppu.framebuffer[8], 9, "next block starts at column 8");
    }

    #[test]
    fn mode5_mosaic_only_affects_enabled_layers() {
        let mut ppu = Ppu::new();
        for i in 1..4 {
            set_cgram(&mut ppu, i, i as u16);
        }
        ppu.write_register(0x2105, 0x05); // mode 5 (BG2 2bpp)
        ppu.write_register(0x212C, 0x02); // TM: BG2
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x2106, 0x01); // MOSAIC: BG1 only -- BG2 unaffected
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x400, 1);
        for fine_y in 0..8 {
            for fine_x in 0..8u8 {
                set_2bpp_tile_pixel(&mut ppu, 0, 1, fine_x, fine_y, (fine_x % 3) + 1);
            }
        }
        render_lines(&mut ppu, 1);

        assert_eq!(ppu.framebuffer[0], 1, "BG2 keeps distinct even subpixels");
        assert_eq!(ppu.framebuffer[1], 2, "BG2 keeps distinct odd subpixels");
        assert_eq!(ppu.framebuffer[2], 3, "no collapse on a non-mosaic layer");
    }

    /// Mode 6 scene for offset-per-tile tests: BG1 on both screens with solid char
    /// pairs per map entry (entry 0 -> chars 1/2, entry 1 -> 3/4, entry 2 -> 5/6,
    /// colored by char number), BG1 map at 0x400, BG3 (OPT source) map at 0x800.
    fn setup_mode6_opt(ppu: &mut Ppu) {
        setup_mode5_bg1_both_screens(ppu);
        ppu.write_register(0x2105, 0x06); // mode 6 (BG1 4bpp + BG3 offset-per-tile)
        set_bg_map_base(ppu, 0, 0x400);
        set_bg_map_base(ppu, 2, 0x800);
        for entry in 0..3u16 {
            set_vram_word(ppu, 0x400 + entry as usize, entry * 2 + 1);
            fill_4bpp_tile(ppu, 0, (entry * 2 + 1) as usize, (entry * 2 + 1) as u8);
            fill_4bpp_tile(ppu, 0, (entry * 2 + 2) as usize, (entry * 2 + 2) as u8);
        }
    }

    #[test]
    fn mode6_opt_h_offset_shifts_tilemap_in_hires_units() {
        // An OPT H value of 16 moves the BG1 tilemap by one entry (8 native px);
        // Mesen2 ORs the value into the doubled scroll and shifts back down, so
        // bit 3 (value 8) is dropped in modes 5/6 (halved-OPT quirk).
        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        set_vram_word(&mut ppu, 0x800, 16 | 0x2000); // H entry for BG1, value 16
        render_lines(&mut ppu, 1);
        assert_eq!(
            ppu.framebuffer[16], 5,
            "H offset 16 shifts native x 8 from entry 1 to entry 2"
        );

        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        set_vram_word(&mut ppu, 0x800, 8 | 0x2000); // H entry for BG1, value 8
        render_lines(&mut ppu, 1);
        assert_eq!(
            ppu.framebuffer[16], 3,
            "H offset 8 is dropped by the hires halving"
        );
    }

    #[test]
    fn mode6_opt_v_offset_replaces_vertical_scroll_natively() {
        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        set_vram_word(&mut ppu, 0x421, 7); // BG1 map row 1, col 1 -> char 7
        fill_4bpp_tile(&mut ppu, 0, 7, 7);
        // BG3 row 1 holds the V entries in mode 6 (mode-2-style dual lookup):
        // V offset 8 moves the OPT-affected columns one tile row down.
        set_vram_word(&mut ppu, 0x820, 8 | 0x2000);
        render_lines(&mut ppu, 1);

        assert_eq!(
            ppu.framebuffer[16], 7,
            "V offset 8 fetches the map row below for native x 8"
        );
        assert_eq!(
            ppu.framebuffer[0], 1,
            "exempt first tile column is unshifted"
        );
    }

    #[test]
    fn mode6_opt_first_tile_column_exempt_in_hires() {
        // The exemption boundary stays at 8 native pixels (one hires map entry)
        // even with the BGMODE 16x16 tile-size bit set.
        let mut ppu = Ppu::new();
        setup_mode6_opt(&mut ppu);
        ppu.write_register(0x2105, 0x16); // mode 6 + BG1 16x16 tiles
        set_vram_word(&mut ppu, 0x800, 16 | 0x2000);
        render_lines(&mut ppu, 1);

        assert_eq!(ppu.framebuffer[0], 1, "native x 0-7 are exempt from OPT");
        assert_eq!(
            ppu.framebuffer[16], 5,
            "native x 8 is past the exemption despite 16x16 tiles"
        );
    }

    #[test]
    fn mode6_bg1_uses_doubled_fetch() {
        let mut ppu = Ppu::new();
        setup_mode5_bg1_both_screens(&mut ppu);
        ppu.write_register(0x2105, 0x06); // mode 6 (BG1 4bpp)
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_vram_word(&mut ppu, 0x400, 1);
        fill_4bpp_tile_column_pattern(&mut ppu, 0, 1);
        render_lines(&mut ppu, 1);

        for c in 0..8usize {
            assert_eq!(
                ppu.framebuffer[c],
                (c + 1) as u16,
                "output column {c} shows tile column {c}"
            );
        }
    }

    #[test]
    fn renders_a_bg1_2bpp_tile_in_mode0() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000); // backdrop black
        set_cgram(&mut ppu, 1, 0x7FFF); // palette 0 color 1 = white
        set_vram_word(&mut ppu, 0, 1); // tilemap entry 0 -> char 1, palette 0
        fill_2bpp_tile(&mut ppu, 0, 1, 1); // tile 1 = solid color 1

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x2107, 0x00); // BG1SC base 0
        ppu.write_register(0x210B, 0x00); // BG1 char base 0
        ppu.write_register(0x212C, 0x01); // TM: BG1 enabled
        ppu.write_register(0x2100, 0x0F); // full brightness
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "BG1 tile pixel is white"
        );
    }

    #[test]
    fn transparent_bg1_pixel_falls_back_to_backdrop() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x001F); // backdrop red
        set_vram_word(&mut ppu, 0, 1); // entry -> char 1
        fill_2bpp_tile(&mut ppu, 0, 1, 0); // color 0 = transparent everywhere

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 0, 0],
            "transparent BG1 shows backdrop"
        );
    }

    #[test]
    fn disabled_bg1_in_tm_shows_backdrop() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x7C00); // backdrop blue
        set_cgram(&mut ppu, 1, 0x7FFF);
        set_vram_word(&mut ppu, 0, 1);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x00); // BG1 NOT enabled
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [0, 0, 255],
            "disabled BG1 shows backdrop"
        );
    }

    #[test]
    fn renders_a_bg1_4bpp_tile_in_mode1() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 5, 0x03E0); // palette 0 color 5 = green
        set_vram_word(&mut ppu, 0, 2); // entry -> char 2
        fill_4bpp_tile(&mut ppu, 0, 2, 5); // tile 2 = solid color 5

        ppu.write_register(0x2105, 0x01); // mode 1 -> BG1 is 4bpp
        ppu.write_register(0x2107, 0x00);
        ppu.write_register(0x210B, 0x00);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [0, 255, 0],
            "BG1 4bpp tile pixel is green"
        );
    }

    #[test]
    fn applies_horizontal_and_vertical_flip() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        // Tile 1: only the top-left pixel (row 0, col 0) is color 1.
        let base = 16; // char 1 at char_base 0: word 8 -> byte 16
        ppu.vram[base] = 0x80; // plane0 row 0, bit7 (left-most) set
        // Tilemap at word 0x400 (away from the char data): entry -> char 1 with H-flip + V-flip
        // (bits 14,15).
        set_vram_word(&mut ppu, 0x400, 1 | 0x4000 | 0x8000);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x2107, 0x04); // BG1SC: map base word 0x400
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // With both flips, the lit pixel moves to the bottom-right of the tile: BG (7,7). Row 0
        // shows BG line 1, so BG line 7 lands on framebuffer row 6.
        assert_eq!(pixel(&rgb, 7, 6), [255, 255, 255], "flipped pixel at (7,6)");
        assert_eq!(
            pixel(&rgb, 7, 7),
            [0, 0, 0],
            "row 7 shows the next tile row"
        );
        assert_eq!(pixel(&rgb, 0, 0), [0, 0, 0], "top-left now transparent");
    }

    #[test]
    fn applies_horizontal_scroll() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        // Tile at map column 1 (chars 0 elsewhere = transparent), solid color 1.
        set_vram_word(&mut ppu, 1, 1); // entry (0,1) -> char 1
        fill_2bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        // Scroll BG1 right by 8 so tile column 1 lands at screen x=0.
        ppu.write_register(0x210D, 0x08); // hofs low
        ppu.write_register(0x210D, 0x00); // hofs high
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [255, 255, 255], "scrolled tile at x=0");
    }

    #[test]
    fn vertical_scroll_samples_display_line_y_plus_1() {
        // The first visible framebuffer row is display line 1 (line 0 is never rendered), and the
        // BG fetch adds BGnVOFS to the raw display line: row `y` samples BG line `y + 1 + VOFS`
        // (ares `background.cpp` fetchNameTable, Mesen2 `SnesPpu.cpp` realY = _scanline). This is
        // why games write VOFS = -1 to pixel-align a BG with the top of the screen.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        // Tile 1: only the pixel at (fine_x=0, fine_y=1) is lit.
        set_vram_word(&mut ppu, 0, 1);
        set_2bpp_tile_pixel(&mut ppu, 0, 1, 0, 1, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // With VOFS = 0, framebuffer row 0 shows BG line 1: the lit pixel lands on row 0.
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "BG line 1 appears on framebuffer row 0"
        );
        assert_eq!(pixel(&rgb, 0, 1), [0, 0, 0], "BG line 2 on row 1 is dark");
    }

    #[test]
    fn vertical_scroll_of_minus_one_pixel_aligns_the_bg() {
        // VOFS = -1 (0x3FF) cancels the +1: framebuffer row 0 shows BG line 0. This is the
        // convention used by the undisbeliever scpu-a-dma-bug ROMs (issue #2945).
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        // Tile 1: only the pixel at (fine_x=0, fine_y=0) is lit.
        set_vram_word(&mut ppu, 0, 1);
        set_2bpp_tile_pixel(&mut ppu, 0, 1, 0, 0, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x210E, 0xFF); // BG1VOFS low
        ppu.write_register(0x210E, 0xFF); // BG1VOFS high -> vofs = 0x3FF = -1
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "VOFS -1 puts BG line 0 on framebuffer row 0"
        );
    }

    /// Configure BGnSC for layer `bg` with a 32x32 tilemap at `base` words.
    fn set_bg_map_base(ppu: &mut Ppu, bg: usize, base_words: u16) {
        let reg = 0x2107 + bg as u16;
        ppu.write_register(reg, ((base_words >> 10) << 2) as u8);
    }

    #[test]
    fn bg1_draws_over_bg2_at_the_same_priority_in_mode0() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000); // backdrop
        set_cgram(&mut ppu, 1, 0x7FFF); // BG1 palette 0 color 1 = white
        set_cgram(&mut ppu, 33, 0x001F); // BG2 region (bg*32=32) palette 0 color 1 = red
        // BG1 map at word 0, BG2 map at word 0x400.
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2); // BG2 entry -> char 2
        fill_2bpp_tile(&mut ppu, 0, 1, 1); // BG1 tile solid color 1
        fill_2bpp_tile(&mut ppu, 0, 2, 1); // BG2 tile solid color 1

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x212C, 0x03); // TM: BG1 + BG2
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "BG1 wins over BG2 at same priority"
        );
    }

    #[test]
    fn higher_per_tile_priority_wins_across_layers_in_mode0() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // BG1 white
        set_cgram(&mut ppu, 33, 0x001F); // BG2 red
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry, priority 0
        set_vram_word(&mut ppu, 0x400, 2 | 0x2000); // BG2 entry, priority 1 (bit 13)
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x03);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // BG2.1 sits above BG1.0 in the Mode 0 priority chart.
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 0, 0],
            "BG2 priority-1 beats BG1 priority-0"
        );
    }

    #[test]
    fn mode0_bg2_uses_its_own_palette_region() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 33, 0x03E0); // BG2 region color 1 = green
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x400, 2);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x02); // TM: BG2 only
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [0, 255, 0],
            "BG2 resolves CGRAM index 33"
        );
    }

    #[test]
    fn mode1_bg3_high_priority_bit_lifts_bg3_above_bg1() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // BG3 (2bpp, region 0) color 1 = white
        set_cgram(&mut ppu, 5, 0x001F); // BG1 (4bpp) color 5 = red
        set_bg_map_base(&mut ppu, 0, 0x000); // BG1 map
        set_bg_map_base(&mut ppu, 2, 0x400); // BG3 map
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1 (4bpp)
        set_vram_word(&mut ppu, 0x400, 2 | 0x2000); // BG3 entry -> char 2, priority 1
        fill_4bpp_tile(&mut ppu, 0, 1, 5); // BG1 color 5
        fill_2bpp_tile(&mut ppu, 0, 2, 1); // BG3 color 1

        ppu.write_register(0x2105, 0x09); // mode 1 + BG3 priority (bit 3)
        ppu.write_register(0x212C, 0x05); // TM: BG1 + BG3
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "BG3 high-priority lifts above BG1"
        );
    }

    #[test]
    fn renders_a_16x16_bg1_tile_with_correct_subtiles() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x001F); // color 1 = red   (top-left subtile, char N)
        set_cgram(&mut ppu, 2, 0x03E0); // color 2 = green (top-right, N+1)
        set_cgram(&mut ppu, 3, 0x7C00); // color 3 = blue  (bottom-left, N+16)
        // BG1 map entry 0 -> char 4 (top-left of the 2x2 block), palette 0.
        set_vram_word(&mut ppu, 0, 4);
        fill_2bpp_tile(&mut ppu, 0, 4, 1); // N   = red
        fill_2bpp_tile(&mut ppu, 0, 5, 2); // N+1 = green
        fill_2bpp_tile(&mut ppu, 0, 20, 3); // N+16 = blue

        ppu.write_register(0x2105, 0x10); // mode 0, BG1 tile size 16x16 (bit 4)
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [255, 0, 0], "top-left subtile N");
        assert_eq!(pixel(&rgb, 8, 0), [0, 255, 0], "top-right subtile N+1");
        assert_eq!(pixel(&rgb, 0, 8), [0, 0, 255], "bottom-left subtile N+16");
    }

    /// Write a BG map entry into a specific 32x32 sub-screen (SC0..SC3) of layer `bg`.
    fn set_sc_entry(ppu: &mut Ppu, base_words: usize, sc: usize, col: usize, row: usize, v: u16) {
        let word = base_words + sc * 0x400 + row * 32 + col;
        set_vram_word(ppu, word, v);
    }

    #[test]
    fn tilemap_size_64x32_selects_sc1_when_scrolled_right() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // white
        // BG1 map base 0; SC1 entry (col 0,row 0) -> char 1 (solid white).
        set_sc_entry(&mut ppu, 0x000, 1, 0, 0, 1);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x00); // mode 0, 8x8
        ppu.write_register(0x2107, 0x01); // BG1SC: base 0, size 1 (64x32)
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        // Scroll right by 256 px (32 tiles) so screen x=0 reads entry col 32 -> SC1.
        ppu.write_register(0x210D, 0x00);
        ppu.write_register(0x210D, 0x01); // hofs = 0x100 = 256
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "SC1 tile visible after H-scroll"
        );
    }

    #[test]
    fn tilemap_size_32x64_selects_sc1_when_scrolled_down() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        set_sc_entry(&mut ppu, 0x000, 1, 0, 0, 1); // SC1 (bottom) entry
        fill_2bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x2107, 0x02); // BG1SC: size 2 (32x64)
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        // Scroll down 256 px (32 tiles) so screen y=0 reads entry row 32 -> SC1.
        ppu.write_register(0x210E, 0x00);
        ppu.write_register(0x210E, 0x01); // vofs = 0x100 = 256
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "SC1 tile visible after V-scroll"
        );
    }

    #[test]
    fn tilemap_size_64x64_selects_sc3_when_scrolled_diagonally() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        set_sc_entry(&mut ppu, 0x000, 3, 0, 0, 1); // SC3 (bottom-right) entry
        fill_2bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x2107, 0x03); // BG1SC: size 3 (64x64)
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x210D, 0x00);
        ppu.write_register(0x210D, 0x01); // hofs = 256 -> col 32
        ppu.write_register(0x210E, 0x00);
        ppu.write_register(0x210E, 0x01); // vofs = 256 -> row 32
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "SC3 tile visible after diagonal scroll"
        );
    }

    #[test]
    fn bgmode_decodes_mode_priority_and_tile_sizes() {
        let mut ppu = Ppu::new();
        // mode 1, BG3 priority set, BG1 + BG3 16x16 tiles (bits 4 and 6).
        ppu.write_register(0x2105, 0b0101_1001);

        assert_eq!(ppu.bg_mode, 1);
        assert!(ppu.bg3_priority);
        assert_eq!(ppu.bg_tile_size_16, [true, false, true, false]);
    }

    #[test]
    fn bgsc_decodes_tilemap_base_and_size() {
        let mut ppu = Ppu::new();
        // BG2SC ($2108): base bits 2-7 = 0b001001 (9) -> word base 9<<10; size 0b10 = 2.
        ppu.write_register(0x2108, 0b0010_0110);

        assert_eq!(ppu.bg_tilemap_base[1], 9 << 10);
        assert_eq!(ppu.bg_screen_size[1], 2);
    }

    #[test]
    fn bgnba_decodes_char_base_for_each_bg() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x210B, 0x53); // BG1 = 3, BG2 = 5
        ppu.write_register(0x210C, 0x21); // BG3 = 1, BG4 = 2

        assert_eq!(ppu.bg_char_base[0], 3 << 12);
        assert_eq!(ppu.bg_char_base[1], 5 << 12);
        assert_eq!(ppu.bg_char_base[2], 1 << 12);
        assert_eq!(ppu.bg_char_base[3], 2 << 12);
    }

    #[test]
    fn bg_hofs_uses_the_shared_write_twice_latch() {
        let mut ppu = Ppu::new();
        // First HOFS write sets BG_old and the intermediate high bits; second supplies the high
        // byte. The hardware result reconstructs bits 0-7 from the first byte and bits 8-10 from
        // the second: (0x02 & 7) << 8 | 0x1F = 0x21F.
        ppu.write_register(0x210D, 0x1F);
        ppu.write_register(0x210D, 0x02);

        assert_eq!(ppu.bg_hofs[0], 0x21F);
    }

    #[test]
    fn bg_vofs_uses_the_shared_write_twice_latch() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x210E, 0x34); // low byte -> bg_old = 0x34
        ppu.write_register(0x210E, 0x01); // high byte

        // vofs = (0x01 << 8) | 0x34 = 0x134
        assert_eq!(ppu.bg_vofs[0], 0x134);
    }

    #[test]
    fn bg_old_latch_is_shared_across_layers() {
        let mut ppu = Ppu::new();
        // Write the low byte of BG1HOFS, then the low byte of BG2VOFS: the second write sees the
        // BG_old left by the first. Scroll values are stored unmasked (masked when applied).
        ppu.write_register(0x210D, 0xAB); // bg_old = 0xAB
        ppu.write_register(0x2110, 0x05); // BG2VOFS: vofs = (0x05<<8) | 0xAB = 0x5AB

        assert_eq!(ppu.bg_vofs[1], 0x5AB);
    }

    #[test]
    fn tm_stores_the_main_screen_enable_mask() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x212C, 0x13);
        assert_eq!(ppu.tm, 0x13);
    }

    #[test]
    fn renders_a_mode3_bg1_8bpp_tile() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 200, 0x7FFF); // 8bpp color index 200 -> white
        set_vram_word(&mut ppu, 0, 1); // BG1 entry -> char 1
        fill_8bpp_tile(&mut ppu, 0, 1, 200); // solid color 200

        ppu.write_register(0x2105, 0x03); // mode 3
        ppu.write_register(0x2107, 0x00); // BG1SC base 0
        ppu.write_register(0x210B, 0x00); // BG1 char base 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "8bpp BG1 resolves CGRAM 200"
        );
    }

    #[test]
    fn mode3_bg2_is_4bpp_over_bg1() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 200, 0x001F); // BG1 8bpp color 200 = red
        set_cgram(&mut ppu, 1, 0x7FFF); // BG2 4bpp palette 0 color 1 = white
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry char 1 (8bpp, words 32-63)
        set_vram_word(&mut ppu, 0x400, 4); // BG2 entry char 4 (4bpp, words 64-79; no overlap)
        fill_8bpp_tile(&mut ppu, 0, 1, 200);
        fill_4bpp_tile(&mut ppu, 0, 4, 1);

        ppu.write_register(0x2105, 0x03); // mode 3
        ppu.write_register(0x212C, 0x03); // TM: BG1 + BG2
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // BG1.0 sits above BG2.0 in modes 2-5; both priority 0 -> BG1 wins.
        assert_eq!(pixel(&rgb, 0, 0), [255, 0, 0], "BG1 over BG2 in mode 3");
    }

    #[test]
    fn mode4_bg2_is_2bpp() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x03E0); // BG2 2bpp palette 0 color 1 = green
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x400, 2);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x04); // mode 4
        ppu.write_register(0x212C, 0x02); // TM: BG2 only
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [0, 255, 0], "mode 4 BG2 is 2bpp");
    }

    #[test]
    fn mode6_renders_bg1_only() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // BG1 4bpp color 1 = white
        set_vram_word(&mut ppu, 0, 1);
        fill_4bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x06); // mode 6
        ppu.write_register(0x212C, 0x03); // TM: BG1 + BG2 (BG2 not displayed in mode 6)
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        // sub screen (backdrop here, since TS is empty).
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            &rgb[3..6],
            [255, 255, 255],
            "mode 6 BG1 renders on odd columns"
        );
    }

    #[test]
    fn mode5_renders_bg1_4bpp_at_standard_width() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7C00); // BG1 4bpp color 1 = blue
        set_vram_word(&mut ppu, 0, 1);
        fill_4bpp_tile(&mut ppu, 0, 1, 1);

        ppu.write_register(0x2105, 0x05); // mode 5
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        // Main screen lands on the odd output columns; the even column shows the
        // sub screen (backdrop here, since TS is empty).
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(&rgb[3..6], [0, 0, 255], "mode 5 BG1 renders on odd columns");
    }

    #[test]
    fn mode5_hi_res_places_sub_on_even_and_main_on_odd_columns() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // main BG1 color 1 = white
        set_cgram(&mut ppu, 5, 0x001F); // sub BG2 palette 1 color 1 = red
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2 | (1 << 10)); // BG2 entry -> char 2, palette 1
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x05); // mode 5
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(rgb.len(), 512 * 224 * 3);
        // Hardware/Mesen2: the sub screen supplies the even (left) half-pixel and the
        // main screen the odd (right) one (Mesen2 ApplyHiResMode).
        assert_eq!(&rgb[0..3], &[255, 0, 0], "even column uses sub screen");
        assert_eq!(&rgb[3..6], &[255, 255, 255], "odd column uses main screen");
    }

    #[test]
    fn pseudo_hires_shifts_sub_screen_to_the_first_half_pixel() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF); // main BG1 color 1 = white
        set_cgram(&mut ppu, 33, 0x001F); // sub BG2 color 1 = red (mode 0 region)
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2); // BG2 entry -> char 2
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2133, 0x08); // pseudo-hires
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(rgb.len(), 512 * 224 * 3);
        assert_eq!(
            &rgb[0..3],
            &[255, 0, 0],
            "first half-pixel uses shifted sub"
        );
        assert_eq!(&rgb[3..6], &[255, 255, 255], "second half-pixel uses main");
    }

    /// Mode 1 scene with white BG1 on the main screen and red BG2 (palette 1) on the
    /// sub screen (CGWSEL sub-screen BG/OBJ enabled), for the line-buffer tests.
    fn setup_mode1_main_bg1_sub_bg2(ppu: &mut Ppu) {
        set_cgram(ppu, 0, 0x0000);
        set_cgram(ppu, 1, 0x7FFF); // main BG1 color 1 = white
        set_cgram(ppu, 17, 0x001F); // sub BG2 palette 1 color 1 = red
        set_bg_map_base(ppu, 0, 0x000);
        set_bg_map_base(ppu, 1, 0x400);
        set_vram_word(ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(ppu, 0x400, 2 | (1 << 10)); // BG2 entry -> char 2, palette 1
        fill_4bpp_tile(ppu, 0, 1, 1);
        fill_4bpp_tile(ppu, 0, 2, 1);

        ppu.write_register(0x2105, 0x01); // mode 1 (BG1 + BG2 both 4bpp)
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // enable sub-screen BG/OBJ
        ppu.write_register(0x2100, 0x0F);
    }

    #[test]
    fn render_dot_populates_main_and_sub_line_buffers() {
        let mut ppu = Ppu::new();
        setup_mode1_main_bg1_sub_bg2(&mut ppu);
        render_first_visible_line(&mut ppu);

        assert_eq!(ppu.line_main[0].color, 0x7FFF, "main buffer holds BG1");
        assert_eq!(ppu.line_main[0].source, super::PixelSource::Bg(0));
        assert_eq!(ppu.line_sub[0].color, 0x001F, "sub buffer holds BG2");
        assert_eq!(ppu.line_sub[0].source, super::PixelSource::Bg(1));
    }

    #[test]
    fn line_main_final_records_post_color_math_output() {
        let mut ppu = Ppu::new();
        setup_mode1_main_bg1_sub_bg2(&mut ppu);
        ppu.write_register(0x2131, 0x41); // CGADSUB: add-half, BG1 enabled
        render_first_visible_line(&mut ppu);

        // (white + red) / 2 = (31,15,15) in BGR555.
        let expected = (15u16 << 10) | (15 << 5) | 31;
        assert_eq!(
            ppu.line_main_final[0], expected,
            "final buffer holds the post-color-math output"
        );
        assert_ne!(
            ppu.line_main_final[0], ppu.line_main[0].color,
            "final differs from the pre-math resolve"
        );
        assert_eq!(
            ppu.framebuffer[0], expected,
            "framebuffer matches the finalized pixel"
        );
    }

    #[test]
    fn direct_color_computes_color_from_index_bypassing_cgram() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 7, 0x0000); // CGRAM[7] black, to prove direct color bypasses it
        set_vram_word(&mut ppu, 0, 1); // BG1 entry char 1, palette 0
        fill_8bpp_tile(&mut ppu, 0, 1, 0x07); // color index 0x07 (RRR=7)

        ppu.write_register(0x2105, 0x03); // mode 3 (BG1 256-color)
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2130, 0x01); // CGWSEL: direct color on
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        // Direct color of (0x07, palette 0): Red5 = (7<<2)|0 = 28 -> R8 = 231; G=B=0.
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [231, 0, 0],
            "direct color computed, not CGRAM[7]"
        );
    }

    #[test]
    fn direct_color_off_uses_cgram_lookup() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 7, 0x7FFF); // CGRAM[7] white
        set_vram_word(&mut ppu, 0, 1);
        fill_8bpp_tile(&mut ppu, 0, 1, 0x07);

        ppu.write_register(0x2105, 0x03);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2130, 0x00); // CGWSEL: direct color OFF
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [255, 255, 255], "direct off -> CGRAM[7]");
    }

    #[test]
    fn direct_color_includes_palette_low_bits() {
        let mut ppu = Ppu::new();
        set_vram_word(&mut ppu, 0, 1 | (1 << 10)); // BG1 entry char 1, palette 1 (bgr bit0=r=1)
        fill_8bpp_tile(&mut ppu, 0, 1, 0x01); // color RRR low bit

        ppu.write_register(0x2105, 0x03);
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2130, 0x01); // direct color on
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        // Red5 = (color[2:0]<<2) | (palette[0]<<1) = (1<<2) | (1<<1) = 6 -> R8 = 49.
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [49, 0, 0], "palette bit0 adds to red");
    }

    #[test]
    fn offset_per_tile_mode2_h_offset_redirects_bg1() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000); // backdrop black
        set_cgram(&mut ppu, 1, 0x7FFF); // BG1 4bpp color 1 = white
        set_bg_map_base(&mut ppu, 0, 0x000); // BG1 map
        set_vram_word(&mut ppu, 4, 1); // BG1 (col 4, row 0) -> char 1; other columns char 0 (transparent)
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        // BG3 offset map; entry (0,0) = valid-BG1 (0x2000) + H offset 0x18 (=24).
        set_bg_map_base(&mut ppu, 2, 0x400);
        set_vram_word(&mut ppu, 0x400, 0x2018);

        ppu.write_register(0x2105, 0x02); // mode 2
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // Tile column 1 (x=8..15) is redirected to BG1 column 4 -> white.
        assert_eq!(
            pixel(&rgb, 8, 0),
            [255, 255, 255],
            "H offset redirects BG1 to column 4"
        );
        // Column 0 (x=0..7) is exempt from offset -> samples column 0 (transparent) -> backdrop.
        assert_eq!(pixel(&rgb, 0, 0), [0, 0, 0], "first column is exempt");
    }

    #[test]
    fn offset_per_tile_mode2_v_offset_redirects_bg1() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_vram_word(&mut ppu, 4 * 32 + 1, 1); // BG1 (col 1, row 4) -> char 1
        fill_4bpp_tile(&mut ppu, 0, 1, 1);
        // BG3: 1st (H) fetch (0,0) = 0 (no H offset); 2nd (V) fetch (0,1) = valid-BG1 + offset 0x18.
        set_bg_map_base(&mut ppu, 2, 0x400);
        set_vram_word(&mut ppu, 0x400, 0x0000); // H lookup: no valid bit
        set_vram_word(&mut ppu, 0x400 + 32, 0x2018); // V lookup (row 1): valid-BG1 + 0x18

        ppu.write_register(0x2105, 0x02); // mode 2
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // At (x=8,y=8): no H offset (col 1), V offset redirects to row 4 -> BG1 (col1,row4) white.
        assert_eq!(
            pixel(&rgb, 8, 8),
            [255, 255, 255],
            "V offset redirects BG1 to row 4"
        );
    }

    #[test]
    fn offset_per_tile_mode4_bit15_selects_v_offset() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 5, 0x7FFF); // BG1 8bpp color 5 = white
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_vram_word(&mut ppu, 4 * 32 + 1, 1); // BG1 (col 1, row 4) -> char 1
        fill_8bpp_tile(&mut ppu, 0, 1, 5);
        // Mode 4: single BG3 fetch. bit15 set -> apply to V. valid-BG1 (0x2000) + V offset 0x18.
        set_bg_map_base(&mut ppu, 2, 0x400);
        set_vram_word(&mut ppu, 0x400, 0x2000 | 0x8000 | 0x18);

        ppu.write_register(0x2105, 0x04); // mode 4
        ppu.write_register(0x212C, 0x01);
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // At (x=8,y=8): bit15 -> V offset to row 4; H unchanged (col 1) -> BG1 (col1,row4) white.
        assert_eq!(
            pixel(&rgb, 8, 8),
            [255, 255, 255],
            "mode4 bit15 applies offset to V"
        );
    }

    #[test]
    fn color_math_combines_main_and_sub_screen_pixels() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x001F); // BG1 color 1 = red
        set_cgram(&mut ppu, 33, 0x03E0); // BG2 color 1 = green
        set_bg_map_base(&mut ppu, 0, 0x000);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x000, 1); // BG1 entry -> char 1
        set_vram_word(&mut ppu, 0x400, 2); // BG2 entry -> char 2
        fill_2bpp_tile(&mut ppu, 0, 1, 1); // solid red
        fill_2bpp_tile(&mut ppu, 0, 2, 1); // solid green

        ppu.write_register(0x2105, 0x00); // mode 0
        ppu.write_register(0x212C, 0x02); // main screen: BG2
        ppu.write_register(0x212D, 0x01); // sub screen: BG1
        ppu.write_register(0x2130, 0x02); // sub screen BG/OBJ enable
        ppu.write_register(0x2131, 0x42); // add + div2 + BG2 math enable
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [123, 123, 0],
            "main BG2 green should be color-mathed with sub BG1 red"
        );
    }

    #[test]
    fn window_area_can_disable_a_layer_inside_the_window() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        set_vram_word(&mut ppu, 0x000, 1);
        set_vram_word(&mut ppu, 0x019, 1); // x=200 lands in tile column 25

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01); // main screen: BG1
        // W12SEL BG1 nibble is `EIei`: bit 1 = window-1 enable, bit 0 = window-1
        // invert. %0010 is "enabled, inside" (see the vendored hardware header
        // undisbeliever-inidisp/sources/src/_common/registers.inc, WSEL::win1).
        ppu.write_register(0x2123, 0x02);
        ppu.write_register(0x2126, 0x00); // WH0 = 0
        ppu.write_register(0x2127, 0x7F); // WH1 = 127
        ppu.write_register(0x212E, 0x01); // disable BG1 inside the window
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(pixel(&rgb, 0, 0), [0, 0, 0], "windowed pixel falls back");
        assert_eq!(
            pixel(&rgb, 200, 0),
            [255, 255, 255],
            "outside window remains visible"
        );
    }

    #[test]
    fn bg2_window_selection_uses_the_high_bits_of_w12sel() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 33, 0x7FFF);
        set_bg_map_base(&mut ppu, 1, 0x400);
        set_vram_word(&mut ppu, 0x400, 2);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);
        set_vram_word(&mut ppu, 0x419, 2);

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x02); // main screen: BG2
        ppu.write_register(0x2123, 0x20); // BG2 window1 enabled, inside (not inverted)
        ppu.write_register(0x2126, 0x00);
        ppu.write_register(0x2127, 0x7F);
        ppu.write_register(0x212E, 0x02); // disable BG2 inside the window
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 0, 0),
            [0, 0, 0],
            "BG2 is masked inside the window"
        );
        assert_eq!(
            pixel(&rgb, 200, 0),
            [255, 255, 255],
            "BG2 stays visible outside"
        );
    }

    #[test]
    fn sub_screen_backdrop_participates_in_color_math() {
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x001F); // BG1 color 1 = red
        set_vram_word(&mut ppu, 0x000, 1);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        ppu.write_register(0x2132, 0xE0); // sub backdrop black
        ppu.write_register(0x2132, 0x5F); // sub backdrop green

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01); // main screen: BG1
        ppu.write_register(0x212D, 0x00); // sub screen: backdrop only
        ppu.write_register(0x2130, 0x02); // sub screen BG/OBJ enable
        ppu.write_register(0x2131, 0x41); // add + div2 + BG1 math enable
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // TS = 0 with CGWSEL bit 1 set is the empty-sub-screen fallback: the
        // fixed colour is substituted AND halving is disabled (#3012), so the
        // green arrives at full strength rather than halved to olive. This
        // expectation previously encoded the missing rule.
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 0],
            "sub-screen fixed color blends at full strength"
        );
    }

    #[test]
    fn two_window_or_combination_masks_pixels_in_either_window() {
        // When both windows are enabled for a layer (WBGLOG mode=OR) any pixel inside
        // either W1 or W2 is masked; pixels outside both are visible.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        // Keep tile data at char_base=0 (words 8-15 for tile 1).
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        // Place the BG1 map at 0x400 so it doesn't overlap with tile pixel data.
        set_bg_map_base(&mut ppu, 0, 0x400);
        for col in 0..32usize {
            set_vram_word(&mut ppu, 0x400 + col, 1);
        }

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01); // main screen: BG1
        // W1=[0,40], W2=[60,100]; both enabled as "inside" for BG1 window1 and window2.
        ppu.write_register(0x2123, 0x0A); // BG1 window1=%10, window2=%10 (both enabled, inside)
        ppu.write_register(0x2126, 0x00); // WH0=0
        ppu.write_register(0x2127, 0x28); // WH1=40
        ppu.write_register(0x2128, 0x3C); // WH2=60
        ppu.write_register(0x2129, 0x64); // WH3=100
        ppu.write_register(0x212A, 0x00); // WBGLOG: BG1 uses OR (bits 1-0 = 00)
        ppu.write_register(0x212E, 0x01); // TMW: mask BG1 inside the window area
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 10, 0),
            [0, 0, 0],
            "x=10 is inside W1, should be masked"
        );
        assert_eq!(
            pixel(&rgb, 70, 0),
            [0, 0, 0],
            "x=70 is inside W2, should be masked"
        );
        assert_eq!(
            pixel(&rgb, 50, 0),
            [255, 255, 255],
            "x=50 is outside both W1 and W2, should be visible"
        );
    }

    #[test]
    fn two_window_and_combination_masks_only_pixels_in_both_windows() {
        // When both windows are enabled for a layer (WBGLOG mode=AND) only pixels inside
        // both W1 and W2 are masked; pixels inside only one window remain visible.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        // Place the BG1 map at 0x400 so it doesn't overlap with tile pixel data.
        set_bg_map_base(&mut ppu, 0, 0x400);
        for col in 0..32usize {
            set_vram_word(&mut ppu, 0x400 + col, 1);
        }

        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01); // main screen: BG1
        // W1=[0,80], W2=[40,120]; overlap [40,80]. Only overlap (AND) should be masked.
        ppu.write_register(0x2123, 0x0A); // BG1 window1=%10, window2=%10 (both enabled, inside)
        ppu.write_register(0x2126, 0x00); // WH0=0
        ppu.write_register(0x2127, 0x50); // WH1=80
        ppu.write_register(0x2128, 0x28); // WH2=40
        ppu.write_register(0x2129, 0x78); // WH3=120
        ppu.write_register(0x212A, 0x01); // WBGLOG: BG1 uses AND (bits 1-0 = 01)
        ppu.write_register(0x212E, 0x01); // TMW: mask BG1 inside the window area
        ppu.write_register(0x2100, 0x0F);
        render_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            pixel(&rgb, 20, 0),
            [255, 255, 255],
            "x=20 inside W1 only, not masked with AND"
        );
        assert_eq!(
            pixel(&rgb, 100, 0),
            [255, 255, 255],
            "x=100 inside W2 only, not masked with AND"
        );
        assert_eq!(
            pixel(&rgb, 60, 0),
            [0, 0, 0],
            "x=60 inside both W1 and W2, masked with AND"
        );
    }

    // ── Colour-window regions and halve suppression (#3011 / #3012) ──────────

    /// Red BG1 on the main screen, black main backdrop, and a fixed colour of
    /// full green in COLDATA -- so an add shows up as yellow, a halved add as
    /// olive, and a clipped main as pure green.
    fn setup_red_bg1_green_fixed(ppu: &mut Ppu) {
        set_cgram(ppu, 0, 0x0000);
        set_cgram(ppu, 1, 0x001F); // BG1 colour 1 = red
        fill_2bpp_tile(ppu, 0, 1, 1);
        set_bg_map_base(ppu, 0, 0x400);
        for col in 0..32usize {
            set_vram_word(ppu, 0x400 + col, 1);
        }
        ppu.write_register(0x2105, 0x00); // Mode 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x00); // TS: nothing
        ppu.write_register(0x2132, 0xE0); // COLDATA: all planes 0
        ppu.write_register(0x2132, 0x5F); // COLDATA: green = 31
        ppu.write_register(0x2100, 0x0F);
    }

    /// Colour window 1 spanning x = 64..191 (WOBJSEL bit 5 = enable, bit 4 =
    /// invert, so 0x20 is "enabled, inside").
    fn set_color_window_64_191(ppu: &mut Ppu) {
        ppu.write_register(0x2125, 0x20);
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
    }

    const YELLOW: [u8; 3] = [255, 255, 0];
    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const OLIVE: [u8; 3] = [123, 123, 0];

    #[test]
    fn prevent_color_math_region_is_where_math_is_suppressed() {
        // CGWSEL bits 5-4 name where colour math is PREVENTED: 1 = prevented
        // outside the window (so maths runs INSIDE), 2 = prevented inside (so
        // maths runs OUTSIDE). NESER had the two the wrong way round (#3011).
        // Vendored registers.inc: CGWSEL::prevent { outside = %00010000,
        // inside = %00100000 }.
        for (prevent, math_at_100, math_at_0, name) in [
            (0x10u8, true, false, "prevent outside -> math inside"),
            (0x20, false, true, "prevent inside -> math outside"),
        ] {
            let mut ppu = Ppu::new();
            setup_red_bg1_green_fixed(&mut ppu);
            set_color_window_64_191(&mut ppu);
            ppu.write_register(0x2130, prevent); // no clip, fixed-colour operand
            ppu.write_register(0x2131, 0x01); // add, BG1 enabled, no half

            let row = render_row0(&mut ppu);
            assert_eq!(
                row[100],
                if math_at_100 { YELLOW } else { RED },
                "{name} @100"
            );
            assert_eq!(row[0], if math_at_0 { YELLOW } else { RED }, "{name} @0");
        }
    }

    #[test]
    fn clip_to_black_region_is_where_the_main_pixel_is_blacked() {
        // CGWSEL bits 7-6: 1 = clip outside the window, 2 = clip inside.
        for (clip, black_at_100, name) in
            [(0x40u8, false, "clip outside"), (0x80, true, "clip inside")]
        {
            let mut ppu = Ppu::new();
            setup_red_bg1_green_fixed(&mut ppu);
            set_color_window_64_191(&mut ppu);
            ppu.write_register(0x2130, clip);
            ppu.write_register(0x2131, 0x00); // no colour math at all

            let row = render_row0(&mut ppu);
            assert_eq!(
                row[100],
                if black_at_100 { BLACK } else { RED },
                "{name} @100"
            );
            assert_eq!(row[0], if black_at_100 { RED } else { BLACK }, "{name} @0");
        }
    }

    #[test]
    fn clip_to_black_applies_even_when_the_layer_has_no_color_math() {
        // Mesen2 applies the clip before the CGADSUB per-layer gate, so a layer
        // with its math bit clear is still blacked out inside the clip region.
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        set_color_window_64_191(&mut ppu);
        ppu.write_register(0x2130, 0x80); // clip inside
        ppu.write_register(0x2131, 0x00); // BG1 NOT enabled for colour math

        let row = render_row0(&mut ppu);
        assert_eq!(
            row[100], BLACK,
            "clipped despite the math gate being closed"
        );
        assert_eq!(row[0], RED, "untouched outside the clip region");
    }

    #[test]
    fn clipping_the_main_pixel_to_black_suppresses_halving() {
        // Hardware disables the CGADSUB half when the main pixel was clipped to
        // black, so the operand lands at FULL strength (Mesen2 sets
        // halfShift = 0 alongside pixelA = 0).
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        set_color_window_64_191(&mut ppu);
        ppu.write_register(0x2130, 0x80); // clip inside, fixed-colour operand
        ppu.write_register(0x2131, 0x41); // add + half + BG1

        let row = render_row0(&mut ppu);
        assert_eq!(row[100], GREEN, "clipped: full-strength green, not halved");
        assert_eq!(row[0], OLIVE, "unclipped: red + green, halved");
    }

    #[test]
    fn clip_always_also_suppresses_halving() {
        // CGWSEL clip mode 3 ("always"). ares applies one uniform rule --
        // halving is off whenever the main pixel is black -- while Mesen2
        // omits it in this branch only. NESER deliberately follows ares here;
        // no vendored ROM exercises mode 3, so no golden depends on it.
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        ppu.write_register(0x2130, 0xC0); // clip always
        ppu.write_register(0x2131, 0x41); // add + half + BG1

        let row = render_row0(&mut ppu);
        assert!(
            row.iter().all(|&px| px == GREEN),
            "always-clipped pixels take the operand at full strength"
        );
    }

    #[test]
    fn empty_sub_screen_fallback_suppresses_halving() {
        // CGWSEL bit 1 set asks for the sub screen as the operand, but nothing
        // is on the sub screen here. Hardware substitutes the fixed colour AND
        // disables halving (Mesen2's _subScreenPriority[x] == 0 branch) -- the
        // rule NESER was missing (#3012).
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        ppu.write_register(0x2130, 0x02); // add subscreen, no clip, no prevent
        ppu.write_register(0x2131, 0x41); // add + half + BG1

        let row = render_row0(&mut ppu);
        assert!(
            row.iter().all(|&px| px == YELLOW),
            "fixed-colour fallback is added at full strength"
        );
    }

    #[test]
    fn fixed_color_operand_still_halves_when_add_subscreen_is_clear() {
        // The suppression above is specific to the empty-SUB-SCREEN fallback.
        // With CGWSEL bit 1 clear the fixed colour is the operand by design and
        // halving applies normally -- the guard against over-applying #3012.
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        ppu.write_register(0x2130, 0x00); // fixed-colour operand by design
        ppu.write_register(0x2131, 0x41); // add + half + BG1

        let row = render_row0(&mut ppu);
        assert!(
            row.iter().all(|&px| px == OLIVE),
            "a deliberate fixed-colour operand is still halved"
        );
    }

    #[test]
    fn populated_sub_screen_operand_halves_normally() {
        // And with a real sub-screen layer present, halving applies.
        let mut ppu = Ppu::new();
        setup_red_bg1_green_fixed(&mut ppu);
        set_cgram(&mut ppu, 33, 0x03E0); // BG2 colour 1 = green
        fill_2bpp_tile(&mut ppu, 0, 2, 1);
        set_bg_map_base(&mut ppu, 1, 0x500);
        for col in 0..32usize {
            set_vram_word(&mut ppu, 0x500 + col, 2);
        }
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // add subscreen
        ppu.write_register(0x2131, 0x41); // add + half + BG1

        let row = render_row0(&mut ppu);
        assert_eq!(row[0], OLIVE, "a present sub-screen layer halves normally");
    }

    // ── Window mask decode and evaluation (#3011) ────────────────────────────

    /// Render one frame of a full-width white BG1 and return the row-0 pixels,
    /// so window tests can assert on the masked/unmasked spans directly.
    fn render_row0(ppu: &mut Ppu) -> Vec<[u8; 3]> {
        render_frame(ppu);
        let rgb = ppu.screen_snapshot_rgb();
        (0..256).map(|x| pixel(&rgb, x, 0)).collect()
    }

    /// White BG1 across the whole screen at CGRAM colour 1, black backdrop.
    fn setup_white_bg1(ppu: &mut Ppu) {
        set_cgram(ppu, 0, 0x0000);
        set_cgram(ppu, 1, 0x7FFF);
        fill_2bpp_tile(ppu, 0, 1, 1);
        set_bg_map_base(ppu, 0, 0x400);
        for col in 0..32usize {
            set_vram_word(ppu, 0x400 + col, 1);
        }
        ppu.write_register(0x2105, 0x00); // Mode 0
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x2100, 0x0F);
    }

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    #[test]
    fn window_mask_settings_decode_enable_from_the_high_bit_of_each_pair() {
        // W12SEL/W34SEL/WOBJSEL nibbles are `EIei`: within each 2-bit pair the
        // HIGH bit enables the window and the LOW bit inverts it (Mesen2
        // ProcessWindowMaskSettings, ares io.cpp:478, and the vendored
        // registers.inc WSEL constants: win1 enable = %0010, outside = %0001).
        let mut ppu = Ppu::new();

        // Every layer's window-1 enable bit, one register at a time.
        ppu.write_register(0x2123, 0x02);
        assert!(ppu.windows[0].active_layers[0], "BG1 win1 enable = bit 1");
        assert!(!ppu.windows[0].inverted_layers[0], "bit 1 is not invert");

        ppu.write_register(0x2123, 0x01);
        assert!(
            !ppu.windows[0].active_layers[0],
            "bit 0 alone must NOT enable the window"
        );
        assert!(ppu.windows[0].inverted_layers[0], "BG1 win1 invert = bit 0");

        // Full per-bit map for all three registers.
        ppu.write_register(0x2123, 0xFF);
        ppu.write_register(0x2124, 0xFF);
        ppu.write_register(0x2125, 0xFF);
        for layer in 0..6 {
            assert!(ppu.windows[0].active_layers[layer], "layer {layer} win1 on");
            assert!(ppu.windows[1].active_layers[layer], "layer {layer} win2 on");
            assert!(
                ppu.windows[0].inverted_layers[layer],
                "layer {layer} w1 inv"
            );
            assert!(
                ppu.windows[1].inverted_layers[layer],
                "layer {layer} w2 inv"
            );
        }

        // Layer indices: 0-3 = BG1-4, 4 = OBJ, 5 = colour window.
        ppu.write_register(0x2123, 0x00);
        ppu.write_register(0x2124, 0x00);
        ppu.write_register(0x2125, 0x00);
        ppu.write_register(0x2124, 0x20); // W34SEL high nibble = BG4 win1 enable
        assert!(ppu.windows[0].active_layers[3], "W34SEL bit 5 = BG4 win1");
        assert!(!ppu.windows[0].active_layers[2], "BG3 untouched");
        ppu.write_register(0x2125, 0x20); // WOBJSEL high nibble = colour win1 enable
        assert!(
            ppu.windows[0].active_layers[5],
            "WOBJSEL bit 5 = colour-window win1"
        );
        assert!(!ppu.windows[0].active_layers[4], "OBJ untouched");
    }

    #[test]
    fn window_coordinates_populate_both_window_configs() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2126, 10); // WH0 = window 1 left
        ppu.write_register(0x2127, 20); // WH1 = window 1 right
        ppu.write_register(0x2128, 30); // WH2 = window 2 left
        ppu.write_register(0x2129, 40); // WH3 = window 2 right
        assert_eq!((ppu.windows[0].left, ppu.windows[0].right), (10, 20));
        assert_eq!((ppu.windows[1].left, ppu.windows[1].right), (30, 40));
    }

    #[test]
    fn mask_logic_registers_map_each_bit_pair_to_its_layer() {
        // WBGLOG ($212A) is `44332211`: BG1 = bits 1-0, BG2 = 3-2, BG3 = 5-4,
        // BG4 = 7-6. WOBJLOG ($212B) is `----ccoo`: OBJ = bits 1-0, colour
        // window = bits 3-2. Deriving BG3/BG4's pair from the W34SEL nibble
        // instead made them read BG1/BG2's logic bits (#3011).
        let mut ppu = Ppu::new();
        ppu.write_register(0x212A, 0b11_10_01_00);
        assert_eq!(ppu.mask_logic[0], 0, "BG1 = OR");
        assert_eq!(ppu.mask_logic[1], 1, "BG2 = AND");
        assert_eq!(ppu.mask_logic[2], 2, "BG3 = XOR");
        assert_eq!(ppu.mask_logic[3], 3, "BG4 = XNOR");

        ppu.write_register(0x212B, 0b0000_10_01);
        assert_eq!(ppu.mask_logic[4], 1, "OBJ = AND");
        assert_eq!(ppu.mask_logic[5], 2, "colour window = XOR");
    }

    #[test]
    fn window_decode_is_rederived_after_a_save_state_restore() {
        // The decoded window cache is derived state; the raw registers stay the
        // save-state source of truth, so a restore must rebuild it.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2123, 0x0A);
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
        ppu.write_register(0x212A, 0x01);
        let state = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");
        assert!(restored.windows[0].active_layers[0], "win1 enable restored");
        assert!(restored.windows[1].active_layers[0], "win2 enable restored");
        assert_eq!(
            (restored.windows[0].left, restored.windows[0].right),
            (64, 191)
        );
        assert_eq!(restored.mask_logic[0], 1, "BG1 mask logic restored");
    }

    #[test]
    fn single_window_inverted_masks_outside_the_span() {
        // %0011 = enabled + inverted: the masked area is everything OUTSIDE
        // [WH0, WH1]. NESER used to decode %0010 and %0011 identically, which
        // is why the ROM suite rendered the same picture for both (#3011).
        let mut ppu = Ppu::new();
        setup_white_bg1(&mut ppu);
        ppu.write_register(0x2123, 0x03); // BG1 win1: enable + invert
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
        ppu.write_register(0x212E, 0x01); // TMW: BG1

        let row = render_row0(&mut ppu);
        assert_eq!(row[0], BLACK, "x=0 outside the span is masked");
        assert_eq!(row[63], BLACK, "x=63 outside the span is masked");
        assert_eq!(row[64], WHITE, "x=64 is the inclusive left edge");
        assert_eq!(row[191], WHITE, "x=191 is the inclusive right edge");
        assert_eq!(row[192], BLACK, "x=192 outside the span is masked");
    }

    #[test]
    fn invert_bit_without_enable_leaves_the_layer_unmasked() {
        // %0001 sets only the invert bit. The window is DISABLED, so with no
        // other window enabled nothing is masked. NESER used to treat any
        // nonzero pair as enabled.
        let mut ppu = Ppu::new();
        setup_white_bg1(&mut ppu);
        ppu.write_register(0x2123, 0x01); // BG1 win1: invert only, not enabled
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
        ppu.write_register(0x212E, 0x01);

        let row = render_row0(&mut ppu);
        assert!(
            row.iter().all(|&px| px == WHITE),
            "a disabled window must not mask anything"
        );
    }

    #[test]
    fn no_enabled_window_never_masks_even_with_xnor_logic() {
        // Mesen2 ProcessMaskWindow returns false for activeWindowCount == 0 --
        // XNOR does NOT degenerate into "always masked".
        let mut ppu = Ppu::new();
        setup_white_bg1(&mut ppu);
        ppu.write_register(0x2123, 0x00); // no windows enabled for BG1
        ppu.write_register(0x212A, 0x03); // BG1 logic = XNOR
        ppu.write_register(0x212E, 0x01);

        let row = render_row0(&mut ppu);
        assert!(row.iter().all(|&px| px == WHITE), "nothing masked");
    }

    #[test]
    fn single_enabled_window_bypasses_the_mask_logic() {
        // With exactly one window enabled the WBGLOG operator is not applied at
        // all (Mesen2 ProcessMaskWindow case 1) -- an AND setting must not turn
        // a lone window into "masked nowhere".
        let mut ppu = Ppu::new();
        setup_white_bg1(&mut ppu);
        ppu.write_register(0x2123, 0x02); // BG1: window 1 only
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
        ppu.write_register(0x212A, 0x01); // BG1 logic = AND
        ppu.write_register(0x212E, 0x01);

        let row = render_row0(&mut ppu);
        assert_eq!(row[0], WHITE, "outside the lone window");
        assert_eq!(row[100], BLACK, "inside the lone window, AND is bypassed");
    }

    #[test]
    fn two_window_xor_and_xnor_combinations() {
        // W1 = [0,80], W2 = [40,120]. XOR masks the symmetric difference;
        // XNOR masks its complement.
        for (logic, name, expected) in [
            (0x02u8, "XOR", [false, true, false, true]),
            (0x03, "XNOR", [true, false, true, false]),
        ] {
            let mut ppu = Ppu::new();
            setup_white_bg1(&mut ppu);
            ppu.write_register(0x2123, 0x0A); // both windows enabled, inside
            ppu.write_register(0x2126, 0);
            ppu.write_register(0x2127, 80);
            ppu.write_register(0x2128, 40);
            ppu.write_register(0x2129, 120);
            ppu.write_register(0x212A, logic);
            ppu.write_register(0x212E, 0x01);

            let row = render_row0(&mut ppu);
            // x=200: neither; x=20: W1 only; x=60: both; x=100: W2 only.
            for (x, want_masked) in [200usize, 20, 60, 100].iter().zip(expected) {
                let px = row[*x];
                let masked = px == BLACK;
                assert_eq!(masked, want_masked, "{name} at x={x} (px {px:?})");
            }
        }
    }

    #[test]
    fn empty_window_masks_nothing_and_inverted_masks_everything() {
        // left > right is an empty window (Mesen2 PixelNeedsMasking): never
        // inside, so inverting it covers the whole line.
        for (sel, all_masked, name) in [(0x02u8, false, "empty"), (0x03, true, "empty inverted")] {
            let mut ppu = Ppu::new();
            setup_white_bg1(&mut ppu);
            ppu.write_register(0x2123, sel);
            ppu.write_register(0x2126, 200); // WH0 > WH1
            ppu.write_register(0x2127, 100);
            ppu.write_register(0x212E, 0x01);

            let row = render_row0(&mut ppu);
            let want = if all_masked { BLACK } else { WHITE };
            assert!(
                row.iter().all(|&px| px == want),
                "{name} window should render entirely {want:?}"
            );
        }
    }

    #[test]
    fn bg3_uses_its_own_wbglog_bit_pair() {
        // BG3's mask logic lives in WBGLOG bits 5-4. NESER used to read BG1's
        // bits 1-0 for BG3 (and BG2's for BG4), so a BG3-only AND setting was
        // silently evaluated as whatever BG1 was configured for (#3011).
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 65, 0x7FFF); // Mode 0 gives BG3 palette base 64
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        set_bg_map_base(&mut ppu, 2, 0x400);
        for col in 0..32usize {
            set_vram_word(&mut ppu, 0x400 + col, 1);
        }
        ppu.write_register(0x2105, 0x00); // Mode 0: BG3 uses palette 0 too
        ppu.write_register(0x212C, 0x04); // TM: BG3
        ppu.write_register(0x2100, 0x0F);

        ppu.write_register(0x2124, 0x0A); // W34SEL BG3: both windows, inside
        ppu.write_register(0x2126, 0);
        ppu.write_register(0x2127, 80);
        ppu.write_register(0x2128, 40);
        ppu.write_register(0x2129, 120);
        // BG3 = AND (bits 5-4), while BG1's pair (bits 1-0) is set to OR. If
        // the wrong pair is read, x=20 masks as OR instead of staying visible.
        ppu.write_register(0x212A, 0b00_01_00_00);
        ppu.write_register(0x212E, 0x04); // TMW: BG3

        let row = render_row0(&mut ppu);
        assert_eq!(row[20], WHITE, "x=20 is W1-only: AND leaves it visible");
        assert_eq!(row[60], BLACK, "x=60 is in both windows: AND masks it");
        assert_eq!(row[100], WHITE, "x=100 is W2-only: AND leaves it visible");
    }

    #[test]
    fn sub_screen_window_masking_uses_tsw() {
        // TSW ($212F) gates the same window evaluation for the sub screen. With
        // the sub screen as the colour-math operand, masking BG2 out of it
        // removes its contribution inside the window.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x001F); // BG1 red on the main screen
        set_cgram(&mut ppu, 33, 0x03E0); // BG2 green on the sub screen
        fill_2bpp_tile(&mut ppu, 0, 1, 1);
        fill_2bpp_tile(&mut ppu, 0, 2, 1);
        set_bg_map_base(&mut ppu, 0, 0x400);
        set_bg_map_base(&mut ppu, 1, 0x500);
        for col in 0..32usize {
            set_vram_word(&mut ppu, 0x400 + col, 1);
            set_vram_word(&mut ppu, 0x500 + col, 2);
        }
        ppu.write_register(0x2105, 0x00);
        ppu.write_register(0x212C, 0x01); // TM: BG1
        ppu.write_register(0x212D, 0x02); // TS: BG2
        ppu.write_register(0x2130, 0x02); // add subscreen
        ppu.write_register(0x2131, 0x01); // add, BG1 math enabled
        ppu.write_register(0x2132, 0xE0); // fixed colour black
        ppu.write_register(0x2100, 0x0F);

        ppu.write_register(0x2123, 0x20); // W12SEL BG2 win1: enable, inside
        ppu.write_register(0x2126, 64);
        ppu.write_register(0x2127, 191);
        ppu.write_register(0x212F, 0x02); // TSW: mask BG2 on the sub screen

        let row = render_row0(&mut ppu);
        assert_eq!(row[0], [255, 255, 0], "outside: red + green sub");
        assert_eq!(
            row[100],
            [255, 0, 0],
            "inside: BG2 masked off the sub screen"
        );
    }

    // ── Mosaic rendering ─────────────────────────────────────────────────────

    /// Set up a BG1 Mode 0 scene: BG1 enabled, no scroll, tilemap at word 0, char base at 0.
    fn setup_bg1_mode0(ppu: &mut Ppu) {
        ppu.write_register(0x2105, 0x00); // Mode 0
        ppu.write_register(0x2107, 0x00); // BG1SC: tilemap base 0
        ppu.write_register(0x210B, 0x00); // BG12NBA: char base 0
        ppu.write_register(0x212C, 0x01); // TM: BG1 only
        ppu.write_register(0x2100, 0x0F); // INIDISP: full brightness
    }

    #[test]
    fn horizontal_mosaic_replicates_leftmost_pixel_of_each_block() {
        // Tile 1: only column 0 (fine_x=0) is white; all other columns are black (color 0).
        // With tilemap full of tile 1 and mosaic block_size=4: pixels 0-3 show white (x_mos=0),
        // pixels 4-7 show black (x_mos=4, fine_x=4), pixels 8-11 show white (fine_x=0), etc.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000); // color 0 = black (also backdrop)
        set_cgram(&mut ppu, 1, 0x7FFF); // color 1 = white

        // Tilemap: all entries point to char 1.
        for col in 0..32usize {
            set_vram_word(&mut ppu, col, 1);
        }
        // Tile 1: only fine_x=0 of every row is white.
        for fine_y in 0..8 {
            set_2bpp_tile_pixel(&mut ppu, 0, 1, 0, fine_y, 1); // col 0 = white
        }

        setup_bg1_mode0(&mut ppu);
        ppu.write_register(0x2106, 0x31); // size=3 → block_size=4; BG1 enabled

        render_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();

        // Block 0 (x=0..3): x_mos=0 → fine_x=0 → white
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "x=0 → leftmost pixel white"
        );
        assert_eq!(
            pixel(&rgb, 1, 0),
            [255, 255, 255],
            "x=1 → replicated from x_mos=0"
        );
        assert_eq!(
            pixel(&rgb, 2, 0),
            [255, 255, 255],
            "x=2 → replicated from x_mos=0"
        );
        assert_eq!(
            pixel(&rgb, 3, 0),
            [255, 255, 255],
            "x=3 → replicated from x_mos=0"
        );
        // Block 1 (x=4..7): x_mos=4 → fine_x=4 → black
        assert_eq!(
            pixel(&rgb, 4, 0),
            [0, 0, 0],
            "x=4 → new block, fine_x=4 is black"
        );
        assert_eq!(
            pixel(&rgb, 5, 0),
            [0, 0, 0],
            "x=5 → replicated from x_mos=4"
        );
        // Block 2 (x=8..11): x_mos=8 → fine_x=0 → white (new tile, col 0 again)
        assert_eq!(
            pixel(&rgb, 8, 0),
            [255, 255, 255],
            "x=8 → next-tile col 0, white"
        );
        assert_eq!(
            pixel(&rgb, 9, 0),
            [255, 255, 255],
            "x=9 → replicated from x_mos=8"
        );
    }

    #[test]
    fn vertical_mosaic_replicates_top_row_of_each_block() {
        // Tile 1: only row 1 (fine_y=1) is white; all other rows are black.
        // Framebuffer row 0 is display line 1, and with VOFS=0 it samples BG line 1. With mosaic
        // block_size=4: rows 0-3 (block index 0,1,2,3) all replicate the block's first sampled
        // line (BG line 1, white). Rows 4-7 replicate BG line 5 (black).
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);

        // Tilemap: all entries char 1.
        set_vram_word(&mut ppu, 0, 1);
        // Tile 1: only fine_y=1 is white for all columns.
        for fine_x in 0..8 {
            set_2bpp_tile_pixel(&mut ppu, 0, 1, fine_x, 1, 1); // row 1 = white
        }

        setup_bg1_mode0(&mut ppu);
        ppu.write_register(0x2106, 0x31); // size=3 → block_size=4; BG1 enabled

        render_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();

        // Rows 0-3: block index 0,1,2,3 all replicate the block's first line (BG line 1, white).
        assert_eq!(
            pixel(&rgb, 0, 0),
            [255, 255, 255],
            "row 0: block start, BG line 1, white"
        );
        assert_eq!(
            pixel(&rgb, 0, 1),
            [255, 255, 255],
            "row 1: block index 1 → BG line 1"
        );
        assert_eq!(
            pixel(&rgb, 0, 2),
            [255, 255, 255],
            "row 2: block index 2 → BG line 1"
        );
        assert_eq!(
            pixel(&rgb, 0, 3),
            [255, 255, 255],
            "row 3: block index 3 → BG line 1"
        );
        // Row 4: new block, replicates BG line 5 → black.
        assert_eq!(
            pixel(&rgb, 0, 4),
            [0, 0, 0],
            "row 4: new block, BG line 5, black"
        );
        assert_eq!(
            pixel(&rgb, 0, 5),
            [0, 0, 0],
            "row 5: block index 1 → BG line 5"
        );
    }

    #[test]
    fn no_mosaic_when_bg_bit_not_enabled() {
        // With mosaic size set but BG1 bit NOT enabled, rendering should be unaffected.
        let mut ppu = Ppu::new();
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 1, 0x7FFF);

        set_vram_word(&mut ppu, 0, 1);
        // Tile 1: only fine_x=0 column is white.
        for fine_y in 0..8 {
            set_2bpp_tile_pixel(&mut ppu, 0, 1, 0, fine_y, 1);
        }

        setup_bg1_mode0(&mut ppu);
        ppu.write_register(0x2106, 0x30); // size=3 but BG1 NOT enabled (bits 3-0 = 0)

        render_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();

        // Without mosaic, x=1 should be black (not replicated from x=0).
        assert_eq!(pixel(&rgb, 0, 0), [255, 255, 255], "x=0: fine_x=0 → white");
        assert_eq!(
            pixel(&rgb, 1, 0),
            [0, 0, 0],
            "x=1: fine_x=1 → black (no mosaic)"
        );
    }
}
