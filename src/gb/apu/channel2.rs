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
    /// Envelope clock state for zombie mode glitch tracking.
    #[serde(default)]
    env_clock_state: EnvelopeClockState,
    /// Startup delay counter. When non-zero, the channel is active but the
    /// frequency timer doesn't tick and duty position doesn't advance.
    #[serde(default)]
    startup_delay: u8,
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
            env_clock_state: EnvelopeClockState::default(),
            startup_delay: 0,
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
        let bit = super::apu::DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        if bit == 1 { self.volume } else { 0 }
    }

    pub fn tick(&mut self) {
        let period = (2048 - self.freq) * 4;
        if self.freq_timer == 0 {
            self.freq_timer = period;
        }
        if self.freq_timer > 4 {
            self.freq_timer -= 4;
        } else {
            self.freq_timer = period;
            if self.triggered_once {
                let old_pos = self.duty_pos;
                self.duty_pos = (self.duty_pos + 1) & 7;
                trace_apu!(5; "GB APU CH2 tick duty_pos {} -> {} period=0x{:03X}", old_pos, self.duty_pos, self.freq);
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

    pub fn clock_envelope(&mut self) {
        // Clear the clock flag from any previous tick.
        self.env_clock_state.clock = false;

        if self.env_period == 0 {
            return;
        }
        if self.env_timer > 0 {
            self.env_timer -= 1;
        }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            // Set clock state to indicate envelope just ticked.
            self.env_clock_state.clock = true;

            if self.env_clock_state.locked {
                // Envelope is locked - no volume change.
                return;
            }

            let old_volume = self.volume;
            if self.env_add && self.volume < 15 {
                self.volume += 1;
            } else if !self.env_add && self.volume > 0 {
                self.volume -= 1;
            }

            // Lock envelope if volume has hit its limit.
            if (self.env_add && self.volume == 15) || (!self.env_add && self.volume == 0) {
                self.env_clock_state.locked = true;
            }

            if old_volume != self.volume {
                trace_apu!(3; "GB APU CH2 envelope volume {} -> {}", old_volume, self.volume);
            }
        }
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
        self.env_clock_state = EnvelopeClockState::default();
        self.startup_delay = 0;
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
        let delay_t = if was_active {
            // Retrigger delay: 1 2MHz tick (2 T-cycles) shorter than fresh
            if lf_div { 4u16 } else { 6u16 }
        } else if lf_div {
            6u16
        } else {
            8u16
        };
        // Convert delay to T-cycles and add to period for initial freq_timer
        let period = (2048 - self.freq) * 4;
        self.freq_timer = period + delay_t;
        self.startup_delay = 0; // Not used with this approach
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
}
