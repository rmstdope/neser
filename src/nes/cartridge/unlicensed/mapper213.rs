//! Mapper 213 – BMC multicart (9999999-in-1 / 168-in-1)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_213>
//! - Reference implementation: Mesen2 `Mapper213.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Mapper213.h>
//!
//! ## Hardware behavior
//!
//! NESdev notes that this mapper is "a duplicate of INES Mapper 058", and both
//! known ROMs work with mapper 58 behavior.  Mesen implements it as a distinct
//! variant where banking is decoded differently:
//!
//! ```text
//! A~[.... .... .CCC P...] (write to $8000–$FFFF, data ignored)
//!                || +--- PRG A16..A15 → 32 KiB PRG bank
//!                ++---- CHR A15..A13 → 8 KiB CHR bank
//! ```
//!
//! More precisely:
//! - PRG: 32 KiB bank `(addr >> 1) & 0x03` at `$8000–$FFFF`
//! - CHR: 8 KiB bank `(addr >> 3) & 0x07` at `$0000–$1FFF`
//! - Mirroring: fixed from the ROM header (no dynamic control)
//!
//! Known ROMs: *9999999-in-1*, *168-in-1*.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: bank 0.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 213;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 213 – BMC multicart (9999999-in-1 / 168-in-1).
///
/// See the module-level documentation for hardware details.
pub struct Mapper213 {
    base: BaseMapper,
    /// Bits A2:A1 of the write address: selects the 32 KiB PRG bank.
    prg_bank: u8,
    /// Bits A5:A3 of the write address: selects the 8 KiB CHR bank.
    chr_bank: u8,
}

impl Mapper213 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper213 {
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
        if addr < 0x8000 {
            return;
        }
        // Banking is determined by address lines; data byte is ignored.
        let _ = value;
        self.prg_bank = ((addr >> 1) & 0x03) as u8;
        self.chr_bank = ((addr >> 3) & 0x07) as u8;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 4; // 4 × 32 KiB = 128 KiB
    const CHR_BANKS: usize = 8; // 8 × 8 KiB = 64 KiB

    fn make_mapper() -> Mapper213 {
        Mapper213::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_213_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 213 must be creatable via factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_maps_prg_and_chr_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should read bank 0 byte 0 on power-on"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should read bank 0 byte 0 on power-on"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_selects_prg_bank_from_addr_bits_a2_a1() {
        let mut mapper = make_mapper();
        // addr bits A2:A1 = 0b10 → PRG bank 2: use addr = 0x8000 | (2 << 1) = 0x8004
        mapper.write_prg(0x8004, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank 2 should be selected by addr bits A2:A1 = 0b10"
        );
    }

    #[test]
    fn write_selects_all_prg_banks() {
        let mut mapper = make_mapper();
        for bank in 0u8..4 {
            mapper.write_prg(0x8000 | ((bank as u16) << 1), 0);
            assert_eq!(
                mapper.read_prg(0x8000),
                bank,
                "PRG bank {bank} should be selectable"
            );
        }
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_selects_chr_bank_from_addr_bits_a5_a3() {
        let mut mapper = make_mapper();
        // addr bits A5:A3 = 0b101 = 5 → CHR bank 5: use addr = 0x8000 | (5 << 3) = 0x8028
        mapper.write_prg(0x8028, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank 5 should be selected by addr bits A5:A3 = 0b101"
        );
    }

    #[test]
    fn prg_and_chr_banks_are_independent() {
        let mut mapper = make_mapper();
        // PRG bank 3 (A2:A1=11), CHR bank 6 (A5:A3=110):
        // addr = 0x8000 | (3 << 1) | (6 << 3) = 0x8000 | 6 | 48 = 0x8036
        mapper.write_prg(0x8036, 0);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank 3 from A2:A1");
        assert_eq!(mapper.read_chr(0x0000), 6, "CHR bank 6 from A5:A3");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8036, 0); // PRG=3, CHR=6

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_bank, mapper.prg_bank);
        assert_eq!(restored.chr_bank, mapper.chr_bank);
        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8036, 0);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should return to bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should return to bank 0 after reset"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper213::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        mapper.write_chr(0x0200, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0200),
            0xCD,
            "CHR-RAM write/read must work"
        );
    }
}
