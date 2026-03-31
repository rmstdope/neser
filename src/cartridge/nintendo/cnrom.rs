//! Mapper 3 - CNROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::mapper_templates::SimpleFixedPrgMapper;

/// Mapper 3 - CNROM
///
/// Hardware: Simple CHR banking with fixed PRG-ROM
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/CNROM>
/// - Variants: <https://www.nesdev.org/wiki/CNROM#Variants>
/// - PRG-ROM: 16KB or 32KB fixed (no banking)
/// - PRG-RAM: None (some bootleg boards have 8KB)
/// - CHR-ROM: Up to 32KB (4 8KB banks)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Common boards: NES-CNROM
///
/// Notes:
/// - Any write to $8000-$FFFF selects CHR bank (bits 0-1)
/// - Some variants support up to 2048KB CHR-ROM
/// - Used in many early NES games like Solomon's Key, Arkanoid
///
/// Implementation:
/// - Uses `SimpleFixedPrgMapper` template with 8KB CHR banks
pub type CNROMMapper = SimpleFixedPrgMapper<8, 3>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;
    use crate::cartridge::{Mapper, NametableLayout};

    #[test]
    fn test_cnrom_32kb_prg_no_banking() {
        // CNROM has 32KB PRG ROM with no banking (like NROM)
        let mut prg_rom = vec![0; 32 * 1024];

        // Fill with pattern - each 1KB block gets a unique value
        for (i, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (i / 1024) as u8;
        }

        let mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            prg_rom,
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));

        // PRG ROM should be accessible at $8000-$FFFF
        assert_eq!(mapper.read_prg(0x8000), 0); // First byte of first 1KB block
        assert_eq!(mapper.read_prg(0x9000), 4); // $9000 = $8000 + $1000 = 4KB offset = block 4
        assert_eq!(mapper.read_prg(0xC000), 16); // $C000 = $8000 + $4000 = 16KB offset = block 16
        assert_eq!(mapper.read_prg(0xFFFF), 31); // $FFFF = last byte of block 31
    }

    #[test]
    fn test_cnrom_chr_bank_switching_4_banks() {
        // 32KB CHR ROM = 4 banks of 8KB
        let mut chr_rom = vec![0; 32 * 1024];

        // Fill each 8KB bank with its bank number
        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 10);
        assert_eq!(mapper.read_chr(0x1FFF), 10);

        // Switch to bank 2
        mapper.write_prg(0x8000, 0b0000_0010);
        assert_eq!(mapper.read_chr(0x0000), 20);
        assert_eq!(mapper.read_chr(0x1FFF), 20);

        // Switch to bank 3
        mapper.write_prg(0x8000, 0b0000_0011);
        assert_eq!(mapper.read_chr(0x0000), 30);
        assert_eq!(mapper.read_chr(0x1FFF), 30);

        // Switch back to bank 0
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn test_cnrom_chr_bank_switching_2_banks() {
        // 16KB CHR ROM = 2 banks of 8KB
        let mut chr_rom = vec![0; 16 * 1024];

        for bank in 0..2 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 50) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 50);

        // Writing higher bits should wrap (only 2 banks available)
        mapper.write_prg(0x8000, 0b0000_0011); // Bank 3 wraps to bank 1
        assert_eq!(mapper.read_chr(0x0000), 50);
    }

    #[test]
    fn test_cnrom_registers_snapshot_restores_chr_bank() {
        let mut chr_rom = vec![0; 32 * 1024];

        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 7) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0x8000, 0b0000_0011);

        let registers = mapper.registers_snapshot();

        let mut restored = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
            chr_rom,
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&registers);

        assert_eq!(restored.read_chr(0x0000), 21);
        assert_eq!(restored.read_chr(0x1FFF), 21);
    }

    #[test]
    fn test_cnrom_chr_read_only() {
        // CNROM uses CHR-ROM, not CHR-RAM - writes should be ignored
        let chr_rom = vec![0xAA; 32 * 1024];
        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0; 32 * 1024],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Try to write to CHR
        mapper.write_chr(0x0000, 0x55);

        // Should still read original ROM value
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let mapper_h = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_cnrom_bank_select_any_address() {
        // CNROM responds to writes anywhere in $8000-$FFFF
        let mut chr_rom = vec![0; 32 * 1024];

        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Write to different addresses in PRG space
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x0000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_chr(0x0000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_chr(0x0000), 103);
    }

    #[test]
    fn test_cnrom_registers_snapshot_bank_wrapping() {
        // Test edge case: bank selection wrapping when bank number exceeds available banks
        let prg_rom = vec![0xFF; 32 * 1024]; // 0xFF ensures bus conflicts don't mask the bank select
        let mut chr_rom = vec![0; 16 * 1024]; // Only 2 banks (16KB)

        // Fill CHR ROM with bank-specific data
        for bank in 0..2 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        // Create mapper and attempt to select bank 3 (should wrap to bank 1)
        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0x8000, 3); // Select bank 3, should wrap to bank 1 (3 % 2 = 1)

        // Verify bank wrapping before snapshot
        assert_eq!(mapper.read_chr(0x0000), 51); // Bank 1

        // Take snapshot
        let registers = mapper.registers_snapshot();

        // Create a fresh mapper and restore
        let mut restored = CNROMMapper::new(MapperContext::new_for_test(
            3,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&registers);

        // Verify the restored state maintains bank wrapping
        assert_eq!(restored.read_chr(0x0000), 51);
        assert_eq!(restored.read_chr(0x1FFF), 51);
    }

    #[test]
    fn test_cnrom_prg_ram_snapshot_roundtrip() {
        // Test that PRG-RAM (if present) can be saved and restored
        let prg_rom = vec![0; 32 * 1024];
        let chr_rom = vec![0; 32 * 1024];

        let mut mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        // Write pattern to PRG-RAM
        for i in 0..0x2000 {
            mapper.write_prg(0x6000 + i, (i & 0xFF) as u8);
        }

        // Verify writes
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x6100), 0x00); // i=0x100: (0x100 & 0xFF) = 0x00
        assert_eq!(mapper.read_prg(0x7FFF), 0xFF); // i=0x1FFF: (0x1FFF & 0xFF) = 0xFF

        // Take snapshot
        let prg_ram = mapper.wram_snapshot();

        // Clear PRG-RAM
        for i in 0..0x2000 {
            mapper.write_prg(0x6000 + i, 0x00);
        }

        // Verify cleared
        assert_eq!(mapper.read_prg(0x7FFF), 0x00);

        // Restore from snapshot
        mapper.load_wram_snapshot(&prg_ram);

        // Verify restoration
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x7FFF), 0xFF);
    }

    #[test]
    fn test_cnrom_open_bus() {
        let mapper = CNROMMapper::new(MapperContext::new_for_test(
            3,
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xCC), 0xCC);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xDD), 0xDD);
    }

    #[test]
    fn test_cnrom_no_prg_ram_when_banks_zero() {
        // CNROM with explicitly zero PRG-RAM banks should return open bus
        // and ignore writes at $6000-$7FFF (spec: no PRG-RAM on standard boards)
        let mut mapper = CNROMMapper::new(
            MapperContext::new_for_test(
                3,
                vec![0; 32 * 1024],
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        // Writes should be silently ignored
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Reads via open-bus path should return open-bus value, not RAM
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x5A),
            0x5A,
            "Should return open bus when no PRG-RAM"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, 0xC3),
            0xC3,
            "Should return open bus when no PRG-RAM"
        );
        assert_eq!(mapper.wram_size(), 0, "WRAM size must be 0 when no PRG-RAM");
    }

    #[test]
    fn test_cnrom_no_prg_ram_when_size_unspecified() {
        // CNROM with unspecified PRG-RAM size must default to no PRG-RAM
        // (standard CNROM boards have none; bootleg boards declare it explicitly)
        let mut mapper = CNROMMapper::new(
            MapperContext::new_for_test(
                3,
                vec![0; 32 * 1024],
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            )
            .with_unspecified_prg_ram_size(),
        );

        mapper.write_prg(0x6000, 0xAA);

        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x5A),
            0x5A,
            "Unspecified PRG-RAM size must result in open bus at $6000-$7FFF"
        );
        assert_eq!(mapper.wram_size(), 0);
    }

    #[test]
    fn test_cnrom_prg_ram_present_when_explicitly_specified() {
        // A bootleg CNROM board with explicitly-declared PRG-RAM should work
        let mut mapper = CNROMMapper::new(
            MapperContext::new_for_test(
                3,
                vec![0; 32 * 1024],
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );

        mapper.write_prg(0x6000, 0xDE);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xDE,
            "Explicitly declared PRG-RAM must be readable"
        );
        assert_eq!(mapper.wram_size(), 8 * 1024);
    }

    #[test]
    fn test_cnrom_submapper_0_applies_and_type_bus_conflicts() {
        // Submapper 0 = original CNROM hardware, which has AND-type bus conflicts.
        // Effective bank = written_value & ROM_value_at_written_address.
        let mut chr_rom = vec![0; 32 * 1024]; // 4 banks of 8KB
        for bank in 0..4usize {
            let start = bank * 8 * 1024;
            for byte in &mut chr_rom[start..start + 8 * 1024] {
                *byte = (bank * 10) as u8;
            }
        }

        // PRG-ROM: offset 0 (= CPU $8000) contains 0x02
        let mut prg_rom = vec![0xFF; 32 * 1024];
        prg_rom[0] = 0x02;

        let mut mapper = CNROMMapper::new(
            MapperContext::new_for_test(3, prg_rom, chr_rom, NametableLayout::Horizontal)
                .with_submapper(0)
                .with_prg_ram_banks(0),
        );

        // Write 0x01 to $8000. ROM[0] = 0x02. Bus conflict: 0x01 & 0x02 = 0x00 → bank 0
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "submapper 0: AND-type bus conflict must reduce effective bank to write & ROM"
        );
    }

    #[test]
    fn test_cnrom_submapper_1_disables_bus_conflicts() {
        // Submapper 1 = no bus conflicts; written value selects bank directly.
        let mut chr_rom = vec![0; 32 * 1024];
        for bank in 0..4usize {
            let start = bank * 8 * 1024;
            for byte in &mut chr_rom[start..start + 8 * 1024] {
                *byte = (bank * 10) as u8;
            }
        }

        // PRG-ROM offset 0 = 0x02 (would cause conflict if conflicts were active)
        let mut prg_rom = vec![0xFF; 32 * 1024];
        prg_rom[0] = 0x02;

        let mut mapper = CNROMMapper::new(
            MapperContext::new_for_test(3, prg_rom, chr_rom, NametableLayout::Horizontal)
                .with_submapper(1)
                .with_prg_ram_banks(0),
        );

        // Write 0x01 to $8000. ROM[0] = 0x02.
        // No conflict: written value 0x01 directly → bank 1 (value 10)
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            10,
            "submapper 1: no bus conflict, write value selects bank directly"
        );
    }
}
