//! Mapper 227 – BMC (unlicensed multicart)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper227.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 227;

/// Mapper 227 – BMC unlicensed multicart
///
/// All bank-switching is controlled by the write ADDRESS (data byte is ignored).
/// A write to any address in `$8000–$FFFF` updates PRG banking and mirroring.
///
/// ## Register decoding (write address fields)
///
/// ```text
/// Bit  A9  A8  A7  A6..A2  A1  A0
///      L   P5  M   P4..P0  H   S
/// ```
/// - `P5..P0` = 6-bit PRG bank: `((addr >> 2) & 0x1F) | ((addr & 0x100) >> 3)`
/// - `S` = mode-S flag (A0): selects 32 KB aligned vs independent window behaviour
/// - `H` = mirroring (A1): 0 = Vertical, 1 = Horizontal
/// - `M` = mode-M flag (A7): 0 = outer/inner mode, 1 = 32 KB mode
/// - `L` = latch (A9): upper-window latch in outer mode
///
/// ## PRG banking (2×16 KB windows)
///
/// Four banking modes selected by M and S:
///
/// | M | S | `$8000` window     | `$C000` window       |
/// |---|---|--------------------|----------------------|
/// | 1 | 1 | `bank & 0xFE`      | `(bank & 0xFE) + 1`  |
/// | 1 | 0 | `bank`             | `bank`               |
/// | 0 | 1 | `bank & 0x3E`      | `(bank & 0x38) | L ? 0x07 : 0x00` (see spec) |
/// | 0 | 0 | `bank`             | `(bank & 0x38) | L ? 0x07 : 0x00` (see spec) |
///
/// Full formula for M=0:
/// - lower = `if S { bank & 0x3E } else { bank }`
/// - upper = `if L { bank | 0x07 } else { bank & 0x38 }`  (S does not affect upper in M=0)
///
/// Wait, re-reading the Mesen2 source more carefully:
/// - M=0, S=1, L=1: lo = bank & 0x3E, hi = bank | 0x07
/// - M=0, S=1, L=0: lo = bank & 0x3E, hi = bank & 0x38
/// - M=0, S=0, L=1: lo = bank,         hi = bank | 0x07
/// - M=0, S=0, L=0: lo = bank,         hi = bank & 0x38
///
/// ## CHR (8 KB)
///
/// Single 8 KB CHR window fixed to bank 0 (no CHR banking).
///
/// ## Mirroring
///
/// Controlled by address bit A1 (0 = Vertical, 1 = Horizontal).
///
/// ## Power-on / Reset
///
/// Writing 0 to `$8000` on init: both PRG windows at bank 0, Vertical mirroring.
pub struct Mapper227 {
    base: BaseMapper,
}

impl Mapper227 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        let mut mapper = Self { base };
        mapper.apply_register(0x8000);
        mapper
    }

    fn apply_register(&mut self, addr: u16) {
        let prg_bank = (((addr >> 2) & 0x1F) | ((addr & 0x100) >> 3)) as i16;
        let s_flag = (addr & 0x01) != 0;
        let l_flag = ((addr >> 9) & 0x01) != 0;
        let prg_mode = ((addr >> 7) & 0x01) != 0;
        let horizontal = (addr & 0x02) != 0;

        let (lo, hi) = if prg_mode {
            if s_flag {
                (prg_bank & !1, (prg_bank & !1) + 1)
            } else {
                (prg_bank, prg_bank)
            }
        } else {
            let lo = if s_flag { prg_bank & 0x3E } else { prg_bank };
            let hi = if l_flag {
                prg_bank | 0x07
            } else {
                prg_bank & 0x38
            };
            (lo, hi)
        };

        self.base.select_prg_page(0, lo);
        self.base.select_prg_page(1, hi);
        self.base.set_mirroring_hv(horizontal);
    }
}

impl Mapper for Mapper227 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn reset(&mut self) {
        self.apply_register(0x8000);
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_register(addr);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.base.banking_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Non-power-of-two bank count prevents silent modulo-wrap false passes.
    /// 48 × 16 KB = 768 KB PRG (covers 6-bit bank index range 0..63)
    const PRG_BANKS: usize = 48;

    fn make_mapper(prg_rom: Vec<u8>) -> Mapper227 {
        Mapper227::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    fn make_default_mapper() -> Mapper227 {
        make_mapper(banked_data(16 * 1024, PRG_BANKS))
    }

    // ─── Factory registration ───────────────────────────────────────────────

    #[test]
    fn mapper_227_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 227 should be registered in factory");
    }

    // ─── Power-on state ─────────────────────────────────────────────────────

    #[test]
    fn power_on_both_prg_windows_at_bank_0() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should start at bank 0 on power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should start at bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be Vertical on power-on"
        );
    }

    // ─── PRG banking: M=0, S=0, L=0 (independent lower, upper = bank & 0x38) ──

    #[test]
    fn mode_m0_s0_l0_lower_at_selected_bank_upper_masked_to_block() {
        let mut mapper = make_default_mapper();
        // bank=5, M=0, S=0, L=0: addr = 0x8000|(5<<2) = 0x8014
        // lo=5, hi=5&0x38=0
        mapper.write_prg(0x8014, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 = bank 5");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 = bank 0 (5 & 0x38)");
    }

    #[test]
    fn mode_m0_s0_l0_bank_9_upper_at_block_start() {
        let mut mapper = make_default_mapper();
        // bank=9, M=0, S=0, L=0: addr = 0x8000|(9<<2) = 0x8024
        // lo=9, hi=9&0x38=8
        mapper.write_prg(0x8024, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 9, "$8000 = bank 9");
        assert_eq!(mapper.read_prg(0xC000), 8, "$C000 = bank 8 (9 & 0x38)");
    }

    // ─── PRG banking: M=0, S=0, L=1 (lower = bank, upper = bank | 0x07) ────

    #[test]
    fn mode_m0_s0_l1_lower_at_bank_upper_at_bank_or_7() {
        let mut mapper = make_default_mapper();
        // bank=5, M=0, S=0, L=1: addr = 0x8000|0x200|(5<<2) = 0xA214
        // lo=5, hi=5|0x07=7
        mapper.write_prg(0xA214, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 = bank 5");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 = bank 7 (5|0x07)");
    }

    #[test]
    fn mode_m0_s0_l1_bank_0_upper_fixed_at_7() {
        let mut mapper = make_default_mapper();
        // bank=0, M=0, S=0, L=1: addr = 0x8200
        // lo=0, hi=0|0x07=7
        mapper.write_prg(0x8200, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = bank 0");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 = bank 7 (0|0x07)");
    }

    // ─── PRG banking: M=0, S=1, L=0 (lower = bank & 0x3E, upper = bank & 0x38) ─

    #[test]
    fn mode_m0_s1_l0_lower_aligned_upper_at_block_start() {
        let mut mapper = make_default_mapper();
        // bank=9, M=0, S=1, L=0: addr = 0x8000|(9<<2)|0x01 = 0x8025
        // lo=9&0x3E=8, hi=9&0x38=8
        mapper.write_prg(0x8025, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 = bank 8 (9 & 0x3E)");
        assert_eq!(mapper.read_prg(0xC000), 8, "$C000 = bank 8 (9 & 0x38)");
    }

    #[test]
    fn mode_m0_s1_l0_bank_5_lower_aligned() {
        let mut mapper = make_default_mapper();
        // bank=5, M=0, S=1, L=0: addr = 0x8000|(5<<2)|0x01 = 0x8015
        // lo=5&0x3E=4, hi=5&0x38=0
        mapper.write_prg(0x8015, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 = bank 4 (5 & 0x3E)");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 = bank 0 (5 & 0x38)");
    }

    // ─── PRG banking: M=0, S=1, L=1 (lower = bank & 0x3E, upper = bank | 0x07) ─

    #[test]
    fn mode_m0_s1_l1_lower_aligned_upper_at_top_of_block() {
        let mut mapper = make_default_mapper();
        // bank=9, M=0, S=1, L=1: addr = 0x8000|0x200|(9<<2)|0x01 = 0xA225
        // lo=9&0x3E=8, hi=9|0x07=15
        mapper.write_prg(0xA225, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 = bank 8 (9 & 0x3E)");
        assert_eq!(mapper.read_prg(0xC000), 15, "$C000 = bank 15 (9|0x07)");
    }

    // ─── PRG banking: M=1, S=0 (both windows = bank) ────────────────────────

    #[test]
    fn mode_m1_s0_both_windows_at_same_bank() {
        let mut mapper = make_default_mapper();
        // bank=5, M=1, S=0: addr = 0x8000|0x80|(5<<2) = 0x8094
        // lo=5, hi=5
        mapper.write_prg(0x8094, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 = bank 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 = bank 5");
    }

    #[test]
    fn mode_m1_s0_bank_0_both_windows_at_0() {
        let mut mapper = make_default_mapper();
        // bank=0, M=1, S=0: addr = 0x8080
        mapper.write_prg(0x8080, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 = bank 0");
    }

    // ─── PRG banking: M=1, S=1 (32KB aligned: lo=bank&0xFE, hi=lo+1) ────────

    #[test]
    fn mode_m1_s1_selects_32kb_aligned_pair() {
        let mut mapper = make_default_mapper();
        // bank=5, M=1, S=1: addr = 0x8000|0x80|(5<<2)|0x01 = 0x8095
        // prgBank=5, lo=4 (5&0xFE), hi=5
        mapper.write_prg(0x8095, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 = bank 4 (5 & 0xFE)");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 = bank 5 (lo+1)");
    }

    #[test]
    fn mode_m1_s1_even_bank_stays_aligned() {
        let mut mapper = make_default_mapper();
        // bank=6, M=1, S=1: addr = 0x8000|0x80|(6<<2)|0x01 = 0x8099
        // prgBank=6, lo=6 (6&0xFE=6), hi=7
        mapper.write_prg(0x8099, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 = bank 6");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 = bank 7");
    }

    // ─── A8 (bit 5 of bank) ──────────────────────────────────────────────────

    #[test]
    fn bank_bit_a8_extends_bank_to_6_bits() {
        let mut mapper = make_default_mapper();
        // A8=1, other bank bits=0: addr = 0x8100
        // prgBank = 0 | (0x100>>3) = 32
        // M=0, S=0, L=0: lo=32, hi=32&0x38=32
        mapper.write_prg(0x8100, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "$8000 = bank 32 (A8 extends bank)"
        );
        assert_eq!(mapper.read_prg(0xC000), 32, "$C000 = bank 32 (32 & 0x38)");
    }

    // ─── Mirroring ───────────────────────────────────────────────────────────

    #[test]
    fn mirroring_bit_a1_zero_selects_vertical() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // A1=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit_a1_one_selects_horizontal() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8002, 0x00); // A1=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8002, 0x00); // horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0x00); // vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ─── Write data value is ignored ─────────────────────────────────────────

    #[test]
    fn write_data_value_is_ignored() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8014, 0x00); // bank 5
        assert_eq!(mapper.read_prg(0x8000), 5);
        mapper.write_prg(0x8014, 0xFF); // different data, same address
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    // ─── Write to entire $8000–$FFFF range ───────────────────────────────────

    #[test]
    fn write_to_upper_rom_region_applies_register() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xE014, 0x00); // addr = $E014, bank=5 from bits A2-A6
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "$8000 = bank 5 from high address"
        );
    }

    // ─── Reset ───────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8094, 0x00); // select bank 5 (M=1, S=0)
        assert_eq!(mapper.read_prg(0x8000), 5);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower window bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "upper window bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "vertical mirroring after reset"
        );
    }

    // ─── Save state ──────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = make_mapper(prg.clone());
        // M=0, S=0, L=1, bank=5: lo=5, hi=7
        mapper.write_prg(0xA214, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 7);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 5, "lower PRG after restore");
        assert_eq!(restored.read_prg(0xC000), 7, "upper PRG after restore");
    }
}
