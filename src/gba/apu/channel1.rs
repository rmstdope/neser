use super::apu::DUTY_TABLE;

// ── CH1 — Pulse + Sweep ───────────────────────────────────────────────────────

/// Channel 1: pulse wave with frequency sweep.
#[derive(Debug, Clone, Default)]
pub struct Channel1 {
    // Sweep (SOUND1CNT_L)
    pub sweep_period: u8,
    pub sweep_negate: bool,
    pub sweep_shift: u8,

    // Tone (SOUND1CNT_H)
    pub duty: u8,
    pub length_counter: u16,
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,

    // Frequency + control (SOUND1CNT_X)
    pub freq: u16,
    pub length_en: bool,

    // Internal state
    pub active: bool,
    pub dac_on: bool,
    pub duty_pos: u8,
    pub freq_timer: u32,
    pub volume: u8,
    pub env_timer: u8,
    pub sweep_timer: u8,
    /// Shadow copy of frequency used by sweep.
    pub sweep_shadow: u16,
    pub sweep_enabled: bool,
    /// True if a sweep calculation was performed while sweep_negate was set.
    /// Used for the "zombie mode" quirk: switching from negate to add after
    /// a negate calculation immediately disables the channel.
    negate_used: bool,
}

impl Channel1 {
    /// Analogue output in `[-1.0, +1.0]`; 0.0 when inactive or DAC is off
    /// (disconnected).
    ///
    /// Per GBATek, the PSG DAC converts digital value D (0–15) to bipolar:
    ///   output = (D / 7.5) − 1.0
    /// giving −1.0 for D=0 and +1.0 for D=15.
    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        let bit = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        let d = if bit == 1 { self.volume as f32 } else { 0.0 };
        d / 7.5 - 1.0
    }

    /// Advance channel by `cycles` GBA cycles (frequency timer only).
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

    /// Clock the length counter (FS steps 0, 2, 4, 6).
    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        if self.length_counter == 0 {
            self.active = false;
        }
    }

    /// Clock the volume envelope (FS step 7).
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

    /// Clock the frequency sweep (FS steps 2 and 6).
    pub fn clock_sweep(&mut self) {
        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }
        if self.sweep_timer == 0 {
            self.sweep_timer = if self.sweep_period > 0 {
                self.sweep_period
            } else {
                8
            };
            if self.sweep_enabled && self.sweep_period > 0 {
                let new_freq = self.calc_sweep();
                if self.sweep_negate {
                    self.negate_used = true;
                }
                if new_freq > 2047 {
                    self.active = false;
                } else if self.sweep_shift > 0 {
                    self.sweep_shadow = new_freq;
                    self.freq = new_freq;
                    // Overflow check with new shadow.
                    if self.calc_sweep() > 2047 {
                        self.active = false;
                    }
                }
            }
        }
    }

    fn calc_sweep(&self) -> u16 {
        if self.sweep_negate {
            self.sweep_shadow
                .wrapping_sub(self.sweep_shadow >> self.sweep_shift)
        } else {
            self.sweep_shadow
                .wrapping_add(self.sweep_shadow >> self.sweep_shift)
        }
    }

    /// Trigger (write 1 to reset/trigger bit).
    pub fn trigger(&mut self) {
        self.active = self.dac_on;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        let period = (2048_u32.wrapping_sub(self.freq as u32)) * 16;
        self.freq_timer = period;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        // Sweep initialisation.
        self.sweep_shadow = self.freq;
        self.sweep_timer = if self.sweep_period > 0 {
            self.sweep_period
        } else {
            8
        };
        self.sweep_enabled = self.sweep_period > 0 || self.sweep_shift > 0;
        // Reset negate_used so zombie mode doesn't fire on the next direction change.
        self.negate_used = false;
        // Overflow check on trigger.
        if self.sweep_enabled && self.sweep_shift > 0 && self.calc_sweep() > 2047 {
            self.active = false;
        }
    }

    // ── Register writes ───────────────────────────────────────────────────

    /// SOUND1CNT_L: sweep register (bits 6-4 period, bit 3 negate, bits 2-0 shift).
    pub fn write_cnt_l(&mut self, val: u16) {
        let old_negate = self.sweep_negate;
        self.sweep_shift = (val & 0x07) as u8;
        self.sweep_negate = (val & 0x08) != 0;
        self.sweep_period = ((val >> 4) & 0x07) as u8;
        // Zombie mode: if negate was in use and we're switching to add mode,
        // the channel must be disabled immediately.
        if old_negate && !self.sweep_negate && self.negate_used {
            self.active = false;
        }
    }

    /// SOUND1CNT_H: tone / envelope (bits 5-0 length, 7-6 duty, 10-8 env period,
    /// 11 env add, 15-12 init volume).
    pub fn write_cnt_h(&mut self, val: u16) {
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

    /// SOUND1CNT_X: frequency and control (bits 10-0 freq, 14 length-enable,
    /// 15 trigger/reset — write-only).
    ///
    /// `extra_clk` must be `true` when the frame sequencer's next step will NOT
    /// clock the length counter (i.e. `fs_step` is odd: 1, 3, 5, 7).  In that
    /// case two extra-clock quirks apply:
    ///
    /// * Enabling `length_en` (0→1, no trigger): immediately decrement the
    ///   counter by 1 if it is nonzero.
    /// * Trigger: if the counter was just reloaded to the maximum (64) because
    ///   it was 0, decrement by 1.
    pub fn write_cnt_x(&mut self, val: u16, extra_clk: bool) {
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
            // Extra clock when trigger reloaded the counter to max (was 0) and
            // length_en is set.
            if extra_clk && self.length_en && reloaded_length {
                self.length_counter = 63;
            }
        }
    }

    /// Power-off reset.
    pub fn power_off(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_ch1(volume: u8, duty: u8, duty_pos: u8) -> Channel1 {
        Channel1 {
            active: true,
            dac_on: true,
            volume,
            duty,
            duty_pos,
            ..Channel1::default()
        }
    }

    /// Helper: build a triggered CH1 with sweep in negate mode so we can
    /// force a sweep calc to occur.
    fn triggered_negate_sweep_ch1() -> Channel1 {
        let mut ch = Channel1 {
            dac_on: true,
            ..Channel1::default()
        };
        // sweep_period=1, negate=true, shift=1 → each sweep tick subtracts
        ch.write_cnt_l(0x0019); // period=1, negate=1, shift=1
        // write_cnt_h: no length, 25% duty, volume env off, init_volume=8
        ch.write_cnt_h(0x8040); // init_vol=8, duty=1(25%), env off
        // write_cnt_x: freq=500, trigger
        ch.write_cnt_x(0x81F4, false); // trigger | freq=500
        ch
    }

    // ─── Unit tests: sweep zombie mode ───────────────────────────────────

    /// After a negate sweep calculation, clearing the negate bit must disable
    /// the channel immediately (zombie mode).
    #[test]
    fn test_ch1_sweep_zombie_disable_on_negate_to_add() {
        let mut ch = triggered_negate_sweep_ch1();
        assert!(ch.active, "channel should be active after trigger");

        // Perform one full sweep clock so negate_used becomes true.
        // sweep_period=1, sweep_timer will be 1, so one clock_sweep() fires.
        ch.clock_sweep();
        assert!(ch.active, "channel still active after negate sweep calc");

        // Now switch direction from negate to add → zombie mode disables channel.
        ch.write_cnt_l(0x0011); // period=1, negate=0, shift=1
        assert!(
            !ch.active,
            "channel must be disabled when negate→add after negate calc (zombie mode)"
        );
    }

    /// Without a prior negate calculation (negate_used=false), switching from
    /// negate to add should NOT disable the channel.
    #[test]
    fn test_ch1_sweep_no_zombie_if_negate_not_used() {
        let mut ch = triggered_negate_sweep_ch1();
        assert!(ch.active, "channel should be active after trigger");

        // Do NOT clock sweep — negate_used is still false.
        // Switch direction: this must NOT kill the channel.
        ch.write_cnt_l(0x0011); // period=1, negate=0, shift=1
        assert!(
            ch.active,
            "channel must stay active when negate→add without prior negate calc"
        );
    }

    /// After trigger, negate_used must be reset so a subsequent direction
    /// change does not spuriously disable the channel.
    #[test]
    fn test_ch1_sweep_negate_used_resets_on_trigger() {
        let mut ch = triggered_negate_sweep_ch1();
        // Perform sweep so negate_used=true.
        ch.clock_sweep();

        // Re-trigger resets negate_used.
        ch.write_cnt_x(0x81F4, false);
        assert!(ch.active, "channel still active after re-trigger");

        // Now direction change must NOT disable it (negate_used was reset).
        ch.write_cnt_l(0x0011); // period=1, negate=0, shift=1
        assert!(
            ch.active,
            "channel must stay active after trigger clears negate_used"
        );
    }

    #[test]
    fn test_ch1_dac_off_outputs_zero() {
        // When DAC is off, channel is disconnected → must output exactly 0.0.
        let ch1 = Channel1 {
            dac_on: false,
            active: false,
            ..Channel1::default()
        };
        assert_eq!(ch1.output(), 0.0);
    }

    #[test]
    fn test_ch1_duty_low_outputs_minus_one() {
        // When duty bit is 0, digital value D=0, bipolar output = -1.0.
        // Duty=0 (12.5%) at pos 0 has bit=0.
        let ch1 = active_ch1(15, 0, 0);
        let got = ch1.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "duty low must produce -1.0, got {got}"
        );
    }

    #[test]
    fn test_ch1_duty_high_full_volume_outputs_plus_one() {
        // When duty bit is 1 and volume=15, D=15, bipolar output = +1.0.
        // Duty=2 (50%) at pos 0 has bit=1.
        let ch1 = active_ch1(15, 2, 0);
        let got = ch1.output();
        assert!(
            (got - 1.0_f32).abs() < 1e-5,
            "volume=15 duty high must produce +1.0, got {got}"
        );
    }

    #[test]
    fn test_ch1_duty_high_half_volume_is_bipolar() {
        // Volume=8, duty high → D=8, output = 8/7.5 - 1.0 ≈ 0.0667
        let ch1 = active_ch1(8, 2, 0);
        let expected = 8.0_f32 / 7.5 - 1.0;
        let got = ch1.output();
        assert!(
            (got - expected).abs() < 1e-5,
            "volume=8 duty high: expected {expected}, got {got}"
        );
    }

    #[test]
    fn test_ch1_volume_zero_duty_high_outputs_minus_one() {
        // Volume=0, duty high → D=0, output = -1.0.
        let ch1 = active_ch1(0, 2, 0);
        let got = ch1.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "volume=0 duty high must produce -1.0, got {got}"
        );
    }
}
