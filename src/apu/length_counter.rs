//! APU length counter
//!
//! Specs: https://www.nesdev.org/apu_ref.txt
use crate::trace_apu;

/// Length counter lookup table (32 entries), indexed by bits 7-3 of $4003/$4007/$400B/$400F.
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LengthCounter {
    enabled: bool,
    halt: bool,
    value: u8,
}

impl LengthCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset length counter to initial state
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn lookup(index: u8) -> u8 {
        LENGTH_COUNTER_TABLE[(index & 0x1F) as usize]
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        trace_apu!(2; "length_counter set_enabled {}", enabled);
        self.enabled = enabled;
        if !enabled {
            // NESDev: when a channel is disabled via $4015, its length counter is cleared.
            self.value = 0;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_halt(&mut self, halt: bool) {
        trace_apu!(3; "length_counter set_halt {}", halt);
        self.halt = halt;
    }

    pub fn is_halted(&self) -> bool {
        self.halt
    }

    pub fn clear(&mut self) {
        trace_apu!(4; "length_counter clear");
        self.value = 0;
    }

    pub fn load_from_index(&mut self, index: u8) {
        if self.enabled {
            trace_apu!(3; "length_counter load index={} value={}", index & 0x1F, Self::lookup(index));
            self.value = Self::lookup(index);
        }
    }

    pub fn clock(&mut self) {
        if !self.halt && self.value > 0 {
            self.value -= 1;
            trace_apu!(5; "length_counter clock value={}", self.value);
        }
    }

    pub fn value(&self) -> u8 {
        self.value
    }

    /// Set the length counter value directly (for save-state restore).
    pub fn set_value(&mut self, value: u8) {
        self.value = value;
    }

    /// Enable the length counter (for save-state restore).
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable the length counter (for save-state restore).
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_counter_table_matches_nesdev() {
        // Values from NESDev APU reference: 32-entry length table.
        let expected: [u8; 32] = [
            10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20,
            96, 22, 192, 24, 72, 26, 16, 28, 32, 30,
        ];

        for (i, &value) in expected.iter().enumerate() {
            assert_eq!(LengthCounter::lookup(i as u8), value, "index {i}");
        }
    }

    #[test]
    fn load_does_nothing_when_disabled() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(false);

        lc.load_from_index(0);
        assert_eq!(lc.value(), 0);
    }

    #[test]
    fn load_sets_value_when_enabled() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);

        lc.load_from_index(0);
        assert_eq!(lc.value(), 10);

        lc.load_from_index(1);
        assert_eq!(lc.value(), 254);
    }

    #[test]
    fn clock_decrements_when_not_halted() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.set_halt(false);

        lc.load_from_index(0); // 10
        lc.clock();
        assert_eq!(lc.value(), 9);
    }

    #[test]
    fn clock_does_not_decrement_when_halted() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.set_halt(true);

        lc.load_from_index(0); // 10
        lc.clock();
        assert_eq!(lc.value(), 10);
    }

    #[test]
    fn disabling_clears_the_counter_immediately() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.load_from_index(0);
        assert_eq!(lc.value(), 10);

        lc.set_enabled(false);
        assert_eq!(lc.value(), 0);
    }

    #[test]
    fn reset_restores_length_counter_to_initial_state() {
        let mut lc = LengthCounter::new();
        // Modify all fields
        lc.set_enabled(true);
        lc.set_halt(true);
        lc.load_from_index(1); // 254

        // Verify state changed
        assert!(lc.is_enabled());
        assert!(lc.is_halted());
        assert_eq!(lc.value(), 254);

        // Reset
        lc.reset();

        // Verify all fields back to default
        assert!(!lc.is_enabled());
        assert!(!lc.is_halted());
        assert_eq!(lc.value(), 0);
    }
}
