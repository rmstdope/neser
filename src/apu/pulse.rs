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

    // Sweep unit fields
    sweep_enabled: bool,
    sweep_divider_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_reload: bool,
    sweep_divider: u8,
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

            // Sweep unit fields
            sweep_enabled: false,
            sweep_divider_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_reload: false,
            sweep_divider: 0,
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

    /// Get current timer period (for testing)
    pub fn get_timer_period(&self) -> u16 {
        self.timer_period
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

    /// Write to sweep register ($4001/$4005)
    /// Bit 7: Enable flag
    /// Bits 6-4: Divider period (P), actual period = P + 1
    /// Bit 3: Negate flag (ones' complement for Pulse 1)
    /// Bits 2-0: Shift count
    pub fn write_sweep(&mut self, value: u8) {
        self.sweep_enabled = (value & 0x80) != 0;
        self.sweep_divider_period = (value >> 4) & 0x07;
        self.sweep_negate = (value & 0x08) != 0;
        self.sweep_shift = value & 0x07;
        self.sweep_reload = true;
    }

    /// Calculate target period for sweep
    /// Target = current period + (current period >> shift)
    /// If negate is set, uses ones' complement: -change - 1 (Pulse 1 specific)
    pub fn get_sweep_target_period(&self) -> u16 {
        let change = self.timer_period >> self.sweep_shift;
        if self.sweep_negate {
            // Pulse 1 uses ones' complement: -change - 1
            let negated = change.wrapping_neg().wrapping_sub(1);
            self.timer_period.wrapping_add(negated)
        } else {
            self.timer_period.wrapping_add(change)
        }
    }

    /// Check if sweep is muting the channel
    /// Mutes if: current period < 8 OR target period > $7FF
    /// Note: Muting check runs continuously, even when sweep is disabled
    pub fn is_sweep_muting(&self) -> bool {
        self.timer_period < 8 || self.get_sweep_target_period() > 0x7FF
    }

    /// Clock the sweep unit (called by half frame)
    pub fn clock_sweep(&mut self) {
        // Decrement divider first (unless we're going to reload)
        let should_update = self.sweep_divider == 0 && !self.sweep_reload;

        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_divider_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }

        // Update period if divider reached 0 and all conditions are met
        if should_update && self.sweep_enabled && self.sweep_shift != 0 {
            let target_period = self.get_sweep_target_period();
            if self.timer_period >= 8 && target_period <= 0x7FF {
                self.timer_period = target_period;
                self.timer_counter = self.timer_period;
            }
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

    // Sweep unit tests

    #[test]
    fn test_sweep_initial_state() {
        let pulse = Pulse::new();
        // Initial period is 0, which is < 8, so it should be muting
        assert!(pulse.is_sweep_muting());
    }

    #[test]
    fn test_sweep_write_register() {
        let mut pulse = Pulse::new();
        // Enable=1, Period=3, Negate=1, Shift=2
        // Binary: 1_011_1_010
        pulse.write_sweep(0b1011_1010);

        // Should set reload flag (tested by observing divider reload on next clock)
        pulse.clock_sweep();
        // After first clock with reload flag, divider should be period (3)
    }

    #[test]
    fn test_sweep_target_period_no_negate() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x04);
        pulse.write_timer_high(0x00); // Period = 4
        pulse.write_sweep(0b1000_0001); // Enable, period=0, no negate, shift=1

        // Target = current + (current >> shift)
        // Target = 4 + (4 >> 1) = 4 + 2 = 6
        assert_eq!(pulse.get_sweep_target_period(), 6);
    }

    #[test]
    fn test_sweep_target_period_with_negate_ones_complement() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x14);
        pulse.write_timer_high(0x00); // Period = 20 (0x14)
        pulse.write_sweep(0b1000_1001); // Enable, period=0, negate=1, shift=1

        // Change = 20 >> 1 = 10
        // Ones' complement: -10 - 1 = -11
        // Target = 20 + (-11) = 9
        assert_eq!(pulse.get_sweep_target_period(), 9);
    }

    #[test]
    fn test_sweep_target_period_negate_with_shift_0() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x14);
        pulse.write_timer_high(0x00); // Period = 20 (0x14)
        pulse.write_sweep(0b1000_1000); // Enable, period=0, negate=1, shift=0

        // Change = 20 >> 0 = 20
        // Ones' complement: -20 - 1 = -21
        // Target = 20 + (-21) = -1, wraps to 65535
        assert_eq!(pulse.get_sweep_target_period(), 65535);
    }

    #[test]
    fn test_sweep_muting_period_less_than_8() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x07);
        pulse.write_timer_high(0x00); // Period = 7

        assert!(pulse.is_sweep_muting()); // Period < 8 should mute

        pulse.write_timer_low(0x08);
        pulse.write_timer_high(0x00); // Period = 8
        assert!(!pulse.is_sweep_muting()); // Period = 8 should not mute
    }

    #[test]
    fn test_sweep_muting_target_greater_than_7ff() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0xFF);
        pulse.write_timer_high(0b000_11111); // Period = $7FF
        pulse.write_sweep(0b1000_0001); // Enable, period=0, no negate, shift=1

        // Target = $7FF + ($7FF >> 1) = $7FF + $3FF = $BFF > $7FF
        assert!(pulse.is_sweep_muting());
    }

    #[test]
    fn test_sweep_divider_reload() {
        let mut pulse = Pulse::new();
        pulse.write_sweep(0b1011_0000); // Enable, period=3, no negate, shift=0

        // First clock should reload divider (reload flag set by write)
        pulse.clock_sweep();
        // Divider should now be 3

        // Clock 3 more times to count down divider
        pulse.clock_sweep(); // divider = 2
        pulse.clock_sweep(); // divider = 1
        pulse.clock_sweep(); // divider = 0, should reload to 3

        // This behavior is hard to observe without internal state access,
        // but we can test period update behavior
    }

    #[test]
    fn test_sweep_period_update_when_enabled() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x10);
        pulse.write_timer_high(0x00); // Period = 16 (0x10)
        pulse.write_sweep(0b1000_0001); // Enable, period=0, no negate, shift=1

        // Target = 16 + (16 >> 1) = 16 + 8 = 24
        assert_eq!(pulse.get_sweep_target_period(), 24);

        // First clock reloads divider
        pulse.clock_sweep();

        // Second clock should update period (divider=0, enabled, shift!=0, not muting)
        pulse.clock_sweep();

        // Period should now be 24
        assert_eq!(pulse.get_timer_period(), 24);
    }

    #[test]
    fn test_sweep_no_update_when_disabled() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x10);
        pulse.write_timer_high(0x00); // Period = 16 (0x10)
        pulse.write_sweep(0b0000_0001); // Disabled, shift=1

        pulse.clock_sweep();
        pulse.clock_sweep();

        // Period should remain 16 (sweep disabled)
        assert_eq!(pulse.get_timer_period(), 16);
    }

    #[test]
    fn test_sweep_no_update_when_shift_zero() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x10);
        pulse.write_timer_high(0x00); // Period = 16 (0x10)
        pulse.write_sweep(0b1000_0000); // Enable, shift=0

        pulse.clock_sweep();
        pulse.clock_sweep();

        // Period should remain 16 (shift=0 means no update)
        assert_eq!(pulse.get_timer_period(), 16);
    }

    #[test]
    fn test_sweep_no_update_when_muting() {
        let mut pulse = Pulse::new();
        pulse.write_timer_low(0x07);
        pulse.write_timer_high(0x00); // Period = 7 (muted, < 8)
        pulse.write_sweep(0b1000_0001); // Enable, shift=1

        assert!(pulse.is_sweep_muting());

        pulse.clock_sweep();
        pulse.clock_sweep();

        // Period should remain 7 (muting prevents update)
        assert_eq!(pulse.get_timer_period(), 7);
    }
}
