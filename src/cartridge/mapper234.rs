//! Mapper 234 - Maxi 15 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_234>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper234.h`
//!
//! Known Limitations:
//! - The lockout defeat control register ($FFC0-$FFDF) is not implemented
//!   (emulators do not need to implement it per spec).

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use std::cell::Cell;

const MAPPER_NUMBER: u16 = 234;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Bit 7 of the outer register: 0 = Vertical mirroring, 1 = Horizontal.
const MIRRORING_FLAG: u8 = 0x80;
/// Bit 6 of the outer register: 0 = CNROM mode, 1 = NINA-03 mode.
const NINA03_MODE_FLAG: u8 = 0x40;
/// Bits 5-0 of the outer register: non-zero means the register is locked.
const OUTER_LOCK_MASK: u8 = 0x3F;
/// Only bits 6-4 (fine CHR) and bit 0 (PRG page) of the inner register are used.
const INNER_REG_MASK: u8 = 0x71;

/// Mapper 234 – Maxi 15 multicart
///
/// Bankswitching is triggered by CPU reads (and writes) from $FF80–$FFF8:
///
/// - **$FF80–$FF9F**: Outer bank control register (`MOQq BBBb`)
///   - Bit 7 (M): Mirroring (0 = Vertical, 1 = Horizontal)
///   - Bit 6 (O): Mode (0 = CNROM, 1 = NINA-03)
///   - Bits 5-0 (QqBBBb): Block/ROM selection — once any of these bits are
///     non-zero the outer register is *locked* and subsequent writes/reads to
///     this range are ignored.
///
/// - **$FFE8–$FFF8**: Inner bank control register (`.cCC ...P`)
///   - Bits 6-4 (cCC): Fine CHR selection
///   - Bit 0 (P): PRG page (NINA-03 mode only)
///   - Stored masked with `0x71`; always updatable (not locked).
///
/// Banking — CNROM mode (O = 0):
///   - 32 KiB PRG bank = `outer & 0x0F`          (BBBb)
///   - 8 KiB CHR bank  = `((outer << 2) & 0x3C) | ((inner >> 4) & 0x03)`
///
/// Banking — NINA-03 mode (O = 1):
///   - 32 KiB PRG bank = `(outer & 0x0E) | (inner & 0x01)`   (BBBP)
///   - 8 KiB CHR bank  = `((outer << 2) & 0x38) | ((inner >> 4) & 0x07)`
pub struct Mapper234 {
    base: BaseMapper,
    /// Outer bank control register. Uses `Cell` for interior mutability, as
    /// bankswitching is triggered on CPU *reads* (`read_prg` is `&self`).
    outer_reg: Cell<u8>,
    /// Inner bank control register. Similarly updated on reads.
    inner_reg: Cell<u8>,
}

impl Mapper234 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            ..Default::default()
        };
        let base = BaseMapper::new(&ctx, capabilities);
        Self {
            base,
            outer_reg: Cell::new(0),
            inner_reg: Cell::new(0),
        }
    }

    fn is_nina03_mode(&self) -> bool {
        self.outer_reg.get() & NINA03_MODE_FLAG != 0
    }

    fn prg_bank(&self) -> usize {
        let outer = self.outer_reg.get();
        let inner = self.inner_reg.get();
        if self.is_nina03_mode() {
            // NINA-03 mode: BBBP
            ((outer & 0x0E) | (inner & 0x01)) as usize
        } else {
            // CNROM mode: BBBb
            (outer & 0x0F) as usize
        }
    }

    fn chr_bank(&self) -> usize {
        let outer = self.outer_reg.get();
        let inner = self.inner_reg.get();
        if self.is_nina03_mode() {
            // NINA-03 mode: BBBcCC
            ((outer as usize) << 2 & 0x38) | ((inner as usize) >> 4 & 0x07)
        } else {
            // CNROM mode: BBBbCC
            ((outer as usize) << 2 & 0x3C) | ((inner as usize) >> 4 & 0x03)
        }
    }

    /// Maps a PPU address to a CHR ROM/RAM index using the current `chr_bank`.
    /// Returns `None` when CHR memory is absent (size zero).
    fn chr_index_for(&self, addr: u16) -> Option<usize> {
        let chr_size = self.base.chr_size();
        if chr_size == 0 {
            return None;
        }
        let bank = self.chr_bank();
        let offset = (addr & 0x1FFF) as usize;
        Some((bank * CHR_BANK_SIZE + offset) % chr_size)
    }

    /// Writes `value` into the outer register only if the register is not yet
    /// locked (bits 5-0 are all zero). Once locked it ignores further writes.
    fn latch_outer_reg(&self, value: u8) {
        if self.outer_reg.get() & OUTER_LOCK_MASK == 0 {
            self.outer_reg.set(value);
        }
    }

    fn update_inner_reg(&self, value: u8) {
        self.inner_reg.set(value & INNER_REG_MASK);
    }
}

impl Mapper for Mapper234 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn get_mirroring(&self) -> NametableLayout {
        if self.outer_reg.get() & MIRRORING_FLAG != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let prg_rom = self.base.prg_rom();
        if prg_rom.is_empty() {
            return 0;
        }
        let bank = self.prg_bank();
        let offset = (addr - 0x8000) as usize;
        let index = (bank * PRG_BANK_SIZE + offset) % prg_rom.len();
        let value = prg_rom[index];
        if (0xFF80..=0xFF9F).contains(&addr) {
            self.latch_outer_reg(value);
        } else if (0xFFE8..=0xFFF8).contains(&addr) {
            self.update_inner_reg(value);
        }
        value
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        match self.chr_index_for(addr) {
            Some(index) => self.base.read_chr_at_index(index),
            None => 0,
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if let Some(index) = self.chr_index_for(addr) {
            self.base.write_chr_at_index(index, value);
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0xFF80..=0xFF9F).contains(&addr) {
            self.latch_outer_reg(value);
        } else if (0xFFE8..=0xFFF8).contains(&addr) {
            self.update_inner_reg(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.outer_reg.get(), self.inner_reg.get()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.outer_reg.set(data[0]);
            self.inner_reg.set(data[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16; // 16 × 32 KB = 512 KB
    const CHR_BANKS: usize = 64; // 64 × 8 KB = 512 KB

    fn create_mapper234(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper234 {
        Mapper234::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    fn prg_rom() -> Vec<u8> {
        banked_data(PRG_BANK_SIZE, PRG_BANKS)
    }

    fn chr_rom() -> Vec<u8> {
        banked_data(CHR_BANK_SIZE, CHR_BANKS)
    }

    // ── Factory registration ──────────────────────────────────────────────

    #[test]
    fn mapper_234_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom(),
            chr_rom(),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 234 should be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────

    #[test]
    fn power_on_prg_reads_from_bank_0() {
        let mapper = create_mapper234(prg_rom(), chr_rom());
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should return bank-0 data on power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = create_mapper234(prg_rom(), chr_rom());
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Outer register / CNROM PRG banking ───────────────────────────────

    #[test]
    fn write_outer_reg_selects_prg_bank_cnrom_mode() {
        // CNROM mode (bit 6 = 0): PRG bank = outer & 0x0F
        // Write 0x03 → PRG bank 3
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank should be 3 after writing outer_reg = 0x03"
        );
    }

    #[test]
    fn write_outer_reg_selects_prg_bank_upper_nibble_ignored_in_cnrom() {
        // CNROM mode: PRG bank = outer & 0x0F — only lower nibble matters
        // Write 0x05 (BBBb = 0101) → PRG bank 5
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    // ── Outer register / CNROM CHR banking ───────────────────────────────

    #[test]
    fn write_outer_reg_selects_chr_bank_cnrom_mode() {
        // CNROM mode: CHR bank = ((outer << 2) & 0x3C) | ((inner >> 4) & 0x03)
        // outer = 0x02 (BBBb=0010), inner = 0x00
        // CHR = (0x02 << 2 & 0x3C) | 0 = 0x08 | 0 = 8
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            8,
            "CHR bank 8 should be visible after outer_reg = 0x02"
        );
    }

    #[test]
    fn write_inner_reg_selects_chr_fine_bank_cnrom_mode() {
        // outer = 0x00 (CNROM), inner = 0x10 (bit 4 set)
        // CHR = ((0x00 << 2) & 0x3C) | ((0x10 >> 4) & 0x03) = 0 | 1 = 1
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFFE8, 0x10);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR fine bank 1 should be visible after inner_reg bit4 set"
        );
    }

    // ── NINA-03 mode banking ──────────────────────────────────────────────

    #[test]
    fn nina03_mode_prg_bank_from_outer_and_inner() {
        // NINA-03 mode (outer bit 6 = 1): PRG = (outer & 0x0E) | (inner & 0x01)
        // outer = 0x44 (0100 0100) → BBB = 010 → (0x44 & 0x0E) = 0x04
        // inner = 0x01 → P = 1
        // PRG bank = 0x04 | 0x01 = 5
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x44);
        mapper.write_prg(0xFFE8, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "PRG bank should be 5 in NINA-03 mode (BBB=2, P=1)"
        );
    }

    #[test]
    fn nina03_mode_chr_bank_from_outer_and_inner() {
        // NINA-03 mode: CHR = ((outer << 2) & 0x38) | ((inner >> 4) & 0x07)
        // outer = 0x40 (0100 0000) → (0x40 << 2) = 0x100; 0x100 & 0x38 = 0x00
        // inner = 0x71 (0111 0001) → (0x71 >> 4) = 0x07; 0x07 & 0x07 = 7
        // CHR bank = 0 | 7 = 7
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x40);
        mapper.write_prg(0xFFE8, 0x71);
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "CHR bank should be 7 in NINA-03 mode (BBB=0, cCC=7)"
        );
    }

    #[test]
    fn nina03_mode_chr_bank_uses_outer_block_bits() {
        // NINA-03 mode: CHR = ((outer << 2) & 0x38) | ((inner >> 4) & 0x07)
        // outer = 0x4E (0100 1110) → (0x4E << 2) = 0x138; 0x138 & 0x38 = 0x38
        // inner = 0x71 → (0x71 >> 4) & 7 = 7
        // CHR bank = 0x38 | 7 = 0x3F = 63
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x4E);
        mapper.write_prg(0xFFE8, 0x71);
        assert_eq!(
            mapper.read_chr(0x0000),
            63,
            "CHR bank should be 63 (max) with outer block=7 and inner fine=7"
        );
    }

    // ── Locking mechanism ─────────────────────────────────────────────────

    #[test]
    fn outer_reg_locks_once_lower_6_bits_are_set() {
        // Writing 0x01 (bit 0 set) locks the outer register
        // Subsequent writes to $FF80 should be ignored
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x01); // PRG bank = 1, now locked
        mapper.write_prg(0xFF80, 0x0F); // should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Outer register should be locked after bit-0 set; bank stays at 1"
        );
    }

    #[test]
    fn outer_reg_does_not_lock_on_mode_and_mirroring_bits_only() {
        // Bits 7 (M) and 6 (O) do not trigger locking; only bits 5-0 do
        // Writing 0xC0 (bits 7+6 set, bits 5-0 = 0) leaves register unlocked
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0xC0); // mode=NINA-03, horizontal, bits 5-0 = 0
        mapper.write_prg(0xFF80, 0x02); // should succeed (register is not locked)
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Outer register should accept 0x02 after 0xC0 (no lock)"
        );
    }

    #[test]
    fn inner_reg_always_updates_even_when_outer_locked() {
        // Lock the outer reg, then verify inner reg still updates
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x01); // lock outer (PRG bank 1)
        mapper.write_prg(0xFFE8, 0x10); // inner: bit4 set → CHR fine bank 1
        // CHR = ((outer << 2) & 0x3C) | ((inner >> 4) & 0x03)
        //     = ((0x01 << 2) & 0x3C) | ((0x10 >> 4) & 0x03)
        //     = (0x04 & 0x3C) | 1 = 0x04 | 1 = 5
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "Inner register should be updatable even when outer is locked"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────

    #[test]
    fn mirroring_horizontal_when_outer_bit7_set() {
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x80); // bit 7 = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_vertical_when_outer_bit7_clear() {
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x00); // bit 7 = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_toggles_from_horizontal_to_vertical() {
        // First set to horizontal (0x80 has bits 5-0 = 0, so unlocked)
        // Then update to 0x00 (vertical)
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xFF80, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Read-triggered bankswitching ──────────────────────────────────────

    #[test]
    fn read_prg_at_ff80_triggers_outer_reg_update() {
        // Build a PRG ROM based on banked_data (bank N filled with byte N),
        // then override bank 0's $7F80 offset (= $FF80 address) to value 0x03.
        // Reading $FF80 should latch 0x03 into outer_reg → PRG bank 3 becomes active.
        let mut prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        prg[0x7F80] = 0x03; // bank 0, offset $7F80 = addr $FF80
        let mapper = create_mapper234(prg, chr_rom());
        let value = mapper.read_prg(0xFF80);
        assert_eq!(value, 0x03, "Read should return ROM byte 0x03 at $FF80");
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Reading $FF80 should latch outer_reg and switch PRG to bank 3"
        );
    }

    #[test]
    fn read_prg_at_ffe8_triggers_inner_reg_update() {
        // Build a PRG ROM where bank 0, offset 0x7FE8 ($FFE8 within 32KB) = 0x10
        // Reading from $FFE8 should latch 0x10 & 0x71 = 0x10 into inner_reg
        // CHR = ((0x00 << 2) & 0x3C) | ((0x10 >> 4) & 0x03) = 0 | 1 = 1
        let mut prg = vec![0u8; PRG_BANK_SIZE * PRG_BANKS];
        prg[0x7FE8] = 0x10; // bank 0, offset $7FE8 = addr $FFE8
        let mut mapper = create_mapper234(prg, chr_rom());
        let value = mapper.read_prg(0xFFE8);
        assert_eq!(value, 0x10, "Read should return ROM byte at $FFE8");
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Reading $FFE8 should latch inner_reg and switch CHR to fine bank 1"
        );
    }

    // ── CHR-RAM support ───────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_readable_and_writable_without_chr_rom() {
        let mut mapper = create_mapper234(prg_rom(), vec![]);
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    // ── Inner register mask ───────────────────────────────────────────────

    #[test]
    fn inner_reg_is_stored_masked_with_0x71() {
        // Writing 0xFF to inner reg should store 0xFF & 0x71 = 0x71
        // CHR in CNROM mode: ((0x00 << 2) & 0x3C) | ((0x71 >> 4) & 0x03)
        //   = 0 | (0x07 & 0x03) = 0 | 3 = 3
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFFE8, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "Inner reg 0xFF masked to 0x71; CNROM CHR fine bits should be 3"
        );
    }

    // ── Save state ────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = create_mapper234(prg_rom(), chr_rom());
        mapper.write_prg(0xFF80, 0x03); // outer: CNROM, PRG bank 3
        mapper.write_prg(0xFFE8, 0x10); // inner: CHR fine bank 1

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper234(prg_rom(), chr_rom());
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 3, "PRG bank 3 after restore");
        assert_eq!(
            restored.read_chr(0x0000),
            // CHR = ((0x03 << 2) & 0x3C) | ((0x10 >> 4) & 0x03) = 0x0C | 1 = 13
            13,
            "CHR bank 13 after restore"
        );
    }
}
