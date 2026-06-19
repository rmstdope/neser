//! OBJ (sprite) support: OBSEL decoding, OAM evaluation, line buffer, and over-limit flags.
//!
//! OBSEL ($2101) selects one of eight OBJ size pairs (including two undocumented pairs), the OBJ
//! tile name base (8K-word steps), and the name gap inserted between tiles $0FF and $100 (4K-word
//! steps). See fullsnes "SNES PPU Sprites (OBJs)".

use super::{Ppu, SCREEN_HEIGHT};

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

    /// Evaluate which OBJs are in vertical range for display scanline `line`.
    ///
    /// OBJs are scanned in priority-rotation order (starting at [`Ppu::obj_first_sprite_index`]),
    /// keeping at most 32; the 33rd in-range OBJ sets the range over-limit (its OAM index is
    /// recorded for dot-accurate flag timing). 8-bit Y wrap yields the 224-line wrap behavior.
    pub(super) fn evaluate_line_objects(&self, line: u16) -> ObjLineEval {
        let first = self.obj_first_sprite_index() as usize;
        let mut eval = ObjLineEval {
            indices: Vec::with_capacity(32),
            ..ObjLineEval::default()
        };
        for k in 0..128usize {
            let i = (first + k) % 128;
            let y = self.oam[i * 4 + 1] as u16;
            let height = self.obj_size(i).1 as u16;
            let row = line.wrapping_sub(y) & 0xFF;
            if row < height {
                if eval.indices.len() < 32 {
                    eval.indices.push(i as u8);
                } else {
                    eval.range_over = true;
                    eval.range_over_index = Some(i as u8);
                    break;
                }
            }
        }
        eval
    }

    /// The OAM high-table X bit 8 for OBJ `index`.
    fn obj_x_high(&self, index: usize) -> u16 {
        ((self.oam[0x200 + (index >> 2)] >> ((index & 3) * 2)) & 1) as u16
    }

    /// Build the composited OBJ pixel line for display scanline `line`: in-range OBJs are drawn
    /// front-to-back (lowest evaluation order wins), gated by OBJ opacity and the 34-tile-per-line
    /// fetch budget (tile columns beyond the budget are dropped, matching the time over-limit).
    pub(super) fn build_obj_line(&self, line: u16) -> ObjLine {
        let eval = self.evaluate_line_objects(line);
        let mut buf = ObjLine::default();
        let mut tile_budget = 34i32;
        for &i in &eval.indices {
            if tile_budget <= 0 {
                break;
            }
            self.render_object_into(&mut buf, i as usize, line, &mut tile_budget);
        }
        buf
    }

    /// Count the on-screen 8x8 OBJ tile columns for the given in-range OBJs, reporting whether the
    /// 34-tile-per-line budget is exceeded (time over-limit). A tile column counts if any of its
    /// pixels fall within the visible 0..255 region.
    pub(super) fn count_obj_tiles(&self, indices: &[u8]) -> (u16, bool) {
        let mut tiles = 0u16;
        for &i in indices {
            let i = i as usize;
            let width = self.obj_size(i).0 as i32;
            let x9 = self.oam[i * 4] as u16 | (self.obj_x_high(i) << 8);
            let base_x = if x9 & 0x100 != 0 {
                x9 as i32 - 0x200
            } else {
                x9 as i32
            };
            for col in (0..width).step_by(8) {
                let tile_x = base_x + col;
                if tile_x + 8 > 0 && tile_x < 256 {
                    tiles += 1;
                    if tiles > 34 {
                        return (tiles, true);
                    }
                }
            }
        }
        (tiles, false)
    }

    /// Advance the OBJ evaluation pipeline for the current dot, raising the dot-accurate STAT77
    /// range/time over-limit flags.
    ///
    /// Following fullsnes STAT77: the line shown on the next scanline is evaluated during the
    /// current scanline, the range over-limit (bit 6) is raised at `H = OAM_index × 2` of the 33rd
    /// in-range OBJ, and the time over-limit (bit 7) is raised at `H = 0` of the displayed line.
    /// Both flags are cleared at the end of VBlank (scanline 0) but not during forced blank.
    pub(super) fn update_obj_pipeline(&mut self, forced_blank: bool) {
        let scanline = self.position.scanline;
        let dot = self.position.dot;

        if dot == 0 {
            if !forced_blank {
                if scanline == 0 {
                    self.stat77_range_over = false;
                    self.stat77_time_over = false;
                }
                if self.obj_time_over_pending {
                    self.stat77_time_over = true;
                }
            }
            self.obj_time_over_pending = false;
            self.obj_range_over_dot = None;

            // During scanline `s` the PPU evaluates the line displayed on scanline `s + 1`
            // (OAM line index `s`); skipped during forced blank.
            if !forced_blank && (scanline as usize) < SCREEN_HEIGHT {
                let eval = self.evaluate_line_objects(scanline);
                self.obj_range_over_dot = if eval.range_over {
                    eval.range_over_index.map(|i| i as u16 * 2)
                } else {
                    None
                };
                self.obj_time_over_pending = self.count_obj_tiles(&eval.indices).1;
            }
        }

        if Some(dot) == self.obj_range_over_dot {
            self.stat77_range_over = true;
        }
    }

    /// Render OBJ `index` into the line buffer for display scanline `line`, writing each opaque
    /// pixel only where no nearer (earlier) OBJ has already drawn. On-screen 8x8 tile columns
    /// consume `tile_budget`; columns past the 34-tile-per-line limit are dropped.
    fn render_object_into(
        &self,
        buf: &mut ObjLine,
        index: usize,
        line: u16,
        tile_budget: &mut i32,
    ) {
        let (width, height) = self.obj_size(index);
        let (width, height) = (width as i32, height as i32);
        let y = self.oam[index * 4 + 1] as u16;
        let attr = self.oam[index * 4 + 3];
        let priority = (attr >> 4) & 0x03;
        let palette = (attr >> 1) & 0x07;
        let hflip = attr & 0x40 != 0;
        let vflip = attr & 0x80 != 0;

        // 9-bit X is sign-extended: 256..511 represent negative on-screen positions.
        let x9 = self.oam[index * 4] as u16 | (self.obj_x_high(index) << 8);
        let base_x = if x9 & 0x100 != 0 {
            x9 as i32 - 0x200
        } else {
            x9 as i32
        };
        let base_tile = self.oam[index * 4 + 2] as u16 | (((attr & 0x01) as u16) << 8);

        let mut within_y = (line.wrapping_sub(y) & 0xFF) as i32;
        if vflip {
            within_y = height - 1 - within_y;
        }

        // Walk 8x8 tile columns left-to-right; each on-screen column consumes one tile-fetch slot.
        for tile_col in 0..(width / 8) {
            let tile_x = base_x + tile_col * 8;
            let on_screen = tile_x + 8 > 0 && tile_x < 256;
            if on_screen {
                if *tile_budget <= 0 {
                    break;
                }
                *tile_budget -= 1;
            }
            for sub in 0..8 {
                let col = tile_col * 8 + sub;
                let screen_x = base_x + col;
                if !(0..256).contains(&screen_x) {
                    continue;
                }
                let sx = screen_x as usize;
                if buf.present[sx] {
                    continue;
                }
                let within_x = if hflip { width - 1 - col } else { col };
                let color = self.obj_tile_pixel(base_tile, within_x, within_y);
                if color == 0 {
                    continue;
                }
                let cgindex = 128 + palette * 16 + color;
                buf.color[sx] = self.cgram_color(cgindex);
                buf.priority[sx] = priority;
                buf.present[sx] = true;
            }
        }
    }

    /// Decode an OBJ pixel color index (0-15) at sprite-relative `(within_x, within_y)`, applying
    /// non-carrying large-tile composition (right wraps the low nibble, down wraps the high nibble)
    /// and the OBSEL name base/gap addressing.
    fn obj_tile_pixel(&self, base_tile: u16, within_x: i32, within_y: i32) -> u8 {
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

/// Result of per-scanline OAM range evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ObjLineEval {
    /// In-range OBJ indices in evaluation order, truncated to 32.
    pub indices: Vec<u8>,
    /// Whether more than 32 OBJs were in range (range over-limit).
    pub range_over: bool,
    /// OAM index of the 33rd in-range OBJ that triggered the range over-limit, if any.
    pub range_over_index: Option<u8>,
}

/// Composited OBJ pixels for one scanline (256 visible pixels).
#[derive(Debug, Clone)]
pub(super) struct ObjLine {
    /// Resolved BGR555 color per pixel (valid only where `present`).
    pub color: [u16; 256],
    /// OBJ priority level (0-3, OAM attr bits 5-4) per pixel, for BG compositing.
    pub priority: [u8; 256],
    /// Whether an opaque OBJ pixel was written at this x.
    pub present: [bool; 256],
}

impl Default for ObjLine {
    fn default() -> Self {
        Self {
            color: [0; 256],
            priority: [0; 256],
            present: [false; 256],
        }
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
        assert!(eval.indices.is_empty());
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

        assert!(ppu.evaluate_line_objects(9).indices.is_empty());
        assert_eq!(ppu.evaluate_line_objects(10).indices, vec![0]);
        assert_eq!(ppu.evaluate_line_objects(17).indices, vec![0]);
        assert!(ppu.evaluate_line_objects(18).indices.is_empty());
    }

    #[test]
    fn large_object_height_comes_from_obsel_large_pair() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // small 8x8, large 16x16
        for i in 1..128 {
            set_obj(&mut ppu, i, 0, 240, 0, 0, false);
        }
        set_obj(&mut ppu, 0, 0, 10, 0, 0, true); // large -> height 16
        assert_eq!(ppu.evaluate_line_objects(25).indices, vec![0]); // 10..25 in range
        assert!(ppu.evaluate_line_objects(26).indices.is_empty());
    }

    #[test]
    fn object_y_wraps_in_8_bit_space_for_224_line_mode() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8
        for i in 1..128 {
            set_obj(&mut ppu, i, 0, 100, 0, 0, false);
        }
        set_obj(&mut ppu, 0, 0, 250, 0, 0, false); // covers 250..255, 0..1 (wrap)
        assert_eq!(ppu.evaluate_line_objects(1).indices, vec![0]);
        assert!(ppu.evaluate_line_objects(2).indices.is_empty());
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
        assert_eq!(eval.indices.len(), 32);
        assert!(eval.range_over);
        assert_eq!(eval.range_over_index, Some(32)); // 33rd in-range OBJ (index 32)
        assert_eq!(eval.indices[0], 0);
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
        assert_eq!(ppu.evaluate_line_objects(50).indices, vec![6, 7, 5]);
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

    #[test]
    fn renders_an_8x8_object_at_its_x_position() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0x00); // 8x8, name base 0
        park_all_offscreen(&mut ppu);
        set_obj_tile_solid(&mut ppu, 0, 3); // tile 0 solid color index 3
        set_cgram(&mut ppu, 128 + 3, 0x1234); // OBJ palette 0, color 3
        set_obj(&mut ppu, 0, 50, 20, 0, 0, false);

        let buf = ppu.build_obj_line(20);
        for x in 50..58 {
            assert!(buf.present[x], "pixel {x} opaque");
            assert_eq!(buf.color[x], 0x1234);
        }
        assert!(!buf.present[49]);
        assert!(!buf.present[58]);
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

        let buf = ppu.build_obj_line(0);
        assert!(buf.present[12], "only the opaque pixel is written");
        assert_eq!(buf.color[12], 0x7FFF);
        assert!(!buf.present[10]);
        assert!(!buf.present[11]);
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
        let buf = ppu.build_obj_line(0);
        assert!(buf.present[107]);
        assert!(!buf.present[100]);

        // Y-flip: top-left pixel moves to the bottom row (y+7).
        set_obj(&mut ppu, 0, 100, 0, 0, 0x80, false);
        let buf = ppu.build_obj_line(7);
        assert!(buf.present[100]);
        let buf0 = ppu.build_obj_line(0);
        assert!(!buf0.present[100]);
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

        let buf = ppu.build_obj_line(0);
        assert_eq!(buf.color[0], 0x2222);
        assert_eq!(buf.priority[0], 2);
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

        let top = ppu.build_obj_line(0);
        assert_eq!(top.color[0], 0x0100, "top-left tile");
        assert_eq!(top.color[8], 0x0200, "top-right tile");
        let bottom = ppu.build_obj_line(8);
        assert_eq!(bottom.color[0], 0x0300, "bottom-left tile");
        assert_eq!(bottom.color[8], 0x0400, "bottom-right tile");
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

        let buf = ppu.build_obj_line(0);
        assert_eq!(buf.color[30], 0x1111, "front-most OBJ (lower index) wins");
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

        let buf = ppu.build_obj_line(0);
        assert!(buf.present[0] && buf.present[3]);
        assert!(!buf.present[4]);
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
        ppu.obj_line = ppu.build_obj_line(0);
    }

    const BG1_COLOR: u16 = 0x0BBB;
    const OBJ_COLOR: u16 = 0x0CCC;

    #[test]
    fn obj_priority_3_draws_in_front_of_high_priority_bg1() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 3, true);
        assert_eq!(ppu.compute_pixel(0, 0), OBJ_COLOR);
    }

    #[test]
    fn obj_priority_0_draws_behind_bg1() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 0, true);
        assert_eq!(ppu.compute_pixel(0, 0), BG1_COLOR);
    }

    #[test]
    fn tm_bit4_disabled_hides_objects() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 3, false);
        assert_eq!(ppu.compute_pixel(0, 0), BG1_COLOR);
    }

    #[test]
    fn obj_over_backdrop_when_no_bg_pixel() {
        let mut ppu = Ppu::new();
        setup_bg1_and_obj(&mut ppu, true, 0, true);
        // x=8 has no BG1 tile (only tile (0,0) was mapped) and no OBJ -> backdrop (0).
        // Move OBJ to x=8 so only the OBJ (priority 0) covers the backdrop there.
        set_obj(&mut ppu, 0, 8, 0, 0, 0, false);
        ppu.obj_line = ppu.build_obj_line(0);
        assert_eq!(ppu.compute_pixel(8, 0), OBJ_COLOR);
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
        tick_dots(&mut ppu, 51 * 341);
        assert_eq!(
            stat77(&mut ppu) & 0x40,
            0,
            "no range over with exactly 32 OBJs"
        );
    }

    #[test]
    fn time_over_flag_is_raised_when_more_than_34_tiles_on_a_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5: large = 64x64 (8 tiles wide)
        for i in 0..128 {
            let y = if i < 5 { 50 } else { 224 }; // park at 224 (32px tall, no wrap into visible)
            set_obj(&mut ppu, i, 0, y, 0, 0, i < 5); // 5 large -> 40 tiles > 34
        }
        // Time over for line 50 is applied at the start of display scanline 51 (H=0).
        tick_dots(&mut ppu, 50 * 341 + 200);
        assert_eq!(
            stat77(&mut ppu) & 0x80,
            0,
            "time over not set during the eval scanline"
        );
        tick_dots(&mut ppu, 141); // cross into scanline 51, dot 0
        assert_eq!(
            stat77(&mut ppu) & 0x80,
            0x80,
            "time over set at H=0 of display line"
        );
    }

    #[test]
    fn over_limit_flags_clear_at_end_of_vblank() {
        let mut ppu = Ppu::new();
        fill_in_range(&mut ppu, 40, 50);
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

    #[test]
    fn tile_columns_beyond_the_34_budget_are_dropped() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2101, 0xA0); // size 5 small = 32x32 (4 tiles wide)
        park_offscreen_32(&mut ppu);
        // Eight transparent 32x32 OBJs cover x 0..255 on line 0 -> 32 tile slots consumed.
        for i in 0..8 {
            set_obj(&mut ppu, i, (i * 32) as u16, 0, 10, 0, false); // tile 10 -> transparent
        }
        // A ninth opaque 32x32 OBJ at x=0: only 2 tile slots remain (34 - 32), so columns past
        // x=15 must be dropped. Make its whole top tile-row opaque (tiles 0..3 solid color 1).
        for t in 0..4 {
            set_obj_tile_solid(&mut ppu, t * 16, 1);
        }
        set_cgram(&mut ppu, 128 + 1, 0x1357);
        set_obj(&mut ppu, 8, 0, 0, 0, 0, false);

        let buf = ppu.build_obj_line(0);
        assert!(
            buf.present[0] && buf.present[15],
            "first two tile columns drawn"
        );
        assert!(!buf.present[16], "tile columns past the budget are dropped");
        assert!(!buf.present[31]);
    }
}
