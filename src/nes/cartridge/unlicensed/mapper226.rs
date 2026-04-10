//! Mapper 226 – BMC 76-in-1 / Super 42-in-1
//!
//! Specifications:
//! - Primary: NesDev wiki (backup mirror)
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_226.xhtml>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper226.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Mapper226.h>
//!
//! # Hardware overview
//!
//! Used by BMC multicart boards (76-in-1, Super 42-in-1).
//!
//! - PRG-ROM: 2 × 16 KiB windows at `$8000–$FFFF`.
//! - CHR-ROM/RAM: 8 KiB, bank 0 (fixed / unbanked).
//! - Mirroring: dynamic, controlled by a register bit.
//! - IRQ: none.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register map
//!
//! Writes to `$8000–$FFFF`; register selected by the low address bit
//! (`addr & 1`, i.e. even/odd addresses). Within this decoded range, this is
//! equivalent to `addr & 0x8001`:
//!
//! | Effective address | Register | Bit layout          |
//! |-------------------|----------|---------------------|
//! | `$8000`           | reg[0]   | `[P M O P P P P P]` |
//! | `$8001`           | reg[1]   | `[. . . . . . . H]` |
//!
//! Bit definitions for `reg[0]`:
//! - Bits `[4:0]` = PRG bits `[4:0]`
//! - Bit  `5`    = O: PRG mode (0 = 32 KB, 1 = 16 KB mirrored)
//! - Bit  `6`    = M: Mirroring (0 = Horizontal, 1 = Vertical)
//! - Bit  `7`    = PRG bit 5
//!
//! Bit definitions for `reg[1]`:
//! - Bit  `0`    = H: PRG bit 6
//!
//! # PRG banking
//!
//! 7-bit PRG page = `(reg[0] & 0x1F) | ((reg[0] & 0x80) >> 2) | ((reg[1] & 0x01) << 6)`
//!
//! - PRG mode 0 (O=0, 32 KB): `$8000` = `page & 0xFE`, `$C000` = `(page & 0xFE) + 1`
//! - PRG mode 1 (O=1, 16 KB mirrored): both windows = `page`
//!
//! # Memory map
//!
//! ```text
//! CPU $8000–$BFFF  PRG-ROM 16 KiB, lower window
//! CPU $C000–$FFFF  PRG-ROM 16 KiB, upper window
//! PPU $0000–$1FFF  CHR 8 KiB bank 0 (fixed)
//! ```
//!
//! # Power-on / reset state
//!
//! Both registers zero: PRG mode 0, page 0 → `$8000` = bank 0, `$C000` = bank 1,
//! Horizontal mirroring.
//! The multicart clears both registers on soft reset.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 226;
const NUM_REGS: usize = 2;

/// Mapper 226 – BMC 76-in-1 / Super 42-in-1
pub struct Mapper226 {
    base: BaseMapper,
    regs: [u8; NUM_REGS],
}

impl Mapper226 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);

        let mut mapper = Self {
            base,
            regs: [0; NUM_REGS],
        };
        mapper.apply_state();
        mapper
    }

    fn prg_page(&self) -> i16 {
        let low5 = (self.regs[0] & 0x1F) as i16;
        let bit5 = ((self.regs[0] & 0x80) >> 2) as i16;
        let bit6 = ((self.regs[1] & 0x01) << 6) as i16;
        low5 | bit5 | bit6
    }

    fn apply_state(&mut self) {
        let page = self.prg_page();
        let prg_mode = (self.regs[0] & 0x20) != 0;

        if prg_mode {
            // 16 KB mirrored: both windows point to the same page
            self.base.select_prg_page(0, page);
            self.base.select_prg_page(1, page);
        } else {
            // 32 KB: lower = page & 0xFE, upper = (page & 0xFE) + 1
            let even = page & !1;
            self.base.select_prg_page(0, even);
            self.base.select_prg_page(1, even + 1);
        }

        let vertical = (self.regs[0] & 0x40) != 0;
        self.base.set_mirroring(if vertical {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        });
    }
}

impl Mapper for Mapper226 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0x8000 {
            let reg_idx = (addr & 0x0001) as usize;
            self.regs[reg_idx] = value;
            self.apply_state();
        }
    }

    fn reset(&mut self) {
        self.regs = [0; NUM_REGS];
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = self.base.banking_snapshot();
        data.extend_from_slice(&self.regs);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_size = self.base.banking_snapshot().len();
        self.base.restore_banking(data);
        if data.len() >= banking_size + NUM_REGS {
            self.regs
                .copy_from_slice(&data[banking_size..banking_size + NUM_REGS]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Use a non-power-of-two bank count to catch modulo-wrap false passes.
    /// 130 × 16 KiB covers the full 7-bit page index range (0..=127) while staying
    /// non-power-of-two so coincidental wrapping cannot mask bugs.
    const PRG_BANKS: usize = 130;

    fn make_mapper(prg_rom: Vec<u8>) -> Mapper226 {
        Mapper226::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                prg_rom,
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    fn make_default_mapper() -> Mapper226 {
        make_mapper(banked_data(16 * 1024, PRG_BANKS))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_226_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(16 * 1024, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "mapper 226 should be registered");
        assert_eq!(result.unwrap().mapper_number(), MAPPER_NUMBER);
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_lower_window_is_bank_0() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should read bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_upper_window_is_bank_1() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should read bank 1 on power-on (32KB mode)"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be Horizontal on power-on (reg0=0)"
        );
    }

    // ── Register selection via addr & 1 (even/odd addresses) ─────────────────

    #[test]
    fn write_8000_updates_reg0() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x05: bits[4:0]=5, O=0, M=0, bit7=0 → page=5, mode0
        // lower = 5 & 0xFE = 4, upper = 5
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 should be bank 4 (5&0xFE)"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5");
    }

    #[test]
    fn write_8001_updates_reg1_extending_prg_to_bit6() {
        let mut mapper = make_default_mapper();
        // reg1 bit 0 = 1 → PRG bit6 = 1 → base page = 64
        // reg0 = 0x00, reg1 = 0x01: page = 64, mode0 → lower=64, upper=65
        mapper.write_prg(0x8001, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 64, "$8000 should be bank 64");
        assert_eq!(mapper.read_prg(0xC000), 65, "$C000 should be bank 65");
    }

    #[test]
    fn write_address_upper_range_still_selects_correct_register() {
        let mut mapper = make_default_mapper();
        // Write to $E000 → reg0 (E000 & 1 = 0)
        mapper.write_prg(0xE000, 0x07);
        // page=7, mode0: lower=6, upper=7
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn write_address_odd_selects_reg1() {
        let mut mapper = make_default_mapper();
        // $FFFF & 1 = 1 → reg1
        mapper.write_prg(0xFFFF, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 64);
    }

    // ── PRG mode 0: 32 KB (O=0) ───────────────────────────────────────────────

    #[test]
    fn mode0_selects_even_lower_odd_upper_bank() {
        let mut mapper = make_default_mapper();
        // page=9, mode0: lower=8 (9&0xFE), upper=9
        mapper.write_prg(0x8000, 0x09);
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 = bank 8 (9&0xFE)");
        assert_eq!(mapper.read_prg(0xC000), 9, "$C000 = bank 9");
    }

    #[test]
    fn mode0_even_page_stays_aligned() {
        let mut mapper = make_default_mapper();
        // page=8, mode0: lower=8, upper=9
        mapper.write_prg(0x8000, 0x08);
        assert_eq!(mapper.read_prg(0x8000), 8);
        assert_eq!(mapper.read_prg(0xC000), 9);
    }

    // ── PRG mode 1: 16 KB mirrored (O=1) ─────────────────────────────────────

    #[test]
    fn mode1_both_windows_same_bank() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x25: bits[4:0]=5, O=1 (bit5=1), M=0, bit7=0 → page=5, mode1
        mapper.write_prg(0x8000, 0x25);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 = bank 5 (mode1)");
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 = bank 5 (mode1, mirrored)"
        );
    }

    #[test]
    fn mode1_odd_page_not_aligned() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x29: bits[4:0]=9, O=1 (bit5=1) → page=9, mode1
        mapper.write_prg(0x8000, 0x29);
        assert_eq!(mapper.read_prg(0x8000), 9);
        assert_eq!(
            mapper.read_prg(0xC000),
            9,
            "$C000 should mirror lower in mode1"
        );
    }

    // ── PRG bit 5 from reg0[7] ────────────────────────────────────────────────

    #[test]
    fn reg0_bit7_sets_prg_bit5() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x80: bit7=1, bits[4:0]=0 → page = 0 | (0x80>>2) = 32
        // mode0: lower=32, upper=33
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.read_prg(0x8000), 32, "bit7 of reg0 sets prg bit5");
        assert_eq!(mapper.read_prg(0xC000), 33);
    }

    #[test]
    fn reg0_bit7_and_low_bits_combine() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x83: bit7=1, bits[4:0]=3 → page = 3 | 32 = 35
        // mode0: lower=34, upper=35
        mapper.write_prg(0x8000, 0x83);
        assert_eq!(mapper.read_prg(0x8000), 34);
        assert_eq!(mapper.read_prg(0xC000), 35);
    }

    // ── Full 7-bit page combining all sources ─────────────────────────────────

    #[test]
    fn all_three_prg_sources_combine_correctly() {
        let mut mapper = make_default_mapper();
        // reg0 = 0x9F: bit7=1 (bit5), O=0, bits[4:0]=0x1F → low5=31, bit5=32
        // reg1 = 0x01: bit0=1 → bit6=64
        // page = 31 | 32 | 64 = 127, mode0: lower=126, upper=127
        mapper.write_prg(0x8000, 0x9F);
        mapper.write_prg(0x8001, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            126,
            "all PRG bits combined: bank 126"
        );
        assert_eq!(mapper.read_prg(0xC000), 127, "upper: bank 127");
    }

    // ── Mirroring control ─────────────────────────────────────────────────────

    #[test]
    fn reg0_bit6_zero_gives_horizontal_mirroring() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // bit6=0 → Horizontal
        assert_eq!(mapper.base().mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn reg0_bit6_one_gives_vertical_mirroring() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x40); // bit6=1 → Vertical
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_toggles_independently_of_prg_banking() {
        let mut mapper = make_default_mapper();
        // Set page=5, vertical
        mapper.write_prg(0x8000, 0x45); // bits[4:0]=5, bit6=1 (vertical), O=0
        // lower = 4, upper = 5
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    // ── Reset (soft) ─────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_both_registers_and_restores_power_on_state() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x45); // page=5, vertical, mode0
        mapper.write_prg(0x8001, 0x01); // PRG bit6 = 1

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower window bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "upper window bank 1 after reset (32KB mode, page 0)"
        );
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "horizontal mirroring after reset"
        );
    }

    // ── Save-state snapshot / restore ────────────────────────────────────────

    #[test]
    fn snapshot_and_restore_preserves_prg_and_mirroring() {
        let mut mapper = make_default_mapper();
        // reg0=0x69 = 0110_1001: bits[4:0]=9, O=1 (bit5, mode1), M=1 (bit6, vertical), bit7=0
        // reg1=0x01: bit0=1 → PRG bit6=64
        // page = 9 | 64 = 73, mode1: both windows = 73
        mapper.write_prg(0x8000, 0x69);
        mapper.write_prg(0x8001, 0x01);

        let snap = mapper.registers_snapshot();

        let mut restored = make_default_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            73,
            "lower window restored to bank 73"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            73,
            "upper window restored to bank 73 (mode1)"
        );
    }
}
