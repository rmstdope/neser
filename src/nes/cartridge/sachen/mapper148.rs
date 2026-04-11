//! Mapper 148 - Sachen SA-008-A / Tengen 800008
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_148>
//!
//! Hardware register behavior:
//! - Register write decode: `$8000-$FFFF`, mask `$8000`.
//!   Sachen SA-008-A uses mask `$E000`; this implementation uses `$8000`
//!   (matches Tengen 800008 and is the looser decode).
//! - Bus conflicts: write value is ANDed with PRG-ROM at the written address.
//! - PRG bank (32 KB at `$8000-$FFFF`): bit 3 of the register byte.
//! - CHR bank (8 KB at `$0000-$1FFF`): bits [2:0] of the register byte.
//! - Mirroring: fixed from the ROM header (not programmable).
//!
//! Same bit assignment as INES Mapper 079, but the register lives in
//! `$8000-$FFFF` (PRG-ROM space) instead of `$4100-$5FFF`, introducing
//! bus conflicts.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const PRG_BANK_SHIFT: u8 = 3;
const PRG_BANK_MASK: u8 = 0b0000_0001;
const CHR_BANK_MASK: u8 = 0b0000_0111;

/// Mapper 148 – Sachen SA-008-A / Tengen 800008
pub struct Mapper148 {
    base: BaseMapper,
    register: u8,
}

impl Mapper148 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        base.set_bus_conflicts(true);
        let mut mapper = Self { base, register: 0 };
        mapper.apply_register(0);
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.base.select_prg_page(0, Self::prg_bank(value) as i16);
        self.base.select_chr_page(0, Self::chr_bank(value) as i16);
    }

    fn prg_bank(value: u8) -> u8 {
        (value >> PRG_BANK_SHIFT) & PRG_BANK_MASK
    }

    fn chr_bank(value: u8) -> u8 {
        value & CHR_BANK_MASK
    }
}

impl Mapper for Mapper148 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        let effective = self.base.apply_bus_conflict(addr, value);
        self.apply_register(effective);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }

    fn reset(&mut self) {
        self.apply_register(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 9;

    fn make_mapper() -> Mapper148 {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        Mapper148::new(MapperContext::new_for_test(
            148,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    /// PRG where byte at offset 0 = 0xFF (bus conflict passthrough at $8000)
    /// and byte at offset 1 = bank index (probe via $8001).
    fn make_prg_with_conflict_passthrough() -> Vec<u8> {
        let mut prg = vec![0xFF; PRG_BANK_SIZE * PRG_BANKS];
        for bank in 0..PRG_BANKS {
            prg[bank * PRG_BANK_SIZE + 1] = bank as u8;
        }
        prg
    }

    fn make_mapper_passthrough() -> Mapper148 {
        let prg = make_prg_with_conflict_passthrough();
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        Mapper148::new(MapperContext::new_for_test(
            148,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_148_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            148,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 148 must be creatable via factory");
    }

    #[test]
    fn power_on_prg_bank_is_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG bank 0 at $8000 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 at power-on");
    }

    #[test]
    fn bit_3_selects_32k_prg_bank() {
        let mut mapper = make_mapper_passthrough();

        // Write 0x00 → bit 3 = 0 → PRG bank 0; bus conflict: 0x00 & 0xFF = 0x00
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_prg(0x8001), 0, "PRG bank 0 when bit 3 = 0");

        // Write 0x08 → bit 3 = 1 → PRG bank 1; bus conflict: 0x08 & 0xFF = 0x08
        mapper.write_prg(0x8000, 0b0000_1000);
        assert_eq!(mapper.read_prg(0x8001), 1, "PRG bank 1 when bit 3 = 1");
    }

    #[test]
    fn bits_2_0_select_8k_chr_bank() {
        let mut mapper = make_mapper_passthrough();

        mapper.write_prg(0x8000, 0b0000_0101);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank 5 from bits [2:0] = 101"
        );
    }

    #[test]
    fn bus_conflicts_and_write_value_with_prg_rom() {
        // PRG ROM with bank ID bytes (banked_data format): bank 0 byte = 0, bank 1 = 1
        // At $8000 PRG ROM = 0 (bank 0 first byte). AND(0b1111, 0) = 0 -> bank 0, chr 0.
        let mut mapper = make_mapper();

        // write 0b1111 but ROM at $8000 (bank 0) = 0, so effective = 0
        mapper.write_prg(0x8000, 0b0000_1111);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Bus conflict: 0x0F & 0x00 = 0x00 → bank 0"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "Bus conflict: chr bits = 0");
    }

    #[test]
    fn register_responds_to_all_addresses_in_8000_ffff() {
        let mut mapper = make_mapper_passthrough();

        // Write to $FFFF: bus conflict 0x08 & 0xFF = 0x08 → PRG bank 1
        mapper.write_prg(0xFFFF, 0b0000_1000);
        assert_eq!(
            mapper.read_prg(0x8001),
            1,
            "Register responds at $FFFF: probe bank ID at $8001"
        );
    }

    #[test]
    fn writes_below_8000_are_ignored() {
        let mut mapper = make_mapper_passthrough();

        mapper.write_prg(0x8000, 0b0000_1000);
        assert_eq!(
            mapper.read_prg(0x8001),
            1,
            "PRG bank 1 before write below $8000"
        );

        mapper.write_prg(0x7FFF, 0b0000_0000);
        assert_eq!(
            mapper.read_prg(0x8001),
            1,
            "Writes below $8000 must be ignored"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header"
        );
    }

    #[test]
    fn mirroring_unchanged_after_write() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must not change on write"
        );
    }

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0b0000_1011); // PRG bank 1, CHR bank 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_passthrough();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8001),
            mapper.read_prg(0x8001),
            "Snapshot must preserve PRG bank"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Snapshot must preserve CHR bank"
        );
    }

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0b0000_1011);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8001), 0, "PRG bank 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
    }
}
