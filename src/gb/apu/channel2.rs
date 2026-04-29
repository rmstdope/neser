//! CH2 – Pulse channel without sweep (NR21–NR24).
//!
//! Identical to Channel1 in structure except there is no sweep unit (NR10).

use crate::trace_apu;
use serde::{Deserialize, Serialize};

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
        if self.env_period == 0 {
            return;
        }
        if self.env_timer > 0 {
            self.env_timer -= 1;
        }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            let old_volume = self.volume;
            if self.env_add && self.volume < 15 {
                self.volume += 1;
            } else if !self.env_add && self.volume > 0 {
                self.volume -= 1;
            }
            if old_volume != self.volume {
                trace_apu!(3; "GB APU CH2 envelope volume {} -> {}", old_volume, self.volume);
            }
        }
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
        self.init_volume = (val >> 4) & 0x0F;
        self.env_add = val & 0x08 != 0;
        self.env_period = val & 0x07;
        self.dac_on = val & 0xF8 != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    pub fn write_nr23(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
        trace_apu!(2; "GB APU CH2 write NR23=0x{:02X} freq=0x{:03X}", val, self.freq);
    }

    pub fn write_nr24(&mut self, val: u8, extra_clk: bool) {
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
            self.trigger();
            if extra_clk && self.length_en && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    pub fn write_nr21_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    fn trigger(&mut self) {
        trace_apu!(1; "GB APU CH2 trigger freq=0x{:03X} volume={}", self.freq, self.init_volume);
        self.triggered_once = true;
        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.freq_timer = (2048 - self.freq) * 4;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
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
        ch.write_nr24(0x80, false); // trigger
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
        ch.write_nr24(0x80, false);
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_expiry_silences_when_enabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF); // counter = 1
        ch.write_nr24(0xC0, false); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_no_expire_when_disabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF);
        ch.write_nr24(0x80, false);
        ch.clock_length();
        assert!(ch.is_active());
    }

    #[test]
    fn test_envelope_decrements_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x71); // vol=7, sub, period=1
        ch.write_nr24(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x79); // vol=7, add, period=1
        ch.write_nr24(0x80, false);
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
        ch.write_nr24(0x40, false);
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
        ch.write_nr24(0x80, false); // trigger

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
