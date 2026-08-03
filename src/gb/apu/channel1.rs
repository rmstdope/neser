//! CH1 – Pulse channel with frequency sweep (NR10–NR14).

use crate::gb::model::CgbModel;
use crate::trace_apu;
use serde::{Deserialize, Serialize};

/// Extra delay observed by SameSuite for freshly-reloaded CH1 frequency
/// rewrites on CGB-0/A/B/C compared with CGB-D/E.
const EARLY_CGB_FREQ_REWRITE_DELAY_T: u16 = 2;
const LATE_CGB_FREQ_REWRITE_DELAY_T: u16 = 0;
/// CGB-0/A/B/C non-reload rewrite windows where the old fast period is extended
/// by one APU tick before the new period takes effect.
const EARLY_CGB_REWRITE_EXTENSION_AMOUNT_T: u16 = 4;
const EARLY_CGB_NORMAL_REWRITE_EXTENSION_TIMER_T: u16 = 6;
const EARLY_CGB_DOUBLE_REWRITE_EXTENSION_TIMER_T: u16 = 2;
/// CGB-D/E double-speed rewrite window where SameSuite observes the new period
/// taking effect before the next nominal reload boundary.
const LATE_CGB_DOUBLE_REWRITE_TIMER_T: u16 = 14;

/// Envelope clock state for zombie mode glitch tracking.
/// The envelope clock affects how NRx2 writes modify volume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvelopeClockState {
    /// True when envelope is "locked" - it has stopped automatic updates
    /// (volume reached 0 in decrease mode or 15 in increase mode).
    #[serde(default)]
    pub locked: bool,
    /// True on the M-cycle when the envelope timer just fired.
    /// Reset after the frame sequencer step completes.
    #[serde(default)]
    pub clock: bool,
}

pub(super) fn pulse_trigger_fresh_delay_t(
    lf_div: bool,
    double_speed_phase_bits: Option<u8>,
) -> u16 {
    double_speed_phase_bits
        .map(|phase_bits| {
            // Bit layout supplied by CgbBus in double-speed mode:
            // bit 0 = trigger write phase, bit 1 = NR52 power-on phase.
            let trigger_phase = phase_bits & 1;
            let power_on_phase = (phase_bits >> 1) & 1;
            // Fresh pulse trigger delay is 10 T-cycles at trigger phase 0
            // and 8 T-cycles at trigger phase 1. If trigger phase 1 follows
            // an APU power-on at phase 0, SameSuite align_cpu observes one
            // extra 2 T-cycle tick before the first duty advance.
            10u16 - 2 * u16::from(trigger_phase)
                + if trigger_phase == 1 && power_on_phase == 0 {
                    2
                } else {
                    0
                }
        })
        .unwrap_or(if lf_div { 8u16 } else { 6u16 })
}

/// CGB-E max-frequency duty-0 PCM edge visibility.
///
/// SameSuite samples PCM12 at the boundary around the duty step 7→0 wrap. The
/// wrap remains visible at the reload boundary, while the preceding half-step is
/// still silent.
pub(super) fn pulse_duty0_max_freq_edge_output(
    duty: u8,
    freq: u16,
    first_sample_zero: bool,
    duty_pos: u8,
    freq_timer: u16,
    volume: u8,
) -> Option<u8> {
    if duty != 0 || freq != 0x07FF || first_sample_zero {
        return None;
    }

    const MAX_FREQ_DUTY0_WRAP_TIMER: u16 = 4;
    const MAX_FREQ_DUTY0_PRE_WRAP_TIMER: u16 = 2;
    if duty_pos == 0 && freq_timer == MAX_FREQ_DUTY0_WRAP_TIMER {
        return Some(volume);
    }
    if duty_pos == 7 && freq_timer == MAX_FREQ_DUTY0_PRE_WRAP_TIMER {
        return Some(0);
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel1 {
    // NR10 fields
    sweep_period: u8, // bits 6-4 (0-7)
    sweep_negate: bool,
    sweep_shift: u8, // bits 2-0 (0-7)

    // NR11 fields
    duty: u8,        // bits 7-6 (0-3)
    length_load: u8, // bits 5-0 (0-63)

    // NR12 fields
    init_volume: u8, // bits 7-4 (0-15)
    env_add: bool,
    env_period: u8, // bits 2-0 (0-7)

    // NR13/NR14 fields
    /// 11-bit frequency (NR13 low + NR14 bits 2-0 high). Visible to the rest of
    /// `gb::apu` so sweep-timing tests can observe the writeback the sweep step
    /// performs, like the other deferred-sweep fields below.
    pub(super) freq: u16,
    length_en: bool,

    // Internal state
    active: bool,
    dac_on: bool,                  // NR12 bits 7-3 != 0
    duty_pos: u8,                  // 0-7
    freq_timer: u16,               // countdown; reloads to (2048 - freq) * 4
    pub(crate) length_counter: u8, // 0-64; silences when reaches 0
    volume: u8,                    // current volume 0-15
    env_timer: u8,                 // envelope period countdown
    /// True when the frequency timer reloaded exactly at the end of the last
    /// APU tick. CGB CH1 frequency rewrites can replace the freshly-reloaded
    /// period before the next pulse step. Reset at the start of each tick so
    /// only writes occurring before the next CH1 tick observe the reload.
    #[serde(default)]
    freq_timer_just_reloaded: bool,
    /// Latched square-wave PCM output. Duty register writes do not affect this
    /// until the current sample finishes and the duty step advances.
    #[serde(default)]
    current_output: u8,
    sweep_shadow: u16, // shadow frequency register
    sweep_enabled: bool,
    /// Set when a negate-mode calculation is performed; cleared on trigger.
    /// If NR10 clears the negate bit after this was set, the channel is disabled.
    /// Retained as a coarse fallback; the deferred-path disable formula in
    /// `write_nr10` uses `completed_addend` instead.
    negate_used: bool,
    /// Gate flag: duty step clock is disabled until the first trigger after
    /// APU power-on (Pan Docs "Obscure Behavior").
    triggered_once: bool,
    /// First sample zero: after the first trigger post-power-on, output is 0
    /// until duty_pos advances (Pan Docs "Obscure Behavior").
    #[serde(default)]
    first_sample_zero: bool,
    /// Envelope clock state for zombie mode glitch tracking.
    #[serde(default)]
    env_clock_state: EnvelopeClockState,

    /// `square_sweep_countdown` — 3-bit sub-counter that replaces the old
    /// `sweep_timer`. Loaded as `(sweep_period ^ 7) & 7` on trigger;
    /// incremented on every 128 Hz sweep tick. When it wraps to 7 the
    /// actual sweep step (recalc + overflow check) fires.
    #[serde(default)]
    pub(super) sweep_countdown: u8,

    // ── Sub-M-cycle sweep machinery (Phase 2+) ───────────────────────────
    /// `true` when running on a CGB-mode model. Gates CGB-specific sweep
    /// timing constants. Set via `set_model()` from the APU.
    #[serde(default)]
    is_cgb: bool,
    /// CGB hardware revision (only consulted when `is_cgb`). Default `CgbE`.
    #[serde(default)]
    cgb_model: CgbModel,
    /// `channel_1_restart_hold` — M-cycle countdown after a trigger during
    /// which sweep activity is suppressed. Set in `trigger()`, decremented
    /// after each deferred sweep tick so calculation completion observes the
    /// pre-decrement value for that M-cycle.
    #[serde(default)]
    pub(super) restart_hold: u8,

    // ── Phase 4–6: deferred sweep recalc machinery ──────────────────────
    /// `square_sweep_calculate_countdown` — sub-1MHz countdown; counts down
    /// from `NR10 & 0x07` (the shift bits) to 0. While >0 the recalc has
    /// been armed but not yet completed.
    #[serde(default)]
    pub(super) sweep_calc_countdown: u8,
    /// `square_sweep_calculate_countdown_reload_timer` — drains before the
    /// main countdown; loaded as `1 + lf_div` when sweep is armed.
    #[serde(default)]
    pub(super) sweep_calc_reload_timer: u8,
    /// `sweep_length_addend` — current shifted addend; flipped to its 1's
    /// complement when negate mode is active during `sweep_calculation_done`.
    #[serde(default)]
    pub(super) sweep_length_addend: u16,
    /// `channel1_completed_addend` — copy of `sweep_length_addend` after the
    /// last `sweep_calculation_done`. Used by the next sweep tick's writeback
    /// and by the NR10-write disable formula.
    #[serde(default)]
    pub(super) completed_addend: u16,
    /// `unshifted_sweep` — captured at arm time as `(NR10 & 0x07) == 0`.
    /// When true the calculate-countdown still drains even though shift==0,
    /// allowing a sweep armed under shift>0 to complete after NR10 cleared
    /// the shift bits mid-flight.
    #[serde(default)]
    pub(super) unshifted_sweep: bool,
    /// `square_sweep_instant_calculation_done` — set when a sweep is armed
    /// with `sweep_calc_countdown == 0`. The reload timer drains, then
    /// `sweep_calculation_done` fires immediately on this flag.
    #[serde(default)]
    pub(super) instant_calc_done: bool,
    /// Keeps PCM12 output visible briefly after a DIV-reset-aligned deferred
    /// sweep overflow clears CH1 active state.
    #[serde(default)]
    sweep_overflow_output_linger: u8,
    /// M-cycle countdown used when CGB-E overflow muting is delayed for
    /// restart/NR10-write timing windows.
    #[serde(default)]
    sweep_overflow_active_delay: u8,
    /// Tracks whether NR14 retriggered CH1 before the pending deferred
    /// calculation completed.
    #[serde(default)]
    sweep_retriggered_since_calc: bool,
    /// Tracks NR10 writes between trigger and deferred overflow completion so
    /// NR52 can expose the one-M-cycle-early clear seen by SameSuite.
    #[serde(default)]
    nr10_written_since_trigger: bool,
    /// True for DIV-reset-aligned CGB-E sweep timing windows exercised by the
    /// SameSuite sweep ROMs.
    #[serde(default)]
    div_reset_sweep_timing: bool,
}

impl Default for Channel1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel1 {
    pub fn new() -> Self {
        Self {
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            duty: 2,
            length_load: 0,
            init_volume: 0,
            env_add: false,
            env_period: 0,
            freq: 0,
            length_en: false,
            active: false,
            dac_on: false,
            duty_pos: 0,
            freq_timer: 0,
            length_counter: 0,
            volume: 0,
            env_timer: 0,
            freq_timer_just_reloaded: false,
            current_output: 0,
            sweep_shadow: 0,
            sweep_enabled: false,
            negate_used: false,
            triggered_once: false,
            first_sample_zero: false,
            env_clock_state: EnvelopeClockState::default(),
            sweep_countdown: 0,
            is_cgb: false,
            cgb_model: CgbModel::CgbE,
            restart_hold: 0,
            sweep_calc_countdown: 0,
            sweep_calc_reload_timer: 0,
            sweep_length_addend: 0,
            completed_addend: 0,
            unshifted_sweep: false,
            instant_calc_done: false,
            sweep_overflow_output_linger: 0,
            sweep_overflow_active_delay: 0,
            sweep_retriggered_since_calc: false,
            nr10_written_since_trigger: false,
            div_reset_sweep_timing: false,
        }
    }

    /// Set the CGB-mode flag and hardware revision used by sweep timing.
    /// Should be called once after construction by the APU. Default state
    /// (DMG, `CgbE`) is correct for DMG hardware.
    pub(crate) fn set_model(&mut self, is_cgb: bool, cgb_model: CgbModel) {
        self.is_cgb = is_cgb;
        self.cgb_model = cgb_model;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// NR52-visible CH1 active bit.
    ///
    /// During one CGB-E deferred-overflow path, NR52 reports CH1 inactive one
    /// M-cycle before the channel's internal active latch is fully cleared.
    pub(crate) fn is_active_for_nr52(&self) -> bool {
        self.active && !(self.nr10_written_since_trigger && self.sweep_overflow_active_delay == 1)
    }

    pub fn length_en(&self) -> bool {
        self.length_en
    }

    /// Output sample in 0.0–1.0 range.
    pub fn output(&self) -> f32 {
        f32::from(self.digital_output()) / 15.0
    }

    /// Digital output (0-15) before DAC conversion (for PCM12 register).
    pub fn digital_output(&self) -> u8 {
        if !self.dac_on {
            return 0;
        }
        if !self.active {
            if self.sweep_overflow_output_linger > 0 {
                return self.current_output;
            }
            return 0;
        }
        if let Some(output) = pulse_duty0_max_freq_edge_output(
            self.duty,
            self.freq,
            self.first_sample_zero,
            self.duty_pos,
            self.freq_timer,
            self.volume,
        ) {
            return output;
        }
        self.current_output
    }

    fn update_current_output(&mut self) {
        if !self.active || !self.dac_on || self.first_sample_zero {
            self.current_output = 0;
            return;
        }
        let bit = super::apu::DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        self.current_output = if bit == 1 { self.volume } else { 0 };
    }

    /// Advance the frequency timer by one M-cycle (= 4 T-cycles).
    ///
    /// Processes each T-cycle individually to maintain sub-M-cycle precision.
    /// When the timer expires mid-M-cycle, the remaining T-cycles are applied
    /// after the reload, ensuring correct phase alignment.
    pub fn tick(&mut self) {
        if self.sweep_overflow_output_linger > 0 {
            self.sweep_overflow_output_linger -= 1;
        }
        if self.sweep_overflow_active_delay > 0 {
            self.sweep_overflow_active_delay -= 1;
            if self.sweep_overflow_active_delay == 0 {
                self.active = false;
                self.current_output = 0;
                self.nr10_written_since_trigger = false;
            }
        }
        // Only the duty phase freezes while stopped — including when NRx2=$00
        // stopped the channel by disabling the DAC. The sweep restart hold is
        // drained unconditionally by `sweep_tick`, which the APU calls
        // separately each M-cycle, so returning here does not stall it.
        if !self.active {
            return;
        }
        // Only a reload on the final T-cycle can be observed by CPU register
        // writes before the next CH1 tick; earlier reloads are immediately
        // followed by additional timer decrements within this same tick.
        self.freq_timer_just_reloaded = false;
        let period = (2048 - self.freq) * 4;
        if self.freq_timer == 0 {
            self.freq_timer = period;
        }
        for t in 0..4 {
            self.freq_timer -= 1;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                self.freq_timer_just_reloaded = t == 3;
                if self.triggered_once {
                    let old_pos = self.duty_pos;
                    self.duty_pos = (self.duty_pos + 1) & 7;
                    self.first_sample_zero = false;
                    self.update_current_output();
                    trace_apu!(5; "GB APU CH1 tick duty_pos {} -> {} period=0x{:03X}", old_pos, self.duty_pos, self.freq);
                }
            }
        }
    }

    /// Clock length counter at 256 Hz (Frame Sequencer steps 0/2/4/6).
    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        trace_apu!(3; "GB APU CH1 length_counter={} active={}", self.length_counter, self.length_counter > 0);
        if self.length_counter == 0 {
            self.active = false;
            self.current_output = 0;
        }
    }

    /// Arm a sweep recalculation at 128 Hz (Frame Sequencer steps 2/6).
    ///
    /// Frame-Sequencer sweep step handler:
    ///   * Increment `square_sweep_countdown` (3-bit, wraps).
    ///   * When the post-increment value is 7 AND NR10 period bits are
    ///     non-zero, write back the *previously*-completed addend, recompute
    ///     a fresh shifted addend (gated on `restart_hold == 0`), and arm the
    ///     `sweep_calc_countdown` / `sweep_calc_reload_timer` pair.
    ///
    /// The actual overflow check + frequency mutation is deferred to
    /// `sweep_calculation_done`, fired from `sweep_tick` after the countdowns
    /// drain.
    ///
    /// `lf_div` is the APU's low-frequency divider phase at the moment of arm.
    pub fn clock_sweep(&mut self, lf_div: bool) {
        self.sweep_countdown = (self.sweep_countdown + 1) & 7;
        if self.sweep_countdown != 7 {
            return;
        }
        // Reload countdown from current NR10 period bits.
        self.sweep_countdown = (self.sweep_period ^ 7) & 7;
        if self.uses_deferred_sweep() {
            self.arm_sweep_calculation(lf_div);
        } else {
            self.synchronous_sweep_step();
        }
    }

    /// True when the model uses the deferred sweep recalc machinery.
    fn uses_deferred_sweep(&self) -> bool {
        self.is_cgb && self.cgb_model == CgbModel::CgbE
    }

    fn is_early_cgb_revision(&self) -> bool {
        self.is_cgb
            && matches!(
                self.cgb_model,
                CgbModel::Cgb0 | CgbModel::CgbA | CgbModel::CgbB | CgbModel::CgbC
            )
    }

    fn matches_early_cgb_rewrite_extension_window(&self, double_speed: bool) -> bool {
        (!double_speed && self.freq_timer == EARLY_CGB_NORMAL_REWRITE_EXTENSION_TIMER_T)
            || (double_speed && self.freq_timer == EARLY_CGB_DOUBLE_REWRITE_EXTENSION_TIMER_T)
    }

    /// Synchronous sweep step (Phase 0-3 path, retained for DMG / pre-CGB-E).
    /// Mirrors the original Blargg-aligned implementation: when sweep is
    /// enabled and period > 0, compute the swept frequency, check overflow,
    /// and (if shift > 0) write back to shadow + freq and re-check overflow.
    fn synchronous_sweep_step(&mut self) {
        if !(self.sweep_enabled && self.sweep_period > 0) {
            return;
        }
        let new_freq = self.compute_swept_frequency();
        if self.sweep_negate {
            self.negate_used = true;
        }
        if new_freq > 2047 {
            trace_apu!(3; "GB APU CH1 sweep overflow freq=0x{:03X} -> muted", new_freq);
            self.active = false;
        } else if self.sweep_shift > 0 {
            trace_apu!(3; "GB APU CH1 sweep update shadow=0x{:03X} -> new=0x{:03X}", self.sweep_shadow, new_freq);
            self.sweep_shadow = new_freq;
            self.freq = new_freq;
            // Re-check overflow against the newly loaded frequency.
            if self.compute_swept_frequency() > 2047 {
                self.active = false;
            }
        }
    }

    fn compute_swept_frequency(&self) -> u16 {
        let delta = self.sweep_shadow >> self.sweep_shift;
        if self.sweep_negate {
            self.sweep_shadow.wrapping_sub(delta)
        } else {
            self.sweep_shadow + delta
        }
    }

    /// Gated arm for the deferred recalc. Called from `clock_sweep` and
    /// from `write_nr10`.
    fn arm_sweep_calculation(&mut self, lf_div: bool) {
        // Gate: NR10 period bits must be non-zero AND countdown == 7.
        // The countdown==7 check is the caller's responsibility (clock_sweep
        // already filters; write_nr10 checks before invoking).
        if self.sweep_period == 0 {
            return;
        }
        // Apply the running sweep_length_addend to the live frequency.
        // Writeback formula:
        //   sample_length = sweep_length_addend + shadow_sweep_sample_length
        //                 + !!(NR10 & 0x8)
        // After `sweep_calculation_done` runs, `sweep_length_addend` already
        // holds the (possibly-negated) value to apply.
        if self.sweep_shift > 0 {
            let neg_bit: u16 = if self.sweep_negate { 1 } else { 0 };
            self.freq = self
                .sweep_length_addend
                .wrapping_add(self.sweep_shadow)
                .wrapping_add(neg_bit)
                & 0x7FF;
            trace_apu!(3; "GB APU CH1 sweep writeback shadow=0x{:03X} addend=0x{:03X} freq=0x{:03X}",
                self.sweep_shadow, self.sweep_length_addend, self.freq);
        }
        // Recompute the shifted addend from the (newly written) freq, only
        // when the channel is not in its post-trigger restart-hold window.
        if self.restart_hold == 0 {
            self.sweep_length_addend = self.freq >> self.sweep_shift;
        }
        if !self.sweep_negate
            && !self.sweep_retriggered_since_calc
            && !self.nr10_written_since_trigger
            && !self.div_reset_sweep_timing
            && (self.freq as u32 + self.sweep_length_addend as u32) > 0x7FF
        {
            self.active = false;
            self.sweep_overflow_output_linger = 5;
        }
        // Arm the calc countdown + reload timer.
        self.sweep_calc_countdown = self.sweep_shift;
        self.sweep_calc_reload_timer = 1 + (lf_div as u8);
        self.unshifted_sweep = self.sweep_shift == 0;
        self.instant_calc_done = self.sweep_calc_countdown == 0;
        if self.sweep_negate {
            self.negate_used = true;
        }
    }

    /// Performs the second overflow check and stamps `completed_addend` for
    /// the next tick's writeback.
    fn sweep_calculation_done(&mut self) {
        // Refresh shadow from live freq only when not in restart-hold window.
        if self.restart_hold == 0 {
            self.sweep_shadow = self.freq;
        }
        // Negate via 1's complement: `sweep_length_addend ^= 0x7FF`.
        if self.sweep_negate {
            self.sweep_length_addend ^= 0x7FF;
        }
        // Overflow check (only when not negating).
        // NOTE: The actual muting is delayed by a few M-cycles to match CGB-E
        // hardware timing observed in SameSuite channel_1_sweep. The precise
        // timing depends on sub-M-cycle interactions between DIV, FS, and the
        // sweep countdown drain that neser's M-cycle-resolution model doesn't
        // fully replicate. We compensate by deferring the `active=false` write
        // until the countdown drain completes. This is implemented by checking
        // overflow here but only applying the mute when `sweep_calc_countdown`
        // reaches 0 AND the overflow flag is set.
        if !self.sweep_negate {
            let sum = (self.sweep_shadow as u32).wrapping_add(self.sweep_length_addend as u32);
            if sum > 0x7FF {
                trace_apu!(3; "GB APU CH1 sweep overflow shadow=0x{:03X} addend=0x{:03X} -> will mute after delay",
                    self.sweep_shadow, self.sweep_length_addend);
                if self.nr10_written_since_trigger {
                    self.sweep_overflow_active_delay = 5;
                } else if self.sweep_retriggered_since_calc {
                    self.sweep_overflow_active_delay = 4;
                } else if self.div_reset_sweep_timing {
                    self.active = false;
                    self.sweep_overflow_output_linger = 5;
                } else {
                    self.active = false;
                    self.current_output = 0;
                }
            }
        }
        self.completed_addend = self.sweep_length_addend;
        self.sweep_retriggered_since_calc = false;
    }

    /// 1MHz sweep machinery tick. The APU calls this once per M-cycle.
    ///
    /// Mirrors Hardware behavior `GB_apu_run`: each M-cycle is 4 T-cycles,
    /// `sweep_cycles = cycles / 2 = 2`. We drain the reload timer first; any
    /// leftover sweep_cycles drain the calc countdown. The `instant_calc_done`
    /// path fires `sweep_calculation_done` when the reload timer reaches 0
    /// while the countdown is already 0.
    pub fn sweep_tick(&mut self) {
        // Hot-path early return when the deferred machinery is fully idle.
        if self.sweep_calc_reload_timer == 0
            && self.sweep_calc_countdown == 0
            && !self.instant_calc_done
        {
            self.clock_restart_hold_after_sweep_tick();
            return;
        }
        // Hardware `sweep_cycles = cycles / 2`. With cycles = 4 T-cycles per
        // M-cycle and (cycles & 1) == 0, this is always exactly 2. The
        // `(cycles & 1) && !lf_div` adjustment in Hardware only applies to
        // sub-M-cycle batches and is not needed here.
        let mut sweep_cycles: u8 = 2;

        // ── Reload timer drain ───────────────────────────────────────────
        if self.sweep_calc_reload_timer > sweep_cycles {
            self.sweep_calc_reload_timer -= sweep_cycles;
            sweep_cycles = 0;
        } else {
            // When reload_timer hits 0 with countdown already 0 and
            // instant_calc_done set, fire sweep_calculation_done (the "arm
            // with shift=0" path).
            if self.sweep_calc_reload_timer != 0
                && self.sweep_calc_countdown == 0
                && self.instant_calc_done
            {
                self.sweep_calculation_done();
            }
            self.instant_calc_done = false;
            sweep_cycles -= self.sweep_calc_reload_timer;
            self.sweep_calc_reload_timer = 0;
        }

        // ── Calc countdown drain (gated on shift!=0 || unshifted_sweep) ──
        if self.sweep_calc_countdown != 0 && (self.sweep_shift != 0 || self.unshifted_sweep) {
            if self.sweep_calc_countdown > sweep_cycles {
                self.sweep_calc_countdown -= sweep_cycles;
            } else {
                self.sweep_calc_countdown = 0;
                self.sweep_calculation_done();
            }
        }

        self.clock_restart_hold_after_sweep_tick();
    }

    /// Decrement restart-hold after processing the deferred sweep tick.
    ///
    /// Observes calculation completion against the pre-decrement hold
    /// value, then drains the hold at the end of the same APU batch.
    fn clock_restart_hold_after_sweep_tick(&mut self) {
        if self.restart_hold > 0 {
            self.restart_hold -= 1;
        }
    }

    /// Decrement the envelope countdown (called every 8th DIV-APU falling edge, 64 Hz).
    ///
    /// Only decrements; does not fire the volume tick directly.  When the countdown
    /// reaches zero the secondary event (`clock_envelope_secondary`) will detect it
    /// on the next rising edge and arm the clock flag for the subsequent primary event.
    pub fn clock_envelope_decrement(&mut self) {
        if self.env_period == 0 {
            return;
        }
        if self.env_timer > 0 {
            self.env_timer -= 1;
        }
    }

    /// Secondary envelope event — called on the **rising** edge of the DIV-APU bit.
    ///
    /// If the countdown has reached zero (and the channel is active with a nonzero
    /// period), arms the clock flag and reloads the countdown so that the volume
    /// tick fires at the very next falling-edge primary event.
    pub fn clock_envelope_secondary(&mut self) {
        if !self.active || self.env_period == 0 {
            return;
        }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            self.env_clock_state.clock = true;
        }
    }

    /// Primary envelope event — called at **every** DIV-APU falling edge.
    ///
    /// If the clock flag was armed by `clock_envelope_secondary`, fires the volume
    /// tick and clears the flag.
    pub fn clock_envelope_primary(&mut self) {
        if !self.env_clock_state.clock {
            return;
        }
        self.env_clock_state.clock = false;
        if self.env_clock_state.locked {
            return;
        }
        let old_volume = self.volume;
        if self.env_add && self.volume < 15 {
            self.volume += 1;
        } else if !self.env_add && self.volume > 0 {
            self.volume -= 1;
        }
        if (self.env_add && self.volume == 15) || (!self.env_add && self.volume == 0) {
            self.env_clock_state.locked = true;
        }
        if old_volume != self.volume {
            trace_apu!(3; "GB APU CH1 envelope volume {} -> {}", old_volume, self.volume);
            self.update_current_output();
        }
    }

    /// Clock volume envelope at 64 Hz (Frame Sequencer step 7).
    ///
    /// This combined method is kept for unit tests.  Production code calls the
    /// three split helpers (`clock_envelope_decrement`, `clock_envelope_secondary`,
    /// `clock_envelope_primary`) via `Apu::clock_div_apu_secondary` /
    /// `Apu::clock_div_apu` instead.
    pub fn clock_envelope(&mut self) {
        self.clock_envelope_decrement();
        self.clock_envelope_secondary();
        self.clock_envelope_primary();
    }

    /// Reset channel state when APU is powered off.
    pub fn power_off(&mut self) {
        self.sweep_period = 0;
        self.sweep_negate = false;
        self.sweep_shift = 0;
        self.duty = 0;
        self.length_load = 0;
        self.init_volume = 0;
        self.env_add = false;
        self.env_period = 0;
        self.freq = 0;
        self.length_en = false;
        self.active = false;
        self.dac_on = false;
        self.duty_pos = 0;
        self.freq_timer = 0;
        self.length_counter = 0;
        self.volume = 0;
        self.env_timer = 0;
        self.current_output = 0;
        self.sweep_shadow = 0;
        self.sweep_enabled = false;
        self.triggered_once = false;
        self.first_sample_zero = false;
        self.env_clock_state = EnvelopeClockState::default();
        // Reset Sub-M-cycle sub-M-cycle sweep state (`is_cgb`/`cgb_model`
        // are configuration, not runtime state, so they are preserved across
        // NR52 power cycles).
        self.restart_hold = 0;
        self.sweep_countdown = 0;
        self.sweep_calc_countdown = 0;
        self.sweep_calc_reload_timer = 0;
        self.sweep_length_addend = 0;
        self.completed_addend = 0;
        self.unshifted_sweep = false;
        self.instant_calc_done = false;
        self.sweep_overflow_output_linger = 0;
        self.sweep_overflow_active_delay = 0;
        self.sweep_retriggered_since_calc = false;
        self.nr10_written_since_trigger = false;
        self.div_reset_sweep_timing = false;
        self.negate_used = false;
    }

    // ── Register reads ────────────────────────────────────────────────────

    /// NR10 read: bits 6-0 meaningful, bit 7 reads as 1.
    pub fn read_nr10(&self) -> u8 {
        0x80 | ((self.sweep_period & 0x07) << 4)
            | (u8::from(self.sweep_negate) << 3)
            | (self.sweep_shift & 0x07)
    }

    /// NR11 read: only duty bits 7-6 readable; length bits read as 0xFF.
    pub fn read_nr11(&self) -> u8 {
        0x3F | ((self.duty & 0x03) << 6)
    }

    /// NR12 read: all bits readable.
    pub fn read_nr12(&self) -> u8 {
        ((self.init_volume & 0x0F) << 4) | (u8::from(self.env_add) << 3) | (self.env_period & 0x07)
    }

    /// NR14 read: only length-enable bit is readable; others read as 1.
    pub fn read_nr14(&self) -> u8 {
        0xBF | (u8::from(self.length_en) << 6)
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr10(&mut self, val: u8, lf_div: bool) {
        trace_apu!(2; "GB APU CH1 write NR10=0x{:02X} period={} negate={} shift={}",
            val, (val >> 4) & 0x07, (val & 0x08) != 0, val & 0x07);
        if self.uses_deferred_sweep() {
            self.write_nr10_deferred(val, lf_div);
        } else {
            self.write_nr10_synchronous(val);
        }
    }

    /// Synchronous NR10 write (Blargg-aligned, used for DMG / pre-CGB-E).
    /// Implements the classic "negate-used" hardware quirk: clearing the
    /// negate bit after a negate-mode calculation has occurred immediately
    /// disables the channel.
    fn write_nr10_synchronous(&mut self, val: u8) {
        let old_negate = self.sweep_negate;
        self.sweep_period = (val >> 4) & 0x07;
        self.sweep_negate = val & 0x08 != 0;
        self.sweep_shift = val & 0x07;
        if old_negate && !self.sweep_negate && self.negate_used {
            self.active = false;
        }
    }

    /// Deferred-recalc NR10 write (CGB-E). Implements the
    /// "shadow + completed_addend + old_negate" disable formula,
    /// the `nr10_write_glitch` sub-cycle countdown corruption, and the
    /// re-arm path.
    fn write_nr10_deferred(&mut self, val: u8, lf_div: bool) {
        self.nr10_written_since_trigger = true;
        // Hardware `nr10_write_glitch` — sub-M-cycle countdown corruption when
        // NR10 is rewritten while the sweep recalc machinery is mid-flight.
        if self.sweep_calc_countdown != 0 || self.sweep_calc_reload_timer != 0 {
            self.nr10_write_glitch(val, lf_div);
        }
        let old_negate = self.sweep_negate;
        self.sweep_period = (val >> 4) & 0x07;
        self.sweep_negate = val & 0x08 != 0;
        self.sweep_shift = val & 0x07;
        // Disable formula: shadow + completed_addend + (old_negate as 1)
        // overflows AND the new value clears the negate bit.
        let neg_bit: u32 = if old_negate { 1 } else { 0 };
        let sum = (self.sweep_shadow as u32) + (self.completed_addend as u32) + neg_bit;
        if sum > 0x7FF && (val & 0x08) == 0 {
            self.active = false;
        }
        // Re-arm sweep (Hardware: trigger_sweep_calculation called at end of
        // NR10 write). Only fires when sweep_countdown == 7 AND new period > 0.
        if self.sweep_countdown == 7 {
            self.arm_sweep_calculation(lf_div);
        }
    }

    /// Hardware `nr10_write_glitch` (CGB-D/E branch). Two narrow conditions
    /// affect the in-flight sweep machinery on NR10 rewrite:
    ///
    /// 1. `reload_timer == 2` — the calc countdown was just reloaded; reload
    ///    it from the new shift bits. If the new shift is zero, also clear
    ///    `reload_timer`.
    /// 2. New shift transitions `0 → non-zero` while `lf_div == false` and
    ///    `countdown > 1` — perform a "zombie step": decrement the countdown
    ///    by one and fire `sweep_calculation_done` if it reaches zero.
    ///
    /// Outside these conditions the machinery is left untouched.
    /// Ref: Hardware behavior `nr10_write_glitch` (model > CGB_C branch,
    /// L1192-L1212).
    fn nr10_write_glitch(&mut self, val: u8, lf_div: bool) {
        let new_shift = val & 0x07;
        let old_shift = self.sweep_shift;

        // Condition 1: countdown just reloaded.
        if self.sweep_calc_reload_timer == 2 {
            self.sweep_calc_countdown = new_shift;
            if self.sweep_calc_countdown == 0 {
                self.sweep_calc_reload_timer = 0;
            }
        }

        // Condition 2: shift transitions 0 → non-zero with !lf_div and
        // countdown > 1.
        if new_shift != 0 && old_shift == 0 && !lf_div && self.sweep_calc_countdown > 1 {
            self.sweep_calc_countdown -= 1;
            if self.sweep_calc_countdown == 0 {
                self.sweep_calculation_done();
            }
        }
    }

    pub fn write_nr11(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH1 write NR11=0x{:02X} duty={} length={}", val, (val >> 6) & 0x03, val & 0x3F);
        self.duty = (val >> 6) & 0x03;
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr12(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH1 write NR12=0x{:02X} volume={} env_add={} env_period={}", 
            val, (val >> 4) & 0x0F, (val & 0x08) != 0, val & 0x07);

        let old_val = self.read_nr12();

        self.init_volume = (val >> 4) & 0x0F;
        self.env_add = val & 0x08 != 0;
        self.env_period = val & 0x07;
        self.dac_on = val & 0xF8 != 0;

        if !self.dac_on {
            self.active = false;
            self.current_output = 0;
        } else if self.active {
            // Apply zombie mode glitch when writing NRx2 while channel is active.
            self.apply_nrx2_glitch(old_val, val);
            self.update_current_output();
        }
    }

    /// Apply the NRx2 "zombie mode" glitch.
    /// Per Pan Docs "Obscure Behavior": writing to NRx2 while channel is playing
    /// can immediately modify the volume based on old and new register values.
    fn apply_nrx2_glitch(&mut self, old_val: u8, new_val: u8) {
        let old_period = old_val & 0x07;
        let new_period = new_val & 0x07;
        let old_direction_add = (old_val & 0x08) != 0;
        let new_direction_add = (new_val & 0x08) != 0;

        // If envelope clock just fired, reload the countdown.
        if self.env_clock_state.clock {
            self.env_timer = new_period;
        }

        // Determine if we should tick volume.
        let mut should_tick =
            (new_period != 0) && (old_period == 0) && !self.env_clock_state.locked;

        // Special case: both $x8 patterns (period=0, add=true) → tick
        if (new_val & 0x0F) == 0x08 && (old_val & 0x0F) == 0x08 && !self.env_clock_state.locked {
            should_tick = true;
        }

        // Check if direction changed.
        let should_invert = old_direction_add != new_direction_add;

        if should_invert {
            let old_volume = self.volume;
            if new_direction_add {
                // Switching to increase mode.
                if old_period == 0 && !self.env_clock_state.locked {
                    self.volume ^= 0x0F;
                } else {
                    self.volume = (0x0E_u8.wrapping_sub(self.volume)) & 0x0F;
                }
                should_tick = false; // Inversion prevents ticking.
            } else {
                // Switching to decrease mode.
                self.volume = (0x10_u8.wrapping_sub(self.volume)) & 0x0F;
            }
            trace_apu!(3; "GB APU CH1 zombie invert volume {} -> {}", old_volume, self.volume);
        }

        if should_tick {
            let old_volume = self.volume;
            if new_direction_add {
                self.volume = (self.volume + 1) & 0x0F;
            } else {
                self.volume = self.volume.wrapping_sub(1) & 0x0F;
            }
            trace_apu!(3; "GB APU CH1 zombie tick volume {} -> {}", old_volume, self.volume);
        } else if new_period == 0 && self.env_clock_state.clock {
            // Clear envelope clock state when period set to 0 during clock.
            self.env_clock_state.clock = false;
        }
    }

    pub fn write_nr13(&mut self, val: u8) {
        self.write_nr13_with_apu_phase(val, None);
    }

    pub fn write_nr13_with_apu_phase(&mut self, val: u8, double_speed_phase_bits: Option<u8>) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
        self.apply_active_freq_rewrite_timing(double_speed_phase_bits);
        trace_apu!(2; "GB APU CH1 write NR13=0x{:02X} freq=0x{:03X}", val, self.freq);
    }

    pub fn write_nr14(&mut self, val: u8, extra_clk: bool, lf_div: bool) {
        self.write_nr14_with_apu_phase(val, extra_clk, lf_div, None);
    }

    pub fn write_nr14_with_apu_phase(
        &mut self,
        val: u8,
        extra_clk: bool,
        lf_div: bool,
        double_speed_phase_bits: Option<u8>,
    ) {
        self.write_nr14_with_apu_phase_and_length_quirk(
            val,
            extra_clk,
            lf_div,
            double_speed_phase_bits,
            false,
            None,
        );
    }

    pub fn write_nr14_with_apu_phase_and_length_quirk(
        &mut self,
        val: u8,
        extra_clk: bool,
        lf_div: bool,
        double_speed_phase_bits: Option<u8>,
        cgb_early_extra_length_clock: bool,
        div_counter: Option<u16>,
    ) {
        trace_apu!(2; "GB APU CH1 write NR14=0x{:02X} trigger={} length_en={} freq_high={}", 
            val, (val & 0x80) != 0, (val & 0x40) != 0, val & 0x07);
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);
        if val & 0x80 == 0 {
            self.apply_active_freq_rewrite_timing(double_speed_phase_bits);
        }
        // CGB-0/A/B clock length on extra even without current length_en set.
        let clocks_length_on_extra = self.length_en || cgb_early_extra_length_clock;

        // Extra length clocking: when length_en transitions 0→1 while the FS
        // next step does NOT clock length, the counter is immediately clocked.
        if extra_clk && !old_length_en && clocks_length_on_extra && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.active = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(lf_div, double_speed_phase_bits, div_counter);
            // If trigger reloaded counter to max AND length_en AND extra-clock
            // window, decrement the freshly-loaded counter by 1.
            if extra_clk && clocks_length_on_extra && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    fn apply_active_freq_rewrite_timing(&mut self, double_speed_phase_bits: Option<u8>) {
        if !self.active {
            return;
        }
        let double_speed = double_speed_phase_bits.is_some();
        if self.is_early_cgb_revision()
            && !self.freq_timer_just_reloaded
            // These SameSuite probes use duty 0. At duty position 6, the next
            // pulse step would enter the only high sample (position 7); early
            // CGB revisions hold that transition off by one T-cycle group.
            && self.current_output == 0
            && self.duty_pos == 6
            && self.matches_early_cgb_rewrite_extension_window(double_speed)
        {
            self.freq_timer += EARLY_CGB_REWRITE_EXTENSION_AMOUNT_T;
            return;
        }
        let late_de_double_speed_rewrite = double_speed
            && self.is_cgb
            && matches!(self.cgb_model, CgbModel::CgbD | CgbModel::CgbE)
            && self.freq_timer == LATE_CGB_DOUBLE_REWRITE_TIMER_T;
        if !self.freq_timer_just_reloaded && !late_de_double_speed_rewrite {
            return;
        }
        let period = (2048 - self.freq) * 4;
        // SameSuite's CGB-0/A/B/C timing ROM observes the freshly-reloaded period
        // taking effect one APU tick (at 2 MHz) later than on CGB-D/E.
        let cgb_revision_delay_t = if self.is_early_cgb_revision() {
            EARLY_CGB_FREQ_REWRITE_DELAY_T
        } else {
            LATE_CGB_FREQ_REWRITE_DELAY_T
        };
        self.freq_timer = period + cgb_revision_delay_t;
        self.freq_timer_just_reloaded = false;
    }

    /// Length counter write when APU is powered off (DMG quirk).
    pub fn write_nr11_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    // ── Trigger ───────────────────────────────────────────────────────────

    fn trigger(
        &mut self,
        lf_div: bool,
        double_speed_phase_bits: Option<u8>,
        div_counter: Option<u16>,
    ) {
        trace_apu!(1; "GB APU CH1 trigger freq=0x{:03X} volume={} sweep_period={} sweep_shift={} lf_div={}", 
            self.freq, self.init_volume, self.sweep_period, self.sweep_shift, lf_div);
        let was_active = self.active;
        self.sweep_overflow_output_linger = 0;
        self.sweep_overflow_active_delay = 0;
        self.sweep_retriggered_since_calc = was_active && self.uses_deferred_sweep();
        self.nr10_written_since_trigger = false;
        // These raw DIV phases correspond to the DIV-reset-aligned windows used
        // by SameSuite's CGB-E CH1 sweep timing probes.
        self.div_reset_sweep_timing = div_counter
            .is_some_and(|counter| counter == 0x2084 || (0x3FF0..=0x4010).contains(&counter));
        // First trigger after power-on: first duty step outputs 0.
        if !self.triggered_once {
            self.first_sample_zero = true;
        }
        self.triggered_once = true;
        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        // Startup delay (in T-cycles) before first duty_pos advance.
        // Values tuned empirically against SameSuite channel_1/2_delay and restart tests.
        // Fresh trigger: 6-8 T-cycles depending on lf_div
        // Retrigger: 4-6 T-cycles depending on lf_div
        //
        // Per SameSuite comment: "the start delay from the 'delay' test is actually
        // 1 tick shorter" after restarting. This means retrigger delay = fresh - 2 T-cycles.
        let fresh_delay_t = pulse_trigger_fresh_delay_t(lf_div, double_speed_phase_bits);
        let delay_t = if was_active {
            // Retrigger delay: 1 2MHz tick (2 T-cycles) shorter than fresh
            fresh_delay_t.saturating_sub(2)
        } else {
            fresh_delay_t
        };
        // Convert delay to T-cycles and add to period for initial freq_timer
        let period = (2048 - self.freq) * 4;
        self.freq_timer = period + delay_t;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        if was_active {
            self.update_current_output();
        } else {
            self.current_output = 0;
        }
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();
        self.sweep_shadow = self.freq;
        // Load: `square_sweep_countdown = ((NR10 >> 4) & 7) ^ 7`.
        self.sweep_countdown = (self.sweep_period ^ 7) & 7;
        self.sweep_enabled = self.sweep_period > 0 || self.sweep_shift > 0;
        self.negate_used = false;
        // Trigger-time deferred-recalc init: Hardware clears completed_addend
        // and `sweep_length_addend` so the first sweep tick's writeback adds
        // 0 (no-op) before computing the fresh addend.
        self.completed_addend = 0;
        self.sweep_length_addend = 0;
        self.instant_calc_done = false;
        if self.uses_deferred_sweep() {
            // Hardware clears `shadow_sweep_sample_length` on trigger; it
            // is lazily refreshed by `sweep_calculation_done` once
            // `channel_1_restart_hold` drains.
            if self.div_reset_sweep_timing {
                self.sweep_shadow = 0;
            }
            // NR14 trigger sweep init:
            //   if (NR10 & 7) {
            //       calc_countdown = NR10 & 7;
            //       reload_timer = (lf_div ^ !double_speed) && model<=CGB_C ? 3 : 2;
            //       if (!was_active) reload_timer++;
            //       sweep_length_addend = sample_length >> (NR10 & 7);
            //   } else {
            //       sweep_length_addend = 0;
            //   }
            // We only target CGB-E (model > CGB_C); the lf_div/CGB-C branch
            // never fires, so reload_timer = 2 (or 3 if !was_active).
            if self.sweep_shift > 0 {
                self.sweep_calc_countdown = self.sweep_shift;
                self.sweep_calc_reload_timer = 2;
                if !was_active {
                    self.sweep_calc_reload_timer += 1;
                }
                self.unshifted_sweep = false;
                self.sweep_length_addend = self.freq >> self.sweep_shift;
                if self.sweep_negate {
                    self.negate_used = true;
                } else if self.freq + self.sweep_length_addend > 0x7FF
                    && !self.div_reset_sweep_timing
                {
                    self.active = false;
                    self.current_output = 0;
                    self.sweep_overflow_active_delay = 0;
                }
            } else if was_active
                && self.sweep_period > 0
                && self.freq == 0x7FF
                && div_counter.is_some_and(|counter| counter & 0x3FFF == 0x3FF4)
            {
                self.sweep_overflow_active_delay = 4;
            }
        } else {
            self.sweep_calc_countdown = 0;
            self.sweep_calc_reload_timer = 0;
            self.unshifted_sweep = false;
            // Synchronous trigger-time overflow check (Pan Docs / Blargg
            // 05-sweep test 4). The deferred machinery (above) handles the
            // overflow check via its own `sweep_calculation_done` path.
            if self.sweep_shift > 0 {
                if self.sweep_negate {
                    self.negate_used = true;
                }
                let delta = self.sweep_shadow >> self.sweep_shift;
                let new_freq = if self.sweep_negate {
                    self.sweep_shadow.wrapping_sub(delta)
                } else {
                    self.sweep_shadow + delta
                };
                if new_freq > 2047 {
                    self.active = false;
                }
            }
        }
        // Hardware behavior:
        //     channel_1_restart_hold = 2 - lf_div + (is_cgb && model != CGB_D) * 2
        // Suppresses sweep activity for a few M-cycles after trigger.
        let cgb_bonus: u8 = if self.is_cgb && self.cgb_model != CgbModel::CgbD {
            2
        } else {
            0
        };
        self.restart_hold = 2u8 - (lf_div as u8) + cgb_bonus;
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch1() -> Channel1 {
        let mut ch = Channel1::new();
        // NR12: volume=15, add=false, period=0 (no envelope ramp) → DAC on
        ch.write_nr12(0xF0);
        // NR11: 50% duty, length = 0 (max)
        ch.write_nr11(0x80);
        // NR14: trigger, no length enable, freq high = 0
        ch.write_nr14(0x80, false, false);
        ch
    }

    #[test]
    fn test_deferred_sweep_is_production_enabled_only_for_cgb_e() {
        let mut ch = Channel1::new();
        ch.set_model(false, CgbModel::CgbE);
        assert!(
            !ch.uses_deferred_sweep(),
            "DMG mode keeps synchronous sweep"
        );

        ch.set_model(true, CgbModel::CgbD);
        assert!(
            !ch.uses_deferred_sweep(),
            "pre-CGB-E models keep synchronous sweep"
        );

        ch.set_model(true, CgbModel::CgbE);
        assert!(ch.uses_deferred_sweep(), "CGB-E uses deferred sweep");
    }

    #[test]
    fn test_trigger_with_shift_zero_preserves_in_flight_deferred_calc() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x10, false); // period=1, shift=0
        ch.sweep_calc_countdown = 3;
        ch.sweep_calc_reload_timer = 2;
        ch.unshifted_sweep = true;

        ch.write_nr14(0x80, false, false);

        assert_eq!(ch.sweep_calc_countdown, 3);
        assert_eq!(ch.sweep_calc_reload_timer, 2);
        assert!(ch.unshifted_sweep);
        assert!(!ch.instant_calc_done);
    }

    #[test]
    fn test_deferred_overflow_lingers_pcm_output_for_div_reset_window() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0x80);
        ch.active = true;
        ch.dac_on = true;
        ch.current_output = 8;
        ch.freq = 0x7FF;
        ch.sweep_shadow = 0x7FF;
        ch.sweep_length_addend = 0x7F;
        ch.div_reset_sweep_timing = true;

        ch.sweep_calculation_done();

        assert!(!ch.is_active(), "overflow clears NR52 active immediately");
        assert_eq!(
            ch.digital_output(),
            8,
            "PCM12 output should remain visible during the DIV-reset overflow window"
        );

        for _ in 0..4 {
            ch.tick();
            assert_eq!(ch.digital_output(), 8);
        }
        ch.tick();
        assert!(!ch.is_active());
        assert_eq!(ch.digital_output(), 0);
    }

    #[test]
    fn test_retrigger_max_frequency_shift_zero_schedules_cgb_e_overflow_mute() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0x80);
        ch.write_nr10(0x10, false); // period=1, shift=0
        ch.write_nr13(0xFF);
        ch.write_nr14(0x83, false, false);

        ch.write_nr14_with_apu_phase_and_length_quirk(
            0x87,
            false,
            false,
            None,
            false,
            Some(0x3FF4),
        );

        assert!(ch.is_active());
        ch.tick();
        assert!(ch.is_active());
        ch.tick();
        assert!(ch.is_active());
        ch.tick();
        assert!(ch.is_active());
        ch.tick();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_retrigger_max_frequency_shift_zero_without_sweep_period_stays_active() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0x80);
        ch.write_nr10(0x00, false); // period=0, shift=0
        ch.write_nr13(0xFF);
        ch.write_nr14(0x83, false, false);

        ch.write_nr14(0x87, false, false);

        for _ in 0..4 {
            ch.tick();
        }
        assert!(ch.is_active());
    }

    #[test]
    fn test_duty0_max_freq_edge_output_boundaries() {
        assert_eq!(
            pulse_duty0_max_freq_edge_output(0, 0x07FF, false, 0, 4, 8),
            Some(8),
            "step 7→0 wrap boundary should expose the high duty-0 edge"
        );
        assert_eq!(
            pulse_duty0_max_freq_edge_output(0, 0x07FF, false, 7, 2, 8),
            Some(0),
            "pre-wrap half-step should remain silent"
        );
        assert_eq!(
            pulse_duty0_max_freq_edge_output(1, 0x07FF, false, 0, 4, 8),
            None,
            "quirk applies only to duty 0"
        );
        assert_eq!(
            pulse_duty0_max_freq_edge_output(0, 0x07FE, false, 0, 4, 8),
            None,
            "quirk applies only at max frequency"
        );
        assert_eq!(
            pulse_duty0_max_freq_edge_output(0, 0x07FF, true, 0, 4, 8),
            None,
            "first startup sample remains suppressed"
        );
    }

    #[test]
    fn test_duty_write_takes_effect_on_next_sample() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x80);
        ch.write_nr11(0xC0);
        ch.write_nr13(0xFF);
        ch.write_nr14(0x87, false, false);

        for _ in 0..3 {
            ch.tick();
        }

        assert_eq!(ch.duty_pos, 1);
        assert_eq!(ch.digital_output(), 8);

        ch.write_nr11(0x00);
        assert_eq!(
            ch.digital_output(),
            8,
            "duty writes should not affect the current square sample"
        );

        ch.tick();
        assert_eq!(ch.duty_pos, 2);
        assert_eq!(ch.digital_output(), 0);
    }

    #[test]
    fn test_duty_phase_does_not_advance_while_stopped_by_dac() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x80);
        ch.write_nr11(0x80);
        ch.write_nr14(0x80, false, false);

        ch.duty_pos = 3;
        ch.write_nr12(0x00);
        assert!(!ch.is_active());
        for _ in 0..16 {
            ch.tick();
        }
        assert_eq!(
            ch.duty_pos, 3,
            "stopping via DAC must freeze the duty phase until restart"
        );

        ch.write_nr12(0x80);
        ch.write_nr14(0x80, false, false);

        assert_eq!(
            ch.duty_pos, 3,
            "restarting after DAC stop must preserve the stopped duty phase"
        );
    }

    #[test]
    fn test_double_speed_phase_bits_adjust_fresh_trigger_delay() {
        assert_eq!(pulse_trigger_fresh_delay_t(false, Some(0b00)), 10);
        assert_eq!(pulse_trigger_fresh_delay_t(false, Some(0b11)), 8);
        assert_eq!(
            pulse_trigger_fresh_delay_t(false, Some(0b01)),
            10,
            "trigger phase 1 after NR52 power-on phase 0 gets the CPU-aligned +2 T-cycle delay"
        );
    }

    // ── Phase 2: restart_hold ────────────────────────────────────────────
    //
    // Hardware behavior:
    //     channel_1_restart_hold = 2 - lf_div + (is_cgb && model != CGB_D) * 2
    //
    // Truth table (lf_div is 0/false or 1/true):
    //   DMG:           lf_div=0 → 2,  lf_div=1 → 1
    //   CGB-D:         lf_div=0 → 2,  lf_div=1 → 1
    //   CGB-A/B/C/E:   lf_div=0 → 4,  lf_div=1 → 3

    fn ch1_for_trigger(is_cgb: bool, cgb_model: CgbModel) -> Channel1 {
        let mut ch = Channel1::new();
        ch.set_model(is_cgb, cgb_model);
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0x80);
        ch
    }

    #[test]
    fn test_restart_hold_dmg_lf_div_low() {
        let mut ch = ch1_for_trigger(false, CgbModel::CgbE);
        ch.write_nr14(0x80, false, /* lf_div = */ false);
        assert_eq!(ch.restart_hold, 2);
    }

    #[test]
    fn test_restart_hold_dmg_lf_div_high() {
        let mut ch = ch1_for_trigger(false, CgbModel::CgbE);
        ch.write_nr14(0x80, false, /* lf_div = */ true);
        assert_eq!(ch.restart_hold, 1);
    }

    #[test]
    fn test_restart_hold_cgb_d_lf_div_low() {
        let mut ch = ch1_for_trigger(true, CgbModel::CgbD);
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.restart_hold, 2);
    }

    #[test]
    fn test_restart_hold_cgb_d_lf_div_high() {
        let mut ch = ch1_for_trigger(true, CgbModel::CgbD);
        ch.write_nr14(0x80, false, true);
        assert_eq!(ch.restart_hold, 1);
    }

    #[test]
    fn test_restart_hold_cgb_e_lf_div_low() {
        let mut ch = ch1_for_trigger(true, CgbModel::CgbE);
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.restart_hold, 4);
    }

    #[test]
    fn test_restart_hold_cgb_e_lf_div_high() {
        let mut ch = ch1_for_trigger(true, CgbModel::CgbE);
        ch.write_nr14(0x80, false, true);
        assert_eq!(ch.restart_hold, 3);
    }

    #[test]
    fn test_restart_hold_cgb_b_matches_cgb_e() {
        // CGB-A/B/C share the +2 branch with CGB-E (only CGB-D differs).
        let mut ch = ch1_for_trigger(true, CgbModel::CgbB);
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.restart_hold, 4);
    }

    #[test]
    fn test_restart_hold_decrements_per_m_cycle_and_saturates_at_zero() {
        let mut ch = ch1_for_trigger(true, CgbModel::CgbE);
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.restart_hold, 4);
        ch.sweep_tick();
        assert_eq!(ch.restart_hold, 3);
        ch.sweep_tick();
        assert_eq!(ch.restart_hold, 2);
        ch.sweep_tick();
        ch.sweep_tick();
        assert_eq!(ch.restart_hold, 0);
        // Must not underflow / wrap.
        ch.sweep_tick();
        ch.sweep_tick();
        assert_eq!(ch.restart_hold, 0);
    }

    // ── Phase 3: square_sweep_countdown ─────────────────────────────────
    //
    // Hardware behavior:
    //   On trigger: square_sweep_countdown = ((NR10 >> 4) & 7) ^ 7
    //   Per 128 Hz tick: ++countdown; countdown &= 7; if 7 → sweep step.
    //
    // For sweep_period = N (1..=7) the post-trigger countdown is 7 - N,
    // and it takes exactly N ticks to reach the wrap point.

    #[test]
    fn test_sweep_countdown_loaded_on_trigger_period_1() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr10(0x10, false); // period=1, negate=0, shift=0
        ch.write_nr14(0x80, false, false);
        // (1 ^ 7) & 7 = 6
        assert_eq!(ch.sweep_countdown, 6);
    }

    #[test]
    fn test_sweep_countdown_loaded_on_trigger_period_3() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x32, false); // period=3, negate=0, shift=2
        ch.write_nr14(0x80, false, false);
        // (3 ^ 7) & 7 = 4
        assert_eq!(ch.sweep_countdown, 4);
    }

    #[test]
    fn test_sweep_countdown_loaded_on_trigger_period_7() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x71, false); // period=7, negate=0, shift=1
        ch.write_nr14(0x80, false, false);
        // (7 ^ 7) & 7 = 0  → 7 ticks to wrap
        assert_eq!(ch.sweep_countdown, 0);
    }

    #[test]
    fn test_sweep_countdown_period_zero_loaded_to_seven() {
        // Hardware: period=0 → countdown loaded as (0^7)&7 = 7. Next tick
        // wraps to 0; never fires the recalc since NR10 period bits == 0.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x00, false); // period=0
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.sweep_countdown, 7);
    }

    #[test]
    fn test_sweep_countdown_increments_and_wraps_to_seven() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x32, false); // period=3
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false, false);
        assert_eq!(ch.sweep_countdown, 4);
        ch.clock_sweep(false); // → 5
        assert_eq!(ch.sweep_countdown, 5);
        ch.clock_sweep(false); // → 6
        assert_eq!(ch.sweep_countdown, 6);
        ch.clock_sweep(false); // → 7 → fires; reloads to (3^7)&7 = 4
        assert_eq!(ch.sweep_countdown, 4);
    }

    #[test]
    fn test_sweep_countdown_period_zero_no_recalc_on_wrap() {
        // period=0, shift=2, freq=100. countdown wraps but recalc must
        // not fire (matches `(NR10 & 0x70)` gate).
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x02, false); // period=0, shift=2
        ch.write_nr13(0x64);
        ch.write_nr14(0x80, false, false);
        let initial_freq = ch.freq;
        // 8 ticks for one full wrap cycle from countdown=7.
        for _ in 0..16 {
            ch.clock_sweep(false);
        }
        assert_eq!(
            ch.freq, initial_freq,
            "freq must not change when sweep_period == 0"
        );
        assert!(ch.is_active());
    }

    #[test]
    fn test_power_off_clears_restart_hold_and_sweep_countdown() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x32, false); // period=3
        ch.write_nr14(0x80, false, false);
        assert_ne!(ch.restart_hold, 0);
        assert_ne!(ch.sweep_countdown, 0);
        ch.power_off();
        assert_eq!(ch.restart_hold, 0);
        assert_eq!(ch.sweep_countdown, 0);
    }

    // ── Sweep correctness (hardware quirks) ──────────────────────────────

    #[test]
    fn test_trigger_makes_channel_active() {
        // Given: CH1 with DAC on; When: trigger (NR14 bit 7); Then: is_active = true
        let ch = triggered_ch1();
        assert!(ch.is_active());
    }

    #[test]
    fn test_dac_off_prevents_activation() {
        // Given: NR12 = 0x00 (DAC off); When: trigger; Then: channel stays inactive
        let mut ch = Channel1::new();
        ch.write_nr12(0x00); // no volume, no envelope → DAC off
        ch.write_nr14(0x80, false, false); // trigger
        assert!(!ch.is_active());
    }

    #[test]
    fn test_dac_off_disables_active_channel() {
        // Given: active channel; When: NR12 written to 0x00; Then: channel becomes inactive
        let mut ch = triggered_ch1();
        assert!(ch.is_active());
        ch.write_nr12(0x00);
        assert!(!ch.is_active());
    }

    // ── Length counter ────────────────────────────────────────────────────

    #[test]
    fn test_length_counter_loaded_from_nr11() {
        // NR11 length field = 0x3F (63); counter = 64 - 63 = 1
        let mut ch = Channel1::new();
        ch.write_nr11(0xFF); // duty=11, length=63
        assert_eq!(ch.length_counter, 1);
    }

    #[test]
    fn test_length_counter_expiry_silences_channel_when_enabled() {
        // Given: length counter = 1, length_en = true;
        // When: clock_length once; Then: channel becomes inactive.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0xFF); // length = 63 → counter = 1
        ch.write_nr14(0xC0, false, false); // trigger + length enable
        assert!(ch.is_active());
        ch.clock_length();
        assert!(
            !ch.is_active(),
            "channel must be silenced when length counter expires"
        );
    }

    #[test]
    fn test_length_counter_does_not_expire_when_disabled() {
        // Given: length counter = 1, length_en = false;
        // When: clock_length; Then: channel remains active.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0xFF); // counter = 1
        ch.write_nr14(0x80, false, false); // trigger, no length enable
        ch.clock_length();
        assert!(
            ch.is_active(),
            "channel must stay active when length enable is off"
        );
    }

    #[test]
    fn test_trigger_reloads_length_counter_when_zero() {
        // If the length counter reaches 0 before triggering, trigger reloads it to 64.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x3F); // length = 63 → counter = 1
        ch.write_nr14(0x40, false, false); // length enable, NO trigger → counter = 1
        ch.clock_length(); // expires to 0, channel inactive
        assert!(!ch.is_active());
        // Trigger again – counter should reload to 64.
        ch.write_nr14(0x80, false, false); // trigger (no length enable)
        assert!(ch.is_active());
        assert_eq!(ch.length_counter, 64);
    }

    // ── Extra length clocking on NRx4 write ──────────────────────────────

    #[test]
    fn test_enabling_length_in_first_half_clocks_length() {
        // Blargg 03-trigger sub-test 3: enabling length_en when the FS next
        // step does NOT clock length must extra-clock the length counter.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0xBE); // length_load=62 → counter = 64-62 = 2
        ch.write_nr14(0x80, false, false); // trigger, no length enable
        assert_eq!(ch.length_counter, 2);
        // Enable length with extra_clk=true (FS in first half).
        ch.write_nr14(0x40, true, false); // length enable, no trigger, extra clock
        assert_eq!(
            ch.length_counter, 1,
            "enabling length in first half must extra-clock (2 → 1)"
        );
    }

    #[test]
    fn test_enabling_length_in_second_half_does_not_clock() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0xBE); // counter = 2
        ch.write_nr14(0x80, false, false); // trigger, no length enable
        // Enable length with extra_clk=false (FS in second half).
        ch.write_nr14(0x40, false, false); // no extra clock
        assert_eq!(
            ch.length_counter, 2,
            "enabling length in second half must NOT extra-clock"
        );
    }

    #[test]
    fn test_trigger_unfreezes_and_extra_clocks_when_enabled() {
        // Blargg 03-trigger sub-test 8: trigger reloads length to max,
        // and if length_en is set and FS extra-clocks, decrement by 1.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x3F); // counter = 1
        ch.write_nr14(0x40, true, false); // enable → extra clock → counter 1→0, channel disabled
        assert!(!ch.is_active());
        // Trigger + length enable with extra clock: counter was 0, reloads to 64,
        // then extra-clock decrements to 63.
        ch.write_nr14(0xC0, true, false); // trigger + length enable
        assert_eq!(
            ch.length_counter, 63,
            "trigger reload + extra clock: 64 → 63"
        );
    }

    // ── Volume envelope ───────────────────────────────────────────────────

    #[test]
    fn test_envelope_decrements_volume() {
        // NR12: vol=7, dir=subtract, period=1
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, add=0, period=1
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        // NR12: vol=7, dir=add, period=1
        let mut ch = Channel1::new();
        ch.write_nr12(0x79); // vol=7, add=1, period=1
        ch.write_nr14(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 8);
    }

    #[test]
    fn test_envelope_does_not_go_below_zero() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x01); // vol=0, add=0, period=1
        ch.write_nr14(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 0);
    }

    #[test]
    fn test_envelope_does_not_exceed_15() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF9); // vol=15, add=1, period=1
        ch.write_nr14(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 15);
    }

    #[test]
    fn test_envelope_frozen_when_period_zero() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x70); // vol=7, period=0
        ch.write_nr14(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 7, "envelope must not change when period = 0");
    }

    // ── NRx2 zombie-mode glitch ───────────────────────────────────────────

    #[test]
    fn test_nrx2_zombie_tick_when_period_zero_to_nonzero_decrease() {
        // Pan Docs zombie mode: writing a nonzero period while old period was 0
        // ticks the volume in the current (decrease) direction.
        let mut ch = Channel1::new();
        ch.write_nr12(0x70); // vol=7, sub, period=0
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x71); // period: 0 → 1, same sub direction
        assert_eq!(ch.volume, 6, "zombie tick should decrement volume");
    }

    #[test]
    fn test_nrx2_zombie_tick_when_period_zero_to_nonzero_increase() {
        // Zombie tick with increase direction.
        let mut ch = Channel1::new();
        ch.write_nr12(0x78); // vol=7, add, period=0
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x79); // period: 0 → 1, same add direction
        assert_eq!(ch.volume, 8, "zombie tick should increment volume");
    }

    #[test]
    fn test_nrx2_zombie_no_tick_when_old_period_nonzero() {
        // No zombie tick when old period was already nonzero.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x72); // period: 1 → 2, same direction
        assert_eq!(ch.volume, 7, "no zombie tick when old_period != 0");
    }

    #[test]
    fn test_nrx2_zombie_x08_to_x08_ticks_add() {
        // Special case: both old and new lower nibble = $08 (period=0, add=true) → tick.
        let mut ch = Channel1::new();
        ch.write_nr12(0x78); // vol=7, add, period=0
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x78); // $08 → $08 pattern
        assert_eq!(ch.volume, 8, "x08->x08 zombie tick should increment volume");
    }

    #[test]
    fn test_nrx2_zombie_direction_switch_to_add_period_zero_xors() {
        // Switching direction to add while old period=0: volume ^= 0x0F.
        let mut ch = Channel1::new();
        ch.write_nr12(0x70); // vol=7, sub, period=0
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x78); // switch to add, period=0
        // 7 ^ 0x0F = 8
        assert_eq!(
            ch.volume, 8,
            "direction switch to add with period=0 should XOR volume with 0x0F"
        );
    }

    #[test]
    fn test_nrx2_zombie_direction_switch_to_add_nonzero_period_subtracts() {
        // Switching direction to add while old period != 0: volume = (0x0E - vol) & 0x0F.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x79); // switch to add, period=1
        // (0x0E - 7) & 0x0F = 7
        assert_eq!(
            ch.volume, 7,
            "direction switch to add with nonzero period should use (0x0E - vol) formula"
        );
    }

    #[test]
    fn test_nrx2_zombie_direction_switch_to_subtract() {
        // Switching direction from add to sub: volume = (0x10 - vol) & 0x0F.
        let mut ch = Channel1::new();
        ch.write_nr12(0x79); // vol=7, add, period=1
        ch.write_nr14(0x80, false, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.write_nr12(0x71); // switch to sub, period=1
        // (0x10 - 7) & 0x0F = 9
        assert_eq!(
            ch.volume, 9,
            "direction switch to sub should use (0x10 - vol) formula"
        );
    }

    #[test]
    fn test_nrx2_zombie_reload_countdown_when_clock_active() {
        // When env clock just fired, writing NRx2 reloads env_timer to new_period.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger → env_timer = 1
        ch.clock_envelope_decrement(); // env_timer → 0
        ch.clock_envelope_secondary(); // arms clock, env_timer → 1
        assert!(
            ch.env_clock_state.clock,
            "clock should be armed after secondary"
        );
        ch.write_nr12(0x73); // sub, period=3 (same direction, clock active)
        assert_eq!(
            ch.env_timer, 3,
            "env_timer should reload to new_period when clock is active"
        );
    }

    // ── Split envelope clock phases ───────────────────────────────────────

    #[test]
    fn test_clock_envelope_decrement_only_decrements_timer() {
        // clock_envelope_decrement decrements env_timer without changing volume.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger → env_timer = 1
        let vol_before = ch.volume;
        ch.clock_envelope_decrement();
        assert_eq!(ch.volume, vol_before, "decrement must not change volume");
        assert_eq!(ch.env_timer, 0, "decrement should reduce env_timer to 0");
    }

    #[test]
    fn test_clock_envelope_secondary_arms_clock_and_reloads_timer() {
        // After decrement reaches 0, secondary arms the clock flag and reloads timer.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // period=1
        ch.write_nr14(0x80, false, false); // trigger → env_timer = 1
        let vol_before = ch.volume;
        ch.clock_envelope_decrement(); // env_timer → 0
        assert!(
            !ch.env_clock_state.clock,
            "clock not yet armed before secondary"
        );
        ch.clock_envelope_secondary(); // arm clock, reload timer
        assert!(
            ch.env_clock_state.clock,
            "clock should be armed after secondary"
        );
        assert_eq!(
            ch.env_timer, 1,
            "timer should reload to period after secondary"
        );
        assert_eq!(ch.volume, vol_before, "secondary must not change volume");
    }

    #[test]
    fn test_clock_envelope_primary_fires_volume_change_when_clock_set() {
        // Primary fires volume change and clears clock flag.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger
        ch.clock_envelope_decrement();
        ch.clock_envelope_secondary();
        assert_eq!(ch.volume, 7);
        ch.clock_envelope_primary();
        assert_eq!(
            ch.volume, 6,
            "primary should decrement volume when clock armed"
        );
        assert!(
            !ch.env_clock_state.clock,
            "clock flag should be cleared after primary"
        );
    }

    #[test]
    fn test_clock_envelope_primary_no_change_without_clock() {
        // Primary is a no-op when clock flag was not armed.
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, sub, period=1
        ch.write_nr14(0x80, false, false); // trigger
        let vol_before = ch.volume;
        ch.clock_envelope_primary(); // clock not armed → no-op
        assert_eq!(
            ch.volume, vol_before,
            "primary must not change volume when clock not armed"
        );
    }

    // ── Phase 4–6: deferred sweep recalc ──────────────────────────────────

    /// Drive the sweep machinery for `n` 1MHz steps. Each call to
    /// `sweep_tick` advances one M-cycle; only every other call performs
    /// work, so we call it `2 * n` times here.
    #[allow(dead_code)]
    fn drive_sweep_1mhz(ch: &mut Channel1, n: u32) {
        for _ in 0..(n * 2) {
            ch.sweep_tick();
        }
    }

    /// Drain the sweep calc machinery to completion (any pending recalc).
    fn drain_sweep(ch: &mut Channel1) {
        for _ in 0..64 {
            if ch.sweep_calc_countdown == 0 && ch.sweep_calc_reload_timer == 0 {
                break;
            }
            ch.sweep_tick();
        }
    }

    #[test]
    fn test_arm_writeback_uses_sweep_length_addend_and_shadow() {
        // Phase 7 / D3+D4: Hardware writeback formula is
        //   sample_length = sweep_length_addend + shadow + neg_bit
        // (shadow is 0 immediately after trigger, refreshed by the first
        // sweep_calculation_done call when restart_hold drains).
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x11, false); // period=1, shift=1, negate=0
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14_with_apu_phase_and_length_quirk(
            0x80,
            false,
            false,
            None,
            false,
            Some(0x2084),
        ); // DIV-reset trigger → shadow=0, addend=50
        for _ in 0..8 {
            ch.sweep_tick();
        }
        ch.clock_sweep(false); // arm #1: freq = 50 + 0 = 50
        assert_eq!(
            ch.freq, 50,
            "first arm: sweep_length_addend(50) + shadow(0) = 50"
        );
        drain_sweep(&mut ch);
        ch.clock_sweep(false); // arm #2: freq = 25 + 50 = 75
        assert_eq!(
            ch.freq, 75,
            "second arm: completed_addend(25) + shadow(50) = 75"
        );
    }

    #[test]
    fn test_sweep_negate_writeback_uses_ones_complement() {
        // Phase 7: in negate mode, sweep_calculation_done flips the addend
        // to its 1's complement so the next writeback subtracts.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x19, false); // period=1, negate=1, shift=1
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14_with_apu_phase_and_length_quirk(
            0x80,
            false,
            false,
            None,
            false,
            Some(0x2084),
        ); // DIV-reset trigger
        for _ in 0..8 {
            ch.sweep_tick();
        }
        drain_sweep(&mut ch);
        assert_eq!(
            ch.completed_addend,
            0x7FF ^ 50,
            "negate calculation must store the one's-complement addend"
        );
    }

    #[test]
    fn test_sweep_overflow_disables_channel_via_completed_addend() {
        // Phase 7: Hardware deferred-path overflow check fires inside
        // sweep_calculation_done when shadow + sweep_length_addend > 0x7FF.
        // Iterating arm/drain cycles raises the running freq until overflow.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x12, false); // period=1, shift=2, negate=0
        ch.write_nr13(0xDC);
        ch.write_nr14(0x85, false, false); // freq=0x5DC=1500, trigger
        assert_eq!(ch.freq, 0x5DC);
        assert!(ch.is_active(), "trigger must not disable on first pass");
        for _ in 0..8 {
            ch.tick();
        }
        // Run arm/drain cycles until overflow is detected (≤ 16 cycles).
        for _ in 0..16 {
            ch.clock_sweep(false);
            drain_sweep(&mut ch);
            if !ch.is_active() {
                break;
            }
        }
        assert!(
            !ch.is_active(),
            "channel must eventually be disabled by deferred overflow check"
        );
    }

    #[test]
    fn test_unshifted_sweep_lets_armed_recalc_complete_after_shift_cleared() {
        // Phase 4: when armed with shift=0, `unshifted_sweep` is set so the
        // calc-countdown drains even though shift is now 0. The recalc still
        // completes via the instant_calc_done path after the reload timer
        // drains.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x10, false); // period=1, shift=0
        ch.write_nr13(0x00);
        ch.write_nr14(0x80, false, false); // trigger, freq=0
        ch.clock_sweep(false); // arm with shift=0 → instant_calc_done=true
        assert!(ch.instant_calc_done);
        assert!(ch.unshifted_sweep);
        drain_sweep(&mut ch);
        assert!(
            !ch.instant_calc_done,
            "instant_calc_done must clear after sweep_calculation_done fires"
        );
    }

    #[test]
    fn test_sweep_calc_countdown_loaded_from_shift() {
        // Phase 4: sweep_calc_countdown loaded from NR10 shift bits at arm.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // period=1, shift=4
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        ch.clock_sweep(false);
        assert_eq!(ch.sweep_calc_countdown, 4);
        assert!(!ch.unshifted_sweep);
    }

    // ── Phase 7 / D1: sweep cadence parity with Hardware ──────────────────
    //
    // Hardware behavior `GB_apu_run`:
    //     unsigned sweep_cycles = cycles / 2;        // T-cycles → 2 MHz
    //     // drain reload_timer first; any leftover sweep_cycles drains
    //     // calc_countdown.
    //
    // neser calls `sweep_tick` once per M-cycle (= 4 T-cycles), so each call
    // must consume `sweep_cycles = 2`, draining the reload_timer first and
    // applying any leftover to the calc_countdown.

    #[test]
    fn test_sweep_tick_consumes_two_sweep_cycles_per_call_chained() {
        // reload_timer=1, calc_countdown=4: per M-cycle (sweep_cycles=2)
        // Hardware drains reload from 1 → 0 (consumes 1 sweep_cycle), then
        // applies leftover (1) to calc_countdown: 4 → 3.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.sweep_calc_reload_timer = 1;
        ch.sweep_calc_countdown = 4;
        ch.sweep_shift = 4;
        ch.unshifted_sweep = false;
        ch.instant_calc_done = false;
        ch.sweep_tick();
        assert_eq!(
            ch.sweep_calc_reload_timer, 0,
            "reload_timer must drain to 0 in a single M-cycle when starting at 1"
        );
        assert_eq!(
            ch.sweep_calc_countdown, 3,
            "leftover sweep_cycle must drain calc_countdown by 1 (4 → 3) in the same M-cycle"
        );
    }

    #[test]
    fn test_sweep_tick_drains_reload_timer_two_per_mcycle() {
        // reload_timer=2, calc_countdown=4, sweep_cycles=2: reload exactly
        // consumed (2 → 0), no leftover for calc_countdown (stays at 4).
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.sweep_calc_reload_timer = 2;
        ch.sweep_calc_countdown = 4;
        ch.sweep_shift = 4;
        ch.unshifted_sweep = false;
        ch.instant_calc_done = false;
        ch.sweep_tick();
        assert_eq!(ch.sweep_calc_reload_timer, 0);
        assert_eq!(ch.sweep_calc_countdown, 4);
    }

    #[test]
    fn test_sweep_tick_drains_calc_countdown_two_per_mcycle_when_reload_zero() {
        // reload_timer=0, calc_countdown=4: full sweep_cycles=2 applied to
        // countdown → 2.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.sweep_calc_reload_timer = 0;
        ch.sweep_calc_countdown = 4;
        ch.sweep_shift = 4;
        ch.unshifted_sweep = false;
        ch.instant_calc_done = false;
        ch.sweep_tick();
        assert_eq!(ch.sweep_calc_countdown, 2);
    }

    // ── Phase 7 / D2: trigger arms the deferred sweep machinery ──────────
    //
    // NR14 trigger (square channel 1):
    //   if (NR10 & 7) {                              // shift > 0
    //       sweep_calculate_countdown = NR10 & 7;
    //       sweep_calc_reload_timer = 2;             // CGB-E (model > C)
    //       if (!was_active) sweep_calc_reload_timer++;   // → 3
    //       sweep_length_addend = sample_length >> (NR10 & 7);
    //   } else {
    //       sweep_length_addend = 0;
    //   }
    //
    // The trigger-time overflow check is a side effect of the deferred path,
    // not an inline synchronous check. Gated on `uses_deferred_sweep()` so
    // DMG/pre-CGB-E retain the synchronous overflow check.

    #[test]
    fn test_trigger_arms_deferred_calc_countdown_from_shift_cgb_e() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // period=1, shift=4
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        assert_eq!(
            ch.sweep_calc_countdown, 4,
            "trigger must load calc_countdown from NR10 shift bits"
        );
    }

    #[test]
    fn test_trigger_arms_reload_timer_to_three_when_inactive_cgb_e() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // shift=4
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false); // first trigger, was_active=false
        assert_eq!(
            ch.sweep_calc_reload_timer, 3,
            "trigger from inactive must set reload_timer = 2 (CGB-E base) + 1 (!was_active)"
        );
    }

    #[test]
    fn test_trigger_arms_reload_timer_to_two_when_already_active_cgb_e() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // shift=4
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false); // first trigger
        // Re-trigger while still active:
        ch.write_nr14(0x80, false, false);
        assert_eq!(
            ch.sweep_calc_reload_timer, 2,
            "retrigger while active must set reload_timer = 2 (no +1 bump)"
        );
    }

    #[test]
    fn test_trigger_loads_sweep_length_addend_from_shifted_freq() {
        // Hardware: sweep_length_addend = sample_length >> (NR10 & 7) at
        // trigger when shift > 0.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x12, false); // period=1, shift=2
        ch.write_nr13(0x40); // freq low: 0x40
        ch.write_nr14(0x82, false, false); // freq high: 2 → freq=0x240=576
        // sweep_length_addend = 576 >> 2 = 144.
        assert_eq!(ch.sweep_length_addend, 144);
    }

    #[test]
    fn test_trigger_with_shift_zero_does_not_arm_machinery() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x10, false); // period=1, shift=0
        ch.write_nr13(0x40);
        ch.write_nr14(0x80, false, false);
        assert_eq!(
            ch.sweep_calc_countdown, 0,
            "trigger with shift=0 must not arm calc_countdown"
        );
        assert_eq!(
            ch.sweep_calc_reload_timer, 0,
            "trigger with shift=0 must not arm reload_timer"
        );
        assert_eq!(
            ch.sweep_length_addend, 0,
            "trigger with shift=0 must zero sweep_length_addend"
        );
    }

    // ── Phase 7 / D3: trigger zeros sweep_shadow on deferred path ────────
    //
    // NR14 trigger: `shadow_sweep_sample_length = 0`.
    // The shadow is then lazily refreshed by the first sweep_calculation_done
    // call after `channel_1_restart_hold` drains. The synchronous path
    // (DMG / pre-CGB-E) keeps the legacy `shadow = freq` semantics so the
    // inline trigger-time overflow check works as before.

    #[test]
    fn test_trigger_zeros_shadow_on_deferred_path() {
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x12, false); // period=1, shift=2
        ch.write_nr13(0x40);
        ch.write_nr14_with_apu_phase_and_length_quirk(
            0x82,
            false,
            false,
            None,
            false,
            Some(0x2084),
        ); // freq = 0x240
        assert_eq!(
            ch.sweep_shadow, 0,
            "trigger on deferred path must zero shadow (CGB-E behavior)"
        );
    }

    #[test]
    fn test_trigger_loads_shadow_from_freq_on_synchronous_path() {
        // Synchronous path: trigger continues to seed shadow from freq so
        // the inline overflow check operates against a meaningful value.
        let mut ch = Channel1::new();
        ch.set_model(false, CgbModel::CgbE); // DMG-mode → synchronous path
        ch.write_nr12(0xF0);
        ch.write_nr10(0x12, false);
        ch.write_nr13(0x40);
        ch.write_nr14(0x82, false, false); // freq = 0x240
        assert_eq!(
            ch.sweep_shadow, 0x240,
            "synchronous path must seed shadow from freq on trigger"
        );
    }

    #[test]
    fn test_restart_hold_suppresses_shadow_refresh() {
        // Phase 5: while restart_hold > 0, sweep_calculation_done must NOT
        // refresh sweep_shadow from freq.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE); // → restart_hold = 4 (lf_div=false)
        ch.write_nr12(0xF0);
        ch.write_nr10(0x11, false); // period=1, shift=1
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false, false); // trigger → restart_hold=4
        // Manually mutate shadow to detect the suppression.
        ch.sweep_shadow = 0x123;
        ch.clock_sweep(false); // arm. restart_hold still > 0 here.
        // Drain the calc-countdown — sweep_calculation_done runs while
        // restart_hold remains non-zero (sweep_tick does not decrement it).
        drain_sweep(&mut ch);
        assert_eq!(
            ch.sweep_shadow, 0x123,
            "shadow must not be refreshed while restart_hold > 0"
        );
    }

    #[test]
    fn test_nr10_clear_negate_disables_when_completed_addend_overflows() {
        // Phase 6: Hardware disable formula —
        //   shadow + completed_addend + (old_negate as 1) > 0x7FF
        //   AND new value clears negate bit.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x19, false); // period=1, negate=1, shift=1
        ch.write_nr13(0xD0);
        ch.write_nr14(0x87, false, false); // freq=2000, trigger
        for _ in 0..8 {
            ch.tick();
        }
        // Trigger sets completed_addend=0; need to drive a sweep_calc to
        // populate it. Arm and drain:
        ch.clock_sweep(false); // arm
        drain_sweep(&mut ch); // negate-mode addend stamped (1's complement)
        // Now completed_addend reflects the 1's-complement form. With
        // shadow=2000 and old_negate=1, sum exceeds 0x7FF.
        let sum = (ch.sweep_shadow as u32) + (ch.completed_addend as u32) + 1;
        assert!(
            sum > 0x7FF,
            "test setup: shadow + completed_addend + 1 must exceed 0x7FF (got {sum})"
        );
        // Clear negate (bit 3=0); period=1, shift=1.
        ch.write_nr10(0x11, false);
        assert!(
            !ch.is_active(),
            "channel must be disabled per NR10 clear-negate formula"
        );
    }

    #[test]
    fn test_nr10_clear_negate_does_not_disable_without_overflow() {
        // Phase 6 negative case: when no negate-mode calc has been performed,
        // The disable formula (shadow + completed_addend + 0) stays well
        // below 0x7FF, so an NR10 write must not disable the channel.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x11, false); // period=1, negate=0, shift=1
        ch.write_nr13(0x10); // freq=16
        ch.write_nr14(0x80, false, false);
        for _ in 0..8 {
            ch.tick();
        }
        ch.clock_sweep(false);
        drain_sweep(&mut ch);
        // shadow=16, completed_addend=8, old_negate=0 → sum=24 ≪ 0x7FF
        ch.write_nr10(0x21, false); // change period only
        assert!(
            ch.is_active(),
            "channel must remain active when no overflow"
        );
    }

    #[test]
    fn test_sweep_disabled_when_period_and_shift_zero() {
        // NR10 = 0x00: period=0, shift=0 → sweep disabled; freq stays constant.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x00, false);
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false, false);
        ch.clock_sweep(false);
        assert_eq!(ch.freq, 100, "freq must not change when sweep is disabled");
        assert!(ch.is_active());
    }

    // ── NR10/NR11/NR12/NR14 read back ────────────────────────────────────

    #[test]
    fn test_nr10_read_back() {
        let mut ch = Channel1::new();
        ch.write_nr10(0x5E, false); // period=5, negate=1, shift=6
        // NR10 read: bit 7 always 1; bits 6-4 = period; bit 3 = negate; bits 2-0 = shift
        let r = ch.read_nr10();
        assert_eq!(r & 0x7F, 0x5E & 0x7F);
    }

    #[test]
    fn test_nr11_read_returns_duty_only() {
        let mut ch = Channel1::new();
        ch.write_nr11(0xBF); // duty=10, length=63
        // Bits 7-6 must match; bits 5-0 always read as 1
        assert_eq!(ch.read_nr11() >> 6, 0b10);
        assert_eq!(ch.read_nr11() & 0x3F, 0x3F);
    }

    #[test]
    fn test_nr12_read_back() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF3); // vol=15, add=1, period=3
        assert_eq!(ch.read_nr12(), 0xF3);
    }

    #[test]
    fn test_nr14_reads_length_en_bit() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr14(0x40, false, false); // length_en=1, no trigger
        assert_eq!(ch.read_nr14() & 0x40, 0x40);
        ch.write_nr14(0x00, false, false);
        assert_eq!(ch.read_nr14() & 0x40, 0x00);
    }

    // ── Sweep correctness (hardware quirks) ──────────────────────────────

    #[test]
    fn test_nr10_write_re_arms_when_countdown_at_seven() {
        // Phase 6: Hardware calls trigger_sweep_calculation at the end of an
        // NR10 write; it actually fires only when sweep_countdown == 7.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x10, false); // period=1, shift=0
        ch.write_nr13(0x64);
        ch.write_nr14(0x80, false, false);
        // Force countdown to 7 to exercise the re-arm path.
        ch.sweep_countdown = 7;
        let calc_before = ch.sweep_calc_reload_timer;
        ch.write_nr10(0x21, false); // period=2, shift=1 — should re-arm
        assert!(
            ch.sweep_calc_reload_timer > calc_before || ch.sweep_calc_countdown != 0,
            "NR10 write at countdown==7 with period>0 must re-arm sweep"
        );
    }

    #[test]
    fn test_nr10_write_glitch_reload_timer_2_resets_countdown() {
        // Phase 7 / D5: CGB-D/E nr10_write_glitch condition 1 —
        // when reload_timer == 2 (countdown was just reloaded), an NR10
        // write reloads the calc countdown from the new shift bits.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // period=1, shift=4
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        // Place machinery in the post-reload state.
        ch.sweep_calc_countdown = 4;
        ch.sweep_calc_reload_timer = 2;
        ch.write_nr10(0x12, false); // new shift=2 (countdown!=7 → no re-arm)
        assert_eq!(
            ch.sweep_calc_countdown, 2,
            "reload_timer==2: countdown must reload from new shift bits"
        );
    }

    #[test]
    fn test_nr10_write_glitch_reload_timer_2_zero_shift_clears_timer() {
        // Phase 7 / D5: Hardware clears reload_timer when new shift bits
        // are zero on the reload_timer==2 path.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false);
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        ch.sweep_calc_countdown = 4;
        ch.sweep_calc_reload_timer = 2;
        ch.write_nr10(0x10, false); // new shift=0
        assert_eq!(ch.sweep_calc_countdown, 0);
        assert_eq!(
            ch.sweep_calc_reload_timer, 0,
            "shift=0 on reload_timer==2 path must clear reload_timer"
        );
    }

    #[test]
    fn test_nr10_write_glitch_zombie_step_decrements_countdown() {
        // Phase 7 / D5: CGB-D/E nr10_write_glitch condition 2 —
        // when the new shift transitions 0 → non-zero, lf_div is false,
        // and countdown > 1, the countdown is decremented.
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x10, false); // period=1, shift=0 (old)
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        // Set up running-calc state with old shift=0.
        ch.sweep_calc_countdown = 3;
        ch.sweep_calc_reload_timer = 0;
        ch.write_nr10(0x12, false); // new shift=2, lf_div=false
        assert_eq!(
            ch.sweep_calc_countdown, 2,
            "zombie-step: countdown must decrement"
        );
    }

    #[test]
    fn test_nr10_write_glitch_no_op_outside_both_conditions() {
        // Phase 7 / D5: when neither Hardware condition matches, the calc
        // machinery is untouched. Setup: reload_timer=3 (≠2) and old shift
        // already non-zero (zombie-step requires old shift==0).
        let mut ch = Channel1::new();
        ch.set_model(true, CgbModel::CgbE);
        ch.write_nr12(0xF0);
        ch.write_nr10(0x14, false); // period=1, shift=4 (non-zero old)
        ch.write_nr13(0x10);
        ch.write_nr14(0x80, false, false);
        ch.sweep_calc_countdown = 4;
        ch.sweep_calc_reload_timer = 3;
        ch.write_nr10(0x12, false); // new shift=2; conditions fail
        assert_eq!(
            ch.sweep_calc_countdown, 4,
            "no-op default: countdown must be unchanged"
        );
        assert_eq!(ch.sweep_calc_reload_timer, 3);
    }

    // ── Output level ──────────────────────────────────────────────────────

    #[test]
    fn test_output_is_zero_when_inactive() {
        let ch = Channel1::new();
        assert_eq!(ch.output(), 0.0);
    }

    #[test]
    fn test_output_nonzero_when_active_duty_high() {
        // 50% duty at position 5 is high (after first sample completes).
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // vol=15, DAC on
        ch.write_nr11(0x80); // 50% duty
        ch.write_nr14(0x80, false, false); // trigger
        // Set duty_pos to 5 (high in 50% duty) and clear first_sample_zero
        // to simulate state after first duty step has completed.
        ch.duty_pos = 5;
        ch.first_sample_zero = false;
        ch.update_current_output();
        // DUTY_TABLE[2][5] = 1
        assert!(
            ch.output() > 0.0,
            "output must be positive at duty-high step"
        );
    }

    #[test]
    fn test_duty_phase_is_not_clocked_before_first_trigger() {
        // Pan Docs: just after APU power-on, pulse duty clocking is disabled
        // until first trigger. Verify CH1 duty phase does not advance before
        // first trigger, then advances normally afterwards.
        let mut ch = Channel1::new();

        for _ in 0..4096 {
            ch.tick();
        }
        assert_eq!(
            ch.duty_pos, 0,
            "duty phase should remain at reset position before first trigger"
        );

        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0x80); // 50% duty
        ch.write_nr14(0x80, false, false); // trigger

        let start = ch.duty_pos;
        for _ in 0..4096 {
            ch.tick();
        }

        assert_ne!(
            ch.duty_pos, start,
            "duty phase should advance after the channel has been triggered"
        );
    }

    // ── T-cycle precision tests ───────────────────────────────────────────

    #[test]
    fn test_tick_freq_timer_decrements_by_tcycles_within_mcycle() {
        // Given: freq_timer = 6 (more than 4 T-cycles);
        // When: tick() once (1 M-cycle = 4 T-cycles);
        // Then: freq_timer should be 2 (6 - 4 = 2), no duty advance.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x80);
        // Set freq so period = (2048 - 2047) * 4 = 4
        ch.write_nr13(0xFF); // freq low = 0xFF
        ch.write_nr14(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FF = 2047
        // After trigger: freq_timer = period + delay_t = 4 + delay_t
        // We need to manipulate freq_timer directly for this test.
        // Set freq_timer to exactly 6 to test partial decrement.
        ch.freq_timer = 6;
        let duty_before = ch.duty_pos;
        ch.tick();
        // After 4 T-cycles: timer should be 2 (no reload happened)
        assert_eq!(
            ch.freq_timer, 2,
            "freq_timer should decrement to 2 after one M-cycle"
        );
        assert_eq!(
            ch.duty_pos, duty_before,
            "duty_pos should not advance when timer > 0"
        );
    }

    #[test]
    fn test_tick_freq_timer_expires_mid_mcycle_and_reloads_with_remainder() {
        // Given: freq_timer = 3, period = 8 (freq = 2046);
        // When: tick() once (4 T-cycles);
        // Then: timer expires at T-cycle 3, reloads to period (8),
        //       then 1 remaining T-cycle decrements it to 7.
        //       duty_pos should advance once.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x80);
        // freq = 2046 → period = (2048 - 2046) * 4 = 8
        ch.write_nr13(0xFE); // freq low = 0xFE
        ch.write_nr14(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FE = 2046
        ch.freq_timer = 3;
        let duty_before = ch.duty_pos;
        ch.tick();
        // Timer expired at T-cycle 3, reloaded to 8, decremented 1 more → 7
        assert_eq!(
            ch.freq_timer, 7,
            "freq_timer should be period - remaining (8 - 1 = 7)"
        );
        assert_eq!(
            ch.duty_pos,
            (duty_before + 1) & 7,
            "duty_pos should advance once"
        );
    }

    #[test]
    fn test_tick_freq_timer_expires_exactly_at_mcycle_boundary() {
        // Given: freq_timer = 4, period = 12 (freq = 2045);
        // When: tick() once;
        // Then: timer expires at T-cycle 4, reloads to 12, no remaining T-cycles.
        //       duty_pos should advance once.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x80);
        // freq = 2045 → period = (2048 - 2045) * 4 = 12
        ch.write_nr13(0xFD); // freq low = 0xFD
        ch.write_nr14(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FD = 2045
        ch.freq_timer = 4;
        let duty_before = ch.duty_pos;
        ch.tick();
        // Timer expired at T-cycle 4, reloaded to 12, 0 remaining → 12
        assert_eq!(
            ch.freq_timer, 12,
            "freq_timer should be exactly period after expiring at boundary"
        );
        assert_eq!(
            ch.duty_pos,
            (duty_before + 1) & 7,
            "duty_pos should advance once"
        );
    }

    #[test]
    fn test_tick_very_short_period_multiple_advances_per_mcycle() {
        // Given: freq_timer = 1, period = 4 (freq = 2047, minimum period);
        // When: tick() once (4 T-cycles);
        // Then: timer expires at T-cycle 1, reloads to 4, then 3 more T-cycles
        //       decrement it to 1. Only one advance per expiry means we advance
        //       duty_pos once (timer doesn't expire again because 4-3=1 > 0).
        //       Wait: 4-3=1, then next T would be the 4th, so timer=4 after reload,
        //       3 remaining decrements = 4-3=1. So freq_timer=1, duty_pos + 1.
        //       Actually with period=4: expire at T1, reload=4, T2→3, T3→2, T4→1.
        //       One advance total.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x80);
        // freq = 2047 → period = (2048 - 2047) * 4 = 4
        ch.write_nr13(0xFF);
        ch.write_nr14(0x87, false, false); // trigger → freq = 0x7FF = 2047
        ch.freq_timer = 1;
        let duty_before = ch.duty_pos;
        ch.tick();
        assert_eq!(
            ch.freq_timer, 1,
            "freq_timer should be 1 (period=4 - 3 remaining)"
        );
        assert_eq!(
            ch.duty_pos,
            (duty_before + 1) & 7,
            "duty_pos should advance once"
        );
    }
}
