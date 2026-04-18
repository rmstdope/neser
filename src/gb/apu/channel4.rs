//! CH4 – Noise channel with LFSR (NR41–NR44).
//!
//! LFSR clock: `f = 524_288 / divisor / 2^(shift+1)` Hz.
//! 7-bit mode: feedback is also written to bit 6, shortening the period.

/// Divisor lookup for noise clock (NR43 bits 2-0).
use serde::{Deserialize, Serialize};

const DIVISORS: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

#[derive(Serialize, Deserialize)]
pub struct Channel4 {
    init_volume: u8,
    env_add: bool,
    env_period: u8,
    clock_shift: u8,
    lfsr_7bit: bool,
    divisor_code: u8,
    length_load: u8,
    length_en: bool,
    active: bool,
    dac_on: bool,
    lfsr: u16,
    freq_timer: u32,
    pub(crate) length_counter: u8,
    volume: u8,
    env_timer: u8,
}

impl Default for Channel4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel4 {
    pub fn new() -> Self {
        Self {
            init_volume: 0,
            env_add: false,
            env_period: 0,
            clock_shift: 0,
            lfsr_7bit: false,
            divisor_code: 0,
            length_load: 0,
            length_en: false,
            active: false,
            dac_on: false,
            lfsr: 0x7FFF,
            freq_timer: 0,
            length_counter: 0,
            volume: 0,
            env_timer: 0,
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
        // LFSR bit 0 low = channel output high.
        if self.lfsr & 0x01 == 0 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }

    fn freq_timer_period(&self) -> u32 {
        DIVISORS[self.divisor_code as usize] << self.clock_shift
    }

    pub fn tick(&mut self) {
        if self.freq_timer == 0 {
            self.freq_timer = self.freq_timer_period();
        }
        if self.freq_timer > 4 {
            self.freq_timer -= 4;
        } else {
            self.freq_timer = self.freq_timer_period();
            self.clock_lfsr();
        }
    }

    /// Clock the LFSR (exposed for testing).
    pub fn clock_lfsr(&mut self) {
        let xor = (self.lfsr & 0x01) ^ ((self.lfsr >> 1) & 0x01);
        self.lfsr >>= 1;
        self.lfsr |= xor << 14;
        if self.lfsr_7bit {
            self.lfsr &= !(1 << 6);
            self.lfsr |= xor << 6;
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
        self.init_volume = 0;
        self.env_add = false;
        self.env_period = 0;
        self.clock_shift = 0;
        self.lfsr_7bit = false;
        self.divisor_code = 0;
        self.length_load = 0;
        self.length_en = false;
        self.active = false;
        self.dac_on = false;
        self.freq_timer = 0;
        self.length_counter = 0;
        self.volume = 0;
        self.env_timer = 0;
    }

    pub fn read_nr42(&self) -> u8 {
        ((self.init_volume & 0x0F) << 4) | (u8::from(self.env_add) << 3) | (self.env_period & 0x07)
    }

    pub fn read_nr43(&self) -> u8 {
        ((self.clock_shift & 0x0F) << 4)
            | (u8::from(self.lfsr_7bit) << 3)
            | (self.divisor_code & 0x07)
    }

    pub fn read_nr44(&self) -> u8 {
        0xBF | (u8::from(self.length_en) << 6)
    }

    pub fn write_nr41(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr42(&mut self, val: u8) {
        self.init_volume = (val >> 4) & 0x0F;
        self.env_add = val & 0x08 != 0;
        self.env_period = val & 0x07;
        self.dac_on = val & 0xF8 != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    pub fn write_nr43(&mut self, val: u8) {
        self.clock_shift = (val >> 4) & 0x0F;
        self.lfsr_7bit = val & 0x08 != 0;
        self.divisor_code = val & 0x07;
    }

    pub fn write_nr44(&mut self, val: u8, extra_clk: bool) {
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;

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

    pub fn write_nr41_length_only(&mut self, val: u8) {
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
        self.freq_timer = self.freq_timer_period();
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        self.lfsr = 0x7FFF;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch4() -> Channel4 {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF1); // vol=15, sub, period=1, DAC on
        ch.write_nr44(0x80, false); // trigger
        ch
    }

    #[test]
    fn test_trigger_makes_channel_active() {
        assert!(triggered_ch4().is_active());
    }

    #[test]
    fn test_dac_off_prevents_activation() {
        let mut ch = Channel4::new();
        ch.write_nr42(0x00);
        ch.write_nr44(0x80, false);
        assert!(!ch.is_active());
    }

    #[test]
    fn test_trigger_resets_lfsr_to_7fff() {
        assert_eq!(triggered_ch4().lfsr, 0x7FFF);
    }

    #[test]
    fn test_length_expiry_silences_when_enabled() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF1);
        ch.write_nr41(0x3F); // counter = 1
        ch.write_nr44(0xC0, false); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_no_expire_when_disabled() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF1);
        ch.write_nr41(0x3F);
        ch.write_nr44(0x80, false);
        ch.clock_length();
        assert!(ch.is_active());
    }

    #[test]
    fn test_envelope_decrements_volume() {
        let mut ch = Channel4::new();
        ch.write_nr42(0x71); // vol=7, sub, period=1
        ch.write_nr44(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        let mut ch = Channel4::new();
        ch.write_nr42(0x79); // vol=7, add, period=1
        ch.write_nr44(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 8);
    }

    #[test]
    fn test_15bit_lfsr_one_clock() {
        // LFSR = 0x7FFF: bit0=1, bit1=1, xor=0 -> shifted right = 0x3FFF, bit14=0.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // 15-bit
        ch.write_nr44(0x80, false); // trigger -> LFSR = 0x7FFF
        ch.clock_lfsr();
        assert_eq!(ch.lfsr, 0x3FFF);
    }

    #[test]
    fn test_7bit_lfsr_sets_bit6_to_xor() {
        // LFSR = 0x7FFF: xor=0 -> bit6 forced to 0 after clock.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x08); // 7-bit
        ch.write_nr44(0x80, false);
        ch.clock_lfsr();
        assert_eq!(ch.lfsr & (1 << 6), 0);
    }

    #[test]
    fn test_15bit_and_7bit_produce_different_patterns() {
        let mut ch15 = Channel4::new();
        ch15.write_nr42(0xF0);
        ch15.write_nr43(0x00);
        ch15.write_nr44(0x80, false);

        let mut ch7 = Channel4::new();
        ch7.write_nr42(0xF0);
        ch7.write_nr43(0x08);
        ch7.write_nr44(0x80, false);

        let bits15: Vec<u8> = (0..32)
            .map(|_| {
                ch15.clock_lfsr();
                (ch15.lfsr & 1) as u8
            })
            .collect();
        let bits7: Vec<u8> = (0..32)
            .map(|_| {
                ch7.clock_lfsr();
                (ch7.lfsr & 1) as u8
            })
            .collect();
        assert_ne!(bits15, bits7);
    }

    #[test]
    fn test_nr42_read_back() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF3);
        assert_eq!(ch.read_nr42(), 0xF3);
    }

    #[test]
    fn test_nr43_read_back() {
        let mut ch = Channel4::new();
        ch.write_nr43(0xAB);
        assert_eq!(ch.read_nr43(), 0xAB);
    }

    #[test]
    fn test_nr44_reads_length_en() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr44(0x40, false);
        assert_eq!(ch.read_nr44() & 0x40, 0x40);
    }

    #[test]
    fn test_output_zero_when_inactive() {
        assert_eq!(Channel4::new().output(), 0.0);
    }
}
