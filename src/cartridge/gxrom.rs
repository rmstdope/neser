//! Mapper 66 - GxROM / GNROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::mapper_templates::DualBank32Mapper;

/// Mapper 66 - GxROM / GNROM
///
/// Hardware: Simple mapper selecting both PRG and CHR banks simultaneously
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/GxROM>
/// - PRG-ROM: Up to 128KB (4 32KB banks)
/// - PRG-RAM: None (some bootleg boards have 8KB)
/// - CHR-ROM: Up to 32KB (4 8KB banks)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Common boards: NES-GNROM, NES-MHROM
///
/// Notes:
/// - Any write to $8000-$FFFF selects banks
/// - Bits 0-1: Select 8KB CHR bank
/// - Bits 4-5: Select 32KB PRG bank
/// - Used in Super Mario Bros. + Duck Hunt, Doraemon, Dragon Power
///
/// Implementation:
/// - Uses `DualBank32Mapper` template with PRG bits 4-5, CHR bits 0-1
pub type GxROMMapper = DualBank32Mapper<0b0011, 4, 0b0011, 0, 66>;

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_gxrom_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(66, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_gxrom_prg_and_chr_bank_selected_by_single_write() {
        // Mapper 66 (GxROM/GNROM):
        // - PRG: 32KB banks selected by bits 4-5
        // - CHR: 8KB banks selected by bits 0-1

        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_gxrom_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("GxROM (mapper 66) should be implemented");

        // Initial banks should be 0.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Select PRG bank 1 (bits 4-5) and CHR bank 2 (bits 0-1): 0b0001_0010 = 0x12
        mapper.write_prg(0x8000, 0x12);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);

        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn test_gxrom_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 2);

        let mut mapper = create_gxrom_mapper(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("GxROM (mapper 66) should be implemented");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Bank select write should not affect mirroring.
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_gxrom_registers_snapshot_restores_bank_selects() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_gxrom_mapper(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("GxROM (mapper 66) should be implemented");

        mapper.write_prg(0x8000, 0x21); // PRG bank 2, CHR bank 1

        let registers = mapper.registers_snapshot();

        let mut restored = create_gxrom_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("GxROM (mapper 66) should be implemented");
        restored.restore_registers(&registers);

        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_chr(0x0000), 1);
    }

    #[test]
    fn test_gxrom_open_bus() {
        let mapper = create_gxrom_mapper(
            vec![0; 128 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        )
        .expect("GxROM (mapper 66) should be implemented");

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x99), 0x99);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xAA), 0xAA);
    }
}
