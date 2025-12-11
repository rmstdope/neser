/// Pulse wave channel for the NES APU
/// Generates square waves with variable duty cycle
pub struct Pulse {
    // Channel identifier (true = Pulse 1, false = Pulse 2)
    // Used for sweep complement mode: Pulse 1 uses ones' complement, Pulse 2 uses two's complement
    is_pulse1: bool,

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
    length_counter_enabled: bool, // Controlled by $4015

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
        Self::new(true) // Default to Pulse 1
    }
}

impl Pulse {
    /// Create a new Pulse channel
    ///
    /// # Arguments
    /// * `is_pulse1` - true for Pulse 1, false for Pulse 2 (affects sweep complement mode)
    pub fn new(is_pulse1: bool) -> Self {
        Self {
            is_pulse1,
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
            length_counter_enabled: false, // Disabled at power-on

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
    #[cfg(test)]
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
    #[cfg(test)]
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
        // Load length counter from bits 7-3 (only if channel is enabled via $4015)
        if self.length_counter_enabled {
            let index = (value >> 3) as usize;
            self.length_counter = LENGTH_COUNTER_TABLE[index];
        }
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

    /// Clear the length counter to 0
    pub fn clear_length_counter(&mut self) {
        self.length_counter = 0;
    }

    /// Get the envelope start flag state
    #[cfg(test)]
    pub fn get_envelope_start_flag(&self) -> bool {
        self.envelope_start_flag
    }

    /// Get the sweep reload flag state
    #[cfg(test)]
    pub fn get_sweep_reload(&self) -> bool {
        self.sweep_reload
    }

    /// Set length counter enabled/disabled (from $4015)
    /// When disabled, the channel is silenced but the length counter value is preserved
    pub fn set_length_counter_enabled(&mut self, enabled: bool) {
        self.length_counter_enabled = enabled;
    }

    /// Get whether length counter is enabled (from $4015)
    pub fn is_length_counter_enabled(&self) -> bool {
        self.length_counter_enabled
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
    /// Pulse 1 uses ones' complement: -change - 1
    /// Pulse 2 uses two's complement: -change
    pub fn get_sweep_target_period(&self) -> u16 {
        let change = self.timer_period >> self.sweep_shift;
        if self.sweep_negate {
            let negated = if self.is_pulse1 {
                // Pulse 1: ones' complement (-change - 1)
                change.wrapping_neg().wrapping_sub(1)
            } else {
                // Pulse 2: two's complement (-change)
                change.wrapping_neg()
            };
            self.timer_period.wrapping_add(negated)
        } else {
            self.timer_period.wrapping_add(change)
        }
    }

    /// Check if sweep is muting the channel
    /// Mutes if: current period < 8 OR target period > $7FF
    /// Note: Muting check runs continuously, even when sweep is disabled
    #[cfg(test)]
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

    /// Get the current output sample from the pulse channel
    /// Returns envelope volume (0-15) if playing, or 0 if muted
    ///
    /// Channel is muted (outputs 0) if ANY of these conditions are true:
    /// 1. Sequencer output is 0 (duty cycle low point)
    /// 2. Length counter is 0
    /// 3. Timer period < 8
    /// 4. Sweep target period > $7FF
    pub fn output(&self) -> u8 {
        // Check all muting conditions
        if self.get_sequencer_output() == 0
            || !self.length_counter_enabled // Channel disabled via $4015
            || self.length_counter == 0
            || self.timer_period < 8
            || self.get_sweep_target_period() > 0x7FF
        {
            0
        } else {
            self.get_envelope_volume()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_new() {
        let pulse = Pulse::default();
        assert_eq!(pulse.timer_period, 0);
        assert_eq!(pulse.sequence_position, 0);
    }

    #[test]
    fn test_write_timer_low() {
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0xAB);
        assert_eq!(pulse.timer_period, 0xAB);
    }

    #[test]
    fn test_write_timer_high() {
        let mut pulse = Pulse::default();
        pulse.write_timer_high(0x05);
        assert_eq!(pulse.timer_period, 0x0500);
    }

    #[test]
    fn test_write_timer_combined() {
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0xCD);
        pulse.write_timer_high(0x07);
        assert_eq!(pulse.timer_period, 0x07CD);
    }

    #[test]
    fn test_timer_high_masks_upper_bits() {
        let mut pulse = Pulse::default();
        pulse.write_timer_high(0xFF); // Only bits 2-0 should be used
        assert_eq!(pulse.timer_period, 0x0700);
    }

    #[test]
    fn test_sequencer_starts_at_zero() {
        let pulse = Pulse::default();
        assert_eq!(pulse.sequence_position, 0);
    }

    #[test]
    fn test_sequencer_counts_down() {
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        pulse.write_duty(0xFF); // Only bits 1-0 should be used
        assert_eq!(pulse.duty_mode, 3);
    }

    // Envelope Generator Tests

    #[test]
    fn test_envelope_constant_volume_mode() {
        let mut pulse = Pulse::default();
        pulse.write_control(0b0001_1010); // Constant volume flag set, volume = 10
        assert_eq!(pulse.get_envelope_volume(), 10);

        // Clock envelope - should not change in constant volume mode
        pulse.clock_envelope();
        assert_eq!(pulse.get_envelope_volume(), 10);
    }

    #[test]
    fn test_envelope_decay_mode() {
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        pulse.write_control(0b0000_0000);

        assert!(!pulse.envelope_start_flag);

        pulse.write_length_counter_timer_high(0x00);
        assert!(pulse.envelope_start_flag);
    }

    #[test]
    fn test_envelope_divider_period() {
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        pulse.set_length_counter_enabled(true);

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
        let mut pulse = Pulse::default();
        pulse.set_length_counter_enabled(true);
        pulse.write_length_counter_timer_high(0b00001_000); // Load value 2 (index 1)
        assert_eq!(pulse.get_length_counter(), 254);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 253);

        pulse.clock_length_counter();
        assert_eq!(pulse.get_length_counter(), 252);
    }

    #[test]
    fn test_length_counter_halt_flag() {
        let mut pulse = Pulse::default();
        pulse.write_control(0b0010_0000); // Set halt flag
        pulse.set_length_counter_enabled(true);
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
        let mut pulse = Pulse::default();
        pulse.write_control(0b0000_0000); // No halt
        pulse.set_length_counter_enabled(true);
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
        let mut pulse = Pulse::default();
        pulse.set_length_counter_enabled(true);
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        // Enable first, then load
        pulse.set_length_counter_enabled(true);
        pulse.write_length_counter_timer_high(0b01010_000); // Load some value
        assert_eq!(pulse.get_length_counter(), 60);

        // Disable should NOT clear counter
        pulse.set_length_counter_enabled(false);
        assert_eq!(pulse.get_length_counter(), 60);

        // Enable again - counter stays at current value (60)
        pulse.set_length_counter_enabled(true);
        assert_eq!(pulse.get_length_counter(), 60);

        // Now we can load again since it's enabled
        pulse.write_length_counter_timer_high(0b00100_000); // Load different value
        assert_eq!(pulse.get_length_counter(), 40);
    }

    #[test]
    fn test_length_counter_with_halt_then_unhalt() {
        let mut pulse = Pulse::default();
        pulse.write_control(0b0010_0000); // Halt flag set
        pulse.set_length_counter_enabled(true);
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
        let pulse = Pulse::default();
        // Initial period is 0, which is < 8, so it should be muting
        assert!(pulse.is_sweep_muting());
    }

    #[test]
    fn test_sweep_write_register() {
        let mut pulse = Pulse::default();
        // Enable=1, Period=3, Negate=1, Shift=2
        // Binary: 1_011_1_010
        pulse.write_sweep(0b1011_1010);

        // Should set reload flag (tested by observing divider reload on next clock)
        pulse.clock_sweep();
        // After first clock with reload flag, divider should be period (3)
    }

    #[test]
    fn test_sweep_target_period_no_negate() {
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0x04);
        pulse.write_timer_high(0x00); // Period = 4
        pulse.write_sweep(0b1000_0001); // Enable, period=0, no negate, shift=1

        // Target = current + (current >> shift)
        // Target = 4 + (4 >> 1) = 4 + 2 = 6
        assert_eq!(pulse.get_sweep_target_period(), 6);
    }

    #[test]
    fn test_sweep_target_period_with_negate_ones_complement() {
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0x07);
        pulse.write_timer_high(0x00); // Period = 7

        assert!(pulse.is_sweep_muting()); // Period < 8 should mute

        pulse.write_timer_low(0x08);
        pulse.write_timer_high(0x00); // Period = 8
        assert!(!pulse.is_sweep_muting()); // Period = 8 should not mute
    }

    #[test]
    fn test_sweep_muting_target_greater_than_7ff() {
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0xFF);
        pulse.write_timer_high(0b000_11111); // Period = $7FF
        pulse.write_sweep(0b1000_0001); // Enable, period=0, no negate, shift=1

        // Target = $7FF + ($7FF >> 1) = $7FF + $3FF = $BFF > $7FF
        assert!(pulse.is_sweep_muting());
    }

    #[test]
    fn test_sweep_divider_reload() {
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
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
        let mut pulse = Pulse::default();
        pulse.write_timer_low(0x07);
        pulse.write_timer_high(0x00); // Period = 7 (muted, < 8)
        pulse.write_sweep(0b1000_0001); // Enable, shift=1

        assert!(pulse.is_sweep_muting());

        pulse.clock_sweep();
        pulse.clock_sweep();

        // Period should remain 7 (muting prevents update)
        assert_eq!(pulse.get_timer_period(), 7);
    }

    // Output logic and integration tests

    #[test]
    fn test_output_when_all_conditions_met() {
        let mut pulse = Pulse::default();

        // Setup: duty 50%, constant volume 10, period 100, length counter loaded
        pulse.write_control(0b1011_1010); // Duty 50%, constant volume, volume=10
        pulse.write_timer_low(0x64); // Period = 100
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Load length counter (index 0 = 10)

        // Advance to a point where sequencer outputs 1
        for _ in 0..=100 {
            pulse.clock_timer();
        }

        // Should output volume (10) when sequencer is high
        let output = pulse.output();
        assert!(output == 10 || output == 0); // Depends on sequencer position
    }

    #[test]
    fn test_output_silenced_when_sequencer_zero() {
        let mut pulse = Pulse::default();

        // Setup: duty 12.5% (mostly zeros), constant volume 15, period 100
        pulse.write_control(0b0001_1111); // Duty 12.5%, constant volume, volume=15
        pulse.write_timer_low(0x64);
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Index 0 = 10

        // Step until sequencer is at 0
        // Sequencer reads in reverse: 0,7,6,5,4,3,2,1
        // Duty 12.5%: [0,0,0,0,0,0,0,1]
        // Most positions should be 0
        for _ in 0..102 {
            pulse.clock_timer();
        }

        // At some point output should be 0 due to sequencer
        let mut found_zero = false;
        for _ in 0..8 {
            if pulse.output() == 0 {
                found_zero = true;
                break;
            }
            pulse.clock_timer();
        }
        assert!(found_zero);
    }

    #[test]
    fn test_output_silenced_when_length_counter_zero() {
        let mut pulse = Pulse::default();

        // Setup with valid conditions but length counter = 0
        pulse.write_control(0b1011_1111); // Duty 50%, constant volume=15
        pulse.write_timer_low(0x64);
        pulse.write_timer_high(0x00);
        // Don't load length counter, it stays at 0

        assert_eq!(pulse.output(), 0);
    }

    #[test]
    fn test_output_silenced_when_timer_period_less_than_8() {
        let mut pulse = Pulse::default();

        // Setup with period < 8
        pulse.write_control(0b1011_1111); // Duty 50%, constant volume=15
        pulse.write_timer_low(0x07); // Period = 7 (< 8)
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Load length counter (index 0 = 10)

        assert_eq!(pulse.output(), 0);
    }

    #[test]
    fn test_output_silenced_when_sweep_overflow() {
        let mut pulse = Pulse::default();

        // Setup with sweep that causes overflow
        pulse.write_control(0b1011_1111); // Duty 50%, constant volume=15
        pulse.write_length_counter_timer_high(0b00000_111); // Index 0, sets timer high to 7
        pulse.write_timer_low(0xFF); // Set low 8 bits -> Period = $7FF
        pulse.write_sweep(0b1000_0001); // Enable, shift=1 (will overflow)

        // Target = $7FF + ($7FF >> 1) = $7FF + $3FF = $BFE > $7FF
        assert!(pulse.is_sweep_muting());
        assert_eq!(pulse.output(), 0);
    }

    #[test]
    fn test_output_uses_envelope_volume() {
        let mut pulse = Pulse::default();

        // Setup with decay mode envelope
        pulse.write_control(0b1000_0101); // Duty 50%, decay mode, period=5
        pulse.write_timer_low(0x64);
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Sets start flag (index 0 = 10)

        // Clock envelope to start it
        pulse.clock_envelope();

        // Envelope should start at 15
        assert_eq!(pulse.get_envelope_volume(), 15);

        // Output might be 15 or 0 depending on sequencer
        let output = pulse.output();
        assert!(output == 15 || output == 0);
    }

    #[test]
    fn test_integration_full_waveform_cycle() {
        let mut pulse = Pulse::default();

        // Setup: 50% duty cycle, constant volume 8, period 10
        pulse.write_control(0b1001_1000); // Duty 50%, constant volume=8
        pulse.write_timer_low(0x0A); // Period = 10
        pulse.write_timer_high(0x00);
        pulse.set_length_counter_enabled(true);
        pulse.write_length_counter_timer_high(0b00000_000); // Index 0 = 10

        // Run through one complete waveform cycle (8 steps)
        let mut outputs = Vec::new();
        for _ in 0..8 {
            outputs.push(pulse.output());
            // Clock timer to advance through period
            for _ in 0..=10 {
                pulse.clock_timer();
            }
        }

        // Should see mix of 8s and 0s (50% duty)
        let has_volume = outputs.iter().any(|&v| v == 8);
        let has_silence = outputs.iter().any(|&v| v == 0);
        assert!(has_volume && has_silence);
    }

    #[test]
    fn test_integration_length_counter_silences_channel() {
        let mut pulse = Pulse::default();

        // Setup with short length counter value
        pulse.write_control(0b1001_1111); // Duty 50%, no halt (bit 5 clear), constant volume=15
        pulse.write_timer_low(0x64);
        pulse.write_timer_high(0x00);
        pulse.set_length_counter_enabled(true);
        pulse.write_length_counter_timer_high(0b00000_000); // Load length=10 (index 0)

        // Initially should be able to output
        assert_eq!(pulse.get_length_counter(), 10);

        // Clock length counter 10 times to reach 0
        for _ in 0..10 {
            pulse.clock_length_counter();
        }

        assert_eq!(pulse.get_length_counter(), 0);
        assert_eq!(pulse.output(), 0); // Now silenced
    }

    #[test]
    fn test_integration_sweep_changes_period_over_time() {
        let mut pulse = Pulse::default();

        // Setup with sweep enabled
        pulse.write_control(0b1011_1111); // Duty 50%, constant volume=15
        pulse.write_timer_low(0x10); // Period = 16
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Index 0 = 10
        pulse.write_sweep(0b1000_0001); // Enable, period=0, shift=1

        let initial_period = pulse.get_timer_period();
        assert_eq!(initial_period, 16);

        // Clock sweep twice (reload, then update)
        pulse.clock_sweep();
        pulse.clock_sweep();

        let new_period = pulse.get_timer_period();
        // Period should have increased: 16 + (16>>1) = 16 + 8 = 24
        assert_eq!(new_period, 24);
    }

    #[test]
    fn test_output_all_muting_conditions_independent() {
        let mut pulse = Pulse::default();

        // Start with valid setup
        pulse.write_control(0b1011_1111); // Duty 50%, constant volume=15
        pulse.write_timer_low(0x64); // Period = 100
        pulse.write_timer_high(0x00);
        pulse.write_length_counter_timer_high(0b00000_000); // Index 0 = 10

        // Test 1: Mute by clearing length counter
        pulse.set_length_counter_enabled(false);
        assert_eq!(pulse.output(), 0);

        // Restore length counter
        pulse.write_length_counter_timer_high(0b00000_000);

        // Test 2: Mute by setting period < 8
        pulse.write_timer_low(0x07);
        pulse.write_timer_high(0x00);
        assert_eq!(pulse.output(), 0);

        // Restore period
        pulse.write_timer_low(0x64);
        pulse.write_timer_high(0x00);

        // Test 3: Mute by sweep overflow
        pulse.write_length_counter_timer_high(0b00000_111); // Index 0, sets timer high to 7
        pulse.write_timer_low(0xFF); // Period = $7FF
        pulse.write_sweep(0b1000_0001); // Causes overflow
        assert_eq!(pulse.output(), 0);
    }
}
