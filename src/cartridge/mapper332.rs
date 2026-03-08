//! Mapper 332 - Super 40-in-1 (BMC-WS)
//!
//! Specifications:
//! - Primary reference: Mesen2 `Super40in1Ws.h`
//! - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_332>
//!
//! ## Register Map ($6000–$6FFF)
//!
//! | Address | Bits | Function |
//! |---------|------|----------|
//! | Even ($6000, $6002, …) | [7:6] outer bank high | PRG + mirroring |
//! | Even | [5] | Lock: once set, all writes ignored |
//! | Even | [4] | Mirroring: 0=Vertical, 1=Horizontal |
//! | Even | [7:0] | PRG bank register `v` |
//! | Odd  ($6001, $6003, …) | [7:0] | CHR 8 KB page select |
//!
//! ## PRG Banking
//!
//! Two 16 KB windows at $8000 and $C000. Let `not_bit3 = !(v & 0x08)`:
//! - $8000 → bank `v & ~not_bit3`  (when bit3=0: even; when bit3=1: v)
//! - $C000 → bank `v | not_bit3`   (when bit3=0: odd;  when bit3=1: v)
//!
//! ## Power-on / Reset
//!
//! Equivalent to writing `0` to an even address: banks 0/1, vertical mirroring, unlocked.
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 332;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const REG_START: u16 = 0x6000;
const REG_END: u16 = 0x6FFF;
const LOCK_BIT: u8 = 0x20;
const MIRROR_BIT: u8 = 0x10;
const REGISTERS_SNAPSHOT_LEN: usize = 3;

pub struct Mapper332 {
    base: BaseMapper,
    prg_reg: u8,
    chr_bank: u8,
    reg_lock: bool,
}

impl Mapper332 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_reg: 0,
            chr_bank: 0,
            reg_lock: false,
        };
        mapper.apply_state(0, 0, false);
        mapper
    }

    fn not_bit3(v: u8) -> u8 {
        // Returns 1 when bit 3 of v is 0, 0 when bit 3 is 1
        (!v >> 3) & 0x01
    }

    fn prg_bank_low(v: u8) -> i16 {
        (v & !(Self::not_bit3(v))) as i16
    }

    fn prg_bank_high(v: u8) -> i16 {
        (v | Self::not_bit3(v)) as i16
    }

    fn apply_state(&mut self, prg_reg: u8, chr_bank: u8, reg_lock: bool) {
        self.prg_reg = prg_reg;
        self.chr_bank = chr_bank;
        self.reg_lock = reg_lock;

        self.base.select_prg_page(0, Self::prg_bank_low(prg_reg));
        self.base.select_prg_page(1, Self::prg_bank_high(prg_reg));
        self.base.select_chr_page(0, chr_bank as i16);
        self.base.set_mirroring_hv((prg_reg & MIRROR_BIT) != 0);
    }
}

impl Mapper for Mapper332 {
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
        if !(REG_START..=REG_END).contains(&addr) || self.reg_lock {
            return;
        }
        if (addr & 0x01) != 0 {
            // Odd address: CHR bank select
            self.apply_state(self.prg_reg, value, self.reg_lock);
        } else {
            // Even address: PRG register
            let new_lock = (value & LOCK_BIT) != 0;
            self.apply_state(value, self.chr_bank, new_lock);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg, self.chr_bank, self.reg_lock as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.apply_state(data[0], data[1], data[2] != 0);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_16K: usize = 48;
    const CHR_BANKS_8K: usize = 8;

    fn make_mapper() -> Mapper332 {
        Mapper332::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_332_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 332 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_selects_banks_0_and_1_with_vertical_mirroring() {
        let mapper = make_mapper();
        // Equivalent to writing 0 to even reg: not_bit3(0)=1 → bank0=0, bank1=1
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should be bank 1 at power-on"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical at power-on"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn when_bit3_is_clear_bank_low_is_even_bank_high_is_odd() {
        // value = 0x04 (bits: 0000_0100) — bit3 = 0 → not_bit3 = 1
        // bank0 = 0x04 & ~1 = 0x04 & 0xFE = 4
        // bank1 = 0x04 | 1 = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5");
    }

    #[test]
    fn when_bit3_is_set_both_windows_point_to_same_bank() {
        // value = 0x0A (bits: 0000_1010) — bit3 = 1 → not_bit3 = 0
        // bank0 = 0x0A & ~0 = 0x0A
        // bank1 = 0x0A | 0 = 0x0A
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 10, "$8000 should be bank 10");
        assert_eq!(
            mapper.read_prg(0xC000),
            10,
            "$C000 should be bank 10 (same)"
        );
    }

    #[test]
    fn prg_register_is_written_on_even_addresses_in_range() {
        let mut mapper = make_mapper();
        // Even address $6002: value = 0x06 → bank0=6, bank1=7
        mapper.write_prg(0x6002, 0x06);
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn prg_writes_outside_register_range_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0x0A); // outside $6000–$6FFF
        assert_eq!(mapper.read_prg(0x8000), 0, "should remain at bank 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "should remain at bank 1");
    }

    // ── Lock bit ──────────────────────────────────────────────────────────────

    #[test]
    fn lock_bit_prevents_further_prg_register_writes() {
        let mut mapper = make_mapper();
        // Write 0x24: bit5=lock, bit3=0 → not_bit3=1 → bank0=0x24&~1=36, bank1=0x24|1=37
        mapper.write_prg(0x6000, 0x24);
        // Try to change bank (would select banks 10/10)
        mapper.write_prg(0x6000, 0x0A);
        // Banks should still be 36/37 because register is locked
        assert_eq!(mapper.read_prg(0x8000), 36, "bank should be locked at 36");
        assert_eq!(mapper.read_prg(0xC000), 37, "bank should be locked at 37");
    }

    #[test]
    fn lock_bit_also_prevents_chr_writes() {
        let mut mapper = make_mapper();
        // First write CHR bank = 3
        mapper.write_prg(0x6001, 3);
        // Set lock
        mapper.write_prg(0x6000, 0x20); // lock with bank 0/1
        // Try to change CHR bank
        mapper.write_prg(0x6001, 7);
        // CHR bank should remain 3
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank should remain 3 after lock"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_is_selected_by_odd_address_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 5);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR should be bank 5");
    }

    #[test]
    fn chr_bank_select_does_not_affect_prg_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04); // PRG bank 4/5
        mapper.write_prg(0x6001, 2); // CHR bank 2
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should remain bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should remain bank 5");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn bit4_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // bit 4 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit4=1 should set horizontal mirroring"
        );
    }

    #[test]
    fn bit4_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // set horizontal first
        mapper.write_prg(0x6000, 0x00); // clear bit 4
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit4=0 should set vertical mirroring"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_chr_and_lock() {
        // Set state: chr=3, prg_reg=0x24 (banks 36/37, locked)
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 3); // CHR bank 3
        mapper.write_prg(0x6000, 0x24); // PRG: lock + bank 36/37
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 36);
        assert_eq!(restored.read_prg(0xC000), 37);
        assert_eq!(restored.read_chr(0x0000), 3);
        // Lock should be restored: further writes ignored
        restored.write_prg(0x6000, 0x0A);
        assert_eq!(restored.read_prg(0x8000), 36, "locked: bank unchanged");
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x24);
        mapper.write_prg(0x6001, 6);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
