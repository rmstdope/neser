/// Frame Counter for the NES APU
/// Sequences envelope, sweep, and length counter clocks
/// Operates in two modes: 4-step and 5-step
pub struct FrameCounter {
    mode: Mode,
    irq_inhibit: bool,
    cycle_counter: u32,
    irq_flag: bool,
    irq_assert_cycles_remaining: u8,
    five_step_extra_cycle: bool, // Alternating +1 cycle offset for 5-step sequencing
    pending_write: Option<u8>,   // Pending write to $4017 register
    write_delay: u8,             // Cycles remaining before pending write takes effect
    pending_write_on_odd_cpu_cycle: bool,
    pending_immediate_clock: (bool, bool), // Extra quarter/half clocks from delayed $4017 side-effects
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    FourStep,
    FiveStep,
}

impl Default for FrameCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCounter {
    /// Create a new frame counter
    pub fn new() -> Self {
        Self {
            mode: Mode::FourStep,
            irq_inhibit: false,
            cycle_counter: 0,
            irq_flag: false,
            irq_assert_cycles_remaining: 0,
            five_step_extra_cycle: false,
            pending_write: None,
            write_delay: 0,
            pending_write_on_odd_cpu_cycle: false,
            pending_immediate_clock: (false, false),
        }
    }

    /// Write to frame counter register ($4017) immediately (for internal/test use only)
    ///
    /// **Note**: For CPU writes to $4017, use `Apu::write_frame_counter()` instead.
    /// This method applies the write immediately without the 3-4 cycle delay
    /// that real hardware exhibits for CPU writes.
    ///
    /// Bit 7: Mode (0 = 4-step, 1 = 5-step)
    /// Bit 6: IRQ inhibit (1 = disable IRQ)
    pub(crate) fn write_register(&mut self, value: u8) {
        let new_mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };
        self.mode = new_mode;
        self.irq_inhibit = (value & 0x40) != 0;
        self.cycle_counter = 0;
        // Note: Phase not tracked here as we don't know the APU cycle
        self.five_step_extra_cycle = false;
        self.irq_assert_cycles_remaining = 0;

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // If mode is set (5-step), generate immediate quarter+half clocks.
        // This matches NESDev: "Writing to $4017 with bit 7 set will immediately
        // generate a clock for both the quarter frame and the half frame units."
        if new_mode == Mode::FiveStep {
            self.pending_immediate_clock = (true, true);
        }
    }

    /// Get the current mode
    #[cfg(test)]
    pub fn get_mode(&self) -> bool {
        self.mode == Mode::FiveStep
    }

    /// Check if IRQ is inhibited
    #[cfg(test)]
    pub fn is_irq_inhibited(&self) -> bool {
        self.irq_inhibit
    }

    /// Get the current cycle counter
    pub fn get_cycle_counter(&self) -> u32 {
        self.cycle_counter
    }

    /// Get the IRQ flag state
    pub fn get_irq_flag(&self) -> bool {
        self.irq_flag
    }

    /// Clear the IRQ flag
    pub fn clear_irq_flag(&mut self) {
        self.irq_flag = false;
    }

    /// Queue a delayed write to $4017 register
    /// This is used for power-on/reset timing where the write takes effect after a delay
    /// delay: number of CPU cycles before the write takes effect (typically 3-4)
    pub fn queue_delayed_write(&mut self, value: u8, delay: u8) {
        self.pending_write = Some(value);
        self.write_delay = delay;
        self.pending_write_on_odd_cpu_cycle = false;
    }

    /// Queue a delayed write to $4017 register, preserving whether the write occurred on an odd
    /// CPU cycle so we can apply frame-counter jitter when the write takes effect.
    pub fn queue_delayed_write_with_jitter(
        &mut self,
        value: u8,
        delay: u8,
        write_on_odd_cpu_cycle: bool,
    ) {
        self.pending_write = Some(value);
        self.write_delay = delay;
        self.pending_write_on_odd_cpu_cycle = write_on_odd_cpu_cycle;
    }

    /// Process pending delayed write (called at start of each clock cycle)
    ///
    /// Returns true if a pending write took effect on this cycle.
    fn process_delayed_write(&mut self) -> bool {
        if self.pending_write.is_none() {
            return false;
        }

        // NESDev: Effects of a $4017 write occur 3 or 4 CPU cycles later.
        // Interpret write_delay as "cycles remaining until the write takes effect".
        // So we count down each CPU clock and apply on the clock that reaches 0.
        if self.write_delay > 0 {
            self.write_delay -= 1;
            if self.write_delay > 0 {
                return false;
            }
        }

        let value = self.pending_write.expect("checked above");

        let new_mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };

        // Apply the delayed write
        self.mode = new_mode;
        self.irq_inhibit = (value & 0x40) != 0;

        // Reset the 5-step alternating offset each time the sequencer is reset.
        self.five_step_extra_cycle = false;
        self.irq_assert_cycles_remaining = 0;

        // Reset cycle_counter to 0.
        // Note: The jitter effect (odd vs even cycle writes) is handled by the 3 vs 4 cycle
        // delay already. We just reset to 0 here.
        self.cycle_counter = 0;

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // If mode is set (5-step), generate immediate quarter+half clocks at effect time.
        if new_mode == Mode::FiveStep {
            self.pending_immediate_clock = (true, true);
        }

        // Clear the pending write
        self.pending_write = None;
        self.pending_write_on_odd_cpu_cycle = false;

        true
    }

    /// Clock the frame counter by one CPU cycle
    /// Returns (quarter_frame, half_frame) signals
    pub fn clock(&mut self) -> (bool, bool) {
        // Process any pending delayed write before advancing
        let write_took_effect = self.process_delayed_write();

        // Frame counter increments every CPU cycle.
        // Important timing detail: when a delayed $4017 write takes effect on this CPU cycle,
        // the sequencer is reset but does not also immediately advance on the same cycle.
        // This matches blargg's APU timing tests (e.g. apu_test 5-len_timing).
        if !write_took_effect {
            self.cycle_counter = self.cycle_counter.wrapping_add(1);
        }

        let (quarter_frame, half_frame) = match self.mode {
            Mode::FourStep => self.clock_four_step(),
            Mode::FiveStep => self.clock_five_step(),
        };

        let (immediate_quarter, immediate_half) = self.pending_immediate_clock;
        self.pending_immediate_clock = (false, false);

        (
            quarter_frame || immediate_quarter,
            half_frame || immediate_half,
        )
    }

    /// Clock the 4-step sequencer
    ///
    /// # IRQ Semantics (blargg compatibility mode)
    ///
    /// The frame IRQ in 4-step mode is implemented as a 3-cycle "asserting signal" that begins
    /// at cycle 29828 (APU cycle 14914). This behavior differs from a strict reading of NESDev's
    /// `apu_ref.txt`, which describes a simple flag that's set once by the sequencer.
    ///
    /// The multi-cycle assertion model was chosen to pass blargg's `apu_test/6-irq_flag_timing`,
    /// which tests that reading $4015 (which clears the IRQ flag) during the asserting window
    /// will cause the flag to be re-set on subsequent cycles.
    ///
    /// If you're debugging IRQ timing issues and need spec-first behavior, the alternative is:
    /// - Set `irq_flag = true` once at IRQ_CYCLE
    /// - Remove the `irq_assert_cycles_remaining` mechanism
    /// - This will likely break blargg test 6
    fn clock_four_step(&mut self) -> (bool, bool) {
        const STEP_1_CYCLES: u32 = 7457;
        const STEP_2_CYCLES: u32 = 14913;
        const STEP_3_CYCLES: u32 = 22371;
        const STEP_4_CYCLES: u32 = 29829;
        const IRQ_CYCLE: u32 = 29828; // Frame IRQ begins asserting (APU 14914 GET)
        const IRQ_ASSERT_CYCLES: u8 = 3; // How long the internal IRQ signal keeps asserting
        const FRAME_CYCLES: u32 = 29830;

        let quarter_frame = matches!(
            self.cycle_counter,
            STEP_1_CYCLES | STEP_2_CYCLES | STEP_3_CYCLES | STEP_4_CYCLES
        );
        let half_frame = matches!(self.cycle_counter, STEP_2_CYCLES | STEP_4_CYCLES);

        // Start the IRQ asserting window at the designated cycle.
        // See doc comment above for why we use a multi-cycle window instead of a one-shot.
        if self.cycle_counter == IRQ_CYCLE && !self.irq_inhibit {
            self.irq_assert_cycles_remaining = IRQ_ASSERT_CYCLES;
        }

        // While the IRQ signal is asserting, keep (re-)setting the flag each cycle.
        // This allows the flag to be re-set if cleared during the window.
        if self.irq_assert_cycles_remaining > 0 {
            if !self.irq_inhibit {
                self.irq_flag = true;
            }
            self.irq_assert_cycles_remaining -= 1;
        }

        // 4-step sequence length is 29830 CPU cycles.
        if self.cycle_counter >= FRAME_CYCLES {
            self.cycle_counter = 0;
        }

        (quarter_frame, half_frame)
    }

    /// Clock the 5-step sequencer
    /// Mesen2/NESDev: 5-step mode clocks at cycles 7457, 14913, 22371, 29829, 37281
    /// Frame types per Mesen2:
    /// - 7457:  QuarterFrame (envelope only)
    /// - 14913: HalfFrame (envelope + length)
    /// - 22371: QuarterFrame (envelope only)
    /// - 29829: None (no clocks)
    /// - 37281: HalfFrame (envelope + length)
    fn clock_five_step(&mut self) -> (bool, bool) {
        const STEP_1_CYCLES: u32 = 7457;
        const STEP_2_CYCLES: u32 = 14913;
        const STEP_3_CYCLES: u32 = 22371;
        const STEP_4_CYCLES: u32 = 29829;
        const STEP_5_CYCLES: u32 = 37281;

        // The 5-step sequence length is odd, which causes the relative phase to alternate.
        // Model this by shifting the sequence boundaries by +1 CPU cycle every other 5-step run.
        let offset: u32 = self.five_step_extra_cycle as u32;

        let step_1 = STEP_1_CYCLES + offset;
        let step_2 = STEP_2_CYCLES + offset;
        let step_3 = STEP_3_CYCLES + offset;
        let _step_4 = STEP_4_CYCLES + offset; // No clocks at step 4
        let step_5 = STEP_5_CYCLES + offset;

        // Quarter frame (envelope) clocks at steps 1, 2, 3, and 5 (NOT step 4)
        let quarter_frame = self.cycle_counter == step_1
            || self.cycle_counter == step_2
            || self.cycle_counter == step_3
            || self.cycle_counter == step_5;
        // Half frame (length counter) clocks at steps 2 and 5
        let half_frame = self.cycle_counter == step_2 || self.cycle_counter == step_5;

        // Wrap around after step 5
        if self.cycle_counter >= step_5 {
            self.cycle_counter = 0;
            self.five_step_extra_cycle = !self.five_step_extra_cycle;
        }

        (quarter_frame, half_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_counter_new() {
        let fc = FrameCounter::new();
        assert!(!fc.get_mode()); // Default to 4-step (false)
        assert!(!fc.is_irq_inhibited());
        assert_eq!(fc.get_cycle_counter(), 0);
    }

    #[test]
    fn test_write_register_4_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // Mode = 0 (4-step), IRQ inhibit = 0

        assert!(!fc.get_mode()); // 4-step mode
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_5_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // Mode = 1 (5-step), IRQ inhibit = 0

        assert!(fc.get_mode()); // 5-step mode
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_irq_inhibit() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // Mode = 0, IRQ inhibit = 1

        assert!(!fc.get_mode()); // 4-step mode
        assert!(fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_both_flags() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1100_0000); // Mode = 1, IRQ inhibit = 1

        assert!(fc.get_mode()); // 5-step mode
        assert!(fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_resets_cycle_counter() {
        let mut fc = FrameCounter::new();
        fc.cycle_counter = 12345; // Manually set counter

        fc.write_register(0b0000_0000);

        assert_eq!(fc.get_cycle_counter(), 0);
    }

    #[test]
    fn test_write_register_ignores_lower_bits() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0011_1111); // All lower bits set, mode = 0, IRQ inhibit = 0

        assert!(!fc.get_mode());
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_mode_change_from_4_to_5_step() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step
        assert!(!fc.get_mode());

        fc.write_register(0b1000_0000); // 5-step
        assert!(fc.get_mode());
    }

    #[test]
    fn test_mode_change_from_5_to_4_step() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step
        assert!(fc.get_mode());

        fc.write_register(0b0000_0000); // 4-step
        assert!(!fc.get_mode());
    }

    #[test]
    fn test_write_register_5_step_generates_immediate_clock() {
        let mut fc = FrameCounter::new();

        // Writing to $4017 with bit 7 set (5-step mode) should generate
        // immediate quarter+half frame clocks on the next clock() call.
        fc.write_register(0b1000_0000); // 5-step mode

        // The first clock() after the write should include the immediate clocks
        let (quarter, half) = fc.clock();
        assert!(quarter, "5-step mode write should generate immediate quarter frame clock");
        assert!(half, "5-step mode write should generate immediate half frame clock");
    }

    #[test]
    fn test_write_register_4_step_no_immediate_clock() {
        let mut fc = FrameCounter::new();

        // Writing to $4017 with bit 7 clear (4-step mode) should NOT generate
        // immediate quarter+half frame clocks.
        fc.write_register(0b0000_0000); // 4-step mode

        // The first clock() after the write should NOT have immediate clocks
        let (quarter, half) = fc.clock();
        assert!(!quarter, "4-step mode write should not generate immediate quarter frame clock");
        assert!(!half, "4-step mode write should not generate immediate half frame clock");
    }

    #[test]
    fn test_irq_inhibit_can_be_toggled() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // IRQ inhibit = 1
        assert!(fc.is_irq_inhibited());

        fc.write_register(0b0000_0000); // IRQ inhibit = 0
        assert!(!fc.is_irq_inhibited());

        fc.write_register(0b0100_0000); // IRQ inhibit = 1
        assert!(fc.is_irq_inhibited());
    }

    // 4-Step Sequencer Tests
    #[test]
    fn test_four_step_cycle_counter_increments() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        assert_eq!(fc.get_cycle_counter(), 0);
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 2);
    }

    #[test]
    fn test_four_step_step_1_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 1 (7457 cycles)
        for _ in 0..7456 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }

        // At cycle 7457, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_four_step_step_2_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 2 (14913 cycles)
        for _ in 0..14912 {
            fc.clock();
        }

        // At cycle 14913, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_four_step_step_3_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 3 (22371 cycles)
        for _ in 0..22370 {
            fc.clock();
        }

        // At cycle 22371, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_four_step_step_4_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 4 (29829 cycles)
        for _ in 0..29828 {
            fc.clock();
        }

        // At cycle 29829, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_four_step_wraparound() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock through full sequence (29830 cycles)
        for _ in 0..29830 {
            fc.clock();
        }

        // Counter should have wrapped to 0
        assert_eq!(fc.get_cycle_counter(), 0);

        // Next clock should be at cycle 1
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_four_step_wraparound_at_29830_and_irq_sticks_until_cleared() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ enabled

        // On NTSC hardware, the 4-step sequencer frame is 29830 CPU cycles long.
        // The frame IRQ flag is set at the end of the sequence and remains set until cleared.
        for _ in 0..29830 {
            fc.clock();
        }

        assert_eq!(fc.get_cycle_counter(), 0, "Should wrap at 29830 cycles");
        assert!(fc.get_irq_flag(), "Frame IRQ flag should be set by wrap");

        fc.clear_irq_flag();
        assert!(
            !fc.get_irq_flag(),
            "IRQ flag should clear via explicit clear"
        );
    }

    #[test]
    fn test_four_step_complete_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        let mut quarter_count = 0;
        let mut half_count = 0;

        // Run through one complete sequence (29830 cycles)
        for _ in 0..29830 {
            let (quarter, half) = fc.clock();
            if quarter {
                quarter_count += 1;
            }
            if half {
                half_count += 1;
            }
        }

        assert_eq!(quarter_count, 4); // 4 quarter frame clocks
        assert_eq!(half_count, 2); // 2 half frame clocks
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around after end-of-frame
    }

    #[test]
    fn test_four_step_no_signals_between_steps() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock past step 1 (7457)
        for _ in 0..7457 {
            fc.clock();
        }

        // Check no signals between step 1 and step 2
        for _ in 0..100 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }
    }

    #[test]
    fn test_four_step_multiple_sequences() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Run two complete sequences (29830 cycles each)
        for sequence in 0..2 {
            let mut quarter_count = 0;
            let mut half_count = 0;

            for _ in 0..29830 {
                let (quarter, half) = fc.clock();
                if quarter {
                    quarter_count += 1;
                }
                if half {
                    half_count += 1;
                }
            }

            assert_eq!(quarter_count, 4, "Sequence {}", sequence);
            assert_eq!(half_count, 2, "Sequence {}", sequence);
            assert_eq!(fc.get_cycle_counter(), 0, "Sequence {}", sequence);
        }
    }

    // 5-Step Sequencer Tests
    #[test]
    fn test_five_step_step_1_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // The first clock after setting 5-step mode generates immediate quarter+half
        // (per NESDev: "Writing to $4017 with bit 7 set will immediately generate a clock")
        let (quarter, half) = fc.clock();
        assert!(quarter, "5-step mode should generate immediate quarter frame");
        assert!(half, "5-step mode should generate immediate half frame");

        // After the immediate clock, no more signals until step 1 (7457 cycles)
        // We're at cycle 1 now, need 7456 more clocks to reach 7457
        for _ in 1..7456 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }

        // At cycle 7457, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_2_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 2 (14913 cycles)
        for _ in 0..14912 {
            fc.clock();
        }

        // At cycle 14913, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_five_step_step_3_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 3 (22371 cycles)
        for _ in 0..22370 {
            fc.clock();
        }

        // At cycle 22371, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_4_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 4 (29829 cycles)
        for _ in 0..29828 {
            fc.clock();
        }

        // At cycle 29829, NO clocks at all (per Mesen2 - step 4 is "None" type)
        let (quarter, half) = fc.clock();
        assert!(!quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_5_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 5 (37281 cycles)
        for _ in 0..37280 {
            fc.clock();
        }

        // At cycle 37281, BOTH quarter and half frame signals (per Mesen2 - "HalfFrame" type)
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_five_step_wraparound() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock to step 5 (37281 cycles)
        for _ in 0..37281 {
            fc.clock();
        }

        // Counter should have wrapped to 0
        assert_eq!(fc.get_cycle_counter(), 0);

        // Next clock should be at cycle 1
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_five_step_complete_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        let mut quarter_count = 0;
        let mut half_count = 0;

        // Run through one complete sequence
        for _ in 0..37281 {
            let (quarter, half) = fc.clock();
            if quarter {
                quarter_count += 1;
            }
            if half {
                half_count += 1;
            }
        }

        // Per NESDev: Writing to $4017 with bit 7 set generates immediate quarter+half clocks.
        // Then the regular sequence has 4 quarter frames and 2 half frames.
        // Total: 5 quarter frames (1 immediate + 4 regular), 3 half frames (1 immediate + 2 regular)
        assert_eq!(quarter_count, 5);
        assert_eq!(half_count, 3);
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around
    }

    // IRQ Generation Tests
    #[test]
    fn test_irq_flag_set_at_step_4_in_4_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ not inhibited

        // Clock to IRQ assertion cycle (29828)
        for _ in 0..29828 {
            fc.clock();
        }

        // IRQ flag should be set
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_set_when_inhibited() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // 4-step mode, IRQ inhibited

        // Clock to IRQ cycle
        for _ in 0..29828 {
            fc.clock();
        }

        // IRQ flag should NOT be set (inhibited)
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_set_in_5_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode, IRQ not inhibited

        // Clock through entire 5-step sequence
        for _ in 0..37281 {
            fc.clock();
        }

        // IRQ flag should NOT be set (5-step mode never generates IRQ)
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_cleared_by_clear_method() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clear it
        fc.clear_irq_flag();
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_cleared_when_setting_inhibit_bit() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ not inhibited

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Write with inhibit bit set - should clear IRQ
        fc.write_register(0b0100_0000);
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_cleared_when_inhibit_already_set() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // Already inhibited

        // Manually set IRQ flag for testing
        fc.irq_flag = true;
        assert!(fc.get_irq_flag());

        // Write with inhibit bit still set - IRQ should be cleared
        fc.write_register(0b0100_0000);
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_persists_across_multiple_cycles() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clock a few more times (wraps around)
        for _ in 0..100 {
            fc.clock();
        }

        // IRQ flag should still be set
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_set_again_on_next_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // First sequence - clock to IRQ cycle
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clear IRQ
        fc.clear_irq_flag();
        assert!(!fc.get_irq_flag());

        // Second sequence - clock through end-of-frame (29830) and back to IRQ cycle (29828)
        // Remaining to frame end: 29830 - 29828 = 2
        // Then another 29828 cycles to reach IRQ again
        for _ in 0..(2 + 29828) {
            fc.clock();
        }
        assert!(fc.get_irq_flag());
    }
}
