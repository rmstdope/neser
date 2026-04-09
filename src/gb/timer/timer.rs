/// SM83 timer subsystem.
///
/// Registers:
/// - `$FF03` / `$FF04` — DIV: increments every 256 T-cycles (64 M-cycles).
///   Writing any value resets DIV to 0.
/// - `$FF05` — TIMA: timer counter; incremented at rate set by TAC.
///   On overflow (wraps past 0xFF), it is reloaded with TMA and IF bit 2 is set.
/// - `$FF06` — TMA: timer modulo (reload value on TIMA overflow).
/// - `$FF07` — TAC: timer control.
///   - Bit 2: Timer enable (1 = enabled).
///   - Bits 1-0: Input clock select.
///     - 00: CPU clock / 1024 (262144 Hz → 4096 Hz at 4 MHz)
///     - 01: CPU clock / 16  (262144 Hz → 262144 Hz)
///     - 10: CPU clock / 64  (262144 Hz → 65536 Hz)
///     - 11: CPU clock / 256 (262144 Hz → 16384 Hz)
///
/// All T-cycle counts below are expressed in M-cycles (1 M = 4 T).
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

/// M-cycle thresholds at which TIMA increments for each TAC clock select.
/// TAC bits 1-0: 00→256 M, 01→4 M, 10→16 M, 11→64 M.
const CLOCK_DIVS: [u16; 4] = [256, 4, 16, 64];

impl Timer {
    pub fn new() -> Self {
        Self {
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            interrupt_pending: false,
        }
    }

    /// Read a timer register by address.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF03 | 0xFF04 => (self.div_counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8, // upper bits read as 1
            _ => 0xFF,
        }
    }

    /// Write a timer register by address.
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF03 | 0xFF04 => self.div_counter = 0, // any write resets DIV
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => self.tac = val & 0x07,
            _ => {}
        }
    }

    /// Advance the timer by `m_cycles` M-cycles.
    ///
    /// Sets `interrupt_pending` when TIMA overflows (caller must propagate
    /// to IF register $FF0F bit 2).
    pub fn tick(&mut self, m_cycles: u8) {
        for _ in 0..m_cycles {
            self.div_counter = self.div_counter.wrapping_add(1);

            let enabled = self.tac & 0x04 != 0;
            if !enabled {
                continue;
            }

            let threshold = CLOCK_DIVS[(self.tac & 0x03) as usize];
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
    fn test_div_increments_every_256_m_cycles() {
        let mut timer = Timer::new();
        for _ in 0..255 {
            timer.tick(1);
        }
        assert_eq!(
            timer.read(0xFF04),
            0,
            "DIV should not increment before 256 M-cycles"
        );
        timer.tick(1); // 256th cycle
        assert_eq!(timer.read(0xFF04), 1, "DIV should be 1 after 256 M-cycles");
    }

    #[test]
    fn test_writing_div_resets_it_to_zero() {
        let mut timer = Timer::new();
        for _ in 0..256 {
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
        // TAC=0x04 (enabled, 00 = div 256 M-cycles)
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x04);
        for _ in 0..255 {
            timer.tick(1);
        }
        assert_eq!(timer.tima, 0, "TIMA should not increment before threshold");
        timer.tick(1); // 256th cycle
        assert_eq!(
            timer.tima, 1,
            "TIMA should be 1 after 256 M-cycles with TAC=0x04"
        );
    }

    #[test]
    fn test_tima_increments_at_rate_set_by_tac_01() {
        // TAC=0x05 (enabled, 01 = div 4 M-cycles)
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
