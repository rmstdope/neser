use super::apu::DUTY_TABLE;

// ── CH2 — Pulse (no sweep) ────────────────────────────────────────────────────

/// Channel 2: pulse wave without sweep.
#[derive(Debug, Clone, Default)]
pub struct Channel2 {
    pub duty: u8,
    pub length_counter: u16,
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,
    pub freq: u16,
    pub length_en: bool,

    pub active: bool,
    pub dac_on: bool,
    pub duty_pos: u8,
    pub freq_timer: u32,
    pub volume: u8,
    pub env_timer: u8,
}

impl Channel2 {
    /// Analogue output in `[-1.0, +1.0]`; 0.0 when inactive or DAC is off
    /// (disconnected).
    ///
    /// Per GBATek, the PSG DAC converts digital value D (0–15) to bipolar:
    ///   output = (D / 7.5) − 1.0
    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        let bit = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        let d = if bit == 1 { self.volume as f32 } else { 0.0 };
        d / 7.5 - 1.0
    }

    pub fn tick(&mut self, cycles: u32) {
        if !self.active {
            return;
        }
        let period = (2048_u32.wrapping_sub(self.freq as u32)) * 16;
        if period == 0 {
            return;
        }
        let mut rem = cycles;
        while rem > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = period;
            }
            let advance = rem.min(self.freq_timer);
            self.freq_timer -= advance;
            rem -= advance;
            if self.freq_timer == 0 {
                self.duty_pos = (self.duty_pos + 1) & 7;
                self.freq_timer = period;
            }
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

    fn trigger(&mut self) {
        self.active = self.dac_on;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        let period = (2048_u32.wrapping_sub(self.freq as u32)) * 16;
        self.freq_timer = period;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
    }

    // ── Register writes ───────────────────────────────────────────────────

    /// SOUND2CNT_L: duty/length/envelope.
    pub fn write_cnt_l(&mut self, val: u16) {
        self.length_counter = 64 - (val & 0x3F);
        self.duty = ((val >> 6) & 0x03) as u8;
        self.env_period = ((val >> 8) & 0x07) as u8;
        self.env_add = (val & 0x0800) != 0;
        self.init_volume = ((val >> 12) & 0x0F) as u8;
        self.dac_on = (val & 0xF800) != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    /// SOUND2CNT_H: frequency and trigger.
    ///
    /// `extra_clk` — see Channel1::write_cnt_x for the full description.
    pub fn write_cnt_h(&mut self, val: u16, extra_clk: bool) {
        self.freq = val & 0x7FF;
        let old_length_en = self.length_en;
        self.length_en = (val & 0x4000) != 0;
        // Extra clock when enabling length_en (0→1) and next FS step won't clock.
        if extra_clk && !old_length_en && self.length_en && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.active = false;
            }
        }
        if val & 0x8000 != 0 {
            let reloaded_length = self.length_counter == 0;
            self.trigger();
            if extra_clk && self.length_en && reloaded_length {
                self.length_counter = 63;
            }
        }
    }

    pub fn power_off(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_ch2(volume: u8, duty: u8, duty_pos: u8) -> Channel2 {
        Channel2 {
            active: true,
            dac_on: true,
            volume,
            duty,
            duty_pos,
            ..Channel2::default()
        }
    }

    #[test]
    fn test_ch2_dac_off_outputs_zero() {
        let ch2 = Channel2 {
            dac_on: false,
            active: false,
            ..Channel2::default()
        };
        assert_eq!(ch2.output(), 0.0);
    }

    #[test]
    fn test_ch2_duty_low_outputs_minus_one() {
        // Duty=0 (12.5%) at pos 0 has bit=0 → D=0 → output = -1.0.
        let ch2 = active_ch2(15, 0, 0);
        let got = ch2.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "duty low must produce -1.0, got {got}"
        );
    }

    #[test]
    fn test_ch2_duty_high_full_volume_outputs_plus_one() {
        // Duty=2 (50%) at pos 0 has bit=1, volume=15 → D=15 → output = +1.0.
        let ch2 = active_ch2(15, 2, 0);
        let got = ch2.output();
        assert!(
            (got - 1.0_f32).abs() < 1e-5,
            "volume=15 duty high must produce +1.0, got {got}"
        );
    }

    #[test]
    fn test_ch2_duty_high_half_volume_is_bipolar() {
        // Volume=8, duty high → output = 8/7.5 - 1.0
        let ch2 = active_ch2(8, 2, 0);
        let expected = 8.0_f32 / 7.5 - 1.0;
        let got = ch2.output();
        assert!(
            (got - expected).abs() < 1e-5,
            "volume=8 duty high: expected {expected}, got {got}"
        );
    }
}
