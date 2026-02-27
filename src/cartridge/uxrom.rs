//! Mapper 2 - UxROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::mapper_templates::SimpleBankedPrgMapper;

/// Mapper 2 - UxROM (UNROM, UOROM boards)
///
/// Hardware: Simple PRG banking with fixed upper bank
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/UxROM>
/// - Variants: <https://www.nesdev.org/wiki/UxROM#Variants>
/// - PRG-ROM: Up to 256KB (16 16KB banks)
/// - PRG-RAM: None to 8KB (depends on board variant)
/// - CHR: 8KB CHR-RAM fixed (no CHR-ROM support)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Common boards: NES-UNROM, NES-UOROM
///
/// Notes:
/// - Switchable 16KB PRG bank at $8000-$BFFF
/// - Fixed last 16KB PRG bank at $C000-$FFFF
/// - Any write to $8000-$FFFF selects bank (value = bank number)
/// - Used in Mega Man, Castlevania, Contra, Duck Tales, Metal Gear
///
/// Implementation:
/// - Uses `SimpleBankedPrgMapper` template with 16KB PRG banks
pub type UxROMMapper = SimpleBankedPrgMapper<16, 2>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;
    use crate::cartridge::{Mapper, NametableLayout};

    #[test]
    fn test_uxrom_128kb_prg_bank_switching() {
        // Create 128KB (8 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Initially bank 0 should be at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Last bank (7) should always be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_prg(0xFFFF), 7);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 3);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 7);

        // Switch to bank 5
        mapper.write_prg(0xFFFF, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);

        // Last bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_uxrom_256kb_prg_bank_switching() {
        // Create 256KB (16 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ));

        // Last bank (15) should be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 10
        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 0
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn test_uxrom_chr_ram() {
        // UxROM uses 8KB CHR-RAM
        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_uxrom_mirroring() {
        let mapper_h = UxROMMapper::new(MapperContext::new_for_test(
            2,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = UxROMMapper::new(MapperContext::new_for_test(
            2,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_uxrom_bank_register_mask() {
        // Test that all 8 bits of the bank register work
        let mut prg_rom = vec![0; 256 * 1024]; // 16 banks

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Test writing different bit patterns
        mapper.write_prg(0x8000, 0b0000_0000); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 0b0000_0111); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 70);

        mapper.write_prg(0x8000, 0b0000_1111); // Bank 15
        assert_eq!(mapper.read_prg(0x8000), 150);
    }

    #[test]
    fn test_uxrom_fixed_last_bank() {
        // Verify that $C000-$FFFF is always the last bank regardless of switches
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Last bank should always read 115 (bank 15 + 100)
        assert_eq!(mapper.read_prg(0xC000), 115);
        assert_eq!(mapper.read_prg(0xFFFF), 115);

        // Switch banks several times
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0xC000), 115);
    }

    #[test]
    fn test_uxrom_registers_snapshot_restores_bank_and_chr_ram() {
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom.clone(),
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 3);
        mapper.write_chr(0x0000, 0x5A);

        let regs = mapper.registers_snapshot();
        let chr = mapper.chr_ram_snapshot();

        let mut restored = UxROMMapper::new(MapperContext::new_for_test(
            2,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&regs);
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 0x5A);
    }

    #[test]
    fn test_uxrom_open_bus() {
        let mapper = UxROMMapper::new(MapperContext::new_for_test(
            2,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xAA), 0xAA);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xBB), 0xBB);
    }

    // --- Issue #345: PRG-RAM absence via metadata ---

    #[test]
    fn test_uxrom_no_prg_ram_write_ignored() {
        // Given: UxROM with no PRG-RAM (prg_ram_banks_8k = 0)
        let mut mapper = UxROMMapper::new(
            MapperContext::new_for_test(
                2,
                vec![0; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        // When: writing to $6000-$7FFF
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);

        // Then: reads still return 0 (writes ignored, no RAM)
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "no PRG-RAM: write should be ignored"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0,
            "no PRG-RAM: write should be ignored"
        );
    }

    #[test]
    fn test_uxrom_no_prg_ram_open_bus_returns_open_bus() {
        // Given: UxROM with no PRG-RAM
        let mapper = UxROMMapper::new(
            MapperContext::new_for_test(
                2,
                vec![0; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        let open_bus = 0x42;

        // When: reading $6000-$7FFF via read_prg_open_bus
        // Then: return open_bus (no RAM present)
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "no PRG-RAM: $6000 should return open-bus"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus),
            open_bus,
            "no PRG-RAM: $7FFF should return open-bus"
        );
    }

    #[test]
    fn test_uxrom_with_prg_ram_read_write_works() {
        // Given: UxROM with 8KB PRG-RAM (prg_ram_banks_8k = 1)
        let mut mapper = UxROMMapper::new(
            MapperContext::new_for_test(
                2,
                vec![0; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );

        // When: writing and reading back
        mapper.write_prg(0x6000, 0x77);
        mapper.write_prg(0x7FFF, 0x88);

        // Then: values are preserved
        assert_eq!(
            mapper.read_prg(0x6000),
            0x77,
            "PRG-RAM present: write/read should work"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0x88,
            "PRG-RAM present: write/read should work"
        );
    }
}
