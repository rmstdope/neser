/// NES APU Noise Channel
///
/// Generates pseudo-random noise using a 15-bit Linear Feedback Shift Register (LFSR).
/// The noise channel includes:
/// - 15-bit LFSR for pseudo-random bit generation
/// - Mode flag (short/long period via different feedback taps)
/// - Timer with period lookup table
/// - Envelope generator for volume control
/// - Length counter

// Period lookup table for NTSC (in CPU cycles)
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

// Length counter lookup table (shared with pulse channels)
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
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
    envelope_start: bool,
    envelope_loop: bool,
    envelope_constant_volume: bool,
    envelope_divider_period: u8,
    envelope_divider: u8,
    envelope_decay_level: u8,

    // Length counter
    length_counter: u8,
    length_counter_halt: bool,
    length_counter_enabled: bool, // Controlled by $4015
}

impl Noise {
    pub fn new() -> Self {
        Noise {
            shift_register: 1, // Power-up state
            mode: false,
            timer: 0,
            timer_period: NOISE_PERIOD_TABLE[0],
            envelope_start: false,
            envelope_loop: false,
            envelope_constant_volume: false,
            envelope_divider_period: 0,
            envelope_divider: 0,
            envelope_decay_level: 0,
            length_counter: 0,
            length_counter_halt: false,
            length_counter_enabled: false, // Disabled at power-on
        }
    }

    /// Clock the timer. When it reaches zero, clock the shift register and reload.
    pub fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.clock_shift_register();
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
    }

    /// Clock the envelope generator
    pub fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_decay_level = 15;
            self.envelope_divider = self.envelope_divider_period;
        } else {
            if self.envelope_divider > 0 {
                self.envelope_divider -= 1;
            } else {
                self.envelope_divider = self.envelope_divider_period;

                if self.envelope_decay_level > 0 {
                    self.envelope_decay_level -= 1;
                } else if self.envelope_loop {
                    self.envelope_decay_level = 15;
                }
            }
        }
    }

    /// Clock the length counter
    pub fn clock_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    /// Write to envelope register ($400C)
    /// Format: --lc vvvv
    /// l = length counter halt / envelope loop
    /// c = constant volume flag
    /// v = volume / envelope period
    pub fn write_envelope(&mut self, value: u8) {
        self.length_counter_halt = (value >> 5) & 1 == 1;
        self.envelope_loop = self.length_counter_halt;
        self.envelope_constant_volume = (value >> 4) & 1 == 1;
        self.envelope_divider_period = value & 0x0F;
    }

    /// Write to period register ($400E)
    /// Format: m--- pppp
    /// m = mode flag (0 = bit 1 feedback, 1 = bit 6 feedback)
    /// p = period index into lookup table
    pub fn write_period(&mut self, value: u8) {
        self.mode = (value >> 7) & 1 == 1;
        let period_index = (value & 0x0F) as usize;
        self.timer_period = NOISE_PERIOD_TABLE[period_index];
    }

    /// Write to length register ($400F)
    /// Format: llll l---
    /// l = length counter load value (index into lookup table)
    pub fn write_length(&mut self, value: u8) {
        // Only load length counter if channel is enabled via $4015
        if self.length_counter_enabled {
            let length_index = ((value >> 3) & 0x1F) as usize;
            self.length_counter = LENGTH_COUNTER_TABLE[length_index];
        }
        self.envelope_start = true;
    }

    /// Get the current output sample (0-15)
    /// Returns 0 if muted (length counter == 0 or shift register bit 0 is set)
    /// Otherwise returns envelope volume
    pub fn output(&self) -> u8 {
        // Muted if length counter is 0 or shift register bit 0 is set
        if self.length_counter == 0 || (self.shift_register & 1) == 1 {
            return 0;
        }

        // Return constant volume or envelope decay level
        if self.envelope_constant_volume {
            self.envelope_divider_period
        } else {
            self.envelope_decay_level
        }
    }

    /// Enable or disable the length counter (controlled by APU status register)
    /// When disabled, the length counter is immediately cleared
    pub fn set_length_counter_enabled(&mut self, enabled: bool) {
        self.length_counter_enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        }
    }

    /// Get the current length counter value
    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_new() {
        let noise = Noise::new();
        assert_eq!(noise.shift_register, 1);
        assert_eq!(noise.mode, false);
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
        noise.envelope_constant_volume = false;
        noise.envelope_divider_period = 2;
        noise.envelope_decay_level = 15;
        noise.envelope_divider = 2;

        // Clock envelope - divider should decrement
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 15); // No change yet
        assert_eq!(noise.envelope_divider, 1);

        // Clock again - divider decrements to 0, then reloads and decay decrements
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 15); // Still no change
        assert_eq!(noise.envelope_divider, 0);

        // Clock third time - divider at 0, reload and decrement decay
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 14);
        assert_eq!(noise.envelope_divider, 2); // Divider reloaded
    }

    #[test]
    fn test_length_counter_clocking() {
        let mut noise = Noise::new();
        noise.length_counter = 10;
        noise.length_counter_halt = false;

        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 9);

        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 8);
    }

    #[test]
    fn test_length_counter_halt() {
        let mut noise = Noise::new();
        noise.length_counter = 5;
        noise.length_counter_halt = true;

        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 5); // Should not decrement when halted
    }

    #[test]
    fn test_write_envelope_register() {
        let mut noise = Noise::new();

        // $400C: --lc vvvv
        // l = length counter halt / envelope loop
        // c = constant volume
        // v = volume/envelope divider period
        noise.write_envelope(0b0001_0101); // halt=0, constant=1, volume=5

        assert_eq!(noise.length_counter_halt, false);
        assert_eq!(noise.envelope_loop, false);
        assert_eq!(noise.envelope_constant_volume, true);
        assert_eq!(noise.envelope_divider_period, 5);
    }

    #[test]
    fn test_write_period_register() {
        let mut noise = Noise::new();

        // $400E: m--- pppp
        // m = mode
        // p = period index
        noise.write_period(0b1000_1010); // mode=1, period=10

        assert_eq!(noise.mode, true);
        assert_eq!(noise.timer_period, NOISE_PERIOD_TABLE[10]);
    }

    #[test]
    fn test_write_length_register() {
        let mut noise = Noise::new();

        // $400F: llll l---
        // l = length counter load
        noise.set_length_counter_enabled(true);
        noise.write_length(0b10110_000); // load index 22

        assert_eq!(noise.length_counter, LENGTH_COUNTER_TABLE[22]);
        assert_eq!(noise.envelope_start, true); // Should trigger envelope restart
    }

    #[test]
    fn test_output_muted_when_length_zero() {
        let mut noise = Noise::new();
        noise.length_counter = 0;
        noise.envelope_decay_level = 10;

        assert_eq!(noise.output(), 0);
    }

    #[test]
    fn test_output_muted_when_shift_register_bit_0_set() {
        let mut noise = Noise::new();
        noise.length_counter = 5;
        noise.envelope_decay_level = 10;
        noise.envelope_constant_volume = false;
        noise.shift_register = 0b0000_0000_0000_0001; // bit 0 set

        assert_eq!(noise.output(), 0);
    }

    #[test]
    fn test_output_uses_envelope_volume() {
        let mut noise = Noise::new();
        noise.length_counter = 5;
        noise.envelope_decay_level = 7;
        noise.envelope_constant_volume = false;
        noise.shift_register = 0b0000_0000_0000_0010; // bit 0 clear

        assert_eq!(noise.output(), 7);
    }

    #[test]
    fn test_output_uses_constant_volume() {
        let mut noise = Noise::new();
        noise.length_counter = 5;
        noise.envelope_divider_period = 12;
        noise.envelope_constant_volume = true;
        noise.shift_register = 0b0000_0000_0000_0010; // bit 0 clear

        assert_eq!(noise.output(), 12);
    }

    #[test]
    fn test_set_length_counter_enabled() {
        let mut noise = Noise::new();
        noise.length_counter = 10;

        noise.set_length_counter_enabled(false);
        assert_eq!(noise.length_counter, 0);

        noise.length_counter = 15;
        noise.set_length_counter_enabled(true);
        assert_eq!(noise.length_counter, 15); // Should not change when enabling
    }
}
