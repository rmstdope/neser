// ── CH4 — Noise ───────────────────────────────────────────────────────────────

/// CH4 noise-clock divisors (in GB T-cycles; ×4 gives GBA cycles).
pub(super) const DIVISORS: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

/// Channel 4: noise generator (15-bit or 7-bit LFSR).
#[derive(Debug, Clone)]
pub struct Channel4 {
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,
    pub clock_shift: u8,
    pub lfsr_7bit: bool,
    pub divisor_code: u8,
    pub length_counter: u16,
    pub length_en: bool,

    pub active: bool,
    pub dac_on: bool,
    pub lfsr: u16,
    pub freq_timer: u32,
    pub volume: u8,
    pub env_timer: u8,
    /// Output level set by the last LFSR clock carry: true=HIGH, false=LOW.
    /// Per GBATek: carry=1 → OUT=HIGH; carry=0 → OUT=LOW.
    pub output_level: bool,
}

impl Default for Channel4 {
    fn default() -> Self {
        Self {
            init_volume: 0,
            env_add: false,
            env_period: 0,
            clock_shift: 0,
            lfsr_7bit: false,
            divisor_code: 0,
            length_counter: 0,
            length_en: false,
            active: false,
            dac_on: false,
            // GBATek: 15-bit mode initial value is 0x4000.
            lfsr: 0x4000,
            freq_timer: 0,
            volume: 0,
            env_timer: 0,
            output_level: false,
        }
    }
}

impl Channel4 {
    /// Analogue output in `[-1.0, +1.0]`; 0.0 when inactive or DAC is off
    /// (disconnected).
    ///
    /// Per GBATek, the PSG DAC converts digital value D (0–15) to bipolar:
    ///   output = (D / 7.5) − 1.0
    /// The output level is set by the carry of the last LFSR clock:
    ///   carry=1 (OUT=HIGH) → D = volume; carry=0 (OUT=LOW) → D = 0.
    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        // Per GBATek: carry=1 → OUT=HIGH (D=volume); carry=0 → OUT=LOW (D=0).
        let d = if self.output_level {
            self.volume as f32
        } else {
            0.0
        };
        d / 7.5 - 1.0
    }

    fn freq_period(&self) -> u32 {
        DIVISORS[self.divisor_code as usize] * (1_u32 << self.clock_shift) * 4
    }

    /// Advance channel by `cycles` GBA cycles.
    pub fn tick(&mut self, cycles: u32) {
        if !self.active {
            return;
        }
        let period = self.freq_period();
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
                self.clock_lfsr();
                self.freq_timer = period;
            }
        }
    }

    /// Clock the LFSR (exposed for testing).
    ///
    /// Implements the GBATek Galois LFSR form:
    ///   15-bit: X = X SHR 1; IF carry THEN Out=HIGH, X = X XOR 6000h ELSE Out=LOW
    ///   7-bit:  X = X SHR 1; IF carry THEN Out=HIGH, X = X XOR 0060h ELSE Out=LOW
    pub fn clock_lfsr(&mut self) {
        let carry = self.lfsr & 1;
        self.output_level = carry != 0;
        self.lfsr >>= 1;
        if carry != 0 {
            self.lfsr ^= if self.lfsr_7bit { 0x0060 } else { 0x6000 };
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
        let reload = if self.env_period > 0 {
            self.env_period
        } else {
            8
        };
        if self.env_timer > 0 {
            self.env_timer -= 1;
        }
        if self.env_timer == 0 {
            self.env_timer = reload;
            if self.env_period > 0 {
                if self.env_add && self.volume < 15 {
                    self.volume += 1;
                } else if !self.env_add && self.volume > 0 {
                    self.volume -= 1;
                }
            }
        }
    }

    fn trigger(&mut self) {
        self.active = self.dac_on;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.freq_timer = self.freq_period();
        self.volume = self.init_volume;
        self.env_timer = if self.env_period > 0 {
            self.env_period
        } else {
            8
        };
        // GBATek: 15-bit mode initial value 0x4000; 7-bit mode initial value 0x0040.
        self.lfsr = if self.lfsr_7bit { 0x0040 } else { 0x4000 };
        self.output_level = false;
    }

    // ── Register writes ───────────────────────────────────────────────────

    /// SOUND4CNT_L: length (5-0), envelope (10-8, 11, 15-12).
    pub fn write_cnt_l(&mut self, val: u16) {
        self.length_counter = 64 - (val & 0x3F);
        self.env_period = ((val >> 8) & 0x07) as u8;
        self.env_add = (val & 0x0800) != 0;
        self.init_volume = ((val >> 12) & 0x0F) as u8;
        self.dac_on = (val & 0xF800) != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    /// SOUND4CNT_H: clock (7-4 shift, 3 LFSR mode, 2-0 divisor), 14 length-en, 15 trigger.
    ///
    /// `extra_clk` — see Channel1::write_cnt_x for the full description.
    pub fn write_cnt_h(&mut self, val: u16, extra_clk: bool) {
        self.divisor_code = (val & 0x07) as u8;
        self.lfsr_7bit = (val & 0x08) != 0;
        self.clock_shift = ((val >> 4) & 0x0F) as u8;
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

    // ── Bipolar output formula tests ──────────────────────────────────────────

    fn active_ch4_low(volume: u8) -> Channel4 {
        Channel4 {
            active: true,
            dac_on: true,
            volume,
            output_level: false,
            ..Channel4::default()
        }
    }

    fn active_ch4_high(volume: u8) -> Channel4 {
        Channel4 {
            active: true,
            dac_on: true,
            volume,
            output_level: true,
            ..Channel4::default()
        }
    }

    #[test]
    fn test_ch4_dac_off_outputs_zero() {
        let ch4 = Channel4 {
            dac_on: false,
            active: false,
            ..Channel4::default()
        };
        assert_eq!(ch4.output(), 0.0);
    }

    #[test]
    fn test_ch4_output_level_false_outputs_minus_one() {
        // output_level=false (LOW) → D=0 → bipolar output = 0/7.5 - 1.0 = -1.0.
        let ch4 = active_ch4_low(15);
        let got = ch4.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "output_level=false must produce -1.0, got {got}"
        );
    }

    #[test]
    fn test_ch4_output_level_true_full_volume_outputs_plus_one() {
        // output_level=true (HIGH), volume=15, D=15 → bipolar output = +1.0.
        let ch4 = active_ch4_high(15);
        let got = ch4.output();
        assert!(
            (got - 1.0_f32).abs() < 1e-5,
            "output_level=true volume=15 must produce +1.0, got {got}"
        );
    }

    #[test]
    fn test_ch4_output_level_true_half_volume_is_bipolar() {
        // output_level=true, volume=8 → D=8, output = 8/7.5 - 1.0
        let ch4 = active_ch4_high(8);
        let expected = 8.0_f32 / 7.5 - 1.0;
        let got = ch4.output();
        assert!(
            (got - expected).abs() < 1e-5,
            "output_level=true volume=8: expected {expected}, got {got}"
        );
    }

    // ── Carry-based output tests (RED → GREEN) ────────────────────────────────

    #[test]
    fn test_ch4_output_uses_carry_not_lfsr_bit0() {
        // Per GBATek: output is HIGH when carry=1, LOW when carry=0.
        // lfsr=0x4000 → bit0=0 → carry=0 → output must be LOW (-1.0) after clock.
        let mut ch4 = Channel4 {
            active: true,
            dac_on: true,
            volume: 15,
            lfsr: 0x4000,
            ..Channel4::default()
        };
        ch4.clock_lfsr();
        let got = ch4.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "After carry=0 clock (lfsr=0x4000), output must be LOW (-1.0), got {got}"
        );
    }

    #[test]
    fn test_ch4_output_high_after_carry_one_clock() {
        // lfsr=0x0001 → bit0=1 → carry=1 → output must be HIGH (+1.0) after clock.
        let mut ch4 = Channel4 {
            active: true,
            dac_on: true,
            volume: 15,
            lfsr: 0x0001,
            ..Channel4::default()
        };
        ch4.clock_lfsr();
        let got = ch4.output();
        assert!(
            (got - 1.0_f32).abs() < 1e-5,
            "After carry=1 clock (lfsr=0x0001), output must be HIGH (+1.0), got {got}"
        );
    }

    // ── LFSR tests ────────────────────────────────────────────────────────────

    #[test]
    fn test_ch4_lfsr_galois_no_carry_step() {
        // RED: Galois clock step when carry=0: only right shift, no XOR.
        // Starting from 0x2000 (bit 13 set, bit 0 = 0 → carry = 0).
        let mut ch4 = Channel4 {
            lfsr: 0x2000,
            ..Channel4::default()
        };
        ch4.clock_lfsr();
        assert_eq!(ch4.lfsr, 0x1000, "Carry=0: should just shift right");
    }

    #[test]
    fn test_ch4_lfsr_galois_carry_15bit() {
        // RED: Galois clock step with carry=1 in 15-bit mode.
        // State 0x0001: carry=1, shift → 0x0000, XOR 0x6000 → 0x6000.
        let mut ch4 = Channel4 {
            lfsr: 0x0001,
            lfsr_7bit: false,
            ..Channel4::default()
        };
        ch4.clock_lfsr();
        assert_eq!(
            ch4.lfsr, 0x6000,
            "15-bit carry=1: 0x0001 >> 1 = 0x0000, XOR 0x6000 = 0x6000"
        );
    }

    #[test]
    fn test_ch4_lfsr_galois_carry_7bit() {
        // RED: Galois clock step with carry=1 in 7-bit mode.
        // State 0x0001: carry=1, shift → 0x0000, XOR 0x0060 → 0x0060.
        let mut ch4 = Channel4 {
            lfsr: 0x0001,
            lfsr_7bit: true,
            ..Channel4::default()
        };
        ch4.clock_lfsr();
        assert_eq!(
            ch4.lfsr, 0x0060,
            "7-bit carry=1: 0x0001 >> 1 = 0x0000, XOR 0x0060 = 0x0060"
        );
    }

    #[test]
    fn test_ch4_lfsr_sequence_7bit() {
        let mut ch4 = Channel4 {
            dac_on: true,
            lfsr_7bit: true,
            lfsr: 0x0040,
            ..Channel4::default()
        };

        // Clock 7-bit LFSR manually using GBATek Galois form and verify.
        let mut lfsr = 0x0040_u16;
        fn clock_7bit(lfsr: &mut u16) {
            let carry = *lfsr & 1;
            *lfsr >>= 1;
            if carry != 0 {
                *lfsr ^= 0x0060;
            }
        }

        for _ in 0..16 {
            clock_7bit(&mut lfsr);
            ch4.clock_lfsr();
            assert_eq!(
                ch4.lfsr, lfsr,
                "7-bit LFSR mismatch: expected 0x{lfsr:04X}, got 0x{:04X}",
                ch4.lfsr
            );
        }
    }

    #[test]
    fn test_ch4_lfsr_15bit_maximal_length() {
        // Verify the 15-bit Galois LFSR has maximum period of 32767 (= 2^15 - 1).
        let mut lfsr = 0x4000_u16;
        for _ in 0..32767 {
            let carry = lfsr & 1;
            lfsr >>= 1;
            if carry != 0 {
                lfsr ^= 0x6000;
            }
        }
        assert_eq!(
            lfsr, 0x4000,
            "15-bit LFSR should return to 0x4000 after 32767 steps"
        );
    }

    #[test]
    fn test_ch4_lfsr_7bit_maximal_length() {
        // Verify the 7-bit Galois LFSR has maximum period of 127 (= 2^7 - 1).
        let mut lfsr = 0x0040_u16;
        for _ in 0..127 {
            let carry = lfsr & 1;
            lfsr >>= 1;
            if carry != 0 {
                lfsr ^= 0x0060;
            }
        }
        assert_eq!(
            lfsr, 0x0040,
            "7-bit LFSR should return to 0x0040 after 127 steps"
        );
    }

    // ─── Envelope timer period=0 behaviour (RED → GREEN) ─────────────────────

    /// On trigger with env_period=0, env_timer must be initialized to 8,
    /// not 0. Per Pan Docs / CGB hardware: the reload value is 8 when period=0.
    #[test]
    fn test_ch4_trigger_env_period_zero_sets_env_timer_to_8() {
        let mut ch = Channel4 {
            dac_on: true,
            init_volume: 7,
            env_period: 0,
            ..Channel4::default()
        };
        ch.write_cnt_h(0x8000, false); // trigger (bit 15)
        assert_eq!(
            ch.env_timer, 8,
            "env_timer must be 8 after trigger when env_period=0"
        );
    }

    /// clock_envelope with env_period=0 must still decrement env_timer
    /// (no volume change).
    #[test]
    fn test_ch4_clock_envelope_period_zero_decrements_timer() {
        let mut ch = Channel4 {
            active: true,
            dac_on: true,
            volume: 7,
            env_period: 0,
            env_timer: 8,
            ..Channel4::default()
        };
        let vol_before = ch.volume;
        ch.clock_envelope();
        assert_eq!(
            ch.volume, vol_before,
            "volume must not change when env_period=0"
        );
        assert_eq!(
            ch.env_timer, 7,
            "env_timer must count down even when env_period=0"
        );
    }

    /// When env_timer expires with env_period=0, the timer must reload to 8
    /// and no volume change must occur.
    #[test]
    fn test_ch4_clock_envelope_period_zero_reloads_to_8_on_expiry() {
        let mut ch = Channel4 {
            active: true,
            dac_on: true,
            volume: 7,
            env_period: 0,
            env_timer: 1, // about to expire
            ..Channel4::default()
        };
        let vol_before = ch.volume;
        ch.clock_envelope();
        assert_eq!(
            ch.env_timer, 8,
            "env_timer must reload to 8 when env_period=0 and timer expires"
        );
        assert_eq!(
            ch.volume, vol_before,
            "volume must not change when env_period=0"
        );
    }
}
