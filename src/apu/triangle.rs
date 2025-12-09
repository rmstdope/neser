/// Triangle wave channel for the NES APU
/// Generates triangle waves with a 32-step linear sequence
pub struct Triangle {
    // Timer fields (11-bit)
    timer_period: u16,
    timer_counter: u16,

    // Sequencer fields (32-step triangle wave)
    sequence_position: u8,

    // Linear counter fields
    linear_counter: u8,
    linear_counter_reload_value: u8,
    linear_counter_reload_flag: bool,
    control_flag: bool, // Also acts as length counter halt

    // Length counter fields
    length_counter: u8,
}

/// Length of the triangle wave sequence
const TRIANGLE_SEQUENCE_LENGTH: u8 = 32;

/// Triangle wave sequence (32 steps)
/// Produces values: 15,14,13,...,1,0,0,1,2,...,14,15
const TRIANGLE_SEQUENCE: [u8; TRIANGLE_SEQUENCE_LENGTH as usize] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

/// Length counter lookup table (shared across all channels)
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}

impl Triangle {
    /// Create a new Triangle channel
    pub fn new() -> Self {
        Self {
            timer_period: 0,
            timer_counter: 0,
            sequence_position: 0,
            linear_counter: 0,
            linear_counter_reload_value: 0,
            linear_counter_reload_flag: false,
            control_flag: false,
            length_counter: 0,
        }
    }

    /// Clock the timer (called every APU cycle)
    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.clock_sequencer();
        } else {
            self.timer_counter -= 1;
        }
    }

    /// Clock the sequencer (advances through triangle wave)
    fn clock_sequencer(&mut self) {
        self.sequence_position = (self.sequence_position + 1) % TRIANGLE_SEQUENCE_LENGTH;
    }

    /// Get the current output sample from the triangle channel
    pub fn output(&self) -> u8 {
        TRIANGLE_SEQUENCE[self.sequence_position as usize]
    }

    /// Set the linear counter reload value
    pub fn set_linear_counter_reload(&mut self, value: u8) {
        self.linear_counter_reload_value = value;
    }

    /// Trigger the linear counter to reload from the reload value
    pub fn trigger_linear_counter_reload(&mut self) {
        self.linear_counter = self.linear_counter_reload_value;
    }

    /// Clock the linear counter (called by frame counter)
    pub fn clock_linear_counter(&mut self) {
        if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
    }

    /// Clock the linear counter with reload behavior (called by frame counter quarter frame)
    pub fn clock_linear_counter_with_reload(&mut self) {
        if self.linear_counter_reload_flag {
            self.linear_counter = self.linear_counter_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }

        if !self.control_flag {
            self.linear_counter_reload_flag = false;
        }
    }

    /// Set the linear counter reload flag
    pub fn set_linear_counter_reload_flag(&mut self) {
        self.linear_counter_reload_flag = true;
    }

    /// Check if the linear counter reload flag is set
    pub fn is_linear_counter_reload_flag_set(&self) -> bool {
        self.linear_counter_reload_flag
    }

    /// Set the control flag (also acts as length counter halt)
    pub fn set_control_flag(&mut self, value: bool) {
        self.control_flag = value;
    }

    /// Load the length counter from the lookup table
    pub fn load_length_counter(&mut self, index: u8) {
        let table_index = (index & 0x1F) as usize;
        self.length_counter = LENGTH_COUNTER_TABLE[table_index];
    }

    /// Get the current length counter value
    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }

    /// Clock the length counter (called by frame counter half frame)
    pub fn clock_length_counter(&mut self) {
        if !self.control_flag && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_new() {
        let triangle = Triangle::new();
        assert_eq!(triangle.timer_period, 0);
        assert_eq!(triangle.timer_counter, 0);
        assert_eq!(triangle.sequence_position, 0);
        assert_eq!(triangle.linear_counter, 0);
        assert_eq!(triangle.length_counter, 0);
    }

    #[test]
    fn test_triangle_32_step_sequence() {
        let mut triangle = Triangle::new();
        triangle.timer_period = 0; // Timer clocks every cycle when period is 0

        // The triangle wave should produce values 0-15 ascending, then 15-0 descending
        // Creating a 32-step sequence: 15,14,13,...,1,0,0,1,2,...,14,15
        let expected_sequence = vec![
            15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, // Descending
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, // Ascending
        ];

        for expected_value in expected_sequence {
            assert_eq!(
                triangle.output(),
                expected_value,
                "Expected {} at position {}",
                expected_value,
                triangle.sequence_position
            );
            triangle.clock_timer();
        }

        // After 32 steps, should wrap back to start
        assert_eq!(triangle.sequence_position, 0);
        assert_eq!(triangle.output(), 15);
    }

    #[test]
    fn test_linear_counter_clocking() {
        let mut triangle = Triangle::new();

        // Set up the linear counter with a reload value
        triangle.set_linear_counter_reload(5);
        triangle.trigger_linear_counter_reload();

        // Linear counter should be reloaded to 5
        assert_eq!(triangle.linear_counter, 5);

        // Clock the linear counter - it should decrement
        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 4);

        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 3);

        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 2);

        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 1);

        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 0);

        // Once at zero, should stay at zero
        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 0);
    }

    #[test]
    fn test_linear_counter_reload_flag() {
        let mut triangle = Triangle::new();
        triangle.set_linear_counter_reload(10);

        // Initially, reload flag should be false
        assert!(!triangle.is_linear_counter_reload_flag_set());

        // Set the reload flag (simulates write to $400B)
        triangle.set_linear_counter_reload_flag();
        assert!(triangle.is_linear_counter_reload_flag_set());

        // When flag is set, quarter frame should reload the counter
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.linear_counter, 10);

        // After reload, if control flag is false, the flag should be cleared
        assert!(!triangle.is_linear_counter_reload_flag_set());

        // Now counter should decrement normally
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.linear_counter, 9);

        // If control flag is set, reload flag should NOT be cleared
        triangle.set_control_flag(true);
        triangle.set_linear_counter_reload_flag();
        assert!(triangle.is_linear_counter_reload_flag_set());

        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.linear_counter, 10); // Reloaded

        // Flag should still be set because control flag is true
        assert!(triangle.is_linear_counter_reload_flag_set());

        // Counter keeps reloading while flag is set
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.linear_counter, 10); // Reloaded again
    }

    #[test]
    fn test_length_counter_clocking() {
        let mut triangle = Triangle::new();

        // Load length counter with value (index 5 = value 4)
        triangle.load_length_counter(5);
        assert_eq!(triangle.get_length_counter(), 4);

        // Clock the length counter - it should decrement
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 3);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 2);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 1);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 0);

        // Once at zero, should stay at zero
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 0);
    }

    #[test]
    fn test_length_counter_halt() {
        let mut triangle = Triangle::new();

        // Load length counter
        triangle.load_length_counter(5);
        assert_eq!(triangle.get_length_counter(), 4);

        // Set control flag (which also acts as length counter halt)
        triangle.set_control_flag(true);

        // Clock the length counter - should NOT decrement when halted
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 4);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 4);

        // Clear control flag - should resume counting
        triangle.set_control_flag(false);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 3);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 2);
    }
}
