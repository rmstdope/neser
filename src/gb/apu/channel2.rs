//! CH2 – Pulse channel without sweep (NR21–NR24).
//!
//! Identical to Channel1 in structure except there is no sweep unit (NR10).

use crate::trace_apu;
use serde::{Deserialize, Serialize};

use super::channel1::EnvelopeClockState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel2 {
    duty: u8,
    length_load: u8,
    init_volume: u8,
    env_add: bool,
    env_period: u8,
    freq: u16,
    length_en: bool,

    active: bool,
    dac_on: bool,
    duty_pos: u8,
    freq_timer: u16,
    pub(crate) length_counter: u8,
    volume: u8,
    env_timer: u8,
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
}

impl Default for Channel2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
            duty: 0,
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
            triggered_once: false,
            first_sample_zero: false,
            env_clock_state: EnvelopeClockState::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn length_en(&self) -> bool {
        self.length_en
    }

    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        // First duty step after first trigger post-power-on outputs 0.
        if self.first_sample_zero {
            return 0.0;
        }
        let bit = super::apu::DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        if bit == 1 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }

    /// Digital output (0-15) before DAC conversion (for PCM12 register).
    pub fn digital_output(&self) -> u8 {
        if !self.active || !self.dac_on {
            return 0;
        }
        // First duty step after first trigger post-power-on outputs 0.
        if self.first_sample_zero {
            return 0;
        }
        let bit = super::apu::DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        if bit == 1 { self.volume } else { 0 }
    }

    /// Advance the frequency timer by one M-cycle (= 4 T-cycles).
    ///
    /// Processes each T-cycle individually to maintain sub-M-cycle precision.
    /// When the timer expires mid-M-cycle, the remaining T-cycles are applied
    /// after the reload, ensuring correct phase alignment.
    pub fn tick(&mut self) {
        let period = (2048 - self.freq) * 4;
        if self.freq_timer == 0 {
            self.freq_timer = period;
        }
        for _ in 0..4 {
            self.freq_timer -= 1;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                if self.triggered_once {
                    let old_pos = self.duty_pos;
                    self.duty_pos = (self.duty_pos + 1) & 7;
                    self.first_sample_zero = false;
                    trace_apu!(5; "GB APU CH2 tick duty_pos {} -> {} period=0x{:03X}", old_pos, self.duty_pos, self.freq);
                }
            }
        }
    }

    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        trace_apu!(3; "GB APU CH2 length_counter={} active={}", self.length_counter, self.length_counter > 0);
        if self.length_counter == 0 {
            self.active = false;
        }
    }

    pub fn clock_envelope_decrement(&mut self) {
        if self.env_period == 0 {
            return;
        }
        if self.env_timer > 0 {
            self.env_timer -= 1;
        }
    }

    pub fn clock_envelope_secondary(&mut self) {
        if !self.active || self.env_period == 0 {
            return;
        }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            self.env_clock_state.clock = true;
        }
    }

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
            trace_apu!(3; "GB APU CH2 envelope volume {} -> {}", old_volume, self.volume);
        }
    }

    pub fn clock_envelope(&mut self) {
        self.clock_envelope_decrement();
        self.clock_envelope_secondary();
        self.clock_envelope_primary();
    }

    /// Clear envelope clock flag after frame sequencer step completes.
    pub fn clear_envelope_clock(&mut self) {
        self.env_clock_state.clock = false;
    }

    pub fn power_off(&mut self) {
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
        self.triggered_once = false;
        self.first_sample_zero = false;
        self.env_clock_state = EnvelopeClockState::default();
    }

    // ── Register reads ────────────────────────────────────────────────────

    pub fn read_nr21(&self) -> u8 {
        0x3F | ((self.duty & 0x03) << 6)
    }

    pub fn read_nr22(&self) -> u8 {
        ((self.init_volume & 0x0F) << 4) | (u8::from(self.env_add) << 3) | (self.env_period & 0x07)
    }

    pub fn read_nr24(&self) -> u8 {
        0xBF | (u8::from(self.length_en) << 6)
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr21(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH2 write NR21=0x{:02X} duty={} length={}", val, (val >> 6) & 0x03, val & 0x3F);
        self.duty = (val >> 6) & 0x03;
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr22(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH2 write NR22=0x{:02X} volume={} env_add={} env_period={}", 
            val, (val >> 4) & 0x0F, (val & 0x08) != 0, val & 0x07);

        let old_val = self.read_nr22();

        self.init_volume = (val >> 4) & 0x0F;
        self.env_add = val & 0x08 != 0;
        self.env_period = val & 0x07;
        self.dac_on = val & 0xF8 != 0;

        if !self.dac_on {
            self.active = false;
        } else if self.active {
            // Apply zombie mode glitch when writing NRx2 while channel is active.
            self.apply_nrx2_glitch(old_val, val);
        }
    }

    /// Apply the NRx2 "zombie mode" glitch.
    fn apply_nrx2_glitch(&mut self, old_val: u8, new_val: u8) {
        let old_period = old_val & 0x07;
        let new_period = new_val & 0x07;
        let old_direction_add = (old_val & 0x08) != 0;
        let new_direction_add = (new_val & 0x08) != 0;

        if self.env_clock_state.clock {
            self.env_timer = new_period;
        }

        let mut should_tick =
            (new_period != 0) && (old_period == 0) && !self.env_clock_state.locked;

        if (new_val & 0x0F) == 0x08 && (old_val & 0x0F) == 0x08 && !self.env_clock_state.locked {
            should_tick = true;
        }

        let should_invert = old_direction_add != new_direction_add;

        if should_invert {
            let old_volume = self.volume;
            if new_direction_add {
                if old_period == 0 && !self.env_clock_state.locked {
                    self.volume ^= 0x0F;
                } else {
                    self.volume = (0x0E_u8.wrapping_sub(self.volume)) & 0x0F;
                }
                should_tick = false;
            } else {
                self.volume = (0x10_u8.wrapping_sub(self.volume)) & 0x0F;
            }
            trace_apu!(3; "GB APU CH2 zombie invert volume {} -> {}", old_volume, self.volume);
        }

        if should_tick {
            let old_volume = self.volume;
            if new_direction_add {
                self.volume = (self.volume + 1) & 0x0F;
            } else {
                self.volume = self.volume.wrapping_sub(1) & 0x0F;
            }
            trace_apu!(3; "GB APU CH2 zombie tick volume {} -> {}", old_volume, self.volume);
        } else if new_period == 0 && self.env_clock_state.clock {
            self.env_clock_state.clock = false;
        }
    }

    pub fn write_nr23(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
        trace_apu!(2; "GB APU CH2 write NR23=0x{:02X} freq=0x{:03X}", val, self.freq);
    }

    pub fn write_nr24(&mut self, val: u8, extra_clk: bool, lf_div: bool) {
        trace_apu!(2; "GB APU CH2 write NR24=0x{:02X} trigger={} length_en={} freq_high={}", 
            val, (val & 0x80) != 0, (val & 0x40) != 0, val & 0x07);
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);

        if extra_clk && !old_length_en && self.length_en && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.active = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(lf_div);
            if extra_clk && self.length_en && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    pub fn write_nr21_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    fn trigger(&mut self, lf_div: bool) {
        trace_apu!(1; "GB APU CH2 trigger freq=0x{:03X} volume={} lf_div={}", self.freq, self.init_volume, lf_div);
        let was_active = self.active;
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
        // Note: lf_div=true means the DIV rising-edge tick fired this M-cycle, which adds
        // 2 extra T-cycles to the delay (longer, not shorter).
        let delay_t = if was_active {
            // Retrigger delay: 1 2MHz tick (2 T-cycles) shorter than fresh
            if lf_div { 6u16 } else { 4u16 }
        } else if lf_div {
            8u16
        } else {
            6u16
        };
        // Convert delay to T-cycles and add to period for initial freq_timer
        let period = (2048 - self.freq) * 4;
        self.freq_timer = period + delay_t;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch2() -> Channel2 {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0); // DAC on, vol=15
        ch.write_nr21(0x80); // 50% duty
        ch.write_nr24(0x80, false, false); // trigger
        ch
    }

    #[test]
    fn test_trigger_makes_channel_active() {
        let ch = triggered_ch2();
        assert!(ch.is_active());
    }

    #[test]
    fn test_dac_off_prevents_activation() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x00);
        ch.write_nr24(0x80, false, false);
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_expiry_silences_when_enabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF); // counter = 1
        ch.write_nr24(0xC0, false, false); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_no_expire_when_disabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF);
        ch.write_nr24(0x80, false, false);
        ch.clock_length();
        assert!(ch.is_active());
    }

    #[test]
    fn test_envelope_decrements_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x71); // vol=7, sub, period=1
        ch.write_nr24(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x79); // vol=7, add, period=1
        ch.write_nr24(0x80, false, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 8);
    }

    #[test]
    fn test_nr21_read_duty_bits() {
        let mut ch = Channel2::new();
        ch.write_nr21(0xC0); // duty=11
        assert_eq!(ch.read_nr21() >> 6, 0b11);
    }

    #[test]
    fn test_nr22_read_back() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF3);
        assert_eq!(ch.read_nr22(), 0xF3);
    }

    #[test]
    fn test_nr24_length_en_readable() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr24(0x40, false, false);
        assert_eq!(ch.read_nr24() & 0x40, 0x40);
    }

    #[test]
    fn test_output_zero_when_inactive() {
        let ch = Channel2::new();
        assert_eq!(ch.output(), 0.0);
    }

    #[test]
    fn test_duty_phase_is_not_clocked_before_first_trigger() {
        // Pan Docs: "duty cycle clocking is disabled until the first trigger"
        // applies to both CH1 and CH2.
        let mut ch = Channel2::new();
        for _ in 0..4096 {
            ch.tick();
        }
        assert_eq!(
            ch.duty_pos, 0,
            "duty phase should remain at reset position before first trigger"
        );

        ch.write_nr22(0xF0); // DAC on
        ch.write_nr21(0x80); // 50% duty
        ch.write_nr24(0x80, false, false); // trigger

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
        // Given: freq_timer = 6;
        // When: tick() once (1 M-cycle = 4 T-cycles);
        // Then: freq_timer should be 2, no duty advance.
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0x80);
        // freq = 2047 → period = 4
        ch.write_nr23(0xFF);
        ch.write_nr24(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FF
        ch.freq_timer = 6;
        let duty_before = ch.duty_pos;
        ch.tick();
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
        // Then: timer expires at T-cycle 3, reloads to 8,
        //       then 1 remaining T-cycle decrements to 7.
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0x80);
        // freq = 2046 → period = (2048 - 2046) * 4 = 8
        ch.write_nr23(0xFE);
        ch.write_nr24(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FE
        ch.freq_timer = 3;
        let duty_before = ch.duty_pos;
        ch.tick();
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
        // Then: timer expires at T-cycle 4, reloads to 12.
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0x80);
        // freq = 2045 → period = (2048 - 2045) * 4 = 12
        ch.write_nr23(0xFD);
        ch.write_nr24(0x87, false, false); // trigger, freq high = 7 → freq = 0x7FD
        ch.freq_timer = 4;
        let duty_before = ch.duty_pos;
        ch.tick();
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
}
