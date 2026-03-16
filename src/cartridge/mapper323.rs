//! Mapper 323 - Farid SLROM (FaridSlrom)
//!
//! ## Specifications
//!
//! - Primary reference: Mesen2 `FaridSlrom` (`Core/NES/Mappers/Homebrew/FaridSlrom.h`)
//! - NesDev page returned HTTP 403 at time of implementation.
//!
//! ## Overview
//!
//! FaridSlrom is a homebrew MMC1-based board that extends the standard MMC1 (SxROM)
//! with an outer-bank register written to `$6000–$7FFF`. The outer bank selects a
//! block of 8 PRG banks (16 KiB each) and 32 CHR banks (4 KiB each), allowing up to
//! 1 MiB PRG-ROM and 1 MiB CHR-ROM.
//!
//! ## Outer-bank register (`$6000–$7FFF` write)
//!
//! - Written only when MMC1 WRAM is enabled (PRG bank register bit 4 = 0) and not locked.
//! - Bits 6:4 → outer bank (stored as `(value & 0x70) >> 1`, i.e., moved to bits 5:3).
//! - Bit 3    → lock; when set, the outer bank register cannot be changed.
//!
//! ## Banking
//!
//! All PRG/CHR page selection follows the MMC1 serial-register modes, but every
//! `SelectPrgPage` / `SelectChrPage` result is modified:
//!
//! - PRG: `outer_bank | (mmc1_page & 0x07)` — 8 PRG banks per outer block.
//! - CHR: `(outer_bank << 2) | (mmc1_chr_page & 0x1F)` — 32 CHR banks per outer block.
//!
//! ## Known Limitations
//!
//! - No PRG-RAM on SLROM hardware; the `$6000–$7FFF` range is write-only (outer bank reg).
//! - Soft-reset vs hard-reset distinction not preserved; both reset to power-on state.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::mmc1::MMC1Mapper;

// Indices into MMC1 `registers_snapshot()` output.
const MMC1_SNAP_CONTROL: usize = 2;
const MMC1_SNAP_CHR_BANK_0: usize = 3;
const MMC1_SNAP_CHR_BANK_1: usize = 4;
const MMC1_SNAP_PRG_BANK: usize = 5;

/// Mapper 323 - Farid SLROM (homebrew extended MMC1).
///
/// Hardware summary:
/// - PRG-ROM: up to 1 MiB (64 × 16 KiB banks via outer-bank extension)
/// - CHR-ROM/RAM: up to 1 MiB (256 × 4 KiB banks in 4 KiB mode)
/// - PRG-RAM: none
/// - Mirroring: programmable via MMC1 control register
/// - IRQ: none
/// - Audio: none
pub struct Mapper323 {
    inner: MMC1Mapper,
    /// Outer-bank value as stored: `(write_byte & 0x70) >> 1` → bits 5:3.
    outer_bank: u8,
    /// When `true`, writes to `$6000–$7FFF` do not update `outer_bank`.
    locked: bool,
}

impl Mapper323 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let inner = MMC1Mapper::new(ctx);
        let mut mapper = Self {
            inner,
            outer_bank: 0,
            locked: false,
        };
        mapper.apply_outer_banks();
        mapper
    }

    /// Returns `true` when MMC1's WRAM-disable bit (PRG bank register bit 4) is clear.
    fn is_wram_enabled(&self) -> bool {
        let regs = self.inner.registers_snapshot();
        regs.get(MMC1_SNAP_PRG_BANK)
            .map(|&b| (b & 0x10) == 0)
            .unwrap_or(true)
    }

    /// Re-applies PRG and CHR page selections with the outer-bank offset on top of
    /// whatever MMC1 last calculated.  Must be called after any MMC1 register commit
    /// or after the outer-bank register itself changes.
    fn apply_outer_banks(&mut self) {
        let regs = self.inner.registers_snapshot();
        if regs.len() <= MMC1_SNAP_PRG_BANK {
            return;
        }

        let control = regs[MMC1_SNAP_CONTROL];
        let chr_bank_0 = regs[MMC1_SNAP_CHR_BANK_0];
        let chr_bank_1 = regs[MMC1_SNAP_CHR_BANK_1];
        let prg_bank = regs[MMC1_SNAP_PRG_BANK];

        self.apply_outer_prg(control, prg_bank);
        self.apply_outer_chr(control, chr_bank_0, chr_bank_1);
    }

    fn apply_outer_prg(&mut self, control: u8, prg_bank: u8) {
        let prg_mode = (control >> 2) & 0x03;
        match prg_mode {
            0 | 1 => {
                // 32 KiB mode: whole $8000–$FFFF switches as a pair.
                let bank_32k = (prg_bank & 0x0E) >> 1;
                let page0 = (bank_32k * 2) & 0x07;
                let page1 = (bank_32k * 2 + 1) & 0x07;
                self.inner
                    .base_mut()
                    .select_prg_page(0, (self.outer_bank | page0) as i16);
                self.inner
                    .base_mut()
                    .select_prg_page(1, (self.outer_bank | page1) as i16);
            }
            2 => {
                // Fix first bank at $8000, switch 16 KiB at $C000.
                let page1 = prg_bank & 0x07;
                self.inner
                    .base_mut()
                    .select_prg_page(0, self.outer_bank as i16);
                self.inner
                    .base_mut()
                    .select_prg_page(1, (self.outer_bank | page1) as i16);
            }
            3 => {
                // Switch 16 KiB at $8000, fix last bank at $C000.
                let page0 = prg_bank & 0x07;
                self.inner
                    .base_mut()
                    .select_prg_page(0, (self.outer_bank | page0) as i16);
                self.inner
                    .base_mut()
                    .select_prg_page(1, (self.outer_bank | 0x07) as i16);
            }
            _ => unreachable!(),
        }
    }

    fn apply_outer_chr(&mut self, control: u8, chr_bank_0: u8, chr_bank_1: u8) {
        let chr_outer = (self.outer_bank as u16) << 2;
        let chr_mode = (control >> 4) & 0x01;
        if chr_mode == 0 {
            // 8 KiB mode: chr_bank_0 selects an 8 KiB pair (bit 0 ignored).
            let bank_8k = ((chr_bank_0 & 0x1E) >> 1) as u16;
            let page0 = (bank_8k * 2) & 0x1F;
            let page1 = (bank_8k * 2 + 1) & 0x1F;
            self.inner
                .base_mut()
                .select_chr_page(0, (chr_outer | page0) as i16);
            self.inner
                .base_mut()
                .select_chr_page(1, (chr_outer | page1) as i16);
        } else {
            // 4 KiB mode: two independent 4 KiB banks.
            let page0 = (chr_bank_0 & 0x1F) as u16;
            let page1 = (chr_bank_1 & 0x1F) as u16;
            self.inner
                .base_mut()
                .select_chr_page(0, (chr_outer | page0) as i16);
            self.inner
                .base_mut()
                .select_chr_page(1, (chr_outer | page1) as i16);
        }
    }
}

impl Mapper for Mapper323 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            // Mapper 323 spec: $6000–$7FFF is write-only (no PRG-RAM), so reads are open bus.
            0x6000..=0x7FFF => open_bus,
            _ => self.inner.read_prg_open_bus(addr, open_bus),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.is_wram_enabled() && !self.locked {
                    self.outer_bank = (value & 0x70) >> 1;
                    self.locked = (value & 0x08) != 0;
                    self.apply_outer_banks();
                }
            }
            0x8000..=0xFFFF => {
                self.inner.write_prg(addr, value);
                self.apply_outer_banks();
            }
            _ => {}
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.inner.write_chr(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.inner.read_chr(addr)
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn get_mirroring(&self) -> crate::cartridge::NametableLayout {
        self.inner.get_mirroring()
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = vec![self.outer_bank, self.locked as u8];
        snap.extend(self.inner.registers_snapshot());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.outer_bank = data[0];
            self.locked = data[1] != 0;
            self.inner.restore_registers(&data[2..]);
            self.apply_outer_banks();
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.outer_bank = 0;
        self.locked = false;
        self.apply_outer_banks();
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.inner.capabilities()
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const MAPPER_NUMBER: u16 = 323;
    // Non-power-of-two to catch modulo-wrapping false-passes.
    const PRG_BANKS_16K: usize = 20;
    const CHR_BANKS_4K: usize = 48; // must be > 41 (outer=8→32, inner=9→41)
    const PRG_BANK_SIZE: usize = 16 * 1024;
    const CHR_BANK_SIZE: usize = 4 * 1024;

    /// Write a 5-bit value to a MMC1 serial register.
    ///
    /// MMC1 requires two CPU cycles between consecutive serial writes to prevent
    /// the consecutive-write filter from dropping bits.
    fn write_mmc1_reg(mapper: &mut Mapper323, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_cycle();
            mapper.cpu_cycle();
            mapper.write_prg(addr, (value >> bit) & 0x01);
        }
    }

    fn make_mapper() -> Mapper323 {
        Mapper323::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_4K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_323_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_4K),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 323 must be registered in the factory"
        );
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_lower_prg_window_maps_to_bank_0() {
        // MMC1 default: mode 3 (switch at $8000, fix last at $C000), outer_bank=0
        // $8000-$BFFF = outer_bank | (prg_bank & 0x07) = 0 | 0 = 0
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_upper_prg_window_maps_to_last_inner_bank() {
        // MMC1 default: mode 3, outer_bank=0
        // $C000-$FFFF = outer_bank | 0x07 = 7
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must map to bank 7 (last of outer block 0) at power-on"
        );
    }

    #[test]
    fn power_on_chr_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR $0000 must be bank 0");
    }

    // ── PRG outer-bank switching (mode 3) ────────────────────────────────────

    #[test]
    fn outer_bank_write_shifts_prg_block_by_8() {
        // Write outer_bank value selecting block 1 (bits 6:4 = 0b001 → outer = 0x08)
        // value = 0b0001_0000 = 0x10 → outer_bank = (0x10 & 0x70) >> 1 = 0x08
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10);
        // $8000: outer_bank | (prg_bank & 0x07) = 8 | 0 = 8
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 must shift to bank 8");
        // $C000: outer_bank | 0x07 = 15
        assert_eq!(mapper.read_prg(0xC000), 15, "$C000 must shift to bank 15");
    }

    #[test]
    fn mmc1_prg_bank_switch_uses_inner_3_bits_within_outer_block() {
        // Set outer block 1 (outer_bank = 8), then switch inner PRG bank to 3
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        // MMC1 mode 3: write PRG bank register ($E000-$FFFF) with value 3
        // inner page = prg_bank & 0x07 = 3; effective = 8 | 3 = 11
        write_mmc1_reg(&mut mapper, 0xE000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            11,
            "$8000: outer 8 + inner 3 = bank 11"
        );
        // Fixed last: outer | 0x07 = 15
        assert_eq!(mapper.read_prg(0xC000), 15, "$C000 stays at bank 15");
    }

    #[test]
    fn outer_bank_upper_bits_beyond_inner_3_are_ignored_for_prg() {
        // MMC1 inner prg_bank register is 5 bits; FaridSlrom masks to 3 bits.
        // Write inner PRG bank = 0b01011 = 11; inner & 0x07 = 3
        // outer_bank = 0 → effective = 0 | 3 = 3
        let mut mapper = make_mapper();
        write_mmc1_reg(&mut mapper, 0xE000, 0b01011); // value 11
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Only lower 3 bits of MMC1 PRG bank used"
        );
    }

    // ── PRG mode 2 (fix first at $8000) with outer bank ─────────────────────

    #[test]
    fn prg_mode2_lower_window_fixed_at_outer_base() {
        // Set control: mode 2 = bits 3:2 = 0b10 → control = 0b0_10_00 = 0x08
        // plus mirroring bits (keep horizontal = 0b11) → control = 0b0_10_11 = 0x0B
        let mut mapper = make_mapper();
        write_mmc1_reg(&mut mapper, 0x8000, 0x0B); // mode 2, horizontal mirroring
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        // $8000: outer_bank | 0 = 8 (fixed to first bank of outer block)
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 fixed at outer block base"
        );
        // $C000: outer_bank | (prg_bank & 0x07) = 8 | 0 = 8 initially
        // (prg_bank starts at 0)
        assert_eq!(
            mapper.read_prg(0xC000),
            8,
            "$C000 switch at outer 8, inner 0"
        );
    }

    #[test]
    fn prg_mode2_upper_window_switches_within_outer_block() {
        let mut mapper = make_mapper();
        write_mmc1_reg(&mut mapper, 0x8000, 0x0B); // mode 2
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8
        write_mmc1_reg(&mut mapper, 0xE000, 5); // inner = 5

        // $8000 fixed = 8
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 fixed");
        // $C000: 8 | 5 = 13
        assert_eq!(mapper.read_prg(0xC000), 13, "$C000 switch: 8|5=13");
    }

    // ── CHR outer-bank switching (4 KiB mode) ────────────────────────────────

    #[test]
    fn chr_4kb_mode_outer_bank_shifts_chr_block_by_32() {
        // Enable 4 KiB CHR mode: control bit 4 = 1 → control = 0b1_00_11 = 0x13
        let mut mapper = make_mapper();
        write_mmc1_reg(&mut mapper, 0x8000, 0x13); // 4KB CHR mode + horizontal
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        // CHR page 0 = (8 << 2) | (chr_bank_0 & 0x1F) = 32 | 0 = 32
        assert_eq!(mapper.read_chr(0x0000), 32, "CHR slot 0: outer 8 → bank 32");
        // CHR page 1 = (8 << 2) | (chr_bank_1 & 0x1F) = 32 | 0 = 32
        assert_eq!(mapper.read_chr(0x1000), 32, "CHR slot 1: outer 8 → bank 32");
    }

    #[test]
    fn chr_4kb_mode_inner_chr_banks_selected_within_outer_block() {
        let mut mapper = make_mapper();
        write_mmc1_reg(&mut mapper, 0x8000, 0x13); // 4KB CHR mode
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        // Write CHR bank 0 = 5
        write_mmc1_reg(&mut mapper, 0xA000, 5);
        // CHR page 0 = (8 << 2) | 5 = 37
        assert_eq!(mapper.read_chr(0x0000), 37, "CHR slot 0: 32|5=37");

        // Write CHR bank 1 = 9
        write_mmc1_reg(&mut mapper, 0xC000, 9);
        // CHR page 1 = (8 << 2) | 9 = 41
        assert_ne!(
            mapper.read_chr(0x1000),
            mapper.read_chr(0x0000),
            "CHR slot 1 must differ from slot 0"
        );
        assert_eq!(mapper.read_chr(0x1000), 41, "CHR slot 1: 32|9=41");
    }

    // ── CHR outer-bank switching (8 KiB mode) ────────────────────────────────

    #[test]
    fn chr_8kb_mode_outer_bank_shifts_chr_pair() {
        // 8 KiB CHR mode is MMC1 default (control bit 4 = 0)
        // outer_bank = 8 → CHR page pair = (8<<2) | (chr_bank_0_pair*2)
        // chr_bank_0 = 0 → bank_8k = 0 → pages 0 and 1 → with outer: 32 and 33
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        assert_eq!(mapper.read_chr(0x0000), 32, "CHR $0000: outer 8 → bank 32");
        assert_eq!(mapper.read_chr(0x1000), 33, "CHR $1000: outer 8 → bank 33");
    }

    // ── Lock bit ─────────────────────────────────────────────────────────────

    #[test]
    fn lock_bit_prevents_outer_bank_update() {
        let mut mapper = make_mapper();
        // Set outer_bank = 8 with lock bit set: bits 6:4 = 001, bit 3 = 1
        // value = 0b0001_1000 = 0x18 → outer_bank = (0x18 & 0x70) >> 1 = 0x08, locked = true
        mapper.write_prg(0x6000, 0x18);
        assert_eq!(mapper.read_prg(0x8000), 8, "outer_bank set to 8 with lock");

        // Try to change outer_bank to block 2 (value = 0x20)
        mapper.write_prg(0x6000, 0x20);
        // Should stay at 8 because locked
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "outer_bank must not change when locked"
        );
    }

    #[test]
    fn lock_bit_is_permanent_once_set() {
        let mut mapper = make_mapper();
        // Lock outer bank 1 (outer_bank = 8) by setting bit 3
        mapper.write_prg(0x6000, 0x18); // outer=8, locked
        // Multiple attempts to change — none should succeed
        mapper.write_prg(0x6000, 0x00);
        mapper.write_prg(0x6000, 0x10);
        mapper.write_prg(0x6000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "outer_bank must remain 8 after lock is set"
        );
    }

    // ── WRAM-disable gate ────────────────────────────────────────────────────

    #[test]
    fn wram_disable_bit_prevents_outer_bank_write() {
        // Set MMC1 PRG bank register bit 4 = 1 to disable WRAM
        // PRG bank register is at $E000-$FFFF, bit 4 = 0x10
        // Write value 0b10000 = 0x10 to MMC1 PRG bank register
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00); // outer_bank = 0 initially
        write_mmc1_reg(&mut mapper, 0xE000, 0x10); // disable WRAM (bit 4)

        // Now attempt to write outer_bank = 8
        mapper.write_prg(0x6000, 0x10);
        // Should remain 0 (WRAM disabled → outer bank register unwritable)
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "outer_bank must not change when WRAM is disabled"
        );
    }

    #[test]
    fn outer_bank_writable_when_wram_is_enabled() {
        // MMC1B: bit 4 = 0 → WRAM enabled
        let mut mapper = make_mapper();
        // Default: prg_bank=0, bit4=0 → WRAM enabled
        mapper.write_prg(0x6000, 0x10); // should succeed: outer_bank = 8
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "outer_bank must be writable when WRAM is enabled"
        );
    }

    // ── Outer-bank persists across MMC1 mode changes ─────────────────────────

    #[test]
    fn mmc1_register_write_keeps_outer_bank_in_effect() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        // Change MMC1 inner PRG bank to 3 via serial register
        write_mmc1_reg(&mut mapper, 0xE000, 3);

        // Effective PRG page = outer_bank | inner = 8 | 3 = 11
        assert_eq!(
            mapper.read_prg(0x8000),
            11,
            "outer bank must stay in effect after MMC1 register write"
        );
    }

    // ── Mirroring delegated to MMC1 ──────────────────────────────────────────

    #[test]
    fn mirroring_follows_mmc1_control_register() {
        let mut mapper = make_mapper();
        // control = 0b00_10 = 0x02 → vertical mirroring
        write_mmc1_reg(&mut mapper, 0x8000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring must follow MMC1 control register"
        );
    }

    // ── Reset behavior ───────────────────────────────────────────────────────

    #[test]
    fn reset_clears_outer_bank_and_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8
        assert_ne!(mapper.read_prg(0x8000), 0, "verify state was changed");

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0, "after reset: $8000 = bank 0");
        assert_eq!(mapper.read_prg(0xC000), 7, "after reset: $C000 = bank 7");
    }

    #[test]
    fn reset_clears_lock_bit() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x18); // outer_bank=8, locked

        mapper.reset();

        // After reset, lock should be cleared; outer bank should be writable again
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "lock must be cleared after reset"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip_preserves_outer_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // outer_bank = 8

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "restored PRG $8000 must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "restored PRG $C000 must match"
        );
    }

    #[test]
    fn snapshot_restore_preserves_lock_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x18); // outer_bank=8, locked

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // Attempt to change outer bank — should be blocked (locked)
        restored.write_prg(0x6000, 0x00);
        assert_eq!(
            restored.read_prg(0x8000),
            8,
            "lock state must be preserved in snapshot"
        );
    }

    // ── No IRQ ───────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10);
        write_mmc1_reg(&mut mapper, 0xE000, 7);
        assert!(!mapper.irq_pending(), "Mapper 323 must never assert IRQ");
    }
}
