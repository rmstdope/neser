//! SPC700 timer subsystem.
//!
//! This module models the three SPC700 timers (T0/T1/T2) visible at `$FA-$FF`.

use serde::{Deserialize, Serialize};

const TIMER_INPUT_DIVIDERS: [u16; 3] = [128, 128, 16];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct Timer {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    target: u8,
    #[serde(default)]
    input_divider_counter: u16,
    #[serde(default)]
    target_counter: u16,
    #[serde(default)]
    readable_counter: u8,
}

impl Timer {
    fn reset_target_progress(&mut self) {
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
            if !was_enabled && now_enabled {
                timer.reset_target_progress();
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
            timer.input_divider_counter = timer.input_divider_counter.wrapping_add(1);
            if timer.input_divider_counter < TIMER_INPUT_DIVIDERS[timer_index] {
                continue;
            }

            timer.input_divider_counter = 0;
            if !timer.enabled {
                continue;
            }

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
    fn enable_edge_keeps_prescaler_phase_but_clears_output_progress() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 8);

        timers.write_control(0x04, 0x00);
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 7);
        assert_eq!(timers.read_counter(2), 0x00);

        advance_cycles(&mut timers, 1);
        assert_eq!(timers.read_counter(2), 0x01);
    }

    // -----------------------------------------------------------------------
    // Reference behavior (Snes9x/blargg): the prescaler keeps running while a
    // timer is disabled; an enable edge clears TnOUT and target progress.
    // -----------------------------------------------------------------------

    #[test]
    fn disabling_a_running_timer_stops_target_counter_but_prescaler_keeps_phase() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2 (clears TnOUT)
        advance_cycles(&mut timers, 48); // 3 full periods → TnOUT = 3 (unread)

        timers.write_control(0x04, 0x00);

        assert_eq!(
            timers.read_counter(2),
            0x03,
            "disabling preserves unread TnOUT until the next enable edge"
        );

        advance_cycles(&mut timers, 15);
        assert_eq!(
            timers.read_counter(2),
            0x00,
            "disabled timer must not advance target/output counters"
        );
        timers.write_control(0x00, 0x04);
        advance_cycles(&mut timers, 1);
        assert_eq!(
            timers.read_counter(2),
            0x01,
            "prescaler phase is retained while disabled"
        );
    }

    #[test]
    fn enabling_a_timer_clears_tout_and_target_progress() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 32); // 2 full periods → TnOUT = 2 (unread)

        timers.write_control(0x04, 0x00);
        timers.write_control(0x00, 0x04); // re-enable: TnOUT must be cleared

        // Immediately after enable, TnOUT must be 0
        assert_eq!(timers.read_counter(2), 0x00, "enable must clear TnOUT");
    }

    // -----------------------------------------------------------------------
    // Power-on default TnDIV=0 means divide-by-256.
    // -----------------------------------------------------------------------

    #[test]
    fn fresh_t2_uses_default_target_zero_period_256() {
        let mut timers = SpcTimers::default();
        timers.write_control(0x00, 0x04); // enable T2

        advance_cycles(&mut timers, 16 * 255);
        assert_eq!(
            timers.read_counter(2),
            0x00,
            "TnOUT should still be 0 one prescaler tick before wrap"
        );

        advance_cycles(&mut timers, 16);
        assert_eq!(
            timers.read_counter(2),
            0x01,
            "TnOUT should increment when the 8-bit target counter wraps to 0"
        );
    }

    #[test]
    fn fresh_t0_uses_default_target_zero_period_256() {
        let mut timers = SpcTimers::default();
        timers.write_control(0x00, 0x01); // enable T0

        advance_cycles(&mut timers, 128 * 255);
        assert_eq!(timers.read_counter(0), 0x00);

        advance_cycles(&mut timers, 128);
        assert_eq!(timers.read_counter(0), 0x01);
    }
}
