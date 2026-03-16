//! Mapper 285 – A65AS
//!
//! Specifications:
//! - Primary source: NesDev wiki unavailable (HTTP 403/404).
//! - Fallback: Mesen2 `A65AS.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/A65AS.h>
//!
//! # Hardware overview
//!
//! Used by unlicensed multi-game cartridges.
//!
//! - PRG-ROM: two 16 KiB windows ($8000–$BFFF and $C000–$FFFF), banked
//!   by write to $8000–$FFFF.
//! - CHR-ROM: 8 KiB window at $0000–$1FFF, fixed at bank 0 (no CHR switching).
//! - Mirroring: programmable via bits 7/5/3 of register value.
//! - IRQ: none.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register ($8000–$FFFF, write)
//!
//! ```text
//! Bit 7 – mirroring mode select (1 = single-screen, 0 = H/V)
//! Bit 6 – PRG bank size (1 = 32 KiB mode, 0 = 16 KiB mode)
//! Bit 5 – single-screen page select (0 = lower/A, 1 = upper/B); used when bit 7 = 1
//! Bit 4 – outer PRG bank bit 1 (upper two bits of outer bank)
//! Bit 3 – mirroring direction (0 = Vertical, 1 = Horizontal); used when bit 7 = 0
//!           also outer PRG bank bit 0
//! Bits 2:0 – inner PRG bank select; used in 16 KiB mode
//! ```
//!
//! ## PRG banking
//!
//! **32 KiB mode (bit 6 = 1):**
//! - $8000–$BFFF: bank `(value & 0x1E)`
//! - $C000–$FFFF: bank `(value & 0x1E) + 1`
//!
//! **16 KiB mode (bit 6 = 0):**
//! - $8000–$BFFF: bank `((value & 0x30) >> 1) | (value & 0x07)`
//! - $C000–$FFFF: bank `((value & 0x30) >> 1) | 0x07`
//!
//! ## Mirroring
//!
//! | Bit 7 | Bit 5 / Bit 3 | Mirroring           |
//! |-------|---------------|---------------------|
//! | 1     | bit 5 = 0     | Single-screen lower |
//! | 1     | bit 5 = 1     | Single-screen upper |
//! | 0     | bit 3 = 0     | Vertical            |
//! | 0     | bit 3 = 1     | Horizontal          |
//!
//! # Power-on state
//!
//! Register initialised to 0: 16 KiB mode, lower page = 0, upper page = 7,
//! vertical mirroring.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 285;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 285 – A65AS
pub struct Mapper285 {
    base: BaseMapper,
    register: u8,
}

impl Mapper285 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        base.select_chr_page(0, 0);

        let mut mapper = Self { base, register: 0 };
        mapper.apply_register(0);
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.apply_prg_banking(value);
        self.apply_mirroring(value);
    }

    fn apply_prg_banking(&mut self, value: u8) {
        if value & 0x40 != 0 {
            // 32 KiB mode: both pages from contiguous pair
            let base_bank = (value & 0x1E) as i16;
            self.base.select_prg_page(0, base_bank);
            self.base.select_prg_page(1, base_bank + 1);
        } else {
            // 16 KiB mode
            let outer = ((value & 0x30) >> 1) as i16;
            let inner = (value & 0x07) as i16;
            self.base.select_prg_page(0, outer | inner);
            self.base.select_prg_page(1, outer | 0x07);
        }
    }

    fn apply_mirroring(&mut self, value: u8) {
        if value & 0x80 != 0 {
            let layout = if value & 0x20 != 0 {
                NametableLayout::SingleScreenUpper
            } else {
                NametableLayout::SingleScreenLower
            };
            self.base.set_mirroring(layout);
        } else {
            self.base.set_mirroring_hv(value & 0x08 != 0);
        }
    }
}

impl Mapper for Mapper285 {
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
        if addr >= 0x8000 {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() != 1 {
            return;
        }
        self.apply_register(data[0]);
    }

    fn reset(&mut self) {
        self.base.select_chr_page(0, 0);
        self.apply_register(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to avoid false-pass modulo wrapping.
    // 16 KiB banks: at least 24 so outer-bank bits don't coincidentally alias.
    const PRG_BANKS_16K: usize = 24;
    const CHR_BANKS_8K: usize = 1; // CHR is fixed; only one bank needed

    fn make_mapper() -> Mapper285 {
        Mapper285::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_285_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 285 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_lower_prg_window_maps_to_bank_0() {
        let mapper = make_mapper();
        // 16 KiB mode, value=0: lower page = ((0 & 0x30)>>1) | (0 & 0x07) = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should map to PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xBFFF),
            0,
            "$BFFF should also be in PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_upper_prg_window_maps_to_bank_7() {
        let mapper = make_mapper();
        // 16 KiB mode, value=0: upper page = ((0 & 0x30)>>1) | 0x07 = 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should map to PRG bank 7 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            7,
            "$FFFF should also be in PRG bank 7 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be Vertical at power-on (bit 3 = 0)"
        );
    }

    // ── 16 KiB mode (bit 6 = 0) ──────────────────────────────────────────────

    #[test]
    fn prg_16k_mode_inner_bank_selects_lower_window() {
        let mut mapper = make_mapper();
        // value = 0x05: bit6=0, outer=(0x00>>1)=0, inner=5 → lower=5, upper=7
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "lower window should be bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "upper window should be bank 7 (fixed)"
        );
    }

    #[test]
    fn prg_16k_mode_outer_bank_shifts_both_windows() {
        let mut mapper = make_mapper();
        // value = 0x10: bit6=0, bits[5:4]=01, outer=((0x10 & 0x30)>>1)=8, inner=0
        // lower = 8|0 = 8, upper = 8|7 = 15
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "lower window bank 8 (outer=8, inner=0)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "upper window bank 15 (outer=8, fixed inner=7)"
        );
    }

    #[test]
    fn prg_16k_mode_both_outer_bits_set() {
        let mut mapper = make_mapper();
        // value = 0x12: bit6=0, bits[5:4]=01
        // outer = ((0x12 & 0x30) >> 1) = (0x10 >> 1) = 8, inner = 0x12 & 0x07 = 2
        // lower window = outer | inner = 8 | 2 = 10
        // upper window = outer | 7 = 8 | 7 = 15
        mapper.write_prg(0x8000, 0x12);
        assert_eq!(
            mapper.read_prg(0x8000),
            10,
            "lower window bank 10 (outer=8, inner=2)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "upper window bank 15 (outer=8, inner fixed to 7)"
        );
    }

    #[test]
    fn prg_16k_mode_upper_window_is_always_outer_or_7() {
        let mut mapper = make_mapper();
        // Even when inner bits are non-7, upper window must be outer|7
        for inner in 0u8..7 {
            // outer=0, inner=inner → lower=inner, upper=7
            mapper.write_prg(0x8000, inner);
            assert_eq!(
                mapper.read_prg(0xC000),
                7,
                "upper window should always be bank 7 when outer=0, inner={inner}"
            );
        }
    }

    // ── 32 KiB mode (bit 6 = 1) ──────────────────────────────────────────────

    #[test]
    fn prg_32k_mode_selects_even_odd_pair() {
        let mut mapper = make_mapper();
        // value = 0x40: bit6=1, value & 0x1E = 0 → lower=0, upper=1
        mapper.write_prg(0x8000, 0x40);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower window bank 0 in 32K mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "upper window bank 1 in 32K mode"
        );
    }

    #[test]
    fn prg_32k_mode_bank_bits_select_pair() {
        let mut mapper = make_mapper();
        // value = 0x44: bit6=1, value & 0x1E = 0x04 → lower=4, upper=5
        mapper.write_prg(0x8000, 0x44);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "lower window bank 4 in 32K mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "upper window bank 5 in 32K mode"
        );
    }

    #[test]
    fn prg_32k_mode_odd_value_selects_same_pair_as_even() {
        // value & 0x1E masks bit0, so values 4 and 5 both select pair (4,5)
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x44); // pair (4, 5)
        let lower_even = mapper.read_prg(0x8000);
        let upper_even = mapper.read_prg(0xC000);

        mapper.write_prg(0x8000, 0x45); // same pair (4, 5), odd input
        let lower_odd = mapper.read_prg(0x8000);
        let upper_odd = mapper.read_prg(0xC000);

        assert_eq!(
            lower_even, lower_odd,
            "odd/even values should select same pair (lower)"
        );
        assert_eq!(
            upper_even, upper_odd,
            "odd/even values should select same pair (upper)"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_vertical_when_bit7_clear_and_bit3_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // bit7=0, bit3=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_when_bit7_clear_and_bit3_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x08); // bit7=0, bit3=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_single_screen_lower_when_bit7_set_and_bit5_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x80); // bit7=1, bit5=0 → SingleScreenLower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn mirroring_single_screen_upper_when_bit7_set_and_bit5_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xA0); // bit7=1, bit5=1 → SingleScreenUpper
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn mirroring_and_banking_bits_are_independent() {
        let mut mapper = make_mapper();
        // value = 0x89: bit7=1 (single-screen lower), bit3=1 (ignored), outer=0, inner=1
        mapper.write_prg(0x8000, 0x89);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "bit3 irrelevant when bit7=1"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "inner=1 still applied to lower window"
        );
    }

    // ── CHR is fixed at bank 0 ────────────────────────────────────────────────

    #[test]
    fn chr_is_always_bank_0_after_any_prg_register_write() {
        let mut mapper = make_mapper();
        for value in [0x00u8, 0x40, 0x80, 0xFF] {
            mapper.write_prg(0x8000, value);
            assert_eq!(
                mapper.read_chr(0x0000),
                0,
                "CHR should always be bank 0 after write value={value:#04X}"
            );
        }
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_register() {
        let mut mapper = make_mapper();
        // 32K mode, value=0x44: lower=4, upper=5
        mapper.write_prg(0x8000, 0x44);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "restored lower window must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "restored upper window must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "restored mirroring must match"
        );
    }

    #[test]
    fn restore_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x44); // set non-default state
        mapper.restore_registers(&[]); // empty → must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "state must be unchanged after empty restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xA8); // set some state
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower window bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "upper window bank 7 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "vertical mirroring after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_chr_banking, "no CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 285 must never assert IRQ");
    }
}
