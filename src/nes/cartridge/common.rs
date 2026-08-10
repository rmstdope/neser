//! Common utilities for NES mapper implementations.
//!
//! This module provides reusable components that are shared across multiple mappers,
//! reducing code duplication and ensuring consistent behavior.

/// Trait for consistent state snapshot and restoration.
///
/// This trait provides a standard interface for capturing and restoring mapper state,
/// making it easier to implement save states and test state preservation.
///
/// # When to Implement
///
/// Implement `StateSnapshot` for:
/// - Mapper register structures that need to be preserved across save states
/// - Any component with internal state that affects emulation behavior
/// - Register banks, shift registers, counters, and other stateful components
///
/// # Design Guidelines
///
/// - Keep snapshots compact but complete
/// - Use deterministic serialization (no HashMap iteration, etc.)
/// - Version your snapshot format if it may change
/// - Document the byte layout in implementation
/// - Handle gracefully if restore data is incomplete/invalid
///
/// # Examples
///
/// ## Simple Register Set
///
/// ```rust
/// use neser::nes::cartridge::StateSnapshot;
///
/// struct ShiftRegister {
///     value: u8,
///     count: u8,
/// }
///
/// impl StateSnapshot for ShiftRegister {
///     fn snapshot(&self) -> Vec<u8> {
///         // Layout: [value, count]
///         vec![self.value, self.count]
///     }
///
///     fn restore(&mut self, data: &[u8]) {
///         if data.len() >= 2 {
///             self.value = data[0];
///             self.count = data[1];
///         }
///     }
/// }
/// ```
///
/// ## Composite Register Structure
///
/// ```rust
/// use neser::nes::cartridge::StateSnapshot;
///
/// struct BankRegisters {
///     prg_bank: u8,
///     chr_banks: [u8; 8],
///     mirroring: u8,
/// }
///
/// impl StateSnapshot for BankRegisters {
///     fn snapshot(&self) -> Vec<u8> {
///         // Layout: [prg_bank, chr_banks[0..8], mirroring]
///         let mut data = Vec::with_capacity(10);
///         data.push(self.prg_bank);
///         data.extend_from_slice(&self.chr_banks);
///         data.push(self.mirroring);
///         data
///     }
///
///     fn restore(&mut self, data: &[u8]) {
///         if let Some(&prg) = data.get(0) {
///             self.prg_bank = prg;
///         }
///         if data.len() >= 9 {
///             self.chr_banks.copy_from_slice(&data[1..9]);
///         }
///         if let Some(&mir) = data.get(9) {
///             self.mirroring = mir;
///         }
///     }
/// }
/// ```
///
/// ## Integration with `Mapper::registers_snapshot()`
///
/// ```rust,ignore
/// use neser::nes::cartridge::{Mapper, StateSnapshot};
///
/// struct MyMapper {
///     registers: MyRegisterSet,
///     // ... other fields
/// }
///
/// impl Mapper for MyMapper {
///     fn registers_snapshot(&self) -> Vec<u8> {
///         self.registers.snapshot()
///     }
///
///     fn restore_registers(&mut self, data: &[u8]) {
///         self.registers.restore(data);
///     }
///     // ... other trait methods
/// }
/// ```
#[allow(dead_code)]
pub trait StateSnapshot {
    /// Create a snapshot of the current state.
    ///
    /// Returns a byte vector containing all state needed to restore this component.
    /// The format is implementation-defined but should be documented.
    fn snapshot(&self) -> Vec<u8>;

    /// Restore state from a snapshot.
    ///
    /// Loads state from a byte slice previously created by `snapshot()`.
    /// If the data is incomplete or invalid, implementations should:
    /// - Restore as much as possible
    /// - Leave unrestorable fields at safe default values
    /// - Not panic (use bounds checking, Option::unwrap_or, etc.)
    fn restore(&mut self, data: &[u8]);
}

/// Standard PRG-RAM size (8KB)
pub const DEFAULT_PRG_RAM_SIZE: usize = 8192;

/// Standard CHR-RAM size (8KB)
pub const DEFAULT_CHR_RAM_SIZE: usize = 8192;

/// PRG-RAM helper for mappers with battery-backed work RAM.
///
/// Handles:
/// - Read/write access at $6000-$7FFF
/// - WRAM snapshot/restore for save persistence
///
/// # Example
/// ```rust
/// use neser::nes::cartridge::PrgRam;
///
/// struct MyMapper {
///     prg_ram: PrgRam,
///     // ...
/// }
///
/// impl MyMapper {
///     fn new() -> Self {
///         Self {
///             prg_ram: PrgRam::new(8192),
///         }
///     }
///
///     // In Mapper trait impl:
///     fn read_prg(&self, addr: u16) -> u8 {
///         if let Some(value) = self.prg_ram.try_read(addr) {
///             return value;
///         }
///         0
///     }
/// }
/// ```
#[derive(Clone)]
pub struct PrgRam {
    data: Vec<u8>,
}

impl PrgRam {
    /// Create a new PRG-RAM with the given size (typically 8KB).
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }

    /// Try to read from PRG-RAM if address is in $6000-$7FFF range.
    /// Returns None if address is outside PRG-RAM range.
    #[inline]
    pub fn try_read(&self, addr: u16) -> Option<u8> {
        if (0x6000..=0x7FFF).contains(&addr) {
            let offset = (addr - 0x6000) as usize;
            Some(self.data.get(offset).copied().unwrap_or(0))
        } else {
            None
        }
    }

    /// Try to write to PRG-RAM if address is in $6000-$7FFF range.
    /// Returns true if the write was handled, false if address is outside range.
    #[inline]
    pub fn try_write(&mut self, addr: u16, value: u8) -> bool {
        if (0x6000..=0x7FFF).contains(&addr) {
            let offset = (addr - 0x6000) as usize;
            if offset < self.data.len() {
                self.data[offset] = value;
            }
            true
        } else {
            false
        }
    }

    /// Get the size of PRG-RAM in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Create a snapshot of PRG-RAM for save persistence.
    pub fn snapshot(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Read a byte from PRG-RAM at an absolute byte offset (not CPU address).
    /// Used for banked RAM access where the caller computes `bank * bank_size + page_offset`.
    /// Returns 0 (open bus) if `offset` is beyond the allocated RAM size.
    #[inline]
    pub fn read_at_offset(&self, offset: usize) -> u8 {
        self.data.get(offset).copied().unwrap_or(0)
    }

    /// Write a byte to PRG-RAM at an absolute byte offset (not CPU address).
    /// No-op if the offset is beyond the allocated RAM size.
    #[inline]
    pub fn write_at_offset(&mut self, offset: usize, value: u8) {
        if offset < self.data.len() {
            self.data[offset] = value;
        }
    }

    /// Load a snapshot into PRG-RAM from save persistence.
    pub fn load_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.data.len());
        self.data[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    /// Re-initialize PRG-RAM contents based on the given mode.
    ///
    /// This should be called on cartridge insertion or hard reset.
    /// Soft resets should NOT call this (RAM contents persist).
    pub fn initialize(&mut self, mode: crate::nes::console::RamInitMode) {
        crate::nes::console::initialize_ram(&mut self.data, mode);
    }
}

impl StateSnapshot for PrgRam {
    fn snapshot(&self) -> Vec<u8> {
        // Explicitly call the inherent method to avoid infinite recursion.
        // Without type qualification, `self.snapshot()` would recursively call
        // this trait method instead of the inherent `PrgRam::snapshot()`.
        PrgRam::snapshot(self)
    }

    fn restore(&mut self, data: &[u8]) {
        self.load_snapshot(data);
    }
}

/// CHR memory helper that handles both CHR-ROM and CHR-RAM.
///
/// Automatically switches between ROM (read-only) and RAM (read-write)
/// based on whether CHR-ROM data was provided at construction.
#[derive(Clone)]
pub struct ChrMemory {
    data: Vec<u8>,
    is_ram: bool,
}

impl ChrMemory {
    /// Create CHR memory from ROM data.
    /// If chr_rom is empty, allocates CHR-RAM instead.
    pub fn new(chr_rom: Vec<u8>) -> Self {
        if chr_rom.is_empty() {
            Self {
                data: vec![0; DEFAULT_CHR_RAM_SIZE],
                is_ram: true,
            }
        } else {
            Self {
                data: chr_rom,
                is_ram: false,
            }
        }
    }

    /// Create CHR-RAM with a specific size.
    pub fn new_ram(size: usize) -> Self {
        Self {
            data: vec![0; size],
            is_ram: true,
        }
    }

    /// Read a byte from CHR memory (8KB window at $0000-$1FFF).
    #[inline]
    pub fn read(&self, addr: u16) -> u8 {
        let index = (addr & 0x1FFF) as usize;
        self.data.get(index).copied().unwrap_or(0)
    }

    /// Write a byte to CHR memory. Only succeeds for CHR-RAM.
    #[inline]
    pub fn write(&mut self, addr: u16, value: u8) {
        if self.is_ram {
            let index = (addr & 0x1FFF) as usize;
            if index < self.data.len() {
                self.data[index] = value;
            }
        }
    }

    /// Check if this is CHR-RAM (writable) vs CHR-ROM (read-only).
    #[cfg(test)]
    #[inline]
    pub fn is_ram(&self) -> bool {
        self.is_ram
    }

    /// Get the total size of CHR memory.
    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Read a byte from CHR memory at a specific index (for banked CHR).
    #[inline]
    pub fn read_at_index(&self, index: usize) -> u8 {
        self.data.get(index).copied().unwrap_or(0)
    }

    /// Write a byte to CHR memory at a specific index (for banked CHR).
    #[inline]
    pub fn write_at_index(&mut self, index: usize, value: u8) {
        if self.is_ram && index < self.data.len() {
            self.data[index] = value;
        }
    }

    /// Create a snapshot of CHR-RAM for save-state persistence.
    pub fn snapshot(&self) -> Vec<u8> {
        if self.is_ram {
            self.data.clone()
        } else {
            Vec::new()
        }
    }

    /// Load a snapshot into CHR-RAM from save-state persistence.
    pub fn load_snapshot(&mut self, data: &[u8]) {
        if !self.is_ram {
            return;
        }

        let to_copy = data.len().min(self.data.len());
        self.data[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    /// Re-initialize CHR-RAM contents based on the given mode.
    ///
    /// This should be called on cartridge insertion or hard reset.
    /// Only applies to CHR-RAM (CHR-ROM is read-only and ignored).
    /// Soft resets should NOT call this (RAM contents persist).
    pub fn initialize(&mut self, mode: crate::nes::console::RamInitMode) {
        if self.is_ram {
            crate::nes::console::initialize_ram(&mut self.data, mode);
        }
    }
}

impl StateSnapshot for ChrMemory {
    fn snapshot(&self) -> Vec<u8> {
        // Explicitly call the inherent method to avoid infinite recursion.
        // Without type qualification, `self.snapshot()` would recursively call
        // this trait method instead of the inherent `ChrMemory::snapshot()`.
        ChrMemory::snapshot(self)
    }

    fn restore(&mut self, data: &[u8]) {
        self.load_snapshot(data);
    }
}

/// Banked ROM helper for PRG and CHR bank switching.
///
/// Centralizes common bank calculation logic used across mappers:
/// - Automatic bank wrapping based on ROM size
/// - Bank offset calculation
/// - Bounds checking
///
/// # Example
/// ```ignore
/// use neser::nes::cartridge::BankedRom;
///
/// let prg_rom = vec![0u8; 0x8000];
/// let bank = 3usize;
///
/// // Create a helper for 16KB PRG banks
/// let banked_prg = BankedRom::new(prg_rom, 0x4000);
///
/// // Read from bank 3, offset 0x1000
/// let value = banked_prg.read(3, 0x1000);
///
/// // Or use with address calculation
/// let value = banked_prg.read_with_base(bank, 0x8000, 0x9000);
/// let _ = value;
/// ```
#[cfg(test)]
#[derive(Clone)]
pub struct BankedRom {
    data: Vec<u8>,
    bank_size: usize,
}

#[cfg(test)]
impl BankedRom {
    /// Create a new banked ROM with the specified bank size.
    ///
    /// # Arguments
    /// * `data` - The ROM data
    /// * `bank_size` - Size of each bank in bytes (e.g., 0x4000 for 16KB banks)
    pub fn new(data: Vec<u8>, bank_size: usize) -> Self {
        Self { data, bank_size }
    }

    /// Get the number of banks available.
    #[inline]
    pub fn num_banks(&self) -> usize {
        if self.bank_size == 0 || self.data.is_empty() {
            return 0;
        }
        self.data.len() / self.bank_size
    }

    /// Read a byte from a specific bank and offset.
    ///
    /// # Arguments
    /// * `bank` - Bank number (automatically wraps based on available banks)
    /// * `offset` - Offset within the bank (0 to bank_size-1)
    ///
    /// # Returns
    /// The byte at the specified location, or 0 if out of bounds.
    #[inline]
    pub fn read(&self, bank: usize, offset: usize) -> u8 {
        let num_banks = self.num_banks();
        if num_banks == 0 {
            return 0;
        }

        // Wrap bank number to available banks
        let bank = bank % num_banks;

        // Calculate absolute index
        let index = bank * self.bank_size + offset;

        // Return byte or 0 if out of bounds
        self.data.get(index).copied().unwrap_or(0)
    }

    /// Read a byte using a base address calculation.
    ///
    /// This is a convenience method for typical mapper usage where you have
    /// an address and need to read from a specific bank.
    ///
    /// # Arguments
    /// * `bank` - Bank number (automatically wraps)
    /// * `base_addr` - Base address for the bank (e.g., 0x8000)
    /// * `addr` - The address being read
    ///
    /// # Returns
    /// The byte at the calculated offset, or 0 if out of bounds.
    #[inline]
    pub fn read_with_base(&self, bank: usize, base_addr: u16, addr: u16) -> u8 {
        let offset = addr.wrapping_sub(base_addr) as usize;
        self.read(bank, offset)
    }

    // /// Get the total size of the ROM data.
    // #[inlines]
    // pub fn size(&self) -> usize {
    //     self.data.len()
    // }

    // /// Get the bank size.
    // #[inline]
    // pub fn bank_size(&self) -> usize {
    //     self.bank_size
    // }
}

/// Helper for bank-switching logic used across mappers.
///
/// Encapsulates common patterns for bank selection, wrapping, and offset calculation.
/// Eliminates manual `.max(1)` calls and reduces duplicated bank calculation code.
///
/// # Example
/// ```ignore
/// use neser::nes::cartridge::BankSwitch;
///
/// // PRG-ROM with 128KB (4 banks of 32KB)
/// let prg_bank = BankSwitch::new(4);
///
/// // Switch to bank 2
/// let mut bank = prg_bank;
/// bank.set(2);
/// assert_eq!(bank.current(), 2);
/// assert_eq!(bank.offset(0x8000), 0x10000); // 2 * 32KB
///
/// // Bank wraps when exceeding available banks
/// bank.set(5);
/// assert_eq!(bank.current(), 1); // 5 % 4 = 1
/// ```
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct BankSwitch {
    num_banks: usize,
    bank: u8,
}

#[cfg(test)]
impl BankSwitch {
    /// Create a new bank switch helper.
    ///
    /// # Arguments
    /// * `num_banks` - Total number of banks available (0 for empty ROM)
    pub fn new(num_banks: usize) -> Self {
        Self { num_banks, bank: 0 }
    }

    /// Create a new bank switch from ROM data and bank size.
    ///
    /// # Arguments
    /// * `rom_data` - The ROM data
    /// * `bank_size` - Size of each bank in bytes
    ///
    /// # Returns
    /// A BankSwitch configured with the appropriate number of banks
    pub fn from_rom(rom_data: &[u8], bank_size: usize) -> Self {
        let num_banks = if rom_data.is_empty() || bank_size == 0 {
            0
        } else {
            rom_data.len() / bank_size
        };
        Self::new(num_banks)
    }

    /// Set the selected bank number.
    ///
    /// The bank value is stored as-is and wrapping is applied during `current()`.
    pub fn set(&mut self, value: u8) {
        self.bank = value;
    }

    /// Get the current bank index with wrapping applied.
    ///
    /// Returns the bank number modulo the available banks, with special
    /// handling for empty ROM (returns 0).
    pub fn current(&self) -> usize {
        if self.num_banks == 0 {
            0
        } else {
            (self.bank as usize) % self.num_banks
        }
    }

    /// Calculate the byte offset for the current bank.
    ///
    /// # Arguments
    /// * `bank_size` - Size of each bank in bytes
    ///
    /// # Returns
    /// The offset into ROM data for the current bank
    #[cfg(test)]
    pub fn offset(&self, bank_size: usize) -> usize {
        self.current() * bank_size
    }

    /// Get the raw bank value without wrapping.
    pub fn raw(&self) -> u8 {
        self.bank
    }
}

#[cfg(test)]
impl StateSnapshot for BankSwitch {
    fn snapshot(&self) -> Vec<u8> {
        vec![self.bank]
    }

    fn restore(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.bank = value;
        }
    }
}

// ============================================================================
// A12 (Scanline) IRQ Counter
// ============================================================================
//
// Used by MMC3-family mappers to generate scanline-based IRQs.
//
// The counter is clocked by PPU address bus A12 rising edges:
// 1. PPU renders a scanline → A12 transitions low→high as it accesses pattern tables
// 2. A12 must be low for at least 3 CPU cycles (debounce/low-pass filter)
// 3. On each valid rising edge, the counter is clocked
// 4. Clock behavior: if counter==0 or reload requested → load from latch, else decrement
// 5. IRQ fires when counter reaches 0 (exact behavior depends on variant)
//
// Two chip variants:
// - Normal (Sharp): IRQ when counter IS 0 after update
// - Alternate (NEC): IRQ only on 1→0 TRANSITION (not reload-to-0 from natural wrap)
//
// Register interface (caller is responsible for mapping addresses):
// - set_latch(value): sets the reload value
// - request_reload(): clears counter to 0, sets reload flag
// - set_enabled(true/false): enables/disables IRQ generation; disable also acknowledges
// - cpu_cycle(): tracks A12 low cycles for debounce
// - ppu_address_changed(addr): detects A12 rising edges and clocks counter
// - is_pending(): returns true if IRQ is asserted
//
// See: https://www.nesdev.org/wiki/MMC3#IRQ_Specifics

/// Reusable A12 (scanline) IRQ counter for MMC3-family mappers.
#[allow(dead_code)]
pub struct A12IrqCounter {
    latch: u8,
    counter: u8,
    reload: bool,
    enabled: bool,
    asserted: bool,
    a12_detector: A12RisingEdgeDetector,
    alternate_behavior: bool,
}

#[allow(dead_code)]
impl A12IrqCounter {
    /// Create a new A12 IRQ counter.
    ///
    /// `alternate_behavior`:
    /// - `false`: Normal (Sharp) — IRQ when counter IS 0 after update
    /// - `true`: Alternate (NEC) — IRQ only on 1→0 transition
    pub fn new(alternate_behavior: bool) -> Self {
        Self {
            latch: 0,
            counter: 0,
            reload: false,
            enabled: false,
            asserted: false,
            a12_detector: A12RisingEdgeDetector::new(3),
            alternate_behavior,
        }
    }

    /// Set the IRQ latch (reload value).
    pub fn set_latch(&mut self, value: u8) {
        self.latch = value;
    }

    /// Request counter reload: clears counter to 0 and sets reload flag.
    pub fn request_reload(&mut self) {
        self.counter = 0;
        self.reload = true;
    }

    /// Enable or disable IRQ generation. Disabling also acknowledges any pending IRQ.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.asserted = false;
        }
    }

    /// Returns true if an IRQ is currently asserted (pending).
    pub fn is_pending(&self) -> bool {
        self.asserted
    }

    /// Track A12 low cycles for debounce. Call once per CPU cycle.
    pub fn cpu_cycle(&mut self) {
        self.a12_detector.cpu_tick();
    }

    /// Notify the counter that the PPU address bus changed.
    /// Detects A12 rising edges and clocks the counter when appropriate.
    pub fn ppu_address_changed(&mut self, addr: u16) {
        if self.a12_detector.update(addr) {
            self.clock_counter();
        }
    }

    fn clock_counter(&mut self) {
        let old_counter = self.counter;
        let was_reload = self.reload;

        if self.counter == 0 || self.reload {
            self.counter = self.latch;
            self.reload = false;
        } else {
            self.counter = self.counter.wrapping_sub(1);
        }

        let should_fire = if self.alternate_behavior {
            // Alternate (NEC): IRQ only on 1→0 transition
            let decremented_to_zero = old_counter == 1 && self.counter == 0;
            let reload_triggered_to_zero = was_reload && self.counter == 0;
            decremented_to_zero || reload_triggered_to_zero
        } else {
            // Normal (Sharp): IRQ when counter is 0
            self.counter == 0
        };

        if should_fire && self.enabled {
            self.asserted = true;
        }
    }

    /// Read the current counter value (for testing/debugging).
    #[cfg(test)]
    pub fn counter(&self) -> u8 {
        self.counter
    }
}

// ============================================================================
// A12 Rising Edge Detector
// ============================================================================
//
// Reusable PPU A12 rising edge detector with configurable debounce.
// Used by MMC3-family mappers (mapper48, mapper64, etc.) and SuperMagicCard.
//
// The PPU address bus bit 12 (A12) toggles between 0 and 1 as the PPU
// fetches pattern table tiles. A rising edge (0→1 transition) that stays
// low for a minimum number of CPU cycles indicates a new scanline.
//
// The debounce threshold filters out rapid A12 toggling during pattern
// table fetches within a single scanline:
// - threshold=3: standard MMC3-family debounce (3 CPU cycles low)
// - threshold=0: no debounce (every rising edge counts)
//
// See: https://www.nesdev.org/wiki/MMC3#IRQ_Specifics

pub struct A12RisingEdgeDetector {
    prev_a12: bool,
    current_a12: bool,
    a12_low_cycles: u8,
    threshold: u8,
}

impl A12RisingEdgeDetector {
    /// Create a new A12 rising edge detector with the given debounce threshold.
    ///
    /// `threshold`: minimum number of CPU cycles A12 must be low before a
    /// rising edge is considered valid. Use 3 for standard MMC3-family
    /// debounce, or 0 for no debounce.
    pub fn new(threshold: u8) -> Self {
        Self {
            prev_a12: false,
            current_a12: false,
            a12_low_cycles: 0,
            threshold,
        }
    }

    /// Notify the detector that the PPU address bus changed.
    /// Returns `true` if a valid A12 rising edge was detected.
    pub fn update(&mut self, addr: u16) -> bool {
        let a12 = (addr & 0x1000) != 0;
        self.current_a12 = a12;
        let rising = a12 && !self.prev_a12;
        self.prev_a12 = a12;
        rising && self.a12_low_cycles >= self.threshold
    }

    /// Track A12 low cycles. Call once per CPU cycle.
    pub fn cpu_tick(&mut self) {
        if self.current_a12 {
            self.a12_low_cycles = 0;
        } else {
            self.a12_low_cycles = self.a12_low_cycles.saturating_add(1);
        }
    }

    /// Get the previous A12 state (for snapshot serialization).
    pub fn prev_a12(&self) -> bool {
        self.prev_a12
    }

    /// Get the current A12 state (for snapshot serialization).
    pub fn current_a12(&self) -> bool {
        self.current_a12
    }

    /// Get the A12 low cycle count (for snapshot serialization).
    pub fn a12_low_cycles(&self) -> u8 {
        self.a12_low_cycles
    }

    /// Set the previous A12 state (for snapshot restoration).
    pub fn set_prev_a12(&mut self, value: bool) {
        self.prev_a12 = value;
    }

    /// Set the current A12 state (for snapshot restoration).
    pub fn set_current_a12(&mut self, value: bool) {
        self.current_a12 = value;
    }

    /// Set the A12 low cycle count (for snapshot restoration).
    pub fn set_a12_low_cycles(&mut self, value: u8) {
        self.a12_low_cycles = value;
    }
}

// ============================================================================
// CPU Cycle IRQ Counter
// ============================================================================
//
// Simple countdown IRQ counter clocked by CPU cycles. Used by mappers like
// 65 (Irem H3001) which have a 16-bit counter that counts down each CPU cycle.
//
// Behavior:
// - Counter counts down each CPU cycle when enabled
// - On reaching 0: fires IRQ, counter stays at 0 (no wrap)
// - Separate 16-bit reload value loaded via set_reload()
// - load_counter() copies reload value into counter
// - acknowledge() clears pending flag
//
// See: https://www.nesdev.org/wiki/INES_Mapper_065

// ============================================================================
// VRC IRQ Counter
// ============================================================================
//
// Used by VRC2/VRC4 and VRC6 mappers. 8-bit count-UP counter with prescaler
// for scanline-mode emulation.
//
// Modes:
// - CPU cycle mode: clocks counter every CPU cycle
// - Scanline mode: prescaler starts at 341, decrements by 3 each CPU cycle;
//   when ≤ 0, adds 341 and clocks counter (~114 CPU cycles per scanline)
//
// Counter counts UP: if counter == 0xFF, reload from latch and assert IRQ;
// otherwise increment.
//
// Register interface (caller maps addresses):
// - set_latch_low(value): sets latch bits [3:0]
// - set_latch_high(value): sets latch bits [7:4]
// - write_control(value): [.... .MEA] — mode, enable, enable-after-ack
// - acknowledge(): clears IRQ, copies enable-after-ack → enabled
// - clock(): call once per CPU cycle
//
// See: https://www.nesdev.org/wiki/VRC6#IRQ_Control
// See: https://www.nesdev.org/wiki/VRC4#IRQ_Control

/// Reusable VRC-style IRQ counter with prescaler for scanline-mode.
#[derive(Default)]
#[allow(dead_code)]
pub struct VrcIrqCounter {
    latch: u8,
    counter: u8,
    enabled: bool,
    mode_cycle: bool,
    enable_after_ack: bool,
    asserted: bool,
    prescaler: i32,
}

#[allow(dead_code)]
impl VrcIrqCounter {
    /// Prescaler initial value: 341 master clock ticks.
    const PRESCALER_INIT: i32 = 341;
    /// Prescaler step: 3 master ticks per CPU cycle.
    const PRESCALER_STEP: i32 = 3;

    /// Create a new VRC IRQ counter.
    pub fn new() -> Self {
        Self {
            latch: 0,
            counter: 0,
            enabled: false,
            mode_cycle: false,
            enable_after_ack: false,
            asserted: false,
            prescaler: 0,
        }
    }

    /// Set the low nibble of the latch (bits [3:0]).
    pub fn set_latch_low(&mut self, value: u8) {
        self.latch = (self.latch & 0xF0) | (value & 0x0F);
    }

    /// Set the high nibble of the latch (bits [7:4]).
    pub fn set_latch_high(&mut self, value: u8) {
        self.latch = (self.latch & 0x0F) | ((value & 0x0F) << 4);
    }

    /// Write IRQ control register: `[.... .MEA]`
    /// - bit 2 (M): mode (1 = CPU cycle, 0 = scanline)
    /// - bit 1 (E): enable (1 = enable and load counter from latch)
    /// - bit 0 (A): enable-after-ack
    pub fn write_control(&mut self, value: u8) {
        self.asserted = false;
        self.prescaler = Self::PRESCALER_INIT;
        self.mode_cycle = (value & 0b0000_0100) != 0;
        let enable = (value & 0b0000_0010) != 0;
        self.enable_after_ack = (value & 0b0000_0001) != 0;
        if enable {
            self.enabled = true;
            self.counter = self.latch;
        } else {
            self.enabled = false;
        }
    }

    /// Acknowledge IRQ: clears asserted flag, copies enable-after-ack → enabled.
    pub fn acknowledge(&mut self) {
        self.asserted = false;
        self.enabled = self.enable_after_ack;
    }

    /// Returns true if an IRQ is asserted (pending).
    pub fn is_pending(&self) -> bool {
        self.asserted
    }

    /// Clock the counter. Call once per CPU cycle.
    pub fn clock(&mut self) {
        if !self.enabled {
            return;
        }

        if self.mode_cycle {
            self.clock_counter();
            return;
        }

        // Scanline mode: prescaler counts down
        self.prescaler -= Self::PRESCALER_STEP;
        if self.prescaler <= 0 {
            self.prescaler += Self::PRESCALER_INIT;
            self.clock_counter();
        }
    }

    fn clock_counter(&mut self) {
        if self.counter == 0xFF {
            self.counter = self.latch;
            self.asserted = true;
        } else {
            self.counter = self.counter.wrapping_add(1);
        }
    }

    /// Read the current counter value (for testing/debugging).
    #[cfg(test)]
    pub fn counter(&self) -> u8 {
        self.counter
    }

    /// Read the current latch value (for testing/debugging).
    #[cfg(test)]
    pub fn latch(&self) -> u8 {
        self.latch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::test_helpers::banked_data;

    #[test]
    fn test_prg_ram_read_write() {
        let mut prg_ram = PrgRam::new(8192);

        // Write to start of PRG-RAM
        assert!(prg_ram.try_write(0x6000, 0x42));
        assert_eq!(prg_ram.try_read(0x6000), Some(0x42));

        // Write to end of PRG-RAM
        assert!(prg_ram.try_write(0x7FFF, 0xAB));
        assert_eq!(prg_ram.try_read(0x7FFF), Some(0xAB));

        // Out of range addresses return None/false
        assert_eq!(prg_ram.try_read(0x5FFF), None);
        assert_eq!(prg_ram.try_read(0x8000), None);
        assert!(!prg_ram.try_write(0x5FFF, 0xFF));
        assert!(!prg_ram.try_write(0x8000, 0xFF));
    }

    #[test]
    fn test_prg_ram_snapshot() {
        let mut prg_ram = PrgRam::new(8192);
        prg_ram.try_write(0x6000, 0x42);
        prg_ram.try_write(0x7FFF, 0xAB);

        let snapshot = prg_ram.snapshot();
        assert_eq!(snapshot.len(), 8192);
        assert_eq!(snapshot[0], 0x42);
        assert_eq!(snapshot[0x1FFF], 0xAB);

        // Load into a new instance
        let mut prg_ram2 = PrgRam::new(8192);
        prg_ram2.load_snapshot(&snapshot);
        assert_eq!(prg_ram2.try_read(0x6000), Some(0x42));
        assert_eq!(prg_ram2.try_read(0x7FFF), Some(0xAB));
    }

    #[test]
    fn test_chr_memory_rom() {
        let chr_rom = vec![0xAA; 8192];
        let mut chr = ChrMemory::new(chr_rom);

        assert!(!chr.is_ram());
        assert_eq!(chr.read(0x0000), 0xAA);

        // Writes to ROM are ignored
        chr.write(0x0000, 0x55);
        assert_eq!(chr.read(0x0000), 0xAA);
    }

    #[test]
    fn test_chr_memory_ram() {
        let mut chr = ChrMemory::new(vec![]); // Empty = CHR-RAM

        assert!(chr.is_ram());
        assert_eq!(chr.size(), DEFAULT_CHR_RAM_SIZE);
        assert_eq!(chr.read(0x0000), 0x00);

        // Writes to RAM succeed
        chr.write(0x0000, 0x55);
        assert_eq!(chr.read(0x0000), 0x55);
    }

    #[test]
    fn test_chr_memory_address_masking() {
        let mut chr = ChrMemory::new_ram(8192);
        assert_eq!(chr.size(), 8192);
        chr.write(0x0100, 0x42);

        // Address should be masked to 8KB range
        assert_eq!(chr.read(0x0100), 0x42);
        assert_eq!(chr.read(0x2100), 0x42); // $2100 & $1FFF = $0100
    }

    #[test]
    fn test_banked_rom_single_bank() {
        // Create ROM with 1 bank of 8KB
        let mut rom = vec![0; 8192];
        rom[0] = 0xAA;
        rom[8191] = 0xBB;

        let banked = BankedRom::new(rom, 8192);

        // Read from bank 0
        assert_eq!(banked.read(0, 0), 0xAA);
        assert_eq!(banked.read(0, 8191), 0xBB);
    }

    #[test]
    fn test_banked_rom_multiple_banks() {
        // Create ROM with 4 banks of 16KB each
        let mut rom = vec![0; 64 * 1024];
        for bank in 0..4 {
            let start = bank * 16 * 1024;
            rom[start] = (bank + 10) as u8;
            rom[start + 16 * 1024 - 1] = (bank + 20) as u8;
        }

        let banked = BankedRom::new(rom, 16 * 1024);

        // Read from different banks
        assert_eq!(banked.read(0, 0), 10);
        assert_eq!(banked.read(0, 16 * 1024 - 1), 20);
        assert_eq!(banked.read(1, 0), 11);
        assert_eq!(banked.read(1, 16 * 1024 - 1), 21);
        assert_eq!(banked.read(3, 0), 13);
        assert_eq!(banked.read(3, 16 * 1024 - 1), 23);
    }

    #[test]
    fn test_banked_rom_wrapping() {
        // Create ROM with 2 banks of 8KB each
        let mut rom = vec![0; 16 * 1024];
        rom[0] = 0x11;
        rom[8192] = 0x22;

        let banked = BankedRom::new(rom, 8192);

        // Bank 0 and 1 should work
        assert_eq!(banked.read(0, 0), 0x11);
        assert_eq!(banked.read(1, 0), 0x22);

        // Bank 2 should wrap to bank 0
        assert_eq!(banked.read(2, 0), 0x11);

        // Bank 3 should wrap to bank 1
        assert_eq!(banked.read(3, 0), 0x22);
    }

    #[test]
    fn test_banked_rom_out_of_bounds_offset() {
        let rom = vec![0xAA; 8192];
        let banked = BankedRom::new(rom, 8192);

        // Reading beyond bank size should return 0
        assert_eq!(banked.read(0, 8192), 0);
        assert_eq!(banked.read(0, 10000), 0);
    }

    #[test]
    fn test_banked_rom_num_banks() {
        let rom = vec![0; 64 * 1024]; // 4 banks of 16KB
        let banked = BankedRom::new(rom, 16 * 1024);

        assert_eq!(banked.num_banks(), 4);
    }

    #[test]
    fn test_banked_rom_empty() {
        let rom = vec![];
        let banked = BankedRom::new(rom, 8192);

        // Empty ROM should have 0 banks but not panic
        assert_eq!(banked.num_banks(), 0);
        assert_eq!(banked.read(0, 0), 0);
    }

    #[test]
    fn test_banked_rom_read_with_base_address() {
        // Test read_with_base for typical mapper usage
        let mut rom = vec![0; 32 * 1024];
        for bank in 0..2 {
            let start = bank * 16 * 1024;
            rom[start] = (bank + 100) as u8;
        }

        let banked = BankedRom::new(rom, 16 * 1024);

        // Read from bank 0 with base address $8000
        assert_eq!(banked.read_with_base(0, 0x8000, 0x8000), 100);

        // Read from bank 1 with base address $C000
        assert_eq!(banked.read_with_base(1, 0xC000, 0xC000), 101);
    }

    #[test]
    fn test_banked_rom_with_cpu_addressing() {
        const PRG_BANK_SIZE: usize = 0x4000; // 16KB
        let rom = banked_data(PRG_BANK_SIZE, 8);
        let banked = BankedRom::new(rom, PRG_BANK_SIZE);

        // Test reading from $8000-$BFFF with bank 0
        assert_eq!(banked.read_with_base(0, 0x8000, 0x8000), 0);
        assert_eq!(banked.read_with_base(0, 0x8000, 0x8001), 0);
        assert_eq!(banked.read_with_base(0, 0x8000, 0xBFFF), 0);

        // Test reading from $8000-$BFFF with bank 3
        assert_eq!(banked.read_with_base(3, 0x8000, 0x8000), 3);
        assert_eq!(banked.read_with_base(3, 0x8000, 0x8001), 3);
    }

    #[test]
    fn test_banked_rom_empty_rom() {
        const PRG_BANK_SIZE: usize = 0x4000;
        let empty_rom = vec![];
        let banked = BankedRom::new(empty_rom, PRG_BANK_SIZE);

        // Should handle gracefully
        assert_eq!(banked.num_banks(), 0);
        assert_eq!(banked.read(0, 0), 0);
        assert_eq!(banked.read(1, 0), 0);
    }

    #[test]
    fn test_banked_rom_bounds_checking() {
        const BANK_SIZE: usize = 1024;
        let rom = banked_data(BANK_SIZE, 4);
        let banked = BankedRom::new(rom, BANK_SIZE);

        // Should read valid data within bank
        assert_eq!(banked.read(0, 0), 0);
        assert_eq!(banked.read(0, BANK_SIZE - 1), 0);
        assert_eq!(banked.read(3, BANK_SIZE - 1), 3);

        // Reading with offset beyond bank size still works (just reads from later in ROM)
        // Since we have 4 banks of 1024 bytes each = 4096 total
        // read(0, 2048) = index 2048 = start of bank 2, value = 2
        assert_eq!(banked.read(0, BANK_SIZE * 2), 2);

        // Reading way beyond total ROM should return 0
        assert_eq!(banked.read(0, 10000), 0);
        assert_eq!(banked.read(99, 10000), 0);
    }

    #[test]
    fn test_state_snapshot_prg_ram() {
        let mut prg_ram = PrgRam::new(8192);
        prg_ram.try_write(0x6000, 0x42);
        prg_ram.try_write(0x7FFF, 0xAB);

        // Use StateSnapshot trait
        let snapshot = prg_ram.snapshot();
        assert_eq!(snapshot.len(), 8192);
        assert_eq!(snapshot[0], 0x42);
        assert_eq!(snapshot[0x1FFF], 0xAB);

        // Restore to a new instance
        let mut prg_ram2 = PrgRam::new(8192);
        prg_ram2.restore(&snapshot);
        assert_eq!(prg_ram2.try_read(0x6000), Some(0x42));
        assert_eq!(prg_ram2.try_read(0x7FFF), Some(0xAB));
    }

    #[test]
    fn test_state_snapshot_chr_memory() {
        let mut chr = ChrMemory::new_ram(8192);
        chr.write(0x0000, 0x11);
        chr.write(0x1FFF, 0x22);

        // Use StateSnapshot trait
        let snapshot = chr.snapshot();
        assert_eq!(snapshot.len(), 8192);
        assert_eq!(snapshot[0], 0x11);
        assert_eq!(snapshot[0x1FFF], 0x22);

        // Restore to a new instance
        let mut chr2 = ChrMemory::new_ram(8192);
        chr2.restore(&snapshot);
        assert_eq!(chr2.read(0x0000), 0x11);
        assert_eq!(chr2.read(0x1FFF), 0x22);
    }

    #[test]
    fn test_state_snapshot_chr_rom_empty() {
        // CHR-ROM should return empty snapshot
        let chr_rom_data = vec![0xAA; 8192];
        let chr = ChrMemory::new(chr_rom_data);

        let snapshot = chr.snapshot();
        assert!(snapshot.is_empty(), "CHR-ROM snapshot should be empty");
    }

    #[test]
    fn test_state_snapshot_chr_rom_restore_is_noop() {
        // CHR-ROM should ignore restore attempts (ROM is read-only)
        let chr_rom_data = vec![0xAA; 8192];
        let mut chr = ChrMemory::new(chr_rom_data);

        // Try to restore different data
        let restore_data = vec![0x55; 8192];
        chr.restore(&restore_data);

        // CHR-ROM should still contain original data
        assert_eq!(chr.read(0x0000), 0xAA);
        assert_eq!(chr.read(0x1FFF), 0xAA);
        assert!(!chr.is_ram(), "Should still be ROM, not RAM");
    }

    #[test]
    fn test_bank_switch_basic() {
        let mut bank = BankSwitch::new(4);

        // Default bank is 0
        assert_eq!(bank.current(), 0);
        assert_eq!(bank.raw(), 0);

        // Set to bank 2
        bank.set(2);
        assert_eq!(bank.current(), 2);
        assert_eq!(bank.raw(), 2);
    }

    #[test]
    fn test_bank_switch_wrapping() {
        let mut bank = BankSwitch::new(4);

        // Bank 5 wraps to 1 (5 % 4 = 1)
        bank.set(5);
        assert_eq!(bank.current(), 1);
        assert_eq!(bank.raw(), 5);

        // Bank 8 wraps to 0 (8 % 4 = 0)
        bank.set(8);
        assert_eq!(bank.current(), 0);

        // Bank 255 wraps appropriately
        bank.set(255);
        assert_eq!(bank.current(), 255 % 4);
    }

    #[test]
    fn test_bank_switch_empty_rom() {
        let mut bank = BankSwitch::new(0);

        // With 0 banks, always returns 0 (safe default)
        assert_eq!(bank.current(), 0);

        bank.set(5);
        assert_eq!(bank.current(), 0);
        assert_eq!(bank.raw(), 5);
    }

    #[test]
    fn test_bank_switch_offset_calculation() {
        let mut bank = BankSwitch::new(4);
        const BANK_SIZE: usize = 0x8000; // 32KB

        // Bank 0
        assert_eq!(bank.offset(BANK_SIZE), 0);

        // Bank 1
        bank.set(1);
        assert_eq!(bank.offset(BANK_SIZE), 0x8000);

        // Bank 2
        bank.set(2);
        assert_eq!(bank.offset(BANK_SIZE), 0x10000);

        // Bank 3
        bank.set(3);
        assert_eq!(bank.offset(BANK_SIZE), 0x18000);

        // Bank 5 wraps to 1
        bank.set(5);
        assert_eq!(bank.offset(BANK_SIZE), 0x8000);
    }

    #[test]
    fn test_bank_switch_snapshot() {
        let mut bank = BankSwitch::new(8);
        bank.set(5);

        // Take snapshot
        let snapshot = bank.snapshot();
        assert_eq!(snapshot, vec![5]);

        // Restore to new instance
        let mut bank2 = BankSwitch::new(8);
        bank2.restore(&snapshot);
        assert_eq!(bank2.current(), 5);
        assert_eq!(bank2.raw(), 5);
    }

    #[test]
    fn test_bank_switch_snapshot_empty_data() {
        let mut bank = BankSwitch::new(4);
        bank.set(3);

        // Restore with empty data should not panic
        bank.restore(&[]);
        assert_eq!(bank.raw(), 3); // Should remain unchanged
    }

    #[test]
    fn test_bank_switch_from_rom() {
        // Normal ROM with 4 banks of 8KB
        let rom_data = vec![0u8; 32 * 1024];
        let bank = BankSwitch::from_rom(&rom_data, 8 * 1024);
        assert_eq!(bank.current(), 0);

        // Empty ROM
        let empty_rom: Vec<u8> = vec![];
        let empty_bank = BankSwitch::from_rom(&empty_rom, 8 * 1024);
        assert_eq!(empty_bank.current(), 0);

        // Zero bank size
        let zero_bank = BankSwitch::from_rom(&rom_data, 0);
        assert_eq!(zero_bank.current(), 0);
    }

    // ========================================================================
    // A12RisingEdgeDetector Tests
    // ========================================================================

    /// Helper: simulate N CPU cycles on the detector.
    fn run_detector_cpu_ticks(det: &mut A12RisingEdgeDetector, n: u32) {
        for _ in 0..n {
            det.cpu_tick();
        }
    }

    #[test]
    fn test_a12_detector_new_defaults() {
        let det = A12RisingEdgeDetector::new(3);
        assert!(!det.prev_a12());
        assert!(!det.current_a12());
        assert_eq!(det.a12_low_cycles(), 0);
    }

    #[test]
    fn test_a12_detector_no_debounce_detects_first_rising_edge() {
        // With threshold=0, the very first rising edge should be detected
        let mut det = A12RisingEdgeDetector::new(0);
        assert!(det.update(0x1000)); // A12 goes high → rising edge
    }

    #[test]
    fn test_a12_detector_no_debounce_no_false_positive_when_staying_high() {
        let mut det = A12RisingEdgeDetector::new(0);
        assert!(det.update(0x1000)); // rising edge
        assert!(!det.update(0x1000)); // still high → no edge
        assert!(!det.update(0x1FFF)); // still high (different addr) → no edge
    }

    #[test]
    fn test_a12_detector_no_debounce_no_edge_on_falling() {
        let mut det = A12RisingEdgeDetector::new(0);
        det.update(0x1000); // go high
        assert!(!det.update(0x0000)); // falling edge → no detection
    }

    #[test]
    fn test_a12_detector_no_debounce_detects_repeated_rising_edges() {
        let mut det = A12RisingEdgeDetector::new(0);
        assert!(det.update(0x1000)); // first rising edge
        assert!(!det.update(0x0000)); // go low
        assert!(det.update(0x1000)); // second rising edge
    }

    #[test]
    fn test_a12_detector_debounce_rejects_edge_without_enough_low_cycles() {
        let mut det = A12RisingEdgeDetector::new(3);
        // Only 2 CPU cycles with A12 low — not enough
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 2);
        assert!(!det.update(0x1000)); // rising edge but debounce not met
    }

    #[test]
    fn test_a12_detector_debounce_accepts_edge_with_enough_low_cycles() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 3); // exactly 3 CPU cycles
        assert!(det.update(0x1000)); // rising edge with debounce met
    }

    #[test]
    fn test_a12_detector_debounce_accepts_edge_with_excess_low_cycles() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 10); // more than enough
        assert!(det.update(0x1000)); // rising edge with debounce met
    }

    #[test]
    fn test_a12_detector_cpu_tick_resets_low_cycles_when_a12_high() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 5); // accumulate some low cycles
        assert!(det.a12_low_cycles() > 0);

        det.update(0x1000); // A12 goes high
        det.cpu_tick(); // should reset counter
        assert_eq!(det.a12_low_cycles(), 0);
    }

    #[test]
    fn test_a12_detector_cpu_tick_increments_low_cycles_when_a12_low() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        det.cpu_tick();
        assert_eq!(det.a12_low_cycles(), 1);
        det.cpu_tick();
        assert_eq!(det.a12_low_cycles(), 2);
        det.cpu_tick();
        assert_eq!(det.a12_low_cycles(), 3);
    }

    #[test]
    fn test_a12_detector_low_cycles_saturate() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        // Run many CPU cycles — should saturate at 255, not overflow
        run_detector_cpu_ticks(&mut det, 300);
        assert_eq!(det.a12_low_cycles(), 255);
    }

    #[test]
    fn test_a12_detector_debounce_resets_after_detection() {
        // After a valid rising edge, a12_low_cycles should be reset
        // so the next rising edge requires fresh debounce
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 3);
        assert!(det.update(0x1000)); // valid rising edge

        // Now A12 is high, cpu_tick should reset counter
        det.cpu_tick();
        assert_eq!(det.a12_low_cycles(), 0);

        // Go low briefly and try again — should fail debounce
        det.update(0x0000);
        run_detector_cpu_ticks(&mut det, 1);
        assert!(!det.update(0x1000)); // not enough low cycles
    }

    #[test]
    fn test_a12_detector_update_tracks_current_a12() {
        let mut det = A12RisingEdgeDetector::new(3);
        assert!(!det.current_a12());

        det.update(0x1000);
        assert!(det.current_a12());

        det.update(0x0000);
        assert!(!det.current_a12());
    }

    #[test]
    fn test_a12_detector_snapshot_restore() {
        let mut det = A12RisingEdgeDetector::new(3);
        det.update(0x0000); // A12 low
        run_detector_cpu_ticks(&mut det, 2);
        det.update(0x1000); // A12 high (but debounce not met)

        // Save state
        let prev = det.prev_a12();
        let curr = det.current_a12();
        let low = det.a12_low_cycles();

        // Create new detector and restore
        let mut det2 = A12RisingEdgeDetector::new(3);
        det2.set_prev_a12(prev);
        det2.set_current_a12(curr);
        det2.set_a12_low_cycles(low);

        assert_eq!(det2.prev_a12(), prev);
        assert_eq!(det2.current_a12(), curr);
        assert_eq!(det2.a12_low_cycles(), low);
    }

    #[test]
    fn test_a12_detector_addr_bit12_extraction() {
        // Verify that only bit 12 of the address matters
        let mut det = A12RisingEdgeDetector::new(0);

        // 0x1000 = bit 12 set
        assert!(det.update(0x1000));

        // Go low with various addresses that have bit 12 clear
        assert!(!det.update(0x0FFF));

        // Rise again with different address but bit 12 set
        assert!(det.update(0x1FFF));
    }

    // ========================================================================
    // A12IrqCounter Tests
    // ========================================================================

    /// Helper: simulate N CPU cycles to satisfy A12 debounce requirement.
    fn run_cpu_cycles(irq: &mut A12IrqCounter, n: u32) {
        for _ in 0..n {
            irq.cpu_cycle();
        }
    }

    /// Helper: simulate a valid A12 rising edge (low for 3+ cycles, then high).
    fn trigger_a12_rising_edge(irq: &mut A12IrqCounter) {
        irq.ppu_address_changed(0x0000); // A12 low
        run_cpu_cycles(irq, 3);
        irq.ppu_address_changed(0x1000); // A12 high → rising edge
    }

    #[test]
    fn test_a12_irq_new_defaults() {
        let irq = A12IrqCounter::new(false);
        assert!(!irq.is_pending());
        assert_eq!(irq.counter(), 0);
    }

    #[test]
    fn test_a12_irq_set_latch() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(10);
        // Latch doesn't affect counter until reload
        assert_eq!(irq.counter(), 0);
    }

    #[test]
    fn test_a12_irq_reload_loads_latch_into_counter() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(5);
        irq.request_reload();
        irq.set_enabled(true);

        // Counter is 0 + reload flag → next clock loads latch
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 5);
    }

    #[test]
    fn test_a12_irq_counter_decrements_on_each_clock() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(3);
        irq.request_reload();
        irq.set_enabled(true);

        // First clock: counter==0 + reload → loads 3
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 3);

        // Second clock: 3 → 2
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 2);

        // Third clock: 2 → 1
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 1);
    }

    #[test]
    fn test_a12_irq_fires_when_counter_reaches_zero_normal() {
        let mut irq = A12IrqCounter::new(false); // Normal (Sharp)
        irq.set_latch(2);
        irq.request_reload();
        irq.set_enabled(true);

        // Clock 1: loads 2 (counter was 0 + reload)
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());

        // Clock 2: 2 → 1
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());

        // Clock 3: 1 → 0 → IRQ fires
        trigger_a12_rising_edge(&mut irq);
        assert!(irq.is_pending());
    }

    #[test]
    fn test_a12_irq_does_not_fire_when_disabled() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(1);
        irq.request_reload();
        // Enabled is false by default

        // Clock 1: loads 1
        trigger_a12_rising_edge(&mut irq);
        // Clock 2: 1 → 0
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());
    }

    #[test]
    fn test_a12_irq_disable_acknowledges_pending() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(1);
        irq.request_reload();
        irq.set_enabled(true);

        // Fire IRQ
        trigger_a12_rising_edge(&mut irq); // loads 1
        trigger_a12_rising_edge(&mut irq); // 1 → 0, fires

        assert!(irq.is_pending());

        // Disable acknowledges
        irq.set_enabled(false);
        assert!(!irq.is_pending());
    }

    #[test]
    fn test_a12_irq_counter_reloads_on_zero_naturally() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(1);
        irq.request_reload();
        irq.set_enabled(true);

        // Clock 1: loads 1 (counter==0 + reload)
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 1);

        // Clock 2: 1 → 0, fires IRQ
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 0);
        assert!(irq.is_pending());

        // Clock 3: counter==0 (no reload flag) → loads latch (1)
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 1);
    }

    #[test]
    fn test_a12_irq_debounce_rejects_short_low() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(5);
        irq.request_reload();
        irq.set_enabled(true);

        // A12 goes low for only 2 cycles, then high — should NOT clock
        irq.ppu_address_changed(0x0000); // A12 low
        run_cpu_cycles(&mut irq, 2); // Only 2 cycles low
        irq.ppu_address_changed(0x1000); // A12 high

        // Counter should still be 0 (not clocked)
        assert_eq!(irq.counter(), 0);
    }

    #[test]
    fn test_a12_irq_debounce_accepts_sufficient_low() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(5);
        irq.request_reload();
        irq.set_enabled(true);

        // A12 low for exactly 3 cycles then high → valid edge
        irq.ppu_address_changed(0x0000); // A12 low
        run_cpu_cycles(&mut irq, 3);
        irq.ppu_address_changed(0x1000); // A12 high → clocked

        assert_eq!(irq.counter(), 5);
    }

    #[test]
    fn test_a12_irq_no_edge_when_already_high() {
        let mut irq = A12IrqCounter::new(false);
        irq.set_latch(5);
        irq.request_reload();
        irq.set_enabled(true);

        // First: valid rising edge to load counter
        trigger_a12_rising_edge(&mut irq);
        assert_eq!(irq.counter(), 5);

        // Second: A12 stays high → no rising edge → no clock
        run_cpu_cycles(&mut irq, 3);
        irq.ppu_address_changed(0x1000);
        assert_eq!(irq.counter(), 5);
    }

    #[test]
    fn test_a12_irq_alternate_fires_on_decrement_to_zero() {
        let mut irq = A12IrqCounter::new(true); // Alternate (NEC)
        irq.set_latch(2);
        irq.request_reload();
        irq.set_enabled(true);

        // Clock 1: loads 2
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());

        // Clock 2: 2 → 1
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());

        // Clock 3: 1 → 0 → IRQ (decrement-to-zero transition)
        trigger_a12_rising_edge(&mut irq);
        assert!(irq.is_pending());
    }

    #[test]
    fn test_a12_irq_alternate_fires_on_reload_triggered_to_zero() {
        let mut irq = A12IrqCounter::new(true); // Alternate (NEC)
        irq.set_latch(0); // Latch = 0
        irq.request_reload();
        irq.set_enabled(true);

        // Clock: counter==0 + reload → loads 0 from latch
        // This is reload-triggered, so alternate fires
        trigger_a12_rising_edge(&mut irq);
        assert!(irq.is_pending());
    }

    #[test]
    fn test_a12_irq_alternate_no_fire_on_natural_zero_reload() {
        let mut irq = A12IrqCounter::new(true); // Alternate (NEC)
        irq.set_latch(0); // Latch = 0
        irq.set_enabled(true);

        // First: fire via reload (reload flag set)
        irq.request_reload();
        trigger_a12_rising_edge(&mut irq);
        assert!(irq.is_pending());

        // Acknowledge
        irq.set_enabled(false);
        irq.set_enabled(true);

        // Now counter is 0, reload flag is NOT set → natural zero-to-zero
        // Alternate behavior: should NOT fire (no transition)
        trigger_a12_rising_edge(&mut irq);
        assert!(!irq.is_pending());
    }

    #[test]
    fn test_a12_irq_normal_fires_on_natural_zero_reload() {
        let mut irq = A12IrqCounter::new(false); // Normal (Sharp)
        irq.set_latch(0); // Latch = 0
        irq.set_enabled(true);

        // Counter is 0, no reload flag → loads latch (0) → counter is 0
        // Normal behavior: fires because counter IS 0
        trigger_a12_rising_edge(&mut irq);
        assert!(irq.is_pending());
    }

    // ========================================================================
    // VrcIrqCounter Tests
    // ========================================================================

    #[test]
    fn test_vrc_irq_new_defaults() {
        let irq = VrcIrqCounter::new();
        assert!(!irq.is_pending());
        assert_eq!(irq.counter(), 0);
        assert_eq!(irq.latch(), 0);
    }

    #[test]
    fn test_vrc_irq_set_latch_nibbles() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x0A); // low nibble = A
        assert_eq!(irq.latch(), 0x0A);

        irq.set_latch_high(0x05); // high nibble = 5 → latch = 0x5A
        assert_eq!(irq.latch(), 0x5A);
    }

    #[test]
    fn test_vrc_irq_set_latch_preserves_other_nibble() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_high(0x0F); // high nibble = F → latch = 0xF0
        irq.set_latch_low(0x03); // low nibble = 3 → latch = 0xF3
        assert_eq!(irq.latch(), 0xF3);

        // Changing high doesn't affect low
        irq.set_latch_high(0x02); // → latch = 0x23
        assert_eq!(irq.latch(), 0x23);
    }

    #[test]
    fn test_vrc_irq_cpu_cycle_mode_counts_up() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x00);
        irq.set_latch_high(0x00);
        // Enable in cycle mode: value = 0b0000_0110 (M=1, E=1, A=0)
        irq.write_control(0b0000_0110);

        // Counter should have been loaded from latch (0)
        assert_eq!(irq.counter(), 0);

        irq.clock();
        assert_eq!(irq.counter(), 1);

        irq.clock();
        assert_eq!(irq.counter(), 2);
    }

    #[test]
    fn test_vrc_irq_cpu_cycle_mode_fires_on_overflow() {
        let mut irq = VrcIrqCounter::new();
        // Set latch to 0xFE so counter overflows quickly
        irq.set_latch_low(0x0E);
        irq.set_latch_high(0x0F); // latch = 0xFE
        irq.write_control(0b0000_0110); // Enable in cycle mode

        assert_eq!(irq.counter(), 0xFE);

        irq.clock(); // 0xFE → 0xFF
        assert!(!irq.is_pending());

        irq.clock(); // 0xFF → reload from latch, assert IRQ
        assert!(irq.is_pending());
        assert_eq!(irq.counter(), 0xFE); // reloaded from latch
    }

    #[test]
    fn test_vrc_irq_scanline_mode_prescaler() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x0E);
        irq.set_latch_high(0x0F); // latch = 0xFE
        // Enable in scanline mode: value = 0b0000_0010 (M=0, E=1, A=0)
        irq.write_control(0b0000_0010);

        assert_eq!(irq.counter(), 0xFE);

        // In scanline mode: prescaler=341, decrements by 3 each CPU cycle.
        // 341/3 = 113.67 → needs 114 CPU cycles for one counter tick
        for _ in 0..113 {
            irq.clock();
        }
        // After 113 clocks: prescaler = 341 - 113*3 = 341 - 339 = 2 (> 0, no tick)
        assert_eq!(irq.counter(), 0xFE);

        irq.clock(); // 114th: prescaler = 2 - 3 = -1 ≤ 0 → tick counter
        assert_eq!(irq.counter(), 0xFF);
    }

    #[test]
    fn test_vrc_irq_acknowledge_clears_and_restores_enable() {
        let mut irq = VrcIrqCounter::new();
        // Enable with enable-after-ack=1: value = 0b0000_0111
        irq.write_control(0b0000_0111);

        // Force IRQ
        irq.set_latch_low(0x0F);
        irq.set_latch_high(0x0F); // latch = 0xFF
        irq.write_control(0b0000_0110); // re-enable, no ack yet
        irq.clock(); // counter 0xFF → reload, assert!

        assert!(irq.is_pending());

        irq.acknowledge();
        assert!(!irq.is_pending());
        // After ack, enabled should be restored from enable_after_ack
    }

    #[test]
    fn test_vrc_irq_write_control_acknowledges() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x0F);
        irq.set_latch_high(0x0F); // latch = 0xFF
        irq.write_control(0b0000_0110); // Enable in cycle mode
        irq.clock(); // counter 0xFF → reload, assert

        assert!(irq.is_pending());

        // Writing control register acknowledges IRQ
        irq.write_control(0b0000_0000); // disable + ack
        assert!(!irq.is_pending());
    }

    #[test]
    fn test_vrc_irq_disabled_does_not_count() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x05);
        // Don't enable: write_control with E=0
        irq.write_control(0b0000_0000);

        irq.clock();
        irq.clock();
        assert_eq!(irq.counter(), 0); // no counting
    }

    #[test]
    fn test_vrc_irq_enable_loads_counter_from_latch() {
        let mut irq = VrcIrqCounter::new();
        irq.set_latch_low(0x0A);
        irq.set_latch_high(0x0B); // latch = 0xBA
        irq.write_control(0b0000_0010); // Enable bit set → loads counter

        assert_eq!(irq.counter(), 0xBA);
    }
}
