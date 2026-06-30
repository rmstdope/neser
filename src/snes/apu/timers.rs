//! SPC700 timer subsystem.
//!
//! This module models the three SPC700 timers (T0/T1/T2) visible at `$FA-$FF`.

use serde::{Deserialize, Serialize};

const TIMER_INPUT_DIVIDERS: [u16; 3] = [128, 128, 16];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Timer {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_timer_target")]
    target: u8,
    #[serde(default)]
    input_divider_counter: u16,
    #[serde(default)]
    target_counter: u16,
    #[serde(default)]
    readable_counter: u8,
}

fn default_timer_target() -> u8 {
    0xFF
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            enabled: false,
            target: 0xFF, // hardware power-on default (fullsnes: T0DIV/T1DIV/T2DIV = FFh)
            input_divider_counter: 0,
            target_counter: 0,
            readable_counter: 0,
        }
    }
}

impl Timer {
    fn reset_progress(&mut self) {
        self.input_divider_counter = 0;
        self.target_counter = 0;
        self.readable_counter = 0;
    }

    fn target_period(&self) -> u16 {
        if self.target == 0 {
            256
        } else {
            u16::from(self.target)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SpcTimers {
    #[serde(default)]
    timers: [Timer; 3],
}

impl SpcTimers {
    pub fn write_control(&mut self, old_control: u8, new_control: u8) {
        for timer_index in 0..3 {
            let mask = 1u8 << timer_index;
            let was_enabled = old_control & mask != 0;
            let now_enabled = new_control & mask != 0;
            let timer = &mut self.timers[timer_index];
            timer.enabled = now_enabled;
            // Spec ($F1): "0=Disable, set TnOUT=0 & reload divider"
            // Reset happens on the 1→0 (disable) transition, not 0→1.
            if was_enabled && !now_enabled {
                timer.reset_progress();
            }
        }
    }

    pub fn write_target(&mut self, timer: usize, value: u8) {
        self.timers[timer].target = value;
    }

    pub fn read_counter(&mut self, timer: usize) -> u8 {
        let value = self.timers[timer].readable_counter & 0x0F;
        self.timers[timer].readable_counter = 0;
        value
    }

    pub fn tick_cycle(&mut self) {
        for (timer_index, timer) in self.timers.iter_mut().enumerate() {
            if !timer.enabled {
                continue;
            }

            timer.input_divider_counter = timer.input_divider_counter.wrapping_add(1);
            if timer.input_divider_counter < TIMER_INPUT_DIVIDERS[timer_index] {
                continue;
            }

            timer.input_divider_counter = 0;
            timer.target_counter = timer.target_counter.wrapping_add(1);
            if timer.target_counter < timer.target_period() {
                continue;
            }

            timer.target_counter = 0;
            timer.readable_counter = (timer.readable_counter.wrapping_add(1)) & 0x0F;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SpcTimers;

    fn advance_cycles(timers: &mut SpcTimers, cycles: usize) {
        for _ in 0..cycles {
            timers.tick_cycle();
        }
    }

    #[test]
    fn t0_counts_once_every_128_spc_cycles_when_target_is_one() {
        let mut timers = SpcTimers::default();
        timers.write_target(0, 1);
        timers.write_control(0x00, 0x01);

        advance_cycles(&mut timers, 127);
        assert_eq!(timers.read_counter(0), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(0), 0x01);
    }

    #[test]
    fn t2_counts_once_every_16_spc_cycles_when_target_is_one() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04);

        advance_cycles(&mut timers, 15);
        assert_eq!(timers.read_counter(2), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(2), 0x01);
    }

    #[test]
    fn t1_counts_once_every_128_spc_cycles_when_target_is_one() {
        let mut timers = SpcTimers::default();
        timers.write_target(1, 1);
        timers.write_control(0x00, 0x02);

        advance_cycles(&mut timers, 127);
        assert_eq!(timers.read_counter(1), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(1), 0x01);
    }

    #[test]
    fn target_zero_means_256_input_clocks() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 0x00);
        timers.write_control(0x00, 0x04);

        advance_cycles(&mut timers, 16 * 255);
        assert_eq!(timers.read_counter(2), 0x00);

        advance_cycles(&mut timers, 16);
        assert_eq!(timers.read_counter(2), 0x01);
    }

    #[test]
    fn read_counter_clears_the_latched_nibble() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 16);

        assert_eq!(timers.read_counter(2), 0x01);
        assert_eq!(timers.read_counter(2), 0x00);
    }

    #[test]
    fn enable_edge_resets_internal_progress() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 8);

        timers.write_control(0x04, 0x00);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 15);
        assert_eq!(timers.read_counter(2), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(2), 0x01);
    }

    // -----------------------------------------------------------------------
    // Spec: $F1 bit0-2 = "0=Disable, set TnOUT=0 & reload divider, 1=Enable"
    // The reset must happen on the 1→0 (disable) transition, not 0→1.
    // -----------------------------------------------------------------------

    #[test]
    fn disabling_a_running_timer_clears_tout_immediately() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 48); // 3 full periods → TnOUT = 3 (unread)

        timers.write_control(0x04, 0x00); // disable T2 → spec: TnOUT = 0

        // TnOUT must be 0 immediately after disable — NOT after re-enable.
        assert_eq!(
            timers.read_counter(2),
            0x00,
            "disabling must clear TnOUT immediately per spec"
        );
    }

    #[test]
    fn reading_tout_while_disabled_always_returns_zero_even_if_it_accumulated() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 32); // 2 full periods

        timers.write_control(0x04, 0x00); // disable T2

        // Two consecutive reads must both be zero.
        assert_eq!(timers.read_counter(2), 0x00);
        assert_eq!(timers.read_counter(2), 0x00);
    }

    // -----------------------------------------------------------------------
    // Spec: T0DIV/T1DIV/T2DIV power-on default is 0xFF.
    // A fresh (never-written) timer should fire after 255 prescaler ticks,
    // not 256 (which is what a zero-initialised target would give).
    // -----------------------------------------------------------------------

    #[test]
    fn fresh_t2_uses_hardware_default_target_0xff_period_255() {
        let mut timers = SpcTimers::default();
        // No write_target → hardware default 0xFF → period = 255 × 16 = 4080 cycles
        timers.write_control(0x00, 0x04); // enable T2

        advance_cycles(&mut timers, 4079); // one cycle before the first fire
        assert_eq!(
            timers.read_counter(2),
            0x00,
            "TnOUT should still be 0 at 4079 cycles with default target 0xFF"
        );

        advance_cycles(&mut timers, 1); // cycle 4080 → first fire
        assert_eq!(
            timers.read_counter(2),
            0x01,
            "TnOUT should be 1 at 4080 cycles with default target 0xFF"
        );
    }

    #[test]
    fn fresh_t0_uses_hardware_default_target_0xff_period_255() {
        let mut timers = SpcTimers::default();
        // No write_target → hardware default 0xFF → period = 255 × 128 = 32640 cycles
        timers.write_control(0x00, 0x01); // enable T0

        advance_cycles(&mut timers, 32639);
        assert_eq!(timers.read_counter(0), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(0), 0x01);
    }
}
