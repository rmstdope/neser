//! Mappers 21/22/23/25 - Konami VRC2/VRC4
//!
//! Known Limitations:
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.

use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

/// Mappers 21, 22, 23, 25 - Konami VRC2/VRC4
///
/// Hardware: Konami's VRC2 and VRC4 chips with different pin configurations
///
/// Specifications:
/// - VRC2a (Mapper 22): <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC2a>
/// - VRC2b (Mapper 23): <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC2b>
/// - VRC4 variants: <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC4_Pinout>
/// - IRQ: <https://www.nesdev.org/wiki/VRC_IRQ> (VRC4 only)
/// - PRG-ROM: Up to 512KB (two 8KB banks switchable, one fixed)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR: Up to 256KB (eight 1KB switchable banks) or CHR-RAM
/// - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
///
/// Mapper variants (different address line connections):
/// - Mapper 21: VRC4a (Wai Wai World 2) / VRC4c (Ganbare Goemon Gaiden 2)
/// - Mapper 22: VRC2a (no IRQ support)
/// - Mapper 23: VRC2b / VRC4e (has IRQ, typically VRC4)
/// - Mapper 25: VRC4b (Gradius II, Teenage Mutant Ninja Turtles II) / VRC4d (Bio Miracle)
///
/// Notes:
/// - VRC2 variants (22, some 23) have no IRQ support
/// - VRC4 has CPU-cycle or scanline-driven IRQ counter
/// - Different mappers due to different A0/A1 pin connections
/// - Used in Gradius II, Contra, Castlevania III (Japan)
///
/// Implementation:
/// - Supports all address line variants via mapper number
/// - VRC IRQ system fully implemented for VRC4 variants
/// - No expansion audio (see VRC6 for audio)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vrc2Vrc4Variant {
    Mapper21, // VRC4a, VRC4c
    Mapper22, // VRC2a (no IRQ)
    Mapper23, // VRC2b, VRC4e (has IRQ, typically treated as VRC4)
    Mapper25, // VRC4b, VRC4d
}

impl Vrc2Vrc4Variant {
    fn has_irq(&self) -> bool {
        match self {
            Vrc2Vrc4Variant::Mapper21 => true,
            Vrc2Vrc4Variant::Mapper22 => false, // VRC2 has no IRQ
            Vrc2Vrc4Variant::Mapper23 => true,
            Vrc2Vrc4Variant::Mapper25 => true,
        }
    }

    /// Returns the mask applied to the high nibble when writing a CHR bank register.
    /// VRC2 uses 4 high bits (0x0F); VRC4 uses 5 high bits (0x1F).
    fn chr_high_nibble_mask(&self) -> u8 {
        if self.has_irq() { 0x1F } else { 0x0F }
    }

    /// VRC2a (mapper 22) wires CHR data lines shifted right by 1; the low register
    /// bit is not connected to the CHR ROM address lines.
    fn shifts_chr_bank_right(&self) -> bool {
        *self == Vrc2Vrc4Variant::Mapper22
    }
}

/// Controls which address line mapping(s) are active for mapper 21.
///
/// iNES 1.0 uses Combined (both VRC4a and VRC4c active simultaneously).
/// NES 2.0 submapper 1 = VRC4a only, submapper 2 = VRC4c only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapper21PinMode {
    Combined,  // Both VRC4a (A1,A2) and VRC4c (A6,A7) active (iNES 1.0 default)
    Vrc4aOnly, // Submapper 1: A1→chip A0, A2→chip A1
    Vrc4cOnly, // Submapper 2: A6→chip A0, A7→chip A1
}

pub struct Vrc2Vrc4Mapper {
    variant: Vrc2Vrc4Variant,
    /// Mapper 21 only: which address line pin wiring is active.
    pin_mode: Mapper21PinMode,

    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    prg_ram: PrgRam,
    /// VRC4 WRAM enable ($9002 bit 0). When false, PRG RAM at $6000-$7FFF is inaccessible.
    prg_ram_enabled: bool,

    /// 8KB PRG bank for $8000-$9FFF (or $C000-$DFFF when swap mode is active)
    prg_bank_0: u8,
    /// 8KB PRG bank for $A000-$BFFF (always)
    prg_bank_1: u8,
    /// VRC4 swap mode: when true, $8000-$9FFF is fixed to second-to-last bank
    /// and $C000-$DFFF is controlled by prg_bank_0 (register $9002 bit 1)
    prg_swap_mode: bool,
    /// Eight 1KB CHR bank selectors; each is a 9-bit value (VRC4 supports 512KB CHR).
    /// Written via split low-nibble / high-nibble registers.
    chr_banks_1k: [u16; 8],

    mirroring_reg: u8,
    mirroring: NametableLayout,

    // --- VRC IRQ (used by VRC4 variants only) ---
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    irq_mode_cycle: bool,
    irq_enable_after_ack: bool,
    irq_asserted: bool,
    irq_prescaler: i32,
}

impl Vrc2Vrc4Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;

    /// Mask for the 5-bit PRG bank number (bits [4:0]).
    const PRG_BANK_MASK: u8 = 0x1F;
    /// Mask to preserve the low nibble of a 9-bit CHR bank value.
    const CHR_LOW_NIBBLE_MASK: u16 = 0x000F;
    /// Mask to preserve the high 5 bits of a 9-bit CHR bank value.
    const CHR_HIGH_BITS_MASK: u16 = 0x01F0;

    /// Starting value for the IRQ scanline prescaler (341 master clocks per scanline).
    const IRQ_PRESCALER_INIT: i32 = 341;
    /// Master clocks consumed per CPU cycle.
    const IRQ_PRESCALER_STEP: i32 = 3;

    /// Expected byte length of the registers snapshot produced by `registers_snapshot`.
    const SNAPSHOT_SIZE: usize = 27;

    // Flag bit positions in the snapshot flags byte.
    const FLAG_IRQ_ENABLED: u8 = 1 << 0;
    const FLAG_IRQ_MODE_CYCLE: u8 = 1 << 1;
    const FLAG_IRQ_ENABLE_AFTER_ACK: u8 = 1 << 2;
    const FLAG_IRQ_ASSERTED: u8 = 1 << 3;
    const FLAG_PRG_SWAP_MODE: u8 = 1 << 4;

    const FLAG_PRG_RAM_ENABLED: u8 = 1 << 5;

    pub fn new(
        mapper_number: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Self {
        Self::new_with_submapper(mapper_number, 0, prg_rom, chr_rom, mirroring)
    }

    pub fn new_with_submapper(
        mapper_number: u8,
        submapper: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Self {
        let variant = match mapper_number {
            21 => Vrc2Vrc4Variant::Mapper21,
            22 => Vrc2Vrc4Variant::Mapper22,
            23 => Vrc2Vrc4Variant::Mapper23,
            25 => Vrc2Vrc4Variant::Mapper25,
            _ => Vrc2Vrc4Variant::Mapper21,
        };

        let pin_mode = if variant == Vrc2Vrc4Variant::Mapper21 {
            match submapper {
                1 => Mapper21PinMode::Vrc4aOnly,
                2 => Mapper21PinMode::Vrc4cOnly,
                _ => Mapper21PinMode::Combined,
            }
        } else {
            Mapper21PinMode::Combined
        };

        Self {
            variant,
            pin_mode,
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_ram_enabled: false,
            prg_bank_0: 0,
            prg_bank_1: 0,
            prg_swap_mode: false,
            chr_banks_1k: [0u16; 8],
            mirroring_reg: 0,
            mirroring,

            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_mode_cycle: false,
            irq_enable_after_ack: false,
            irq_asserted: false,
            irq_prescaler: 0,
        }
    }

    fn prg_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE_8K
    }

    fn chr_bank_count_1k(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE_1K
    }

    fn prg_bank_index_8k(&self, bank: usize) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        bank % count
    }

    fn chr_bank_index_1k(&self, bank: u16) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn fixed_last_prg_bank_8k(&self) -> usize {
        let count = self.prg_bank_count_8k();
        count.saturating_sub(1)
    }

    fn fixed_second_to_last_prg_bank_8k(&self) -> usize {
        let count = self.prg_bank_count_8k();
        count.saturating_sub(2)
    }

    /// Normalize register address based on the mapper variant.
    ///
    /// Each mapper variant has different CPU address line connections to the chip's
    /// register address inputs:
    /// - Mapper 21: CPU A1→chip A0, CPU A2→chip A1 (VRC4a/VRC4c)
    /// - Mapper 22: CPU A1→chip A0, CPU A0→chip A1 (VRC2a) - swapped from normal
    /// - Mapper 23: CPU (A0|A1)→chip A0, CPU (A2|A3)→chip A1 (VRC2b/VRC4e) - uses OR of address lines
    /// - Mapper 25: CPU A1→chip A0, CPU A3→chip A1 (VRC4b/VRC4d)
    fn normalize_reg_addr(&self, addr: u16) -> u16 {
        // Base address uses A12-A15 for register selection
        let base = addr & 0xF000;

        match self.variant {
            Vrc2Vrc4Variant::Mapper21 => {
                // VRC4a: CPU A1→chip A0, CPU A2→chip A1  ($x000, $x002, $x004, $x006)
                // VRC4c: CPU A6→chip A0, CPU A7→chip A1  ($x000, $x040, $x080, $x0C0)
                // pin_mode selects which wiring(s) are active (submapper or combined for iNES 1.0).
                let a0 = match self.pin_mode {
                    Mapper21PinMode::Vrc4aOnly => (addr >> 1) & 0x01,
                    Mapper21PinMode::Vrc4cOnly => (addr >> 6) & 0x01,
                    Mapper21PinMode::Combined => ((addr >> 1) | (addr >> 6)) & 0x01,
                };
                let a1 = match self.pin_mode {
                    Mapper21PinMode::Vrc4aOnly => (addr >> 2) & 0x01,
                    Mapper21PinMode::Vrc4cOnly => (addr >> 7) & 0x01,
                    Mapper21PinMode::Combined => ((addr >> 2) | (addr >> 7)) & 0x01,
                };
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper22 => {
                // VRC2a: CPU A1→chip A0, CPU A0→chip A1 (swapped on bits 0-1)
                let a0 = (addr >> 1) & 0x01;
                let a1 = addr & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper23 => {
                // VRC2b/VRC4e: CPU (A0|A1)→chip A0, CPU (A2|A3)→chip A1
                let a0 = ((addr & 0x01) | ((addr >> 1) & 0x01)) & 0x01;
                let a1 = (((addr >> 2) & 0x01) | ((addr >> 3) & 0x01)) & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper25 => {
                // VRC4b/VRC4d: CPU A1→chip A0, CPU A3→chip A1 (bits 1 and 3)
                let a0 = (addr >> 1) & 0x01;
                let a1 = (addr >> 3) & 0x01;
                base | (a1 << 1) | a0
            }
        }
    }

    fn apply_mirroring_register(&mut self) {
        self.mirroring = if !self.variant.has_irq() {
            // VRC2: only horizontal or vertical mirroring; bit 1 is ignored.
            match self.mirroring_reg & 0x01 {
                0 => NametableLayout::Vertical,
                _ => NametableLayout::Horizontal,
            }
        } else {
            match self.mirroring_reg & 0x03 {
                0x0 => NametableLayout::Vertical,
                0x1 => NametableLayout::Horizontal,
                0x2 => NametableLayout::SingleScreenLower,
                0x3 => NametableLayout::SingleScreenUpper,
                _ => self.mirroring,
            }
        };
    }

    fn read_prg_rom_8k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE_8K + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE_1K + bank_offset;
        self.chr_memory.read_at_index(addr)
    }

    fn reset_irq_prescaler(&mut self) {
        // VRC IRQ scanline-mode prescaler (nesdev): 341 master ticks / 3 per CPU cycle.
        self.irq_prescaler = Self::IRQ_PRESCALER_INIT;
    }

    fn acknowledge_irq(&mut self) {
        self.irq_asserted = false;
    }

    fn clock_vrc_irq_counter(&mut self) {
        // VRC IRQ (nesdev):
        // If counter is $FF, reload from latch and trip IRQ; otherwise increment.
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_asserted = true;
        } else {
            self.irq_counter = self.irq_counter.wrapping_add(1);
        }
    }

    fn tick_vrc_irq(&mut self) {
        if !self.variant.has_irq() {
            return;
        }

        if !self.irq_enabled {
            return;
        }

        if self.irq_mode_cycle {
            self.clock_vrc_irq_counter();
            return;
        }

        self.irq_prescaler -= Self::IRQ_PRESCALER_STEP;
        if self.irq_prescaler <= 0 {
            self.irq_prescaler += Self::IRQ_PRESCALER_INIT;
            self.clock_vrc_irq_counter();
        }
    }

    /// Update a single 1KB CHR bank slot with either the low or high nibble of
    /// the 9-bit bank number.  Low nibble sets bits [3:0]; high nibble sets bits [8:4].
    fn write_chr_bank_nibble(&mut self, bank_idx: usize, is_high_nibble: bool, value: u8) {
        let high_mask = self.variant.chr_high_nibble_mask();
        if is_high_nibble {
            self.chr_banks_1k[bank_idx] = (self.chr_banks_1k[bank_idx] & Self::CHR_LOW_NIBBLE_MASK)
                | (((value & high_mask) as u16) << 4);
        } else {
            self.chr_banks_1k[bank_idx] =
                (self.chr_banks_1k[bank_idx] & Self::CHR_HIGH_BITS_MASK) | (value & 0x0F) as u16;
        }
    }

    /// Handle a normalised CHR bank register write ($B000-$E003).
    ///
    /// Registers are arranged as four pages ($Bxxx–$Exxx), each covering two 1KB CHR banks.
    /// Within each page, positions 0/1 address the even bank (low/high nibble) and
    /// positions 2/3 address the odd bank (low/high nibble).
    fn write_chr_bank_register(&mut self, reg: u16, value: u8) {
        let page_base: usize = match reg & 0xF000 {
            0xB000 => 0,
            0xC000 => 2,
            0xD000 => 4,
            0xE000 => 6,
            _ => return,
        };
        let pos = (reg & 0x0003) as usize;
        let bank_idx = page_base + (pos >> 1);
        let is_high_nibble = (pos & 1) != 0;
        self.write_chr_bank_nibble(bank_idx, is_high_nibble, value);
    }

    /// Handle a normalised write to the $9000-$9003 register range.
    ///
    /// Position 2 ($9002) is the VRC4-only PRG Swap Mode / WRAM control register.
    /// All other positions update the mirroring control register.
    fn write_9000_register(&mut self, reg: u16, value: u8) {
        let pos = reg & 0x0003;
        if pos == 0x0002 && self.variant.has_irq() {
            // VRC4 only: bit 1 = PRG swap mode, bit 0 = WRAM enable
            self.prg_swap_mode = (value & 0x02) != 0;
            self.prg_ram_enabled = (value & 0x01) != 0;
        } else {
            self.mirroring_reg = value;
            self.apply_mirroring_register();
        }
    }

    /// Handle a normalised IRQ register write ($F000-$F003).
    ///
    /// VRC2 variants silently ignore these writes (no IRQ hardware).
    fn write_irq_registers(&mut self, reg: u16, value: u8) {
        if !self.variant.has_irq() {
            return;
        }
        match reg {
            0xF000 => self.irq_latch = (self.irq_latch & 0xF0) | (value & 0x0F),
            0xF001 => self.irq_latch = (self.irq_latch & 0x0F) | ((value & 0x0F) << 4),
            0xF002 => {
                self.acknowledge_irq();
                self.reset_irq_prescaler();
                self.irq_mode_cycle = (value & 0b0000_0100) != 0;
                let enable = (value & 0b0000_0010) != 0;
                self.irq_enable_after_ack = (value & 0b0000_0001) != 0;
                if enable {
                    self.irq_enabled = true;
                    self.irq_counter = self.irq_latch;
                } else {
                    self.irq_enabled = false;
                }
            }
            0xF003 => {
                self.acknowledge_irq();
                self.irq_enabled = self.irq_enable_after_ack;
            }
            _ => {}
        }
    }
}

impl Mapper for Vrc2Vrc4Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF (only when WRAM is enabled; VRC2 is always enabled)
        if (!self.variant.has_irq() || self.prg_ram_enabled)
            && let Some(value) = self.prg_ram.try_read(addr)
        {
            return value;
        }

        match addr {
            0x8000..=0x9FFF => {
                let offset = (addr - 0x8000) as usize;
                let bank = if self.prg_swap_mode {
                    self.fixed_second_to_last_prg_bank_8k()
                } else {
                    self.prg_bank_index_8k((self.prg_bank_0 & Self::PRG_BANK_MASK) as usize)
                };
                self.read_prg_rom_8k(bank, offset)
            }
            0xA000..=0xBFFF => {
                let offset = (addr - 0xA000) as usize;
                let bank = self.prg_bank_index_8k((self.prg_bank_1 & Self::PRG_BANK_MASK) as usize);
                self.read_prg_rom_8k(bank, offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                let bank = if self.prg_swap_mode {
                    self.prg_bank_index_8k((self.prg_bank_0 & Self::PRG_BANK_MASK) as usize)
                } else {
                    self.fixed_second_to_last_prg_bank_8k()
                };
                self.read_prg_rom_8k(bank, offset)
            }
            0xE000..=0xFFFF => {
                let offset = (addr - 0xE000) as usize;
                self.read_prg_rom_8k(self.fixed_last_prg_bank_8k(), offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (only when WRAM is enabled; VRC2 is always enabled)
        if (!self.variant.has_irq() || self.prg_ram_enabled) && self.prg_ram.try_write(addr, value)
        {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = self.normalize_reg_addr(addr);
            match reg {
                0x8000..=0x8003 => self.prg_bank_0 = value & Self::PRG_BANK_MASK,
                0x9000..=0x9003 => self.write_9000_register(reg, value),
                0xA000..=0xA003 => self.prg_bank_1 = value & Self::PRG_BANK_MASK,
                0xB000..=0xB003 | 0xC000..=0xC003 | 0xD000..=0xD003 | 0xE000..=0xE003 => {
                    self.write_chr_bank_register(reg, value);
                }
                0xF000..=0xF003 => self.write_irq_registers(reg, value),
                _ => {}
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        let bank_slot = (addr as usize) / Self::CHR_BANK_SIZE_1K;
        let bank_offset = (addr as usize) % Self::CHR_BANK_SIZE_1K;

        let bank: u16 = self.chr_banks_1k.get(bank_slot).copied().unwrap_or(0);
        let effective_bank = if self.variant.shifts_chr_bank_right() {
            bank >> 1
        } else {
            bank
        };
        self.read_chr_1k(self.chr_bank_index_1k(effective_bank), bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let addr = (addr & 0x1FFF) as usize;
        self.chr_memory.write_at_index(addr, value);
    }

    fn cpu_cycle(&mut self) {
        trace_mapper!(5; "[vrc2_vrc4] cpu_cycle (irq)");
        if self.variant.has_irq() {
            self.tick_vrc_irq();
        }
    }

    fn irq_pending(&self) -> bool {
        if self.variant.has_irq() {
            self.irq_asserted
        } else {
            false
        }
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        match self.variant {
            Vrc2Vrc4Variant::Mapper21 => 21,
            Vrc2Vrc4Variant::Mapper22 => 22,
            Vrc2Vrc4Variant::Mapper23 => 23,
            Vrc2Vrc4Variant::Mapper25 => 25,
        }
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0]:    prg_bank_0
        // [1]:    prg_bank_1
        // [2-17]: chr_banks_1k[0-7] as little-endian u16 pairs (9-bit values)
        // [18]:   mirroring_reg
        // [19]:   irq_latch
        // [20]:   irq_counter
        // [21]:   flags (see FLAG_* constants)
        // [22-25]: irq_prescaler (little-endian i32)
        // [26]:   mirroring
        let mut snapshot = Vec::with_capacity(Self::SNAPSHOT_SIZE);
        snapshot.push(self.prg_bank_0);
        snapshot.push(self.prg_bank_1);
        for bank in &self.chr_banks_1k {
            snapshot.extend_from_slice(&bank.to_le_bytes());
        }
        snapshot.push(self.mirroring_reg);
        snapshot.push(self.irq_latch);
        snapshot.push(self.irq_counter);
        let mut flags = 0u8;
        if self.irq_enabled {
            flags |= Self::FLAG_IRQ_ENABLED;
        }
        if self.irq_mode_cycle {
            flags |= Self::FLAG_IRQ_MODE_CYCLE;
        }
        if self.irq_enable_after_ack {
            flags |= Self::FLAG_IRQ_ENABLE_AFTER_ACK;
        }
        if self.irq_asserted {
            flags |= Self::FLAG_IRQ_ASSERTED;
        }
        if self.prg_swap_mode {
            flags |= Self::FLAG_PRG_SWAP_MODE;
        }
        if self.prg_ram_enabled {
            flags |= Self::FLAG_PRG_RAM_ENABLED;
        }
        snapshot.push(flags);
        snapshot.extend_from_slice(&self.irq_prescaler.to_le_bytes());
        let mirroring = match self.mirroring {
            NametableLayout::Horizontal => 0u8,
            NametableLayout::Vertical => 1,
            NametableLayout::SingleScreenLower | NametableLayout::SingleScreen => 2,
            NametableLayout::SingleScreenUpper => 3,
            NametableLayout::FourScreen => 4,
        };
        snapshot.push(mirroring);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= Self::SNAPSHOT_SIZE {
            self.prg_bank_0 = data[0];
            self.prg_bank_1 = data[1];
            for (i, bank) in self.chr_banks_1k.iter_mut().enumerate() {
                *bank = u16::from_le_bytes([data[2 + i * 2], data[2 + i * 2 + 1]]);
            }
            self.mirroring_reg = data[18];
            self.irq_latch = data[19];
            self.irq_counter = data[20];
            let flags = data[21];
            self.irq_enabled = (flags & Self::FLAG_IRQ_ENABLED) != 0;
            self.irq_mode_cycle = (flags & Self::FLAG_IRQ_MODE_CYCLE) != 0;
            self.irq_enable_after_ack = (flags & Self::FLAG_IRQ_ENABLE_AFTER_ACK) != 0;
            self.irq_asserted = (flags & Self::FLAG_IRQ_ASSERTED) != 0;
            self.prg_swap_mode = (flags & Self::FLAG_PRG_SWAP_MODE) != 0;
            self.prg_ram_enabled = (flags & Self::FLAG_PRG_RAM_ENABLED) != 0;
            self.irq_prescaler = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
            self.mirroring = match data[26] {
                0 => NametableLayout::Horizontal,
                1 => NametableLayout::Vertical,
                2 => NametableLayout::SingleScreenLower,
                3 => NametableLayout::SingleScreenUpper,
                4 => NametableLayout::FourScreen,
                _ => NametableLayout::Horizontal,
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: self.variant.has_irq(),
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_vrc_mapper(
        mapper_number: u16,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new(
            mapper_number,
            prg_rom,
            chr_rom,
            mirroring,
        ))
        .expect("VRC mapper should be implemented")
    }

    fn create_vrc_mapper_with_submapper(
        mapper_number: u16,
        submapper: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new(mapper_number, prg_rom, chr_rom, mirroring)
                .with_submapper(submapper),
        )
        .expect("VRC mapper with submapper should be implemented")
    }

    #[test]
    fn test_vrc4_mapper_21_prg_banking() {
        // VRC4 PRG banking:
        // $8000-$9FFF: 8KB switchable (PRG Select 0, register $800x)
        // $A000-$BFFF: 8KB switchable (PRG Select 1, register $A00x)
        // $C000-$DFFF: fixed to second-to-last 8KB bank
        // $E000-$FFFF: fixed to last 8KB bank
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Select 8KB bank #1 at $8000-$9FFF
        mapper.write_prg(0x8000, 0x01);
        // Select 8KB bank #5 at $A000-$BFFF
        mapper.write_prg(0xA000, 0x05);

        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 should be bank 1");
        assert_eq!(mapper.read_prg(0x9FFF), 1, "$9FFF should still be bank 1");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should be bank 5");
        assert_eq!(mapper.read_prg(0xBFFF), 5, "$BFFF should still be bank 5");
        // $C000-$DFFF fixed to second-to-last (bank 6 for 8 banks)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-to-last bank 6"
        );
        // $E000-$FFFF fixed to last (bank 7)
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be last bank 7");
    }

    #[test]
    fn test_vrc2_mapper_22_no_irq() {
        // Mapper 22 is VRC2a which has no IRQ support
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Try to enable IRQ (should be ignored for VRC2)
        mapper.write_prg(0xF000, 0xFF);
        mapper.write_prg(0xF001, 0b0000_0110); // Enable in cycle mode

        // Run many cycles
        for _ in 0..1000 {
            mapper.cpu_cycle();
        }

        // IRQ should never trigger on VRC2
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_23_irq_cycle_mode() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Mapper 23 normalisation: (A0|A1)→chip A0, (A2|A3)→chip A1
        // Chip positions: 0=$F000, 1=$F001 or $F002, 2=$F004 or $F008, 3=$F005 or $F006
        // Set latch = 0xFE via split nibble writes:
        mapper.write_prg(0xF000, 0x0E); // chip pos 0: latch bits [3:0] = 0xE
        mapper.write_prg(0xF001, 0x0F); // chip pos 1 (A0=1): latch bits [7:4] = 0xF → latch = 0xFE
        // IRQ Control at chip pos 2 (A2=1 → $F004):
        mapper.write_prg(0xF004, 0b0000_0110); // M=1, E=1, A=0

        // After enable, counter reloaded to 0xFE
        // Cycle 1: 0xFE -> 0xFF (no IRQ)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        // Cycle 2: counter == 0xFF -> trip IRQ
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // IRQ Acknowledge at chip pos 3 (A0=1, A2=1 → $F005)
        mapper.write_prg(0xF005, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_25_chr_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 32);

        let mut mapper = create_vrc_mapper(25, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set CHR bank 0 (PPU $0000-$03FF) to bank 7 via low nibble write at $B000
        mapper.write_prg(0xB000, 7);
        assert_eq!(mapper.read_chr(0x0000), 7);

        // Set CHR bank 4 (PPU $1000-$13FF) to bank 15 via low nibble write at $D000
        // ($D000 → page_base=4, pos=0 → bank_idx=4, low nibble)
        mapper.write_prg(0xD000, 15);
        assert_eq!(mapper.read_chr(0x1000), 15);
    }

    #[test]
    fn test_vrc2_vrc4_mirroring_control() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Test vertical mirroring
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Test horizontal mirroring
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Test single screen lower bank mirroring (value 2)
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Test single screen upper bank mirroring (value 3)
        mapper.write_prg(0x9000, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    /// VRC4 spec: $8000-$9FFF and $A000-$BFFF are TWO INDEPENDENT 8KB switchable
    /// banks. Register $800x selects the 8KB bank at $8000-$9FFF (not 16KB).
    /// Register $A00x selects the 8KB bank at $A000-$BFFF.
    /// $C000-$DFFF is fixed to the second-to-last 8KB bank.
    /// $E000-$FFFF is fixed to the last 8KB bank.
    #[test]
    fn test_vrc4_prg_8000_9fff_and_a000_bfff_are_independent_8kb_banks() {
        let prg_rom = banked_data(8 * 1024, 8); // 8 × 8KB banks filled with index byte
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // PRG Select 0: bank 3 at $8000-$9FFF
        mapper.write_prg(0x8000, 3);
        // PRG Select 1: bank 5 at $A000-$BFFF
        mapper.write_prg(0xA000, 5);

        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 should read from 8KB bank 3"
        );
        assert_eq!(
            mapper.read_prg(0x9FFF),
            3,
            "$9FFF should still be in 8KB bank 3"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 should read from 8KB bank 5"
        );
        assert_eq!(
            mapper.read_prg(0xBFFF),
            5,
            "$BFFF should still be in 8KB bank 5"
        );
        // $C000-$DFFF fixed to second-to-last bank (bank 6 for 8 banks)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be fixed to second-to-last bank"
        );
        // $E000-$FFFF fixed to last bank (bank 7)
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should be fixed to last bank"
        );
    }

    /// VRC4 CHR banks are 9-bit values written via split low/high nibble registers.
    /// Low nibble register (even position after normalisation, e.g. $B000):
    ///   sets bits [3:0] of the CHR bank number.
    /// High nibble register (odd position after normalisation, e.g. $B001/physical $B002 on VRC4a):
    ///   sets bits [8:4] of the CHR bank number.
    /// For mapper 21 (VRC4a): A1→chip A0, A2→chip A1, so:
    ///   physical $B000 → chip pos 0 = low nibble of CHR bank 0
    ///   physical $B002 → A1=1 → chip A0=1 → chip pos 1 = high nibble of CHR bank 0
    #[test]
    fn test_vrc4_chr_split_nibble_combines_to_9bit_bank_number() {
        // Use 32 CHR banks of 1KB so we can address banks up to 31
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write CHR bank 0: target bank = 0x1F (0001_1111)
        // Low nibble  = 0xF (bits [3:0])  → physical $B000 (chip pos 0)
        // High nibble = 0x01 (bits [4])   → physical $B002 (chip pos 1, VRC4a A1=1)
        mapper.write_prg(0xB000, 0x0F); // low nibble = 0xF
        mapper.write_prg(0xB002, 0x01); // high nibble = 0x01 → bank bit 4 → bank 0x1F

        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "CHR $0000 should read from bank 31 (= low 0xF | high 0x10)"
        );
    }

    /// VRC4 IRQ latch is 8 bits split across two normalised register positions:
    ///   Normalised $F000 (pos 0): writes low 4 bits of latch
    ///   Normalised $F001 (pos 1): writes high 4 bits of latch
    ///   Normalised $F002 (pos 2): IRQ Control (E/M/A bits)
    ///   Normalised $F003 (pos 3): IRQ Acknowledge
    ///
    /// For mapper 21 VRC4a (A1→chip A0, A2→chip A1):
    ///   physical $F000 → chip pos 0 = IRQ latch low
    ///   physical $F002 → chip pos 1 = IRQ latch high
    ///   physical $F004 → chip pos 2 = IRQ Control
    ///   physical $F006 → chip pos 3 = IRQ Acknowledge
    #[test]
    fn test_vrc4_mapper21_irq_latch_split_low_high_nibbles() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set latch to 0xFE via split writes (VRC4a physical addresses):
        // low  nibble 0xE → $F000 (chip pos 0)
        // high nibble 0xF → $F002 (chip pos 1 for VRC4a, A1=1 → chip A0=1)
        mapper.write_prg(0xF000, 0x0E); // latch bits [3:0] = 0xE
        mapper.write_prg(0xF002, 0x0F); // latch bits [7:4] = 0xF → combined latch = 0xFE

        // IRQ Control at physical $F004 (chip pos 2, VRC4a A2=1 → chip A1=1): M=1, E=1
        mapper.write_prg(0xF004, 0b0000_0110);

        // After reload, counter = 0xFE. In cycle mode:
        // cycle 1: 0xFE → 0xFF (no IRQ yet)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ should not fire after 1 cycle");
        // cycle 2: 0xFF overflows → reload and fire IRQ
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ should fire after 2 cycles (latch=0xFE, cycle mode)"
        );

        // Acknowledge at physical $F006 (chip pos 3, VRC4a A1=1,A2=1 → chip A0=1,A1=1)
        mapper.write_prg(0xF006, 0);
        assert!(!mapper.irq_pending(), "IRQ should clear after acknowledge");
    }

    /// Mapper 21 implements BOTH VRC4a (A1→chip A0, A2→chip A1) and
    /// VRC4c (A6→chip A0, A7→chip A1) address mappings simultaneously.
    /// This test verifies VRC4c-style addresses (bit 6 / bit 7) work.
    ///
    /// VRC4c IRQ addresses: $F000 (latch low), $F040 (latch high),
    ///                      $F080 (control),   $F0C0 (ack)
    #[test]
    fn test_mapper21_vrc4c_register_addressing_via_a6_a7() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write latch 0xFE via VRC4c addresses:
        mapper.write_prg(0xF000, 0x0E); // latch low nibble  = 0xE  (chip pos 0)
        mapper.write_prg(0xF040, 0x0F); // latch high nibble = 0xF  (VRC4c: A6=1 → chip A0=1 → pos 1)

        // IRQ Control via VRC4c: A7=1 → chip A1=1 → chip pos 2 → $F080
        mapper.write_prg(0xF080, 0b0000_0110); // M=1, E=1

        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ via VRC4c addresses should fire");

        // Acknowledge via VRC4c: A6=1, A7=1 → chip A0=1, A1=1 → chip pos 3 → $F0C0
        mapper.write_prg(0xF0C0, 0);
        assert!(
            !mapper.irq_pending(),
            "IRQ should clear via VRC4c ack address"
        );
    }

    /// VRC4 $9002 is PRG Swap Mode + WRAM control, NOT mirroring.
    /// When Swap Mode bit (bit 1) is set:
    ///   $8000-$9FFF becomes fixed to the second-to-last bank (not PRG Select 0)
    ///   $C000-$DFFF becomes the switchable bank controlled by PRG Select 0
    #[test]
    fn test_vrc4_9002_swap_mode_swaps_prg_bank_windows() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Select PRG Select 0 bank = 2
        mapper.write_prg(0x8000, 2);

        // Before swap mode: bank 2 should be at $8000-$9FFF
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "before swap: $8000 should be bank 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "before swap: $C000 should be second-to-last"
        );

        // Enable swap mode: $9002 bit 1 = M
        // For mapper 21 (VRC4a): $9002 normalized → base=$9000, A1=1→chip A0=1 → norm pos 1
        // But $9002 is chip position 2 on the $9xxx range.
        // For VRC4a: A2=1 → chip A1=1 → norm $9002 (pos 2)
        mapper.write_prg(0x9004, 0b0000_0010); // VRC4a: A2=1 → norm $9002, M=1

        // After swap mode: $8000-$9FFF should be fixed to second-to-last (bank 6)
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "after swap: $8000 should be second-to-last"
        );
        // $C000-$DFFF should now be controlled by PRG Select 0 (bank 2)
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "after swap: $C000 should be bank 2"
        );
    }

    #[test]
    fn test_vrc2_vrc4_registers_snapshot_restores_state() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(
            21,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        );

        // PRG Select 0: bank 3 at $8000-$9FFF
        mapper.write_prg(0x8000, 0x03);
        // PRG Select 1: bank 5 at $A000-$BFFF
        mapper.write_prg(0xA000, 0x05);
        // CHR bank 0 low nibble = 2 → bank 2
        mapper.write_prg(0xB000, 0x02);
        // CHR bank 4 low nibble = 4 → bank 4 (physical $D000 for VRC4a: A1=A2=0 → pos 0)
        mapper.write_prg(0xD000, 0x04);
        // Mirroring horizontal
        mapper.write_prg(0x9000, 0x01);

        // IRQ latch = 0xFE via split nibble writes (VRC4a physical addresses):
        //   low  nibble 0xE → $F000 (chip pos 0)
        //   high nibble 0xF → $F002 (VRC4a: A1=1 → chip A0=1 → pos 1)
        mapper.write_prg(0xF000, 0x0E);
        mapper.write_prg(0xF002, 0x0F);
        // IRQ Control at $F004 (VRC4a: A2=1 → chip A1=1 → pos 2): M=1, E=1
        mapper.write_prg(0xF004, 0b0000_0110);
        mapper.cpu_cycle(); // counter: 0xFE → 0xFF (no IRQ yet)

        let regs = mapper.registers_snapshot();

        let mut restored = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Vertical);
        restored.restore_registers(&regs);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "PRG bank 0 should be restored"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            5,
            "PRG bank 1 should be restored"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            2,
            "CHR bank 0 should be restored"
        );
        assert_eq!(
            restored.read_chr(0x1000),
            4,
            "CHR bank 4 should be restored"
        );
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.irq_pending(), mapper.irq_pending());
    }

    // =========================================================================
    // Tests for remaining spec gaps (RED phase - should fail before fix)
    // =========================================================================

    /// VRC4 mirroring values 2 and 3 select one-screen lower and upper bank
    /// respectively, not both mapped to the same single screen.
    #[test]
    fn test_vrc4_single_screen_lower_and_upper_bank_mirroring() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Mirroring value 2 = one-screen, lower bank
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "value 2 should select single-screen lower bank"
        );

        // Mirroring value 3 = one-screen, upper bank
        mapper.write_prg(0x9000, 0x03);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "value 3 should select single-screen upper bank"
        );
    }

    /// VRC4 $9002 bit 0 is the WRAM enable bit.
    /// When clear (default after reset), PRG RAM at $6000-$7FFF is inaccessible
    /// (reads return open bus / 0, writes are ignored).
    /// When set, PRG RAM is accessible normally.
    #[test]
    fn test_vrc4_9002_wram_enable_gates_prg_ram_access() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // After reset, WRAM is disabled — write should be ignored
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "PRG RAM read should return 0 when WRAM disabled"
        );

        // Enable WRAM via $9002 bit 0 (VRC4a: $9004 → norm $9002)
        mapper.write_prg(0x9004, 0b0000_0001); // W=1

        // Now writes and reads should work
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG RAM read should return written value when WRAM enabled"
        );
    }

    /// NES 2.0 submapper 1 selects VRC4a-only addressing (A1→chip A0, A2→chip A1).
    /// A VRC4c-style address like $F040 (A6=1) must NOT be treated as chip pos 1
    /// when submapper 1 is active.
    #[test]
    fn test_mapper21_submapper1_uses_only_vrc4a_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        // Submapper 1 = VRC4a only
        let mut mapper =
            create_vrc_mapper_with_submapper(21, 1, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set latch low nibble via $F000 (chip pos 0 — same in all variants)
        mapper.write_prg(0xF000, 0x0E); // latch bits [3:0] = 0xE

        // Attempt latch HIGH via VRC4c-style $F040 (A6=1 → chip A0=1 on VRC4c, but NOT on VRC4a).
        // For submapper 1 (VRC4a only), this should be a no-op — latch high stays 0x0.
        mapper.write_prg(0xF040, 0x0F); // should be ignored under submapper 1

        // Enable IRQ cycle mode via VRC4a $F004 (A2=1 → chip A1=1 → pos 2)
        mapper.write_prg(0xF004, 0b0000_0110); // M=1, E=1

        // If $F040 was correctly ignored, latch = 0x0E.
        // Counter starts at 0x0E, needs 0xF2 cycles to reach 0xFF — IRQ will NOT fire in 2 cycles.
        // If $F040 was wrongly accepted (combined addressing), latch = 0xFE → IRQ fires after 2 cycles.
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(
            !mapper.irq_pending(),
            "submapper 1 (VRC4a only) must ignore VRC4c $F040 address for latch high"
        );
    }

    // =========================================================================
    // Mapper 22 (VRC2a) specific spec tests — issue #645
    // =========================================================================

    /// NESdev spec: "On VRC2a (mapper 22), the low bit is ignored (right shift value by 1)."
    /// When a CHR bank register is written, the effective bank number used for
    /// PPU reads must be (stored_value >> 1).
    #[test]
    fn test_vrc2a_mapper22_chr_bank_right_shifted_by_1() {
        // 16 banks × 1KB, each bank filled with its index byte
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 16);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC2a address normalisation: CPU A1 → chip A0, CPU A0 → chip A1 (swapped).
        // To write CHR bank 0 low nibble (chip pos 0 = $B000):
        //   chip A0=0, chip A1=0 → CPU: A1 = chip A0 = 0, CPU A0 = chip A1 = 0 → $B000
        // Write 0x02 (binary 0010). Without the shift the game would select bank 2;
        // with the spec-mandated >> 1 shift it must select bank 1.
        mapper.write_prg(0xB000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "VRC2a CHR bank must be right-shifted by 1: writing 0x02 must select bank 1, not bank 2"
        );

        // Write 0x04 → effective bank 2
        mapper.write_prg(0xB000, 0x04);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "VRC2a CHR bank must be right-shifted by 1: writing 0x04 must select bank 2"
        );

        // Odd values: low bit is discarded — 0x03 >> 1 = 1 → bank 1
        mapper.write_prg(0xB000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "VRC2a CHR bank: bit 0 is discarded, 0x03 >> 1 must select bank 1"
        );
    }

    /// NESdev spec: "VRC2 supports only vertical or horizontal mirroring. Bit 1 is ignored."
    /// Writing values 2 or 3 to the mirroring register on mapper 22 must not
    /// produce single-screen mirroring — only bit 0 is honoured.
    #[test]
    fn test_vrc2a_mapper22_mirroring_ignores_bit1() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC2a $9000: chip pos 0 (CPU A1=0, A0=0) → $9000
        // value 0b10 (2): bit 1 set, bit 0 clear → must yield Vertical (same as value 0)
        mapper.write_prg(0x9000, 0b10);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "VRC2a: mirroring value 2 (bit1=1,bit0=0) must behave as Vertical (bit 1 ignored)"
        );

        // value 0b11 (3): bit 1 set, bit 0 set → must yield Horizontal (same as value 1)
        mapper.write_prg(0x9000, 0b11);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "VRC2a: mirroring value 3 (bit1=1,bit0=1) must behave as Horizontal (bit 1 ignored)"
        );
    }

    /// NESdev spec: "VRC2 only has 4 high bits of CHR select. $B001 bit 4 is ignored."
    /// Writing a high nibble value with bit 4 set on VRC2a must NOT advance the bank
    /// beyond what 4 bits allow — bit 4 must be silently discarded.
    ///
    /// Test uses 48 CHR banks so that 256 % 48 = 16 ≠ 0, meaning a raw 9-bit stored
    /// value of 0x100 does NOT accidentally wrap back to bank 0.
    #[test]
    fn test_vrc2a_mapper22_chr_high_nibble_4_bits_only() {
        // 48 × 1KB banks (256 % 48 = 16, so a 9-bit stored value 0x100 would map to
        // bank 16 without the fix, proving the mask is checked).
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write high nibble = 0x10 (only bit 4 set) to CHR bank 0.
        // VRC2a chip pos 1 (chip A0=1): CPU A1 = chip A0 = 1, CPU A0 = chip A1 = 0 → $B002.
        // With 5-bit mask: stored = 0x100 = 256 → >> 1 = 128 → 128 % 48 = 32 → bank 32 (≠ 0).
        // With 4-bit mask: bit 4 discarded → stored = 0x000 → >> 1 = 0 → bank 0 ✓.
        mapper.write_prg(0xB002, 0x10); // high nibble, bit 4 only
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "VRC2a: CHR high nibble bit 4 must be ignored — bank should be 0, not 32 or 128"
        );
    }
}
