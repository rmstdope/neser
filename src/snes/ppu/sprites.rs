//! OBJ (sprite) support: OBSEL decoding, OAM evaluation, line buffer, and over-limit flags.
//!
//! OBSEL ($2101) selects one of eight OBJ size pairs (including two undocumented pairs), the OBJ
//! tile name base (8K-word steps), and the name gap inserted between tiles $0FF and $100 (4K-word
//! steps). See fullsnes "SNES PPU Sprites (OBJs)".

use super::Ppu;

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
        let mut eval = ObjLineEval::default();
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
}
