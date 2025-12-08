/// Frame Counter for the NES APU
/// Sequences envelope, sweep, and length counter clocks
/// Operates in two modes: 4-step and 5-step
pub struct FrameCounter {
    mode: Mode,
    irq_inhibit: bool,
    cycle_counter: u32,
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
        }
    }

    /// Write to frame counter register ($4017)
    /// Bit 7: Mode (0 = 4-step, 1 = 5-step)
    /// Bit 6: IRQ inhibit (1 = disable IRQ)
    pub fn write_register(&mut self, value: u8) {
        self.mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };
        self.irq_inhibit = (value & 0x40) != 0;
        self.cycle_counter = 0;
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

    /// Clock the frame counter by one CPU cycle
    /// Returns (quarter_frame, half_frame) signals
    pub fn clock(&mut self) -> (bool, bool) {
        self.cycle_counter += 1;

        let (quarter_frame, half_frame) = match self.mode {
            Mode::FourStep => self.clock_four_step(),
            Mode::FiveStep => (false, false), // TODO: Implement in sub-issue #83
        };

        (quarter_frame, half_frame)
    }

    /// Clock the 4-step sequencer
    fn clock_four_step(&mut self) -> (bool, bool) {
        const STEP_1_CYCLES: u32 = 7457;
        const STEP_2_CYCLES: u32 = 14913;
        const STEP_3_CYCLES: u32 = 22371;
        const STEP_4_CYCLES: u32 = 29829;

        let quarter_frame = matches!(
            self.cycle_counter,
            STEP_1_CYCLES | STEP_2_CYCLES | STEP_3_CYCLES | STEP_4_CYCLES
        );
        let half_frame = matches!(self.cycle_counter, STEP_2_CYCLES | STEP_4_CYCLES);

        // Wrap around after step 4
        if self.cycle_counter >= STEP_4_CYCLES {
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
}
