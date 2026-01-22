/// NES APU Noise Channel
///
/// Generates pseudo-random noise using a 15-bit Linear Feedback Shift Register (LFSR).
/// The noise channel includes:
/// - 15-bit LFSR for pseudo-random bit generation
/// - Mode flag (short/long period via different feedback taps)
/// - Timer with period lookup table
/// - Envelope generator for volume control
/// - Length counter
use super::envelope::Envelope;
use super::length_counter::LengthCounter;
use crate::trace_apu;

// Period lookup table for NTSC (in CPU cycles)
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

pub struct Noise {
    // Linear Feedback Shift Register (15-bit)
    shift_register: u16,

    // Mode flag: false = bit 1 feedback, true = bit 6 feedback
    mode: bool,

    // Timer
    timer: u16,
    timer_period: u16,

    // Envelope
    envelope: Envelope,

    // Length counter
    length_counter: LengthCounter,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

impl Noise {
    pub fn new() -> Self {
        Noise {
            shift_register: 1, // Power-up state
            mode: false,
            timer: 0,
            timer_period: NOISE_PERIOD_TABLE[0],
            envelope: Envelope::new(),
            length_counter: LengthCounter::new(),
        }
    }

    /// Reset noise channel to initial state
    pub fn reset(&mut self) {
        trace_apu!(2; "noise reset");
        *self = Self::new();
    }

    /// Clock the timer. When it reaches zero, clock the shift register and reload.
    pub fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.clock_shift_register();
            trace_apu!(5; "noise clock_timer reload period={} shift=0x{:04X}", self.timer_period, self.shift_register);
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the shift register to generate the next pseudo-random bit
    fn clock_shift_register(&mut self) {
        // 1. Calculate feedback: XOR of bit 0 and either bit 1 (mode 0) or bit 6 (mode 1)
        let bit0 = self.shift_register & 1;
        let other_bit = if self.mode {
            (self.shift_register >> 6) & 1
        } else {
            (self.shift_register >> 1) & 1
        };
        let feedback = bit0 ^ other_bit;

        // 2. Shift register right by one bit
        self.shift_register >>= 1;

        // 3. Set bit 14 to the feedback value
        self.shift_register = (self.shift_register & 0x3FFF) | (feedback << 14);
        trace_apu!(5; "noise shift_register mode={} feedback={} value=0x{:04X}", self.mode, feedback, self.shift_register);
    }

    /// Clock the envelope generator
    pub fn clock_envelope(&mut self) {
        self.envelope.clock();
    }

    /// Clock the length counter
    pub fn clock_length_counter(&mut self) {
        self.length_counter.clock();
    }

    pub fn apply_pending_length_reload(&mut self) {
        self.length_counter.reload_counter();
    }

    pub fn apply_pending_length_halt(&mut self) {
        self.length_counter.apply_pending_halt();
    }

    /// Write to envelope register ($400C)
    /// Format: --lc vvvv
    /// l = length counter halt / envelope loop
    /// c = constant volume flag
    /// v = volume / envelope period
    pub fn write_envelope(&mut self, value: u8) {
        let halt = (value >> 5) & 1 == 1;
        self.length_counter.set_halt(halt);
        self.envelope.write_control(value);
        trace_apu!(3; "noise write_envelope value=0x{:02X} halt={}", value, halt);
    }

    /// Write to period register ($400E)
    /// Format: m--- pppp
    /// m = mode flag (0 = bit 1 feedback, 1 = bit 6 feedback)
    /// p = period index into lookup table
    pub fn write_period(&mut self, value: u8) {
        self.mode = (value >> 7) & 1 == 1;
        let period_index = (value & 0x0F) as usize;
        self.timer_period = NOISE_PERIOD_TABLE[period_index];
        trace_apu!(3; "noise write_period value=0x{:02X} mode={} period_index={} period={}", value, self.mode, period_index, self.timer_period);
    }

    /// Write to length register ($400F)
    /// Format: llll l---
    /// l = length counter load value (index into lookup table)
    pub fn write_length(&mut self, value: u8) {
        // Only loads if enabled via $4015 (handled by LengthCounter)
        let length_index = (value >> 3) & 0x1F;
        self.length_counter.load_from_index(length_index);
        self.envelope.restart();
        trace_apu!(3; "noise write_length value=0x{:02X} length_index={}", value, length_index);
    }

    /// Get the current output sample (0-15)
    /// Returns 0 if muted (length counter == 0 or shift register bit 0 is set)
    /// Otherwise returns envelope volume
    pub fn output(&self) -> u8 {
        // Muted if length counter is disabled/0 or shift register bit 0 is set
        if !self.length_counter.is_enabled()
            || self.length_counter.value() == 0
            || (self.shift_register & 1) == 1
        {
            return 0;
        }

        self.envelope.volume()
    }

    /// Set length counter enabled/disabled (from $4015)
    /// When disabled, the length counter is immediately cleared
    pub fn set_length_counter_enabled(&mut self, enabled: bool) {
        self.length_counter.set_enabled(enabled);
        trace_apu!(2; "noise set_length_counter_enabled {}", enabled);
    }

    /// Get whether length counter is enabled (from $4015)
    pub fn is_length_counter_enabled(&self) -> bool {
        self.length_counter.is_enabled()
    }

    /// Get the current length counter value
    pub fn get_length_counter(&self) -> u8 {
        self.length_counter.value()
    }

    /// Clear the length counter to 0
    pub fn clear_length_counter(&mut self) {
        self.length_counter.clear();
    }

    /// Capture the current noise channel state for save-state.
    pub fn capture_state(&self) -> crate::savestate::NoiseState {
        crate::savestate::NoiseState {
            timer: self.timer,
            timer_period: self.timer_period,
            length_counter: self.length_counter.value(),
            length_counter_enabled: self.length_counter.is_enabled(),
            length_counter_halt: self.length_counter.is_halted(),
            length_counter_pending_halt: self.length_counter.pending_halt(),
            length_counter_reload_value: self.length_counter.reload_value(),
            length_counter_previous_value: self.length_counter.previous_value(),
            envelope: self.envelope.capture_state(),
            mode_flag: self.mode,
            shift_register: self.shift_register,
        }
    }

    /// Restore noise channel state from a save-state.
    pub fn restore_state(&mut self, state: &crate::savestate::NoiseState) {
        self.timer = state.timer;
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
        self.envelope.restore_state(&state.envelope);
        self.mode = state.mode_flag;
        self.shift_register = state.shift_register;
    }
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)]
mod tests {
    use super::*;

    fn write_length(noise: &mut Noise, value: u8) {
        noise.write_length(value);
        noise.apply_pending_length_reload();
    }

    #[test]
    fn test_noise_new() {
        let noise = Noise::new();
        assert_eq!(noise.shift_register, 1);
        assert!(!noise.mode);
        assert_eq!(noise.timer_period, 4);
    }

    #[test]
    fn test_lfsr_mode_0_feedback() {
        // Mode 0: feedback from bits 0 and 1
        let mut noise = Noise::new();
        noise.shift_register = 0b0000_0000_0000_0011; // bits 0 and 1 both set

        noise.clock_shift_register();

        // Feedback = bit 0 XOR bit 1 = 1 XOR 1 = 0
        // After right shift: bit 14 should be 0
        // Original: 0000_0000_0000_0011
        // After shift right: 0000_0000_0000_0001
        // With feedback in bit 14: 0000_0000_0000_0001 (bit 14 = 0)
        assert_eq!(noise.shift_register, 0b0000_0000_0000_0001);
    }

    #[test]
    fn test_timer_clocks_shift_register() {
        let mut noise = Noise::new();
        noise.shift_register = 0b0000_0000_0000_0001; // Only bit 0 set
        noise.timer_period = 4; // Period of 4 CPU cycles
        noise.timer = 4; // Start with timer loaded

        // Clock 3 times - should decrement but not clock shift register yet
        noise.clock_timer(); // timer: 4 -> 3
        noise.clock_timer(); // timer: 3 -> 2
        noise.clock_timer(); // timer: 2 -> 1
        assert_eq!(noise.shift_register, 0b0000_0000_0000_0001); // Unchanged

        // Clock 4th time - should decrement to 0
        noise.clock_timer(); // timer: 1 -> 0
        assert_eq!(noise.shift_register, 0b0000_0000_0000_0001); // Still unchanged

        // Clock 5th time - timer at 0, should reload and clock shift register
        noise.clock_timer();
        // Feedback = bit 0 XOR bit 1 = 1 XOR 0 = 1
        // After shift: 0b0100_0000_0000_0000 (bit 14 = 1, others shifted right)
        assert_eq!(noise.shift_register, 0b0100_0000_0000_0000);
    }

    #[test]
    fn test_lfsr_mode_1_feedback() {
        // Mode 1: feedback from bits 0 and 6
        let mut noise = Noise::new();
        noise.mode = true;
        noise.shift_register = 0b0000_0000_0100_0001; // bits 0 and 6 both set

        noise.clock_shift_register();

        // Feedback = bit 0 XOR bit 6 = 1 XOR 1 = 0
        // After right shift: 0b0000_0000_0010_0000
        // With feedback (0) in bit 14: 0b0000_0000_0010_0000
        assert_eq!(noise.shift_register, 0b0000_0000_0010_0000);
    }

    #[test]
    fn test_envelope_decay_mode() {
        let mut noise = Noise::new();
        noise.envelope.write_control(0b0000_0010); // loop=0, disable=0, n=2
        noise.envelope.restart();

        // Restart should be applied on first clock.
        noise.clock_envelope();
        assert_eq!(noise.envelope.debug_counter(), 15);
        assert_eq!(noise.envelope.debug_divider(), 2);

        // Clock envelope - divider should decrement
        noise.clock_envelope();
        assert_eq!(noise.envelope.debug_counter(), 15); // No change yet
        assert_eq!(noise.envelope.debug_divider(), 1);

        // Clock again - divider decrements to 0, then reloads and decay decrements
        noise.clock_envelope();
        assert_eq!(noise.envelope.debug_counter(), 15); // Still no change
        assert_eq!(noise.envelope.debug_divider(), 0);

        // Clock third time - divider at 0, reload and decrement decay
        noise.clock_envelope();
        assert_eq!(noise.envelope.debug_counter(), 14);
        assert_eq!(noise.envelope.debug_divider(), 2); // Divider reloaded
    }

    #[test]
    fn test_length_counter_clocking() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true);
        noise.length_counter.load_from_index(0); // 10
        noise.length_counter.reload_counter();
        noise.length_counter.set_halt(false);
        noise.length_counter.apply_pending_halt();

        noise.clock_length_counter();
        assert_eq!(noise.get_length_counter(), 9);

        noise.clock_length_counter();
        assert_eq!(noise.get_length_counter(), 8);
    }

    #[test]
    fn test_length_counter_halt() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true);
        noise.length_counter.load_from_index(0); // 10
        noise.length_counter.reload_counter();
        noise.length_counter.set_halt(true);
        noise.length_counter.apply_pending_halt();

        noise.clock_length_counter();
        assert_eq!(noise.get_length_counter(), 10); // Should not decrement when halted
    }

    #[test]
    fn test_write_envelope_register() {
        let mut noise = Noise::new();

        // $400C: --lc vvvv
        // l = length counter halt / envelope loop
        // c = constant volume
        // v = volume/envelope divider period
        noise.write_envelope(0b0001_0101); // halt=0, constant=1, volume=5

        assert!(!noise.length_counter.is_halted());
        assert!(!noise.envelope.debug_loop_flag());
        assert!(noise.envelope.debug_disable_flag());
        assert_eq!(noise.envelope.debug_n(), 5);
    }

    #[test]
    fn test_write_period_register() {
        let mut noise = Noise::new();

        // $400E: m--- pppp
        // m = mode
        // p = period index
        noise.write_period(0b1000_1010); // mode=1, period=10

        assert!(noise.mode);
        assert_eq!(noise.timer_period, NOISE_PERIOD_TABLE[10]);
    }

    #[test]
    fn test_write_length_register() {
        let mut noise = Noise::new();

        // $400F: llll l---
        // l = length counter load
        noise.set_length_counter_enabled(true);
        write_length(&mut noise, 0b10110_000); // load index 22

        assert_eq!(noise.get_length_counter(), LengthCounter::lookup(22));
        assert!(noise.envelope.debug_start_flag()); // Should trigger envelope restart
    }

    #[test]
    fn test_output_muted_when_length_zero() {
        let mut noise = Noise::new();
        noise.length_counter.clear();
        noise.write_envelope(0b0001_1010); // constant volume 10

        assert_eq!(noise.output(), 0);
    }

    #[test]
    fn test_output_muted_when_shift_register_bit_0_set() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true);
        noise.length_counter.load_from_index(0); // 10
        noise.write_envelope(0b0001_1010); // constant volume 10
        noise.shift_register = 0b0000_0000_0000_0001; // bit 0 set

        assert_eq!(noise.output(), 0);
    }

    #[test]
    fn test_output_uses_envelope_volume() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true); // Must be enabled for output
        noise.length_counter.load_from_index(0); // 10
        noise.write_envelope(0b0000_0000); // decay mode, n=0
        // Trigger envelope restart like a real length write would.
        write_length(&mut noise, 0b00000_000);
        noise.clock_envelope();

        // With n=0, the counter decrements every envelope clock.
        for _ in 0..8 {
            noise.clock_envelope();
        }
        assert_eq!(noise.envelope.debug_counter(), 7);
        noise.shift_register = 0b0000_0000_0000_0010; // bit 0 clear

        assert_eq!(noise.output(), 7);
    }

    #[test]
    fn test_output_uses_constant_volume() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true); // Must be enabled for output
        noise.length_counter.load_from_index(0); // 10
        noise.length_counter.reload_counter();
        noise.write_envelope(0b0001_1100); // constant volume 12
        noise.shift_register = 0b0000_0000_0000_0010; // bit 0 clear

        assert_eq!(noise.output(), 12);
    }

    #[test]
    fn test_set_length_counter_enabled() {
        let mut noise = Noise::new();
        noise.set_length_counter_enabled(true);
        noise.length_counter.load_from_index(0);
        noise.length_counter.reload_counter();
        assert_eq!(noise.get_length_counter(), 10);

        // Disabling via $4015 clears length counter
        noise.set_length_counter_enabled(false);
        assert_eq!(noise.get_length_counter(), 0);

        noise.set_length_counter_enabled(true);
        assert_eq!(noise.get_length_counter(), 0);
    }

    #[test]
    fn reset_restores_noise_to_initial_state() {
        let mut noise = Noise::new();
        // Modify all fields
        noise.write_period(0b1000_1111); // mode=1, period index=15
        noise.write_envelope(0b0011_1010); // halt=1, constant=1, volume=10
        noise.set_length_counter_enabled(true);
        write_length(&mut noise, 0b00000_000); // length index=0 (value=10)
        for _ in 0..1000 {
            noise.clock_timer();
        }

        // Verify state changed
        assert!(noise.mode); // short mode
        assert_eq!(noise.get_length_counter(), 10);
        assert_ne!(noise.shift_register, 1);

        // Reset
        noise.reset();

        // Verify all fields back to default
        assert_eq!(noise.shift_register, 1); // Power-up state
        assert!(!noise.mode);
        assert_eq!(noise.get_length_counter(), 0);
        assert!(!noise.is_length_counter_enabled());
    }
}
