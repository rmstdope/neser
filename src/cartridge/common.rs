//! Common utilities for NES mapper implementations.
//!
//! This module provides reusable components that are shared across multiple mappers,
//! reducing code duplication and ensuring consistent behavior.

use super::mapper::MapperCapabilities;
use crate::cartridge::NametableLayout;

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
/// use neser::cartridge::StateSnapshot;
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
/// use neser::cartridge::StateSnapshot;
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
/// use neser::cartridge::{Mapper, StateSnapshot};
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
/// use neser::cartridge::PrgRam;
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

    /// Load a snapshot into PRG-RAM from save persistence.
    pub fn load_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.data.len());
        self.data[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    /// Re-initialize PRG-RAM contents based on the given mode.
    ///
    /// This should be called on cartridge insertion or hard reset.
    /// Soft resets should NOT call this (RAM contents persist).
    pub fn initialize(&mut self, mode: crate::console::RamInitMode) {
        crate::console::initialize_ram(&mut self.data, mode);
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
    pub fn initialize(&mut self, mode: crate::console::RamInitMode) {
        if self.is_ram {
            crate::console::initialize_ram(&mut self.data, mode);
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
/// ```rust
/// use neser::cartridge::BankedRom;
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
#[derive(Clone)]
pub struct BankedRom {
    data: Vec<u8>,
    bank_size: usize,
}

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
/// ```rust
/// use neser::cartridge::BankSwitch;
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
#[derive(Clone, Copy, Debug)]
pub struct BankSwitch {
    num_banks: usize,
    bank: u8,
}

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
    pub fn offset(&self, bank_size: usize) -> usize {
        self.current() * bank_size
    }

    /// Get the raw bank value without wrapping.
    pub fn raw(&self) -> u8 {
        self.bank
    }
}

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

/// Common mapper infrastructure that handles the boilerplate memory management
/// shared by most NES mappers.
///
/// Inspired by Mesen2's `BaseMapper` class, this struct owns the common fields
/// (PRG-ROM, PRG-RAM, CHR memory, mirroring, mapper number) and provides
/// default implementations for the repetitive `Mapper` trait methods.
///
/// Mappers embed `BaseMapper` via composition and delegate boilerplate to it,
/// only implementing the truly mapper-specific logic themselves.
///
/// # Usage
///
/// ```rust,ignore
/// use neser::cartridge::{BaseMapper, Mapper, NametableLayout, MapperCapabilities};
///
/// pub struct MyMapper {
///     base: BaseMapper,
///     // ... mapper-specific fields
/// }
///
/// impl Mapper for MyMapper {
///     fn read_prg(&self, addr: u16) -> u8 {
///         if let Some(v) = self.base.try_read_prg_ram(addr) { return v; }
///         // mapper-specific PRG-ROM read logic
///         0
///     }
///     fn write_prg(&mut self, addr: u16, value: u8) {
///         if self.base.try_write_prg_ram(addr, value) { return; }
///         // mapper-specific register write logic
///     }
///     fn read_chr(&mut self, addr: u16) -> u8 { self.base.read_chr(addr) }
///     fn write_chr(&mut self, addr: u16, value: u8) { self.base.write_chr(addr, value) }
///     fn get_mirroring(&self) -> NametableLayout { self.base.mirroring() }
///     fn mapper_number(&self) -> u8 { self.base.mapper_number() }
///     fn wram_size(&self) -> usize { self.base.wram_size() }
///     fn wram_snapshot(&self) -> Vec<u8> { self.base.wram_snapshot() }
///     fn load_wram_snapshot(&mut self, data: &[u8]) { self.base.load_wram_snapshot(data) }
///     fn chr_ram_snapshot(&self) -> Vec<u8> { self.base.chr_ram_snapshot() }
///     fn restore_chr_ram(&mut self, data: &[u8]) { self.base.restore_chr_ram(data) }
///     fn initialize_ram(&mut self, mode: neser::console::RamInitMode) {
///         self.base.initialize_ram(mode);
///     }
///     fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
///         self.base.read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
///     }
///     fn capabilities(&self) -> MapperCapabilities { self.base.capabilities() }
/// }
/// ```
pub struct BaseMapper {
    prg_rom: Vec<u8>,
    prg_ram: Option<PrgRam>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    mapper_number: u16,
    capabilities: MapperCapabilities,
}

#[allow(dead_code)]
impl BaseMapper {
    /// Create a new `BaseMapper` from a `MapperContext`.
    ///
    /// PRG-RAM is created only when the header explicitly specifies a non-zero size.
    /// CHR memory is ROM when `chr_rom` is non-empty, otherwise CHR-RAM is allocated.
    pub fn new(ctx: &super::mapper::MapperContext, capabilities: MapperCapabilities) -> Self {
        let prg_ram = if ctx.prg_ram_size_specified && ctx.prg_ram_banks_8k > 0 {
            Some(PrgRam::new(
                ctx.prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        } else {
            None
        };
        Self {
            prg_rom: ctx.prg_rom.clone(),
            prg_ram,
            chr_memory: ChrMemory::new(ctx.chr_rom.clone()),
            mirroring: ctx.mirroring,
            mapper_number: ctx.mapper,
            capabilities,
        }
    }

    // --- PRG-ROM access ---

    /// Get a reference to the PRG-ROM data.
    #[inline]
    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    /// Read a byte from fixed PRG-ROM at $8000-$FFFF with automatic mirroring.
    ///
    /// For mappers with no PRG banking (e.g., NROM), this maps the full
    /// 32KB window using `offset % prg_rom.len()` which naturally handles
    /// 16KB mirroring.
    #[inline]
    pub fn read_prg_rom_fixed(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            let index = (addr - 0x8000) as usize % self.prg_rom.len();
            self.prg_rom.get(index).copied().unwrap_or(0)
        } else {
            0
        }
    }

    // --- PRG-RAM access ---

    /// Try to read from PRG-RAM if the address is in $6000-$7FFF.
    /// Returns `Some(value)` if PRG-RAM exists and address is in range, else `None`.
    #[inline]
    pub fn try_read_prg_ram(&self, addr: u16) -> Option<u8> {
        self.prg_ram.as_ref().and_then(|ram| ram.try_read(addr))
    }

    /// Try to write to PRG-RAM if the address is in $6000-$7FFF.
    /// Returns `true` if the write was handled.
    #[inline]
    pub fn try_write_prg_ram(&mut self, addr: u16, value: u8) -> bool {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.try_write(addr, value)
        } else {
            false
        }
    }

    /// Whether PRG-RAM is present.
    #[inline]
    pub fn has_prg_ram(&self) -> bool {
        self.prg_ram.is_some()
    }

    // --- CHR memory access ---

    /// Read a byte from CHR memory (ROM or RAM) at $0000-$1FFF.
    #[inline]
    pub fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    /// Write a byte to CHR memory. Only succeeds for CHR-RAM.
    #[inline]
    pub fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    // --- Mirroring ---

    /// Get the nametable mirroring layout.
    #[inline]
    pub fn mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    /// Set the nametable mirroring layout (for mappers with dynamic mirroring).
    #[inline]
    pub fn set_mirroring(&mut self, mirroring: NametableLayout) {
        self.mirroring = mirroring;
    }

    // --- Mapper identification ---

    /// Get the mapper number.
    #[inline]
    pub fn mapper_number(&self) -> u8 {
        self.mapper_number as u8
    }

    // --- Save-state / WRAM support ---

    /// Get the WRAM (PRG-RAM) size in bytes.
    pub fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    /// Create a snapshot of PRG-RAM for save persistence.
    pub fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    /// Load a PRG-RAM snapshot from save persistence.
    pub fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
    }

    /// Create a snapshot of CHR-RAM for save-state.
    pub fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    /// Restore CHR-RAM from a save-state.
    pub fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    /// Re-initialize all RAM (PRG-RAM + CHR-RAM) for cartridge insertion / hard reset.
    pub fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.initialize(mode);
        }
        self.chr_memory.initialize(mode);
    }

    /// Read PRG with open-bus handling.
    ///
    /// Returns `open_bus` for addresses below $6000 and for $6000-$7FFF when
    /// no PRG-RAM is present. Otherwise delegates to the provided `read_prg` function.
    pub fn read_prg_open_bus(&self, addr: u16, open_bus: u8, read_prg: impl Fn(u16) -> u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => read_prg(addr),
        }
    }

    /// Get the mapper capabilities.
    pub fn capabilities(&self) -> MapperCapabilities {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::test_helpers::banked_data;

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

    // --- BaseMapper tests ---

    use crate::cartridge::mapper::MapperContext;

    fn make_base_mapper_with_prg_ram(prg_ram_banks: u8) -> BaseMapper {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0xAA; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        )
        .with_prg_ram_banks(prg_ram_banks);
        BaseMapper::new(
            &ctx,
            MapperCapabilities {
                max_prg_ram_kb: if prg_ram_banks > 0 {
                    prg_ram_banks as usize * 8
                } else {
                    0
                },
                ..Default::default()
            },
        )
    }

    #[test]
    fn test_base_mapper_read_prg_rom_fixed_32kb() {
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x0000] = 0xAA;
        prg_rom[0x4000] = 0xBB;
        prg_rom[0x7FFF] = 0xCC;
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_prg_rom_fixed(0x8000), 0xAA);
        assert_eq!(base.read_prg_rom_fixed(0xC000), 0xBB);
        assert_eq!(base.read_prg_rom_fixed(0xFFFF), 0xCC);
    }

    #[test]
    fn test_base_mapper_read_prg_rom_fixed_16kb_mirrors() {
        let mut prg_rom = vec![0; 0x4000];
        prg_rom[0x0000] = 0xAA;
        prg_rom[0x3FFF] = 0xBB;
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_prg_rom_fixed(0x8000), 0xAA);
        assert_eq!(base.read_prg_rom_fixed(0xBFFF), 0xBB);
        assert_eq!(base.read_prg_rom_fixed(0xC000), 0xAA); // mirrored
        assert_eq!(base.read_prg_rom_fixed(0xFFFF), 0xBB); // mirrored
    }

    #[test]
    fn test_base_mapper_prg_ram_read_write() {
        let mut base = make_base_mapper_with_prg_ram(1);

        assert_eq!(base.try_read_prg_ram(0x6000), Some(0));
        base.try_write_prg_ram(0x6000, 0x42);
        assert_eq!(base.try_read_prg_ram(0x6000), Some(0x42));

        // Out of range
        assert_eq!(base.try_read_prg_ram(0x5FFF), None);
        assert_eq!(base.try_read_prg_ram(0x8000), None);
    }

    #[test]
    fn test_base_mapper_no_prg_ram() {
        let base = make_base_mapper_with_prg_ram(0);

        assert!(!base.has_prg_ram());
        assert_eq!(base.try_read_prg_ram(0x6000), None);
        assert_eq!(base.wram_size(), 0);
        assert!(base.wram_snapshot().is_empty());
    }

    #[test]
    fn test_base_mapper_chr_read_write_ram() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![], // empty = CHR-RAM
            NametableLayout::Horizontal,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        base.write_chr(0x0000, 0xAA);
        assert_eq!(base.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_base_mapper_chr_read_rom() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0x55; 8192],
            NametableLayout::Horizontal,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_chr(0x0000), 0x55);
        base.write_chr(0x0000, 0xAA); // should be ignored (ROM)
        assert_eq!(base.read_chr(0x0000), 0x55);
    }

    #[test]
    fn test_base_mapper_mirroring() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Vertical,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.mirroring(), NametableLayout::Vertical);
        base.set_mirroring(NametableLayout::Horizontal);
        assert_eq!(base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_base_mapper_wram_snapshot_restore() {
        let mut base = make_base_mapper_with_prg_ram(1);
        base.try_write_prg_ram(0x6000, 0x42);
        base.try_write_prg_ram(0x7FFF, 0xAB);

        let snapshot = base.wram_snapshot();
        assert_eq!(snapshot.len(), 8192);

        let mut base2 = make_base_mapper_with_prg_ram(1);
        base2.load_wram_snapshot(&snapshot);
        assert_eq!(base2.try_read_prg_ram(0x6000), Some(0x42));
        assert_eq!(base2.try_read_prg_ram(0x7FFF), Some(0xAB));
    }

    #[test]
    fn test_base_mapper_chr_ram_snapshot_restore() {
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.write_chr(0x0000, 0xAA);
        base.write_chr(0x1FFF, 0xBB);

        let snapshot = base.chr_ram_snapshot();

        let ctx2 =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base2 = BaseMapper::new(&ctx2, MapperCapabilities::default());
        base2.restore_chr_ram(&snapshot);
        assert_eq!(base2.read_chr(0x0000), 0xAA);
        assert_eq!(base2.read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_base_mapper_open_bus_no_prg_ram() {
        let base = make_base_mapper_with_prg_ram(0);

        // Below $6000: open bus
        assert_eq!(base.read_prg_open_bus(0x5000, 0x42, |_| 0xFF), 0x42);
        // $6000-$7FFF with no PRG-RAM: open bus
        assert_eq!(base.read_prg_open_bus(0x6000, 0x42, |_| 0xFF), 0x42);
        // $8000+: delegates to read_prg
        assert_eq!(base.read_prg_open_bus(0x8000, 0x42, |_| 0xAA), 0xAA);
    }

    #[test]
    fn test_base_mapper_open_bus_with_prg_ram() {
        let base = make_base_mapper_with_prg_ram(1);

        // $6000-$7FFF with PRG-RAM: delegates to read_prg
        assert_eq!(base.read_prg_open_bus(0x6000, 0x42, |_| 0xBB), 0xBB);
    }

    #[test]
    fn test_base_mapper_mapper_number() {
        let ctx = MapperContext::new_for_test(
            7,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());
        assert_eq!(base.mapper_number(), 7);
    }

    #[test]
    fn test_base_mapper_capabilities() {
        let caps = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            ..Default::default()
        };
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let base = BaseMapper::new(&ctx, caps.clone());
        assert_eq!(base.capabilities(), caps);
    }
}
