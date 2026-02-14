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
    use crate::cartridge::{Mapper, MirroringMode};

    #[test]
    fn test_cnrom_32kb_prg_no_banking() {
        // CNROM has 32KB PRG ROM with no banking (like NROM)
        let mut prg_rom = vec![0; 32 * 1024];

        // Fill with pattern - each 1KB block gets a unique value
        for (i, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (i / 1024) as u8;
        }

        let mapper = CNROMMapper::new(prg_rom, vec![0; 32 * 1024], MirroringMode::Horizontal);

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

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

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

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Vertical);

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

        let mut mapper = CNROMMapper::new(
            vec![0; 32 * 1024],
            chr_rom.clone(),
            MirroringMode::Horizontal,
        );
        mapper.write_prg(0x8000, 0b0000_0011);

        let registers = mapper.registers_snapshot();

        let mut restored = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);
        restored.restore_registers(&registers);

        assert_eq!(restored.read_chr(0x0000), 21);
        assert_eq!(restored.read_chr(0x1FFF), 21);
    }

    #[test]
    fn test_cnrom_chr_read_only() {
        // CNROM uses CHR-ROM, not CHR-RAM - writes should be ignored
        let chr_rom = vec![0xAA; 32 * 1024];
        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Try to write to CHR
        mapper.write_chr(0x0000, 0x55);

        // Should still read original ROM value
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let mapper_h = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Horizontal,
        );
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Vertical,
        );
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
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

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

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
        let prg_rom = vec![0; 32 * 1024];
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
        let mut mapper =
            CNROMMapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal);
        mapper.write_prg(0x8000, 3); // Select bank 3, should wrap to bank 1 (3 % 2 = 1)

        // Verify bank wrapping before snapshot
        assert_eq!(mapper.read_chr(0x0000), 51); // Bank 1

        // Take snapshot
        let registers = mapper.registers_snapshot();

        // Create a fresh mapper and restore
        let mut restored = CNROMMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
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

        let mut mapper =
            CNROMMapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Vertical);

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
        let mapper = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Horizontal,
        );

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xCC), 0xCC);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xDD), 0xDD);
    }
}
