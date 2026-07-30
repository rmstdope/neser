//! OBJ (sprite) support: OBSEL decoding and the dot-incremental evaluation/fetch pipeline.
//!
//! Sprites shown on framebuffer row N are prepared one scanline early, mirroring hardware
//! (Mesen2 `SnesPpu.cpp`, ares `object.cpp`): during scanline N the evaluation window
//! (H=0..255) scans one OAM entry per 2 dots into an in-range list of at most 32, then the
//! fetch window (H=270..339) loads up to 34 8x1 tile slivers from that list in REVERSE order
//! (so when the time over-limit drops slivers, the front-most sprites lose theirs). At dot 0
//! of scanline N+1 the slivers are composited into the presented [`ObjLine`], which
//! [`Ppu::obj_pixel_at`] serves while row N renders. Mid-scanline OAM/OBSEL/OAMADD writes
//! therefore affect the next row, not the one being drawn.
//!
//! OBSEL ($2101) selects one of eight OBJ size pairs (including two undocumented pairs), the OBJ
//! tile name base (8K-word steps), and the name gap inserted between tiles $0FF and $100 (4K-word
//! steps). See fullsnes "SNES PPU Sprites (OBJs)".

use super::Ppu;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObjPixel {
    pub color: u16,
    pub palette: u8,
    pub priority: u8,
}

impl Ppu {
    /// Small-OBJ pixel size `(width, height)` selected by OBSEL bits 7-5.
    pub(super) fn obj_size_small(&self) -> (u8, u8) {
        obj_size_pair(self.obsel).0
    }

    /// Large-OBJ pixel size `(width, height)` selected by OBSEL bits 7-5.
    pub(super) fn obj_size_large(&self) -> (u8, u8) {
        obj_size_pair(self.obsel).1
    }

    /// OBJ tile name base as a VRAM word address (OBSEL bits 2-0, 8K-word steps).
    pub(super) fn obj_name_base_word(&self) -> u16 {
        ((self.obsel & 0x07) as u16) << 13
    }

    /// Extra gap (in VRAM words) added for OBJ tiles $100-$1FF (OBSEL bits 4-3, 4K-word steps).
    pub(super) fn obj_name_gap_word(&self) -> u16 {
        (((self.obsel >> 3) & 0x03) as u16) << 12
    }

    /// Whether OBJ priority rotation (OAMADDH $2103 bit 7) is enabled.
    #[cfg(test)]
    pub(super) fn obj_priority_rotation_enabled(&self) -> bool {
        self.oam_priority_rotation
    }

    /// The OBJ index (0-127) evaluated first: OBJ #N (OAMADD reload bits 7-1) when priority
    /// rotation is enabled, otherwise OBJ #0.
    pub(super) fn obj_first_sprite_index(&self) -> u8 {
        if self.oam_priority_rotation {
            ((self.oam_addr_reload >> 1) & 0x7F) as u8
        } else {
            0
        }
    }

    /// Whether OBJ `index` (0-127) uses the large size (its OAM high-table size bit is set).
    pub(super) fn obj_is_large(&self, index: usize) -> bool {
        let byte = self.oam[0x200 + (index >> 2)];
        let shift = (index & 3) * 2 + 1;
        (byte >> shift) & 1 != 0
    }

    /// Pixel size `(width, height)` of OBJ `index` per its OAM size bit and the OBSEL size pair.
    pub(super) fn obj_size(&self, index: usize) -> (u8, u8) {
        if self.obj_is_large(index) {
            self.obj_size_large()
        } else {
            self.obj_size_small()
        }
    }

    /// Sign-extended 9-bit X position of OBJ `index` (-256..=255).
    fn obj_base_x(&self, index: usize) -> i32 {
        let x9 = self.oam[index * 4] as u16 | (self.obj_x_high(index) << 8);
        if x9 & 0x100 != 0 {
            x9 as i32 - 0x200
        } else {
            x9 as i32
        }
    }

    /// The OAM high-table X bit 8 for OBJ `index`.
    fn obj_x_high(&self, index: usize) -> u16 {
        ((self.oam[0x200 + (index >> 2)] >> ((index & 3) * 2)) & 1) as u16
    }

    /// Whether OBJ `index` is in range for display line `line`: it must intersect the line
    /// vertically (8-bit Y wrap) and not be fully off-screen horizontally. Exception: raw X=256
    /// (sign-extended -256) is always in range -- it consumes range/time budget without being
    /// drawn (Mesen2 `SpriteInfo::IsVisible`, ares `PPU::Object::onScanline`).
    ///
    /// With OBJ interlace (SETINI $2133 bit 1, independent of screen interlace) the sprite keeps
    /// its OAM Y anchor but spans half its height on screen (Mesen2/ares: `height >> 1`).
    fn obj_in_range(&self, index: usize, line: u16) -> bool {
        let (width, height) = self.obj_size(index);
        let height = if self.obj_interlace_enabled() {
            height >> 1
        } else {
            height
        };
        let y = self.oam[index * 4 + 1] as u16;
        let row = line.wrapping_sub(y) & 0xFF;
        if row >= height as u16 {
            return false;
        }
        let base_x = self.obj_base_x(index);
        base_x == -256 || base_x + width as i32 > 0
    }

    /// Test helper: the full in-range OBJ list for source line `line` in one pass, mirroring the
    /// incremental eval window (rotation order, 32-entry truncation, X visibility).
    #[cfg(test)]
    pub(super) fn evaluate_line_objects(&self, line: u16) -> ObjLineEval {
        let first = self.obj_first_sprite_index() as usize;
        let mut eval = ObjLineEval::default();
        for k in 0..128usize {
            let i = (first + k) % 128;
            if !self.obj_in_range(i, line) {
                continue;
            }
            if eval.len < 32 {
                eval.indices[eval.len] = i as u8;
                eval.len += 1;
            } else {
                eval.range_over = true;
                eval.range_over_index = Some(i as u8);
                break;
            }
        }
        eval
    }

    /// The front-most OBJ pixel at visible coordinate `(x, y)` from the presented line buffer.
    ///
    /// Returns `None` unless row `y` is the line currently presented by the OBJ pipeline (rows
    /// are fetched during the previous scanline and presented at the dot-0 buffer swap). The
    /// CGRAM color is resolved at query time so mid-scanline palette writes stay live.
    pub(super) fn obj_pixel_at(&self, x: u16, y: u16) -> Option<ObjPixel> {
        let pipeline = &self.obj_pipeline;
        if pipeline.presented_row != Some(y) {
            return None;
        }
        let x = x as usize;
        if x >= super::SCREEN_WIDTH || !pipeline.line.present[x] {
            return None;
        }
        Some(ObjPixel {
            color: self.cgram_color(pipeline.line.cgram_index[x]),
            palette: pipeline.line.palette[x],
            priority: pipeline.line.priority[x],
        })
    }

    /// Advance the dot-incremental OBJ pipeline (hardware model, per Mesen2 `SnesPpu.cpp` and
    /// ares `object.cpp`):
    ///
    /// - Dot 0: present the line fetched during the previous scanline (buffer swap + composite),
    ///   clear the STAT77 over-limit flags at end of VBlank (scanline 0, not during forced
    ///   blank), and start a new evaluation window.
    /// - Dots 0..255 of an active scanline: evaluate one OAM entry per 2 dots for the line shown
    ///   on the next scanline. The 33rd in-range OBJ raises range over (bit 6) at
    ///   `H = OAM_index x 2` (fullsnes).
    /// - Dots 270..339: fetch one 8x1 tile sliver per 2 dots from the in-range list in REVERSE
    ///   order (34 CHR slots; the 35th attempted fetch raises time over, bit 7).
    pub(super) fn update_obj_pipeline(&mut self, forced_blank: bool) {
        let scanline = self.position.scanline;
        let dot = self.position.dot;
        let active_height = self.active_screen_height() as u16;

        if dot == 0 {
            // Present the line fetched during the previous scanline as row `scanline - 1`.
            if (1..=active_height).contains(&scanline) {
                self.composite_obj_line(scanline - 1);
            } else {
                self.obj_pipeline.presented_row = None;
            }
            if scanline == 0 && !forced_blank {
                self.stat77_range_over = false;
                self.stat77_time_over = false;
            }
            if scanline < active_height {
                let first_sprite = self.obj_first_sprite_index();
                self.obj_pipeline.begin_eval(first_sprite);
            }
        }

        if scanline >= active_height {
            return;
        }

        // Evaluation window: one OAM entry per 2 dots during H=0..255.
        if dot < 256 && dot.is_multiple_of(2) {
            self.obj_eval_step(forced_blank);
        }
        // Raise the scheduled range over-limit flag at H = OAM_index x 2 (or as soon as the
        // schedule is reached when priority rotation put the index earlier than its scan slot).
        if !forced_blank && self.obj_pipeline.range_over_dot.is_some_and(|d| dot >= d) {
            self.stat77_range_over = true;
            self.obj_pipeline.range_over_dot = None;
        }
        // Sliver fetch window: one 2-dot slot per sliver.
        if dot == OBJ_FETCH_START_DOT {
            self.obj_pipeline.begin_fetch();
        }
        if (OBJ_FETCH_START_DOT..OBJ_FETCH_START_DOT + 2 * OBJ_FETCH_SLOTS).contains(&dot)
            && (dot - OBJ_FETCH_START_DOT).is_multiple_of(2)
        {
            self.obj_fetch_step(forced_blank);
        }
    }

    /// Evaluate the next OAM entry of the current window (2-dot cadence).
    ///
    /// Forced blank PAUSES the evaluation cursor: OAM scanning stops and the pending entries
    /// are deferred to the dots after blank releases (entries past the window end are lost).
    /// Mesen2 models the same cursor hold (`_oamEvaluationIndex` only advances on processed
    /// entries); ares skips blanked entries instead -- the paused model was chosen because it
    /// preserves the approved `inidisp_enable_display_mid_frame` behavior (see #2999).
    fn obj_eval_step(&mut self, forced_blank: bool) {
        if forced_blank {
            return;
        }
        let cursor = self.obj_pipeline.eval_cursor;
        if cursor >= 128 {
            return;
        }
        self.obj_pipeline.eval_cursor += 1;
        let index = (self.obj_pipeline.first_sprite as usize + cursor as usize) & 0x7F;
        if !self.obj_in_range(index, self.position.scanline) {
            return;
        }
        let pipeline = &mut self.obj_pipeline;
        if (pipeline.item_count as usize) < pipeline.items.len() {
            pipeline.items[pipeline.item_count as usize] = index as u8;
            pipeline.item_count += 1;
        } else if pipeline.range_over_dot.is_none() {
            pipeline.range_over_dot = Some(index as u16 * 2);
        }
    }

    /// Process one sliver-fetch slot: re-read the current sprite's position (so mid-window OAM
    /// writes affect the remaining slivers, like hardware), consume one of the 34 CHR slots, and
    /// record the decoded sliver. The 35th attempted fetch only raises the time over-limit.
    ///
    /// The live re-read is deliberate and matches both references (#3026): evaluation stores
    /// only the OAM INDEX, exactly as Mesen2's `_spriteIndexes[32]` and ares-accurate's
    /// `{valid, index}` do, and neither re-checks range at fetch either. So a write landing in
    /// the H=256..269 gap between the windows -- SETINI, the high-table size bit, or OAM Y
    /// behind a forced-blank round trip -- changes what this fetch samples, and can take the
    /// line-within-sprite outside the sprite's own extent. That is modelled, not a defect; see
    /// the `*_between_the_windows_*` tests.
    ///
    /// The 8-bit mask on `within_y` below is likewise not a divergence from Mesen2's signed
    /// unmasked form: the two differ by a multiple of 256, and the OBJ tile lookup cannot see
    /// that -- the fine row is `& 7` and 256 is a multiple of 8, while the tile row is `>> 3`
    /// then `& 0x0F` and 256/8 = 32 is a multiple of 16. `masked_and_signed_source_lines_agree`
    /// pins that equivalence so it fails loudly if any of those three is changed.
    fn obj_fetch_step(&mut self, forced_blank: bool) {
        if self.obj_pipeline.fetch_remaining == 0 {
            return;
        }
        let sprite = self.obj_pipeline.items[self.obj_pipeline.fetch_remaining as usize - 1];
        let index = sprite as usize;
        // Only the width matters here: V-flip mirrors within width-sized blocks, and the
        // height merely bounds `within_y`, which `obj_in_range` already guaranteed.
        let width = self.obj_size(index).0 as i32;
        let base_x = self.obj_base_x(index);
        let col_count = width / 8;

        // "Position fetch": recompute the column cursor when the sprite changes, skipping tiles
        // fully hidden to the left of the screen (not for X=-256, which fetches its full width).
        if self.obj_pipeline.fetch_current != Some(sprite) {
            self.obj_pipeline.fetch_current = Some(sprite);
            let mut offset = col_count;
            if base_x <= -8 && base_x != -256 {
                offset += base_x / 8;
            }
            self.obj_pipeline.fetch_column_offset = offset;
        }

        // "Attribute fetch": consumes one fetch slot; only 34 get their CHR data. Unlike the
        // range flag (gated on !forced_blank in the eval window), this deliberately raises
        // even under forced blank: Mesen2 keeps attribute fetches (and the flag) running
        // while only the CHR read is suppressed, and a fully blanked line produces no items
        // to fetch anyway.
        self.obj_pipeline.tile_count += 1;
        if self.obj_pipeline.tile_count > 34 {
            self.stat77_time_over = true;
            return;
        }

        self.obj_pipeline.fetch_column_offset -= 1;
        let column_offset = self.obj_pipeline.fetch_column_offset;
        let col_index = col_count - column_offset - 1;

        let attr = self.oam[index * 4 + 3];
        let priority = (attr >> 4) & 0x03;
        let palette = (attr >> 1) & 0x07;
        let hflip = attr & 0x40 != 0;
        let vflip = attr & 0x80 != 0;
        let base_tile = self.oam[index * 4 + 2] as u16 | (((attr & 0x01) as u16) << 8);
        let y = self.oam[index * 4 + 1] as u16;
        // The line-within-sprite is taken in 8-bit space, so a sprite near Y=255 wraps to
        // the top of the screen (ares `object.cpp` and Snes9x mask identically). Keeping the
        // mask also keeps `obj_tile_pixel`'s truncating `within_y / 8` valid, since it can
        // then only be reached with a non-negative value.
        let mut within_y = (self.position.scanline.wrapping_sub(y) & 0xFF) as i32;
        // OBJ interlace: each field shows alternate source lines of the half-height sprite --
        // double the line-within-sprite and OR in the field, then V-flip mirrors the doubled
        // coordinate within its width block (Mesen2 `FetchSpriteAttributes`, ares equivalent).
        // Interlace combined with a RECTANGULAR size is not hardware-verified (deferred to
        // #3000); the mirror below is well-defined there and stays in range regardless.
        if self.obj_interlace_enabled() {
            within_y = (within_y << 1) | self.interlace_field as i32;
        }
        if vflip {
            // Rectangular OBJs (the OBSEL 6/7 sizes 16x32 and 32x64) flip as two stacked
            // squares -- each width-sized block mirrors in place and the blocks do NOT swap:
            // "rows 01234567 flip to 32107654, not 76543210" (SNESdev wiki OAM page). Every
            // OBJ width is a power of two and `within_y < height <= 2 * width`, so that is a
            // plain XOR, identical to `height - 1 - within_y` for the six square sizes
            // (Snes9x `line ^ (OBJWidths[S] - 1)`, ares accurate/performance and higan
            // `object.cpp`). NESER mirrored against the height, which was wrong for the
            // rectangular sizes and produced the wrong rows once Y wrapped (#3003).
            //
            // Deliberate Mesen2 divergence: Mesen2 computes the row as an unmasked SIGNED
            // difference, so for a wrapped sprite its branch -- the one its own comment
            // labels "Square sprites" -- is taken for a rectangular one, selecting tile rows
            // 15/14 where the four implementations above select 3/2. NESER follows them.
            within_y ^= width - 1;
        }

        // Under forced blank the slot is consumed but VRAM is not read: nothing is drawn.
        let mut colors = [0u8; 8];
        if !forced_blank {
            for (sub, color) in colors.iter_mut().enumerate() {
                let col = col_index * 8 + sub as i32;
                let within_x = if hflip { width - 1 - col } else { col };
                *color = self.obj_tile_pixel(base_tile, within_x, within_y);
            }
        }

        // X=256 uses X=0 for the off-screen-right cutoff only; DrawX stays at the real
        // (invisible) position (Mesen2 `FetchSpriteAttributes`).
        let x_effective = if base_x == -256 { 0 } else { base_x };
        let end_tile_x = x_effective + col_index * 8 + 8;

        let pipeline = &mut self.obj_pipeline;
        pipeline.slivers[pipeline.sliver_count as usize] = ObjSliver {
            x: (base_x + col_index * 8) as i16,
            colors,
            palette,
            priority,
        };
        pipeline.sliver_count += 1;

        if column_offset == 0 || end_tile_x >= 256 {
            // Last tile of the sprite, or the rest is hidden to the right of the screen.
            pipeline.fetch_remaining -= 1;
            pipeline.fetch_column_offset = 0;
        }
    }

    /// Composite the fetched slivers into the presented line buffer for row `row`. Slivers were
    /// fetched in reverse evaluation order, so later slivers (front-most OBJs) overwrite earlier
    /// ones per opaque pixel (ares `PPU::Object::run`).
    fn composite_obj_line(&mut self, row: u16) {
        let pipeline = &mut self.obj_pipeline;
        pipeline.line = ObjLine::default();
        for sliver in &pipeline.slivers[..pipeline.sliver_count as usize] {
            for (sub, &color) in sliver.colors.iter().enumerate() {
                if color == 0 {
                    continue;
                }
                let x = sliver.x as i32 + sub as i32;
                if !(0..super::SCREEN_WIDTH as i32).contains(&x) {
                    continue;
                }
                let x = x as usize;
                pipeline.line.cgram_index[x] = 128 + sliver.palette * 16 + color;
                pipeline.line.palette[x] = sliver.palette;
                pipeline.line.priority[x] = sliver.priority;
                pipeline.line.present[x] = true;
            }
        }
        pipeline.presented_row = Some(row);
    }

    /// Decode an OBJ pixel color index (0-15) at sprite-relative `(within_x, within_y)`, applying
    /// non-carrying large-tile composition (right wraps the low nibble, down wraps the high nibble)
    /// and the OBSEL name base/gap addressing.
    fn obj_tile_pixel(&self, base_tile: u16, within_x: i32, within_y: i32) -> u8 {
        // Both coordinates are non-negative by construction (the 8-bit row mask and the
        // H-flip mirror keep them so), which is what makes the truncating `/ 8` below agree
        // with the flooring `& 7`.
        debug_assert!(
            within_x >= 0 && within_y >= 0,
            "OBJ tile coords must be non-negative, got ({within_x}, {within_y})"
        );
        let tile_col = (within_x / 8) as u16;
        let tile_row = (within_y / 8) as u16;
        let page = base_tile & 0x100;
        let lo = ((base_tile & 0x0F) + tile_col) & 0x0F;
        let hi = (((base_tile >> 4) & 0x0F) + tile_row) & 0x0F;
        let tile = page | (hi << 4) | lo;

        let mut word_addr = self.obj_name_base_word().wrapping_add(tile << 4);
        if tile & 0x100 != 0 {
            word_addr = word_addr.wrapping_add(self.obj_name_gap_word());
        }
        let fine_x = (within_x & 7) as u8;
        let fine_y = (within_y & 7) as u8;
        self.decode_tile_pixel(word_addr, 0, 4, fine_x, fine_y)
    }
}

/// First dot of the OBJ sliver-fetch window (Mesen2 fetches sprite CHR at H=270..339).
const OBJ_FETCH_START_DOT: u16 = 270;
/// Number of 2-dot fetch slots in the window: 35 attribute fetches, of which 34 get CHR data.
const OBJ_FETCH_SLOTS: u16 = 35;

/// Result of per-scanline OAM range evaluation (test helper mirroring the eval window).
#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ObjLineEval {
    /// In-range OBJ indices in evaluation order, truncated to 32.
    pub indices: [u8; 32],
    /// Number of valid entries in `indices`.
    pub len: usize,
    /// Whether more than 32 OBJs were in range (range over-limit).
    pub range_over: bool,
    /// OAM index of the 33rd in-range OBJ that triggered the range over-limit, if any.
    pub range_over_index: Option<u8>,
}

#[cfg(test)]
impl ObjLineEval {
    pub(super) fn indices(&self) -> &[u8] {
        &self.indices[..self.len]
    }
}

/// One fetched 8x1 OBJ tile sliver: the unit of the 34-per-line time over-limit budget.
#[derive(Debug, Clone, Copy, Default)]
struct ObjSliver {
    /// Leftmost screen X of the sliver (may be off-screen, e.g. for X=256 OBJs).
    x: i16,
    /// Decoded color indices (0-15) in screen order (H-flip already applied).
    colors: [u8; 8],
    /// OBJ palette number (0-7).
    palette: u8,
    /// OBJ priority level (0-3, OAM attr bits 5-4).
    priority: u8,
}

/// Composited OBJ pixels for one presented scanline (256 visible pixels).
#[derive(Debug, Clone)]
pub(super) struct ObjLine {
    /// CGRAM color index per pixel (valid only where `present`). The color itself is resolved
    /// at query time so mid-scanline CGRAM writes stay live.
    cgram_index: [u8; super::SCREEN_WIDTH],
    /// OBJ palette number (0-7) per pixel, for color math gating.
    palette: [u8; super::SCREEN_WIDTH],
    /// OBJ priority level (0-3, OAM attr bits 5-4) per pixel, for BG compositing.
    priority: [u8; super::SCREEN_WIDTH],
    /// Whether an opaque OBJ pixel was written at this x.
    present: [bool; super::SCREEN_WIDTH],
}

impl Default for ObjLine {
    fn default() -> Self {
        Self {
            cgram_index: [0; super::SCREEN_WIDTH],
            palette: [0; super::SCREEN_WIDTH],
            priority: [0; super::SCREEN_WIDTH],
            present: [false; super::SCREEN_WIDTH],
        }
    }
}

/// Dot-incremental OBJ evaluation/fetch pipeline state.
///
/// `line` presents the previous scanline's fetch results while the current scanline's
/// eval/fetch windows prepare `items`/`slivers` for the next. Only `line` is double-buffered;
/// the ordering within [`Ppu::update_obj_pipeline`] is load-bearing: the dot-0 composite must
/// consume `slivers` before `begin_fetch` (dot 270) resets them for the new line.
#[derive(Debug, Clone)]
pub(super) struct ObjPipeline {
    /// In-range OAM indices (at most 32) in evaluation order for the line being prepared.
    items: [u8; 32],
    /// Number of valid entries in `items`.
    item_count: u8,
    /// Next OAM scan offset (0..=128) within the evaluation window.
    eval_cursor: u8,
    /// First-OBJ index (priority rotation) latched at the start of the evaluation window.
    first_sprite: u8,
    /// Scheduled dot for raising STAT77 range over (33rd in-range OBJ's OAM index x 2).
    range_over_dot: Option<u16>,
    /// Items left to fetch (the list is walked in reverse: `items[fetch_remaining - 1]`).
    fetch_remaining: u8,
    /// OAM index whose columns are currently being fetched.
    fetch_current: Option<u8>,
    /// Remaining tile columns of the current sprite (Mesen2 `ColumnOffset`).
    fetch_column_offset: i32,
    /// Attribute fetches consumed this line (the 35th raises time over).
    tile_count: u8,
    /// Slivers fetched for the line being prepared.
    slivers: [ObjSliver; 34],
    /// Number of valid entries in `slivers`.
    sliver_count: u8,
    /// Composited OBJ pixels for `presented_row`.
    line: ObjLine,
    /// Framebuffer row that `line` corresponds to, if any.
    presented_row: Option<u16>,
}

impl Default for ObjPipeline {
    fn default() -> Self {
        Self {
            items: [0; 32],
            item_count: 0,
            eval_cursor: 0,
            first_sprite: 0,
            range_over_dot: None,
            fetch_remaining: 0,
            fetch_current: None,
            fetch_column_offset: 0,
            tile_count: 0,
            slivers: [ObjSliver::default(); 34],
            sliver_count: 0,
            line: ObjLine::default(),
            presented_row: None,
        }
    }
}

impl ObjPipeline {
    /// Start a new evaluation window (dot 0 of an active scanline).
    fn begin_eval(&mut self, first_sprite: u8) {
        self.item_count = 0;
        self.eval_cursor = 0;
        self.first_sprite = first_sprite;
        self.range_over_dot = None;
    }

    /// Start the sliver-fetch window (the in-range list is fetched last-to-first).
    fn begin_fetch(&mut self) {
        self.fetch_remaining = self.item_count;
        self.fetch_current = None;
        self.fetch_column_offset = 0;
        self.tile_count = 0;
        self.sliver_count = 0;
    }
}

/// Decode the OBSEL size selection (bits 7-5) into the `(small, large)` `(width, height)` pixel
/// sizes, including the two undocumented pairs 6 and 7 (fullsnes OBSEL table).
fn obj_size_pair(obsel: u8) -> ((u8, u8), (u8, u8)) {
    match obsel >> 5 {
        0 => ((8, 8), (16, 16)),
        1 => ((8, 8), (32, 32)),
        2 => ((8, 8), (64, 64)),
        3 => ((16, 16), (32, 32)),
        4 => ((16, 16), (64, 64)),
        5 => ((32, 32), (64, 64)),
        6 => ((16, 32), (32, 64)),
        _ => ((16, 32), (32, 32)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Ppu;

    fn ppu_with_obsel(obsel: u8) -> Ppu {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, obsel);
        ppu
    }

    #[test]
    fn obsel_decodes_all_eight_size_pairs() {
        // (small, large) per fullsnes OBSEL table, including undocumented pairs 6 and 7.
        let expected = [
            ((8, 8), (16, 16)),
            ((8, 8), (32, 32)),
            ((8, 8), (64, 64)),
            ((16, 16), (32, 32)),
            ((16, 16), (64, 64)),
            ((32, 32), (64, 64)),
            ((16, 32), (32, 64)),
            ((16, 32), (32, 32)),
        ];
        for (sel, &(small, large)) in expected.iter().enumerate() {
            let ppu = ppu_with_obsel((sel as u8) << 5);
            assert_eq!(ppu.obj_size_small(), small, "small size for select {sel}");
            assert_eq!(ppu.obj_size_large(), large, "large size for select {sel}");
        }
    }

    #[test]
    fn obsel_decodes_name_base_in_8k_word_steps() {
        for base in 0u16..8 {
            let ppu = ppu_with_obsel(base as u8);
            assert_eq!(ppu.obj_name_base_word(), base << 13);
        }
    }

    #[test]
    fn obsel_decodes_name_gap_in_4k_word_steps() {
        for gap in 0u16..4 {
            let ppu = ppu_with_obsel((gap as u8) << 3);
            assert_eq!(ppu.obj_name_gap_word(), gap << 12);
        }
    }

    #[test]
    fn priority_rotation_is_disabled_at_power_on() {
        let ppu = Ppu::new();
        assert!(!ppu.obj_priority_rotation_enabled());
        assert_eq!(ppu.obj_first_sprite_index(), 0);
    }

    #[test]
    fn oamaddh_bit7_enables_priority_rotation() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2103, 0x80);
        assert!(ppu.obj_priority_rotation_enabled());

        ppu.write_register(0x2103, 0x00);
        assert!(!ppu.obj_priority_rotation_enabled());
    }

    #[test]
    fn first_sprite_index_is_reload_bits_7_1_when_rotation_enabled() {
        let mut ppu = Ppu::new();
        // OAMADDL = 0x14 (word reload 0x14 -> OBJ #0x0A from bits 7-1), rotation on.
        ppu.write_register(0x2102, 0x14);
        ppu.write_register(0x2103, 0x80);
        assert_eq!(ppu.obj_first_sprite_index(), 0x0A);
    }

    #[test]
    fn first_sprite_index_is_zero_when_rotation_disabled() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2102, 0x14);
        ppu.write_register(0x2103, 0x00);
        assert_eq!(ppu.obj_first_sprite_index(), 0);
    }

    /// Set OBJ `i` in OAM: low-table X/Y/tile/attr plus the high-table X-bit8 and size bit.
    fn set_obj(ppu: &mut Ppu, i: usize, x: u16, y: u8, tile: u8, attr: u8, large: bool) {
        ppu.set_oam_byte(i * 4, (x & 0xFF) as u8);
        ppu.set_oam_byte(i * 4 + 1, y);
        ppu.set_oam_byte(i * 4 + 2, tile);
        ppu.set_oam_byte(i * 4 + 3, attr);
        let hi_index = 0x200 + (i >> 2);
        let shift = (i & 3) * 2;
        let mut byte = ppu.oam_byte(hi_index);
        let bits = (((x >> 8) & 1) as u8) | ((large as u8) << 1);
        byte &= !(0b11 << shift);
        byte |= bits << shift;
        ppu.set_oam_byte(hi_index, byte);
    }

    #[test]
    fn empty_oam_yields_no_in_range_objects() {
        let mut ppu = Ppu::new();
        // OAM all zero -> every OBJ has Y=0, size 8x8 (obsel 0): in range only for lines 0..7.
        ppu.write_register(0x2101, 0x00);
        let eval = ppu.evaluate_line_objects(100);
        assert!(eval.indices().is_empty());
        assert!(!eval.range_over);
    }

    #[test]
    fn small_object_is_in_range_only_within_its_height() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // small = 8x8
        // Park all other OBJs off-screen (Y=240, height 8 -> lines 240..247, not line 10).
        for i in 1..128 {
            set_obj(&mut ppu, i, 0, 240, 0, 0, false);
        }
        set_obj(&mut ppu, 0, 0, 10, 0, 0, false);

        assert!(ppu.evaluate_line_objects(9).indices().is_empty());
        assert_eq!(ppu.evaluate_line_objects(10).indices(), [0]);
        assert_eq!(ppu.evaluate_line_objects(17).indices(), [0]);
        assert!(ppu.evaluate_line_objects(18).indices().is_empty());
    }

    #[test]
    fn large_object_height_comes_from_obsel_large_pair() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // small 8x8, large 16x16
        for i in 1..128 {
            set_obj(&mut ppu, i, 0, 240, 0, 0, false);
        }
        set_obj(&mut ppu, 0, 0, 10, 0, 0, true); // large -> height 16
        assert_eq!(ppu.evaluate_line_objects(25).indices(), [0]); // 10..25 in range
        assert!(ppu.evaluate_line_objects(26).indices().is_empty());
    }

    #[test]
    fn object_y_wraps_in_8_bit_space_for_224_line_mode() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        for i in 1..128 {
            set_obj(&mut ppu, i, 0, 100, 0, 0, false);
        }
        set_obj(&mut ppu, 0, 0, 250, 0, 0, false); // covers 250..255, 0..1 (wrap)
        assert_eq!(ppu.evaluate_line_objects(1).indices(), [0]);
        assert!(ppu.evaluate_line_objects(2).indices().is_empty());
    }

    #[test]
    fn more_than_32_in_range_sets_range_over_and_truncates() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        // 40 OBJs all covering line 50; the rest parked off-screen.
        for i in 0..128 {
            let y = if i < 40 { 50 } else { 240 };
            set_obj(&mut ppu, i, 0, y, 0, 0, false);
        }
        let eval = ppu.evaluate_line_objects(50);
        assert_eq!(eval.indices().len(), 32);
        assert!(eval.range_over);
        assert_eq!(eval.range_over_index, Some(32)); // 33rd in-range OBJ (index 32)
        assert_eq!(eval.indices()[0], 0);
    }

    #[test]
    fn priority_rotation_starts_evaluation_at_first_sprite() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        for i in 0..128 {
            set_obj(&mut ppu, i, 0, 240, 0, 0, false);
        }
        // Three in-range OBJs: 5, 6, 7.
        for i in 5..=7 {
            set_obj(&mut ppu, i, 0, 50, 0, 0, false);
        }
        // Rotation starting at OBJ #6: order becomes 6, 7, then wrap to 5.
        ppu.write_register(0x2102, 6 << 1);
        ppu.write_register(0x2103, 0x80);
        assert_eq!(ppu.evaluate_line_objects(50).indices(), [6, 7, 5]);
    }

    /// Fill an 8x8 4bpp OBJ tile (16 words at VRAM word `word_addr`) with a solid color index.
    fn set_obj_tile_solid(ppu: &mut Ppu, word_addr: u16, color: u8) {
        let base = (word_addr as usize) << 1;
        for row in 0..8usize {
            let r = base + row * 2;
            ppu.set_vram_byte(r, if color & 1 != 0 { 0xFF } else { 0x00 });
            ppu.set_vram_byte(r + 1, if color & 2 != 0 { 0xFF } else { 0x00 });
            ppu.set_vram_byte(r + 16, if color & 4 != 0 { 0xFF } else { 0x00 });
            ppu.set_vram_byte(r + 17, if color & 8 != 0 { 0xFF } else { 0x00 });
        }
    }

    /// Set one pixel (fine_x, fine_y) of an 8x8 4bpp OBJ tile to a color index.
    fn set_obj_tile_pixel(ppu: &mut Ppu, word_addr: u16, fx: usize, fy: usize, color: u8) {
        let base = ((word_addr as usize) << 1) + fy * 2;
        let bit = 7 - fx;
        for (plane, off) in [(0usize, 0usize), (1, 1), (2, 16), (3, 17)] {
            let mut b = ppu.vram_byte(base + off);
            b &= !(1 << bit);
            if color & (1 << plane) != 0 {
                b |= 1 << bit;
            }
            ppu.set_vram_byte(base + off, b);
        }
    }

    fn set_cgram(ppu: &mut Ppu, index: u8, bgr: u16) {
        ppu.write_register(0x2121, index);
        ppu.write_register(0x2122, (bgr & 0xFF) as u8);
        ppu.write_register(0x2122, (bgr >> 8) as u8);
    }

    fn park_all_offscreen(ppu: &mut Ppu) {
        for i in 0..128 {
            set_obj(ppu, i, 0, 240, 0, 0, false);
        }
    }

    /// Park all OBJs at Y=224 (safe for sizes up to 32 px tall: covers lines 224..255, never wraps
    /// into the visible 0..223 region).
    fn park_offscreen_32(ppu: &mut Ppu) {
        for i in 0..128 {
            set_obj(ppu, i, 0, 224, 0, 0, false);
        }
    }

    /// Resolved OBJ pixel color at `(x, line)` from the presented pipeline line, if opaque.
    fn obj_color_at(ppu: &Ppu, x: u16, line: u16) -> Option<u16> {
        ppu.obj_pixel_at(x, line).map(|p| p.color)
    }

    /// Label every source line of a `tile_rows`-tall OBJ at base tile 0 so a test can
    /// identify which one landed on a given screen row.
    ///
    /// Source line `s` lights exactly one pixel, at x offset `s & 7` of its tile row's
    /// LEFT tile, using color index `(s / 8) + 1` -- a diagonal per tile. CGRAM maps that
    /// index to `0x1000 | tile_row`, so a read-back of (color, x offset) recovers
    /// `s = tile_row * 8 + x_offset`.
    fn mark_obj_source_lines(ppu: &mut Ppu, tile_rows: u16) {
        for row in 0..tile_rows {
            let color = (row + 1) as u8;
            for fine in 0..8usize {
                // Tile `row * 0x10` (hi nibble = tile row) lives at word address row * 0x100.
                set_obj_tile_pixel(ppu, row * 0x100, fine, fine, color);
            }
            set_cgram(ppu, 128 + color, 0x1000 | row);
        }
    }

    /// The OBJ source line presented on framebuffer `row`, read back from the marker set up
    /// by [`mark_obj_source_lines`]. `x_base` is the sprite's left edge.
    fn marked_source_line(ppu: &mut Ppu, x_base: u16, row: u16) -> Option<u16> {
        present_line(ppu, row);
        read_marked_source_line(ppu, x_base, row)
    }

    /// The read-back half of [`marked_source_line`], for tests that must tick the PPU
    /// themselves (e.g. to land a register write at a specific dot).
    fn read_marked_source_line(ppu: &Ppu, x_base: u16, row: u16) -> Option<u16> {
        (0..8)
            .find_map(|dx| obj_color_at(ppu, x_base + dx, row).map(|color| (color & 0x0F) * 8 + dx))
    }

    #[test]
    fn renders_an_8x8_object_at_its_x_position() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8, name base 0
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 3); // tile 0 solid color index 3
        set_cgram(&mut ppu, 128 + 3, 0x1234); // OBJ palette 0, color 3
        set_obj(&mut ppu, 0, 50, 20, 0, 0, false);

        present_line(&mut ppu, 20);
        for x in 50..58 {
            assert_eq!(obj_color_at(&ppu, x, 20), Some(0x1234), "pixel {x} opaque");
        }
        assert_eq!(obj_color_at(&ppu, 49, 20), None);
        assert_eq!(obj_color_at(&ppu, 58, 20), None);
    }

    #[test]
    fn transparent_color_zero_is_not_written() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        park_all_offscreen(&mut ppu);
        // Tile is all color 0 (transparent) except pixel (2,0)=color 5.
        set_obj_tile_pixel(&mut ppu, 0, 2, 0, 5);
        set_cgram(&mut ppu, 128 + 5, 0x7FFF);
        set_obj(&mut ppu, 0, 10, 0, 0, 0, false);

        present_line(&mut ppu, 0);
        assert_eq!(
            obj_color_at(&ppu, 12, 0),
            Some(0x7FFF),
            "only the opaque pixel is written"
        );
        assert_eq!(obj_color_at(&ppu, 10, 0), None);
        assert_eq!(obj_color_at(&ppu, 11, 0), None);
    }

    #[test]
    fn x_and_y_flip_mirror_the_object() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        park_all_offscreen(&mut ppu);
        set_obj_tile_pixel(&mut ppu, 0, 0, 0, 6); // top-left pixel
        set_cgram(&mut ppu, 128 + 6, 0x0AAA);

        // X-flip: top-left pixel moves to the right edge (x+7).
        set_obj(&mut ppu, 0, 100, 0, 0, 0x40, false);
        present_line(&mut ppu, 0);
        assert!(ppu.obj_pixel_at(107, 0).is_some());
        assert!(ppu.obj_pixel_at(100, 0).is_none());

        // Y-flip: top-left pixel moves to the bottom row (y+7).
        set_obj(&mut ppu, 0, 100, 0, 0, 0x80, false);
        present_line(&mut ppu, 7);
        assert!(ppu.obj_pixel_at(100, 7).is_some());
        present_line(&mut ppu, 0); // wraps into the next frame
        assert!(ppu.obj_pixel_at(100, 0).is_none());
    }

    #[test]
    fn obj_interlace_uses_alternating_source_lines_per_field() {
        let line_color_for_field = |field: bool| {
            let mut ppu = Ppu::new();
            ppu.write_register(0x2101, 0x00); // 8x8
            ppu.write_register(0x2133, 0x03); // interlace + OBJ interlace
            park_all_offscreen(&mut ppu);
            set_obj_tile_solid(&mut ppu, 0, 0);
            set_obj_tile_pixel(&mut ppu, 0, 0, 0, 1); // y=0 pixel
            set_obj_tile_pixel(&mut ppu, 0, 0, 1, 2); // y=1 pixel
            set_cgram(&mut ppu, 128 + 1, 0x001F); // red
            set_cgram(&mut ppu, 128 + 2, 0x03E0); // green
            set_obj(&mut ppu, 0, 10, 0, 0, 0, false);
            ppu.interlace_field = field;
            present_line(&mut ppu, 0);
            obj_color_at(&ppu, 10, 0)
        };

        // Field 0 samples source line 0 for display line 0; field 1 samples source line 1.
        assert_eq!(line_color_for_field(false), Some(0x001F));
        assert_eq!(line_color_for_field(true), Some(0x03E0));
    }

    #[test]
    fn obj_interlace_halves_height_anchored_at_oam_y() {
        // SETINI bit 1 alone (no screen interlace): the sprite stays anchored at OAM Y with
        // halved on-screen height (Mesen2 `SpriteInfo::IsVisible`, ares `onScanline`).
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        ppu.write_register(0x2133, 0x02); // OBJ interlace only
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 3);
        set_cgram(&mut ppu, 128 + 3, 0x1234);
        set_obj(&mut ppu, 0, 50, 40, 0, 0, false);

        present_line(&mut ppu, 20);
        assert_eq!(obj_color_at(&ppu, 50, 20), None, "not anchored at Y/2");
        present_line(&mut ppu, 39);
        assert_eq!(obj_color_at(&ppu, 50, 39), None, "row above OAM Y");
        present_line(&mut ppu, 40);
        assert_eq!(
            obj_color_at(&ppu, 50, 40),
            Some(0x1234),
            "anchored at OAM Y"
        );
        present_line(&mut ppu, 43);
        assert_eq!(
            obj_color_at(&ppu, 50, 43),
            Some(0x1234),
            "last halved-height row"
        );
        present_line(&mut ppu, 44);
        assert_eq!(
            obj_color_at(&ppu, 50, 44),
            None,
            "halved height: 4 rows for 8x8"
        );
    }

    #[test]
    fn obj_interlace_samples_doubled_source_line_plus_field() {
        // Presented row `r` fetches sprite source line `(r - Y) * 2 + field`
        // (Mesen2 `FetchSpriteAttributes`, ares `Object::fetch`).
        let color_at_row = |row: u16, field: bool| {
            let mut ppu = Ppu::new();
            ppu.write_register(0x2101, 0x00); // 8x8
            ppu.write_register(0x2133, 0x02); // OBJ interlace only
            park_all_offscreen(&mut ppu);
            for fy in 0..4usize {
                set_obj_tile_pixel(&mut ppu, 0, 0, fy, fy as u8 + 1);
                set_cgram(&mut ppu, 128 + fy as u8 + 1, 0x1000 + fy as u16);
            }
            set_obj(&mut ppu, 0, 50, 40, 0, 0, false);
            ppu.interlace_field = field;
            present_line(&mut ppu, row);
            obj_color_at(&ppu, 50, row)
        };

        assert_eq!(
            color_at_row(40, false),
            Some(0x1000),
            "row 40 field 0 -> line 0"
        );
        assert_eq!(
            color_at_row(40, true),
            Some(0x1001),
            "row 40 field 1 -> line 1"
        );
        assert_eq!(
            color_at_row(41, false),
            Some(0x1002),
            "row 41 field 0 -> line 2"
        );
        assert_eq!(
            color_at_row(41, true),
            Some(0x1003),
            "row 41 field 1 -> line 3"
        );
    }

    #[test]
    fn obj_interlace_applies_vflip_to_the_doubled_source_line() {
        // With V-flip, the mirror applies to the doubled coordinate within its width block:
        // source line = ((r - Y) * 2 + field) ^ (width - 1). This sprite is 8x8, where that
        // is the same as the full-height mirror; the two differ only for the rectangular
        // OBSEL 6/7 sizes (#3003).
        let color_at_row = |row: u16, field: bool| {
            let mut ppu = Ppu::new();
            ppu.write_register(0x2101, 0x00); // 8x8
            ppu.write_register(0x2133, 0x02); // OBJ interlace only
            park_all_offscreen(&mut ppu);
            for fy in 0..8usize {
                set_obj_tile_pixel(&mut ppu, 0, 0, fy, fy as u8 + 1);
                set_cgram(&mut ppu, 128 + fy as u8 + 1, 0x1000 + fy as u16);
            }
            set_obj(&mut ppu, 0, 50, 40, 0, 0x80, false);
            ppu.interlace_field = field;
            present_line(&mut ppu, row);
            obj_color_at(&ppu, 50, row)
        };

        assert_eq!(
            color_at_row(40, false),
            Some(0x1007),
            "row 40 field 0 -> line 7"
        );
        assert_eq!(
            color_at_row(40, true),
            Some(0x1006),
            "row 40 field 1 -> line 6"
        );
        assert_eq!(
            color_at_row(41, false),
            Some(0x1005),
            "row 41 field 0 -> line 5"
        );
        assert_eq!(
            color_at_row(43, true),
            Some(0x1000),
            "row 43 field 1 -> line 0"
        );
    }

    #[test]
    fn obj_interlace_range_accounting_uses_halved_height() {
        // The range over-limit counts sprites in range of the halved height only.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        ppu.write_register(0x2133, 0x02); // OBJ interlace only
        park_all_offscreen(&mut ppu);
        for i in 0..33 {
            set_obj(&mut ppu, i, 0, 40, 0, 0, false);
        }

        let eval = ppu.evaluate_line_objects(44);
        assert!(
            eval.indices().is_empty(),
            "row 44 is past the halved height"
        );
        assert!(
            !eval.range_over,
            "out-of-range sprites consume no range budget"
        );

        let eval = ppu.evaluate_line_objects(40);
        assert_eq!(eval.indices().len(), 32, "in-range rows keep the 32 budget");
        assert!(eval.range_over, "33rd in-range OBJ still raises range over");
    }

    #[test]
    fn obj_interlace_with_screen_interlace_keeps_oam_y_anchor() {
        // Regression for the combined $2133=3 case: the doubled-line model previously anchored
        // the sprite at Y/2; the reference model keeps OAM Y with halved height per field.
        let color_at_row = |row: u16, field: bool| {
            let mut ppu = Ppu::new();
            ppu.write_register(0x2101, 0x00); // 8x8
            ppu.write_register(0x2133, 0x03); // screen + OBJ interlace
            park_all_offscreen(&mut ppu);
            set_obj_tile_solid(&mut ppu, 0, 3);
            set_obj_tile_pixel(&mut ppu, 0, 0, 0, 1); // source line 0 marker
            set_cgram(&mut ppu, 128 + 3, 0x1234);
            set_cgram(&mut ppu, 128 + 1, 0x0F0F);
            set_obj(&mut ppu, 0, 50, 40, 0, 0, false);
            ppu.interlace_field = field;
            present_line(&mut ppu, row);
            obj_color_at(&ppu, 50, row)
        };

        assert_eq!(color_at_row(20, false), None, "not anchored at Y/2");
        assert_eq!(
            color_at_row(40, false),
            Some(0x0F0F),
            "row 40 field 0 samples source line 0"
        );
        assert_eq!(
            color_at_row(43, false),
            Some(0x1234),
            "last halved-height row"
        );
        assert_eq!(
            color_at_row(44, false),
            None,
            "halved height: 4 rows for 8x8"
        );
    }

    #[test]
    fn palette_and_priority_come_from_attributes() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 1);
        // Palette 5 (attr bits 3-1 = 5 -> attr 0b1010), priority 2 (bits 5-4 -> 0b10_0000).
        set_cgram(&mut ppu, 128 + 5 * 16 + 1, 0x2222);
        let attr = (2 << 4) | (5 << 1);
        set_obj(&mut ppu, 0, 0, 0, 0, attr, false);

        present_line(&mut ppu, 0);
        let pixel = ppu.obj_pixel_at(0, 0).expect("opaque OBJ pixel");
        assert_eq!(pixel.color, 0x2222);
        assert_eq!(pixel.priority, 2);
    }

    #[test]
    fn large_object_uses_non_carrying_tile_composition() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // small 8x8, large 16x16
        park_all_offscreen(&mut ppu);
        // 16x16 OBJ at base tile 0: sub-tiles 0 (TL), 1 (TR), 16 (BL), 17 (BR), 16 words each.
        set_obj_tile_solid(&mut ppu, 0, 1); // tile 0 (TL) -> color 1
        set_obj_tile_solid(&mut ppu, 16, 2); // tile 1 (TR) -> color 2
        set_obj_tile_solid(&mut ppu, 16 * 16, 3); // tile 16 (BL) -> color 3
        set_obj_tile_solid(&mut ppu, 17 * 16, 4); // tile 17 (BR) -> color 4
        for c in 1..=4 {
            set_cgram(&mut ppu, 128 + c, 0x0100 * c as u16);
        }
        set_obj(&mut ppu, 0, 0, 0, 0, 0, true);

        present_line(&mut ppu, 0);
        assert_eq!(obj_color_at(&ppu, 0, 0), Some(0x0100), "top-left tile");
        assert_eq!(obj_color_at(&ppu, 8, 0), Some(0x0200), "top-right tile");
        present_line(&mut ppu, 8);
        assert_eq!(obj_color_at(&ppu, 0, 8), Some(0x0300), "bottom-left tile");
        assert_eq!(obj_color_at(&ppu, 8, 8), Some(0x0400), "bottom-right tile");
    }

    #[test]
    fn front_most_object_wins_overlap() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 1); // tile 0 -> color 1 (front obj uses palette 0)
        set_obj_tile_solid(&mut ppu, 16, 1); // tile 1 -> color 1 (back obj uses palette 1)
        set_cgram(&mut ppu, 128 + 1, 0x1111); // palette 0 color 1
        set_cgram(&mut ppu, 128 + 16 + 1, 0x2222); // palette 1 color 1
        // OBJ 0 (front) palette 0 tile 0; OBJ 1 (back) palette 1 tile 1, same position.
        set_obj(&mut ppu, 0, 30, 0, 0, 0, false);
        set_obj(&mut ppu, 1, 30, 0, 1, 1 << 1, false);

        present_line(&mut ppu, 0);
        assert_eq!(
            obj_color_at(&ppu, 30, 0),
            Some(0x1111),
            "front-most OBJ (lower index) wins"
        );
    }

    #[test]
    fn object_with_negative_x_is_clipped_to_the_screen() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00);
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 7);
        set_cgram(&mut ppu, 128 + 7, 0x3333);
        // X = 0x1FC (-4): columns 0..3 off-screen left, columns 4..7 visible at x 0..3.
        set_obj(&mut ppu, 0, 0xFC, 0, 0, 0, false);
        ppu.set_oam_byte(0x200, 0b01); // OBJ0 X bit8 = 1 -> X = 0x1FC

        present_line(&mut ppu, 0);
        assert!(ppu.obj_pixel_at(0, 0).is_some() && ppu.obj_pixel_at(3, 0).is_some());
        assert!(ppu.obj_pixel_at(4, 0).is_none());
    }

    /// Set up Mode 1 with a single solid BG1 pixel at screen (0,0) and OBJ tiles at a separate
    /// VRAM name base; returns the BG1 color so tests can distinguish it from the OBJ color.
    fn setup_bg1_and_obj(
        ppu: &mut Ppu,
        bg_high_priority: bool,
        obj_priority: u8,
        obj_enable: bool,
    ) {
        ppu.write_register(0x2105, 0x01); // BG mode 1
        ppu.write_register(0x2107, 0x10); // BG1SC: tilemap base word 0x1000
        ppu.write_register(0x210B, 0x00); // BG12NBA: BG1 char base word 0
        ppu.write_register(0x2101, 0x02); // OBSEL: 8x8, OBJ name base word 0x4000
        park_all_offscreen(ppu);

        // BG1 palette 0 color 2 -> CGRAM index 2; tile 1 solid color 2 at char word 16.
        set_cgram(ppu, 2, 0x0BBB);
        set_obj_tile_solid(ppu, 16, 2);
        let entry = 1u16 | if bg_high_priority { 0x2000 } else { 0 };
        ppu.set_vram_byte(0x2000, (entry & 0xFF) as u8);
        ppu.set_vram_byte(0x2001, (entry >> 8) as u8);

        // OBJ 0 at (0,0): tile 0 solid color 1, OBJ palette 0 -> CGRAM 129.
        set_obj_tile_solid(ppu, 0x4000, 1);
        set_cgram(ppu, 128 + 1, 0x0CCC);
        set_obj(ppu, 0, 0, 0, 0, obj_priority << 4, false);

        let tm = 0x01 | if obj_enable { 0x10 } else { 0x00 };
        ppu.write_register(0x212C, tm);
    }

    const BG1_COLOR: u16 = 0x0BBB;
    const OBJ_COLOR: u16 = 0x0CCC;

    fn composed_pixel(ppu: &Ppu, x: u16, y: u16) -> u16 {
        let (main, sub) = ppu.resolve_pixel_pair(x, y);
        ppu.compose_pixels(x, y, main, sub)
    }

    #[test]
    fn obj_priority_3_draws_in_front_of_high_priority_bg1() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 3, true);
        present_line(&mut ppu, 0);
        assert_eq!(composed_pixel(&ppu, 0, 0), OBJ_COLOR);
    }

    #[test]
    fn obj_priority_0_draws_behind_bg1() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 0, true);
        present_line(&mut ppu, 0);
        assert_eq!(composed_pixel(&ppu, 0, 0), BG1_COLOR);
    }

    #[test]
    fn tm_bit4_disabled_hides_objects() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 3, false);
        present_line(&mut ppu, 0);
        assert_eq!(composed_pixel(&ppu, 0, 0), BG1_COLOR);
    }

    #[test]
    fn obj_over_backdrop_when_no_bg_pixel() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 0, true);
        // x=8 has no BG1 tile (only tile (0,0) was mapped) and no OBJ -> backdrop (0).
        // Move OBJ to x=8 so only the OBJ (priority 0) covers the backdrop there.
        set_obj(&mut ppu, 0, 8, 0, 0, 0, false);
        present_line(&mut ppu, 0);
        assert_eq!(composed_pixel(&ppu, 8, 0), OBJ_COLOR);
    }

    fn tick_dots(ppu: &mut Ppu, dots: u32) {
        for _ in 0..(dots * 4) {
            ppu.tick();
        }
    }

    fn stat77(ppu: &mut Ppu) -> u8 {
        ppu.read_register(0x213E)
    }

    /// Place `count` 8x8 OBJs all covering OAM line `line`; park the rest off-screen.
    fn fill_in_range(ppu: &mut Ppu, count: usize, line: u8) {
        ppu.write_register(0x2101, 0x00);
        for i in 0..128 {
            let y = if i < count { line } else { 240 };
            set_obj(ppu, i, 0, y, 0, 0, false);
        }
    }

    #[test]
    fn range_over_flag_is_raised_at_oam_index_times_two_dot() {
        let mut ppu = Ppu::new();
        fill_in_range(&mut ppu, 40, 50); // 40 in range on line 50 -> 33rd is OBJ #32, dot 64
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        tick_dots(&mut ppu, 50 * 341 + 63); // scanline 50, dot 63 (just before)
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "range over not yet set at dot 63"
        );
        tick_dots(&mut ppu, 1); // dot 64
        assert_eq!(stat77(&mut ppu) & 0x40, 0x40, "range over set at dot 64");
    }

    #[test]
    fn range_over_flag_stays_clear_with_32_or_fewer_objects() {
        let mut ppu = Ppu::new();
        fill_in_range(&mut ppu, 32, 50);
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        tick_dots(&mut ppu, 51 * 341);
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "no range over with exactly 32 OBJs"
        );
    }

    #[test]
    fn time_over_flag_rises_during_the_fetch_window_of_the_eval_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5: large = 64x64 (8 tiles wide)
        for i in 0..128 {
            let y = if i < 5 { 50 } else { 224 }; // park at 224 (32px tall, no wrap into visible)
            let x = (i as u16 % 5) * 48; // five large OBJs spread across the screen
            set_obj(&mut ppu, i, x, y, 0, 0, i < 5); // 5 large -> 40 tiles > 34
        }
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        // Sliver fetching for line 50 occupies H~270..339 of scanline 50; the 35th
        // attempted fetch raises the flag inside that window (Mesen2/ares timing).
        tick_dots(&mut ppu, 50 * 341 + 269);
        assert_eq!(
            stat77(&mut ppu) & 0x80,
            0,
            "time over clear before the fetch window"
        );
        tick_dots(&mut ppu, 71); // through dot 340: the fetch window has completed
        assert_eq!(ppu.position().scanline, 50);
        assert_eq!(
            stat77(&mut ppu) & 0x80,
            0x80,
            "time over set inside the fetch window of the eval scanline"
        );
    }

    #[test]
    fn over_limit_flags_clear_at_end_of_vblank() {
        let mut ppu = Ppu::new();
        fill_in_range(&mut ppu, 40, 50);
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        tick_dots(&mut ppu, 50 * 341 + 100); // raise range over
        assert_eq!(stat77(&mut ppu) & 0x40, 0x40);
        // Advance to scanline 0 of the next frame (end of VBlank) -> flags cleared.
        tick_dots(&mut ppu, (262 - 50) * 341);
        assert_eq!(ppu.position().scanline, 0);
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "range over cleared at end of VBlank"
        );
    }

    #[test]
    fn over_limit_flags_are_not_cleared_during_forced_blank() {
        let mut ppu = Ppu::new();
        fill_in_range(&mut ppu, 40, 50);
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        tick_dots(&mut ppu, 50 * 341 + 100);
        assert_eq!(stat77(&mut ppu) & 0x40, 0x40);
        ppu.write_register(0x2100, 0x80); // forced blank
        tick_dots(&mut ppu, 262 * 341); // a full frame, crossing end of VBlank
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0x40,
            "not cleared during forced blank"
        );
    }

    /// Tick the PPU (display enabled) until the OBJ line for framebuffer row `line` has been
    /// fetched and presented (scanline `line + 1`, past the dot-0 buffer swap).
    fn present_line(ppu: &mut Ppu, line: u16) {
        ppu.write_register(0x2100, 0x0F); // display on (evaluation skips forced blank)
        loop {
            ppu.tick();
            let pos = ppu.position();
            if pos.scanline == line + 1 && pos.dot >= 1 {
                break;
            }
        }
    }

    #[test]
    fn time_over_drops_slivers_of_the_first_evaluated_objects() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5: small 32x32, large 64x64
        park_offscreen_32(&mut ppu);
        // A 64px-wide OBJ row spans tiles 0..7: make the whole top tile row opaque.
        for t in 0..8 {
            set_obj_tile_solid(&mut ppu, t * 16, 1);
        }
        set_cgram(&mut ppu, 128 + 1, 0x1111);
        set_cgram(&mut ppu, 128 + 16 + 1, 0x2222);
        // Five 64x64 OBJs covering line 100 -> 40 slivers, 6 over the 34-per-line budget.
        // Hardware fetches slivers in REVERSE evaluation order (OBJ 4 first), so the excess
        // is lost by the FIRST evaluated OBJ: OBJ 0 keeps only its 2 leftmost tile columns.
        for i in 0..5u16 {
            let attr = if i == 0 { 0 } else { 1 << 1 }; // OBJ0 palette 0, rest palette 1
            set_obj(&mut ppu, i as usize, i * 48, 100, 0, attr, true);
        }
        present_line(&mut ppu, 100);

        assert!(
            ppu.obj_pixel_at(0, 100).is_some(),
            "OBJ0 keeps its first surviving sliver"
        );
        assert!(ppu.obj_pixel_at(15, 100).is_some());
        assert!(
            ppu.obj_pixel_at(16, 100).is_none(),
            "OBJ0 slivers past the remaining budget are dropped"
        );
        assert!(ppu.obj_pixel_at(47, 100).is_none());
        assert!(
            ppu.obj_pixel_at(48, 100).is_some(),
            "later-evaluated OBJ1 (fetched earlier) keeps all slivers"
        );
        assert!(
            ppu.obj_pixel_at(255, 100).is_some(),
            "last-evaluated OBJ4 is fetched first and fully drawn"
        );
        assert_eq!(stat77(&mut ppu) & 0x80, 0x80, "time over flag set");
    }

    #[test]
    fn x256_object_consumes_sliver_budget_without_drawing() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5: small 32x32, large 64x64
        park_offscreen_32(&mut ppu);
        // A 64px-wide OBJ row spans tiles 0..7: make the whole top tile row opaque.
        for t in 0..8 {
            set_obj_tile_solid(&mut ppu, t * 16, 1);
        }
        set_cgram(&mut ppu, 128 + 1, 0x1111);
        // OBJs 0..3 on-screen (32 slivers) plus OBJ 4 at raw X=256: off-screen, but its
        // 8 slivers still consume the fetch budget (as if at X=0) without being drawn.
        for i in 0..4u16 {
            set_obj(&mut ppu, i as usize, i * 48, 100, 0, 0, true);
        }
        set_obj(&mut ppu, 4, 0x100, 100, 0, 0, true);
        present_line(&mut ppu, 100);

        // Reverse fetch: OBJ4 (invisible) + OBJ3..OBJ1 consume 32 slots; OBJ0 keeps 2.
        assert!(ppu.obj_pixel_at(0, 100).is_some());
        assert!(ppu.obj_pixel_at(15, 100).is_some());
        assert!(
            ppu.obj_pixel_at(16, 100).is_none(),
            "OBJ0 starved by the X=256 OBJ's slivers"
        );
        assert!(ppu.obj_pixel_at(47, 100).is_none());
        assert_eq!(
            stat77(&mut ppu) & 0x80,
            0x80,
            "X=256 slivers count toward the time over flag"
        );
    }

    #[test]
    fn fully_offscreen_left_objects_do_not_count_toward_the_range_limit() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5: small 32x32
        park_offscreen_32(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 1);
        set_cgram(&mut ppu, 128 + 1, 0x1111);
        // 32 OBJs fully off-screen left (X = -64, rightmost pixel at -33) on line 100.
        for i in 0..32 {
            set_obj(&mut ppu, i, 0x1C0, 100, 0, 0, false);
        }
        // A 33rd OBJ on-screen: hardware still evaluates it because horizontally
        // off-screen OBJs are not in range (Mesen2 IsVisible / ares onScanline).
        set_obj(&mut ppu, 32, 10, 100, 0, 0, false);
        present_line(&mut ppu, 100);

        assert!(
            ppu.obj_pixel_at(10, 100).is_some(),
            "on-screen OBJ is still evaluated"
        );
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "no range over: off-screen OBJs do not fill the 32-entry list"
        );
    }

    fn rgb_at(ppu: &Ppu, x: usize, y: usize) -> [u8; 3] {
        let rgb = ppu.screen_snapshot_rgb();
        let i = (y * 256 + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    #[test]
    fn mid_scanline_oamdata_write_affects_the_next_line_not_the_current_one() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // visible output
        ppu.write_register(0x2101, 0x00); // 8x8
        ppu.write_register(0x212C, 0x10); // OBJ on main screen
        park_all_offscreen(&mut ppu);

        set_cgram(&mut ppu, 0, 0x001F); // backdrop red
        set_obj_tile_solid(&mut ppu, 0, 1);
        set_cgram(&mut ppu, 128 + 1, 0x03E0); // OBJ green
        set_obj(&mut ppu, 0, 0, 0, 0, 0, false); // OBJ0 at x=0, rows 0..7

        // Render row 0 through x=4 (scanline 1, dot 26).
        tick_dots(&mut ppu, 341 + 26);

        // Mid-scanline OAMDATA write: move OBJ0 from x=0 to x=40. During active display
        // OAM low-table writes are redirected to the high table, so briefly enter forced
        // blank around the write, the way real games do (Mario Kart changes OAM mid-screen
        // via forced blank).
        ppu.write_register(0x2100, 0x80);
        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2104, 40);
        ppu.write_register(0x2104, 0);
        ppu.write_register(0x2100, 0x0F);

        // Finish row 0 and render row 1 past x=44.
        tick_dots(&mut ppu, 341 + 44);

        // Row 0 was fetched during scanline 0: the mid-line write must not disturb it.
        assert_eq!(rgb_at(&ppu, 4, 0), [0, 255, 0], "x=4 rendered before write");
        assert_eq!(
            rgb_at(&ppu, 6, 0),
            [0, 255, 0],
            "row 0 keeps the pre-write OBJ position after the write"
        );
        assert_eq!(rgb_at(&ppu, 40, 0), [255, 0, 0]);
        // Row 1's sliver fetch (H~270+ of scanline 1) reads the updated OAM.
        assert_eq!(
            rgb_at(&ppu, 4, 1),
            [255, 0, 0],
            "row 1 no longer shows the OBJ at its old position"
        );
        assert_eq!(
            rgb_at(&ppu, 40, 1),
            [0, 255, 0],
            "row 1 shows the OBJ at its new position"
        );
    }

    #[test]
    fn mid_scanline_obsel_write_affects_the_next_line_not_the_current_one() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // visible output
        ppu.write_register(0x2101, 0x60); // size pair: small 16x16, large 32x32
        ppu.write_register(0x212C, 0x10); // OBJ on main screen
        park_all_offscreen(&mut ppu);

        set_cgram(&mut ppu, 0, 0x001F); // backdrop red
        for t in 0..4 {
            set_obj_tile_solid(&mut ppu, t * 16, 1);
        }
        set_cgram(&mut ppu, 128 + 1, 0x03E0); // OBJ green
        set_obj(&mut ppu, 0, 0, 0, 0, 0, true); // large OBJ

        // Render row 0 through x=8 with 32x32 sizing.
        tick_dots(&mut ppu, 341 + 30);

        // Mid-scanline OBSEL write shrinks the large size to 16x16.
        ppu.write_register(0x2101, 0x00);

        // Finish row 0 and render row 1 past x=20.
        tick_dots(&mut ppu, 341 + 20);

        assert_eq!(rgb_at(&ppu, 8, 0), [0, 255, 0], "x=8 rendered before write");
        assert_eq!(
            rgb_at(&ppu, 20, 0),
            [0, 255, 0],
            "row 0 keeps the 32px width fetched during the previous scanline"
        );
        assert_eq!(rgb_at(&ppu, 8, 1), [0, 255, 0]);
        assert_eq!(
            rgb_at(&ppu, 20, 1),
            [255, 0, 0],
            "row 1 is fetched with the shrunk 16px width"
        );
    }

    #[test]
    fn mid_scanline_rotation_write_takes_effect_at_the_next_eval_window() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // visible output
        ppu.write_register(0x2101, 0x00); // 8x8
        ppu.write_register(0x212C, 0x10); // OBJ on main screen
        park_all_offscreen(&mut ppu);

        set_cgram(&mut ppu, 0, 0x001F); // backdrop red
        set_obj_tile_solid(&mut ppu, 0, 1);
        set_cgram(&mut ppu, 128 + 1, 0x03E0); // OBJ0 green
        set_cgram(&mut ppu, 128 + 16 + 1, 0x7C00); // OBJ1 blue
        set_obj(&mut ppu, 0, 40, 0, 0, 0, false); // palette 0, rows 0..7
        set_obj(&mut ppu, 1, 40, 0, 0, 1 << 1, false); // palette 1, same position

        // Reach scanline 1 dot 40: row 1's eval window latched its start index at dot 0.
        tick_dots(&mut ppu, 341 + 40);

        // OAMADD + rotation write: start eval from OBJ1.
        ppu.write_register(0x2102, 0x02); // bits 7-1 = 1
        ppu.write_register(0x2103, 0x80); // rotation enable

        // Render rows 1 and 2 past x=40.
        tick_dots(&mut ppu, 2 * 341 + 30);

        assert_eq!(
            rgb_at(&ppu, 40, 0),
            [0, 255, 0],
            "row 0 fetched before the write: OBJ0 wins"
        );
        assert_eq!(
            rgb_at(&ppu, 40, 1),
            [0, 255, 0],
            "row 1's eval order was latched at its window start: OBJ0 still wins"
        );
        assert_eq!(
            rgb_at(&ppu, 40, 2),
            [0, 0, 255],
            "row 2 evaluates with the rotated order: OBJ1 wins"
        );
    }

    #[test]
    fn rotation_shifts_the_range_over_dot_to_the_new_33rd_oam_index() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        park_all_offscreen(&mut ppu);
        // 40 OBJs in range on line 0; rotation start at OBJ1 makes the 33rd in-range
        // entry OAM index 33 -> flag at dot 66 (fullsnes H = OAM_INDEX*2).
        for i in 0..40 {
            set_obj(&mut ppu, i, 0, 0, 0, 0, false);
        }
        ppu.write_register(0x2102, 0x02); // bits 7-1 = 1
        ppu.write_register(0x2103, 0x80); // rotation enable
        ppu.write_register(0x2100, 0x0F); // display on

        tick_dots(&mut ppu, 65); // scanline 0, dot 65
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "range over not yet set at dot 65"
        );
        tick_dots(&mut ppu, 1); // dot 66
        assert_eq!(stat77(&mut ppu) & 0x40, 0x40, "range over set at dot 66");
    }

    // -- OBJ eval/fetch race: live re-read at fetch (#3026) -------------------

    /// Tick to `dot` of `scanline`, from a freshly-reset PPU.
    fn tick_to(ppu: &mut Ppu, scanline: u16, dot: u16) {
        ppu.write_register(0x2100, 0x0F); // display on: evaluation skips forced blank
        while ppu.position().scanline != scanline || ppu.position().dot != dot {
            ppu.tick();
        }
    }

    #[test]
    fn setini_toggled_between_the_eval_and_fetch_windows_is_read_live() {
        // The OBJ pipeline evaluates which sprites are on a line during H=0..255 and
        // fetches their slivers during H=270..339, storing only the OAM INDEX in between.
        // Everything else is re-read at fetch, so a SETINI write in the H=256..269 gap
        // changes the fetched source line for a line whose range check already ran.
        //
        // This matches both references: Mesen2 keeps only `_spriteIndexes[32]` and re-reads
        // via `FetchSpritePosition`, and ares-accurate keeps `{valid, index}` and re-reads
        // `oam.object[...]`. Neither re-checks range at fetch either (#3026).
        //
        // SETINI is the most legitimate lever for this: it is a plain register write with
        // no OAM address redirect, and the vendored hardware header marks it h-blank-legal
        // while marking OAMDATA/OBSEL v-blank-only.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        park_all_offscreen(&mut ppu);
        mark_obj_source_lines(&mut ppu, 2); // source lines 0..15
        set_obj(&mut ppu, 0, 50, 40, 0, 0x00, false);

        // Row 47 is the sprite's last row: evaluated with interlace OFF it is in range
        // (40..47) and its source line is 7.
        // Control: with no mid-gap write the row shows its own source line 7.
        let mut control = Ppu::new();
        control.write_register(0x2101, 0x00);
        park_all_offscreen(&mut control);
        mark_obj_source_lines(&mut control, 2);
        set_obj(&mut control, 0, 50, 40, 0, 0x00, false);
        assert_eq!(
            marked_source_line(&mut control, 50, 47),
            Some(7),
            "control: row 47 is the sprite's own last row"
        );

        tick_to(&mut ppu, 47, 260); // inside the eval -> fetch gap
        ppu.write_register(0x2133, 0x02); // OBJ interlace on, after the range check
        present_line(&mut ppu, 47);

        // The fetch doubles the line-within-sprite from the live SETINI, giving source
        // line 14 -- outside the 8-row sprite, and read from the next tile row down. A
        // value latched at evaluation would still be 7, as the control shows.
        assert_eq!(
            read_marked_source_line(&ppu, 50, 47),
            Some(14),
            "the fetch re-reads SETINI live, doubling the source line out of the sprite"
        );
    }

    #[test]
    fn oam_y_rewritten_between_the_windows_is_read_live() {
        // Same race via OAM Y. Note this is NOT reachable the way #3026 describes it: an
        // OAMDATA write during active display is redirected to the HIGH table, and Y lives
        // in the low table, so a game must bracket the write in forced blank (as Mario
        // Kart does). Driving the real $2104 path here rather than the `set_oam_byte`
        // helper is the point -- it proves the scenario is reachable at all.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        park_all_offscreen(&mut ppu);
        mark_obj_source_lines(&mut ppu, 8); // source lines 0..63
        set_obj(&mut ppu, 0, 50, 40, 0, 0x00, false);

        // Control: without the rewrite, row 40 is the sprite's own first row.
        let mut control = Ppu::new();
        control.write_register(0x2101, 0x00);
        park_all_offscreen(&mut control);
        mark_obj_source_lines(&mut control, 8);
        set_obj(&mut control, 0, 50, 40, 0, 0x00, false);
        assert_eq!(
            marked_source_line(&mut control, 50, 40),
            Some(0),
            "control: row 40 is the sprite's own first row"
        );

        tick_to(&mut ppu, 40, 260); // gap: range check for row 40 already ran (line 0)
        ppu.write_register(0x2100, 0x80); // forced blank, so the write is not redirected
        ppu.write_register(0x2102, 0x00); // OAMADD word 0 -> sprite 0's X/Y pair
        ppu.write_register(0x2104, 50); // X (even byte latches)
        ppu.write_register(0x2104, 0); // Y = 0 (odd byte commits the word)
        ppu.write_register(0x2100, 0x0F); // display back on before the fetch window
        present_line(&mut ppu, 40);

        // (40 - 0) & 0xFF = 40: five tile rows below the sprite's own single row. A Y
        // latched at evaluation would give source line 0.
        //
        // The masked arithmetic here is not a divergence from Mesen2's signed model: the
        // two differ by a multiple of 256, and 256 is a multiple of 8 (so the fine row is
        // unchanged) while 256/8 = 32 is a multiple of 16 (so the non-carrying tile-row
        // wrap absorbs it). See `masked_and_signed_source_lines_agree` below.
        assert_eq!(
            read_marked_source_line(&ppu, 50, 40),
            Some(40),
            "the fetch re-reads OAM Y live, sampling well outside the sprite"
        );
        assert_eq!(
            ppu.oam_byte(1),
            0,
            "the forced-blank window let the write reach the low table"
        );
    }

    #[test]
    fn oam_size_rewritten_between_the_windows_changes_the_vflip_mask() {
        // The third lever, and the only one reachable with NO forced blank: an OAMDATA
        // write during active display is redirected to the high table, which is exactly
        // where the per-sprite size bit lives. The fetch re-reads the size, so this
        // changes `width` -- and with V-flip that changes the mirror's XOR mask, not just
        // the sprite's footprint.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // OBSEL 0: small 8x8, large 16x16
        park_all_offscreen(&mut ppu);
        mark_obj_source_lines(&mut ppu, 2); // source lines 0..15
        set_obj(&mut ppu, 0, 50, 40, 0, 0x80, false); // V-flip, small

        // Control: staying small, row 43 mirrors within 8 rows -> 3 ^ 7 = 4.
        let mut control = Ppu::new();
        control.write_register(0x2101, 0x00);
        park_all_offscreen(&mut control);
        mark_obj_source_lines(&mut control, 2);
        set_obj(&mut control, 0, 50, 40, 0, 0x80, false);
        assert_eq!(
            marked_source_line(&mut control, 50, 43),
            Some(4),
            "control: an 8-wide sprite mirrors row 3 to source line 4"
        );

        // Row 43 of a small 8x8 sprite: line-within-sprite 3, mirrored to 3 ^ 7 = 4.
        tick_to(&mut ppu, 43, 260);
        // Redirected write: $2104 during active display lands at
        // 0x200 | ((addr & 0x1F0) >> 4), so an OAMADD word address of 0 targets high-table
        // byte 0x200 -- sprite 0's size/X8 bits. 0b10 = large, X bit 8 clear.
        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2104, 0x02);
        present_line(&mut ppu, 43);

        // Now 16 wide, so the mirror masks against 15 instead of 7: 3 ^ 15 = 12.
        assert_eq!(
            read_marked_source_line(&ppu, 50, 43),
            Some(12),
            "the fetch re-reads the size live, widening the V-flip mirror mask"
        );
    }

    #[test]
    fn masked_and_signed_source_lines_agree() {
        // Why #3026 needs no behaviour change. NESER takes the line-within-sprite in 8-bit
        // space; Mesen2 leaves it a signed unmasked difference. Those differ by a multiple
        // of 256, and the OBJ tile lookup cannot see that difference:
        //   - the fine row is `& 7`, and 256 is a multiple of 8;
        //   - the tile row is `>> 3` then `& 0x0F`, and 256/8 = 32 is a multiple of 16.
        // So off the V-flip path the two models are indistinguishable for EVERY input, in
        // range or not. (On the V-flip path they can differ, but via Mesen2's branch
        // predicate rather than the masking -- settled in #3003 in favour of
        // ares/higan/Snes9x and the SNESdev wiki.)
        //
        // This test exists so that claim fails loudly if the mask, the `/ 8` or the
        // `& 0x0F` is ever changed.
        for scanline in [0i32, 1, 39, 40, 100, 200, 224, 255] {
            for y in [0i32, 1, 40, 100, 200, 240, 255] {
                let signed = scanline - y;
                let masked = signed & 0xFF;
                assert_eq!(
                    masked & 7,
                    signed & 7,
                    "fine row differs for scanline {scanline}, Y {y}"
                );
                assert_eq!(
                    (masked >> 3) & 0x0F,
                    (signed >> 3) & 0x0F,
                    "tile row differs for scanline {scanline}, Y {y}"
                );
            }
        }
    }

    // -- OBJ vertical flip: width-block mirroring (#3003) ---------------------

    #[test]
    fn rectangular_object_vflip_mirrors_within_each_square_half() {
        // Rectangular OBJs (the OBSEL 6/7 sizes) flip as two stacked squares, NOT against
        // the full height: "rows 01234567 flip to 32107654, not 76543210" (SNESdev wiki
        // OAM page; Snes9x `line ^ (OBJWidths[S] - 1)`; ares/higan `object.cpp`).
        // For a 16x32 sprite each 16-row half mirrors inside itself and the halves keep
        // their positions.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xC0); // OBSEL size select 6: small 16x32
        park_offscreen_32(&mut ppu); // parked at Y=224: 224..255, never wraps
        mark_obj_source_lines(&mut ppu, 4);
        set_obj(&mut ppu, 0, 50, 100, 0, 0x80, false); // V-flip, fully visible

        let expected: [u16; 32] = [
            15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1,
            0, // top half, mirrored in place
            31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, // bottom half
        ];
        for (offset, want) in expected.iter().enumerate() {
            let row = 100 + offset as u16;
            assert_eq!(
                marked_source_line(&mut ppu, 50, row),
                Some(*want),
                "screen row {row} must show source line {want}"
            );
        }
    }

    #[test]
    fn rectangular_object_vflip_mirrors_within_the_half_when_y_wraps() {
        // Regression test for #3003. A 16x32 V-flipped sprite at Y=240 puts its lower
        // 16-row square half on screen lines 0-15, and that half mirrors within itself:
        // line 0 shows source line 31, line 15 shows source line 16. NESER used to mirror
        // against the full height, yielding source lines 15..0 instead.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xC0); // small 16x32
        park_offscreen_32(&mut ppu);
        mark_obj_source_lines(&mut ppu, 4);
        set_obj(&mut ppu, 0, 50, 240, 0, 0x80, false); // V-flip, wraps past 255

        for row in 0..16u16 {
            let want = 31 - row;
            assert_eq!(
                marked_source_line(&mut ppu, 50, row),
                Some(want),
                "wrapped V-flipped row {row} must show source line {want}"
            );
        }
    }

    #[test]
    fn square_object_vflip_still_mirrors_the_full_height_when_y_wraps() {
        // Equivalence guard: for the six square sizes the width-block mirror is identical
        // to the old full-height mirror, so this must pass both before and after #3003.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // small 8x8
        park_all_offscreen(&mut ppu); // parked at Y=240: 240..247, never wraps
        mark_obj_source_lines(&mut ppu, 1);
        set_obj(&mut ppu, 0, 50, 252, 0, 0x80, false); // V-flip, wraps: rows 252..255, 0..3

        for row in 0..4u16 {
            let within = row + 4; // (row - 252) & 0xFF
            let want = 7 - within;
            assert_eq!(
                marked_source_line(&mut ppu, 50, row),
                Some(want),
                "wrapped square V-flipped row {row} must show source line {want}"
            );
        }
    }

    #[test]
    fn large_rectangular_object_vflip_mirrors_within_each_32_row_half() {
        // The 32x64 large size of OBSEL 6 exercises the second bit of the width mask:
        // each 32-row half mirrors in place.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xC0); // OBSEL 6: large 32x64
        // Park at Y=224: the filler sprites take the SMALL size here (16x32), so they cover
        // 224..255 without wrapping, and they must not share a row with the test sprite
        // (100..163) -- the fetch window walks items in reverse, so a filler overlapping the
        // test sprite starves OBJ 0's slivers via time-over and reads back as `None`.
        park_offscreen_32(&mut ppu);
        mark_obj_source_lines(&mut ppu, 8);
        set_obj(&mut ppu, 0, 50, 100, 0, 0x80, true); // large, V-flip, fully visible

        for (offset, want) in [(0u16, 31u16), (31, 0), (32, 63), (63, 32)] {
            let row = 100 + offset;
            assert_eq!(
                marked_source_line(&mut ppu, 50, row),
                Some(want),
                "32x64 V-flipped row {row} must show source line {want}"
            );
        }
    }
}
