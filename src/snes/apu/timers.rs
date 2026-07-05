//! SPC700 timer subsystem.
//!
//! This module models the three SPC700 timers (T0/T1/T2) visible at `$FA-$FF`
//! using the hardware's two-stage prescaler (verified against Mesen2's
//! `SpcTimer` and blargg's `test_timer_stop2` ROM):
//!
//! * Stage 0 divides the SPC clock into a square wave (stage 1) that toggles
//!   every half-period — 64 SPC cycles for T0/T1, 8 for T2.
//! * The 8-bit target counter (stage 2) clocks on stage-1 **falling edges**,
//!   giving the documented full periods of 128 (T0/T1) and 16 (T2) cycles.
//! * The TEST register's global stop (`$F0` bits 0/3) forces the edge
//!   detector's input low without clearing TnOUT. If stage 1 is high at the
//!   moment of the stop, that forced low is itself a falling edge and injects
//!   one stage-2 clock — the quirk blargg's `test_timer_stop2` measures by
//!   rapidly pumping the TEST stop/start bits.
//! * Stage 0/1 keep running while a timer is disabled (per-timer via `$F1`
//!   or globally via TEST), so re-enabling resumes mid-phase.

use serde::{Deserialize, Serialize};

const TIMER_HALF_PERIODS: [u16; 3] = [64, 64, 8];

fn default_global_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct Timer {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    target: u8,
    #[serde(default)]
    stage0: u16,
    #[serde(default)]
    stage1: bool,
    #[serde(default)]
    prev_stage1: bool,
    #[serde(default)]
    stage2: u8,
    #[serde(default)]
    output: u8,
}

impl Timer {
    fn reset_target_progress(&mut self) {
        self.stage2 = 0;
        self.output = 0;
    }

    /// Feed the current stage-1 level (masked by the TEST-register global
    /// enable) into the edge detector; clock stage 2 on a falling edge.
    fn clock_edge_detector(&mut self, global_enabled: bool) {
        let current = self.stage1 && global_enabled;
        let previous = self.prev_stage1;
        self.prev_stage1 = current;
        if !self.enabled || !previous || current {
            return;
        }
        self.stage2 = self.stage2.wrapping_add(1);
        if self.stage2 == self.target {
            self.stage2 = 0;
            self.output = self.output.wrapping_add(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpcTimers {
    #[serde(default)]
    timers: [Timer; 3],
    #[serde(default = "default_global_enabled")]
    global_enabled: bool,
}

impl Default for SpcTimers {
    fn default() -> Self {
        Self {
            timers: Default::default(),
            global_enabled: default_global_enabled(),
        }
    }
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
        let value = self.timers[timer].output & 0x0F;
        self.timers[timer].output = 0;
        value
    }

    /// Apply the TEST register's global timer enable/disable. Runs the edge
    /// detector immediately: a stop while stage 1 is high injects one
    /// stage-2 clock (falling edge), matching real hardware.
    pub fn set_global_enabled(&mut self, enabled: bool) {
        self.global_enabled = enabled;
        for timer in &mut self.timers {
            timer.clock_edge_detector(enabled);
        }
    }

    pub fn tick_cycle(&mut self) {
        let global_enabled = self.global_enabled;
        for (timer_index, timer) in self.timers.iter_mut().enumerate() {
            timer.stage0 += 1;
            if timer.stage0 < TIMER_HALF_PERIODS[timer_index] {
                continue;
            }
            timer.stage0 = 0;
            timer.stage1 = !timer.stage1;
            timer.clock_edge_detector(global_enabled);
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

    // -----------------------------------------------------------------------
    // TEST-register global stop/start semantics (blargg test_timer_stop2):
    // a stop forces the stage-1 edge-detector input low without clearing
    // TnOUT; pumping stop/start while stage 1 is high injects one stage-2
    // clock per stop.
    // -----------------------------------------------------------------------

    #[test]
    fn global_stop_start_pump_injects_one_target_clock_per_stop() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 2);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 8); // stage 1 toggles high

        // Four stop/start pairs with stage 1 held high: each stop is a
        // falling edge at the detector, each start re-arms it.
        for _ in 0..4 {
            timers.set_global_enabled(false);
            timers.set_global_enabled(true);
        }

        assert_eq!(
            timers.read_counter(2),
            0x02,
            "4 injected clocks at target=2 must produce TnOUT=2"
        );
    }

    #[test]
    fn global_stop_with_stage1_low_injects_nothing_and_preserves_tout() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 48); // 3 falling edges; stage 1 ends low

        timers.set_global_enabled(false);

        assert_eq!(
            timers.read_counter(2),
            0x03,
            "a TEST stop must not clear TnOUT"
        );
    }

    #[test]
    fn globally_stopped_timer_does_not_count_but_stage1_keeps_toggling() {
        let mut timers = SpcTimers::default();
        timers.write_target(2, 1);
        timers.write_control(0x00, 0x04); // enable T2
        advance_cycles(&mut timers, 4);

        timers.set_global_enabled(false);
        advance_cycles(&mut timers, 160); // many toggles, no counting
        assert_eq!(timers.read_counter(2), 0x00);

        // Stage 1 toggled 20 times while stopped (cycles 8,16,...,160) and is
        // low again, with stage 0 at 4 of 8. After the restart it toggles
        // high 4 cycles in and falls 8 cycles later → tick at +12 exactly.
        timers.set_global_enabled(true);
        advance_cycles(&mut timers, 12);
        assert_eq!(
            timers.read_counter(2),
            0x01,
            "prescaler phase must be preserved across a TEST stop"
        );
    }
}
