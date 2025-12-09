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
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
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
            15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,  // Descending
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,  // Ascending
        ];
        
        for expected_value in expected_sequence {
            assert_eq!(triangle.output(), expected_value, 
                "Expected {} at position {}", expected_value, triangle.sequence_position);
            triangle.clock_timer();
        }
        
        // After 32 steps, should wrap back to start
        assert_eq!(triangle.sequence_position, 0);
        assert_eq!(triangle.output(), 15);
    }
}
