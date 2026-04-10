//! Mapper 203 – 35-in-1 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_203>
//! - Reference implementation: Mesen2 `Mapper203.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Mapper203.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` selects the active PRG and CHR banks
//! from the **data byte**:
//!
//! ```text
//! D~[PPPP PPCC]
//!    ||| || ++-- CHR A14..A13  → 8 KiB CHR bank at PPU $0000–$1FFF
//!    +++-++---- PRG A19..A14  → 16 KiB PRG bank (mirrored at $8000 and $C000)
//! ```
//!
//! **PRG banking (16 KiB pages):**
//! - Both `$8000–$BFFF` and `$C000–$FFFF` map to the same 16 KiB bank `value >> 2`.
//!
//! **CHR banking:** 8 KiB bank `value & 0x03` at `$0000–$1FFF`.
//!
//! **Mirroring:** fixed from the ROM header; no dynamic control.
//!
//! The only known ROM uses 64 KiB PRG-ROM (4 × 16 KiB) and 32 KiB CHR-ROM
//! (4 × 8 KiB), so in practice only bits 3:2 (PRG) and 1:0 (CHR) matter.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: bank 0, vertical mirroring (header default).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 203;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 203 – 35-in-1 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper203 {
    base: BaseMapper,
    /// Bits [7:2] of the last write data byte: selects the 16 KiB PRG bank.
    prg_bank: u8,
    /// Bits [1:0] of the last write data byte: selects the 8 KiB CHR bank.
    chr_bank: u8,
}

impl Mapper203 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
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
        // Both PRG slots mirror the same 16 KiB bank (NROM-128 style).
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper203 {
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
        self.prg_bank = value >> 2;
        self.chr_bank = value & 0x03;
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

    const PRG_BANKS: usize = 4; // 4 × 16 KiB = 64 KiB (typical ROM)
    const CHR_BANKS: usize = 4; // 4 × 8 KiB = 32 KiB

    fn make_mapper() -> Mapper203 {
        Mapper203::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_203_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 203 must be creatable via factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_both_prg_slots_map_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 ($8000-$BFFF) should read bank 0 byte 0 on power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG slot 1 ($C000-$FFFF) should also read bank 0 byte 0 on power-on"
        );
    }

    #[test]
    fn power_on_chr_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should read bank 0 byte 0 on power-on"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_selects_prg_bank_from_bits_7_to_2() {
        let mut mapper = make_mapper();
        // value = 0b00001100 → bits[7:2] = 0b000011 = 3 → bank 3
        mapper.write_prg(0x8000, 0b0000_1100);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG slot 0 should read bank 3 first byte"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "PRG slot 1 should also read bank 3 (NROM-128 mirroring)"
        );
    }

    #[test]
    fn both_prg_slots_always_mirror_same_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0100); // bank 1
        assert_eq!(mapper.read_prg(0x8000), mapper.read_prg(0xC000));
        mapper.write_prg(0x8000, 0b0000_1000); // bank 2
        assert_eq!(mapper.read_prg(0x8000), mapper.read_prg(0xC000));
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_selects_chr_bank_from_bits_1_to_0() {
        let mut mapper = make_mapper();
        // value = 0b00000010 → bits[1:0] = 0b10 = 2 → CHR bank 2
        mapper.write_prg(0x8000, 0b0000_0010);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR should read bank 2 first byte"
        );
    }

    #[test]
    fn write_sets_prg_and_chr_independently() {
        let mut mapper = make_mapper();
        // PRG bank 2, CHR bank 3: value = (2 << 2) | 3 = 0b00001011
        mapper.write_prg(0x9FFF, (2 << 2) | 3);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank should be 2");
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank should be 3");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, (1 << 2) | 2); // PRG=1, CHR=2

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.prg_bank, mapper.prg_bank,
            "prg_bank must be preserved"
        );
        assert_eq!(
            restored.chr_bank, mapper.chr_bank,
            "chr_bank must be preserved"
        );
        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, (3 << 2) | 3);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper203::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM write/read must work"
        );
    }
}
