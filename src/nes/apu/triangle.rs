/// Triangle wave channel for the NES APU
/// Generates triangle waves with a 32-step linear sequence
use super::length_counter::LengthCounter;
use crate::trace_apu;
use serde::{Deserialize, Serialize};

/// APU triangle channel state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TriangleState {
    pub timer: u16,
    pub timer_period: u16,
    pub length_counter: u8,
    pub length_counter_enabled: bool,
    pub length_counter_halt: bool,
    pub length_counter_pending_halt: Option<bool>,
    pub length_counter_reload_value: u8,
    pub length_counter_previous_value: u8,
    pub linear_counter: u8,
    pub linear_counter_reload: u8,
    pub linear_counter_reload_flag: bool,
    pub control_flag: bool,
    pub sequence_position: u8,
    #[serde(default)]
    pub output: u8,
}

pub struct Triangle {
    // Timer fields (11-bit)
    timer_period: u16,
    timer_counter: u16,

    // Sequencer fields (32-step triangle wave)
    sequence_position: u8,
    output: u8,

    // Linear counter fields
    linear_counter: u8,
    linear_counter_reload_value: u8,
    linear_counter_reload_flag: bool,
    control_flag: bool, // Also acts as length counter halt

    // Length counter fields
    length_counter: LengthCounter,
}

/// Length of the triangle wave sequence
const TRIANGLE_SEQUENCE_LENGTH: u8 = 32;

/// Triangle wave sequence (32 steps)
/// Produces values: 15,14,13,...,1,0,0,1,2,...,14,15
const TRIANGLE_SEQUENCE: [u8; TRIANGLE_SEQUENCE_LENGTH as usize] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
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
            output: 0,
            linear_counter: 0,
            linear_counter_reload_value: 0,
            linear_counter_reload_flag: false,
            control_flag: false,
            length_counter: LengthCounter::new(),
        }
    }

    /// Reset triangle channel to initial state with length counter disabled
    ///
    /// Unlike other channels, the triangle channel is mostly preserved across reset,
    /// but the length counter is disabled.
    pub fn reset(&mut self) {
        trace_apu!(2; "triangle reset");
        self.length_counter.set_enabled(false);
    }

    /// Clock the timer (called every APU cycle)
    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            // NESDev: The triangle sequencer only advances when the timer clocks
            // and both the length counter and linear counter are non-zero.
            if self.linear_counter != 0 && self.length_counter.value() != 0 {
                self.clock_sequencer();
                trace_apu!(5; "triangle clock_timer advance seq_pos={} timer_period=0x{:03X}", self.sequence_position, self.timer_period);
            }
        } else {
            self.timer_counter -= 1;
        }
    }

    /// Clock the sequencer (advances through triangle wave)
    fn clock_sequencer(&mut self) {
        self.sequence_position = (self.sequence_position + 1) % TRIANGLE_SEQUENCE_LENGTH;
        self.output = TRIANGLE_SEQUENCE[self.sequence_position as usize];
    }

    /// Expose the current sequencer position for integration tests.
    #[cfg(test)]
    pub fn debug_sequence_position(&self) -> u8 {
        self.sequence_position
    }

    /// Get the current output sample from the triangle channel
    pub fn output(&self) -> u8 {
        self.output
    }

    /// Write to $4008 register (linear counter control and reload value)
    pub fn write_linear_counter(&mut self, value: u8) {
        self.control_flag = (value & 0x80) != 0;
        self.length_counter.set_halt(self.control_flag);
        self.linear_counter_reload_value = value & 0x7F;
        trace_apu!(3; "triangle write_linear_counter value=0x{:02X} control={} reload={}", value, self.control_flag, self.linear_counter_reload_value);
    }

    /// Write to $400A register (timer low 8 bits)
    pub fn write_timer_low(&mut self, value: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | (value as u16);
        trace_apu!(4; "triangle write_timer_low value=0x{:02X} period=0x{:03X}", value, self.timer_period);
    }

    /// Write to $400B register (length counter load and timer high 3 bits)
    pub fn write_length_counter_timer_high(&mut self, value: u8) {
        // Set timer high 3 bits (bits 0-2 of value)
        let timer_high = ((value & 0x07) as u16) << 8;
        self.timer_period = (self.timer_period & 0x00FF) | timer_high;

        // Load length counter from bits 3-7 (5 bits)
        let length_index = value >> 3;
        self.load_length_counter(length_index);

        // Set linear counter reload flag
        self.linear_counter_reload_flag = true;
        trace_apu!(3; "triangle write_length_counter_timer_high value=0x{:02X} length_index={} period=0x{:03X}", value, length_index, self.timer_period);
    }

    /// Set the linear counter reload value
    #[cfg(test)]
    pub fn set_linear_counter_reload(&mut self, value: u8) {
        self.linear_counter_reload_value = value;
    }

    /// Trigger the linear counter to reload from the reload value
    #[cfg(test)]
    pub fn trigger_linear_counter_reload(&mut self) {
        self.linear_counter = self.linear_counter_reload_value;
    }

    /// Clock the linear counter (called by frame counter)
    #[cfg(test)]
    pub fn clock_linear_counter(&mut self) {
        if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
    }

    /// Clock the linear counter with reload behavior (called by frame counter quarter frame)
    pub fn clock_linear_counter_with_reload(&mut self) {
        if self.linear_counter_reload_flag {
            self.linear_counter = self.linear_counter_reload_value;
            trace_apu!(5; "triangle linear_counter reload value={}", self.linear_counter);
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
            trace_apu!(5; "triangle linear_counter decrement value={}", self.linear_counter);
        }

        if !self.control_flag {
            self.linear_counter_reload_flag = false;
        }
    }

    /// Set the linear counter reload flag
    #[cfg(test)]
    pub fn set_linear_counter_reload_flag(&mut self) {
        self.linear_counter_reload_flag = true;
    }

    /// Check if the linear counter reload flag is set
    #[cfg(test)]
    pub fn is_linear_counter_reload_flag_set(&self) -> bool {
        self.linear_counter_reload_flag
    }

    /// Set the control flag (also acts as length counter halt)
    #[cfg(test)]
    pub fn set_control_flag(&mut self, value: bool) {
        self.control_flag = value;
        self.length_counter.set_halt(value);
    }

    /// Load the length counter from the lookup table
    pub fn load_length_counter(&mut self, index: u8) {
        self.length_counter.load_from_index(index);
        trace_apu!(3; "triangle load_length_counter index={}", index);
    }

    /// Get the current length counter value
    pub fn get_length_counter(&self) -> u8 {
        self.length_counter.value()
    }

    /// Clear the length counter to 0
    pub fn clear_length_counter(&mut self) {
        self.length_counter.clear();
    }

    /// Get the current linear counter value
    #[cfg(test)]
    pub fn get_linear_counter(&self) -> u8 {
        self.linear_counter
    }

    /// Clock the length counter (called by frame counter half frame)
    pub fn clock_length_counter(&mut self) {
        self.length_counter.clock();
    }

    pub fn apply_pending_length_reload(&mut self) {
        self.length_counter.reload_counter();
    }

    pub fn apply_pending_length_halt(&mut self) {
        self.length_counter.apply_pending_halt();
    }

    /// Set length counter enabled/disabled (from $4015)
    /// When disabled, the length counter is cleared.
    pub fn set_length_counter_enabled(&mut self, enabled: bool) {
        self.length_counter.set_enabled(enabled);
        trace_apu!(2; "triangle set_length_counter_enabled {}", enabled);
    }

    /// Get whether length counter is enabled (from $4015)
    pub fn is_length_counter_enabled(&self) -> bool {
        self.length_counter.is_enabled()
    }

    /// Capture the current triangle channel state for save-state.
    pub fn capture_state(&self) -> TriangleState {
        TriangleState {
            timer: self.timer_counter,
            timer_period: self.timer_period,
            length_counter: self.length_counter.value(),
            length_counter_enabled: self.length_counter.is_enabled(),
            length_counter_halt: self.length_counter.is_halted(),
            length_counter_pending_halt: self.length_counter.pending_halt(),
            length_counter_reload_value: self.length_counter.reload_value(),
            length_counter_previous_value: self.length_counter.previous_value(),
            linear_counter: self.linear_counter,
            linear_counter_reload: self.linear_counter_reload_value,
            linear_counter_reload_flag: self.linear_counter_reload_flag,
            control_flag: self.control_flag,
            sequence_position: self.sequence_position,
            output: self.output,
        }
    }

    /// Restore triangle channel state from a save-state.
    pub fn restore_state(&mut self, state: &TriangleState) {
        self.timer_counter = state.timer;
        self.timer_period = state.timer_period;
        self.length_counter.set_value(state.length_counter);
        if state.length_counter_enabled {
            self.length_counter.enable();
        } else {
            self.length_counter.disable();
        }
        self.length_counter
            .set_halt_state(state.length_counter_halt, state.length_counter_pending_halt);
        self.length_counter.set_reload_state(
            state.length_counter_reload_value,
            state.length_counter_previous_value,
        );
        self.linear_counter = state.linear_counter;
        self.linear_counter_reload_value = state.linear_counter_reload;
        self.linear_counter_reload_flag = state.linear_counter_reload_flag;
        self.control_flag = state.control_flag;
        self.sequence_position = state.sequence_position;
        self.output = state.output;
    }
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)]
mod tests {
    use super::*;

    fn load_length(triangle: &mut Triangle, index: u8) {
        triangle.load_length_counter(index);
        triangle.apply_pending_length_reload();
    }

    fn write_length_high(triangle: &mut Triangle, value: u8) {
        triangle.write_length_counter_timer_high(value);
        triangle.apply_pending_length_reload();
    }

    fn enabled_triangle_with_counters() -> Triangle {
        let mut triangle = Triangle::new();
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 1); // Large non-zero length value
        triangle.write_linear_counter(0x7F); // control=0 (no halt), reload=max
        triangle.trigger_linear_counter_reload();
        triangle
    }

    #[test]
    fn test_triangle_new() {
        let triangle = Triangle::new();
        assert_eq!(triangle.timer_period, 0);
        assert_eq!(triangle.timer_counter, 0);
        assert_eq!(triangle.sequence_position, 0);
        assert_eq!(triangle.output(), 0);
        assert_eq!(triangle.linear_counter, 0);
        assert_eq!(triangle.get_length_counter(), 0);
    }

    #[test]
    fn test_triangle_32_step_sequence() {
        let mut triangle = Triangle::new();
        triangle.timer_period = 0; // Timer clocks every cycle when period is 0

        // Set counters to non-zero to enable output
        triangle.linear_counter = 1;
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 0); // 10

        // The DAC output starts at 0 and updates only when the sequencer clocks.
        // The timer advances the sequence before latching the next DAC value.
        let expected_sequence = vec![
            14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, // Descending
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, // Ascending
            15, // Wrapped to sequence position 0
        ];

        for expected_value in expected_sequence {
            triangle.clock_timer();
            assert_eq!(
                triangle.output(),
                expected_value,
                "Expected {} at position {}",
                expected_value,
                triangle.sequence_position
            );
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
    fn test_linear_counter_reload_timing_requires_reload_flag_and_clears_when_control_clear() {
        let mut triangle = Triangle::new();
        triangle.set_length_counter_enabled(true);

        // 1) Write $4008 with reload=7 (control/halt clear)
        triangle.write_linear_counter(0b0000_0111);

        // 2) Wait several frames (simulate quarter-frame clocks)
        // 3) Verify counter does NOT reload unless reload flag is set.
        for _ in 0..20 {
            triangle.clock_linear_counter_with_reload();
        }
        assert_eq!(triangle.get_linear_counter(), 0);
        assert!(!triangle.is_linear_counter_reload_flag_set());

        // Now set the reload flag via $400B and verify reload happens on quarter frame.
        write_length_high(&mut triangle, 0b0000_0000);
        assert!(triangle.is_linear_counter_reload_flag_set());

        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 7);

        // Reload flag clears when control flag is 0.
        assert!(!triangle.is_linear_counter_reload_flag_set());

        // Subsequent quarter frames decrement.
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 6);
    }

    #[test]
    fn test_linear_counter_halt_control_flag_stops_decrement_and_resumes_when_cleared() {
        let mut triangle = Triangle::new();
        triangle.set_length_counter_enabled(true);

        // Enable halt/control flag and set reload=7.
        triangle.write_linear_counter(0b1000_0111);

        // Set reload flag via $400B.
        write_length_high(&mut triangle, 0b0000_0000);
        assert!(triangle.is_linear_counter_reload_flag_set());

        // While halt/control is set, reload flag persists and the counter reloads each quarter frame.
        for _ in 0..5 {
            triangle.clock_linear_counter_with_reload();
            assert_eq!(triangle.get_linear_counter(), 7);
            assert!(triangle.is_linear_counter_reload_flag_set());
        }

        // Clear halt/control flag; the reload flag should clear on the next quarter frame,
        // and the counter should begin decrementing afterwards.
        triangle.write_linear_counter(0b0000_0111);

        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 7);
        assert!(!triangle.is_linear_counter_reload_flag_set());

        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 6);
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 5);
    }

    #[test]
    fn test_length_counter_clocking() {
        let mut triangle = Triangle::new();

        // Load length counter with value (index 5 = value 4)
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 5);
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
    fn test_length_counter_zero_holds_output_but_linear_counter_still_clocks() {
        let mut triangle = Triangle::new();

        // Set length to a small value: table index 3 => length 2.
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 3);
        assert_eq!(triangle.get_length_counter(), 2);

        // Set linear reload=3 (control/halt clear) and perform a quarter-frame clock
        // to load the counter via the reload flag.
        triangle.write_linear_counter(0b0000_0011);
        triangle.set_linear_counter_reload_flag();
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 3);

        // Output is audible after the timer clocks while both counters are non-zero.
        triangle.clock_timer();
        assert_ne!(triangle.output(), 0);
        let held_output = triangle.output();

        // Length counter decrements on half-frame clocks.
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 1);
        assert_eq!(triangle.output(), held_output);

        // Output holds when length reaches zero; only sequencer advancement stops.
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 0);
        assert_eq!(triangle.output(), held_output);

        // Linear counter still behaves internally while length=0 halts the sequencer.
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 2);
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 1);
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 0);

        // And reload still works irrespective of length counter being zero.
        triangle.set_linear_counter_reload_flag();
        triangle.clock_linear_counter_with_reload();
        assert_eq!(triangle.get_linear_counter(), 3);
        assert_eq!(triangle.output(), held_output);
    }

    #[test]
    fn test_length_counter_halt() {
        let mut triangle = Triangle::new();

        // Load length counter
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 5);
        assert_eq!(triangle.get_length_counter(), 4);

        // Set control flag (which also acts as length counter halt)
        triangle.set_control_flag(true);
        triangle.apply_pending_length_halt();

        // Clock the length counter - should NOT decrement when halted
        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 4);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 4);

        // Clear control flag - should resume counting
        triangle.set_control_flag(false);
        triangle.apply_pending_length_halt();

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 3);

        triangle.clock_length_counter();
        assert_eq!(triangle.get_length_counter(), 2);
    }

    #[test]
    fn test_write_linear_counter_register() {
        let mut triangle = Triangle::new();

        // $4008: CRRR.RRRR - Control flag and reload value
        // C: Control flag (length counter halt / linear counter reload persistence)
        // R: Linear counter reload value (7 bits)

        // Write 0b1010_1010 (control=1, reload=42)
        triangle.write_linear_counter(0b1010_1010);
        assert_eq!(triangle.linear_counter_reload_value, 42);
        assert!(triangle.control_flag);

        // Write 0b0011_1111 (control=0, reload=63)
        triangle.write_linear_counter(0b0011_1111);
        assert_eq!(triangle.linear_counter_reload_value, 63);
        assert!(!triangle.control_flag);

        // Write 0b1111_1111 (control=1, reload=127, max value)
        triangle.write_linear_counter(0b1111_1111);
        assert_eq!(triangle.linear_counter_reload_value, 127);
        assert!(triangle.control_flag);
    }

    #[test]
    fn test_write_timer_low_register() {
        let mut triangle = Triangle::new();

        // $400A: TTTT.TTTT - Timer low 8 bits
        triangle.write_timer_low(0xAB);
        assert_eq!(triangle.timer_period & 0xFF, 0xAB);

        triangle.write_timer_low(0x12);
        assert_eq!(triangle.timer_period & 0xFF, 0x12);
    }

    #[test]
    fn test_write_length_counter_timer_high_register() {
        let mut triangle = Triangle::new();

        // $400B: LLLL.LTTT - Length counter load (5 bits) and timer high (3 bits)
        // Sets linear counter reload flag

        // Write 0b1010_1101 (length index=21, timer high=0b101)
        triangle.set_length_counter_enabled(true);
        triangle.write_timer_low(0xFF); // Set low bits first
        write_length_high(&mut triangle, 0b1010_1101);

        // Check length counter loaded from table (index 21 = value 20)
        assert_eq!(triangle.get_length_counter(), 20);

        // Check timer high bits (bits 0-2)
        let expected_timer = (0b101 << 8) | 0xFF;
        assert_eq!(triangle.timer_period, expected_timer);

        // Check that linear counter reload flag was set
        assert!(triangle.is_linear_counter_reload_flag_set());
    }

    #[test]
    fn test_output_holds_last_value_when_counters_zero() {
        let mut triangle = Triangle::new();
        triangle.timer_period = 0;
        triangle.set_length_counter_enabled(true);

        // With no prior DAC output, halted triangle output remains 0.
        triangle.set_linear_counter_reload(0);
        triangle.trigger_linear_counter_reload();
        load_length(&mut triangle, 1); // Length counter = 254
        assert_eq!(triangle.output(), 0);

        // With no prior DAC output, length-gated triangle output remains 0.
        triangle.set_linear_counter_reload(10);
        triangle.trigger_linear_counter_reload();
        triangle.clear_length_counter();
        assert_eq!(triangle.output(), 0);

        // Triangle latches a non-zero DAC value once both counters are non-zero
        // and the timer clocks the sequencer.
        triangle.linear_counter = 5;
        load_length(&mut triangle, 1); // Length counter = 254
        triangle.clock_timer();
        assert_ne!(triangle.output(), 0);
    }

    #[test]
    fn test_output_holds_immediately_when_linear_counter_zero_and_recovers() {
        let mut triangle = Triangle::new();
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 1); // Non-zero length

        // Start audible.
        triangle.linear_counter = 5;
        triangle.clock_timer();
        assert_ne!(triangle.output(), 0);
        let held_output = triangle.output();

        // Force linear counter to zero: output holds its last DAC value.
        triangle.linear_counter = 0;
        assert_eq!(triangle.output(), held_output);

        // Recover by restoring linear counter to non-zero.
        triangle.linear_counter = 5;
        assert_eq!(triangle.output(), held_output);
    }

    #[test]
    fn test_output_holds_immediately_when_length_counter_zero_and_recovers() {
        let mut triangle = Triangle::new();
        triangle.set_length_counter_enabled(true);

        // Start audible.
        triangle.linear_counter = 5;
        load_length(&mut triangle, 3); // index 3 => length 2
        triangle.clock_timer();
        assert_ne!(triangle.output(), 0);
        let held_output = triangle.output();

        // Force length counter to zero: output holds its last DAC value.
        triangle.clear_length_counter();
        assert_eq!(triangle.output(), held_output);

        // Recover by reloading length to a non-zero value.
        load_length(&mut triangle, 3);
        assert_eq!(triangle.output(), held_output);
    }

    #[test]
    fn test_set_length_counter_enabled() {
        let mut triangle = Triangle::new();

        // Enable first, then load a length counter value
        triangle.set_length_counter_enabled(true);
        load_length(&mut triangle, 5);
        assert_eq!(triangle.get_length_counter(), 4);

        // Disabling via $4015 clears the length counter
        triangle.set_length_counter_enabled(false);
        assert_eq!(triangle.get_length_counter(), 0);

        // Load again while disabled - should NOT load (NES hardware behavior)
        // Length counter can only be loaded when channel is enabled via $4015
        load_length(&mut triangle, 10);
        assert_eq!(triangle.get_length_counter(), 0); // Cleared when disabled

        // Enabling should not affect the counter (stays at 4)
        triangle.set_length_counter_enabled(true);
        assert_eq!(triangle.get_length_counter(), 0);

        // Now that it's enabled, we can load a value
        load_length(&mut triangle, 11);
        assert_eq!(triangle.get_length_counter(), 10); // Index 11 = value 10
    }

    #[test]
    fn test_triangle_timer_period_sweep_no_skipping_steps() {
        let mut triangle = enabled_triangle_with_counters();

        // Sweep a few representative timer periods from small -> larger.
        // For each period, the sequencer should advance by exactly 1 step every (period + 1)
        // timer clocks, with no skipping.
        let periods = [0u16, 1, 2, 3, 7, 15, 31, 63];

        for &period in &periods {
            triangle.timer_period = period;
            triangle.timer_counter = period; // Avoid the special "starts at 0" immediate-clock case

            let mut prev_pos = triangle.sequence_position;
            let mut steps_seen = 0u32;
            let mut clocks_since_step = 0u16;

            while steps_seen < 64 {
                triangle.clock_timer();
                clocks_since_step += 1;

                if triangle.sequence_position != prev_pos {
                    // Step occurred; it should be exactly +1 mod 32.
                    let expected = (prev_pos + 1) % TRIANGLE_SEQUENCE_LENGTH;
                    assert_eq!(
                        triangle.sequence_position, expected,
                        "period={}: step skipped: prev={}, got={}, expected={}",
                        period, prev_pos, triangle.sequence_position, expected
                    );

                    assert_eq!(
                        clocks_since_step,
                        period + 1,
                        "period={}: step interval mismatch",
                        period
                    );

                    prev_pos = triangle.sequence_position;
                    clocks_since_step = 0;
                    steps_seen += 1;
                }
            }
        }
    }

    #[test]
    fn test_triangle_timer_period_change_does_not_affect_current_countdown() {
        let mut triangle = enabled_triangle_with_counters();

        // Start mid-countdown.
        triangle.timer_period = 5;
        triangle.timer_counter = 3;
        let start_pos = triangle.sequence_position;

        // Change period while countdown is in progress.
        triangle.timer_period = 10;

        // The next sequencer tick should occur after the remaining countdown reaches 0,
        // i.e. after 4 timer clocks (3 -> 2 -> 1 -> 0, then tick).
        for _ in 0..3 {
            triangle.clock_timer();
            assert_eq!(triangle.sequence_position, start_pos);
        }

        // On the 4th clock, the sequencer should tick exactly once.
        triangle.clock_timer();
        assert_eq!(
            triangle.sequence_position,
            (start_pos + 1) % TRIANGLE_SEQUENCE_LENGTH
        );

        // And after ticking, it should have reloaded with the new period (10).
        assert_eq!(triangle.timer_counter, 10);
    }

    #[test]
    fn test_triangle_sequencer_does_not_advance_when_linear_counter_zero() {
        let mut triangle = enabled_triangle_with_counters();
        triangle.linear_counter = 0;
        triangle.timer_period = 0;
        triangle.timer_counter = 0;

        let start_pos = triangle.sequence_position;
        for _ in 0..100 {
            triangle.clock_timer();
        }

        assert_eq!(
            triangle.sequence_position, start_pos,
            "Sequencer should not advance when linear counter is zero"
        );
    }

    #[test]
    fn test_triangle_sequencer_does_not_advance_when_length_counter_zero() {
        let mut triangle = enabled_triangle_with_counters();
        triangle.clear_length_counter();
        triangle.timer_period = 0;
        triangle.timer_counter = 0;

        let start_pos = triangle.sequence_position;
        for _ in 0..100 {
            triangle.clock_timer();
        }

        assert_eq!(
            triangle.sequence_position, start_pos,
            "Sequencer should not advance when length counter is zero"
        );
    }

    #[test]
    fn reset_disables_length_counter_but_preserves_other_state() {
        let mut triangle = Triangle::new();
        // Set up triangle with various state
        triangle.write_linear_counter(0b1000_0111); // control=1, reload=7
        triangle.write_timer_low(0x50);
        write_length_high(&mut triangle, 0b00001_010); // length index=0, timer high=2
        triangle.set_length_counter_enabled(true);

        // Clock a bit to build state
        triangle.linear_counter = 5;
        triangle.sequence_position = 10;
        triangle.timer_counter = 20;

        // Verify state is set
        assert!(triangle.is_length_counter_enabled());
        assert_eq!(triangle.linear_counter, 5);
        assert_eq!(triangle.sequence_position, 10);

        // Reset
        triangle.reset();

        // Verify length counter is disabled
        assert!(!triangle.is_length_counter_enabled());

        // Verify other state is preserved (triangle is special - mostly preserved across reset)
        assert_eq!(triangle.linear_counter, 5);
        assert_eq!(triangle.sequence_position, 10);
    }
}
