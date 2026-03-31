//! Mapper 110 - Sachen SA-020A (deprecated iNES duplicate assignment)
//!
//! Specifications:
//! - Legacy assignment: <https://www.nesdev.org/wiki/INES_Mapper_110>
//! - Canonical implementation: <https://www.nesdev.org/wiki/INES_Mapper_243>
//!
//! Known Limitations:
//! - Mapper 110 is an obsolete duplicate assignment for Sachen SA-020A.
//! - This implementation intentionally mirrors mapper 243 behavior.

use crate::cartridge::mapper243::Mapper243;

/// Mapper 110 - obsolete iNES duplicate assignment for Sachen SA-020A.
///
/// Per NesDev, mapper 110 should behave like mapper 243 and is kept only for
/// compatibility with legacy ROM headers.
pub type Mapper110 = Mapper243;

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper110(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            110, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn mapper_110_is_registered_in_factory() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 8);
        let mapper = create_mapper110(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 110 should be creatable via factory");
    }

    #[test]
    fn mapper_110_prg_bank_switching_matches_sa020a_register_5() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper110(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 1);
        assert_eq!(mapper.read_prg(0x8000), 1);

        mapper.write_prg(0x4101, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn mapper_110_chr_and_mirroring_match_sa020a_registers() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper110(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x0A); // bit1=1 (H mirroring), bit3=1 (CHR bit0)

        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 0x02); // CHR bit1=1

        mapper.write_prg(0x4100, 6);
        mapper.write_prg(0x4101, 0x02); // CHR bit2=1

        assert_eq!(mapper.read_chr(0x0000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }
}
