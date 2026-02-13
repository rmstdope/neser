use crate::cartridge::mapper_templates::DualBank32Mapper;

/// Mapper 11 - Color Dreams
///
/// Hardware: Simple unlicensed mapper with combined PRG/CHR banking
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/Color_Dreams>
/// - PRG-ROM: Up to 128KB (4 32KB banks)
/// - PRG-RAM: None
/// - CHR-ROM: Up to 128KB (16 8KB banks)
/// - Mirroring: Fixed horizontal or vertical
///
/// Common boards: Unlicensed Color Dreams boards
///
/// Notes:
/// - Single register at any write to $8000-$FFFF
/// - Bits 0-3: Select 8KB CHR bank
/// - Bits 4-7: Select 32KB PRG bank
/// - Used in unlicensed games like Crystal Mines, Bible Adventures
/// - Some variants support different bank counts
///
/// Implementation:
/// - Uses `DualBank32Mapper` template with PRG bits 4-7, CHR bits 0-3
pub type ColorDreamsMapper = DualBank32Mapper<0b1111, 4, 0b1111, 0, 11>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::MirroringMode;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_colordreams_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(11, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_colordreams_prg_and_chr_bank_selected_by_single_write() {
        // Mapper 11 (ColorDreams):
        // - PRG: 32KB banks selected by bits 4-7
        // - CHR: 8KB banks selected by bits 0-1

        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Initial banks should be 0.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Select PRG bank 1 (bits 4-7) and CHR bank 2 (bits 0-1): 0b0001_0010 = 0x12
        mapper.write_prg(0x8000, 0x12);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);

        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn test_colordreams_prg_uses_4_bits() {
        // ColorDreams uses bits 4-7 for PRG bank selection (16 banks possible)
        // This differs from GxROM which only uses bits 4-5 (4 banks)

        let prg_rom = banked_data(32 * 1024, 16); // 16 banks of 32KB
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Test bank 8 (0b1000_0000 = 0x80)
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.read_prg(0x8000), 8);

        // Test bank 15 (0b1111_0000 = 0xF0)
        mapper.write_prg(0x8000, 0xF0);
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_colordreams_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 2);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("ColorDreams (mapper 11) should be implemented");

        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Bank select write should not affect mirroring.
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_colordreams_bank_wrapping() {
        // Test that bank selection wraps when ROM is smaller than max banks

        let prg_rom = banked_data(32 * 1024, 4); // Only 4 PRG banks
        let chr_rom = banked_data(8 * 1024, 2); // Only 2 CHR banks

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Select bank 5, should wrap to bank 1 (5 % 4 = 1)
        mapper.write_prg(0x8000, 0x50); // 0b0101_0000
        assert_eq!(mapper.read_prg(0x8000), 1);

        // Select CHR bank 3, should wrap to bank 1 (3 % 2 = 1)
        mapper.write_prg(0x8000, 0x03); // 0b0000_0011
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn test_colordreams_registers_snapshot_restores_banks() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper =
            create_colordreams_mapper(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal)
                .expect("ColorDreams (mapper 11) should be implemented");

        // Select PRG bank 2 and CHR bank 3 (0b0010_0011 = 0x23)
        mapper.write_prg(0x8000, 0x23);

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_chr(0x0000), 3);
    }

    #[test]
    fn test_colordreams_banked_rom_replacement() {
        use crate::cartridge::common::BankedRom;
        use crate::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 32 * 1024;
        const CHR_BANK_SIZE: usize = 8 * 1024;

        // Create test ROM with distinct data per bank
        let prg_rom = banked_data(PRG_BANK_SIZE, 4);
        let chr_rom = banked_data(CHR_BANK_SIZE, 4);

        // Create BankedRom instances like the mapper would
        let prg_banked = BankedRom::new(prg_rom.clone(), PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom.clone(), CHR_BANK_SIZE);

        // Test reading from different banks
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(2, 0), 2);
        assert_eq!(prg_banked.read(3, 0), 3);

        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(1, 0), 1);
        assert_eq!(chr_banked.read(2, 0), 2);
        assert_eq!(chr_banked.read(3, 0), 3);

        // Test bank wrapping for PRG (4 banks)
        assert_eq!(prg_banked.read(4, 0), 0); // wraps to bank 0
        assert_eq!(prg_banked.read(5, 0), 1); // wraps to bank 1
        assert_eq!(prg_banked.read(7, 0), 3); // wraps to bank 3
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to bank 0
    }

    #[test]
    fn test_colordreams_open_bus() {
        let mapper = ColorDreamsMapper::new(
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            MirroringMode::Horizontal,
        );

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x55), 0x55);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x66), 0x66);
    }
}
