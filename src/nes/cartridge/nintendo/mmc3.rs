// ============================================================================
// MMC3 Mapper (Mapper 4) - Nintendo TxROM boards
// ============================================================================
//
// References:
// - Main: https://www.nesdev.org/wiki/MMC3
// - IRQ: https://www.nesdev.org/wiki/MMC3#IRQ_Specifics
// - Variants: https://www.nesdev.org/wiki/TxROM
//
// ============================================================================

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::common::A12RisingEdgeDetector;
use crate::nes::cartridge::rom_db;
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

// ============================================================================
// Mapper Structure & Constants
// ============================================================================

/// Mapper 4 - MMC3 (TxROM boards)
///
/// Hardware: Nintendo's second-most common mapper with scanline IRQ counter
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC3>
/// - IRQ: <https://www.nesdev.org/wiki/MMC3#IRQ_Specifics>
/// - Variants: <https://www.nesdev.org/wiki/TxROM>
/// - PRG-ROM: Up to 512KB (64 8KB banks)
/// - PRG-RAM: 8KB at $6000-$7FFF with enable/write-protect
/// - CHR: Up to 256KB (256 1KB banks, ROM or RAM)
/// - Mirroring: Programmable (horizontal or vertical)
///
/// Common boards: NES-TLROM, NES-TKROM, NES-TQROM, NES-TSROM
///
/// Notes:
/// - Scanline IRQ counter triggered by PPU A12 rising edges (MC-ACC, submapper 3: every
///   eighth falling edge)
/// - Two IRQ behaviors (Sharp vs NEC) selected per-ROM via CRC database
/// - Register $8000 selects target bank and PRG/CHR swap mode
/// - Used in Super Mario Bros. 3, Mega Man 3-6, Kirby's Adventure
///
/// Implementation:
/// - This implementation focuses on PRG/CHR banking + mirroring control
/// - Includes MMC3 scanline IRQ counter with both Sharp and NEC behaviors
/// - A12 edge detection with debounce (3 PPU cycles low required)
///
/// Known Limitations:
/// - IRQ timing is not fully PPU-cycle accurate in all edge cases.
/// - IRQ behavior selection is currently derived from ROM metadata/CRC heuristics.
/// - Some board-specific clone quirks are intentionally not modeled yet.
pub struct MMC3Mapper {
    pub(crate) base: BaseMapper,
    prg_ram: Vec<u8>,

    prg_ram_enabled: bool,
    prg_ram_write_protected: bool,

    bank_select: u8,
    regs: [u8; 8],

    // --- MMC3 scanline IRQ ---
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_asserted: bool,

    a12_detector: A12RisingEdgeDetector,

    /// Use alternate (NEC) IRQ behavior: only fire on 1→0 transition
    use_alternate_irq: bool,

    /// MMC6 (mapper 4, submapper 1): 1 KB PRG-RAM at $7000-$7FFF, enabled by
    /// $8000 bit 5 and gated per 512-byte half by `mmc6_protect` ($A001).
    mmc6: bool,
    /// MMC6 $A001 (HhLl xxxx); forced to $00 while $8000 bit 5 is clear.
    mmc6_protect: u8,

    /// Acclaim MC-ACC (mapper 4, submapper 3): the counter is clocked through a ÷8
    /// prescaler by falling edges of PPU A12, unfiltered, instead of by filtered rises.
    mc_acc: bool,
    /// MC-ACC: A12 as last seen on the PPU bus, to find its falling edges.
    mc_acc_prev_a12: bool,
    /// MC-ACC: falling edges seen in the current group of eight (0-7); the counter clocks
    /// on the first of each group, and a $C001 write starts a new group.
    mc_acc_pulses: u8,
}

// ============================================================================
// Mapper Initialization & Configuration
// ============================================================================

impl MMC3Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1KB
    const PRG_RAM_SIZE: usize = 0x2000; // 8KB
    const DEFAULT_PRG_RAM_BANKS_8K: u8 = 1;

    const PRG_RAM_ENABLE_MASK: u8 = 0b1000_0000;
    const PRG_RAM_WRITE_PROTECT_MASK: u8 = 0b0100_0000;

    const MMC6_SUBMAPPER: u8 = 1;
    const MMC6_PRG_RAM_SIZE: usize = 0x0400; // 1KB
    const MMC6_RAM_ENABLE_MASK: u8 = 0b0010_0000; // $8000 bit 5

    const MC_ACC_SUBMAPPER: u8 = 3;
    const MC_ACC_PRESCALER: u8 = 8;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let crc32 = ctx.crc32;
        let use_alternate_irq = rom_db::requires_mmc3_alternate_irq(crc32);
        let prg_ram_banks_8k = ctx.header_prg_ram_banks_8k();
        Self::new_internal(ctx, prg_ram_banks_8k, use_alternate_irq)
    }

    /// Create an MMC3 mapper with explicit IRQ behavior mode.
    /// Used by MMC3-based multicart wrappers (mappers 12, 37, 44, etc.).
    pub fn new_with_irq_mode(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
        use_alternate_irq: bool,
    ) -> Self {
        let ctx = crate::nes::cartridge::mapper::MapperContext {
            mapper: 4,
            submapper: 0,
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k: Self::DEFAULT_PRG_RAM_BANKS_8K,
            prg_ram_size_specified: true,
            battery_backed_prg_ram: false,
            hardware_type: crate::nes::cartridge::HardwareType::NesNtsc,
            chr_ram_size_bytes: None,
            crc32: 0,
            vs_hardware_type: None,
        };
        Self::new_internal(ctx, Self::DEFAULT_PRG_RAM_BANKS_8K, use_alternate_irq)
    }

    /// Create an MMC3 mapper with explicit IRQ behavior mode and PRG-RAM bank count.
    /// Used by MMC3-based wrappers that need to control whether PRG-RAM is present
    /// (e.g., mapper 292 which has no PRG-RAM at $6000–$7FFF).
    pub fn new_with_irq_mode_and_prg_ram_banks(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
        use_alternate_irq: bool,
        prg_ram_banks_8k: u8,
    ) -> Self {
        let ctx = crate::nes::cartridge::mapper::MapperContext {
            mapper: 4,
            submapper: 0,
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k,
            prg_ram_size_specified: true,
            battery_backed_prg_ram: false,
            hardware_type: crate::nes::cartridge::HardwareType::NesNtsc,
            chr_ram_size_bytes: None,
            crc32: 0,
            vs_hardware_type: None,
        };
        Self::new_internal(ctx, prg_ram_banks_8k, use_alternate_irq)
    }

    fn new_internal(
        ctx: crate::nes::cartridge::mapper::MapperContext,
        prg_ram_banks_8k: u8,
        use_alternate_irq: bool,
    ) -> Self {
        let mmc6 = ctx.mapper == 4 && ctx.submapper == Self::MMC6_SUBMAPPER;
        let mc_acc = ctx.mapper == 4 && ctx.submapper == Self::MC_ACC_SUBMAPPER;
        let prg_ram_size = if mmc6 {
            Self::MMC6_PRG_RAM_SIZE
        } else {
            prg_ram_banks_8k as usize * Self::PRG_RAM_SIZE
        };
        let has_prg_ram = prg_ram_size > 0;
        let mirroring = ctx.mirroring;

        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0, // MMC3 manages PRG-RAM separately
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);

        let mut mapper = Self {
            base,
            prg_ram: vec![0; prg_ram_size],
            prg_ram_enabled: has_prg_ram,
            prg_ram_write_protected: false,
            bank_select: 0,
            regs: [0; 8],

            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_asserted: false,

            a12_detector: A12RisingEdgeDetector::new(3),
            use_alternate_irq,

            mmc6,
            mmc6_protect: 0,

            mc_acc,
            mc_acc_prev_a12: false,
            mc_acc_pulses: 0,
        };
        mapper.base.set_mirroring(mirroring);
        mapper.update_banks();
        mapper
    }

    /// Builder method to set alternate (NEC) IRQ behavior.
    #[allow(dead_code)]
    pub fn with_irq_mode(mut self, use_alternate_irq: bool) -> Self {
        self.use_alternate_irq = use_alternate_irq;
        self
    }

    // ============================================================================
    // Bank Management Utilities
    // ============================================================================

    fn prg_bank_count(&self) -> usize {
        self.base.prg_bank_count()
    }

    pub fn chr_bank_count_1k(&self) -> usize {
        self.base.chr_bank_count()
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        let count = self.prg_bank_count();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn prg_mode(&self) -> bool {
        (self.bank_select & 0b0100_0000) != 0
    }

    fn chr_mode(&self) -> bool {
        (self.bank_select & 0b1000_0000) != 0
    }

    fn selected_reg(&self) -> usize {
        (self.bank_select & 0b0000_0111) as usize
    }

    fn read_prg_rom_bank(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE + bank_offset;
        self.base.prg_rom().get(addr).copied().unwrap_or(0)
    }

    /// Reads one byte from a 1 KiB CHR bank. `bank_index` is wrapped modulo
    /// the total CHR bank count so out-of-range values mirror correctly.
    fn read_chr_bank_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        let addr = (bank_index % count) * Self::CHR_BANK_SIZE + bank_offset;
        self.base.read_chr_at_index(addr)
    }

    fn update_prg_ram_control(&mut self, value: u8) {
        if self.prg_ram.is_empty() {
            self.prg_ram_enabled = false;
            self.prg_ram_write_protected = false;
            return;
        }
        self.prg_ram_enabled = (value & Self::PRG_RAM_ENABLE_MASK) != 0;
        self.prg_ram_write_protected = (value & Self::PRG_RAM_WRITE_PROTECT_MASK) != 0;
    }

    fn mmc6_ram_enabled(&self) -> bool {
        (self.bank_select & Self::MMC6_RAM_ENABLE_MASK) != 0
    }

    /// MMC6 read of $6000-$7FFF: `None` is open bus.
    /// Refs: https://www.nesdev.org/wiki/MMC6
    fn mmc6_read(&self, addr: u16) -> Option<u8> {
        if addr < 0x7000 {
            return None;
        }
        let read_low = (self.mmc6_protect & 0x20) != 0;
        let read_high = (self.mmc6_protect & 0x80) != 0;
        if !read_low && !read_high {
            return None;
        }
        let offset = (addr as usize) & (Self::MMC6_PRG_RAM_SIZE - 1);
        let readable = if offset < 0x200 { read_low } else { read_high };
        Some(if readable { self.prg_ram[offset] } else { 0 })
    }

    fn mmc6_write(&mut self, addr: u16, value: u8) {
        if addr < 0x7000 {
            return;
        }
        let offset = (addr as usize) & (Self::MMC6_PRG_RAM_SIZE - 1);
        // A half is writable only while it is also readable.
        let rw_bits = if offset < 0x200 { 0x30 } else { 0xC0 };
        if self.mmc6_protect & rw_bits == rw_bits {
            self.prg_ram[offset] = value;
        }
    }

    #[cfg(test)]
    fn irq_counter(&self) -> u8 {
        self.irq_counter
    }

    // ============================================================================
    // Public accessors for MMC3-based multicart composition (mappers 44, 47, 49)
    // ============================================================================

    /// Returns the raw 8KB PRG bank index for the given address, based on the
    /// current bank register state. Callers (multicart mappers) apply AND/OR masking.
    pub fn mapped_prg_bank(&self, addr: u16) -> usize {
        let prg_count = self.prg_bank_count();
        if prg_count == 0 {
            return 0;
        }
        let fixed_last = prg_count.saturating_sub(1);
        let fixed_second_last = prg_count.saturating_sub(2);
        let r6 = self.prg_bank_index(self.regs[6]);
        let r7 = self.prg_bank_index(self.regs[7]);
        match addr {
            0x8000..=0x9FFF => {
                if self.prg_mode() {
                    fixed_second_last
                } else {
                    r6
                }
            }
            0xA000..=0xBFFF => r7,
            0xC000..=0xDFFF => {
                if self.prg_mode() {
                    r6
                } else {
                    fixed_second_last
                }
            }
            0xE000..=0xFFFF => fixed_last,
            _ => 0,
        }
    }

    /// Returns the wrapped 1KB CHR bank index for the given PPU address, based on
    /// the current bank register state. The bank register value is wrapped modulo
    /// the total physical CHR bank count inside `map_chr_addr_to_bank_1k`. Callers
    /// apply AND/OR masking.
    pub fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        let chr_addr = (addr & 0x1FFF) as usize;
        let (bank_index, _) = self.map_chr_addr_to_bank_1k(chr_addr);
        bank_index
    }

    /// Returns the raw (unwrapped) 8KB PRG page number selected for a CPU address.
    ///
    /// This returns the logical MMC3 page number before modulo-wrapping against
    /// physical PRG size. Fixed pages are represented as 0xFE/0xFF.
    pub fn raw_prg_8k_page_number(&self, addr: u16) -> u8 {
        let reg6 = self.regs[6];
        let reg7 = self.regs[7];
        if !self.prg_mode() {
            match addr {
                0x8000..=0x9FFF => reg6,
                0xA000..=0xBFFF => reg7,
                0xC000..=0xDFFF => 0xFE,
                0xE000..=0xFFFF => 0xFF,
                _ => 0,
            }
        } else {
            match addr {
                0x8000..=0x9FFF => 0xFE,
                0xA000..=0xBFFF => reg7,
                0xC000..=0xDFFF => reg6,
                0xE000..=0xFFFF => 0xFF,
                _ => 0,
            }
        }
    }

    /// Returns the raw (unwrapped) 1KB CHR bank register value for the given PPU address.
    ///
    /// Unlike `mapped_chr_1k_bank`, this does NOT apply modulo-wrapping against the
    /// physical CHR bank count. Use this when the bank *number itself* is semantically
    /// meaningful (e.g., mapper 74 treats register values 8–9 as CHR-RAM, regardless
    /// of the ROM size).
    pub fn raw_chr_1k_bank(&self, addr: u16) -> usize {
        let chr_addr = (addr & 0x1FFF) as usize;
        let r0 = self.regs[0] & 0xFE; // 2KB bank, even-aligned
        let r1 = self.regs[1] & 0xFE; // 2KB bank, even-aligned
        let r2 = self.regs[2];
        let r3 = self.regs[3];
        let r4 = self.regs[4];
        let r5 = self.regs[5];
        let bank_1k = if !self.chr_mode() {
            // CHR mode 0
            match chr_addr {
                0x0000..=0x03FF => r0,
                0x0400..=0x07FF => r0.wrapping_add(1),
                0x0800..=0x0BFF => r1,
                0x0C00..=0x0FFF => r1.wrapping_add(1),
                0x1000..=0x13FF => r2,
                0x1400..=0x17FF => r3,
                0x1800..=0x1BFF => r4,
                0x1C00..=0x1FFF => r5,
                _ => 0,
            }
        } else {
            // CHR mode 1
            match chr_addr {
                0x0000..=0x03FF => r2,
                0x0400..=0x07FF => r3,
                0x0800..=0x0BFF => r4,
                0x0C00..=0x0FFF => r5,
                0x1000..=0x13FF => r0,
                0x1400..=0x17FF => r0.wrapping_add(1),
                0x1800..=0x1BFF => r1,
                0x1C00..=0x1FFF => r1.wrapping_add(1),
                _ => 0,
            }
        };
        bank_1k as usize
    }

    /// Reads a byte from PRG ROM at the given 8KB bank and byte offset.
    ///
    /// `bank` is wrapped modulo total PRG bank count so outer-bank mappers that
    /// produce high address bits on smaller ROMs mirror correctly instead of
    /// reading out-of-range zeros.
    pub fn read_prg_at_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.prg_bank_count();
        if count == 0 {
            return 0;
        }
        self.read_prg_rom_bank(bank % count, offset)
    }

    /// Reads a byte from CHR memory at the given 1KB bank and byte offset.
    pub fn read_chr_1k_at(&self, bank: usize, offset: usize) -> u8 {
        self.read_chr_bank_1k(bank, offset)
    }

    /// Writes a byte to CHR-RAM at the given 1KB bank and byte offset.
    pub fn write_chr_1k_at(&mut self, bank: usize, offset: usize, value: u8) {
        let mapped_addr = bank * Self::CHR_BANK_SIZE + offset;
        self.base.write_chr_at_index(mapped_addr, value);
    }

    /// Returns true if the PRG-RAM window is enabled and writable (used by
    /// multicart mappers that gate the block register behind PRG-RAM control).
    pub fn is_prg_ram_writable(&self) -> bool {
        !self.prg_ram.is_empty() && self.prg_ram_enabled && !self.prg_ram_write_protected
    }

    /// Returns true if the PRG-RAM window is currently enabled ($A001 bit 7).
    ///
    /// Unlike `is_prg_ram_writable`, this only checks the cached enable flag and
    /// does not consider the write-protect flag. However, when no PRG-RAM is
    /// present, the mapper forces this flag false, so this is not a generic proxy
    /// for raw `$A001.7` state on no-WRAM boards. Used by MMC3 variant mappers
    /// (e.g., mapper 259 BMC-F15) that gate outer-bank register writes behind
    /// the PRG-RAM enable bit on boards that actually provide PRG-RAM.
    pub fn is_prg_ram_enabled(&self) -> bool {
        self.prg_ram_enabled
    }

    /// Returns the raw CHR bank register value at the given index (0–7).
    /// This is the raw register value without CHR-mode-dependent interpretation.
    /// Used by mappers that override CHR banking but still need MMC3 register values.
    pub fn chr_bank_reg(&self, index: usize) -> u8 {
        self.regs.get(index).copied().unwrap_or(0)
    }

    /// Returns the current value of the bank-select register ($8000 even writes).
    ///
    /// Useful for MMC3 variant mappers that need to inspect the bank-select value
    /// when intercepting bank-data writes ($8001).
    pub fn bank_select_reg(&self) -> u8 {
        self.bank_select
    }

    // ============================================================================
    // PPU A12 Edge Detection for IRQ Timing
    // ============================================================================
    //
    // The MMC3 mapper generates IRQs via cycle-accurate A12 edge detection.
    // A12 is PPU address line 12 (bit 12 of the PPU address bus).
    //
    // Edge detection process:
    // 1. Track A12 state changes via PPU address bus
    // 2. Detect rising edge (transition from 0 to 1)
    // 3. A12 must be low for at least 3 CPU cycles before rising edge counts
    // 4. Each valid rising edge clocks the IRQ counter
    // 5. Generate IRQ when counter reaches 0 (behavior varies by chip)
    //
    // A12 Low-Pass Filter:
    // ```text
    // CPU Cycle: 1     2     3     4
    // A12:       Low   Low   Low   High → Valid rising edge, clocks IRQ
    //
    // CPU Cycle: 1     2     3
    // A12:       Low   Low   High       → Invalid, only 2 cycles low
    // ```
    //
    // IRQ Counter Behavior:
    // - Sharp MMC3: IRQ when counter IS 0 after clocking
    // - NEC MMC3: IRQ only on 1→0 TRANSITION
    //
    // Register Layout:
    // ```text
    // $C000 (write): IRQ Latch
    // 7  bit  0
    // ---- ----
    // LLLL LLLL
    // |||| ||||
    // ++++-++++- IRQ counter reload value
    //
    // $C001 (write): IRQ Reload
    // - Clears counter to 0 immediately
    // - Sets reload flag for next A12 rising edge
    //
    // $E000 (write): IRQ Disable
    // - Disables IRQ generation
    // - Acknowledges pending IRQ
    //
    // $E001 (write): IRQ Enable
    // - Enables IRQ generation
    // ```
    //
    // See: https://www.nesdev.org/wiki/MMC3#IRQ_Specifics

    fn clock_irq_counter(&mut self) {
        // MMC3 IRQ counter behavior:
        // - On each A12 rising edge, update the counter.
        // - If counter==0 or reload requested: load counter from latch.
        // - Else: decrement counter.
        //
        // IRQ assertion differs by chip variant:
        // - Normal (Sharp): IRQ when counter IS 0 after update
        // - Alternate (NEC): IRQ only on 1→0 TRANSITION (decrement or reload-triggered reload)
        let old_counter = self.irq_counter;
        let was_reload = self.irq_reload;

        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }

        trace_mapper!(4; "MMC3 IRQ clock: old_counter={}, reload_flag={}, latch={}, new_counter={}, enabled={}, alternate={}", 
            old_counter, was_reload, self.irq_latch, self.irq_counter, self.irq_enabled, self.use_alternate_irq);

        // Determine if IRQ should fire
        let should_fire_irq = if self.use_alternate_irq {
            // Alternate (NEC) behavior: IRQ only on 1→0 transition.
            // - Decrementing from 1 to 0: fire
            // - Reloading to 0 when reload flag was set (via $C001): fire
            // - Reloading to 0 because counter was already 0 (natural): DON'T fire
            let decremented_to_zero = old_counter == 1 && self.irq_counter == 0;
            let reload_triggered_to_zero = was_reload && self.irq_counter == 0;
            self.irq_counter == 0 && (decremented_to_zero || reload_triggered_to_zero)
        } else {
            // Normal (Sharp) behavior: IRQ when counter is 0
            self.irq_counter == 0
        };

        if should_fire_irq && self.irq_enabled {
            trace_mapper!(1; "MMC3 IRQ ASSERTED!");
            self.irq_asserted = true;
        }
    }

    /// MC-ACC: track A12 and clock the counter on the first falling edge of every eight
    /// (NESdev MMC3, "MC-ACC"; the prescaler phase is krikzz's research, as in Mesen2).
    fn mc_acc_a12(&mut self, addr: u16) {
        let a12 = addr & 0x1000 != 0;
        let falling = self.mc_acc_prev_a12 && !a12;
        self.mc_acc_prev_a12 = a12;
        if !falling {
            return;
        }
        if self.mc_acc_pulses == 0 {
            trace_mapper!(5; "MC-ACC A12 falling edge clocks the counter: addr=${:04X}", addr);
            self.clock_irq_counter();
        }
        self.mc_acc_pulses = (self.mc_acc_pulses + 1) % Self::MC_ACC_PRESCALER;
    }

    // ============================================================================
    // CHR Bank Switching Logic
    // ============================================================================

    fn map_chr_addr_to_bank_1k(&self, chr_addr: usize) -> (usize, usize) {
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE - 1);

        let r0 = self.regs[0] & 0xFE; // 2KB bank, even-aligned
        let r1 = self.regs[1] & 0xFE; // 2KB bank, even-aligned
        let r2 = self.regs[2];
        let r3 = self.regs[3];
        let r4 = self.regs[4];
        let r5 = self.regs[5];

        let bank_1k = if !self.chr_mode() {
            // CHR mode 0
            match chr_addr {
                0x0000..=0x03FF => r0,
                0x0400..=0x07FF => r0.wrapping_add(1),
                0x0800..=0x0BFF => r1,
                0x0C00..=0x0FFF => r1.wrapping_add(1),
                0x1000..=0x13FF => r2,
                0x1400..=0x17FF => r3,
                0x1800..=0x1BFF => r4,
                0x1C00..=0x1FFF => r5,
                _ => 0,
            }
        } else {
            // CHR mode 1
            match chr_addr {
                0x0000..=0x03FF => r2,
                0x0400..=0x07FF => r3,
                0x0800..=0x0BFF => r4,
                0x0C00..=0x0FFF => r5,
                0x1000..=0x13FF => r0,
                0x1400..=0x17FF => r0.wrapping_add(1),
                0x1800..=0x1BFF => r1,
                0x1C00..=0x1FFF => r1.wrapping_add(1),
                _ => 0,
            }
        };

        (self.chr_bank_index_1k(bank_1k), bank_offset)
    }

    fn update_banks(&mut self) {
        // PRG: 4 × 8KB slots
        if self.prg_mode() {
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(1, self.regs[7] as i16);
            self.base.select_prg_page(2, self.regs[6] as i16);
            self.base.select_prg_page(3, -1);
        } else {
            self.base.select_prg_page(0, self.regs[6] as i16);
            self.base.select_prg_page(1, self.regs[7] as i16);
            self.base.select_prg_page(2, -2);
            self.base.select_prg_page(3, -1);
        }

        // CHR: 8 × 1KB slots
        let r0 = (self.regs[0] & 0xFE) as i16;
        let r1 = (self.regs[1] & 0xFE) as i16;
        let r2 = self.regs[2] as i16;
        let r3 = self.regs[3] as i16;
        let r4 = self.regs[4] as i16;
        let r5 = self.regs[5] as i16;

        if !self.chr_mode() {
            self.base.select_chr_page(0, r0);
            self.base.select_chr_page(1, r0 + 1);
            self.base.select_chr_page(2, r1);
            self.base.select_chr_page(3, r1 + 1);
            self.base.select_chr_page(4, r2);
            self.base.select_chr_page(5, r3);
            self.base.select_chr_page(6, r4);
            self.base.select_chr_page(7, r5);
        } else {
            self.base.select_chr_page(0, r2);
            self.base.select_chr_page(1, r3);
            self.base.select_chr_page(2, r4);
            self.base.select_chr_page(3, r5);
            self.base.select_chr_page(4, r0);
            self.base.select_chr_page(5, r0 + 1);
            self.base.select_chr_page(6, r1);
            self.base.select_chr_page(7, r1 + 1);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::mmc3::MMC3Mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mmc3_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(4, prg_rom, chr_rom, mirroring))
            .expect("MMC3 (mapper 4) should be implemented")
    }

    #[test]
    fn test_mmc3_new_prg_ram_enabled_by_default() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Horizontal, false);
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn test_mmc3_unspecified_prg_ram_size_behaves_as_no_prg_ram() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);
        let metadata =
            MapperContext::new_for_test(4, prg_rom, chr_rom, NametableLayout::Horizontal)
                .with_unspecified_prg_ram_size();
        let mut mapper = create_mapper(metadata).expect("MMC3 (mapper 4) should be implemented");

        assert_eq!(
            mapper.wram_size(),
            0,
            "unspecified PRG-RAM should default to no PRG-RAM for MMC3"
        );

        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x00,
            "reads should return 0 when PRG-RAM is absent"
        );

        let open_bus = 0x5C;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "open-bus read should be returned when PRG-RAM is absent"
        );
    }

    // --- MMC6 (mapper 4, submapper 1), https://www.nesdev.org/wiki/MMC6 ---

    fn create_mmc6_mapper() -> Box<dyn Mapper> {
        let ctx = MapperContext::new_for_test(
            4,
            banked_data(8 * 1024, 16),
            banked_data(1024, 16),
            NametableLayout::Horizontal,
        )
        .with_submapper(1);
        create_mapper(ctx).expect("MMC6 (mapper 4.1) should be implemented")
    }

    /// $8000 bit 5 = 1 enables PRG-RAM; $A001 = HhLl xxxx.
    fn mmc6_enable(mapper: &mut Box<dyn Mapper>, a001: u8) {
        mapper.write_prg(0x8000, 0x20);
        mapper.write_prg(0xA001, a001);
    }

    #[test]
    fn test_mmc6_a001_f0_reads_and_writes_both_halves() {
        // $A001 = $F0 enables reading and writing both 512-byte halves. On the MMC3,
        // bit 6 would write-protect; StarTropics writes $F0 and needs its RAM (nr-cl1).
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x7000, 0xAA);
        mapper.write_prg(0x7200, 0xBB);
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0xAA);
        assert_eq!(mapper.read_prg_open_bus(0x7200, 0x5C), 0xBB);
    }

    #[test]
    fn test_mmc6_has_1k_prg_ram_mirrored_through_7000_7fff() {
        let mut mapper = create_mmc6_mapper();
        assert_eq!(mapper.wram_size(), 1024);
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x7001, 0x11);
        mapper.write_prg(0x73FF, 0x22);
        assert_eq!(mapper.read_prg_open_bus(0x7401, 0x5C), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x7C01, 0x5C), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, 0x5C), 0x22);
        mapper.write_prg(0x7E05, 0x33);
        assert_eq!(mapper.read_prg_open_bus(0x7205, 0x5C), 0x33);
    }

    #[test]
    fn test_mmc6_6000_6fff_is_open_bus() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x6000, 0x44);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0x5C), 0x5C);
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x00);
    }

    #[test]
    fn test_mmc6_only_one_half_readable_the_other_reads_zero() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x7000, 0xAA);
        mapper.write_prg(0x7200, 0xBB);
        mapper.write_prg(0xA001, 0x20); // read $7000-$71FF only
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0xAA);
        assert_eq!(mapper.read_prg_open_bus(0x7200, 0x5C), 0x00);
        mapper.write_prg(0xA001, 0x80); // read $7200-$73FF only
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x00);
        assert_eq!(mapper.read_prg_open_bus(0x7200, 0x5C), 0xBB);
    }

    #[test]
    fn test_mmc6_neither_half_readable_is_open_bus() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0x00);
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x5C);
        assert_eq!(mapper.read_prg_open_bus(0x7200, 0x5C), 0x5C);
    }

    #[test]
    fn test_mmc6_write_needs_read_enable_of_that_half() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x7000, 0xAA);
        mapper.write_prg(0x7200, 0xBB);
        // Write enable without read enable for the low half; high half read-only.
        mapper.write_prg(0xA001, 0x90);
        mapper.write_prg(0x7000, 0x01);
        mapper.write_prg(0x7200, 0x02);
        mapper.write_prg(0xA001, 0xA0);
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0xAA);
        assert_eq!(mapper.read_prg_open_bus(0x7200, 0x5C), 0xBB);
    }

    #[test]
    fn test_mmc6_8000_bit5_clear_disables_ram_and_ignores_a001() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0xF0);
        mapper.write_prg(0x7000, 0xAA);
        mapper.write_prg(0x8000, 0x00); // PRG-RAM disabled: $A001 forced to $00
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x5C);
        mapper.write_prg(0xA001, 0xF0); // ignored while disabled
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x5C);
        mapper.write_prg(0x8000, 0x20); // re-enabled, but $A001 is still $00
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0x5C);
        mapper.write_prg(0xA001, 0x30);
        assert_eq!(mapper.read_prg_open_bus(0x7000, 0x5C), 0xAA);
    }

    #[test]
    fn test_mmc6_save_state_keeps_a001() {
        let mut mapper = create_mmc6_mapper();
        mmc6_enable(&mut mapper, 0x30);
        mapper.write_prg(0x7000, 0xAA);
        let registers = mapper.registers_snapshot();
        let wram = mapper.wram_snapshot();

        let mut restored = create_mmc6_mapper();
        restored.restore_registers(&registers);
        restored.load_wram_snapshot(&wram);
        assert_eq!(restored.read_prg_open_bus(0x7000, 0x5C), 0xAA);
        assert_eq!(restored.read_prg_open_bus(0x7200, 0x5C), 0x00);
    }

    // --- Acclaim MC-ACC (mapper 4, submapper 3), https://www.nesdev.org/wiki/MMC3 ---
    // "The scanline counter uses a ÷8 prescaler instead of filtering A12, clocked by
    // falling edges." The prescaler's phase (the counter clocks on the first of every
    // eight falling edges) and its reset on a $C001 write are not on the wiki: they come
    // from krikzz's hardware research as Mesen2 implements it (Mmc3Variants/McAcc.h).

    fn create_mc_acc_mapper() -> Box<dyn Mapper> {
        let ctx = MapperContext::new_for_test(
            4,
            banked_data(8 * 1024, 16),
            banked_data(1024, 16),
            NametableLayout::Horizontal,
        )
        .with_submapper(3);
        create_mapper(ctx).expect("MC-ACC (mapper 4.3) should be implemented")
    }

    /// One A12 pulse as 8x8 sprites at $1000 make it: high (the pattern fetch), then
    /// low (the next slot's garbage nametable fetch, which the PPU reports through
    /// `ppu_nametable_address`), with no CPU cycle in between.
    fn a12_pulse(mapper: &mut Box<dyn Mapper>) {
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_nametable_address(0x2000);
    }

    #[test]
    fn test_mc_acc_counter_clocks_on_a12_falling_edge_not_rising() {
        let mut mapper = create_mc_acc_mapper();
        mapper.write_prg(0xC000, 0); // latch 0: the first clock reloads to 0 and fires
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        mapper.ppu_address_changed(0x0000);
        mapper.ppu_address_changed(0x1000); // rising edge, no low-time filter on MC-ACC
        assert!(!mapper.irq_pending(), "MC-ACC must not clock on A12 rising");
        mapper.ppu_address_changed(0x0000); // falling edge
        assert!(mapper.irq_pending(), "MC-ACC clocks on A12 falling");
    }

    #[test]
    fn test_mc_acc_counter_clocks_once_per_eight_falling_edges() {
        let mut mapper = create_mc_acc_mapper();
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        // Pulse 1 clocks: reload to 1, no IRQ. Pulses 2-8 are swallowed by the prescaler.
        for pulse in 1..=8 {
            a12_pulse(&mut mapper);
            assert!(!mapper.irq_pending(), "no IRQ after pulse {pulse}");
        }
        // Pulse 9 starts the next group of eight and clocks: 1 -> 0, IRQ.
        a12_pulse(&mut mapper);
        assert!(mapper.irq_pending(), "IRQ on the 9th falling edge");
    }

    #[test]
    fn test_mc_acc_c001_resets_the_prescaler() {
        let mut mapper = create_mc_acc_mapper();
        mapper.write_prg(0xC000, 0);
        mapper.write_prg(0xE001, 0);
        mapper.write_prg(0xC001, 0);
        a12_pulse(&mut mapper); // clocks: reload to 0, IRQ
        mapper.write_prg(0xE000, 0); // acknowledge
        mapper.write_prg(0xE001, 0);
        a12_pulse(&mut mapper); // 2nd of eight: swallowed
        assert!(!mapper.irq_pending());

        mapper.write_prg(0xC001, 0); // resets the prescaler: the next edge is the 1st
        a12_pulse(&mut mapper);
        assert!(
            mapper.irq_pending(),
            "the first falling edge after $C001 clocks"
        );
    }

    #[test]
    fn test_mc_acc_save_state_keeps_the_prescaler() {
        let mut mapper = create_mc_acc_mapper();
        mapper.write_prg(0xC000, 0);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);
        a12_pulse(&mut mapper); // 1st clocks
        mapper.write_prg(0xE000, 0);
        mapper.write_prg(0xE001, 0);
        for _ in 2..=8 {
            a12_pulse(&mut mapper);
        }
        let snapshot = mapper.registers_snapshot();
        let mut restored = create_mc_acc_mapper();
        restored.restore_registers(&snapshot);
        assert!(!restored.irq_pending());
        a12_pulse(&mut restored); // 9th: clocks again
        assert!(
            restored.irq_pending(),
            "prescaler position survives a save state"
        );
    }

    #[test]
    fn test_mmc3_irq_asserts_after_a12_rising_edges() {
        // Minimal MMC3 scanline IRQ spec (A12 rising edge counter):
        // With latch=1, reload requested, and IRQ enabled, the IRQ should assert
        // on the second A12 rising edge.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set IRQ latch to 1
        mapper.write_prg(0xC000, 1);
        // Request reload
        mapper.write_prg(0xC001, 0);
        // Enable IRQ
        mapper.write_prg(0xE001, 0);

        // First A12 rising edge ($0xxx -> $1xxx). MMC3 requires A12 low for 3 CPU cycles.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        // Second A12 rising edge
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(mapper.irq_pending());

        // Acknowledge/disable should clear IRQ
        mapper.write_prg(0xE000, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_mmc3_irq_a12_rising_edge_requires_3_cpu_cycles_low() {
        // MMC3 has a simple A12 low-pass filter: a rising edge should only clock
        // the IRQ counter if A12 has been low for at least 3 CPU cycles.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xC000, 1); // latch=1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        // A12 low for 1 CPU cycle, then rising edge: should be ignored.
        mapper.ppu_address_changed(0x0FFF);
        mapper.cpu_cycle();
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        // Now hold A12 low for 3 CPU cycles, then rising edge: this clocks once.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        // Another valid edge after 3 low cycles: second clock should assert IRQ.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(mapper.irq_pending());
    }

    #[test]
    fn test_mmc3_irq_a12_low_pass_uses_cpu_cycles() {
        // NesDev: A12 must be low for at least 3 falling edges of M2 (CPU cycles)
        // before a rising edge clocks the IRQ counter.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xC000, 1); // latch=1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        // Only 2 CPU cycles with A12 low: rising edge should be ignored.
        mapper.ppu_address_changed(0x0FFF);
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        // 3 CPU cycles with A12 low: first valid edge clocks but should not assert yet.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        // Second valid edge after 3 low cycles should assert IRQ.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(mapper.irq_pending());
    }

    #[test]
    fn test_mmc3_registers_snapshot_preserves_a12_low_pass_state() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        );

        mapper.write_prg(0xC000, 0); // latch=0
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        // Establish A12 low state and 3 low CPU cycles to satisfy the filter.
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }

        let saved = mapper.registers_snapshot();

        let mut restored = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);
        restored.restore_registers(&saved);

        restored.ppu_address_changed(0x1000);
        assert!(
            restored.irq_pending(),
            "A12 low-pass state should be preserved across save-state"
        );
    }

    #[test]
    fn test_mmc3_irq_reload_clears_counter_immediately() {
        // NesDev: Writing to $C001 (IRQ reload) clears the IRQ counter immediately to 0
        // and sets the reload flag. The counter reloads from latch on the next A12 rising edge.
        // Refs: https://www.nesdev.org/wiki/MMC3#IRQ_reload_(-,_odd)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Horizontal, false);

        // Set latch to a non-zero value
        mapper.write_prg(0xC000, 5);

        // Simulate an A12 rising edge to load the counter from the latch
        // (Counter starts at 0, so it will reload to 5)
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);

        // Counter should now be 5
        assert_eq!(
            mapper.irq_counter(),
            5,
            "Counter should be loaded with latch value 5"
        );

        // Simulate another A12 rising edge to decrement counter
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);

        // Counter should now be 4
        assert_eq!(
            mapper.irq_counter(),
            4,
            "Counter should be decremented to 4"
        );

        // Now write to $C001 (reload): should clear counter to 0 immediately
        mapper.write_prg(0xC001, 0);

        // THE KEY TEST: Counter should be 0 immediately after writing to $C001
        assert_eq!(
            mapper.irq_counter(),
            0,
            "Counter should be cleared to 0 immediately after writing to $C001"
        );

        // Next A12 rising edge should reload from latch (5)
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);

        // Counter should now be 5 (reloaded from latch)
        assert_eq!(
            mapper.irq_counter(),
            5,
            "Counter should be reloaded from latch to 5"
        );
    }

    #[test]
    fn test_mmc3_prg_bank_switching_modes() {
        // MMC3 PRG banking (no IRQ): four 8KB CPU banks at $8000-$FFFF.
        // - PRG mode 0: $8000 = R6 (switch), $A000 = R7 (switch), $C000 = fixed second-last, $E000 = fixed last
        // - PRG mode 1: $8000 = fixed second-last, $A000 = R7 (switch), $C000 = R6 (switch), $E000 = fixed last

        let prg_rom = banked_data(8 * 1024, 8); // 8 x 8KB banks; last=7, second-last=6
        let chr_rom = banked_data(1024, 16); // Enough to satisfy mapper creation; not used in this test

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // PRG mode 0, set R6=$8000 bank 2, R7=$A000 bank 3
        mapper.write_prg(0x8000, 0b0000_0110); // bank select: register 6, PRG mode 0, CHR mode 0
        mapper.write_prg(0x8001, 2); // R6 = 2

        mapper.write_prg(0x8000, 0b0000_0111); // bank select: register 7
        mapper.write_prg(0x8001, 3); // R7 = 3

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // Switch to PRG mode 1, set R6=$C000 bank 1
        mapper.write_prg(0x8000, 0b0100_0110); // bank select: register 6, PRG mode 1
        mapper.write_prg(0x8001, 1); // R6 = 1

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc3_chr_bank_switching_mode0() {
        // MMC3 CHR banking (CHR mode 0):
        // - R0: 2KB bank @ $0000 (even-aligned)
        // - R1: 2KB bank @ $0800 (even-aligned)
        // - R2..R5: 1KB banks @ $1000, $1400, $1800, $1C00

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16); // 16 x 1KB banks

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Ensure CHR mode 0.
        mapper.write_prg(0x8000, 0b0000_0000); // bank select: register 0, CHR mode 0

        // R0 = 5 -> should map banks 4 and 5 at $0000-$07FF
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);

        // R1 = 2 -> should map banks 2 and 3 at $0800-$0FFF
        mapper.write_prg(0x8000, 0b0000_0001); // R1
        mapper.write_prg(0x8001, 2);
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0C00), 3);

        // R2..R5: 1KB banks
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 10);

        assert_eq!(mapper.read_chr(0x1000), 7);
        assert_eq!(mapper.read_chr(0x1400), 8);
        assert_eq!(mapper.read_chr(0x1800), 9);
        assert_eq!(mapper.read_chr(0x1C00), 10);
    }

    #[test]
    fn test_mmc3_prg_ram_enable_and_write_protect() {
        // MMC3 PRG-RAM control ($A001, odd address):
        // - bit 7: PRG-RAM enable (0 = disabled)
        // - bit 6: PRG-RAM write protect (1 = write-protected)
        // PRG-RAM is enabled by default on power-on (hardware behavior).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Default: PRG-RAM enabled (can read and write)
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable PRG-RAM: reads return 0, writes are ignored
        mapper.write_prg(0xA001, 0b0000_0000);
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Enable PRG-RAM and allow writes
        mapper.write_prg(0xA001, 0b1000_0000);
        mapper.write_prg(0x6000, 0xCC);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);

        // Enable + write-protect (writes ignored, reads still work)
        mapper.write_prg(0xA001, 0b1100_0000);
        mapper.write_prg(0x6000, 0xDD);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);
    }

    #[test]
    fn test_mmc3_prg_ram_disabled_returns_open_bus() {
        // MMC3 PRG-RAM disable (A001 bit 7 = 0) should return open bus, not 0.
        // Refs: https://www.nesdev.org/wiki/MMC3#PRG_RAM_protect_(-,_odd)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write a value to PRG-RAM while it's enabled
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable PRG-RAM
        mapper.write_prg(0xA001, 0b0000_0000);

        // read_prg should still return 0 for backward compatibility
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // read_prg_open_bus should return the open bus value
        let open_bus_value = 0x42;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus_value),
            open_bus_value
        );

        // Test with different open bus value
        let open_bus_value2 = 0xBD;
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus_value2),
            open_bus_value2
        );

        // Re-enable PRG-RAM - should read the original value (writes were ignored)
        mapper.write_prg(0xA001, 0b1000_0000);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn test_mmc3_chr_bank_switching_mode1() {
        // MMC3 CHR banking (CHR mode 1):
        // - R2..R5: 1KB banks @ $0000, $0400, $0800, $0C00
        // - R0: 2KB bank @ $1000 (even-aligned)
        // - R1: 2KB bank @ $1800 (even-aligned)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16); // 16 x 1KB banks

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Enable CHR mode 1 (bit7=1) and program registers.
        mapper.write_prg(0x8000, 0b1000_0000); // R0, CHR mode 1
        mapper.write_prg(0x8001, 5); // R0 -> banks 4+5 at $1000-$17FF

        mapper.write_prg(0x8000, 0b1000_0000 | 1); // R1
        mapper.write_prg(0x8001, 2); // R1 -> banks 2+3 at $1800-$1FFF

        mapper.write_prg(0x8000, 0b1000_0000 | 2); // R2
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b1000_0000 | 3); // R3
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b1000_0000 | 4); // R4
        mapper.write_prg(0x8001, 11);
        mapper.write_prg(0x8000, 0b1000_0000 | 5); // R5
        mapper.write_prg(0x8001, 12);

        // R2..R5 mapping at $0000-$0FFF
        assert_eq!(mapper.read_chr(0x0000), 9);
        assert_eq!(mapper.read_chr(0x0400), 10);
        assert_eq!(mapper.read_chr(0x0800), 11);
        assert_eq!(mapper.read_chr(0x0C00), 12);

        // R0 2KB mapping at $1000-$17FF (even aligned: 4 then 5)
        assert_eq!(mapper.read_chr(0x1000), 4);
        assert_eq!(mapper.read_chr(0x1400), 5);

        // R1 2KB mapping at $1800-$1FFF (even aligned: 2 then 3)
        assert_eq!(mapper.read_chr(0x1800), 2);
        assert_eq!(mapper.read_chr(0x1C00), 3);
    }

    #[test]
    fn test_mmc3_mirroring_control_via_a000() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Starts with the cartridge-provided mirroring.
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // $A000 even: mirroring control (bit 0)
        // 0 => Vertical
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // $A001 odd: PRG-RAM protect; must not affect mirroring
        mapper.write_prg(0xA001, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // 1 => Horizontal
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mmc3_chr_ram_writes_are_banked() {
        // If the cartridge has CHR-RAM (no CHR-ROM), writes should go to the currently mapped CHR bank.
        // Switching the bank should change what is visible at the same PPU address.

        let prg_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mmc3_mapper(prg_rom, vec![], NametableLayout::Horizontal);

        // CHR mode 0: $1000-$13FF uses R2 (1KB bank).
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 1); // map bank 1 at $1000

        mapper.write_chr(0x1000, 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xAA);
        // Switch to bank 2: should not see the value written into bank 1.
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 2); // map bank 2 at $1000
        assert_eq!(mapper.read_chr(0x1000), 0x00);

        // Switch back to bank 1: value should still be there.
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 1); // map bank 1 at $1000
        assert_eq!(mapper.read_chr(0x1000), 0xAA);
    }

    #[test]
    fn test_mmc3_chr_r1_even_aligned_when_odd_bank_written() {
        // R1 controls a 2KB CHR bank and must be even-aligned (odd values map the previous even bank).
        // In CHR mode 0, R1 maps to $0800-$0FFF (two 1KB banks).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Ensure CHR mode 0 and select R1.
        mapper.write_prg(0x8000, 0b0000_0001); // R1, CHR mode 0
        mapper.write_prg(0x8001, 7); // odd; should map banks 6 and 7

        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
    }

    #[test]
    fn test_mmc3_two_bank_prg_rom() {
        // Test MMC3 with only 2 x 8KB PRG banks (like the Blargg MMC3 test ROMs)
        // This is the minimum configuration and should work correctly.

        let mut prg_rom = vec![0u8; 16 * 1024]; // 16KB = 2 x 8KB banks
        // Fill bank 0 with 0xAA
        prg_rom[0..8192].fill(0xAA);
        // Fill bank 1 with 0xBB
        prg_rom[8192..16384].fill(0xBB);

        let chr_rom = vec![]; // No CHR-ROM (uses CHR-RAM)

        let mapper = create_mmc3_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // With 2 banks and default configuration (PRG mode 0):
        // $8000-$9FFF: R6 (bank 0) = 0xAA
        // $A000-$BFFF: R7 (bank 0) = 0xAA
        // $C000-$DFFF: fixed second-to-last (bank 0) = 0xAA
        // $E000-$FFFF: fixed last (bank 1) = 0xBB

        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xA000), 0xAA);
        assert_eq!(mapper.read_prg(0xC000), 0xAA);
        assert_eq!(mapper.read_prg(0xE000), 0xBB);

        // The reset vector at $FFFC-$FFFD should be readable from bank 1
        assert_eq!(mapper.read_prg(0xFFFC), 0xBB);
        assert_eq!(mapper.read_prg(0xFFFD), 0xBB);
    }

    #[test]
    fn test_mmc3_comprehensive_state_roundtrip() {
        // Test complete MMC3 state including banks, IRQ, and PRG-RAM
        let prg_rom = banked_data(8 * 1024, 32); // 256KB = 32 8KB banks
        let chr_rom = banked_data(1024, 128); // 128KB = 128 1KB banks

        let mut mapper = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
            false,
        );

        // Configure all 8 bank registers
        mapper.write_prg(0x8000, 0x00); // Select R0 (2KB CHR)
        mapper.write_prg(0x8001, 0x10); // R0 = bank 16
        mapper.write_prg(0x8000, 0x01); // Select R1 (2KB CHR)
        mapper.write_prg(0x8001, 0x20); // R1 = bank 32
        mapper.write_prg(0x8000, 0x02); // Select R2 (1KB CHR)
        mapper.write_prg(0x8001, 0x05); // R2 = bank 5
        mapper.write_prg(0x8000, 0x03); // Select R3 (1KB CHR)
        mapper.write_prg(0x8001, 0x06); // R3 = bank 6
        mapper.write_prg(0x8000, 0x04); // Select R4 (1KB CHR)
        mapper.write_prg(0x8001, 0x07); // R4 = bank 7
        mapper.write_prg(0x8000, 0x05); // Select R5 (1KB CHR)
        mapper.write_prg(0x8001, 0x08); // R5 = bank 8
        mapper.write_prg(0x8000, 0x06); // Select R6 (8KB PRG)
        mapper.write_prg(0x8001, 0x02); // R6 = bank 2
        mapper.write_prg(0x8000, 0x07); // Select R7 (8KB PRG)
        mapper.write_prg(0x8001, 0x03); // R7 = bank 3

        // Configure IRQ
        mapper.write_prg(0xC000, 10); // IRQ latch = 10
        mapper.write_prg(0xC001, 0); // Reload IRQ counter
        mapper.write_prg(0xE001, 0); // Enable IRQ

        // Set mirroring (bit 0: 0 = Vertical, 1 = Horizontal)
        mapper.write_prg(0xA000, 0x00); // Vertical mirroring

        // Configure PRG-RAM
        mapper.write_prg(0xA001, 0x80); // Enable PRG-RAM, no write protect
        mapper.write_prg(0x6000, 0xAA); // Write to PRG-RAM
        mapper.write_prg(0x7FFF, 0x55);

        // Verify state before snapshot
        assert_eq!(mapper.read_prg(0x8000), 2); // R6 bank
        assert_eq!(mapper.read_prg(0xA000), 3); // R7 bank
        assert_eq!(mapper.read_chr(0x0000), 16); // R0 bank (even 2KB)
        assert_eq!(mapper.read_chr(0x0400), 17); // R0 bank + 1 (odd 2KB)
        assert_eq!(mapper.read_prg(0x6000), 0xAA); // PRG-RAM
        assert_eq!(mapper.read_prg(0x7FFF), 0x55);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Take snapshot
        let registers = mapper.registers_snapshot();
        let prg_ram = mapper.wram_snapshot();

        // Create fresh mapper and restore
        let mut restored =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Horizontal, false);
        restored.restore_registers(&registers);
        restored.load_wram_snapshot(&prg_ram);

        // Verify all state is restored
        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_prg(0xA000), 3);
        assert_eq!(restored.read_chr(0x0000), 16);
        assert_eq!(restored.read_chr(0x0400), 17);
        assert_eq!(restored.read_prg(0x6000), 0xAA);
        assert_eq!(restored.read_prg(0x7FFF), 0x55);
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_mmc3_irq_state_roundtrip() {
        // Test that IRQ counter and state are preserved across save/load
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
            false,
        );

        // Configure IRQ
        mapper.write_prg(0xC000, 5); // Latch = 5
        mapper.write_prg(0xC001, 0); // Reload
        mapper.write_prg(0xE001, 0); // Enable IRQ

        // Set up A12 low state
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }

        // Trigger A12 rising edge to reload counter
        mapper.ppu_address_changed(0x1000);

        // Counter should now be at latch value (5)
        assert_eq!(mapper.irq_counter(), 5);
        assert!(!mapper.irq_pending());

        // Take snapshot with counter at 5
        let registers = mapper.registers_snapshot();

        // Create fresh mapper and restore
        let mut restored =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Horizontal, false);
        restored.restore_registers(&registers);

        // Verify IRQ state is preserved
        assert_eq!(restored.irq_counter(), 5);
        assert!(!restored.irq_pending());

        // Continue decrementing on restored mapper
        restored.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            restored.cpu_cycle();
        }
        restored.ppu_address_changed(0x1000); // Counter: 5 -> 4

        assert_eq!(restored.irq_counter(), 4);
    }

    #[test]
    fn test_mmc3_mirroring_modes_roundtrip() {
        // Test that FourScreen and SingleScreen mirroring modes are preserved across save/load
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        // Test FourScreen mirroring
        let mut mapper_fourscreen = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::FourScreen,
            false,
        );

        // Configure some state to make the test more realistic
        mapper_fourscreen.write_prg(0x8000, 0x00); // Select R0
        mapper_fourscreen.write_prg(0x8001, 0x05); // R0 = bank 5

        // FourScreen mode should be preserved even without explicit write (it's set at construction)
        assert_eq!(
            mapper_fourscreen.get_mirroring(),
            NametableLayout::FourScreen
        );

        // Take snapshot
        let registers_fourscreen = mapper_fourscreen.registers_snapshot();

        // Restore to fresh mapper (initially Horizontal) and verify FourScreen is restored
        let mut restored_fourscreen = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
            false,
        );
        restored_fourscreen.restore_registers(&registers_fourscreen);

        assert_eq!(
            restored_fourscreen.get_mirroring(),
            NametableLayout::FourScreen,
            "FourScreen mirroring should be preserved across save/load"
        );

        // Test SingleScreen mirroring
        let mut mapper_singlescreen = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::SingleScreen,
            false,
        );

        // Configure some state
        mapper_singlescreen.write_prg(0x8000, 0x01); // Select R1
        mapper_singlescreen.write_prg(0x8001, 0x08); // R1 = bank 8

        assert_eq!(
            mapper_singlescreen.get_mirroring(),
            NametableLayout::SingleScreen
        );

        // Take snapshot
        let registers_singlescreen = mapper_singlescreen.registers_snapshot();

        // Restore to fresh mapper and verify SingleScreen is restored
        let mut restored_singlescreen =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Vertical, false);
        restored_singlescreen.restore_registers(&registers_singlescreen);

        assert_eq!(
            restored_singlescreen.get_mirroring(),
            NametableLayout::SingleScreen,
            "SingleScreen mirroring should be preserved across save/load"
        );
    }

    #[test]
    fn test_mmc3_builder_pattern_with_irq_mode() {
        // Test builder pattern for alternate IRQ mode
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);

        // Create mapper with default (Sharp) IRQ behavior
        let mapper_default = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
            false,
        );
        assert!(!mapper_default.use_alternate_irq);

        // Create mapper with alternate (NEC) IRQ behavior using builder pattern
        let mapper_alternate = MMC3Mapper::new_with_irq_mode(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
            false,
        )
        .with_irq_mode(true);
        assert!(mapper_alternate.use_alternate_irq);

        // Verify new_with_irq_mode still works
        let mapper_explicit =
            MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, NametableLayout::Horizontal, true);
        assert!(mapper_explicit.use_alternate_irq);
    }

    /// Test MMC3 enabled PRG-RAM doesn't return open-bus
    #[test]
    fn test_mmc3_enabled_prg_ram_returns_data_not_open_bus() {
        let mut mapper = MMC3Mapper::new_with_irq_mode(
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
            false,
        );

        // Enable PRG-RAM (bit 7 = 1)
        mapper.write_prg(0xA001, 0b1000_0000);

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0x99);

        let open_bus = 0xFF;
        let result = mapper.read_prg_open_bus(0x6000, open_bus);

        // Should return the written value, not open-bus
        assert_eq!(
            result, 0x99,
            "Enabled PRG-RAM should return written data, not open-bus"
        );
    }
}

// ============================================================================
// CPU & PPU I/O (Mapper Trait Implementation)
// ============================================================================

impl Mapper for MMC3Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.mmc6 => self.mmc6_read(addr).unwrap_or(0),
            0x6000..=0x7FFF => {
                if self.prg_ram.is_empty() || !self.prg_ram_enabled {
                    return 0;
                }
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.mmc6 => self.mmc6_read(addr).unwrap_or(open_bus),
            0x6000..=0x7FFF => {
                if self.prg_ram.is_empty() || !self.prg_ram_enabled {
                    return open_bus;
                }
                self.read_prg(addr)
            }
            _ => {
                if addr < 0x6000 {
                    open_bus
                } else {
                    self.read_prg(addr)
                }
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF if self.mmc6 => self.mmc6_write(addr, value),
            0x6000..=0x7FFF => {
                if self.prg_ram.is_empty() || !self.prg_ram_enabled || self.prg_ram_write_protected
                {
                    return;
                }
                let offset = (addr - 0x6000) as usize;
                if let Some(byte) = self.prg_ram.get_mut(offset) {
                    *byte = value;
                }
            }
            0x8000..=0x9FFF => {
                if (addr & 1) == 0 {
                    // Bank select
                    trace_mapper!(1; "MMC3 bank_select=${:02X}", value);
                    self.bank_select = value;
                    if self.mmc6 && !self.mmc6_ram_enabled() {
                        // PRG-RAM disabled: the MMC6 holds $A001 at $00.
                        self.mmc6_protect = 0;
                    }
                } else {
                    // Bank data
                    let reg = self.selected_reg();
                    trace_mapper!(1; "MMC3 reg[{}]=${:02X}", reg, value);
                    self.regs[reg] = value;
                }
                self.update_banks();
            }
            0xA000..=0xBFFF => {
                if (addr & 1) == 0 {
                    // Mirroring
                    // MMC3: bit0 selects mirroring.
                    self.base.set_mirroring_hv((value & 1) != 0);
                    trace_mapper!(1; "MMC3 mirroring={:?}", self.base.mirroring());
                } else {
                    if self.mmc6 {
                        // MMC6 PRG RAM protect: HhLl xxxx, ignored while
                        // $8000 bit 5 has PRG-RAM disabled.
                        if self.mmc6_ram_enabled() {
                            self.mmc6_protect = value & 0xF0;
                        }
                    } else {
                        // PRG RAM protect
                        // - bit 7: PRG-RAM enable
                        // - bit 6: PRG-RAM write protect
                        self.update_prg_ram_control(value);
                    }
                }
            }
            0xC000..=0xDFFF => {
                if (addr & 1) == 0 {
                    // IRQ latch
                    trace_mapper!(1; "MMC3 IRQ_latch=${:02X}", value);
                    self.irq_latch = value;
                } else {
                    // IRQ reload
                    trace_mapper!(1; "MMC3 IRQ_reload");
                    self.irq_counter = 0;
                    self.irq_reload = true;
                    // MC-ACC: "Writing to $C001 resets pulse counter" (krikzz, via Mesen2).
                    self.mc_acc_pulses = 0;
                }
            }
            0xE000..=0xFFFF => {
                if (addr & 1) == 0 {
                    // IRQ disable + acknowledge
                    trace_mapper!(1; "MMC3 IRQ_disable");
                    self.irq_enabled = false;
                    self.irq_asserted = false;
                } else {
                    // IRQ enable
                    trace_mapper!(1; "MMC3 IRQ_enable");
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }

    fn ppu_nametable_address(&mut self, addr: u16) {
        // Only MC-ACC needs these: the sprite slots' garbage nametable fetches are where
        // A12 falls between their pattern fetches. The MMC3's filter ignores them.
        if self.mc_acc {
            self.mc_acc_a12(addr);
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.mc_acc {
            self.mc_acc_a12(addr);
            return;
        }
        if self.a12_detector.update(addr) {
            trace_mapper!(5; "MMC3 A12 rising edge detected: addr=${:04X}", addr);
            self.clock_irq_counter();
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12_detector.cpu_tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        // Return a direct copy of PRG-RAM, bypassing enable/protect state
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        // Write directly to PRG-RAM, bypassing enable/protect state
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        crate::nes::console::initialize_ram(&mut self.prg_ram, mode);
        self.base.initialize_ram(mode);
    }

    // ============================================================================
    // Save State Management
    // ============================================================================

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize MMC3 internal registers:
        // [0]: bank_select
        // [1-8]: regs[0-7]
        // [9]: irq_latch
        // [10]: irq_counter
        // [11]: flags (irq_reload, irq_enabled, irq_asserted, prg_ram_enabled, prg_ram_write_protected)
        // [12]: mirroring mode
        // [13]: prev_a12 (from A12 detector)
        // [14]: current_a12 (from A12 detector)
        // [15]: a12_low_cycles (from A12 detector)
        let mut snapshot = Vec::with_capacity(16);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        snapshot.push(self.irq_latch);
        snapshot.push(self.irq_counter);
        let flags = (self.irq_reload as u8)
            | ((self.irq_enabled as u8) << 1)
            | ((self.irq_asserted as u8) << 2)
            | ((self.prg_ram_enabled as u8) << 3)
            | ((self.prg_ram_write_protected as u8) << 4);
        snapshot.push(flags);
        snapshot.push(match self.base.mirroring() {
            NametableLayout::Vertical => 0,
            NametableLayout::Horizontal => 1,
            NametableLayout::FourScreen => 2,
            NametableLayout::SingleScreen => 3,
            _ => 1,
        });
        snapshot.push(self.a12_detector.prev_a12() as u8);
        snapshot.push(self.a12_detector.current_a12() as u8);
        snapshot.push(self.a12_detector.a12_low_cycles());
        if self.mmc6 {
            // [16]: MMC6 $A001
            snapshot.push(self.mmc6_protect);
        }
        if self.mc_acc {
            // [16]: MC-ACC prescaler position, [17]: MC-ACC last A12
            snapshot.push(self.mc_acc_pulses);
            snapshot.push(self.mc_acc_prev_a12 as u8);
        }
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 13 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.irq_latch = data[9];
            self.irq_counter = data[10];
            let flags = data[11];
            self.irq_reload = (flags & 1) != 0;
            self.irq_enabled = (flags & 2) != 0;
            self.irq_asserted = (flags & 4) != 0;
            self.prg_ram_enabled = (flags & 8) != 0;
            self.prg_ram_write_protected = (flags & 16) != 0;
            self.base.set_mirroring(match data[12] {
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                2 => NametableLayout::FourScreen,
                3 => NametableLayout::SingleScreen,
                _ => NametableLayout::Horizontal,
            });
        }

        if data.len() >= 16 {
            self.a12_detector.set_prev_a12(data[13] != 0);
            self.a12_detector.set_current_a12(data[14] != 0);
            self.a12_detector.set_a12_low_cycles(data[15]);
        }

        if self.mmc6 {
            self.mmc6_protect = data.get(16).copied().unwrap_or(0) & 0xF0;
        }
        if self.mc_acc {
            self.mc_acc_pulses = data.get(16).copied().unwrap_or(0) % Self::MC_ACC_PRESCALER;
            self.mc_acc_prev_a12 = data.get(17).copied().unwrap_or(0) != 0;
        }

        self.update_banks();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: self.prg_ram.len() / 1024,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}
