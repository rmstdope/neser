//! Mapper 71 - Camerica / BF909x
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;

/// Mapper 71 - Camerica / Codemasters
///
/// Hardware: Unlicensed mapper similar to UxROM with programmable one-screen mirroring
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_071>
/// - Codemasters: <https://www.nesdev.org/wiki/Codemasters>
/// - PRG-ROM: Up to 256KB (16 16KB banks)
/// - PRG-RAM: None
/// - CHR: 8KB CHR-RAM fixed (no CHR-ROM support)
/// - Mirroring: Submapper 0 = hardwired H/V; Submapper 1 = programmable one-screen
///
/// Common boards: Camerica/Codemasters unlicensed boards
///
/// Registers (per NESdev spec):
/// - $8000-$9FFF: Mirroring control (submapper 1 / Fire Hawk only)
///   - Bit 4: One-screen mirroring (0 = lower/A, 1 = upper/B)
/// - $C000-$FFFF: PRG bank select (all variants)
///   - Bits 0-3: Select 16KB PRG bank at $8000-$BFFF
///
/// Notes:
/// - Last 16KB PRG bank always fixed at $C000-$FFFF
/// - Similar to UxROM but with programmable mirroring (submapper 1)
/// - Used in Micro Machines, Fire Hawk, Dizzy series (Codemasters games)
pub struct CamericaMapper {
    base: BaseMapper,
    bank_select: u8,
    has_mirroring_control: bool,
}

impl CamericaMapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let has_mirroring_control = ctx.submapper == 1;
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: has_mirroring_control,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        // Fixed last bank at slot 1 ($C000-$FFFF)
        base.select_prg_page(1, -1);
        if has_mirroring_control {
            // Fire Hawk (submapper 1): default to one-screen lower
            base.set_mirroring(NametableLayout::SingleScreenLower);
        }
        // Submapper 0: mirroring from iNES header (no override)
        Self {
            base,
            bank_select: 0,
            has_mirroring_control,
        }
    }
}

impl Mapper for CamericaMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x8000..=0x9FFF if self.has_mirroring_control => {
                // Mirroring control (submapper 1 / Fire Hawk only, bit 4)
                let upper = (value & 0x10) != 0;
                self.base.set_mirroring(if upper {
                    NametableLayout::SingleScreenUpper
                } else {
                    NametableLayout::SingleScreenLower
                });
            }
            0xC000..=0xFFFF => {
                // PRG bank select (bits 0-3)
                self.bank_select = value & 0x0F;
                self.base.select_prg_page(0, self.bank_select as i16);
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let one_screen_upper = matches!(self.base.mirroring(), NametableLayout::SingleScreenUpper);
        vec![self.bank_select, one_screen_upper as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.bank_select = data[0];
            self.base.select_prg_page(0, self.bank_select as i16);
            if self.has_mirroring_control {
                self.base.set_mirroring(if data[1] != 0 {
                    NametableLayout::SingleScreenUpper
                } else {
                    NametableLayout::SingleScreenLower
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    #[test]
    fn test_mapper_71_is_wired_in_factory() {
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![];
        let mapper = create_mapper(MapperContext::new_for_test(
            71,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 71 should be implemented");
    }

    #[test]
    fn test_mapper71_prg_bank_switching() {
        // Create 256KB (16 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each bank with its bank number
        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Initially bank 0 should be at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Last bank (15) should always be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 15);
        assert_eq!(mapper.read_prg(0xFFFF), 15);

        // Switch to bank 3 (bank select at $C000-$FFFF per spec)
        mapper.write_prg(0xC000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 3);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 10
        mapper.write_prg(0xD000, 10);
        assert_eq!(mapper.read_prg(0x8000), 10);

        // Last bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn test_mapper71_bank_register_mask() {
        // Test that only bits 0-3 affect bank selection
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Test that upper bits are masked off (bank select at $C000+)
        mapper.write_prg(0xC000, 0b1111_0101); // Should select bank 5
        assert_eq!(mapper.read_prg(0x8000), 50);

        mapper.write_prg(0xC000, 0b0000_1111); // Bank 15
        assert_eq!(mapper.read_prg(0x8000), 150);
    }

    #[test]
    fn test_mapper71_one_screen_mirroring() {
        // Mirroring control is submapper 1 (Fire Hawk) only
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = CamericaMapper::new(
            MapperContext::new_for_test(71, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        );

        // Default should be lower nametable (submapper 1 overrides to SingleScreenLower)
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Write to $8000-$9FFF with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x9000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x9000, 0b0001_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        // Write with bit 4 = 0 again
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Test that other bits don't affect mirroring
        mapper.write_prg(0x9000, 0b0001_1111); // Bit 4 = 1 (0x1F)
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_mapper71_submapper0_no_mirroring_control() {
        // Submapper 0 has hardwired mirroring from header
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Should use header mirroring (horizontal)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Writes to $8000-$9FFF should NOT change mirroring
        mapper.write_prg(0x9000, 0x10);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mapper71_chr_ram() {
        // Mapper 71 uses 8KB CHR-RAM
        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
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
    fn test_mapper71_fixed_last_bank() {
        // Verify that $C000-$FFFF is always the last bank regardless of switches
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Last bank should always read 115 (bank 15 + 100)
        assert_eq!(mapper.read_prg(0xC000), 115);
        assert_eq!(mapper.read_prg(0xFFFF), 115);

        // Switch banks several times (bank select at $C000+)
        mapper.write_prg(0xC000, 0);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0xC000, 5);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0xC000, 10);
        assert_eq!(mapper.read_prg(0xC000), 115);
    }

    #[test]
    fn test_mapper71_separate_registers() {
        // Verify that bank select and mirroring control are separate registers
        // Submapper 1 has mirroring control
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 20) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(
            MapperContext::new_for_test(71, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        );

        // Set bank to 5 ($C000+ = bank select)
        mapper.write_prg(0xC000, 5);
        assert_eq!(mapper.read_prg(0x8000), 25);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Set mirroring to upper ($8000-$9FFF = mirroring for submapper 1)
        mapper.write_prg(0x9000, 0x10);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
        // Bank should remain 5
        assert_eq!(mapper.read_prg(0x8000), 25);

        // Change bank to 3
        mapper.write_prg(0xC000, 3);
        assert_eq!(mapper.read_prg(0x8000), 23);
        // Mirroring should still be upper
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_mapper71_prg_ram_support() {
        // Mapper 71 should support PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = CamericaMapper::new(MapperContext::new_for_test(
            71,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
    }

    #[test]
    fn test_camerica_registers_snapshot_restores_bank_mirroring_and_chr_ram() {
        let mut prg_rom = vec![0; 64 * 1024];
        for bank in 0..4 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        // Use submapper 1 to test mirroring control
        let mut mapper = CamericaMapper::new(
            MapperContext::new_for_test(71, prg_rom.clone(), vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        );

        mapper.write_prg(0xC000, 2); // Bank select at $C000+
        mapper.write_prg(0x9000, 0x10); // Mirroring at $8000-$9FFF (submapper 1)
        mapper.write_chr(0x0000, 0x5A);

        let regs = mapper.registers_snapshot();
        let chr = mapper.chr_ram_snapshot();

        let mut restored = CamericaMapper::new(
            MapperContext::new_for_test(71, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        );
        restored.restore_registers(&regs);
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.get_mirroring(), NametableLayout::SingleScreenUpper);
        assert_eq!(restored.read_chr(0x0000), 0x5A);
    }
}
