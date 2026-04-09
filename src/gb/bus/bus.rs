/// Minimal bus interface for the SM83 CPU.
///
/// Implementors provide `read` and `write` over the 16-bit address space.
/// A `StubBus` (always returns 0xFF) is provided for isolated unit tests.
pub trait GbBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

/// Bus stub that returns 0xFF for every read and silently discards writes.
///
/// Used in unit tests where memory contents are irrelevant.
pub struct StubBus;

impl GbBus for StubBus {
    fn read(&mut self, _addr: u16) -> u8 {
        0xFF
    }

    fn write(&mut self, _addr: u16, _val: u8) {}
}
