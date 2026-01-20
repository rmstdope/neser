//! Common utilities for NES mapper implementations.
//!
//! This module provides reusable components that are shared across multiple mappers,
//! reducing code duplication and ensuring consistent behavior.

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
/// ```ignore
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
/// }
///
/// // In Mapper trait impl:
/// fn read_prg(&self, addr: u16) -> u8 {
///     if let Some(value) = self.prg_ram.try_read(addr) {
///         return value;
///     }
///     // ... handle other addresses
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
/// // Create a helper for 16KB PRG banks
/// let banked_prg = BankedRom::new(prg_rom, 0x4000);
///
/// // Read from bank 3, offset 0x1000
/// let value = banked_prg.read(3, 0x1000);
///
/// // Or use with address calculation
/// let value = banked_prg.read_with_base(bank, 0x8000, 0x9000);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
