/// Frame Counter for the NES APU
/// Sequences envelope, sweep, and length counter clocks
/// Operates in two modes: 4-step and 5-step
pub struct FrameCounter {
    mode: Mode,
    irq_inhibit: bool,
    cycle_counter: u32,
    irq_flag: bool,
    reset_phase: bool,     // Phase when counter was reset (for jitter calculation)
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
            reset_phase: false,     // Reset on even cycle
        }
    }

    /// Write to frame counter register ($4017)
    /// Bit 7: Mode (0 = 4-step, 1 = 5-step)
    /// Bit 6: IRQ inhibit (1 = disable IRQ)
    pub fn write_register(&mut self, value: u8) {
        let old_mode = self.mode;
        self.mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };
        self.irq_inhibit = (value & 0x40) != 0;
        self.cycle_counter = 0;
        // Note: Phase not tracked here as we don't know the APU cycle

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // Note: Immediate clock handled by write_register_with_immediate_clock
        let _ = old_mode;
    }

    /// Write to frame counter register with immediate clock support
    /// Returns (quarter_frame, half_frame) signals if immediate clock occurs
    pub fn write_register_with_immediate_clock(&mut self, value: u8, apu_cycle: u32) -> (bool, bool) {
        let old_mode = self.mode;
        let new_mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };

        self.mode = new_mode;
        self.irq_inhibit = (value & 0x40) != 0;
        
        // Jitter: When $4017 is written on an odd CPU cycle, the reset is delayed by 1 cycle
        // This shifts the entire frame sequence, including when we read relative to IRQ timing
        // Note: apu_cycle odd means CPU cycle odd (they increment together)
        let write_on_odd_cpu_cycle = (apu_cycle % 2) != 0;
        self.cycle_counter = if write_on_odd_cpu_cycle { 
            // Odd write: start at -1 (will become 0 on next clock)
            // This delays everything by 1 cycle
            u32::MAX  // wraps to 0 on increment
        } else { 
            0 
        };
        self.reset_phase = write_on_odd_cpu_cycle;

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // Immediate clock when writing 1 to mode bit (bit 7)
        // This happens every time $80 is written, not just when switching modes
        if new_mode == Mode::FiveStep {
            (true, true) // Clock both quarter and half frame
        } else {
            (false, false)
        }
    }

    /// Get the current mode
    pub fn get_mode(&self) -> bool {
        self.mode == Mode::FiveStep
    }

    /// Check if IRQ is inhibited
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

    /// Clock the frame counter by one CPU cycle
    /// Returns (quarter_frame, half_frame) signals
    pub fn clock(&mut self) -> (bool, bool) {
        // Frame counter increments every CPU cycle
        // Use wrapping_add to handle the jitter case where cycle_counter starts at u32::MAX
        self.cycle_counter = self.cycle_counter.wrapping_add(1);

        let (quarter_frame, half_frame) = match self.mode {
            Mode::FourStep => self.clock_four_step(),
            Mode::FiveStep => self.clock_five_step(),
        };

        (quarter_frame, half_frame)
    }

    /// Clock the 4-step sequencer
    fn clock_four_step(&mut self) -> (bool, bool) {
        const STEP_1_CYCLES: u32 = 7457;
        const STEP_2_CYCLES: u32 = 14913;
        const STEP_3_CYCLES: u32 = 22371;
        const STEP_4_CYCLES: u32 = 29829;
        const IRQ_CYCLE: u32 = STEP_4_CYCLES + 2; // IRQ at 29831

        let quarter_frame = matches!(
            self.cycle_counter,
            STEP_1_CYCLES | STEP_2_CYCLES | STEP_3_CYCLES | STEP_4_CYCLES
        );
        let half_frame = matches!(self.cycle_counter, STEP_2_CYCLES | STEP_4_CYCLES);

        // Set IRQ flag at cycle 29831 (jitter offset already in cycle_counter)
        if self.cycle_counter == IRQ_CYCLE && !self.irq_inhibit {
            self.irq_flag = true;
        }

        // Wrap around after IRQ is set
        if self.cycle_counter > IRQ_CYCLE {
            self.cycle_counter = 0;
        }

        (quarter_frame, half_frame)
    }

    /// Clock the 5-step sequencer
    fn clock_five_step(&mut self) -> (bool, bool) {
        const STEP_1_CYCLES: u32 = 7457;
        const STEP_2_CYCLES: u32 = 14913;
        const STEP_3_CYCLES: u32 = 22371;
        const STEP_4_CYCLES: u32 = 29829;
        const STEP_5_CYCLES: u32 = 37281;

        let quarter_frame = matches!(
            self.cycle_counter,
            STEP_1_CYCLES | STEP_2_CYCLES | STEP_3_CYCLES | STEP_4_CYCLES
        );
        let half_frame = matches!(self.cycle_counter, STEP_2_CYCLES | STEP_5_CYCLES);

        // Wrap around after step 5
        if self.cycle_counter >= STEP_5_CYCLES {
            self.cycle_counter = 0;
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

        // Clock to step 4 (29829 cycles)
        for _ in 0..29829 {
            fc.clock();
        }

        // Counter should have wrapped to 0
        assert_eq!(fc.get_cycle_counter(), 0);

        // Next clock should be at cycle 1
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_four_step_complete_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        let mut quarter_count = 0;
        let mut half_count = 0;

        // Run through one complete sequence
        for _ in 0..29829 {
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
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around
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

        // Run two complete sequences
        for sequence in 0..2 {
            let mut quarter_count = 0;
            let mut half_count = 0;

            for _ in 0..29829 {
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

        // At cycle 29829, quarter frame signal (no half frame!)
        let (quarter, half) = fc.clock();
        assert!(quarter);
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

        // At cycle 37281, half frame signal ONLY (no quarter frame!)
        let (quarter, half) = fc.clock();
        assert!(!quarter);
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

        assert_eq!(quarter_count, 4); // 4 quarter frame clocks (steps 1-4)
        assert_eq!(half_count, 2); // 2 half frame clocks (step 2 and step 5)
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around
    }

    #[test]
    fn test_five_step_immediate_clock_on_mode_switch() {
        let mut fc = FrameCounter::new();
        
        // Start in 4-step mode, advance a bit
        fc.write_register(0b0000_0000);
        for _ in 0..100 {
            fc.clock();
        }

        // Switch to 5-step mode - should immediately clock quarter and half
        let result = fc.write_register_with_immediate_clock(0b1000_0000, 0);
        
        assert_eq!(result, (true, true)); // Both quarter and half frame clocked
        assert_eq!(fc.get_cycle_counter(), 0); // Counter reset
    }

    #[test]
    fn test_five_step_no_immediate_clock_when_staying_in_5_step() {
        let mut fc = FrameCounter::new();
        
        // Already in 5-step mode
        fc.write_register(0b1000_0000);
        
        // Write to 5-step again - should NOT trigger immediate clock
        let result = fc.write_register_with_immediate_clock(0b1000_0000, 0);
        
        assert_eq!(result, (false, false)); // No immediate clock
    }

    #[test]
    fn test_five_step_no_immediate_clock_when_switching_to_4_step() {
        let mut fc = FrameCounter::new();
        
        // Start in 5-step mode
        fc.write_register(0b1000_0000);
        
        // Switch to 4-step - should NOT trigger immediate clock
        let result = fc.write_register_with_immediate_clock(0b0000_0000, 0);
        
        assert_eq!(result, (false, false)); // No immediate clock
    }

    // IRQ Generation Tests
    #[test]
    fn test_irq_flag_set_at_step_4_in_4_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ not inhibited

        // Clock to step 4 (29829 cycles)
        for _ in 0..29829 {
            fc.clock();
        }

        // IRQ flag should be set
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_set_when_inhibited() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // 4-step mode, IRQ inhibited

        // Clock to step 4
        for _ in 0..29829 {
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
        for _ in 0..29829 {
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
        for _ in 0..29829 {
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
        for _ in 0..29829 {
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

        // First sequence
        for _ in 0..29829 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clear IRQ
        fc.clear_irq_flag();
        assert!(!fc.get_irq_flag());

        // Second sequence - should set IRQ again
        for _ in 0..29829 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());
    }
}
