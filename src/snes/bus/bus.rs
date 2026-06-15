//! SNES bus trait, StubBus, and TestBus implementations.

/// SNES bus interface for memory reads, writes, and cycle advancement.
///
/// This trait defines the contract for all SNES bus implementations,
/// mirroring the pattern used in `GbBus` and `GbaBus`.
pub trait SnesBus {
    /// Read a byte from the given address.
    fn read(&self, addr: u32) -> u8;

    /// Write a byte to the given address.
    fn write(&mut self, addr: u32, value: u8);

    /// Advance the bus state by one master clock cycle.
    fn tick(&mut self);
}

/// Stub bus implementation for unit tests.
///
/// Returns 0xFF for all reads and ignores all writes.
#[derive(Debug)]
pub struct StubBus;

impl SnesBus for StubBus {
    fn read(&self, _addr: u32) -> u8 {
        0xFF
    }

    fn write(&mut self, _addr: u32, _value: u8) {
        // No-op
    }

    fn tick(&mut self) {
        // No-op
    }
}

/// Configurable bus for unit tests.
///
/// Backs the full 24-bit address space (16 MB) so addressing modes
/// and opcodes can be tested by pre-loading specific memory locations.
#[cfg(test)]
pub struct TestBus {
    mem: Vec<u8>,
}

#[cfg(test)]
impl TestBus {
    /// Create a new `TestBus` with all memory zeroed.
    pub fn new() -> Self {
        Self {
            mem: vec![0u8; 0x100_0000], // 16 MB
        }
    }

    /// Write a contiguous slice of bytes starting at `addr`.
    ///
    /// # Panics
    /// Panics if `addr + data.len()` exceeds the 24-bit address space.
    pub fn load(&mut self, addr: u32, data: &[u8]) {
        let addr = (addr & 0xFF_FFFF) as usize;
        let end = addr
            .checked_add(data.len())
            .expect("TestBus::load: address + length overflows");
        assert!(
            end <= self.mem.len(),
            "TestBus::load: addr {addr:#08X} + len {} overflows 24-bit address space",
            data.len()
        );
        self.mem[addr..end].copy_from_slice(data);
    }
}

#[cfg(test)]
impl Default for TestBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl SnesBus for TestBus {
    fn read(&self, addr: u32) -> u8 {
        assert!(
            addr <= 0xFF_FFFF,
            "TestBus::read: addr {addr:#08X} out of 24-bit range"
        );
        self.mem[addr as usize]
    }

    fn write(&mut self, addr: u32, value: u8) {
        assert!(
            addr <= 0xFF_FFFF,
            "TestBus::write: addr {addr:#08X} out of 24-bit range"
        );
        self.mem[addr as usize] = value;
    }

    fn tick(&mut self) {
        // Intentional no-op in test bus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_bus_read_returns_0xff() {
        let bus = StubBus;
        assert_eq!(bus.read(0x0000), 0xFF);
        assert_eq!(bus.read(0x7FFF), 0xFF);
        assert_eq!(bus.read(0xFFFF), 0xFF);
    }

    #[test]
    fn stub_bus_write_does_not_panic() {
        let mut bus = StubBus;
        bus.write(0x0000, 0x42);
        bus.write(0x7FFF, 0x00);
    }

    #[test]
    fn stub_bus_tick_does_not_panic() {
        let mut bus = StubBus;
        bus.tick();
    }

    #[test]
    fn test_bus_new_returns_zeroed_memory() {
        let bus = TestBus::new();
        assert_eq!(bus.read(0x00_0000), 0x00);
        assert_eq!(bus.read(0x7F_FFFF), 0x00);
        assert_eq!(bus.read(0xFF_FFFF), 0x00);
    }

    #[test]
    fn test_bus_write_and_read_roundtrip() {
        let mut bus = TestBus::new();
        bus.write(0x00_1234, 0xAB);
        assert_eq!(bus.read(0x00_1234), 0xAB);
    }

    #[test]
    fn test_bus_write_does_not_affect_adjacent_bytes() {
        let mut bus = TestBus::new();
        bus.write(0x00_0010, 0xFF);
        assert_eq!(bus.read(0x00_000F), 0x00);
        assert_eq!(bus.read(0x00_0011), 0x00);
    }

    #[test]
    fn test_bus_write_highest_address() {
        let mut bus = TestBus::new();
        bus.write(0xFF_FFFF, 0x55);
        assert_eq!(bus.read(0xFF_FFFF), 0x55);
    }

    #[test]
    fn test_bus_load_fills_contiguous_bytes() {
        let mut bus = TestBus::new();
        bus.load(0x00_8000, &[0x01, 0x02, 0x03]);
        assert_eq!(bus.read(0x00_8000), 0x01);
        assert_eq!(bus.read(0x00_8001), 0x02);
        assert_eq!(bus.read(0x00_8002), 0x03);
    }

    #[test]
    fn test_bus_overwrite_existing_byte() {
        let mut bus = TestBus::new();
        bus.write(0x01_0000, 0x10);
        bus.write(0x01_0000, 0x20);
        assert_eq!(bus.read(0x01_0000), 0x20);
    }
}
