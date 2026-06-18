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
}
