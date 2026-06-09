//! SNES bus architecture and memory access.

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
}
