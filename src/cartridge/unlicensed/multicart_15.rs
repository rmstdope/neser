//! Mapper 15 - Multicart 100-in-1
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::BaseMapper;
use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;

#[cfg(test)]
const PRG_BANK_SIZE_8K: usize = 0x2000; // 8KB
#[cfg(test)]
const PRG_BANK_SIZE_16K: usize = 0x4000; // 16KB

/// Mapper 15 - 100-in-1 Contra Function
///
/// Hardware: Pirate multicart mapper with multiple banking modes
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_015>
/// - PRG-ROM: Up to 1MB with various banking modes
/// - PRG-RAM: None
/// - CHR: 8KB CHR-RAM (no CHR-ROM support)
/// - Mirroring: Programmable (horizontal, vertical, or one-screen)
///
/// Common boards: Various pirate multicart boards
///
/// Notes:
/// - Banking mode selected by low two address bits (SS)
/// - Mode 0 (`SS=0`): NROM-256 style (32KB bank)
/// - Mode 1 (`SS=1`): UNROM style
/// - Mode 2 (`SS=2`): NROM-64 style
/// - Mode 3 (`SS=3`): NROM-128 style (16KB mirrored)
/// - Bit 6 of data: mirroring control
/// - Used in various pirate multicarts (100-in-1, 168-in-1, etc.)
pub struct Multicart15Mapper {
    base: BaseMapper,
    bank_select: u8,
    sub_bank: u8,
    mode: u8,
    mirroring: NametableLayout,
}

impl Multicart15Mapper {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000); // 8KB
        let mut mapper = Self {
            base,
            bank_select: 0,
            sub_bank: 0,
            mode: 0,
            mirroring,
        };
        mapper.update_banks();
        mapper
    }

    fn get_prg_page_for_slot(&self, slot: u8) -> i16 {
        let sub_bank = self.sub_bank & 0x01;
        let base = self.bank_select.wrapping_shl(1);

        let page = match self.mode {
            0 => base.wrapping_add(slot) ^ sub_bank,
            1 | 3 => {
                let lower = base | sub_bank;
                if slot < 2 {
                    lower.wrapping_add(slot)
                } else {
                    let upper = if self.mode == 3 { lower } else { lower | 0x0E };
                    upper.wrapping_add(slot - 2)
                }
            }
            2 => base | sub_bank,
            _ => unreachable!("Invalid banking mode"),
        };
        page as i16
    }

    fn is_chr_ram_writable(&self) -> bool {
        matches!(self.mode, 1 | 2)
    }

    fn update_banks(&mut self) {
        for slot in 0..4 {
            let page = self.get_prg_page_for_slot(slot);
            self.base.select_prg_page(slot as usize, page);
        }
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for Multicart15Mapper {
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

        // Mapper control registers at $8000-$FFFF
        if addr >= 0x8000 {
            self.bank_select = value & 0x7F;
            self.sub_bank = value >> 7;
            self.mirroring = if (value & 0x40) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };

            // Set mode from low two address bits (SS)
            self.mode = (addr & 0x0003) as u8;
            self.update_banks();
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.is_chr_ram_writable() {
            self.base.write_chr(addr, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.bank_select,
            self.sub_bank,
            self.mode,
            self.mirroring.to_snapshot_byte(),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.bank_select = data[0];
            self.sub_bank = data[1];
            self.mode = data[2];
            self.mirroring = NametableLayout::from_snapshot_byte(data[3]);
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;

    fn create_test_prg_rom(num_16k_banks: usize) -> Vec<u8> {
        let mut prg_rom = vec![0; num_16k_banks * 16 * 1024];
        // Fill each 16KB bank with its bank number
        for bank in 0..num_16k_banks {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }
        prg_rom
    }

    fn create_test_prg_rom_8k(num_8k_banks: usize) -> Vec<u8> {
        let mut prg_rom = vec![0; num_8k_banks * PRG_BANK_SIZE_8K];
        for bank in 0..num_8k_banks {
            let start = bank * PRG_BANK_SIZE_8K;
            let end = start + PRG_BANK_SIZE_8K;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }
        prg_rom
    }

    #[test]
    fn test_multicart15_mode0_16kb_mirror() {
        // Mode 0: 8KB pages are sequential, with optional XOR by sub-bank bit
        let prg_rom = create_test_prg_rom_8k(32);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // mode 0 at SS=0, bank_select=2, sub-bank=0
        mapper.write_prg(0x8000, 2);

        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_multicart15_mode0_vectors_come_from_upper_16k_half() {
        let mut prg_rom = vec![0xFF; 4 * PRG_BANK_SIZE_16K];

        let upper_half_of_pair = 3 * PRG_BANK_SIZE_16K;
        prg_rom[upper_half_of_pair + 0x3FFA] = 0x34;
        prg_rom[upper_half_of_pair + 0x3FFB] = 0x12;
        prg_rom[upper_half_of_pair + 0x3FFC] = 0x78;
        prg_rom[upper_half_of_pair + 0x3FFD] = 0x56;

        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 2);

        assert_eq!(mapper.read_prg(0xFFFA), 0x34);
        assert_eq!(mapper.read_prg(0xFFFB), 0x12);
        assert_eq!(mapper.read_prg(0xFFFC), 0x78);
        assert_eq!(mapper.read_prg(0xFFFD), 0x56);
    }

    #[test]
    fn test_multicart15_mode1_32kb() {
        // Mode 1 (UNROM style): top 16KB in current 128KB window is fixed to pages 0x0E/0x0F
        let prg_rom = create_test_prg_rom_8k(64);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8001, 0);

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 14);
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    #[test]
    fn test_multicart15_mode2_8kb_banks() {
        // Mode 2 (NROM-64 style): all four 8KB slots map to one 8KB page
        let prg_rom = create_test_prg_rom_8k(64);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8002, 3);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 6);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 6);
    }

    #[test]
    fn test_multicart15_mode3_16kb_mirror() {
        let prg_rom = create_test_prg_rom_8k(32);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8003, 3);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 7);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_multicart15_mode3_vectors_read_from_selected_bank() {
        let mut prg_rom = vec![0xFF; 8 * PRG_BANK_SIZE_8K];

        // In mode 3 with value 0, vectors are read from 8KB page 1 at offset 0x1FFA-0x1FFD
        let page1 = PRG_BANK_SIZE_8K;
        prg_rom[page1 + 0x1FFA] = 0x34;
        prg_rom[page1 + 0x1FFB] = 0x12;
        prg_rom[page1 + 0x1FFC] = 0x78;
        prg_rom[page1 + 0x1FFD] = 0x56;

        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0x8003, 0);

        assert_eq!(mapper.read_prg(0xFFFA), 0x34);
        assert_eq!(mapper.read_prg(0xFFFB), 0x12);
        assert_eq!(mapper.read_prg(0xFFFC), 0x78);
        assert_eq!(mapper.read_prg(0xFFFD), 0x56);
    }

    #[test]
    fn test_multicart15_mirroring_control() {
        let prg_rom = create_test_prg_rom(4);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Initially horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Bit 6 = 0: vertical mirroring
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Bit 6 = 1: horizontal mirroring
        mapper.write_prg(0x8000, 0x40);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Test with mode 1
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8002, 0x40);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_multicart15_mirroring_uses_d6_bit() {
        let prg_rom = create_test_prg_rom(4);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8000, 0x40);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_multicart15_mode_decode_uses_low_two_address_bits() {
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.mode, 0);

        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.mode, 1);

        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.mode, 2);

        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.mode, 3);

        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.mode, 0);

        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.mode, 1);

        mapper.write_prg(0x8006, 0);
        assert_eq!(mapper.mode, 2);

        mapper.write_prg(0x8007, 0);
        assert_eq!(mapper.mode, 3);
    }

    #[test]
    fn test_multicart15_bank_select_masking() {
        // Test that bank selection wraps safely for high values
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0xFF);

        // Should wrap to available banks
        let value = mapper.read_prg(0x8000);
        assert!(value < 80); // Should be within available banks
    }

    #[test]
    fn test_multicart15_chr_ram() {
        // Multicart mappers typically use CHR-RAM
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        // Mode 1 enables CHR-RAM writes
        mapper.write_prg(0x8001, 0);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_multicart15_mode_switching() {
        // Test switching between modes
        let prg_rom = create_test_prg_rom(16);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Start in mode 0
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.mode, 0);
        let _val0 = mapper.read_prg(0x8000);

        // Switch to mode 1
        mapper.write_prg(0x8001, 2);
        assert_eq!(mapper.mode, 1);
        let _val1 = mapper.read_prg(0x8000);

        // Values should differ since banking modes are different
        // (This might not always be true depending on bank numbers, but demonstrates mode switch)

        // Switch to mode 2
        mapper.write_prg(0x8002, 3);
        assert_eq!(mapper.mode, 2);
    }

    #[test]
    fn test_multicart15_address_discrimination() {
        // Test that different addresses trigger different modes
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // SS = 0 -> mode 0
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.mode, 0);

        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.mode, 0);

        // SS = 1 -> mode 1
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.mode, 1);

        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.mode, 1);

        // SS = 2 -> mode 2
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.mode, 2);

        mapper.write_prg(0x8006, 0);
        assert_eq!(mapper.mode, 2);

        // SS = 3 -> mode 3
        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.mode, 3);

        mapper.write_prg(0x8007, 0);
        assert_eq!(mapper.mode, 3);
    }

    #[test]
    fn test_multicart15_large_rom() {
        // Test with a large ROM to ensure bank wrapping works
        let prg_rom = create_test_prg_rom(32); // 512KB
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Write to mode 0 with various bank numbers
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 20);
        assert_eq!(mapper.read_prg(0x8000), 200);
    }

    #[test]
    fn test_multicart15_registers_snapshot_restores_state() {
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom.clone(),
            vec![],
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x8002, 0x40 | 0x12); // mode 2, bank select 0x12, horizontal mirroring
        mapper.write_chr(0x0000, 0xAB);

        let regs = mapper.registers_snapshot();
        let chr = mapper.chr_ram_snapshot();

        let mut restored = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&regs);
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_chr(0x0000), 0xAB);
        assert_eq!(restored.mode, 2);
        assert_eq!(restored.bank_select, (0x40 | 0x12) & 0x7F);
    }

    #[test]
    fn test_multicart15_chr_ram_write_protected_in_modes_0_and_3() {
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0);
        mapper.write_chr(0x0000, 0xAA);
        assert_eq!(mapper.read_chr(0x0000), 0x00);

        mapper.write_prg(0x8003, 0);
        mapper.write_chr(0x0000, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    #[test]
    fn test_multicart15_chr_ram_write_enabled_in_modes_1_and_2() {
        let mut mapper = Multicart15Mapper::new(MapperContext::new_for_test(
            15,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8001, 0);
        mapper.write_chr(0x0000, 0xAA);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);

        mapper.write_prg(0x8002, 0);
        mapper.write_chr(0x0000, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0xBB);
    }
}
