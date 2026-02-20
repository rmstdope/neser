//! Mapper 6 — Front Fareast Magic Card (SMC) 1M/2M/4M PRG banking
//!
//! Sub-issue #627: Core latch-based banking modes 0–7 + register scaffolding.
//! Sub-issue #628: 2M/4M PRG banking mode ($43FC-$43FF, $4504-$4507).
//!
//! Spec: <https://www.nesdev.org/wiki/INES_Mapper_006>
//!       <https://www.nesdev.org/wiki/Super_Magic_Card>
//!
//! Known Limitations:
//! - 1 KiB CHR banking mode ($4510-$451B) not yet implemented (sub-issue #629).
//! - IRQ counter ($4501-$4503) not yet implemented (sub-issue #630).
//! - Trainer initialization at $7000-$71FF not yet implemented (sub-issue #631).
use crate::cartridge::common::{BankedRom, ChrMemory};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const PRG_BANK_SIZE_8K: usize = 0x2000;
const CHR_BANK_SIZE_8K: usize = 0x2000;
const WRAM_BANK_SIZE_8K: usize = 0x2000;
const WRAM_SIZE_32K: usize = 0x8000;
const CHR_RAM_SIZE_32K: usize = 0x8000;

/// 16 KiB bank indices for the lower and upper halves of 32 KiB PRG bank #3.
/// Modes 5, 6, and 7 fix PRG at this bank pair (= 8 KiB banks 12–15).
const PRG_BANK3_LOWER_HALF: usize = 6; // 16 KiB index → 8 KiB banks 12 & 13
const PRG_BANK3_UPPER_HALF: usize = 7; // 16 KiB index → 8 KiB banks 14 & 15

/// Mapper 6 — Front Fareast Magic Card (SMC-801)
///
/// Latch-based banking covering eight modes inherited from the Magic Card 1M/2M:
///   - Mode 0: UNROM  — 3-bit PRG bank at $8000, fixed last at $C000
///   - Mode 1: UN1ROM+CHRSW — 4-bit PRG (bits 5-2) + 2-bit CHR (bits 1-0)
///   - Mode 2: UOROM  — 4-bit PRG bank at $8000, fixed last at $C000
///   - Mode 3: Reverse UOROM+CHRSW — switchable $C000, fixed last at $8000
///   - Mode 4: GNROM  — 32KB PRG bank (bits 5-4), 8KB CHR bank (bits 1-0), CHR-protected
///   - Mode 5: CNROM-256 — fixed 32KB bank 3, 2-bit CHR, CHR-protected
///   - Mode 6: CNROM-128 — fixed 32KB bank 3, 1-bit CHR, CHR-protected
///   - Mode 7: NROM-256 — fixed 32KB bank 3, fixed CHR bank 0, CHR-protected
///
/// Registers:
///   $42FC-$42FF — 1M mode register; address bits A1/A0 encode latch-enable and
///                 mirroring LSB, data bits D7-D5/D4 encode mode and mirroring MSB.
///   $43FC-$43FF — 2M/4M mode register; A0=M (1=disable), A1=N (1=2M when M=0).
///   $4500       — SMC mode register (bits 5-4: WRAM bank select)
///   $4504-$4507 — 4M PRG slot registers; data bits 5-0 select 8 KiB bank per slot.
///   $6000-$7FFF — 8 KiB window into 32 KiB banked WRAM
///   $8000-$FFFF — latch write when latch is enabled; always updates 2M shadow slots
pub struct Mapper6Mapper {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    wram: Vec<u8>,
    latch_mode: u8,       // D7-D5 of $42FC-$42FF: 0-7
    latch_value: u8,      // last value written to the latch at $8000-$FFFF
    latch_enabled: bool,  // A1 of $42FC-$42FF: PRG write-protected ↔ latch enabled
    mirroring_type: u8,   // (A0 << 1) | D4; 0=SingleScreenLower, 1=Upper, 2=Vertical, 3=Horizontal
    wram_bank: u8,        // bits 5-4 of $4500: 0-3, selects 8 KiB WRAM bank
    prg_2m_slots: [u8; 4],  // shadow 8 KiB PRG banks for 2M mode (always updated on $8000-$FFFF writes)
    prg_4m_slots: [u8; 4],  // 8 KiB PRG banks for 4M mode (updated via $4504-$4507)
    mode_2m_active: bool,   // true when $43FE was the last $43FC-$43FF write
    mode_4m_active: bool,   // true when $43FC was the last $43FC-$43FF write
}

/// Map a switched 16 KiB bank `b` to an 8 KiB slot index where slots 0–1 follow
/// the switched bank and slots 2–3 are fixed at the last 16 KiB bank.
/// Used by latch modes 0 (UNROM), 1 (UN1ROM), and 2 (UOROM).
fn lower_switched_upper_fixed(b: usize, slot: usize, last_lo: usize, last_hi: usize) -> usize {
    match slot {
        0 => b * 2,
        1 => b * 2 + 1,
        2 => last_lo,
        _ => last_hi,
    }
}

/// Derive the 8 KiB PRG slot index (0–3) from a CPU address in $8000–$FFFF.
fn prg_slot_from_addr(addr: u16) -> usize {
    ((addr - 0x8000) / 0x2000) as usize
}

impl Mapper6Mapper {
    /// Create a new Mapper 6 instance.
    ///
    /// # Arguments
    /// * `prg_rom`   — PRG-ROM data (up to 256 KiB for iNES 1.0 submapper 1)
    /// * `_chr_rom`  — ignored; mapper always uses 32 KiB CHR-RAM
    /// * `mirroring` — initial nametable mirroring from the iNES header
    /// * `submapper` — iNES 2.0 submapper (0 is remapped to 1 per spec)
    pub fn new(
        prg_rom: Vec<u8>,
        _chr_rom: Vec<u8>,
        mirroring: NametableLayout,
        submapper: u8,
    ) -> Self {
        // Per NesDev spec, the emulator simulates writing
        //   [$42FF] = (submapper << 5) | (horizontalMirroring ? 0x10 : 0x00)
        // at power-on:
        //   addr=$42FF  → A1=1 (latch enabled), A0=1 (mirroring LSB)
        //   data D7-D5  → BBB = submapper & 0x07   (latch mode)
        //   data D4     → 1 for horizontal, 0 for vertical/other
        //   mirroring_type = (A0 << 1) | D4
        let effective_submapper = if submapper == 0 { 1 } else { submapper };
        let latch_mode = effective_submapper & 0x07;
        let d4 = u8::from(matches!(mirroring, NametableLayout::Horizontal));
        // A0 = 1 (addr $42FF has bit 0 set)
        let mirroring_type = (1 << 1) | d4;

        Self {
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE_8K),
            chr_memory: ChrMemory::new_ram(CHR_RAM_SIZE_32K),
            wram: vec![0; WRAM_SIZE_32K],
            latch_mode,
            latch_value: 0,
            latch_enabled: true,
            mirroring_type,
            wram_bank: 0,
            prg_2m_slots: [0; 4],
            prg_4m_slots: [0; 4],
            mode_2m_active: false,
            mode_4m_active: false,
        }
    }

    fn last_8k_bank(&self) -> usize {
        self.prg_rom.num_banks().saturating_sub(1)
    }

    fn last_16k_bank(&self) -> usize {
        self.prg_rom.num_banks().saturating_sub(2)
    }

    /// Return the 8 KiB bank index for PRG slot `slot` (0-3) using the 1M latch.
    fn latch_bank_for_slot(&self, slot: usize) -> usize {
        let num = self.prg_rom.num_banks();
        let last_lo = num.saturating_sub(2);
        let last_hi = num.saturating_sub(1);
        match self.latch_mode {
            0 => {
                // UNROM: bits 2-0 → 16 KiB bank at $8000; last at $C000
                let b = (self.latch_value & 0x07) as usize;
                lower_switched_upper_fixed(b, slot, last_lo, last_hi)
            }
            1 => {
                // UN1ROM+CHRSW: bits 5-2 → 16 KiB bank at $8000; last at $C000
                let b = ((self.latch_value >> 2) & 0x0F) as usize;
                lower_switched_upper_fixed(b, slot, last_lo, last_hi)
            }
            2 => {
                // UOROM: bits 3-0 → 16 KiB bank at $8000; last at $C000
                let b = (self.latch_value & 0x0F) as usize;
                lower_switched_upper_fixed(b, slot, last_lo, last_hi)
            }
            3 => {
                // Reverse UOROM: bits 3-0 → $C000 bank; fixed last at $8000
                let b = (self.latch_value & 0x0F) as usize;
                match slot {
                    0 => last_lo,
                    1 => last_hi,
                    2 => b * 2,
                    _ => b * 2 + 1,
                }
            }
            4 => {
                // GNROM: bits 5-4 → 32 KiB bank (PP)
                let pp = ((self.latch_value >> 4) & 0x03) as usize;
                pp * 4 + slot
            }
            _ => {
                // Modes 5-7: fixed 32 KiB bank #3 (16 KiB banks 6+7 = 8 KiB banks 12-15)
                PRG_BANK3_LOWER_HALF * 2 + slot
            }
        }
    }

    /// Return the 8 KiB bank index for PRG slot `slot` (0-3),
    /// honouring 4M > 2M > latch priority.
    fn bank_for_slot(&self, slot: usize) -> usize {
        if self.mode_4m_active {
            self.prg_4m_slots[slot] as usize
        } else if self.mode_2m_active {
            self.prg_2m_slots[slot] as usize
        } else {
            self.latch_bank_for_slot(slot)
        }
    }

    fn chr_bank_8k(&self) -> usize {
        match self.latch_mode {
            0 | 2 | 7 => 0,                                 // fixed CHR bank 0
            1 => (self.latch_value & 0x03) as usize,        // CC = bits 1-0
            3 => ((self.latch_value >> 4) & 0x03) as usize, // CC = bits 5-4
            4 | 5 => (self.latch_value & 0x03) as usize,    // CC = bits 1-0
            6 => (self.latch_value & 0x01) as usize,        // C = bit 0
            _ => 0,
        }
    }

    fn chr_write_protected(&self) -> bool {
        self.latch_mode >= 4
    }

    /// Decode a write to the 1M mode register ($42FC–$42FF) and apply it.
    fn apply_mode_register(&mut self, addr: u16, value: u8) {
        let latch_enable = (addr >> 1) & 1;
        let mirroring_lsb = (addr & 1) as u8;
        let mode = (value >> 5) & 0x07;
        let mirroring_msb = (value >> 4) & 0x01;
        self.latch_enabled = latch_enable != 0;
        self.latch_mode = mode;
        self.mirroring_type = (mirroring_lsb << 1) | mirroring_msb;
    }

    /// Decode a write to the 2M/4M mode register ($43FC–$43FF) and apply it.
    ///
    /// Address encoding: A0=M (0=enable, 1=disable), A1=N (0=4M, 1=2M when M=0).
    /// Data bits 1-0 (CC): CHR bank update (mirrors latch CC — handled by latch writes).
    fn apply_2m4m_register(&mut self, addr: u16) {
        let m = addr & 1;  // A0
        let n = (addr >> 1) & 1; // A1
        if m != 0 {
            // M=1 → disable both modes
            self.mode_2m_active = false;
            self.mode_4m_active = false;
        } else if n != 0 {
            // M=0, N=1 → 2M mode
            self.mode_2m_active = true;
            self.mode_4m_active = false;
        } else {
            // M=0, N=0 → 4M mode
            self.mode_2m_active = false;
            self.mode_4m_active = true;
        }
    }

    fn wram_index(&self, addr: u16) -> usize {
        self.wram_bank as usize * WRAM_BANK_SIZE_8K + (addr - 0x6000) as usize
    }
}

impl Mapper for Mapper6Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.wram.get(self.wram_index(addr)).copied().unwrap_or(0),
            0x8000..=0x9FFF => self.prg_rom.read_with_base(self.bank_for_slot(0), 0x8000, addr),
            0xA000..=0xBFFF => self.prg_rom.read_with_base(self.bank_for_slot(1), 0xA000, addr),
            0xC000..=0xDFFF => self.prg_rom.read_with_base(self.bank_for_slot(2), 0xC000, addr),
            0xE000..=0xFFFF => self.prg_rom.read_with_base(self.bank_for_slot(3), 0xE000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x42FC..=0x42FF => self.apply_mode_register(addr, value),
            0x43FC..=0x43FF => self.apply_2m4m_register(addr),
            0x4500 => self.wram_bank = (value >> 4) & 0x03,
            0x4504..=0x4507 => self.prg_4m_slots[(addr - 0x4504) as usize] = value & 0x3F,
            0x6000..=0x7FFF => {
                let index = self.wram_index(addr);
                if index < self.wram.len() {
                    self.wram[index] = value;
                }
            }
            0x8000..=0xFFFF => {
                // Always update 2M shadow slot (spec: "2M registers always accept writes")
                let slot = prg_slot_from_addr(addr);
                self.prg_2m_slots[slot] = (value >> 2) & 0x3F;
                if self.latch_enabled {
                    self.latch_value = value;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank = self.chr_bank_8k();
        let index = bank * CHR_BANK_SIZE_8K + (addr & 0x1FFF) as usize;
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_write_protected() {
            let bank = self.chr_bank_8k();
            let index = bank * CHR_BANK_SIZE_8K + (addr & 0x1FFF) as usize;
            self.chr_memory.write_at_index(index, value);
        }
    }

    fn get_mirroring(&self) -> NametableLayout {
        match self.mirroring_type {
            0 => NametableLayout::SingleScreenLower,
            1 => NametableLayout::SingleScreenUpper,
            2 => NametableLayout::Vertical,
            3 => NametableLayout::Horizontal,
            _ => NametableLayout::Horizontal, // mirroring_type is always 0–3; unreachable in practice
        }
    }

    fn mapper_number(&self) -> u8 {
        6
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 32,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // byte  0: latch_mode (bits 2-0)
        // byte  1: latch_value
        // byte  2: latch_enabled (bit 0) | mirroring_type (bits 2-1) | wram_bank (bits 5-4)
        // bytes 3-6:  prg_2m_slots[0-3]
        // bytes 7-10: prg_4m_slots[0-3]
        // byte 11: mode flags: bit 0 = mode_2m_active, bit 1 = mode_4m_active
        let mut v = vec![
            self.latch_mode & 0x07,
            self.latch_value,
            (self.latch_enabled as u8) | (self.mirroring_type << 1) | (self.wram_bank << 4),
        ];
        v.extend_from_slice(&self.prg_2m_slots);
        v.extend_from_slice(&self.prg_4m_slots);
        v.push((self.mode_2m_active as u8) | ((self.mode_4m_active as u8) << 1));
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.latch_mode = data[0] & 0x07;
            self.latch_value = data[1];
            self.latch_enabled = (data[2] & 0x01) != 0;
            self.mirroring_type = (data[2] >> 1) & 0x03;
            self.wram_bank = (data[2] >> 4) & 0x03;
        }
        if data.len() >= 7 {
            self.prg_2m_slots.copy_from_slice(&data[3..7]);
        }
        if data.len() >= 11 {
            self.prg_4m_slots.copy_from_slice(&data[7..11]);
        }
        if data.len() >= 12 {
            self.mode_2m_active = (data[11] & 0x01) != 0;
            self.mode_4m_active = (data[11] & 0x02) != 0;
        }
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn wram_size(&self) -> usize {
        WRAM_SIZE_32K
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.wram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.wram.len());
        self.wram[..to_copy].copy_from_slice(&data[..to_copy]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANK_SIZE_16K: usize = 0x4000;

    fn create_m6(prg: Vec<u8>, submapper: u8, mirroring: NametableLayout) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new(6, prg, vec![], mirroring).with_submapper(submapper))
            .expect("Failed to create Mapper 6")
    }

    // ── Initial state ──────────────────────────────────────────────────────────

    #[test]
    fn test_initial_state_submapper1_vertical_mirroring() {
        // Air Fortress-like: submapper 1, vertical mirroring from iNES header
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Vertical);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_initial_state_submapper1_horizontal_mirroring() {
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Horizontal);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── $42FC-$42FF mirroring register ────────────────────────────────────────
    //
    // Address bits: A1=latch-enable, A0=mirroring-LSB
    // Data bits:    D7-D5=mode(BBB), D4=mirroring-MSB
    // mirroring_type = (A0 << 1) | D4
    //   0 → SingleScreenLower, 1 → SingleScreenUpper, 2 → Vertical, 3 → Horizontal

    #[test]
    fn test_write_42fe_d4_0_sets_single_screen_lower() {
        // A0=0, D4=0 → mirroring_type = 0
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FE, 0x20); // D7-D5=001 (mode 1), D4=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_write_42fe_d4_1_sets_single_screen_upper() {
        // A0=0, D4=1 → mirroring_type = 1
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FE, 0x30); // D7-D5=001 (mode 1), D4=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_write_42ff_d4_0_sets_vertical() {
        // A0=1, D4=0 → mirroring_type = 2
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Horizontal);
        mapper.write_prg(0x42FF, 0x20); // D7-D5=001 (mode 1), D4=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_write_42ff_d4_1_sets_horizontal() {
        // A0=1, D4=1 → mirroring_type = 3
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x30); // D7-D5=001 (mode 1), D4=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_write_42fc_disables_latch_so_bank_writes_are_ignored() {
        // A1=0 for $42FC → latch disabled; writes to $8000-$FFFF are ignored
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Select PRG bank 5 while latch is enabled (initial state)
        mapper.write_prg(0x8000, 5 << 2); // mode 1: bits 5-2 = 0101 → bank 5
        assert_eq!(mapper.read_prg(0x8000), 5);
        // Disable latch: write to $42FC (A1=0)
        mapper.write_prg(0x42FC, 0x20); // mode 1, A1=0 → latch disabled
        // Attempt to change bank — must be ignored
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5); // still bank 5
    }

    #[test]
    fn test_write_42ff_enables_latch_so_bank_writes_take_effect() {
        // A1=1 for $42FF → latch enabled
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Initial: latch already enabled; write bank 3
        mapper.write_prg(0x8000, 3 << 2); // bank 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // ── Mode 0 — UNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode0_prg_8000_switches_with_bits_0_2() {
        // D~[..... PPP] → 16 KiB bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 8); // 8 × 16 KiB = 128 KiB
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x00); // BBB=000 → mode 0
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_mode0_c000_fixed_at_last_bank() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 8);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x00); // mode 0
        mapper.write_prg(0x8000, 0x00); // bank 0 at $8000
        assert_eq!(mapper.read_prg(0xC000), 7); // fixed last bank
    }

    // ── Mode 1 — UN1ROM + CHRSW (Air Fortress) ────────────────────────────────

    #[test]
    fn test_mode1_prg_8000_switches_with_bits_2_5() {
        // D~[..BBBB CC] → PRG bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 16); // 16 × 16 KiB = 256 KiB
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x04); // bank 1 (bits 5-2 = 0001)
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.write_prg(0x8000, 0x3C); // bank 15 (bits 5-2 = 1111)
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_mode1_c000_fixed_at_last_bank() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x00); // bank 0 at $8000
        assert_eq!(mapper.read_prg(0xC000), 15); // last bank = 15
    }

    #[test]
    fn test_mode1_chr_bank_selected_by_bits_0_1() {
        // CC (bits 1-0) selects 8 KiB CHR bank 0-3
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Write distinct markers to CHR banks 0-3
        for bank in 0..4u8 {
            mapper.write_prg(0x8000, bank); // latch: CC=bank, BBBB=0
            mapper.write_chr(0x0000, bank + 10);
        }
        // Verify each bank holds its marker
        for bank in 0..4u8 {
            mapper.write_prg(0x8000, bank);
            assert_eq!(mapper.read_chr(0x0000), bank + 10);
        }
    }

    #[test]
    fn test_mode1_chr_is_writable() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB);
    }

    // ── Mode 2 — UOROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode2_prg_8000_switches_with_4bit_bank() {
        // D~[....PPPP] → 16 KiB bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x40); // BBB=010 → mode 2
        mapper.write_prg(0x8000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 10);
    }

    #[test]
    fn test_mode2_c000_fixed_at_last_bank() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x40); // mode 2
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    // ── Mode 3 — Reverse UOROM + CHRSW ───────────────────────────────────────

    #[test]
    fn test_mode3_c000_switches_8000_fixed_at_last_bank() {
        // D~[..CC PPPP] → $C000-$FFFF = latch bits 0-3; $8000-$BFFF = fixed last
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x60); // BBB=011 → mode 3
        mapper.write_prg(0x8000, 0x05); // PPPP=5 → $C000 bank 5
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0x8000), 15); // fixed last bank
    }

    #[test]
    fn test_mode3_chr_bank_selected_by_bits_4_5() {
        // CC = bits 5-4
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x60); // mode 3
        mapper.write_chr(0x0000, 0xAA); // write to CHR bank 0
        mapper.write_prg(0x8000, 0x10); // bits 5-4 = 01 → CHR bank 1
        mapper.write_chr(0x0000, 0xBB); // write to CHR bank 1
        mapper.write_prg(0x8000, 0x00); // back to CHR bank 0
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_chr(0x0000), 0xBB);
    }

    // ── Mode 4 — GNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode4_32kb_prg_bank_selected_by_bits_4_5() {
        // D~[..PP..CC] → $8000-$FFFF = 32 KiB bank PP
        let prg = banked_data(PRG_BANK_SIZE_16K, 16); // 4 × 32 KiB banks
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x80); // BBB=100 → mode 4
        // PP=0 → 32 KiB bank 0 (16 KiB banks 0+1)
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0); // lower 16 KiB
        assert_eq!(mapper.read_prg(0xC000), 1); // upper 16 KiB
        // PP=1 → 32 KiB bank 1 (16 KiB banks 2+3)
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    #[test]
    fn test_mode4_chr_is_write_protected() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x80); // mode 4
        mapper.write_chr(0x0000, 0xAB); // must be ignored
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 5 — CNROM-256 ────────────────────────────────────────────────────

    #[test]
    fn test_mode5_prg_fixed_at_32kb_bank3() {
        // PRG fixed at 32 KiB bank #3 = 16 KiB banks 6 and 7
        let prg = banked_data(PRG_BANK_SIZE_16K, 8); // 128 KiB
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xA0); // BBB=101 → mode 5
        mapper.write_prg(0x8000, 0x00); // latch write must not change PRG
        assert_eq!(mapper.read_prg(0x8000), 6); // 32 KiB bank 3, lower half
        assert_eq!(mapper.read_prg(0xC000), 7); // upper half
    }

    #[test]
    fn test_mode5_chr_is_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xA0); // mode 5
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 6 — CNROM-128 ────────────────────────────────────────────────────

    #[test]
    fn test_mode6_prg_fixed_at_32kb_bank3() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 8);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xC0); // BBB=110 → mode 6
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_mode6_chr_is_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xC0); // mode 6
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 7 — NROM-256 ─────────────────────────────────────────────────────

    #[test]
    fn test_mode7_prg_fixed_at_32kb_bank3() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 8);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xE0); // BBB=111 → mode 7
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_mode7_chr_fixed_at_bank0_and_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xE0); // mode 7
        mapper.write_chr(0x0000, 0xAB); // must be ignored
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── WRAM at $6000-$7FFF ───────────────────────────────────────────────────

    #[test]
    fn test_wram_read_write_in_bank0() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCD);
    }

    #[test]
    fn test_wram_4500_bits_5_4_select_8kb_bank() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Write 0x11 to WRAM bank 0
        mapper.write_prg(0x6000, 0x11);
        // Switch to bank 1 (bits 5-4 of $4500 = 01)
        mapper.write_prg(0x4500, 0x10);
        mapper.write_prg(0x6000, 0x22);
        // Switch back to bank 0 and verify
        mapper.write_prg(0x4500, 0x00);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        // Switch to bank 1 and verify
        mapper.write_prg(0x4500, 0x10);
        assert_eq!(mapper.read_prg(0x6000), 0x22);
    }

    // ── Register snapshot / restore ───────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_preserves_mode_bank_and_mirroring() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        // Set mode 1, bank 5, vertical mirroring, WRAM bank 2
        mapper.write_prg(0x42FF, 0x20); // mode 1, latch enabled, vertical
        mapper.write_prg(0x4500, 0x20); // WRAM bank 2
        mapper.write_prg(0x8000, 5 << 2); // mode 1: bits 5-2 = 5 → bank 5
        let snap = mapper.registers_snapshot();
        // Restore into a fresh mapper
        let mut restored = create_m6(prg, 1, NametableLayout::Horizontal);
        restored.restore_registers(&snap);
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(restored.read_prg(0x8000), 5);
    }

    #[test]
    fn test_chr_ram_snapshot_preserves_all_banks() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        // Write distinct markers to CHR banks 0 and 1
        mapper.write_prg(0x8000, 0); // CHR bank 0
        mapper.write_chr(0x0000, 0x42);
        mapper.write_prg(0x8000, 1); // CHR bank 1
        mapper.write_chr(0x0000, 0x99);
        let snap = mapper.chr_ram_snapshot();
        // Restore and verify
        let mut restored = create_m6(prg, 1, NametableLayout::Vertical);
        restored.restore_chr_ram(&snap);
        restored.write_prg(0x8000, 0);
        assert_eq!(restored.read_chr(0x0000), 0x42);
        restored.write_prg(0x8000, 1);
        assert_eq!(restored.read_chr(0x0000), 0x99);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn test_capabilities_reports_chr_banking_and_dynamic_mirroring() {
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Vertical);
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "mapper 6 has CHR banking");
        assert!(caps.has_dynamic_mirroring, "mapper 6 has dynamic mirroring");
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(mapper.wram_size(), 32 * 1024);
    }

    // ── 2M PRG banking ($43FE enables; writes to $8000-$FFFF set 8 KiB slots) ─

    #[test]
    fn test_2m_mode_slot0_reads_8kb_bank_via_8000_write() {
        // $43FE enables 2M mode; data [PPPPPPCC] → slot 0 = PPPPPP = value >> 2.
        // banked_data(0x2000, 32): each 8 KiB bank k filled with k.
        let prg = banked_data(0x2000, 32); // 32 × 8 KiB = 256 KiB
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00); // enable 2M mode (A1=N=1, A0=M=0)
        mapper.write_prg(0x8000, 9 << 2); // slot 0 = 8 KiB bank 9
        assert_eq!(mapper.read_prg(0x8000), 9);
    }

    #[test]
    fn test_2m_mode_slot1_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xA000, 7 << 2); // slot 1 = bank 7
        assert_eq!(mapper.read_prg(0xA000), 7);
    }

    #[test]
    fn test_2m_mode_slot2_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xC000, 15 << 2); // slot 2 = bank 15
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn test_2m_mode_slot3_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xE000, 20 << 2); // slot 3 = bank 20
        assert_eq!(mapper.read_prg(0xE000), 20);
    }

    #[test]
    fn test_2m_mode_all_four_slots_independent() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0x8000, 3 << 2);  // slot 0 = bank 3
        mapper.write_prg(0xA000, 11 << 2); // slot 1 = bank 11
        mapper.write_prg(0xC000, 17 << 2); // slot 2 = bank 17
        mapper.write_prg(0xE000, 28 << 2); // slot 3 = bank 28
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xA000), 11);
        assert_eq!(mapper.read_prg(0xC000), 17);
        assert_eq!(mapper.read_prg(0xE000), 28);
    }

    #[test]
    fn test_2m_slot_shadow_registers_updated_even_when_mode_disabled() {
        // Per spec, 2M registers ALWAYS accept writes (even when 2M mode inactive).
        // A write to $8000-$FFFF while 2M is disabled still updates the shadow slot.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // 2M disabled at start; write slot 0 shadow = bank 7
        mapper.write_prg(0x8000, 7 << 2);
        // Now enable 2M — slot 0 should already be bank 7 from the shadow write
        mapper.write_prg(0x43FE, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    #[test]
    fn test_43fd_disables_2m_mode_and_latch_takes_over() {
        // $43FD: A0=M=1 → disable 2M/4M; fallback to latch-based banking.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0x8000, 7 << 2); // 2M slot 0 = bank 7; latch = 0x1C
        // In 2M mode slot 0 = bank 7 → data 7
        assert_eq!(mapper.read_prg(0x8000), 7);
        // Disable 2M/4M
        mapper.write_prg(0x43FD, 0x00);
        // Latch mode 1: bank = (0x1C >> 2) & 0xF = 7 → 8 KiB bank 7*2=14 → data 14
        assert_eq!(mapper.read_prg(0x8000), 14);
    }

    // ── 4M PRG banking ($43FC enables; $4504-$4507 set 8 KiB slots) ───────────

    #[test]
    fn test_4m_mode_slot0_via_4504_write() {
        // $43FC enables 4M mode (A1=N=0, A0=M=0); data [..PPPPPP] = 6-bit bank.
        // banked_data(0x2000, 64): 64 × 8 KiB = 512 KiB, bank k filled with k.
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00); // enable 4M mode
        mapper.write_prg(0x4504, 15);   // slot 0 = bank 15
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_4m_mode_slot1_via_4505_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4505, 22); // slot 1 = bank 22
        assert_eq!(mapper.read_prg(0xA000), 22);
    }

    #[test]
    fn test_4m_mode_slot2_via_4506_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4506, 40); // slot 2 = bank 40
        assert_eq!(mapper.read_prg(0xC000), 40);
    }

    #[test]
    fn test_4m_mode_slot3_via_4507_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4507, 55); // slot 3 = bank 55
        assert_eq!(mapper.read_prg(0xE000), 55);
    }

    #[test]
    fn test_4m_mode_all_four_slots_independent() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4504, 5);  // slot 0 = bank 5
        mapper.write_prg(0x4505, 20); // slot 1 = bank 20
        mapper.write_prg(0x4506, 45); // slot 2 = bank 45
        mapper.write_prg(0x4507, 63); // slot 3 = bank 63
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xA000), 20);
        assert_eq!(mapper.read_prg(0xC000), 45);
        assert_eq!(mapper.read_prg(0xE000), 63);
    }

    #[test]
    fn test_4m_mode_chr_not_changed_by_4504_writes() {
        // 4M $4504 writes carry no CC bits; CHR must remain from latch.
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Latch mode 1: set CHR bank 2 and PRG bank 0
        mapper.write_prg(0x8000, 0x02); // latch: BBBB=0, CC=2 → CHR bank 2
        mapper.write_chr(0x0000, 0x42); // mark CHR bank 2
        // Enable 4M and assign slot 0 via $4504
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4504, 40); // PRG slot 0 = bank 40
        // PRG should now reflect 4M slot 0
        assert_eq!(mapper.read_prg(0x8000), 40);
        // CHR bank 2 must still be active (latch CC bits unchanged)
        assert_eq!(mapper.read_chr(0x0000), 0x42);
    }

    // ── Snapshot roundtrip for 2M/4M state ────────────────────────────────────

    #[test]
    fn test_2m_4m_snapshot_includes_mode_and_slots() {
        // After enabling 2M mode and setting all slot banks, a snapshot+restore
        // must preserve mode_2m_active and prg_2m_slots exactly.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);   // enable 2M
        mapper.write_prg(0x8000, 3 << 2); // slot 0 = bank 3
        mapper.write_prg(0xA000, 11 << 2);// slot 1 = bank 11
        mapper.write_prg(0xC000, 17 << 2);// slot 2 = bank 17
        mapper.write_prg(0xE000, 28 << 2);// slot 3 = bank 28
        let snap = mapper.registers_snapshot();
        let mut restored = create_m6(prg, 1, NametableLayout::Horizontal);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xA000), 11);
        assert_eq!(restored.read_prg(0xC000), 17);
        assert_eq!(restored.read_prg(0xE000), 28);
    }
}
