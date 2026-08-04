//! SNES bus trait, StubBus, and TestBus implementations.

/// SNES bus interface for memory reads, writes, and cycle advancement.
///
/// This trait defines the contract for all SNES bus implementations,
/// mirroring the pattern used in `GbBus` and `GbaBus`.
pub trait SnesBus {
    /// Read a byte from the given address.
    fn read(&self, addr: u32) -> u8;

    /// Read a byte for debugging without mutating bus-visible state.
    ///
    /// Implementations should avoid side effects such as MDR updates, register
    /// acknowledgements, or address latching. The default falls back to `read`.
    fn read_for_debugger(&self, addr: u32) -> u8 {
        self.read(addr)
    }

    /// Write a byte to the given address.
    fn write(&mut self, addr: u32, value: u8);

    /// Advance the bus state by one master clock cycle.
    fn tick(&mut self);

    /// Called by the CPU at the start of every CPU cycle (memory access or internal), before
    /// any of that cycle's `tick`s. Buses that model the DMA start delay run a pending
    /// transfer here, one full CPU cycle after the `$420B` write (Mesen2
    /// `ProcessPendingTransfers`' `_dmaStartDelay`).
    ///
    /// **Returns whether a transfer actually ran in this cycle**, which is exactly the return
    /// value Mesen2 assigns to `_state.IrqLock`:
    ///
    /// ```text
    /// _state.IrqLock = _dmaController->ProcessPendingTransfers();   // SnesCpu.cpp
    /// ```
    ///
    /// The CPU uses it to lock interrupt recognition for that one cycle -- while a transfer
    /// holds the bus the 65816 is not clocking cycles, so it cannot notice a line that
    /// asserts during it. Default: `false` (no DMA modelled).
    fn gpdma_cycle_hook(&mut self) -> bool {
        false
    }

    /// Poll for a pending NMI edge from the bus (e.g. PPU VBlank NMI), returning the
    /// recognition-arm delay the edge carries in CPU cycles, or `0` for no edge.
    ///
    /// NMI is edge-triggered: this reports each rising edge once and consumes it. The
    /// delay mirrors Mesen2's `SnesCpu::SetNmiFlag(delay)` callers: the PPU's own
    /// vblank edge arms with 1 (`InternalRegisters.h`, `ProcessIrqCounters`), while an
    /// NMITIMEN write enabling NMI mid-vblank arms with 2 (`InternalRegisters.cpp`,
    /// case `0x4200`) -- hardware-verified by byuu's `test_nmi` v1.1 test 27, where a
    /// 16-bit `STA $4200` lands the enabling write on the store's second-to-last
    /// cycle and the NMI must still let the following instruction complete (#3081).
    /// The default returns `0` for buses without an NMI source (test buses).
    fn poll_nmi(&mut self) -> u8 {
        0
    }

    /// Poll whether an IRQ is currently visible to the CPU for dispatch/WAI-wake purposes
    /// (e.g. the PPU H/V timer IRQ).
    ///
    /// IRQ is level-triggered, but implementations may model a real-hardware pipeline delay
    /// between the underlying IRQ source's line level and when the CPU actually notices it
    /// (see `SnesSystemBus::poll_irq`, which gates on `Ppu::poll_irq_dispatch`). Register
    /// reads that expose the raw line (e.g. TIMEUP `$4211`) are unaffected by that delay and
    /// must not use this method. The default returns `false` for buses without an IRQ source.
    fn poll_irq(&self) -> bool {
        false
    }

    /// Tell the bus how many master clocks the CPU cycle that is about to run will take
    /// (6, 8 or 12 for a memory access; 6 for an internal cycle).
    ///
    /// This mirrors Mesen2's `SnesMemoryManager::SetCpuSpeed`, which `SnesCpu::Read`/`Write`
    /// call *before* `ProcessCpuCycle` -- so a DMA that runs at the start of this cycle ends
    /// its `SyncEndDma` pad on a whole cycle of the *upcoming* access (see
    /// `DmaController::sync_end_pad`, #3050). Buses that don't model DMA ignore it.
    fn set_cpu_speed(&mut self, _speed: u8) {}

    /// The cumulative master-clock count, used only to stamp trace lines so a NESER bus
    /// trace can be diffed clock-for-clock against a reference emulator (#3050). Buses
    /// without a clock source report 0.
    fn master_clock(&self) -> u64 {
        0
    }

    /// Return the active screen dimensions for the current frame.
    ///
    /// Buses without a video source use the default SNES visible size.
    fn screen_dimensions(&self) -> (u32, u32) {
        (256, 224)
    }
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
/// Also counts `tick()` calls for verifying master-clock timing.
#[cfg(test)]
pub struct TestBus {
    mem: Vec<u8>,
    tick_count: u64,
}

#[cfg(test)]
impl TestBus {
    /// Create a new `TestBus` with all memory zeroed.
    pub fn new() -> Self {
        Self {
            mem: vec![0u8; 0x100_0000], // 16 MB
            tick_count: 0,
        }
    }

    /// Returns the total number of master clock ticks recorded.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
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
        self.tick_count += 1;
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

    #[test]
    fn test_bus_tick_count_starts_at_zero() {
        let bus = TestBus::new();
        assert_eq!(bus.tick_count(), 0);
    }

    #[test]
    fn test_bus_tick_increments_count() {
        let mut bus = TestBus::new();
        bus.tick();
        assert_eq!(bus.tick_count(), 1);
        bus.tick();
        bus.tick();
        assert_eq!(bus.tick_count(), 3);
    }
}
