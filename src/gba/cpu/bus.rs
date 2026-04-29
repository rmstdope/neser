//! Memory bus abstraction used by the ARM7TDMI core.
//!
//! The full GBA memory map (BIOS, on-board work RAM, on-chip work RAM, I/O,
//! palette/VRAM/OAM, cartridge ROM, cartridge RAM) will be implemented in
//! later sub-issues. For now the CPU only needs a small abstraction so that
//! it can be wired against test fixtures and a future bus implementation.
//!
//! The default [`RamBus`] implementation provides a flat little-endian RAM
//! window that is convenient for unit tests of the instruction set.

/// Abstraction over the memory bus seen by the CPU.
pub trait Bus {
    /// Read a 32-bit word. The address is automatically aligned to a 4-byte
    /// boundary by the implementation when required by the architecture.
    fn read32(&mut self, addr: u32) -> u32;

    /// Read a 16-bit halfword.
    fn read16(&mut self, addr: u32) -> u16;

    /// Read a single byte.
    fn read8(&mut self, addr: u32) -> u8;

    /// Write a 32-bit word.
    fn write32(&mut self, addr: u32, value: u32);

    /// Write a 16-bit halfword.
    fn write16(&mut self, addr: u32, value: u16);

    /// Write a single byte.
    fn write8(&mut self, addr: u32, value: u8);
}

/// Flat, little-endian RAM-only bus used in unit tests and as a stub for the
/// boot-sequence smoke test. All addresses are taken modulo the RAM size.
#[derive(Debug, Clone)]
pub struct RamBus {
    bytes: Vec<u8>,
}

impl RamBus {
    /// Create a new RAM bus with the given size in bytes (zero-initialised).
    pub fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    /// Write a sequence of bytes starting at `addr`.
    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) {
        let len = self.bytes.len();
        let base = (addr as usize) % len;
        for (i, &b) in data.iter().enumerate() {
            self.bytes[(base + i) % len] = b;
        }
    }

    /// Convenience helper to write a 32-bit word at `addr` (little-endian).
    pub fn write_word(&mut self, addr: u32, word: u32) {
        self.write_bytes(addr, &word.to_le_bytes());
    }

    /// Convenience helper to write a 16-bit halfword at `addr` (little-endian).
    pub fn write_halfword(&mut self, addr: u32, hw: u16) {
        self.write_bytes(addr, &hw.to_le_bytes());
    }

    fn idx(&self, addr: u32) -> usize {
        (addr as usize) % self.bytes.len()
    }
}

impl Bus for RamBus {
    fn read32(&mut self, addr: u32) -> u32 {
        let a = addr & !0x3;
        let i = self.idx(a);
        let b = &self.bytes;
        u32::from_le_bytes([
            b[i],
            b[(i + 1) % b.len()],
            b[(i + 2) % b.len()],
            b[(i + 3) % b.len()],
        ])
    }

    fn read16(&mut self, addr: u32) -> u16 {
        let a = addr & !0x1;
        let i = self.idx(a);
        let b = &self.bytes;
        u16::from_le_bytes([b[i], b[(i + 1) % b.len()]])
    }

    fn read8(&mut self, addr: u32) -> u8 {
        let i = self.idx(addr);
        self.bytes[i]
    }

    fn write32(&mut self, addr: u32, value: u32) {
        let a = addr & !0x3;
        self.write_bytes(a, &value.to_le_bytes());
    }

    fn write16(&mut self, addr: u32, value: u16) {
        let a = addr & !0x1;
        self.write_bytes(a, &value.to_le_bytes());
    }

    fn write8(&mut self, addr: u32, value: u8) {
        let i = self.idx(addr);
        self.bytes[i] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_bus_round_trips() {
        let mut bus = RamBus::new(0x100);
        bus.write32(0x10, 0xDEAD_BEEF);
        assert_eq!(bus.read32(0x10), 0xDEAD_BEEF);
        assert_eq!(bus.read16(0x10), 0xBEEF);
        assert_eq!(bus.read16(0x12), 0xDEAD);
        assert_eq!(bus.read8(0x10), 0xEF);
        assert_eq!(bus.read8(0x13), 0xDE);

        bus.write16(0x20, 0x1234);
        assert_eq!(bus.read16(0x20), 0x1234);
        assert_eq!(bus.read8(0x20), 0x34);
        assert_eq!(bus.read8(0x21), 0x12);

        bus.write8(0x30, 0xAB);
        assert_eq!(bus.read8(0x30), 0xAB);
    }

    #[test]
    fn read32_aligns_address() {
        let mut bus = RamBus::new(0x100);
        bus.write32(0x10, 0xCAFEBABE);
        // Reading from an unaligned address returns the aligned word.
        assert_eq!(bus.read32(0x11), 0xCAFEBABE);
        assert_eq!(bus.read32(0x12), 0xCAFEBABE);
        assert_eq!(bus.read32(0x13), 0xCAFEBABE);
    }
}
