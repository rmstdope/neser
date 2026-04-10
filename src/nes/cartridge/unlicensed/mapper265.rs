//! Mapper 265 – T-262 multicart (iNES/NES 2.0 Mapper 265)
//!
//! Specifications:
//! - Primary source: NesDev wiki page not available at time of implementation.
//! - Fallback source: Mesen2 `Core/NES/Mappers/Unlicensed/T262.h`
//!
//! ## Overview
//!
//! T-262 is a simple multicart mapper with:
//! - Two 16 KB PRG windows at `$8000–$BFFF` and `$C000–$FFFF`.
//! - A single 8 KB CHR window fixed to bank 0 (CHR-ROM or CHR-RAM).
//! - Software-controlled H/V mirroring.
//! - A one-way latch that freezes bank-group and mode selection once set.
//!
//! ## Register (write to `$8000–$FFFF`)
//!
//! Any write to the PRG-ROM region is decoded using **both** the address bus
//! and the data bus:
//!
//! | source       | bits used    | target field  |
//! |--------------|--------------|---------------|
//! | address      | A1           | mirroring (0=Vertical, 1=Horizontal) |
//! | address      | A6–A5        | `_base` bits 4–3 |
//! | address      | A7           | `_mode`       |
//! | address      | A8           | `_base` bit 5 |
//! | address      | A13          | `_locked`     |
//! | data         | D2–D0        | `_bank`       |
//!
//! The `_base`, `_mode`, `_locked`, and mirroring fields are only updated
//! when `_locked` is `false`.  `_bank` and the resulting PRG mapping are
//! always updated.
//!
//! ## PRG banking formula
//!
//! ```text
//! page_0 = _base | _bank
//! page_1 = _base | (if _mode { _bank } else { 7 })
//! ```
//!
//! Power-on state: `_base=0`, `_bank=0`, `_mode=false`, `_locked=false`
//! → page 0 = bank 0, page 1 = bank 7.
//!
//! ## Known limitations
//!
//! - No known functional gaps relative to the Mesen2 T262 reference implementation.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::console::RamInitMode;

/// Mapper 265 – T-262 multicart
pub struct Mapper265 {
    base: BaseMapper,
    /// Lower 3 bits of PRG bank index (from data bus D2–D0).
    bank: u8,
    /// Upper bits of PRG bank group (from address bus A8, A6–A5).
    base_bank: u8,
    /// Mode flag (from address bus A7):
    /// `false` → upper window fixed to `_base | 7`; `true` → mirrors lower window.
    mode: bool,
    /// One-way latch: once set, `base_bank`, `mode`, mirroring, and `locked` no longer update.
    locked: bool,
    /// Power-on nametable mirroring (restored on reset).
    initial_mirroring: NametableLayout,
}

impl Mapper265 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);

        let initial_mirroring = ctx.mirroring;
        let mut mapper = Self {
            base,
            bank: 0,
            base_bank: 0,
            mode: false,
            locked: false,
            initial_mirroring,
        };
        mapper.update_prg();
        mapper
    }

    /// Recompute `_base` from an address bus value (when not locked).
    fn decode_base(addr: u16) -> u8 {
        ((addr & 0x60) >> 2) as u8 | ((addr & 0x100) >> 3) as u8
    }

    /// Apply current `base_bank`, `bank`, and `mode` to PRG page selection.
    fn update_prg(&mut self) {
        let page0 = (self.base_bank | self.bank) as i16;
        let page1_bank = if self.mode { self.bank } else { 7 };
        let page1 = (self.base_bank | page1_bank) as i16;
        self.base.select_prg_page(0, page0);
        self.base.select_prg_page(1, page1);
    }
}

impl Mapper for Mapper265 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            if !self.locked {
                self.base_bank = Self::decode_base(addr);
                self.mode = (addr & 0x80) != 0;
                self.locked = (addr & 0x2000) != 0;
                let mirroring = if (addr & 0x02) != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                };
                self.base.set_mirroring(mirroring);
            }
            self.bank = value & 0x07;
            self.update_prg();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Start with the banking snapshot (includes mirroring since has_dynamic_mirroring=true).
        let mut snap = self.base.banking_snapshot();
        // Append mapper-specific state.
        // flags: bit0=mode, bit1=locked
        let flags = (self.mode as u8) | ((self.locked as u8) << 1);
        snap.push(self.bank);
        snap.push(self.base_bank);
        snap.push(flags);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_len = self.base.banking_snapshot().len();
        if data.len() >= banking_len {
            self.base.restore_banking(&data[..banking_len]);
        } else {
            self.base.restore_banking(data);
            return;
        }
        if data.len() >= banking_len + 3 {
            self.bank = data[banking_len] & 0x07;
            self.base_bank = data[banking_len + 1];
            let flags = data[banking_len + 2];
            self.mode = (flags & 0x01) != 0;
            self.locked = (flags & 0x02) != 0;
            self.update_prg();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.base_bank = 0;
        self.mode = false;
        self.locked = false;
        self.base.set_mirroring(self.initial_mirroring);
        self.update_prg();
    }

    fn initialize_ram(&mut self, mode: RamInitMode) {
        self.base.initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use a non-power-of-2 count to prevent modulo-wrap false passes.
    // 48 banks × 16 KB = 768 KB (bank indices 0..=47 are all distinct).
    const PRG_BANKS_16K: usize = 48;

    fn make_mapper() -> Mapper265 {
        Mapper265::new(
            MapperContext::new_for_test(
                265,
                banked_data(16 * 1024, PRG_BANKS_16K),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_265_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                265,
                banked_data(16 * 1024, PRG_BANKS_16K),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 265 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // Power-on state
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_page0_is_bank_0() {
        let mapper = make_mapper();
        // bank 0 is filled with 0x00 by banked_data
        assert_eq!(mapper.read_prg(0x8000), 0x00);
        assert_eq!(mapper.read_prg(0xBFFF), 0x00);
    }

    #[test]
    fn power_on_page1_is_bank_7() {
        let mapper = make_mapper();
        // bank 7 is filled with 0x07 by banked_data
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_prg(0xFFFF), 7);
    }

    // -------------------------------------------------------------------------
    // Bank switching via data bus (D2–D0)
    // -------------------------------------------------------------------------

    #[test]
    fn bank_field_selects_page0_from_data_low_3_bits() {
        let mut mapper = make_mapper();
        // Write to $8000: base=0, mode=false → page0 = bank, page1 = 7
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "page 0 must be bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "page 1 stays fixed to base|7 when mode=false"
        );
    }

    #[test]
    fn only_low_3_bits_of_data_used_for_bank() {
        let mut mapper = make_mapper();
        // Bits D7–D3 must be ignored; only D2–D0 matter
        mapper.write_prg(0x8000, 0b1111_1011); // D2–D0 = 011 = 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // -------------------------------------------------------------------------
    // Address bus fields: base (A8, A6–A5)
    // -------------------------------------------------------------------------

    #[test]
    fn base_from_addr_bits_5_and_6_selects_bank_group() {
        let mut mapper = make_mapper();
        // A5=1, A6=0 → base bits: (addr & 0x60) >> 2 = (0x20) >> 2 = 0x08
        // addr = 0x8020, bank = 1 → page0 = 0x08 | 1 = 9
        mapper.write_prg(0x8020, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            9,
            "page 0 = base(0x08) | bank(1) = 9"
        );

        let mut mapper = make_mapper();
        // A5=0, A6=1 → base = (0x40) >> 2 = 0x10
        // addr = 0x8040, bank = 2 → page0 = 0x10 | 2 = 18
        mapper.write_prg(0x8040, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            18,
            "page 0 = base(0x10) | bank(2) = 18"
        );
    }

    #[test]
    fn base_from_addr_bit_8_contributes_to_bank_group() {
        let mut mapper = make_mapper();
        // A8=1 → base: (addr & 0x100) >> 3 = 0x20
        // addr = 0x8100, bank = 1 → page0 = 0x20 | 1 = 33
        mapper.write_prg(0x8100, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            33,
            "page 0 = base(0x20) | bank(1) = 33"
        );
    }

    // -------------------------------------------------------------------------
    // Mode flag (A7)
    // -------------------------------------------------------------------------

    #[test]
    fn mode_false_fixes_page1_to_base_or_7() {
        let mut mapper = make_mapper();
        // addr = 0x8000: A7=0 → mode=false → page1 = base|7 = 0|7 = 7
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn mode_true_mirrors_page1_to_same_bank_as_page0() {
        let mut mapper = make_mapper();
        // addr = 0x8080: A7=1 → mode=true → page1 = base|bank = 0|3 = 3
        mapper.write_prg(0x8080, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "page 1 mirrors page 0 when mode=true"
        );
    }

    #[test]
    fn mode_true_page1_follows_subsequent_bank_changes() {
        let mut mapper = make_mapper();
        // Set mode=true first
        mapper.write_prg(0x8080, 1);
        assert_eq!(mapper.read_prg(0xC000), 1);
        // Change bank (mode stays true because locked/mode are re-evaluated from addr)
        mapper.write_prg(0x8080, 4);
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    // -------------------------------------------------------------------------
    // Lock flag (A13)
    // -------------------------------------------------------------------------

    #[test]
    fn lock_bit_freezes_base_and_mode() {
        let mut mapper = make_mapper();
        // Write with A13=1 to lock: addr = 0xA000 (bit13=1)
        // First set base via A5=1 (addr & 0x20), mode=false, lock=true
        // addr = 0xA020: A5=1 → base=0x08, A7=0 → mode=false, A13=1 → locked=true
        mapper.write_prg(0xA020, 1); // page0 = 0x08|1=9, page1 = 0x08|7=15
        assert_eq!(mapper.read_prg(0x8000), 9);
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Now try to change base and mode with a different address — must be ignored
        mapper.write_prg(0x8000, 2); // Would set base=0, mode=false if not locked
        // base and mode should remain as before, only bank changes
        // page0 = 0x08|2 = 10, page1 = 0x08|7 = 15 (still mode=false, base=0x08)
        assert_eq!(
            mapper.read_prg(0x8000),
            10,
            "page 0 bank changes even when locked"
        );
        assert_eq!(mapper.read_prg(0xC000), 15, "page 1 base stays locked");
    }

    #[test]
    fn bank_always_updates_even_when_locked() {
        let mut mapper = make_mapper();
        // Lock the mapper
        mapper.write_prg(0xA000, 0); // A13=1 → locked
        // Change bank
        mapper.write_prg(0x8000, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "bank must still update after lock"
        );
    }

    // -------------------------------------------------------------------------
    // Mirroring (A1)
    // -------------------------------------------------------------------------

    #[test]
    fn addr_bit1_clear_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        // addr & 0x02 == 0 → Vertical
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn addr_bit1_set_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        // addr & 0x02 != 0 → Horizontal
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_does_not_change_after_lock() {
        let mut mapper = make_mapper();
        // Set Horizontal, then lock
        mapper.write_prg(0xA002, 0); // A1=1 (H), A13=1 (lock)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // Try to switch to Vertical after lock — must be ignored
        mapper.write_prg(0x8000, 0); // A1=0 (V) but locked
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------------
    // CHR is fixed to bank 0
    // -------------------------------------------------------------------------

    #[test]
    fn chr_uses_8kb_page_fixed_to_bank_0() {
        let chr_data: Vec<u8> = (0..0x4000u16).map(|i| (i & 0xFF) as u8).collect();
        let mut mapper = Mapper265::new(
            MapperContext::new_for_test(
                265,
                banked_data(16 * 1024, PRG_BANKS_16K),
                chr_data.clone(),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        // CHR page 0 ($0000–$1FFF) must match the first 8 KB of CHR data
        assert_eq!(mapper.read_chr(0x0000), chr_data[0x0000]);
        assert_eq!(mapper.read_chr(0x1FFF), chr_data[0x1FFF]);
    }

    // -------------------------------------------------------------------------
    // Save state round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_restore_round_trips_prg_state() {
        let mut mapper = make_mapper();
        // base from A5+A8, mode=false, bank=5: addr = 0x8120 → base = (0x20>>2)|(0x100>>3) = 0x08|0x20 = 0x28
        // page0 = 0x28 | 5 = 45, page1 = 0x28 | 7 = 47
        mapper.write_prg(0x8120, 5);
        let prg0 = mapper.read_prg(0x8000);
        let prg1 = mapper.read_prg(0xC000);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            prg0,
            "page 0 must survive round-trip"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            prg1,
            "page 1 must survive round-trip"
        );
    }

    #[test]
    fn registers_snapshot_restore_round_trips_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0); // A1=1 → Horizontal
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn registers_snapshot_restore_round_trips_lock() {
        let mut mapper = make_mapper();
        // Lock with base=0x08 (A5=1), mode=true (A7=1), bank=3
        mapper.write_prg(0xA0A0, 3); // A5=1→base=0x08, A7=1→mode=true, A13=1→locked
        // Confirm locked: try changing base via different addr
        mapper.write_prg(0x8000, 4); // bank changes, base stays 0x08, mode stays true
        let prg0_before = mapper.read_prg(0x8000); // 0x08 | 4 = 12
        let prg1_before = mapper.read_prg(0xC000); // mode=true → 0x08 | 4 = 12

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // Try changing base after restore — should still be locked
        restored.write_prg(0x8000, 4);
        assert_eq!(restored.read_prg(0x8000), prg0_before);
        assert_eq!(restored.read_prg(0xC000), prg1_before);
    }

    // -------------------------------------------------------------------------
    // Reset
    // -------------------------------------------------------------------------

    #[test]
    fn reset_restores_power_on_prg_mapping() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8020, 3); // change state
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "page 0 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "page 1 must be bank 7 after reset"
        );
    }

    #[test]
    fn reset_clears_lock() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0); // lock
        mapper.reset();
        // After reset the lock must be cleared; a new write should update base
        mapper.write_prg(0x8020, 1); // A5=1 → base=0x08, bank=1 → page0=9
        assert_eq!(mapper.read_prg(0x8000), 9, "lock must be cleared by reset");
    }

    #[test]
    fn reset_restores_power_on_mirroring() {
        let mut mapper = make_mapper();
        // Change mirroring to Horizontal
        mapper.write_prg(0x8002, 0); // A1=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // Reset should restore the power-on mirroring (Vertical, from make_mapper())
        mapper.reset();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "reset must restore power-on mirroring"
        );
    }
}
