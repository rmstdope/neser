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

    fn target_compare_value(&self) -> u16 {
        u16::from(self.target)
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
            // Spec ($F1): "0=Disable, [1=]Enable: set TnOUT=0 & reload divider"
            // Both transitions should reload timer progress; enable starts
            // counting, disable stops counting.
            if !was_enabled && now_enabled {
                // Enable: clear TnOUT and reload dividers
                timer.reset_progress();
            } else if was_enabled && !now_enabled {
                // Disable: clear TnOUT and reload dividers
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

    pub fn clear_all_tout(&mut self) {
        for timer in &mut self.timers {
            timer.readable_counter = 0;
        }
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
            timer.target_counter = timer.target_counter.wrapping_add(1) & 0x00FF;
            if timer.target_counter != timer.target_compare_value() {
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
    fn lowering_target_below_current_progress_waits_for_counter_wraparound() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 5);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 16 * 3);

        timers.write_target(2, 2);
        advance_cycles(&mut timers, 16);

        assert_eq!(
            timers.read_counter(2),
            0x00,
            "timer must fire only when the target counter equals TnDIV"
        );
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
    // Spec ($F1 bit0-2): "0=Disable, [1=]Enable: set TnOUT=0 & reload divider"
    //   - ENABLE  (0→1): TnOUT is cleared and dividers are reloaded.
    //   - DISABLE (1→0): TnOUT is cleared and dividers are reloaded.
    // -----------------------------------------------------------------------

    #[test]
    fn disabling_a_running_timer_clears_tout() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2 (clears TnOUT)
        advance_cycles(&mut timers, 48); // 3 full periods → TnOUT = 3 (unread)

        timers.write_control(0x04, 0x00); // disable T2 → TnOUT must be cleared

        assert_eq!(timers.read_counter(2), 0x00, "disabling must clear TnOUT");

        advance_cycles(&mut timers, 64);
        assert_eq!(
            timers.read_counter(2),
            0x00,
            "disabled timer must not advance"
        );
    }

    #[test]
    fn enabling_a_timer_clears_tout_and_reloads_dividers() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 32); // 2 full periods → TnOUT = 2 (unread)

        timers.write_control(0x04, 0x00); // disable: TnOUT cleared
        timers.write_control(0x00, 0x04); // re-enable: TnOUT must be cleared

        // Immediately after enable, TnOUT must be 0
        assert_eq!(timers.read_counter(2), 0x00, "enable must clear TnOUT");

        // And the divider must have been reloaded (no stale partial period)
        advance_cycles(&mut timers, 15);
        assert_eq!(timers.read_counter(2), 0x00, "divider reloaded on enable");
        advance_cycles(&mut timers, 1);
        assert_eq!(
            timers.read_counter(2),
            0x01,
            "one fire after full fresh period"
        );
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
