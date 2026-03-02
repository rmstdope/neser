//! Mapper 10 - MMC4
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc2_mmc4_latch::{LatchTriggerMode, Mmc2Mmc4Latch};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 10 - MMC4 (FxROM boards)
///
/// Hardware: Similar to MMC2 but with 16KB PRG banking instead of 8KB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC4>
/// - Latch behavior: Same as MMC2, see <https://www.nesdev.org/wiki/MMC2#Latch_Behavior>
/// - PRG-ROM: 128KB or 256KB (16KB switchable + 16KB fixed)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR-ROM: 128KB with two 4KB regions controlled by PPU address latches
/// - Mirroring: Programmable (horizontal or vertical)
///
/// Common boards: NES-FxROM
///
/// Notes:
/// - Same CHR latch mechanism as MMC2 (FD/FE switching)
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - Last 16KB PRG bank fixed at $C000-$FFFF
/// - Used in Fire Emblem (Japan), Fire Emblem Gaiden (Japan)
pub struct MMC4Mapper {
    base: BaseMapper,

    // --- PRG banking ---
    prg_bank_16k: u8,

    // --- CHR banking + latches ---
    chr_latch: Mmc2Mmc4Latch,
}

impl MMC4Mapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 4,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(4 * 1024);
        // $8000: bank 0, $C000: last bank
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -1);
        // CHR: both latches FE (false), all bank regs = 0
        base.select_chr_page(0, 0);
        base.select_chr_page(1, 0);
        Self {
            base,
            prg_bank_16k: 0,
            chr_latch: Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc4),
        }
    }

    fn update_prg_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank_16k as i16);
        // $C000-$FFFF always fixed to last bank
    }

    fn update_chr_pages(&mut self) {
        let bank0 = self.chr_latch.selected_bank_0();
        let bank1 = self.chr_latch.selected_bank_1();
        self.base.select_chr_page(0, bank0 as i16);
        self.base.select_chr_page(1, bank1 as i16);
    }

    fn update_latches(&mut self, addr: u16) {
        self.chr_latch.update_for_chr_read(addr);
    }

    fn encode_mirroring(mirroring: NametableLayout) -> u8 {
        match mirroring {
            NametableLayout::Vertical => 0,
            NametableLayout::Horizontal => 1,
            _ => 1,
        }
    }

    fn decode_mirroring(value: u8) -> NametableLayout {
        match value {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            _ => NametableLayout::Horizontal,
        }
    }
}

impl Mapper for MMC4Mapper {
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
            0xA000..=0xAFFF => {
                self.prg_bank_16k = value & 0x0F;
                self.update_prg_banks();
            }
            0xB000..=0xBFFF => {
                self.chr_latch.bank_0_fd = value & 0x1F;
                self.update_chr_pages();
            }
            0xC000..=0xCFFF => {
                self.chr_latch.bank_0_fe = value & 0x1F;
                self.update_chr_pages();
            }
            0xD000..=0xDFFF => {
                self.chr_latch.bank_1_fd = value & 0x1F;
                self.update_chr_pages();
            }
            0xE000..=0xEFFF => {
                self.chr_latch.bank_1_fe = value & 0x1F;
                self.update_chr_pages();
            }
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
        // Read from current bank (pages already set from previous latch state)
        let value = self.base.read_chr_banked(addr);
        // Update latches after read, then sync pages for next access
        self.update_latches(addr);
        self.update_chr_pages();
        value
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank_16k,
            self.chr_latch.bank_0_fd,
            self.chr_latch.bank_0_fe,
            self.chr_latch.bank_1_fd,
            self.chr_latch.bank_1_fe,
            (self.chr_latch.latch0_is_fd as u8) | ((self.chr_latch.latch1_is_fd as u8) << 1),
            Self::encode_mirroring(self.base.mirroring()),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 7 {
            self.prg_bank_16k = data[0];
            self.chr_latch.bank_0_fd = data[1];
            self.chr_latch.bank_0_fe = data[2];
            self.chr_latch.bank_1_fd = data[3];
            self.chr_latch.bank_1_fe = data[4];
            self.chr_latch.latch0_is_fd = (data[5] & 1) != 0;
            self.chr_latch.latch1_is_fd = (data[5] & 2) != 0;
            self.base.set_mirroring(Self::decode_mirroring(data[6]));
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
    fn test_mmc4_prg_bank_8000_is_switchable_and_upper_bank_fixed() {
        let prg_banks = 4;
        let prg_rom = filled_banks(0x4000, prg_banks);
        let chr_rom = filled_banks(0x1000, 8);

        let mut mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Power-on: bank 0 at $8000.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Fixed region should map to last bank.
        assert_eq!(mapper.read_prg(0xC000), (prg_banks - 1) as u8);
        assert_eq!(mapper.read_prg(0xFFFF), (prg_banks - 1) as u8);

        // Switch $8000-$BFFF bank via $A000.
        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0xA999, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn test_mmc4_chr_latches_select_between_fd_and_fe_banks() {
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x4000, 4);

        let mut mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Configure banks.
        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE
        mapper.write_prg(0xD000, 3); // high FD
        mapper.write_prg(0xE000, 4); // high FE

        // Low region latch: FD (triggered by CHR read, takes effect after that fetch)
        let _ = mapper.read_chr(0x0FD8);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Low region latch: FE
        let _ = mapper.read_chr(0x0FE8);
        assert_eq!(mapper.read_chr(0x0000), 2);

        // High region latch: FD
        let _ = mapper.read_chr(0x1FD8);
        assert_eq!(mapper.read_chr(0x1000), 3);

        // High region latch: FE
        let _ = mapper.read_chr(0x1FE8);
        assert_eq!(mapper.read_chr(0x1000), 4);
    }

    #[test]
    fn test_mmc4_registers_snapshot_preserves_latches_and_mirroring() {
        let prg_rom = filled_banks(0x4000, 4);
        let chr_rom = filled_banks(0x1000, 8);

        let mut mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
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

        let _ = mapper.read_chr(0x0FD8); // latch0 FD after fetch
        let _ = mapper.read_chr(0x1FE8); // latch1 FE after fetch

        let saved = mapper.registers_snapshot();

        let mut restored = MMC4Mapper::new(MapperContext::new_for_test(
            10,
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
    fn test_mmc4_open_bus() {
        let mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x33), 0x33);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x44), 0x44);
    }

    #[test]
    fn test_mmc4_trigger_read_uses_previous_bank_then_latch_updates() {
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x4000, 4);

        let mut mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE

        // Ensure low latch is FE before triggering FD tile read.
        let _ = mapper.read_chr(0x0FE8);

        // Per MMC4 spec, the triggering $0FD8 fetch itself must still use old FE bank.
        assert_eq!(mapper.read_chr(0x0FD8), 2);

        // Latch effect applies after that fetch: subsequent low-region reads use FD bank.
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn test_mmc4_write_chr_does_not_update_latch_state() {
        let chr_rom = filled_banks(0x1000, 8);
        let prg_rom = filled_banks(0x4000, 4);

        let mut mapper = MMC4Mapper::new(MapperContext::new_for_test(
            10,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE

        // Set low latch to FE via triggering read.
        let _ = mapper.read_chr(0x0FE8);
        assert_eq!(mapper.read_chr(0x0000), 2);

        // CHR writes must not change latch state.
        mapper.write_chr(0x0FD8, 0xAA);
        assert_eq!(mapper.read_chr(0x0000), 2);
    }
}
