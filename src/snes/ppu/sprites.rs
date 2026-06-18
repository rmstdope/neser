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
}
