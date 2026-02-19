//! Mapper 6 — Front Fareast Magic Card (SMC) 1M latch-based banking
//!
//! Sub-issue #627: Core latch-based banking modes 0–7 + register scaffolding.
//!
//! Spec: <https://www.nesdev.org/wiki/INES_Mapper_006>
//!       <https://www.nesdev.org/wiki/Super_Magic_Card>
//!
//! Known Limitations:
//! - 2M/4M PRG banking mode ($43FC-$43FF) not yet implemented (sub-issue #628).
//! - 1 KiB CHR banking mode ($4510-$451B) not yet implemented (sub-issue #629).
//! - IRQ counter ($4501-$4503) not yet implemented (sub-issue #630).
//! - Trainer initialization at $7000-$71FF not yet implemented (sub-issue #631).
use crate::cartridge::common::{BankedRom, ChrMemory};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const PRG_BANK_SIZE_16K: usize = 0x4000;
const CHR_BANK_SIZE_8K: usize = 0x2000;
const WRAM_BANK_SIZE_8K: usize = 0x2000;
const WRAM_SIZE_32K: usize = 0x8000;
const CHR_RAM_SIZE_32K: usize = 0x8000;

/// 16 KiB bank indices for the lower and upper halves of 32 KiB PRG bank #3.
/// Modes 5, 6, and 7 fix PRG at this bank pair.
const PRG_BANK3_LOWER_HALF: usize = 6;
const PRG_BANK3_UPPER_HALF: usize = 7;

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
///   $4500       — SMC mode register (bits 5-4: WRAM bank select)
///   $6000-$7FFF — 8 KiB window into 32 KiB banked WRAM
///   $8000-$FFFF — latch write when latch is enabled (PRG write-protected)
pub struct Mapper6Mapper {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    wram: Vec<u8>,
    latch_mode: u8,      // D7-D5 of $42FC-$42FF: 0-7
    latch_value: u8,     // last value written to the latch at $8000-$FFFF
    latch_enabled: bool, // A1 of $42FC-$42FF: PRG write-protected ↔ latch enabled
    mirroring_type: u8,  // (A0 << 1) | D4; 0=SingleScreenLower, 1=Upper, 2=Vertical, 3=Horizontal
    wram_bank: u8,       // bits 5-4 of $4500: 0-3, selects 8 KiB WRAM bank
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
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE_16K),
            chr_memory: ChrMemory::new_ram(CHR_RAM_SIZE_32K),
            wram: vec![0; WRAM_SIZE_32K],
            latch_mode,
            latch_value: 0,
            latch_enabled: true,
            mirroring_type,
            wram_bank: 0,
        }
    }

    fn last_16k_bank(&self) -> usize {
        self.prg_rom.num_banks().saturating_sub(1)
    }

    fn prg_bank_8000(&self) -> usize {
        match self.latch_mode {
            0 => (self.latch_value & 0x07) as usize, // UNROM: bits 2-0
            1 => ((self.latch_value >> 2) & 0x0F) as usize, // UN1ROM+CHRSW: bits 5-2
            2 => (self.latch_value & 0x0F) as usize, // UOROM: bits 3-0
            3 => self.last_16k_bank(),               // Reversed: fixed last
            4 => ((self.latch_value >> 4) & 0x03) as usize * 2, // GNROM: PP → 32 KiB bank, lower half
            5 | 6 | 7 => PRG_BANK3_LOWER_HALF,                  // fixed 32 KiB bank #3, lower half
            _ => 0, // latch_mode is always 0–7 (masked to 3 bits); unreachable in practice
        }
    }

    fn prg_bank_c000(&self) -> usize {
        match self.latch_mode {
            0 | 1 | 2 => self.last_16k_bank(),       // fixed last
            3 => (self.latch_value & 0x0F) as usize, // Reversed: switchable
            4 => ((self.latch_value >> 4) & 0x03) as usize * 2 + 1, // GNROM: upper half
            5 | 6 | 7 => PRG_BANK3_UPPER_HALF,       // fixed 32 KiB bank #3, upper half
            _ => 0, // latch_mode is always 0–7 (masked to 3 bits); unreachable in practice
        }
    }

    fn chr_bank_8k(&self) -> usize {
        match self.latch_mode {
            0 | 2 | 7 => 0,                                 // fixed CHR bank 0
            1 => (self.latch_value & 0x03) as usize,        // CC = bits 1-0
            3 => ((self.latch_value >> 4) & 0x03) as usize, // CC = bits 5-4
            4 | 5 => (self.latch_value & 0x03) as usize,    // CC = bits 1-0
            6 => (self.latch_value & 0x01) as usize,        // C = bit 0
            _ => 0, // latch_mode is always 0–7 (masked to 3 bits); unreachable in practice
        }
    }

    fn chr_write_protected(&self) -> bool {
        self.latch_mode >= 4
    }

    /// Decode a write to the 1M mode register ($42FC–$42FF) and apply it.
    ///
    /// Address encoding:  A1 = latch enable,  A0 = mirroring LSB
    /// Data encoding:     D7-D5 = latch mode, D4 = mirroring MSB
    /// `mirroring_type = (A0 << 1) | D4`
    fn apply_mode_register(&mut self, addr: u16, value: u8) {
        let latch_enable = (addr >> 1) & 1; // A1
        let mirroring_lsb = (addr & 1) as u8; // A0
        let mode = (value >> 5) & 0x07; // D7-D5
        let mirroring_msb = (value >> 4) & 0x01; // D4
        self.latch_enabled = latch_enable != 0;
        self.latch_mode = mode;
        self.mirroring_type = (mirroring_lsb << 1) | mirroring_msb;
    }

    fn wram_index(&self, addr: u16) -> usize {
        self.wram_bank as usize * WRAM_BANK_SIZE_8K + (addr - 0x6000) as usize
    }
}

impl Mapper for Mapper6Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.wram.get(self.wram_index(addr)).copied().unwrap_or(0),
            0x8000..=0xBFFF => self
                .prg_rom
                .read_with_base(self.prg_bank_8000(), 0x8000, addr),
            0xC000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank_c000(), 0xC000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x42FC..=0x42FF => self.apply_mode_register(addr, value),
            0x4500 => self.wram_bank = (value >> 4) & 0x03, // bits 5-4 select the 8 KiB WRAM bank
            0x6000..=0x7FFF => {
                let index = self.wram_index(addr);
                if index < self.wram.len() {
                    self.wram[index] = value;
                }
            }
            0x8000..=0xFFFF if self.latch_enabled => self.latch_value = value,
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
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // byte 0: latch_mode (bits 2-0)
        // byte 1: latch_value
        // byte 2: latch_enabled (bit 0) | mirroring_type (bits 2-1) | wram_bank (bits 5-4)
        vec![
            self.latch_mode & 0x07,
            self.latch_value,
            (self.latch_enabled as u8) | (self.mirroring_type << 1) | (self.wram_bank << 4),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.latch_mode = data[0] & 0x07;
            self.latch_value = data[1];
            self.latch_enabled = (data[2] & 0x01) != 0;
            self.mirroring_type = (data[2] >> 1) & 0x03;
            self.wram_bank = (data[2] >> 4) & 0x03;
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
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(mapper.wram_size(), 32 * 1024);
    }
}
