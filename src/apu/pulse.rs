/// Pulse wave channel for the NES APU
/// Generates square waves with variable duty cycle
pub struct Pulse {
    // Timer fields
    timer_period: u16,
    timer_counter: u16,

    // Sequencer fields
    duty_mode: u8,
    sequence_position: u8,
}

/// Duty cycle sequence lookup tables
/// Sequencer starts at 0 and counts down (reads 0, 7, 6, 5, 4, 3, 2, 1)
const DUTY_SEQUENCES: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [0, 0, 0, 0, 0, 0, 1, 1], // 25%
    [0, 0, 0, 0, 1, 1, 1, 1], // 50%
    [1, 1, 1, 1, 1, 1, 0, 0], // 25% negated
];

impl Default for Pulse {
    fn default() -> Self {
        Self::new()
    }
}

impl Pulse {
    /// Create a new Pulse channel
    pub fn new() -> Self {
        Self {
            timer_period: 0,
            timer_counter: 0,
            duty_mode: 0,
            sequence_position: 0,
        }
    }

    /// Write to timer low register ($4002 for Pulse 1)
    pub fn write_timer_low(&mut self, value: u8) {
        self.timer_period = (self.timer_period & 0x0700) | (value as u16);
    }

    /// Write to timer high register ($4003 bits 2-0 for Pulse 1)
    pub fn write_timer_high(&mut self, value: u8) {
        self.timer_period = (self.timer_period & 0x00FF) | (((value & 0x07) as u16) << 8);
    }

    /// Clock the timer (called every APU cycle, which is every 2 CPU cycles)
    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.clock_sequencer();
        } else {
            self.timer_counter -= 1;
        }
    }

    /// Clock the sequencer (decrements position)
    fn clock_sequencer(&mut self) {
        self.sequence_position = if self.sequence_position == 0 {
            7
        } else {
            self.sequence_position - 1
        };
    }

    /// Get the current sequencer output (0 or 1)
    pub fn get_sequencer_output(&self) -> u8 {
        DUTY_SEQUENCES[self.duty_mode as usize][self.sequence_position as usize]
    }

    /// Write duty cycle mode (bits 7-6 of $4000)
    pub fn write_duty(&mut self, duty: u8) {
        self.duty_mode = duty & 0x03;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_new() {
        let pulse = Pulse::new();
        assert_eq!(pulse.timer_period, 0);
        assert_eq!(pulse.sequence_position, 0);
    }

    #[test]
    fn test_write_timer_low() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0xAB);
        assert_eq!(pulse.timer_period, 0xAB);
    }

    #[test]
    fn test_write_timer_high() {
        let mut pulse = Pulse::new();
        pulse.write_timer_high(0x05);
        assert_eq!(pulse.timer_period, 0x0500);
    }

    #[test]
    fn test_write_timer_combined() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0xCD);
        pulse.write_timer_high(0x07);
        assert_eq!(pulse.timer_period, 0x07CD);
    }

    #[test]
    fn test_timer_high_masks_upper_bits() {
        let mut pulse = Pulse::new();
        pulse.write_timer_high(0xFF); // Only bits 2-0 should be used
        assert_eq!(pulse.timer_period, 0x0700);
    }

    #[test]
    fn test_sequencer_starts_at_zero() {
        let pulse = Pulse::new();
        assert_eq!(pulse.sequence_position, 0);
    }

    #[test]
    fn test_sequencer_counts_down() {
        let mut pulse = Pulse::new();
        pulse.timer_period = 0;

        // Initial position is 0, clock should move to 7
        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 7);

        // Then count down 7, 6, 5, 4, 3, 2, 1, 0
        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 6);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 5);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 4);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 3);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 2);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 1);

        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 0);

        // Should wrap back to 7
        pulse.clock_timer();
        assert_eq!(pulse.sequence_position, 7);
    }

    #[test]
    fn test_timer_countdown() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(3);

        // Timer starts at period value
        assert_eq!(pulse.timer_counter, 0);

        // First clock: counter = 0, reload to 3, clock sequencer
        pulse.clock_timer();
        assert_eq!(pulse.timer_counter, 3);
        assert_eq!(pulse.sequence_position, 7);

        // Count down: 3, 2, 1, 0
        pulse.clock_timer();
        assert_eq!(pulse.timer_counter, 2);
        assert_eq!(pulse.sequence_position, 7); // Sequencer unchanged

        pulse.clock_timer();
        assert_eq!(pulse.timer_counter, 1);

        pulse.clock_timer();
        assert_eq!(pulse.timer_counter, 0);

        // Next clock: reload and clock sequencer
        pulse.clock_timer();
        assert_eq!(pulse.timer_counter, 3);
        assert_eq!(pulse.sequence_position, 6);
    }

    #[test]
    fn test_duty_cycle_0_12_5_percent() {
        let mut pulse = Pulse::new();
        pulse.write_duty(0);

        // Duty 0: [0,0,0,0,0,0,0,1]
        // Sequencer reads: 0,7,6,5,4,3,2,1
        assert_eq!(pulse.get_sequencer_output(), 0); // Position 0

        pulse.sequence_position = 7;
        assert_eq!(pulse.get_sequencer_output(), 1); // Position 7

        pulse.sequence_position = 6;
        assert_eq!(pulse.get_sequencer_output(), 0); // Position 6

        pulse.sequence_position = 1;
        assert_eq!(pulse.get_sequencer_output(), 0); // Position 1
    }

    #[test]
    fn test_duty_cycle_1_25_percent() {
        let mut pulse = Pulse::new();
        pulse.write_duty(1);

        // Duty 1: [0,0,0,0,0,0,1,1]
        pulse.sequence_position = 7;
        assert_eq!(pulse.get_sequencer_output(), 1); // Position 7

        pulse.sequence_position = 6;
        assert_eq!(pulse.get_sequencer_output(), 1); // Position 6

        pulse.sequence_position = 5;
        assert_eq!(pulse.get_sequencer_output(), 0); // Position 5
    }

    #[test]
    fn test_duty_cycle_2_50_percent() {
        let mut pulse = Pulse::new();
        pulse.write_duty(2);

        // Duty 2: [0,0,0,0,1,1,1,1]
        pulse.sequence_position = 7;
        assert_eq!(pulse.get_sequencer_output(), 1);

        pulse.sequence_position = 4;
        assert_eq!(pulse.get_sequencer_output(), 1);

        pulse.sequence_position = 3;
        assert_eq!(pulse.get_sequencer_output(), 0);

        pulse.sequence_position = 0;
        assert_eq!(pulse.get_sequencer_output(), 0);
    }

    #[test]
    fn test_duty_cycle_3_25_percent_negated() {
        let mut pulse = Pulse::new();
        pulse.write_duty(3);

        // Duty 3: [1,1,1,1,1,1,0,0]
        pulse.sequence_position = 0;
        assert_eq!(pulse.get_sequencer_output(), 1);

        pulse.sequence_position = 5;
        assert_eq!(pulse.get_sequencer_output(), 1);

        pulse.sequence_position = 6;
        assert_eq!(pulse.get_sequencer_output(), 0);

        pulse.sequence_position = 7;
        assert_eq!(pulse.get_sequencer_output(), 0);
    }

    #[test]
    fn test_duty_write_masks_upper_bits() {
        let mut pulse = Pulse::new();
        pulse.write_duty(0xFF); // Only bits 1-0 should be used
        assert_eq!(pulse.duty_mode, 3);
    }
}
