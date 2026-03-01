//! Mapper 052 - BMC Realtec 8213 MMC3 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_052>
//!
//! Known Limitations:
//! - Submapper 13/14 (CHR RAM variants) are not implemented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

/// Mapper 052 - Realtec 8213 MMC3-based multicart
///
/// Hardware: MMC3 ASIC plus an outer bank register at $6000-$7FFF.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_052>
/// - PRG-ROM: Up to 512 KiB (outer block extends MMC3 PRG banking)
/// - CHR: Up to 512 KiB (outer block extends MMC3 CHR banking)
/// - Mirroring: MMC3-controlled
///
/// Outer bank register ($6000-$7FFF, write):
/// D~[L T C c   S B P p]
///   bit 7 = L: Lock register until next reset (once set, $6000 writes ignored)
///   bit 6 = T: CHR A17 mode: 0=from MMC3 bank[7], 1=from c
///   bit 5 = C: CHR A18
///   bit 4 = c: CHR A17 (used when T=1)
///   bit 3 = S: PRG A17 mode: 0=from MMC3 bank[4], 1=from p
///   bit 2 = B: PRG/CHR A19 (shared high bit)
///   bit 1 = P: PRG A18
///   bit 0 = p: PRG A17 (used when S=1)
///
/// Final PRG 8KB bank:
///   A17 = if S=1 { p } else { mmc3_bank[4] }
///   A18 = P
///   A19 = B
///   bank = (B<<6) | (P<<5) | (A17<<4) | (mmc3_bank & 0x0F)
///
/// Final CHR 1KB bank:
///   A17 = if T=1 { c } else { mmc3_1k_bank[7] }
///   A18 = C
///   A19 = B
///   bank = (B<<9) | (C<<8) | (A17<<7) | (mmc3_1k_bank & 0x7F)
pub struct Mapper52 {
    pub(crate) mmc3: MMC3Mapper,
    outer: u8,
    locked: bool,
}

impl Mapper52 {
    const MAPPER_NUMBER: u8 = 52;
    const WRAM_START: u16 = 0x6000;
    const WRAM_END: u16 = 0x7FFF;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            outer: 0,
            locked: false,
        }
    }

    /// Alias for factory compatibility.
    #[allow(dead_code)]
    pub fn new_with_submapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
        _submapper: u8,
    ) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            outer: 0,
            locked: false,
        }
    }

    fn apply_prg_block(&self, mmc3_raw_bank: usize) -> usize {
        let b = ((self.outer >> 2) & 0x01) as usize; // B = bit 2
        let p = ((self.outer >> 1) & 0x01) as usize; // P = bit 1
        let p_bit = (self.outer & 0x01) as usize; // p = bit 0
        let s = ((self.outer >> 3) & 0x01) as usize; // S = bit 3

        let a17 = if s != 0 {
            p_bit
        } else {
            (mmc3_raw_bank >> 4) & 1
        };
        let low = mmc3_raw_bank & 0x0F;
        (b << 6) | (p << 5) | (a17 << 4) | low
    }

    fn apply_chr_block(&self, mmc3_raw_bank: usize) -> usize {
        let b = ((self.outer >> 2) & 0x01) as usize; // B = bit 2
        let c_bit = ((self.outer >> 5) & 0x01) as usize; // C = bit 5
        let c_lo = ((self.outer >> 4) & 0x01) as usize; // c = bit 4
        let t = ((self.outer >> 6) & 0x01) as usize; // T = bit 6

        let a17 = if t != 0 {
            c_lo
        } else {
            (mmc3_raw_bank >> 7) & 1
        };
        let low = mmc3_raw_bank & 0x7F;
        let final_bank = (b << 9) | (c_bit << 8) | (a17 << 7) | low;
        trace_mapper!(3; "[52] CHR bank: outer=0x{:02X} T={} B={} C={} c={} raw={} -> final={}",
            self.outer, t, b, c_bit, c_lo, mmc3_raw_bank, final_bank);
        final_bank
    }

    fn is_wram_window(addr: u16) -> bool {
        (Self::WRAM_START..=Self::WRAM_END).contains(&addr)
    }
}

impl Mapper for Mapper52 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_wram_window(addr) {
            return self.mmc3.read_prg_open_bus(addr, open_bus);
        }

        if addr < Self::WRAM_START {
            return open_bus;
        }

        self.read_prg(addr)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if Self::is_wram_window(addr) {
            return self.mmc3.read_prg(addr); // Forward to MMC3 PRG-RAM
        }
        let raw_bank = self.mmc3.mapped_prg_bank(addr);
        let final_bank = self.apply_prg_block(raw_bank);
        trace_mapper!(3; "[52] PRG read: addr=${:04X} outer=0x{:02X} raw={} -> final={}",
            addr, self.outer, raw_bank, final_bank);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(final_bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            // The MMC3's WRAM interface must be enabled and writeable ($A001 bit7=1,
            // bit6=0) before any $6000-$7FFF write takes effect. This matches
            // Mesen's CanWriteToWorkRam() guard and the NesDev spec.
            if !self.mmc3.is_prg_ram_writable() {
                trace_mapper!(2; "[52] $6000 write BLOCKED (WRAM not writable): addr=${:04X} value=0x{:02X}", addr, value);
                return;
            }
            if self.locked {
                // Outer register is locked; write goes to MMC3 PRG-RAM (WRAM)
                trace_mapper!(2; "[52] WRAM write (outer locked): addr=${:04X} value=0x{:02X}", addr, value);
                self.mmc3.write_prg(addr, value);
            } else {
                // First (unlocked) write sets the outer register and lock bit.
                // The outer register is a write-only latch; it does NOT write to WRAM.
                self.locked = (value & 0x80) != 0;
                self.outer = value;
                let snap = self.mmc3.registers_snapshot();
                // snap[0]=bank_select, snap[1..=8]=regs[0-7]
                let (_bs, _r) = (snap[0], &snap[1..=8]);
                trace_mapper!(1; "[52] OUTER REG <- 0x{:02X} locked={} (B={} P={} p={} S={} C={} c={} T={}) | mmc3: bs=0x{:02X} chr=[{},{},{},{},{},{}] prg=[{},{}]",
                    value, self.locked,
                    (value >> 2) & 1, (value >> 1) & 1, value & 1,
                    (value >> 3) & 1, (value >> 5) & 1, (value >> 4) & 1, (value >> 6) & 1,
                    _bs, _r[0], _r[1], _r[2], _r[3], _r[4], _r[5], _r[6], _r[7]);
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.apply_chr_block(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.apply_chr_block(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(final_bank, offset, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mmc3.get_mirroring()
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0x2000 // 8KB PRG-RAM; outer register overlaps but is write-only
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        // Delegate directly to MMC3 to avoid routing through mapper52's write_prg,
        // which would corrupt the outer register during state restoration.
        Mapper::load_wram_snapshot(&mut self.mmc3, data);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.mmc3.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.mmc3.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.mmc3.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.mmc3.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer);
        snap.push(self.locked as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let (outer_and_lock, mmc3_data) = data.split_at(data.len() - 2);
            self.outer = mmc3_data[0];
            self.locked = mmc3_data[1] != 0;
            self.mmc3.restore_registers(outer_and_lock);
        }
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer = 0;
        self.locked = false;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 2 outer blocks × 16 PRG 8KB banks = 32 banks × 8 KiB = 256 KiB PRG
    // 4 outer blocks × 128 CHR 1KB banks = 512 CHR 1KB banks = 512 KiB CHR
    const PRG_BANKS: usize = 32;
    const CHR_1K_BANKS: usize = 512;

    fn make_mapper() -> Mapper52 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        Mapper52::new(MapperContext::new_for_test(
            52,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_52_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            52,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 52 must be registered");
    }

    #[test]
    fn default_outer_zero_passes_mmc3_banks_unchanged() {
        let mapper = make_mapper();
        // Outer=0, S=0, B=0, P=0, p=0:
        // final PRG bank = (B<<6)|(P<<5)|(a17<<4)|(mmc3_bank & 0x0F)
        // For the fixed last bank ($E000): mmc3 bank = PRG_BANKS-1 = 31 = 0x1F
        // a17 = (31 >> 4) & 1 = 1; low = 31 & 0x0F = 15
        // final = 0 | 0 | (1<<4) | 15 = 16 + 15 = 31 = PRG_BANKS-1 ✓
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Default outer=0: last bank must be passthrough"
        );
    }

    #[test]
    fn outer_p_bit_selects_prg_a18() {
        let mut mapper = make_mapper();
        // MMC3 starts with PRG-RAM enabled and writable by default.
        // Set outer: P=1 (bit1), S=0, B=0, p=0 → A18=1
        // outer = 0b0000_0010 = 0x02
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(mapper.outer, 0x02, "Outer register must be set");
    }

    #[test]
    fn lock_bit_prevents_outer_register_update() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80); // enable PRG-RAM writable
        mapper.write_prg(0x6000, 0x80); // set L=1 (lock)
        assert!(mapper.locked, "L=1 must set locked");
        mapper.write_prg(0x6000, 0x02); // try to change outer
        assert_eq!(mapper.outer, 0x80, "Locked register must not change");
    }

    #[test]
    fn reset_clears_lock() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x80); // lock
        assert!(mapper.locked);
        mapper.reset();
        assert!(!mapper.locked, "Reset must clear lock");
    }

    #[test]
    fn outer_register_works_when_prg_ram_enabled() {
        let mut mapper = make_mapper();
        // MMC3 starts with PRG-RAM enabled and writable by default.
        // Write outer register to verify it takes effect.
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(
            mapper.outer, 0x02,
            "Outer register must update when PRG-RAM is enabled"
        );
    }

    #[test]
    fn outer_register_blocked_when_wram_disabled() {
        // Per NesDev spec and Mesen: MMC3 WRAM must be enabled ($A001 bit7=1)
        // before $6000-$7FFF writes take effect. Writes while WRAM is disabled
        // ($A001=$00) must NOT update the outer register.
        let mut mapper = make_mapper();
        // Default: prg_ram_enabled=true. Disable WRAM.
        mapper.write_prg(0xA001, 0x00); // disable WRAM
        mapper.write_prg(0x6000, 0x02); // should be blocked
        assert_eq!(
            mapper.outer, 0x00,
            "Outer register must NOT update when WRAM is disabled"
        );
        // Re-enable WRAM; now write should succeed.
        mapper.write_prg(0xA001, 0x80); // enable WRAM
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(
            mapper.outer, 0x02,
            "Outer register must update when WRAM is enabled"
        );
    }

    #[test]
    fn outer_register_blocked_when_wram_write_protected() {
        // Per NesDev spec and Mesen: WRAM must also not be write-protected.
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0xC0); // enable + write-protect PRG-RAM
        mapper.write_prg(0x6000, 0x02); // should be blocked (write-protected)
        assert_eq!(
            mapper.outer, 0x00,
            "Outer register must NOT update when WRAM is write-protected"
        );
    }

    #[test]
    fn wram_reads_return_data_written_when_locked() {
        let mut mapper = make_mapper();
        // Lock the outer register (first write sets outer=0x80 and locks)
        mapper.write_prg(0x6000, 0x80);
        assert!(mapper.locked);
        // With outer register locked, writes go to WRAM
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x6001, 0xCD);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "WRAM read must return written value"
        );
        assert_eq!(
            mapper.read_prg(0x6001),
            0xCD,
            "WRAM read at +1 must return written value"
        );
    }

    #[test]
    fn wram_is_not_written_when_setting_outer_register() {
        let mut mapper = make_mapper();
        // The outer register write is a latch and does NOT write to WRAM
        mapper.write_prg(0x6000, 0x80); // sets outer=0x80, locks; should NOT write to WRAM
        // WRAM should still be 0 (uninitialized)
        assert_eq!(
            mapper.read_prg(0x6000),
            0x00,
            "Outer register write must not corrupt WRAM"
        );
    }

    #[test]
    fn wram_disabled_reads_return_open_bus_value() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x00);

        let open_bus = 0x5A;
        assert_eq!(mapper.read_prg_open_bus(0x6000, open_bus), open_bus);
    }
}
