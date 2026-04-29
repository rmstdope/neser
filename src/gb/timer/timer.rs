use serde::{Deserialize, Serialize};

/// SM83 timer subsystem.
///
/// Registers:
/// - `$FF04` — DIV: the upper 8 bits of an internal 16-bit T-cycle counter.
///   Increments every 256 T-cycles (64 M-cycles). Writing any value resets the
///   full internal counter to 0. `$FF03` is unmapped (returns 0xFF on read;
///   writes are ignored).
/// - `$FF05` — TIMA: timer counter; incremented at rate set by TAC.
///   On overflow, TIMA stays $00 for one M-cycle (cycle A), then is reloaded
///   with TMA and IF bit 2 is set (cycle B).
/// - `$FF06` — TMA: timer modulo (reload value on TIMA overflow).
/// - `$FF07` — TAC: timer control.
///   - Bit 2: Timer enable (1 = enabled).
///   - Bits 1-0: Input clock select.
///     - 00: every 1024 T-cycles  (4096 Hz at 4.194 MHz)
///     - 01: every 16   T-cycles  (262144 Hz)
///     - 10: every 64   T-cycles  (65536 Hz)
///     - 11: every 256  T-cycles  (16384 Hz)
///
/// ## Obscure behaviours (Pan Docs — Timer Obscure Behaviour)
///
/// The TIMA increment is triggered by the **falling edge** of a specific bit of
/// the system counter, ANDed with TAC.enable on DMG hardware.  This has three
/// important side-effects:
///
/// 1. **DIV write**: resetting the counter to 0 forces a falling edge if the
///    selected bit was HIGH → TIMA increments (if timer is enabled).
/// 2. **TAC write**: if the AND(selected_bit, enable) was HIGH and the write
///    makes it LOW (e.g., disabling the timer, or switching clock to a currently-
///    LOW bit) → TIMA increments once.
/// 3. **TIMA overflow delay**: when TIMA overflows, it stays $00 for one M-cycle
///    before TMA is loaded and the interrupt fires.  Writing to TIMA during that
///    M-cycle cancels the reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// True during the M-cycle after TIMA overflows (cycle A).
    /// A CPU write to TIMA while this is true cancels the pending TMA reload.
    tima_overflow_pending: bool,
    /// True during the M-cycle when TMA is loaded into TIMA (cycle B).
    /// CPU writes to TIMA during this cycle are ignored; writes to TMA also
    /// update TIMA.
    tima_load_active: bool,
    /// DIV-APU bit used for frame sequencer clocking.
    /// On DMG/CGB normal speed: bit 12 (DIV bit 4).
    /// On CGB double-speed: bit 13 (DIV bit 5).
    /// The APU frame sequencer is clocked on the falling edge of this bit.
    div_apu_bit: u16,
}

/// Bit of the 16-bit system counter that the TIMA multiplexer selects for each
/// TAC clock-select value.  TIMA increments on the falling edge of this bit
/// (ANDed with TAC.enable).
///
/// TAC bits 1-0:  00 → bit 9 (1024 T-cycle period)
///                01 → bit 3 (  16 T-cycle period)
///                10 → bit 5 (  64 T-cycle period)
///                11 → bit 7 ( 256 T-cycle period)
const MUX_BIT: [u16; 4] = [1 << 9, 1 << 3, 1 << 5, 1 << 7];

/// DIV-APU bit for normal speed (DMG / CGB single-speed).
/// Bit 12 of the 16-bit counter = DIV bit 4.
/// The APU frame sequencer steps on the falling edge of this bit (512 Hz).
pub const DIV_APU_BIT_NORMAL: u16 = 1 << 12;

/// DIV-APU bit for CGB double-speed mode.
/// Bit 13 of the 16-bit counter = DIV bit 5.
/// In double-speed mode, the CPU runs at 2x but APU timing stays the same,
/// so the APU uses the next higher bit to maintain 512 Hz.
pub const DIV_APU_BIT_DOUBLE: u16 = 1 << 13;

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
            tima_overflow_pending: false,
            tima_load_active: false,
            div_apu_bit: DIV_APU_BIT_NORMAL,
        }
    }

    /// Set the DIV-APU bit mask for CGB double-speed mode switching.
    ///
    /// Call with `DIV_APU_BIT_DOUBLE` when entering double-speed mode,
    /// or `DIV_APU_BIT_NORMAL` when returning to normal speed.
    pub fn set_div_apu_bit(&mut self, bit: u16) {
        self.div_apu_bit = bit;
    }

    /// Check if the DIV-APU bit is currently HIGH.
    ///
    /// Used by the bus when writing NR52 to determine if the APU power-on
    /// should skip the first DIV-APU event.
    pub fn is_div_apu_bit_high(&self) -> bool {
        self.div_counter & self.div_apu_bit != 0
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

    /// Set the raw 16-bit internal counter value.
    ///
    /// Used to initialize the DIV counter to post-boot-ROM values for
    /// different hardware models. DIV reads the upper 8 bits, so
    /// `set_div_counter(0x0400)` sets DIV=$04.
    ///
    /// Note: This bypasses normal DIV write side-effects (TAC edge detection).
    /// Use only for bus-level initialization, not runtime writes.
    pub(crate) fn set_div_counter(&mut self, value: u16) {
        self.div_counter = value;
    }

    /// Write a timer register by address.
    ///
    /// Handles the obscure falling-edge side-effects for DIV and TAC writes,
    /// and the TIMA-write/TMA-write interactions during the overflow delay.
    ///
    /// Returns `true` if a DIV-APU falling edge occurred (only possible for DIV writes).
    /// The caller should notify the APU to step its frame sequencer.
    pub fn write(&mut self, addr: u16, val: u8) -> bool {
        match addr {
            0xFF04 => {
                // Writing DIV resets the full system counter to 0.
                // If the currently selected mux bit was HIGH (and timer is enabled),
                // the reset forces a falling edge → TIMA increments.
                if self.mux_and_high() {
                    self.do_tima_increment();
                }
                // Check for DIV-APU falling edge: if the DIV-APU bit was HIGH,
                // resetting to 0 forces it LOW → falling edge → APU event.
                let div_apu_falling_edge = self.div_counter & self.div_apu_bit != 0;
                self.div_counter = 0;
                div_apu_falling_edge
            }
            0xFF05 => {
                // Writing TIMA during cycle A (overflow pending but not yet loaded)
                // cancels the reload: the written value stays, no interrupt fires.
                self.tima_overflow_pending = false;
                // Writes during cycle B (tima_load_active) are ignored: TMA already won.
                if !self.tima_load_active {
                    self.tima = val;
                }
                false
            }
            0xFF06 => {
                self.tma = val;
                // If TMA is written during cycle B, TIMA also takes the new value.
                if self.tima_load_active {
                    self.tima = val;
                }
                false
            }
            0xFF07 => {
                // If the AND gate output falls from HIGH to LOW, TIMA increments once.
                let was_high = self.mux_and_high();
                self.tac = val & 0x07;
                if was_high && !self.mux_and_high() {
                    self.do_tima_increment();
                }
                false
            }
            _ => false, // 0xFF03 and all other addresses are unmapped; writes ignored
        }
    }

    /// Advance the timer by `m_cycles` M-cycles.
    ///
    /// Internally the timer steps one T-cycle at a time for correct falling-edge
    /// detection.  Sets `interrupt_pending` when TIMA overflows (caller must
    /// propagate to IF register $FF0F bit 2).
    ///
    /// Returns the number of DIV-APU falling edges that occurred during this tick.
    /// The caller should notify the APU to step its frame sequencer for each edge.
    pub fn tick(&mut self, m_cycles: u8) -> u8 {
        let mut div_apu_edges: u8 = 0;

        for _ in 0..m_cycles {
            // Clear the load-active flag from the previous M-cycle.
            self.tima_load_active = false;

            // Cycle B: fire the pending TMA→TIMA reload and set the interrupt.
            if self.tima_overflow_pending {
                self.tima = self.tma;
                self.interrupt_pending = true;
                self.tima_overflow_pending = false;
                self.tima_load_active = true;
            }

            let enabled = self.tac & 0x04 != 0;
            let bit = MUX_BIT[(self.tac & 0x03) as usize];

            // Advance the system counter one T-cycle at a time, detecting falling
            // edges on the multiplexer bit and the DIV-APU bit.
            for _ in 0..4 {
                let old = self.div_counter;
                self.div_counter = self.div_counter.wrapping_add(1);

                // Check for TIMA falling edge (timer enabled AND selected bit HIGH→LOW).
                if enabled && old & bit != 0 && self.div_counter & bit == 0 {
                    self.do_tima_increment();
                }

                // Check for DIV-APU falling edge (bit HIGH→LOW).
                if old & self.div_apu_bit != 0 && self.div_counter & self.div_apu_bit == 0 {
                    div_apu_edges += 1;
                }
            }
        }

        div_apu_edges
    }

    /// Returns true when the AND-gate output (TAC.enable AND selected mux bit) is HIGH.
    ///
    /// A falling edge on this signal (HIGH → LOW) triggers a TIMA increment.
    fn mux_and_high(&self) -> bool {
        self.tac & 0x04 != 0 && self.div_counter & MUX_BIT[(self.tac & 0x03) as usize] != 0
    }

    /// Increment TIMA by one, handling overflow into the 1-M-cycle reload delay.
    fn do_tima_increment(&mut self) {
        let (new_val, overflow) = self.tima.overflowing_add(1);
        if overflow {
            self.tima = 0x00;
            self.tima_overflow_pending = true;
        } else {
            self.tima = new_val;
        }
    }

    /// Fire any pending TIMA reload and return `true` if the timer interrupt should
    /// be set in IF.
    ///
    /// This must be called by the bus immediately after every timer-register write.
    /// Any `tima_overflow_pending` at that point is necessarily write-triggered (a
    /// normal tick overflow would have already been propagated via `tick()` before
    /// the write executes).  Calling this after the write compensates for the fact
    /// that our M-cycle model runs the bus tick *before* the register write, so no
    /// post-write T-cycles naturally fire the reload.
    ///
    /// This mirrors SameBoy's `flush_pending_cycles` / `advance_tima_state_machine`
    /// called at instruction end: the overflow fires within the same instruction slot,
    /// making the interrupt visible to `service_interrupts()` at the start of the
    /// next instruction.
    pub(crate) fn fire_write_overflow_if_pending(&mut self) -> bool {
        if self.tima_overflow_pending {
            self.tima = self.tma;
            self.tima_overflow_pending = false;
            self.tima_load_active = true;
            self.interrupt_pending = true;
            return true;
        }
        false
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
        // TAC=0x05 (clock 01: every 16 T-cycles = 4 M-cycles), TMA=0x10.
        // The increment fires at the end of M-cycle 4.  The TMA reload and
        // interrupt fire at the START of M-cycle 5 (1 M-cycle obscure delay).
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x05);
        timer.write(0xFF06, 0x10); // TMA = 0x10
        timer.tima = 0xFF; // one more increment causes overflow

        timer.tick(4); // increment fires; TIMA goes to 0x00, overflow pending
        assert_eq!(
            timer.tima, 0x00,
            "TIMA should be 0x00 immediately after overflow"
        );
        assert!(
            !timer.interrupt_pending,
            "interrupt fires on the next M-cycle"
        );

        timer.tick(1); // cycle B: TMA reload fires
        assert_eq!(timer.tima, 0x10, "TIMA should reload from TMA on cycle B");
        assert!(
            timer.interrupt_pending,
            "Interrupt should be pending after TIMA reload"
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
