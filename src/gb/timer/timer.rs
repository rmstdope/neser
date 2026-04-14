/// SM83 timer subsystem.
///
/// Registers:
/// - `$FF04` — DIV: the upper 8 bits of an internal 16-bit T-cycle counter.
///   Increments every 256 T-cycles (64 M-cycles). Writing any value resets the
///   full internal counter to 0. `$FF03` is unmapped (returns 0xFF on read;
///   writes are ignored).
/// - `$FF05` — TIMA: timer counter; incremented at rate set by TAC.
///   On overflow (wraps past 0xFF), it is reloaded with TMA and IF bit 2 is set.
/// - `$FF06` — TMA: timer modulo (reload value on TIMA overflow).
/// - `$FF07` — TAC: timer control.
///   - Bit 2: Timer enable (1 = enabled).
///   - Bits 1-0: Input clock select.
///     - 00: every 1024 T-cycles  (4096 Hz at 4.194 MHz)
///     - 01: every 16   T-cycles  (262144 Hz)
///     - 10: every 64   T-cycles  (65536 Hz)
///     - 11: every 256  T-cycles  (16384 Hz)
pub struct Timer {
    /// Internal 16-bit counter; DIV is the upper 8 bits ($FF04 = div_counter >> 8).
    div_counter: u16,
    /// TIMA register ($FF05).
    pub tima: u8,
    /// TMA register ($FF06).
    pub tma: u8,
    /// TAC register ($FF07).
    pub tac: u8,
    /// Pending IF bit-2 set after TIMA overflow.
    pub interrupt_pending: bool,
}

/// T-cycle thresholds at which TIMA increments for each TAC clock select.
/// TAC bits 1-0: 00→1024 T, 01→16 T, 10→64 T, 11→256 T.
const CLOCK_DIVS_T: [u16; 4] = [1024, 16, 64, 256];

impl Timer {
    pub fn new() -> Self {
        Self::with_div_counter(0)
    }

    /// Create a Timer with a specific initial `div_counter` value.
    ///
    /// Used by `DmgBus` to compensate for the sub-byte phase difference between
    /// our custom boot ROM and real DMG-B hardware, ensuring serial clock edges
    /// align correctly for acceptance tests.
    pub(crate) fn with_div_counter(div_counter: u16) -> Self {
        Self {
            div_counter,
            tima: 0,
            tma: 0,
            tac: 0,
            interrupt_pending: false,
        }
    }

    /// Read a timer register by address.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.div_counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8, // upper bits read as 1
            _ => 0xFF,                 // 0xFF03 and all other addresses are unmapped
        }
    }

    /// Return the raw 16-bit internal counter value.
    ///
    /// Used by the serial port to inspect the same divider state that
    /// `DmgBus` derives serial clock timing from via its `0x080` mask
    /// (falling edge of bit 7 = the 8192 Hz serial bit-clock base).
    pub fn raw_counter(&self) -> u16 {
        self.div_counter
    }

    /// Write a timer register by address.
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF04 => self.div_counter = 0, // any write resets the full internal counter
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => self.tac = val & 0x07,
            _ => {} // 0xFF03 and all other addresses are unmapped; writes ignored
        }
    }

    /// Advance the timer by `m_cycles` M-cycles.
    ///
    /// Internally the timer runs at T-cycle granularity (4 T per M-cycle).
    /// Sets `interrupt_pending` when TIMA overflows (caller must propagate
    /// to IF register $FF0F bit 2).
    pub fn tick(&mut self, m_cycles: u8) {
        for _ in 0..m_cycles {
            // The internal counter tracks T-cycles; advance by 4 per M-cycle.
            self.div_counter = self.div_counter.wrapping_add(4);

            let enabled = self.tac & 0x04 != 0;
            if !enabled {
                continue;
            }

            let threshold = CLOCK_DIVS_T[(self.tac & 0x03) as usize];
            if self.div_counter.is_multiple_of(threshold) {
                let (new_tima, overflow) = self.tima.overflowing_add(1);
                if overflow {
                    self.tima = self.tma;
                    self.interrupt_pending = true;
                } else {
                    self.tima = new_tima;
                }
            }
        }
    }

    /// Take the pending interrupt flag (clears it).
    pub fn take_interrupt(&mut self) -> bool {
        let p = self.interrupt_pending;
        self.interrupt_pending = false;
        p
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_div_increments_every_64_m_cycles() {
        // DIV increments every 256 T-cycles = 64 M-cycles.
        let mut timer = Timer::new();
        for _ in 0..63 {
            timer.tick(1);
        }
        assert_eq!(
            timer.read(0xFF04),
            0,
            "DIV should not increment before 64 M-cycles"
        );
        timer.tick(1); // 64th M-cycle = 256 T-cycles
        assert_eq!(timer.read(0xFF04), 1, "DIV should be 1 after 64 M-cycles");
    }

    #[test]
    fn test_ff03_read_returns_open_bus() {
        let timer = Timer::new();
        assert_eq!(
            timer.read(0xFF03),
            0xFF,
            "$FF03 is unmapped and should return 0xFF"
        );
    }

    #[test]
    fn test_writing_div_resets_it_to_zero() {
        let mut timer = Timer::new();
        for _ in 0..64 {
            timer.tick(1);
        }
        assert_eq!(timer.read(0xFF04), 1);
        timer.write(0xFF04, 0x42); // any write resets DIV
        assert_eq!(timer.read(0xFF04), 0, "DIV should be 0 after any write");
    }

    #[test]
    fn test_tima_does_not_increment_when_timer_disabled() {
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x00); // TAC: disable, clock = 1024 M-cycles
        for _ in 0..1024 {
            timer.tick(1);
        }
        assert_eq!(
            timer.tima, 0,
            "TIMA should not increment when timer is disabled"
        );
    }

    #[test]
    fn test_tima_increments_at_rate_set_by_tac_00() {
        // TAC=0x04 (enabled, clock 00 = every 1024 T-cycles = 256 M-cycles)
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x04);
        for _ in 0..255 {
            timer.tick(1);
        }
        assert_eq!(timer.tima, 0, "TIMA should not increment before threshold");
        timer.tick(1); // 256th M-cycle = 1024 T-cycles
        assert_eq!(
            timer.tima, 1,
            "TIMA should be 1 after 256 M-cycles (1024 T) with TAC=0x04"
        );
    }

    #[test]
    fn test_tima_increments_at_rate_set_by_tac_01() {
        // TAC=0x05 (enabled, clock 01 = every 16 T-cycles = 4 M-cycles)
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x05);
        for _ in 0..3 {
            timer.tick(1);
        }
        assert_eq!(timer.tima, 0);
        timer.tick(1); // 4th cycle
        assert_eq!(timer.tima, 1);
    }

    #[test]
    fn test_tima_overflow_reloads_tma_and_sets_interrupt() {
        // TAC=0x05 (div 4), TMA=0x10
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x05);
        timer.write(0xFF06, 0x10); // TMA = 0x10
        timer.tima = 0xFF; // one more increment causes overflow
        timer.tick(4); // trigger one increment
        assert_eq!(timer.tima, 0x10, "TIMA should reload from TMA on overflow");
        assert!(
            timer.interrupt_pending,
            "Interrupt should be pending after TIMA overflow"
        );
    }

    #[test]
    fn test_take_interrupt_clears_pending_flag() {
        let mut timer = Timer::new();
        timer.interrupt_pending = true;
        assert!(timer.take_interrupt());
        assert!(!timer.interrupt_pending);
    }
}
