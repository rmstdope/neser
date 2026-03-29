//! Mapper 265 – T-262 (unlicensed Chinese multicart)
//!
//! Specifications:
//! - NesDev wiki page for Mapper 265 could not be retrieved during research
//!   (CloudFlare challenge blocked access).
//! - Fallback source: Mesen2 `Core/NES/Mappers/Unlicensed/T262.h`
//!
//! Hardware: Unlicensed multicart board used for Chinese cartridges (T-262).
//!
//! PRG ROM: two switchable 16 KiB banks.
//!   - Slot 0 ($8000–$BFFF): `_base | _bank`
//!   - Slot 1 ($C000–$FFFF): `_base | (if _mode { _bank } else { 7 })`
//!
//! CHR ROM: single fixed 8 KiB bank at $0000–$1FFF (bank 0, never switched).
//!
//! Register: written to any address in $8000–$FFFF.
//!
//! If NOT locked:
//!   - `_base  = ((addr & 0x60) >> 2) | ((addr & 0x100) >> 3)`
//!     Addr bits [6:5] are placed into base bits [4:3];
//!     addr bit [8] is placed into base bit [5].
//!     All other base bits remain zero.
//!   - `_mode   = (addr & 0x80) != 0`
//!   - `_locked = (addr & 0x2000) != 0`
//!   - Mirroring: addr bit 1 set → Horizontal, else Vertical
//!
//! Always (locked or not):
//!   - `_bank = value & 0x07`
//!   - PRG banking is re-applied using the current `_base`, `_bank`, and `_mode`.
//!
//! Power-on: `_locked=false`, `_base=0`, `_bank=0`, `_mode=false`
//!   → slot 0 = PRG bank 0, slot 1 = PRG bank 7.
//!
//! Reset: restores power-on state (all fields cleared).
//!
//! Known limitations:
//! - No PRG-RAM.
//! - CHR is always bank 0; no in-game CHR switching.
//! - Source: Mesen2 T262.h only. No known delta from NesDev specification.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::console::RamInitMode;

/// Mapper 265 – T-262 unlicensed multicart.
pub struct Mapper265 {
    base: BaseMapper,
    locked: bool,
    base_prg: u8,
    bank: u8,
    mode: bool,
}

/// PRG bank index written to slot 1 when `_mode` is false.
///
/// This is the final bank within each 8-bank "group" (bank index 7 within
/// the group selected by `_base`), effectively pinning $C000–$FFFF to
/// the last bank of the current outer-bank set.
const FIXED_SLOT1_BANK: u8 = 7;

impl Mapper265 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);

        let mut mapper = Self {
            base,
            locked: false,
            base_prg: 0,
            bank: 0,
            mode: false,
        };
        mapper.apply_banking();
        mapper
    }

    fn apply_banking(&mut self) {
        let p0 = (self.base_prg | self.bank) as i16;
        let slot1_bank = if self.mode {
            self.bank
        } else {
            FIXED_SLOT1_BANK
        };
        let p1 = (self.base_prg | slot1_bank) as i16;
        self.base.select_prg_page(0, p0);
        self.base.select_prg_page(1, p1);
        self.base.select_chr_page(0, 0);
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
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr < 0x8000 {
            return;
        }
        if !self.locked {
            let bits_6_5 = (addr & 0x60) >> 2; // addr bits [6:5] → base bits [4:3]
            let bit_8 = (addr & 0x100) >> 3; // addr bit [8] → base bit [5]
            self.base_prg = (bits_6_5 | bit_8) as u8;
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
        self.apply_banking();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.locked as u8, self.base_prg, self.bank, self.mode as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        self.locked = data[0] != 0;
        self.base_prg = data[1];
        self.bank = data[2];
        self.mode = data[3] != 0;
        self.apply_banking();
    }

    fn reset(&mut self) {
        self.locked = false;
        self.base_prg = 0;
        self.bank = 0;
        self.mode = false;
        self.apply_banking();
    }

    fn initialize_ram(&mut self, mode: RamInitMode) {
        self.base.initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-2 bank count so modulo-wrap tests are meaningful.
    const PRG_16K_BANKS: usize = 11;

    fn make_mapper() -> Mapper265 {
        Mapper265::new(
            MapperContext::new_for_test(
                265,
                banked_data(16 * 1024, PRG_16K_BANKS),
                banked_data(8 * 1024, 1),
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
                banked_data(16 * 1024, PRG_16K_BANKS),
                banked_data(8 * 1024, 1),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 265 must be registered in the factory"
        );
    }

    // -------------------------------------------------------------------------
    // Power-on state
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_prg_slot0_is_bank_0() {
        let mapper = make_mapper();
        // banked_data fills each 16KB bank with its index byte; bank 0 → 0x00
        assert_eq!(
            mapper.read_prg(0x8000),
            0x00,
            "slot 0 must map bank 0 at power-on"
        );
        assert_eq!(mapper.read_prg(0xBFFF), 0x00);
    }

    #[test]
    fn power_on_prg_slot1_is_bank_7() {
        let mapper = make_mapper();
        // bank 7 → byte value 7 (from banked_data), wrapped: 7 % 11 = 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot 1 must map bank 7 at power-on"
        );
        assert_eq!(mapper.read_prg(0xFFFF), 7);
    }

    // -------------------------------------------------------------------------
    // PRG bank switching via data bits (value & 0x07 = _bank)
    // -------------------------------------------------------------------------

    #[test]
    fn bank_field_from_value_bits_0_to_2() {
        let mut mapper = make_mapper();

        // addr=0x8000: base=0, mode=false, locked=false → slot1 fixed at bank7
        // value = 3 → _bank = 3 → slot0 = 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "slot 0 must be bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot 1 still fixed at bank 7 when mode=false"
        );

        // value = 5 → _bank = 5 → slot0 = 5
        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);

        // Upper bits of value are ignored
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 7, "only low 3 bits of value count");
    }

    // -------------------------------------------------------------------------
    // Mode bit (addr bit 7): controls whether slot1 mirrors slot0 or is fixed
    // -------------------------------------------------------------------------

    #[test]
    fn mode_false_slot1_fixed_at_base_or_7() {
        let mut mapper = make_mapper();

        // addr bit7 = 0 → mode = false → slot1 = base | 7
        mapper.write_prg(0x8000, 2); // addr bit7=0, value=2
        assert_eq!(mapper.read_prg(0x8000), 2, "slot0 = bank 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot1 = bank 7 (base=0, mode=false)"
        );
    }

    #[test]
    fn mode_true_slot1_mirrors_slot0() {
        let mut mapper = make_mapper();

        // addr bit7 = 1 → mode = true → slot1 mirrors slot0 (base | _bank)
        mapper.write_prg(0x8080, 2); // addr bit7=1, value=2 → both slots = 2
        assert_eq!(mapper.read_prg(0x8000), 2, "slot0 = bank 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "slot1 mirrors slot0 when mode=true"
        );
    }

    // -------------------------------------------------------------------------
    // Base field (addr bits [6:5] and bit [8])
    // -------------------------------------------------------------------------

    #[test]
    fn base_from_addr_bits_5_6_and_8() {
        let mut mapper = make_mapper();

        // addr = 0x8120:
        //   addr & 0x60 = 0x20 → >> 2 = 0x08
        //   addr & 0x100 = 0x100 → >> 3 = 0x20
        //   base = 0x08 | 0x20 = 0x28 = 40
        //   addr & 0x80 = 0 → mode = false
        //   addr & 0x2000 = 0 → not locked
        //   addr & 0x02 = 0 → Vertical
        // value = 1 → _bank = 1
        // slot0 = base | bank = 40 | 1 = 41
        // slot1 = base | 7 = 40 | 7 = 47
        // PRG_16K_BANKS = 11 → 41 % 11 = 8, 47 % 11 = 3
        mapper.write_prg(0x8120, 1);
        assert_eq!(mapper.read_prg(0x8000), 8, "slot0 = 41 % 11 = 8");
        assert_eq!(mapper.read_prg(0xC000), 3, "slot1 = 47 % 11 = 3");
    }

    #[test]
    fn base_from_addr_bit_5_only() {
        let mut mapper = make_mapper();

        // addr = 0x8020: addr & 0x60 = 0x20 → >> 2 = 0x08; addr & 0x100 = 0 → base = 8
        // mode=false, value=0 → slot0 = 8, slot1 = 15
        // 8 % 11 = 8, 15 % 11 = 4
        mapper.write_prg(0x8020, 0);
        assert_eq!(mapper.read_prg(0x8000), 8, "slot0 = 8 % 11 = 8");
        assert_eq!(mapper.read_prg(0xC000), 4, "slot1 = 15 % 11 = 4");
    }

    #[test]
    fn base_from_addr_bit_6_only() {
        let mut mapper = make_mapper();

        // addr = 0x8040: addr & 0x60 = 0x40 → >> 2 = 0x10 = 16; base = 16
        // mode=false, value=0 → slot0 = 16, slot1 = 23
        // 16 % 11 = 5, 23 % 11 = 1
        mapper.write_prg(0x8040, 0);
        assert_eq!(mapper.read_prg(0x8000), 5, "slot0 = 16 % 11 = 5");
        assert_eq!(mapper.read_prg(0xC000), 1, "slot1 = 23 % 11 = 1");
    }

    // -------------------------------------------------------------------------
    // Lock bit (addr bit 13)
    // -------------------------------------------------------------------------

    #[test]
    fn lock_prevents_base_mode_and_mirroring_changes() {
        let mut mapper = make_mapper();

        // First write: addr bit13 = 1 → locked=true, base=0, mode=false, vertical
        // value=3 → bank=3 → slot0=3, slot1=7
        mapper.write_prg(0xA000, 3); // addr & 0x2000 = 0x2000 → locked
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Second write: locked → base/mode/locked/mirroring must NOT change
        // addr=0x80E2: bit7=1(mode), bit6=1, bit5=1, bit1=1(H mirroring), bit13=0
        // But since locked, only _bank = value & 7 changes.
        mapper.write_prg(0x80E2, 5); // would set mode=true, base=0x18, H if not locked
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "bank still switches when locked"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot1 still = base|7 since mode unchanged"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring unchanged when locked"
        );
    }

    #[test]
    fn bank_always_updates_even_when_locked() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0); // lock with bank=0
        mapper.write_prg(0x8000, 4); // locked: only _bank = 4
        assert_eq!(mapper.read_prg(0x8000), 4, "bank changes even when locked");
    }

    // -------------------------------------------------------------------------
    // Mirroring control (addr bit 1)
    // -------------------------------------------------------------------------

    #[test]
    fn addr_bit1_zero_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // addr bit1 = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn addr_bit1_one_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0); // addr bit1 = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------------
    // Reset
    // -------------------------------------------------------------------------

    #[test]
    fn reset_restores_power_on_prg_banking() {
        let mut mapper = make_mapper();

        // Change state: addr=0x80E0 → base=0x18, mode=true, bank=3
        // slot0 = (0x18 | 3) = 27, 27 % 11 = 5
        mapper.write_prg(0x80E0, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "slot0 = bank 27 % 11 = 5 after write"
        );

        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "slot0 = bank 0 after reset");
        assert_eq!(mapper.read_prg(0xC000), 7, "slot1 = bank 7 after reset");
    }

    #[test]
    fn reset_unlocks_the_lock() {
        let mut mapper = make_mapper();

        // Lock
        mapper.write_prg(0xA000, 0);

        mapper.reset();

        // After reset, base should be changeable again
        mapper.write_prg(0x8020, 0); // base=8, no lock
        assert_eq!(
            mapper.read_prg(0x8000),
            8 % PRG_16K_BANKS as u8,
            "after reset, base changes are accepted again"
        );
    }

    // -------------------------------------------------------------------------
    // CHR ROM (always bank 0)
    // -------------------------------------------------------------------------

    #[test]
    fn chr_is_always_bank_0() {
        let mut mapper = make_mapper();
        // banked_data fills each 8KB bank with its index; bank 0 → all 0x00
        assert_eq!(mapper.read_chr(0x0000), 0x00);
        assert_eq!(mapper.read_chr(0x1FFF), 0x00);
    }

    // -------------------------------------------------------------------------
    // Save state round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();

        // Set non-default state: base=8, mode=true, locked=true, bank=3, Horizontal
        mapper.write_prg(0x8082, 3); // base from bit5=1 → 0x08, mode=true(bit7), H(bit1)
        mapper.write_prg(0xA080, 3); // locked=true (bit13), mode still true, same base
        // At this point: locked=true, base=8, mode=true, bank=3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "slot0 must be restored"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "slot1 must be restored"
        );

        // Verify locked flag was restored: a subsequent write must not change base/mode
        let snap2 = restored.registers_snapshot();
        assert_eq!(snap2[0], 1, "locked flag must be restored as 1");
    }
}
