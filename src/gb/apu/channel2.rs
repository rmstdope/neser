//! CH2 – Pulse channel without sweep (NR21–NR24).
//!
//! Identical to Channel1 in structure except there is no sweep unit (NR10).

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
    length_counter: u8,
    volume: u8,
    env_timer: u8,
}

impl Default for Channel2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
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
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        let bit = super::DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        if bit == 1 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }

    pub fn tick(&mut self) {
        if self.freq_timer == 0 {
            self.freq_timer = (2048 - self.freq) * 4;
        }
        if self.freq_timer > 4 {
            self.freq_timer -= 4;
        } else {
            self.freq_timer = (2048 - self.freq) * 4;
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
    }

    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
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
            if self.env_add && self.volume < 15 {
                self.volume += 1;
            } else if !self.env_add && self.volume > 0 {
                self.volume -= 1;
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
    }

    // ── Register reads ────────────────────────────────────────────────────

    pub fn read_nr21(&self) -> u8 {
        0x3F | ((self.duty & 0x03) << 6)
    }

    pub fn read_nr22(&self) -> u8 {
        ((self.init_volume & 0x0F) << 4)
            | (if self.env_add { 0x08 } else { 0x00 })
            | (self.env_period & 0x07)
    }

    pub fn read_nr24(&self) -> u8 {
        0xBF | (if self.length_en { 0x40 } else { 0x00 })
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr21(&mut self, val: u8) {
        self.duty = (val >> 6) & 0x03;
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr22(&mut self, val: u8) {
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
    }

    pub fn write_nr24(&mut self, val: u8) {
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);
        if val & 0x80 != 0 {
            self.trigger();
        }
    }

    pub fn write_nr21_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    fn trigger(&mut self) {
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
        ch.write_nr24(0x80); // trigger
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
        ch.write_nr24(0x80);
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_expiry_silences_when_enabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF); // counter = 1
        ch.write_nr24(0xC0); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_counter_no_expire_when_disabled() {
        let mut ch = Channel2::new();
        ch.write_nr22(0xF0);
        ch.write_nr21(0xFF);
        ch.write_nr24(0x80);
        ch.clock_length();
        assert!(ch.is_active());
    }

    #[test]
    fn test_envelope_decrements_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x71); // vol=7, sub, period=1
        ch.write_nr24(0x80);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        let mut ch = Channel2::new();
        ch.write_nr22(0x79); // vol=7, add, period=1
        ch.write_nr24(0x80);
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
        ch.write_nr24(0x40);
        assert_eq!(ch.read_nr24() & 0x40, 0x40);
    }

    #[test]
    fn test_output_zero_when_inactive() {
        let ch = Channel2::new();
        assert_eq!(ch.output(), 0.0);
    }
}
