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
}
