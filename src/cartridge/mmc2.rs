//! Mapper 9 - MMC2
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::ines::ConsoleType;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 9 - MMC2 (PNROM boards)
///
/// Hardware: Nintendo's PPU-triggered CHR banking used exclusively by Punch-Out!!
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC2>
/// - Latch behavior: <https://www.nesdev.org/wiki/MMC2#Latch_Behavior>
/// - PRG-ROM: 128KB (8KB switchable + 24KB fixed)
/// - PRG-RAM: None
/// - CHR-ROM: 128KB with two 4KB regions controlled by PPU address latches
/// - Mirroring: Programmable (horizontal or vertical)
///
/// Common boards: NES-PNROM
///
/// Notes:
/// - Two independent CHR latches triggered by PPU reads:
///   - Latch 0: $0FD8-$0FDF (FD) or $0FE8-$0FEF (FE) switches $0000-$0FFF
///   - Latch 1: $1FD8-$1FDF (FD) or $1FE8-$1FEF (FE) switches $1000-$1FFF
/// - Latch state determines which of two 4KB CHR banks is active per region
/// - Used exclusively in (Mike Tyson's) Punch-Out!!
pub struct MMC2Mapper {
    base: BaseMapper,

    // --- PRG banking ---
    prg_bank_8k: u8,

    // --- CHR banking + latches ---
    chr_bank_0_fd: u8,
    chr_bank_0_fe: u8,
    chr_bank_1_fd: u8,
    chr_bank_1_fe: u8,

    latch0_is_fd: bool,
    latch1_is_fd: bool,
}

impl MMC2Mapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let has_prg_ram = matches!(ctx.console_type, ConsoleType::Playchoice10);
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: if has_prg_ram { 8 } else { 0 },
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(4 * 1024);
        // $8000: bank 0, $A000/$C000/$E000: fixed last 3 banks
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -3);
        base.select_prg_page(2, -2);
        base.select_prg_page(3, -1);
        // CHR: both latches FE (false), all bank regs = 0
        base.select_chr_page(0, 0);
        base.select_chr_page(1, 0);
        Self {
            base,
            prg_bank_8k: 0,
            chr_bank_0_fd: 0,
            chr_bank_0_fe: 0,
            chr_bank_1_fd: 0,
            chr_bank_1_fe: 0,
            latch0_is_fd: false,
            latch1_is_fd: false,
        }
    }

    fn update_prg_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank_8k as i16);
        // Slots 1-3 always fixed to last 3 banks
    }

    fn update_chr_pages(&mut self) {
        let bank0 = if self.latch0_is_fd {
            self.chr_bank_0_fd
        } else {
            self.chr_bank_0_fe
        };
        let bank1 = if self.latch1_is_fd {
            self.chr_bank_1_fd
        } else {
            self.chr_bank_1_fe
        };
        self.base.select_chr_page(0, bank0 as i16);
        self.base.select_chr_page(1, bank1 as i16);
    }

    fn update_latches_for_chr_read(&mut self, addr: u16) {
        match addr {
            0x0FD8 => self.latch0_is_fd = true,
            0x0FE8 => self.latch0_is_fd = false,
            0x1FD8..=0x1FDF => self.latch1_is_fd = true,
            0x1FE8..=0x1FEF => self.latch1_is_fd = false,
            _ => {}
        }
    }
}

impl Mapper for MMC2Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.base.try_read_prg_ram(addr) {
            return value;
        }
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        match addr {
            // PRG bank select ($A000-$AFFF)
            0xA000..=0xAFFF => {
                self.prg_bank_8k = value & 0x0F;
                self.update_prg_banks();
            }

            // CHR bank registers
            0xB000..=0xBFFF => {
                self.chr_bank_0_fd = value & 0x1F;
                self.update_chr_pages();
            }
            0xC000..=0xCFFF => {
                self.chr_bank_0_fe = value & 0x1F;
                self.update_chr_pages();
            }
            0xD000..=0xDFFF => {
                self.chr_bank_1_fd = value & 0x1F;
                self.update_chr_pages();
            }
            0xE000..=0xEFFF => {
                self.chr_bank_1_fe = value & 0x1F;
                self.update_chr_pages();
            }

            // Mirroring control ($F000-$FFFF)
            0xF000..=0xFFFF => {
                let mirroring = if (value & 0x01) != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                };
                self.base.set_mirroring(mirroring);
            }

            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let value = self.base.read_chr_banked(addr);
        self.update_latches_for_chr_read(addr);
        self.update_chr_pages();
        value
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        let _ = addr;
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank_8k,
            self.chr_bank_0_fd,
            self.chr_bank_0_fe,
            self.chr_bank_1_fd,
            self.chr_bank_1_fe,
            (self.latch0_is_fd as u8) | ((self.latch1_is_fd as u8) << 1),
            match self.base.mirroring() {
                NametableLayout::Vertical => 0,
                NametableLayout::Horizontal => 1,
                _ => 1,
            },
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 7 {
            self.prg_bank_8k = data[0];
            self.chr_bank_0_fd = data[1];
            self.chr_bank_0_fe = data[2];
            self.chr_bank_1_fd = data[3];
            self.chr_bank_1_fe = data[4];
            self.latch0_is_fd = (data[5] & 1) != 0;
            self.latch1_is_fd = (data[5] & 2) != 0;
            let mirroring = match data[6] {
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                _ => NametableLayout::Horizontal,
            };
            self.base.set_mirroring(mirroring);
            self.update_prg_banks();
            self.update_chr_pages();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;

    fn filled_banks(bank_size: usize, banks: usize) -> Vec<u8> {
        (0..banks)
            .flat_map(|bank| vec![bank as u8; bank_size])
            .collect()
    }

    #[test]
    fn test_mmc2_prg_bank_8000_is_switchable_and_upper_banks_are_fixed() {
        let prg_banks = 8;
        let prg_rom = filled_banks(0x2000, prg_banks);
        let chr_rom = filled_banks(0x1000, 8);

        let mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Power-on: bank 0 at $8000.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Fixed region should map to last 3 banks.
        assert_eq!(mapper.read_prg(0xA000), (prg_banks - 3) as u8);
        assert_eq!(mapper.read_prg(0xC000), (prg_banks - 2) as u8);
        assert_eq!(mapper.read_prg(0xE000), (prg_banks - 1) as u8);
    }

    #[test]
    fn test_mmc2_registers_snapshot_preserves_latches_and_mirroring() {
        let prg_rom = filled_banks(0x2000, 4);
        let chr_rom = filled_banks(0x1000, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xA000, 0x03); // PRG bank
        mapper.write_prg(0xB000, 0x01); // CHR 0 FD
        mapper.write_prg(0xC000, 0x02); // CHR 0 FE
        mapper.write_prg(0xD000, 0x03); // CHR 1 FD
        mapper.write_prg(0xE000, 0x04); // CHR 1 FE
        mapper.write_prg(0xF000, 0x01); // Mirroring horizontal

        mapper.read_chr(0x0FD8); // latch0 FD after read
        mapper.read_chr(0x1FE8); // latch1 FE after read

        let saved = mapper.registers_snapshot();

        let mut restored = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&saved);

        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 1);
        assert_eq!(restored.read_chr(0x1000), 4);
    }

    #[test]
    fn test_mmc2_chr_latches_select_between_fd_and_fe_banks() {
        // Provide at least 6 4KB banks.
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x2000, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Configure banks.
        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE
        mapper.write_prg(0xD000, 3); // high FD
        mapper.write_prg(0xE000, 4); // high FE

        // Triggering read uses old bank for that fetch, then switches latch.
        assert_eq!(mapper.read_chr(0x0FD8), 2);
        assert_eq!(mapper.read_chr(0x0000), 1);

        assert_eq!(mapper.read_chr(0x0FE8), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);

        assert_eq!(mapper.read_chr(0x1FD8), 4);
        assert_eq!(mapper.read_chr(0x1000), 3);

        assert_eq!(mapper.read_chr(0x1FE8), 3);
        assert_eq!(mapper.read_chr(0x1000), 4);
    }

    #[test]
    fn test_mmc2_latch0_only_switches_on_exact_addresses() {
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x2000, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE

        // Power-on FE bank selected.
        assert_eq!(mapper.read_chr(0x0000), 2);

        // Neighbor of FD trigger must not switch latch0.
        assert_eq!(mapper.read_chr(0x0FDF), 2);
        assert_eq!(mapper.read_chr(0x0000), 2);

        // Exact FD trigger should switch to FD for subsequent reads.
        assert_eq!(mapper.read_chr(0x0FD8), 2);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Neighbor of FE trigger must not switch latch0.
        assert_eq!(mapper.read_chr(0x0FEF), 1);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Exact FE trigger should switch to FE for subsequent reads.
        assert_eq!(mapper.read_chr(0x0FE8), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn test_mmc2_ppu_address_changed_does_not_switch_latches() {
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x2000, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1);
        mapper.write_prg(0xC000, 2);

        // Power-on FE bank selected.
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.ppu_address_changed(0x0FD8);

        // Still FE because address bus activity alone must not switch latch.
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn test_mmc2_open_bus() {
        let mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x11), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x22), 0x22);
    }

    #[test]
    fn test_mmc2_standard_board_has_no_prg_ram_window() {
        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x6000, 0xA5);

        assert_eq!(mapper.wram_size(), 0);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0x3C), 0x3C);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, 0x7E), 0x7E);
    }
}
