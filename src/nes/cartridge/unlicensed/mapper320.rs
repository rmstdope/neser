//! Mapper 320 – BMC-830425C-4391T
//!
//! Specifications:
//! - Primary reference: Mesen2 `Bmc830425C4391T.h`
//! - NesDev: <https://www.nesdev.org/wiki/NES_2.0_Mapper_320>
//!
//! ## Overview
//!
//! Multicart board (e.g., "Super HiK 6-in-1"). Two 16 KB PRG windows with an
//! inner/outer banking scheme supporting UNROM and UOROM modes. CHR is 8 KB RAM.
//! Mirroring is fixed horizontal.
//!
//! ## Register Map ($8000–$FFFF)
//!
//! Any write to $8000–$FFFF:
//! - `inner_reg = value & 0x0F`
//! - If `(addr & 0xFFE0) == 0xF0E0`:
//!   - `outer_reg = addr & 0x0F`
//!   - `prg_mode  = (addr >> 4) & 0x01`
//!
//! ## PRG Banking
//!
//! | Mode   | $8000 bank                          | $C000 bank (fixed)             |
//! |--------|-------------------------------------|-------------------------------|
//! | UOROM (prg_mode=0) | `inner_reg \| (outer_reg << 3)` | `0x0F \| (outer_reg << 3)` |
//! | UNROM (prg_mode=1) | `(inner_reg & 0x07) \| (outer_reg << 3)` | `0x07 \| (outer_reg << 3)` |
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 320;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const OUTER_REG_ADDR_MASK: u16 = 0xFFE0;
const OUTER_REG_ADDR_MATCH: u16 = 0xF0E0;
const REGISTERS_SNAPSHOT_LEN: usize = 3;

pub struct Mapper320 {
    base: BaseMapper,
    inner_reg: u8,
    outer_reg: u8,
    prg_mode: u8,
}

impl Mapper320 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let chr_ram = ChrMemory::new_ram(CHR_BANK_SIZE_BYTES);
        base.set_chr_memory(chr_ram);

        let mut mapper = Self {
            base,
            inner_reg: 0,
            outer_reg: 0,
            prg_mode: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let outer_shift = (self.outer_reg as i16) << 3;
        let (bank_8000, bank_c000) = if self.prg_mode != 0 {
            // UNROM mode: inner uses 3 bits, fixed is 0x07
            let inner = (self.inner_reg & 0x07) as i16;
            (inner | outer_shift, 0x07 | outer_shift)
        } else {
            // UOROM mode: inner uses 4 bits, fixed is 0x0F
            let inner = (self.inner_reg & 0x0F) as i16;
            (inner | outer_shift, 0x0F | outer_shift)
        };

        self.base.select_prg_page(0, bank_8000);
        self.base.select_prg_page(1, bank_c000);
        self.base.select_chr_page(0, 0);
        self.base.set_mirroring(NametableLayout::Horizontal);
    }

    fn apply_state(&mut self, inner_reg: u8, outer_reg: u8, prg_mode: u8) {
        self.inner_reg = inner_reg;
        self.outer_reg = outer_reg;
        self.prg_mode = prg_mode;
        self.update_banks();
    }
}

impl Mapper for Mapper320 {
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
        if addr < 0x8000 {
            return;
        }

        let new_inner = value & 0x0F;
        let (new_outer, new_mode) = if (addr & OUTER_REG_ADDR_MASK) == OUTER_REG_ADDR_MATCH {
            ((addr & 0x0F) as u8, ((addr >> 4) & 0x01) as u8)
        } else {
            (self.outer_reg, self.prg_mode)
        };

        self.apply_state(new_inner, new_outer, new_mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.inner_reg, self.outer_reg, self.prg_mode]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.apply_state(data[0] & 0x0F, data[1] & 0x0F, data[2] & 0x01);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use 48 banks (non-power-of-2) to avoid modulo-wrap false passes
    const PRG_BANKS_16K: usize = 48;

    fn make_mapper() -> Mapper320 {
        Mapper320::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_320_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 320 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_selects_uorom_mode_with_inner_0_and_outer_0() {
        let mapper = make_mapper();
        // UOROM mode: bank0 = 0 | (0 << 3) = 0, bank1 = 0x0F | 0 = 15
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "$C000 should be bank 15 at power-on"
        );
    }

    #[test]
    fn mirroring_is_fixed_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should always be horizontal"
        );
    }

    #[test]
    fn mirroring_is_forced_horizontal_even_when_header_says_vertical() {
        let mapper = Mapper320::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            vec![],
            NametableLayout::Vertical,
        ));
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mapper 320 always enforces horizontal mirroring regardless of ROM header"
        );
    }

    // ── UOROM mode PRG banking (prg_mode = 0) ─────────────────────────────────

    #[test]
    fn uorom_mode_inner_reg_selects_8000_bank_and_0f_is_fixed_at_c000() {
        let mut mapper = make_mapper();
        // Write inner=5 to $8000 (not outer-reg addr), outer=0, mode=0
        mapper.write_prg(0x8000, 0x05);
        // bank_8000 = 5 | (0 << 3) = 5
        // bank_c000 = 0x0F | 0 = 15
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 15, "$C000 should be fixed bank 15");
    }

    #[test]
    fn uorom_mode_with_outer_2_shifts_all_banks() {
        let mut mapper = make_mapper();
        // Set outer=2, mode=0 via outer-reg addr (0xF0E0 | (2 & 0x0F) = 0xF0E2, mode=(0xF0E2>>4)&1=0xE&1=0)
        // Wait: (0xF0E0 >> 4) = 0xF0E, &1 = 0. And outer = 0xF0E0 & 0x0F = 0.
        // Let me pick addr 0xF0E2: (0xF0E2 & 0xFFE0) = 0xF0E0 == 0xF0E0 ✓
        //   outer = 0xF0E2 & 0x0F = 2, mode = (0xF0E2 >> 4) & 1 = 0x0F0E & 1 = 0
        // inner = value & 0x0F; write value=3, inner=3
        mapper.write_prg(0xF0E2, 0x03);
        // bank_8000 = 3 | (2 << 3) = 3 | 16 = 19
        // bank_c000 = 0x0F | 16 = 31
        assert_eq!(mapper.read_prg(0x8000), 19, "$8000 should be bank 19");
        assert_eq!(mapper.read_prg(0xC000), 31, "$C000 should be bank 31");
    }

    // ── UNROM mode PRG banking (prg_mode = 1) ─────────────────────────────────

    #[test]
    fn unrom_mode_inner_uses_only_3_bits_and_07_is_fixed_at_c000() {
        let mut mapper = make_mapper();
        // addr 0xF0F2: (0xF0F2 & 0xFFE0) = 0xF0E0 ✓, outer = 2, mode = (0xF0F2>>4)&1 = 0x0F0F & 1 = 1
        // inner = value & 0x0F = 0x0B & 0x0F = 11, but 3-bit = 11 & 7 = 3
        mapper.write_prg(0xF0F2, 0x0B);
        // bank_8000 = (11 & 7) | (2 << 3) = 3 | 16 = 19
        // bank_c000 = 7 | 16 = 23
        assert_eq!(
            mapper.read_prg(0x8000),
            19,
            "$8000 should be bank 19 in UNROM mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            23,
            "$C000 should be fixed bank 23 in UNROM mode"
        );
    }

    #[test]
    fn unrom_mode_distinguishes_from_uorom_mode_fixed_banks() {
        let mut mapper = make_mapper();
        // In UNROM mode (prg_mode=1) with outer=0, fixed bank at $C000 = 7
        // In UOROM mode (prg_mode=0) with outer=0, fixed bank at $C000 = 15
        // Set UNROM mode: addr = 0xF0F0 → outer=0, mode=1
        mapper.write_prg(0xF0F0, 0x00);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "UNROM fixed bank should be 7 with outer=0"
        );

        // Now set UOROM mode: addr = 0xF0E0 → outer=0, mode=0
        mapper.write_prg(0xF0E0, 0x00);
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "UOROM fixed bank should be 15 with outer=0"
        );
    }

    // ── Outer reg address decode ───────────────────────────────────────────────

    #[test]
    fn writes_outside_outer_reg_range_do_not_change_outer_or_mode() {
        let mut mapper = make_mapper();
        // First set outer=3, mode=1 via $F0F3
        // addr 0xF0F3: (0xF0F3 & 0xFFE0) = 0xF0E0 ✓, outer = 3, mode = 1
        mapper.write_prg(0xF0F3, 0x00);
        // bank_c000 = 7 | (3 << 3) = 7 | 24 = 31
        assert_eq!(mapper.read_prg(0xC000), 31);

        // Now write to non-outer addr (e.g. $8000): only inner changes
        mapper.write_prg(0x8000, 0x04);
        // outer and mode unchanged: bank_c000 = 7 | 24 = 31
        assert_eq!(
            mapper.read_prg(0xC000),
            31,
            "outer/mode should not change from non-outer write"
        );
        // inner=4, mode=1: bank_8000 = (4 & 7) | 24 = 28
        assert_eq!(
            mapper.read_prg(0x8000),
            28,
            "$8000 should use new inner with existing outer"
        );
    }

    #[test]
    fn writes_below_8000_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x05);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "write below $8000 should be ignored"
        );
    }

    // ── CHR RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_unbanked_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0x7E);
        assert_eq!(mapper.read_chr(0x0100), 0x7E, "CHR RAM should be writable");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_inner_outer_and_mode() {
        let mut mapper = make_mapper();
        // Set outer=3, mode=1, inner=5 via write: $F0F3, value=5
        mapper.write_prg(0xF0F3, 0x05);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // UNROM mode (mode=1), outer=3, inner=5&7=5
        // bank_8000 = 5 | (3 << 3) = 5 | 24 = 29
        // bank_c000 = 7 | 24 = 31
        assert_eq!(
            restored.read_prg(0x8000),
            29,
            "snapshot should restore bank_8000=29"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            31,
            "snapshot should restore bank_c000=31"
        );
    }

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF0F3, 0x05);
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "reset should return $8000 to bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "reset should return $C000 to bank 15"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_bmc830425c4391t_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(!caps.has_dynamic_mirroring);
        assert!(!caps.has_chr_banking);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
