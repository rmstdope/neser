use crate::gb::ppu::Ppu;

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

    /// Perform one CPU write M-cycle.
    ///
    /// The default keeps the historical behavior: advance all peripherals for
    /// one complete M-cycle, then perform the write. Full bus implementations
    /// can override this when a device needs the write at its bus phase inside
    /// the M-cycle.
    fn write_cpu_m_cycle(&mut self, addr: u16, val: u8) {
        self.tick(1);
        self.write(addr, val);
    }

    /// Perform the bus sample for a CPU read M-cycle after peripherals have ticked.
    ///
    /// The CPU owns the cycle tick so implementations can use this hook for
    /// CPU-only bus effects that do not apply to debugger or helper reads.
    fn read_cpu_m_cycle(&mut self, addr: u16) -> u8 {
        self.read(addr)
    }

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
    /// M-cycle as an IDU increment/decrement (e.g., `LD A, [HLI]`, `POP rr` M2).
    ///
    /// If `addr` falls in the forbidden zone ($FEA0–$FEFF) and the PPU is in
    /// Mode 2, this triggers the "Read During Increase/Decrease" OAM corruption
    /// pattern. Reads from real OAM ($FE00–$FE9F) are already handled by
    /// `Ppu::read_oam`; restricting to $FEA0–$FEFF avoids double-corruption.
    ///
    /// The default implementation is a no-op.
    fn notify_idu_with_prior_read(&mut self, _addr: u16) {}

    /// Notify the bus that the CPU performed a plain OAM read from `addr` in an
    /// M-cycle where no IDU increment/decrement was active (e.g., `POP rr` M3).
    ///
    /// If `addr` falls in the forbidden zone ($FEA0–$FEFF) and the PPU is in
    /// Mode 2, this triggers a plain OAM read corruption. Reads from real OAM
    /// ($FE00–$FE9F) are already handled by `Ppu::read_oam`.
    ///
    /// The default implementation is a no-op.
    fn notify_oam_read(&mut self, _addr: u16) {}

    /// Notify the bus that the CPU performed a plain write to `addr` in an M-cycle
    /// where no IDU decrement was active (e.g., `PUSH rr` M4).
    ///
    /// If `addr` falls in the forbidden zone ($FEA0–$FEFF) and the PPU is in
    /// Mode 2, this triggers an OAM write corruption. Writes to real OAM
    /// ($FE00–$FE9F) are already handled by `Ppu::write_oam`.
    ///
    /// The default implementation is a no-op.
    fn notify_oam_write(&mut self, _addr: u16) {}

    /// Attempt a CGB double-speed switch.
    ///
    /// Returns `true` if the speed switch was performed (KEY1 bit 0 was armed).
    /// Returns `false` if no switch occurred (not armed, or bus does not support it).
    /// The default implementation always returns `false` (DMG / test buses).
    fn try_speed_switch(&mut self) -> bool {
        false
    }

    /// Enter normal STOP display behavior after a STOP instruction that did not
    /// perform a CGB speed switch.
    fn enter_stop_mode(&mut self) {}

    /// Leave normal STOP display behavior after a joypad wake request.
    fn exit_stop_mode(&mut self) {}

    /// Check if the CPU should be halted for HDMA and consume one halt cycle.
    ///
    /// Returns `true` if the CPU should perform a halt stall (tick subsystems
    /// without executing an instruction) for this M-cycle. CgbBus uses this for
    /// HDMA/GDMA transfers which halt the CPU. The default implementation returns
    /// `false` (DMG / test buses have no HDMA).
    fn consume_hdma_halt_cycle(&mut self) -> bool {
        false
    }

    /// Access the PPU for debugger purposes (timing, frame count, scanline).
    ///
    /// Returns a reference to the PPU. The default implementation panics;
    /// only real bus implementations (DmgBus, CgbBus) should be used with the debugger.
    fn ppu(&self) -> &Ppu {
        panic!("ppu() not available on this bus implementation")
    }

    /// Access the PPU mutably for debugger purposes (clearing frame-ready flag).
    ///
    /// Returns a mutable reference to the PPU. The default implementation panics;
    /// only real bus implementations (DmgBus, CgbBus) should be used with the debugger.
    fn ppu_mut(&mut self) -> &mut Ppu {
        panic!("ppu_mut() not available on this bus implementation")
    }

    /// Read memory for debugger purposes without side effects.
    ///
    /// Unlike `read()`, this does not trigger OAM corruption or other hardware quirks.
    /// The default implementation panics; only real bus implementations should be used with the debugger.
    fn read_for_debugger(&self, _addr: u16) -> u8 {
        panic!("read_for_debugger() not available on this bus implementation")
    }
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
