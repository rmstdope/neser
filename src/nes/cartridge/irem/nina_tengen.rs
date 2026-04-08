//! Mappers 34/64 variants - NINA / Tengen support
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::Mapper;
use crate::nes::cartridge::MapperCapabilities;
use crate::nes::cartridge::NametableLayout;

/// Mapper 78 - Irem Holy Diver / Jaleco JF-16
///
/// Hardware: Two different board types sharing the same mapper number
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_078>
/// - Irem boards: <https://www.nesdev.org/wiki/INES_Mapper_078#Irem_boards>
/// - Jaleco boards: <https://www.nesdev.org/wiki/INES_Mapper_078#Jaleco_boards>
/// - PRG-ROM: Up to 128KB (8 16KB banks)
/// - PRG-RAM: None
/// - CHR-ROM: Up to 128KB (16 8KB banks)
/// - Mirroring: Programmable (horizontal or vertical via register)
///
/// Common boards: Irem 74HC161/32, Jaleco JF-16
///
/// Register at $8000-$FFFF (any write):
/// - Bits 0-2: Select 16KB PRG bank at $8000-$BFFF
/// - Bit 3: Mirroring (0 = vertical, 1 = horizontal)
/// - Bits 4-7: Select 8KB CHR bank
///
/// Notes:
/// - Last 16KB PRG bank always fixed at $C000-$FFFF
/// - Used in Tengen unlicensed games (due to similar design to NINA-03/06)
/// - Games: Holy Diver (Irem), Uchuusen: Cosmo Carrier (Irem)
/// - Also used by Tengen: Pac-Man, RBI Baseball, Tetris (unlicensed)
pub struct NinaTengenMapper {
    base: BaseMapper,
    prg_bank_select: u8,
    chr_bank_select: u8,
    mirroring: NametableLayout,
}

impl NinaTengenMapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x4000); // 16KB
        base.configure_chr_banking(0x2000); // 8KB
        let mut mapper = Self {
            base,
            prg_bank_select: 0,
            chr_bank_select: 0,
            mirroring,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank_select as i16);
        self.base.select_prg_page(1, -1); // Fixed last bank
        self.base.select_chr_page(0, self.chr_bank_select as i16);
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for NinaTengenMapper {
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

        // Any write to $8000-$FFFF sets the bank register
        if (0x8000..=0xFFFF).contains(&addr) {
            // Bits 0-2: PRG bank select
            self.prg_bank_select = value & 0x07;

            // Bit 3: Mirroring (0=vertical, 1=horizontal)
            self.mirroring = if (value & 0x08) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };

            // Bits 4-7: CHR bank select
            self.chr_bank_select = (value >> 4) & 0x0F;

            self.update_banks();
        }
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // Mapper 78 uses CHR-ROM, writes are ignored
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank_select,
            self.chr_bank_select,
            self.mirroring.to_snapshot_byte(),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank_select = data[0];
            self.chr_bank_select = data[1];
            self.mirroring = NametableLayout::from_snapshot_byte(data[2]);
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::MapperContext;

    #[test]
    fn test_nina_tengen_prg_bank_switching() {
        // Create 128KB (8 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom,
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        // Initially bank 0 at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xBFFF), 0);

        // Last bank (7) always at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 70);
        assert_eq!(mapper.read_prg(0xFFFF), 70);

        // Switch to bank 1 (bits 0-2)
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xBFFF), 10);

        // Switch to bank 5 (bits 0-2)
        mapper.write_prg(0x8000, 0b0000_0101);
        assert_eq!(mapper.read_prg(0x8000), 50);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 70);
    }

    #[test]
    fn test_nina_tengen_chr_bank_switching() {
        // Create 128KB (16 banks of 8KB) CHR ROM
        let mut chr_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 15) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            vec![0; 128 * 1024],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);

        // Switch to bank 1 (bits 4-7)
        mapper.write_prg(0x8000, 0b0001_0000);
        assert_eq!(mapper.read_chr(0x0000), 15);

        // Switch to bank 5 (bits 4-7)
        mapper.write_prg(0x8000, 0b0101_0000);
        assert_eq!(mapper.read_chr(0x0000), 75);

        // Switch to bank 15 (bits 4-7)
        mapper.write_prg(0x8000, 0b1111_0000);
        assert_eq!(mapper.read_chr(0x0000), 225);
    }

    #[test]
    fn test_nina_tengen_mirroring_control() {
        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        // Initially horizontal (from constructor)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Bit 3 = 0: vertical mirroring
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Bit 3 = 1: horizontal mirroring
        mapper.write_prg(0x8000, 0b0000_1000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Test with other bits set
        mapper.write_prg(0x8000, 0b1111_0111); // Bit 3 = 0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8000, 0b1111_1111); // Bit 3 = 1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_nina_tengen_combined_register() {
        // Test that all register bits work together
        let mut prg_rom = vec![0; 128 * 1024];
        let mut chr_rom = vec![0; 128 * 1024];

        // Fill PRG banks
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        // Fill CHR banks
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 200) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Write combined register: PRG=3, Mirroring=Vertical, CHR=7
        // Binary: 0111_0011 (CHR=7, Mir=0, PRG=3)
        mapper.write_prg(0x8000, 0b0111_0011);

        assert_eq!(mapper.read_prg(0x8000), 103); // PRG bank 3
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical); // Bit 3 = 0
        assert_eq!(mapper.read_chr(0x0000), 207); // CHR bank 7

        // Write another combined register: PRG=5, Mirroring=Horizontal, CHR=10
        // Binary: 1010_1101 (CHR=10, Mir=1, PRG=5)
        mapper.write_prg(0x8000, 0b1010_1101);

        assert_eq!(mapper.read_prg(0x8000), 105); // PRG bank 5
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal); // Bit 3 = 1
        assert_eq!(mapper.read_chr(0x0000), 210); // CHR bank 10
    }

    #[test]
    fn test_nina_tengen_prg_bank_mask() {
        // Test that only bits 0-2 affect PRG banking
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 25) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom,
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));

        // Write with upper bits set - should only use lower 3 bits
        mapper.write_prg(0x8000, 0b1111_1111); // PRG bank = 7
        assert_eq!(mapper.read_prg(0x8000), 175); // Bank 7

        mapper.write_prg(0x8000, 0b1111_1000); // PRG bank = 0
        assert_eq!(mapper.read_prg(0x8000), 0); // Bank 0
    }

    #[test]
    fn test_nina_tengen_chr_bank_mask() {
        // Test that only bits 4-7 affect CHR banking
        let mut chr_rom = vec![0; 128 * 1024];
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 12) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            vec![0; 32 * 1024],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Write with lower bits set - should only use bits 4-7
        mapper.write_prg(0x8000, 0b1111_0000); // CHR bank = 15
        assert_eq!(mapper.read_chr(0x0000), 180); // Bank 15

        mapper.write_prg(0x8000, 0b0000_0000); // CHR bank = 0
        assert_eq!(mapper.read_chr(0x0000), 0); // Bank 0
    }

    #[test]
    fn test_nina_tengen_fixed_last_prg_bank() {
        // Verify that $C000-$FFFF is always the last bank
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom,
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));

        // Last bank should always read 57 (bank 7 + 50)
        assert_eq!(mapper.read_prg(0xC000), 57);

        // Switch banks several times
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0xC000), 57);

        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0xC000), 57);

        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0xC000), 57);
    }

    #[test]
    fn test_nina_tengen_chr_rom_read_only() {
        // CHR ROM should not be writable
        let chr_rom = vec![0xAA; 8 * 1024];
        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
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
    fn test_nina_tengen_registers_snapshot_restores_banks_and_mirroring() {
        let mut prg_rom = vec![0; 128 * 1024];
        let mut chr_rom = vec![0; 128 * 1024];

        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 20) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // PRG=5, Mirroring=Horizontal, CHR=9
        mapper.write_prg(0x8000, 0b1001_1101);

        let snapshot = mapper.registers_snapshot();

        let mut restored = NinaTengenMapper::new(MapperContext::new_for_test(
            78,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 15);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_chr(0x0000), 29);
    }

    #[test]
    fn test_nina_tengen_banked_rom_replacement() {
        use crate::nes::cartridge::common::BankedRom;
        use crate::nes::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 0x4000; // 16KB
        const CHR_BANK_SIZE: usize = 0x2000; // 8KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 8);
        let chr_rom = banked_data(CHR_BANK_SIZE, 16);

        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom, CHR_BANK_SIZE);

        // Test PRG bank reading
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(7, 0), 7);

        // Test CHR bank reading
        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(15, 0), 15);

        // Test last bank wrapping
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to 0
        assert_eq!(chr_banked.read(16, 0), 0); // wraps to 0
    }
}
