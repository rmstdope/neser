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

    // Length counter fields
    length_counter: u8,
    length_counter_halt: bool,
}

/// Length counter load table (indexed by bits 7-3 of $4003/$4007)
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

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
            length_counter: 0,
            length_counter_halt: false,
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

    /// Write to $4000 register (duty, loop/halt, constant volume, volume/envelope period)
    pub fn write_control(&mut self, value: u8) {
        self.duty_mode = (value >> 6) & 0x03;
        self.envelope_loop_flag = (value & 0x20) != 0;
        self.length_counter_halt = (value & 0x20) != 0; // Same bit as envelope loop
        self.constant_volume_flag = (value & 0x10) != 0;
        self.volume_envelope_period = value & 0x0F;
    }

    /// Write to $4003 register (loads length counter, sets start flag, sets timer high)
    pub fn write_length_counter_timer_high(&mut self, value: u8) {
        self.write_timer_high(value);
        self.envelope_start_flag = true;
        // Load length counter from bits 7-3
        let index = (value >> 3) as usize;
        self.length_counter = LENGTH_COUNTER_TABLE[index];
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

    /// Clock the length counter (called by half frame from frame counter)
    pub fn clock_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    /// Get the current length counter value
    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }

    /// Set length counter enabled/disabled (from $4015)
    pub fn set_length_counter_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.length_counter = 0;
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

    // Length Counter Tests

    #[test]
    fn test_length_counter_load_values() {
        let mut pulse = Pulse::new();

        // Test all 32 load values
        let expected = [
            10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20,
            96, 22, 192, 24, 72, 26, 16, 28, 32, 30,
        ];

        for (i, &expected_value) in expected.iter().enumerate() {
            let value = (i as u8) << 3; // Put index in bits 7-3
            pulse.write_length_counter_timer_high(value);
            assert_eq!(
                pulse.get_length_counter(),
                expected_value,
                "Failed for index {}",
                i
            );
        }
    }

    #[test]
    fn test_length_counter_decrements() {
        let mut pulse = Pulse::new();
        pulse.write_length_counter_timer_high(0b00001_000); // Load value 2 (index 1)
        assert_eq!(pulse.get_length_counter(), 254);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 253);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 252);
    }

    #[test]
    fn test_length_counter_halt_flag() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0010_0000); // Set halt flag
        pulse.write_length_counter_timer_high(0b00010_000); // Load value 20 (index 2)

        assert_eq!(pulse.get_length_counter(), 20);

        // Clock should not decrement when halted
        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 20);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 20);
    }

    #[test]
    fn test_length_counter_stops_at_zero() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0000_0000); // No halt
        pulse.write_length_counter_timer_high(0b00011_000); // Load value 2 (index 3)

        assert_eq!(pulse.get_length_counter(), 2);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 1);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 0);

        // Should not go below zero
        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 0);
    }

    #[test]
    fn test_length_counter_can_be_reloaded() {
        let mut pulse = Pulse::new();
        pulse.write_length_counter_timer_high(0b00100_000); // Load value 40 (index 4)
        assert_eq!(pulse.get_length_counter(), 40);

        // Clock a few times
        pulse.clock_length_counter();
        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 38);

        // Reload with different value
        pulse.write_length_counter_timer_high(0b00000_000); // Load value 10 (index 0)
        assert_eq!(pulse.get_length_counter(), 10);
    }

    #[test]
    fn test_length_counter_halt_flag_shared_with_envelope_loop() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0010_0000); // Bit 5 set

        // Both flags should be set from same bit
        assert!(pulse.length_counter_halt);
        assert!(pulse.envelope_loop_flag);

        pulse.write_control(0b0000_0000); // Bit 5 clear

        assert!(!pulse.length_counter_halt);
        assert!(!pulse.envelope_loop_flag);
    }

    #[test]
    fn test_set_length_counter_enabled() {
        let mut pulse = Pulse::new();
        pulse.write_length_counter_timer_high(0b01010_000); // Load some value
        assert_eq!(pulse.get_length_counter(), 60);

        // Disable should clear counter
        pulse.set_length_counter_enabled(false);
        assert_eq!(pulse.get_length_counter(), 0);

        // Enable should not change counter (it stays at current value)
        pulse.set_length_counter_enabled(true);
        assert_eq!(pulse.get_length_counter(), 0);
    }

    #[test]
    fn test_length_counter_with_halt_then_unhalt() {
        let mut pulse = Pulse::new();
        pulse.write_control(0b0010_0000); // Halt flag set
        pulse.write_length_counter_timer_high(0b00000_000); // Load 10

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 10); // Halted, no change

        // Clear halt flag
        pulse.write_control(0b0000_0000);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 9); // Now decrements
    }
}
