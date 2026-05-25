//! Memory bus abstraction used by the ARM7TDMI core.
//!
//! The full GBA memory map (BIOS, on-board work RAM, on-chip work RAM, I/O,
//! palette/VRAM/OAM, cartridge ROM, cartridge RAM) will be implemented in
//! later sub-issues. For now the CPU only needs a small abstraction so that
//! it can be wired against test fixtures and a future bus implementation.
//!
//! The default [`RamBus`] implementation provides a flat little-endian RAM
//! window that is convenient for unit tests of the instruction set.

use crate::gba::bus::WidthClass;

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

    /// Non-sequential access cycle cost for the given address and width.
    fn n_cycles(&self, addr: u32, width: WidthClass) -> u32;

    /// Sequential access cycle cost for the given address and width.
    fn s_cycles(&self, addr: u32, width: WidthClass) -> u32;

    /// Whether Game Pak ROM opcode prefetch is enabled in WAITCNT.
    fn gamepak_prefetch_enabled(&self) -> bool {
        false
    }

    /// Whether the instruction that just executed enabled an immediate DMA
    /// transfer whose Game Pak bus use blocks the opcode prefetcher for one
    /// extra cycle.
    fn immediate_gamepak_dma_prefetch_penalty(&self, _code_width: WidthClass) -> bool {
        false
    }

    /// Read a 32-bit word for an **instruction fetch**.  The address must
    /// already be aligned to a 4-byte boundary.  Implementations may track
    /// this access separately from data reads so that bus faults can be
    /// attributed to [`prefetch_abort_pending`](Bus::prefetch_abort_pending)
    /// rather than [`data_abort_pending`](Bus::data_abort_pending).
    ///
    /// The default delegates to [`read32`](Bus::read32).
    fn fetch32(&mut self, addr: u32) -> u32 {
        self.read32(addr)
    }

    /// Read a 16-bit halfword for a **Thumb instruction fetch**.
    /// Implementations may set [`prefetch_abort_pending`](Bus::prefetch_abort_pending)
    /// on a bus fault.
    ///
    /// The default delegates to [`read16`](Bus::read16).
    fn fetch16(&mut self, addr: u32) -> u16 {
        self.read16(addr)
    }

    /// Returns `true` if the most recent **instruction fetch** (via
    /// [`fetch32`](Bus::fetch32) / [`fetch16`](Bus::fetch16)) caused a
    /// prefetch abort, and **clears** the pending flag so subsequent calls
    /// return `false` until a new fault occurs (consuming semantics).
    ///
    /// The default returns `false`, which is correct for GBA hardware where
    /// all fetches succeed.
    fn prefetch_abort_pending(&mut self) -> bool {
        false
    }

    /// Returns `true` if the most recent **data access** (load or store via
    /// [`read32`](Bus::read32) / [`write32`](Bus::write32) / etc.) caused a
    /// data abort, and **clears** the pending flag so subsequent calls return
    /// `false` until a new fault occurs (consuming semantics).
    ///
    /// The default returns `false`, which is correct for GBA hardware where
    /// all accesses succeed.
    fn data_abort_pending(&mut self) -> bool {
        false
    }
}

/// Flat, little-endian RAM-only bus used in unit tests and as a stub for the
/// boot-sequence smoke test. All addresses are taken modulo the RAM size.
#[derive(Debug, Clone)]
pub struct RamBus {
    bytes: Vec<u8>,
}

impl RamBus {
    /// Create a new RAM bus with the given size in bytes (zero-initialised).
    ///
    /// # Panics
    ///
    /// Panics if `size` is zero. Subsequent reads/writes use the size as a
    /// modulus, so a zero-sized bus would panic with a divide-by-zero on
    /// every access; reject it here instead with a clear message.
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "RamBus size must be greater than zero");
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

    fn n_cycles(&self, _addr: u32, _width: WidthClass) -> u32 {
        1
    }

    fn s_cycles(&self, _addr: u32, _width: WidthClass) -> u32 {
        1
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

    #[test]
    #[should_panic(expected = "RamBus size must be greater than zero")]
    fn ram_bus_rejects_zero_size() {
        let _ = RamBus::new(0);
    }

    #[test]
    fn ram_bus_n_cycles_returns_1() {
        let bus = RamBus::new(0x100);
        assert_eq!(bus.n_cycles(0x0000_0000, WidthClass::HalfwordOrByte), 1);
        assert_eq!(bus.n_cycles(0x0800_0000, WidthClass::Word), 1);
    }

    #[test]
    fn ram_bus_s_cycles_returns_1() {
        let bus = RamBus::new(0x100);
        assert_eq!(bus.s_cycles(0x0000_0000, WidthClass::HalfwordOrByte), 1);
        assert_eq!(bus.s_cycles(0x0800_0000, WidthClass::Word), 1);
    }
}
