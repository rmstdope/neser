//! Mapper 230 - 22-in-1 multicart with Contra
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper230.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 230;

/// Mapper 230 - 22-in-1 multicart with Contra
///
/// This multicart toggles between two modes on each soft reset:
///
/// ## Contra mode (power-on, every odd reset)
///
/// - PRG `$8000–$BFFF`: switchable 16 KiB bank, selected by bits 2:0 of the written byte (banks 0–7)
/// - PRG `$C000–$FFFF`: fixed to bank 7
/// - Mirroring: Vertical (fixed)
///
/// ## 22-in-1 mode (every even reset)
///
/// - PRG `$8000–$BFFF` and `$C000–$FFFF`: both switchable; base offset = 8
/// - bit 5 = 0: lower = `(value & 0x1E) + 8`, upper = `(value & 0x1E) + 9` (paired)
/// - bit 5 = 1: both windows = `(value & 0x1F) + 8` (single page)
/// - bit 6: 0 = Horizontal, 1 = Vertical
///
/// ## Reset behaviour
///
/// - **Hard reset**: `contra_mode` is reset to `false`; the subsequent `reset()` call
///   toggles it to `true` → Contra mode (power-on state).
/// - **Soft reset**: `contra_mode` toggles; bank registers reset to mode defaults.
///
/// ## CHR
///
/// Uses 8 KiB CHR-RAM (fixed bank 0).
pub struct Mapper230 {
    base: BaseMapper,
    /// `true` = Contra mode, `false` = 22-in-1 mode
    contra_mode: bool,
    /// Written byte used to select PRG banks and mirroring in 22-in-1 mode
    reg: u8,
    /// Set by `initialize_ram`; tells the next `reset()` to treat itself as a hard reset.
    hard_reset_pending: bool,
}

impl Mapper230 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
            contra_mode: false,
            reg: 0,
            hard_reset_pending: false,
        };
        // Simulate hard reset: initialize_ram sets hard_reset_pending, reset() applies it.
        mapper.hard_reset_pending = true;
        mapper.reset();
        mapper
    }

    fn apply_banks(&mut self) {
        if self.contra_mode {
            let lo = (self.reg & 0x07) as i16;
            self.base.select_prg_page(0, lo);
            self.base.select_prg_page(1, 7);
            self.base.set_mirroring_hv(false); // Vertical
        } else {
            let base_page = (self.reg & 0x1E) as i16 + 8;
            if self.reg & 0x20 != 0 {
                // Single-page mode: both windows same bank
                let page = (self.reg & 0x1F) as i16 + 8;
                self.base.select_prg_page(0, page);
                self.base.select_prg_page(1, page);
            } else {
                // Paired mode
                self.base.select_prg_page(0, base_page);
                self.base.select_prg_page(1, base_page + 1);
            }
            let vertical = self.reg & 0x40 != 0;
            self.base.set_mirroring_hv(!vertical); // bit6=1 → Vertical → not horizontal
        }
    }
}

impl Mapper for Mapper230 {
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
        if (0x8000..=0xFFFF).contains(&addr) {
            self.reg = value;
            self.apply_banks();
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.base.initialize_ram(mode);
        self.contra_mode = false;
        self.hard_reset_pending = true;
    }

    fn reset(&mut self) {
        self.contra_mode = if self.hard_reset_pending {
            // Hard reset always starts in Contra mode.
            self.hard_reset_pending = false;
            true
        } else {
            // Soft reset toggles the mode.
            !self.contra_mode
        };
        self.reg = 0;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.reg);
        snap.push(self.contra_mode as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected_banking_len = self.base.banking_snapshot().len();
        if data.len() >= expected_banking_len + 2 {
            self.base.restore_banking(&data[..expected_banking_len]);
            self.reg = data[expected_banking_len];
            self.contra_mode = data[expected_banking_len + 1] != 0;
            self.apply_banks();
        } else {
            self.base.restore_banking(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// 48 banks × 16 KiB (non-power-of-two to catch modulo-wrapping false passes)
    const PRG_BANKS: usize = 48;

    fn create_mapper230(prg_rom: Vec<u8>) -> Mapper230 {
        Mapper230::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    fn make_mapper() -> Mapper230 {
        create_mapper230(banked_data(16 * 1024, PRG_BANKS))
    }

    /// Helper: simulate a soft reset on mapper
    fn soft_reset(mapper: &mut Mapper230) {
        mapper.reset();
    }

    /// Helper: simulate a hard reset on mapper
    fn hard_reset(mapper: &mut Mapper230) {
        mapper.initialize_ram(crate::console::RamInitMode::Zero);
        mapper.reset();
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_230_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 230 should be registered in factory");
    }

    // ── Power-on state (Contra mode) ─────────────────────────────────────────

    #[test]
    fn power_on_lower_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 window should power on to bank 0 (Contra mode)"
        );
    }

    #[test]
    fn power_on_upper_window_is_bank_7() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 window should power on to bank 7 (fixed in Contra mode)"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring should be Vertical (Contra mode)"
        );
    }

    // ── Contra mode: write banking ───────────────────────────────────────────

    #[test]
    fn contra_mode_write_selects_lower_bank_bits_2_to_0() {
        let mut mapper = make_mapper();
        // value = 5 → lower bank = 5
        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);
        // Upper window stays fixed at 7
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn contra_mode_upper_bank_is_always_7_regardless_of_write() {
        let mut mapper = make_mapper();
        // value with upper bits set; only bits 2:0 affect lower bank; upper stays 7
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "Lower bank should be value & 0x07 = 7"
        );
        assert_eq!(mapper.read_prg(0xC000), 7, "Upper bank fixed at 7");
    }

    #[test]
    fn contra_mode_write_bank_3_lower_window_bank_3_upper_still_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn contra_mode_mirroring_stays_vertical_after_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF); // high bits should be ignored for mirroring
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── 22-in-1 mode (after first soft reset) ───────────────────────────────

    #[test]
    fn after_first_soft_reset_lower_window_is_bank_8() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "After soft reset lower window should be bank 8 (22-in-1 default)"
        );
    }

    #[test]
    fn after_first_soft_reset_upper_window_is_bank_9() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        assert_eq!(
            mapper.read_prg(0xC000),
            9,
            "After soft reset upper window should be bank 9 (22-in-1 default)"
        );
    }

    #[test]
    fn after_first_soft_reset_mirroring_is_horizontal() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "22-in-1 mode default mirroring should be Horizontal"
        );
    }

    #[test]
    fn twenty_two_in_one_paired_mode_bit5_clear() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        // value = 0x02 → bit5=0 → lower = (0x02 & 0x1E) + 8 = 2+8=10, upper = 11
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 10, "Lower window bank 10");
        assert_eq!(mapper.read_prg(0xC000), 11, "Upper window bank 11");
    }

    #[test]
    fn twenty_two_in_one_single_page_mode_bit5_set() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        // value = 0x23 → bit5=1 → both = (0x23 & 0x1F) + 8 = 3+8=11
        mapper.write_prg(0x8000, 0x23);
        assert_eq!(mapper.read_prg(0x8000), 11, "Both windows bank 11");
        assert_eq!(mapper.read_prg(0xC000), 11, "Both windows bank 11");
    }

    #[test]
    fn twenty_two_in_one_mirroring_bit6_zero_is_horizontal() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        mapper.write_prg(0x8000, 0x00); // bit6=0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn twenty_two_in_one_mirroring_bit6_one_is_vertical() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        mapper.write_prg(0x8000, 0x40); // bit6=1 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn twenty_two_in_one_paired_mode_odd_value_aligns_to_even_pair() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper);
        // value = 0x03 → bit5=0 → lower = (0x03 & 0x1E) + 8 = 2+8=10, upper = 11
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            10,
            "Odd value should align to even pair: bank 10"
        );
        assert_eq!(mapper.read_prg(0xC000), 11, "Upper bank 11");
    }

    // ── Mode toggling on successive resets ───────────────────────────────────

    #[test]
    fn second_soft_reset_returns_to_contra_mode() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper); // → 22-in-1
        soft_reset(&mut mapper); // → Contra again
        // Contra mode: lower bank 0, upper bank 7, Vertical
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn hard_reset_restores_contra_mode() {
        let mut mapper = make_mapper();
        soft_reset(&mut mapper); // → 22-in-1
        mapper.write_prg(0x8000, 0x40); // Vertical in 22-in-1 mode
        hard_reset(&mut mapper); // → Contra mode again
        assert_eq!(mapper.read_prg(0x8000), 0, "Hard reset lower bank 0");
        assert_eq!(mapper.read_prg(0xC000), 7, "Hard reset upper bank 7");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Hard reset mirroring Vertical"
        );
    }

    #[test]
    fn write_in_contra_mode_does_not_affect_22in1_banks_after_next_reset() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05); // select bank 5 in Contra mode
        soft_reset(&mut mapper); // → 22-in-1, reg cleared to 0
        // Default 22-in-1 state: lower=8, upper=9
        assert_eq!(mapper.read_prg(0x8000), 8);
        assert_eq!(mapper.read_prg(0xC000), 9);
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    // ── Save state (snapshot/restore) ────────────────────────────────────────

    #[test]
    fn snapshot_and_restore_in_contra_mode() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = create_mapper230(prg.clone());
        mapper.write_prg(0x8000, 3); // bank 3 in Contra mode

        let snap = mapper.registers_snapshot();
        let mut restored = create_mapper230(prg);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xC000), 7);
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn snapshot_and_restore_in_22in1_mode() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = create_mapper230(prg.clone());
        soft_reset(&mut mapper);
        mapper.write_prg(0x8000, 0x04); // paired: lower=12, upper=13

        let snap = mapper.registers_snapshot();
        let mut restored = create_mapper230(prg);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 12);
        assert_eq!(restored.read_prg(0xC000), 13);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
