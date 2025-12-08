/// Pulse wave channel for the NES APU
/// Generates square waves with variable duty cycle
pub struct Pulse {
    // Timer fields
    timer_period: u16,
    timer_counter: u16,

    // Sequencer fields
    duty_mode: u8,
    sequence_position: u8,

    // Envelope fields
    envelope_start_flag: bool,
    envelope_loop_flag: bool,
    constant_volume_flag: bool,
    volume_envelope_period: u8,
    envelope_divider: u8,
    envelope_decay_level: u8,
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
            envelope_start_flag: false,
            envelope_loop_flag: false,
            constant_volume_flag: false,
            volume_envelope_period: 0,
            envelope_divider: 0,
            envelope_decay_level: 0,
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

    /// Write to $4000 register (duty, loop, constant volume, volume/envelope period)
    pub fn write_control(&mut self, value: u8) {
        self.duty_mode = (value >> 6) & 0x03;
        self.envelope_loop_flag = (value & 0x20) != 0;
        self.constant_volume_flag = (value & 0x10) != 0;
        self.volume_envelope_period = value & 0x0F;
    }

    /// Write to $4003 register (sets start flag in addition to timer high)
    pub fn write_length_counter_timer_high(&mut self, value: u8) {
        self.write_timer_high(value);
        self.envelope_start_flag = true;
    }

    /// Clock the envelope (called by quarter frame from frame counter)
    pub fn clock_envelope(&mut self) {
        if self.envelope_start_flag {
            self.envelope_start_flag = false;
            self.envelope_decay_level = 15;
            self.envelope_divider = self.volume_envelope_period;
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume_envelope_period;
            if self.envelope_decay_level > 0 {
                self.envelope_decay_level -= 1;
            } else if self.envelope_loop_flag {
                self.envelope_decay_level = 15;
            }
        } else {
            self.envelope_divider -= 1;
        }
    }

    /// Get the envelope volume output (0-15)
    pub fn get_envelope_volume(&self) -> u8 {
        if self.constant_volume_flag {
            self.volume_envelope_period
        } else {
            self.envelope_decay_level
        }
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

    // Envelope Generator Tests

    #[test]
    fn test_envelope_constant_volume_mode() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0001_1010); // Constant volume flag set, volume = 10
        assert_eq!(pulse.get_envelope_volume(), 10);

        // Clock envelope - should not change in constant volume mode
        pulse.clock_envelope();
        assert_eq!(pulse.get_envelope_volume(), 10);
    }

    #[test]
    fn test_envelope_decay_mode() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0000_0000); // Decay mode, period = 0

        // Start flag not set, decay level starts at 0
        assert_eq!(pulse.get_envelope_volume(), 0);

        // Set start flag
        pulse.envelope_start_flag = true;
        pulse.clock_envelope();

        // Should reset to 15
        assert_eq!(pulse.get_envelope_volume(), 15);

        // Clock envelope - divider = 0, so should decrement decay level
        pulse.clock_envelope();
        assert_eq!(pulse.get_envelope_volume(), 14);

        pulse.clock_envelope();
        assert_eq!(pulse.get_envelope_volume(), 13);
    }

    #[test]
    fn test_envelope_start_flag_on_write() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0000_0000);

        assert!(!pulse.envelope_start_flag);

        pulse.write_length_counter_timer_high(0x00);
        assert!(pulse.envelope_start_flag);
    }

    #[test]
    fn test_envelope_divider_period() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0000_0010); // Period = 2 (divider period = 3)
        pulse.envelope_start_flag = true;
        pulse.clock_envelope();

        // Start flag cleared, decay level = 15, divider = 2
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 2);

        // Clock: divider 2 -> 1
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 1);

        // Clock: divider 1 -> 0
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 0);

        // Clock: divider = 0, reload to 2, decrement decay level 15 -> 14
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 14);
        assert_eq!(pulse.envelope_divider, 2);
    }

    #[test]
    fn test_envelope_loop_flag() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0010_0000); // Loop flag set, period = 0
        pulse.envelope_start_flag = true;
        pulse.clock_envelope();

        // With period = 0, divider is always 0, so decay decrements every clock
        // Decay from 15 to 0
        for expected in (0..=15).rev() {
            assert_eq!(pulse.envelope_decay_level, expected);
            pulse.clock_envelope();
        }

        // At 0, with loop flag, should have reloaded to 15
        assert_eq!(pulse.envelope_decay_level, 15);
    }

    #[test]
    fn test_envelope_no_loop_stays_at_zero() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0000_0000); // No loop, period = 0
        pulse.envelope_start_flag = true;
        pulse.clock_envelope();

        // Decay from 15 to 0
        for _ in 0..15 {
            pulse.clock_envelope();
        }

        assert_eq!(pulse.envelope_decay_level, 0);

        // Should stay at 0 without loop
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 0);

        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 0);
    }

    #[test]
    fn test_write_control_parses_all_fields() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b1111_1010);
        // Bits 7-6: duty = 11 (3)
        // Bit 5: loop flag = 1
        // Bit 4: constant volume = 1
        // Bits 3-0: volume/period = 1010 (10)

        assert_eq!(pulse.duty_mode, 3);
        assert!(pulse.envelope_loop_flag);
        assert!(pulse.constant_volume_flag);
        assert_eq!(pulse.volume_envelope_period, 10);
    }

    #[test]
    fn test_envelope_constant_volume_still_updates_decay() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0001_0101); // Constant volume, period = 5
        pulse.envelope_start_flag = true;

        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.get_envelope_volume(), 5); // Returns constant volume

        // Even in constant volume mode, decay level is updated
        for _ in 0..6 {
            pulse.clock_envelope();
        }
        assert_eq!(pulse.envelope_decay_level, 14);
        assert_eq!(pulse.get_envelope_volume(), 5); // Still returns constant volume
    }
}
