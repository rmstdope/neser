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
        assert_eq!(chr.read(0x0000), 0x00);

        // Writes to RAM succeed
        chr.write(0x0000, 0x55);
        assert_eq!(chr.read(0x0000), 0x55);
    }

    #[test]
    fn test_chr_memory_address_masking() {
        let mut chr = ChrMemory::new_ram(8192);
        chr.write(0x0100, 0x42);

        // Address should be masked to 8KB range
        assert_eq!(chr.read(0x0100), 0x42);
        assert_eq!(chr.read(0x2100), 0x42); // $2100 & $1FFF = $0100
    }
}
