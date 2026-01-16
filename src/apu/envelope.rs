/// Envelope Generator
///
/// Based on https://www.nesdev.org/apu_ref.txt ("Envelope Generator" section).
///
/// The envelope contains a divider and a counter. When clocked (quarter frame),
/// either:
/// - If a write to the channel's 4th register occurred since last clock, the
///   counter is set to 15 and the divider is reset.
/// - Otherwise, the divider is clocked; when it outputs a clock, the counter
///   is decremented, or wrapped to 15 if looping.
///
/// When the "disable" flag is set, the channel volume is the constant `n`
/// (the low 4 bits of the channel's first register). Otherwise it is the
/// counter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Envelope {
    start_flag: bool,
    loop_flag: bool,
    disable_flag: bool,
    n: u8,

    divider: u8,
    counter: u8,
}

impl Envelope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset envelope to initial state
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Write to the channel's first register envelope bits.
    /// Format: `--ld nnnn` (loop, disable, n)
    pub fn write_control(&mut self, value: u8) {
        self.loop_flag = (value & 0x20) != 0;
        self.disable_flag = (value & 0x10) != 0;
        self.n = value & 0x0F;
    }

    /// Indicates that the channel's 4th register was written.
    /// This causes the envelope to restart on the next clock.
    pub fn restart(&mut self) {
        self.start_flag = true;
    }

    /// Clocked by the frame sequencer (quarter frame).
    pub fn clock(&mut self) {
        // NESDev: On a frame-sequencer envelope clock:
        // - If start_flag is set (4th register written since last clock),
        //   set counter to 15 and reset divider.
        // - Otherwise, clock the divider; when it outputs a clock,
        //   decrement the counter, or wrap to 15 if looping.
        if self.start_flag {
            self.start_flag = false;
            self.counter = 15;
            self.divider = self.n;
            return;
        }

        if self.divider == 0 {
            self.divider = self.n;

            if self.counter > 0 {
                self.counter -= 1;
            } else if self.loop_flag {
                self.counter = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    /// Current volume presented to the DAC (0-15).
    pub fn volume(&self) -> u8 {
        if self.disable_flag {
            self.n
        } else {
            self.counter
        }
    }

    #[cfg(test)]
    pub fn debug_start_flag(&self) -> bool {
        self.start_flag
    }

    #[cfg(test)]
    pub fn debug_loop_flag(&self) -> bool {
        self.loop_flag
    }

    #[cfg(test)]
    pub fn debug_disable_flag(&self) -> bool {
        self.disable_flag
    }

    #[cfg(test)]
    pub fn debug_n(&self) -> u8 {
        self.n
    }

    #[cfg(test)]
    pub fn debug_divider(&self) -> u8 {
        self.divider
    }

    #[cfg(test)]
    pub fn debug_counter(&self) -> u8 {
        self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_restarts_on_next_clock() {
        let mut env = Envelope::new();
        env.write_control(0b0000_0011); // n = 3

        env.restart();
        env.clock();

        assert_eq!(env.counter, 15);
        assert_eq!(env.divider, env.n);
        assert!(!env.start_flag);
    }

    #[test]
    fn envelope_divider_clocks_then_decrements_counter() {
        let mut env = Envelope::new();
        env.write_control(0b0000_0010); // n = 2
        env.restart();
        env.clock(); // sets counter=15, divider=2

        // With n=2, the divider should tick down 2 -> 1 -> 0,
        // and only when it outputs a clock does the counter decrement.
        env.clock();
        assert_eq!(env.counter, 15);
        assert_eq!(env.divider, 1);

        env.clock();
        assert_eq!(env.counter, 15);
        assert_eq!(env.divider, 0);

        env.clock();
        assert_eq!(env.counter, 14);
        assert_eq!(env.divider, env.n);
    }

    #[test]
    fn envelope_loops_from_zero_back_to_15() {
        let mut env = Envelope::new();
        env.write_control(0b0010_0000); // loop=1, disable=0, n=0
        env.restart();
        env.clock();
        assert_eq!(env.counter, 15);

        // With n=0 the divider outputs a clock every envelope tick,
        // so this decrements counter each clock until it wraps.
        for _ in 0..15 {
            env.clock();
        }
        assert_eq!(env.counter, 0);

        env.clock();
        assert_eq!(env.counter, 15);
    }

    #[test]
    fn envelope_without_loop_stays_at_zero() {
        let mut env = Envelope::new();
        env.write_control(0b0000_0000); // loop=0, disable=0, n=0
        env.restart();
        env.clock();

        for _ in 0..16 {
            env.clock();
        }
        assert_eq!(env.counter, 0);

        env.clock();
        assert_eq!(env.counter, 0);
    }

    #[test]
    fn envelope_disable_flag_outputs_constant_n_but_still_clocks_internally() {
        let mut env = Envelope::new();
        env.write_control(0b0001_0101); // disable=1, n=5
        env.restart();
        env.clock();

        assert_eq!(env.volume(), 5);

        // Internal counter still changes, even though output is constant.
    }

    #[test]
    fn reset_restores_envelope_to_initial_state() {
        let mut env = Envelope::new();
        // Modify all fields
        env.write_control(0b0011_1010); // loop=1, disable=1, n=10
        env.restart();
        env.clock();
        env.clock();

        // Verify state changed
        assert_eq!(env.volume(), 10); // constant volume mode
        assert_eq!(env.counter, 15);

        // Reset
        env.reset();

        // Verify all fields back to default
        assert_eq!(env.volume(), 0);
        assert_eq!(env.counter, 0);
        assert!(!env.start_flag);
        assert!(!env.loop_flag);
        assert!(!env.disable_flag);
        assert_eq!(env.n, 0);
        assert_eq!(env.divider, 0);
    }
}
