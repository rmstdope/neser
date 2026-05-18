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
    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        let bit = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        if bit == 1 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
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
    pub fn write_cnt_h(&mut self, val: u16) {
        self.freq = val & 0x7FF;
        self.length_en = (val & 0x4000) != 0;
        if val & 0x8000 != 0 {
            self.trigger();
        }
    }

    pub fn power_off(&mut self) {
        *self = Self::default();
    }
}
