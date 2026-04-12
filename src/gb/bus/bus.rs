/// Minimal bus interface for the SM83 CPU.
///
/// Implementors provide `read` and `write` over the 16-bit address space.
/// A `StubBus` (always returns 0xFF) is provided for isolated unit tests.
pub trait GbBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);

    /// Advance system peripherals by `m_cycles` M-cycles.
    ///
    /// The default implementation is a no-op, used by `StubBus` and test buses.
    /// `DmgBus` overrides this to tick the timer and propagate interrupts.
    fn tick(&mut self, _m_cycles: u8) {}

    /// Notify the bus that the CPU is about to fetch and execute a new instruction.
    ///
    /// Called BEFORE the M1 bus tick.  The default implementation is a no-op;
    /// bus implementations may override it for per-instruction setup.
    fn begin_instruction(&mut self) {}

    /// Notify the bus that the CPU's Increment/Decrement Unit (IDU) performed a
    /// 16-bit increment or decrement while the register pointed at `addr`.
    ///
    /// On DMG hardware the IDU asserts the address bus during its operation even
    /// when no explicit read/write is taking place.  If `addr` falls in the OAM
    /// region ($FE00–$FEFF, which includes the unusable $FEA0–$FEFF range) while
    /// the PPU is in Mode 2, this bus-tickle triggers an OAM write-corruption.
    ///
    /// The default implementation is a no-op (used by `StubBus` and test buses).
    fn notify_idu_glitch(&mut self, _addr: u16) {}

    /// Notify the bus that the CPU performed a read from `addr` in the same
    /// M-cycle as an IDU increment/decrement (e.g., `LD A, [HLI]`).
    ///
    /// If `addr` falls in OAM and the PPU is in Mode 2, this triggers the more
    /// complex "Read During Increase/Decrease" OAM corruption pattern.
    ///
    /// The default implementation is a no-op.
    fn notify_idu_with_prior_read(&mut self, _addr: u16) {}
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
