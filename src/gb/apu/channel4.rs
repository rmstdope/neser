//! CH4 – Noise channel with LFSR (NR41–NR44).
//!
//! LFSR clock model (Pan Docs / SameBoy):
//! - A 14-bit "noise counter" runs off the global APU T-cycle stream.
//! - The counter increments every `divisor` T-cycles, where
//!   `divisor = (NR43 & 0x07) << 2`, and code 0 is special-cased to 2.
//! - The LFSR is stepped on the **rising edge** of bit `(NR43 >> 4)` of
//!   the counter, but only when the channel is currently active.
//! - When the channel triggers, `prepare_noise_start()` aligns the counter
//!   relative to the APU stream (`alignment & 3`), which is what makes
//!   restart / freq-change / 7↔15 mode switches behave correctly.

use crate::trace_apu;
use serde::{Deserialize, Serialize};

use super::channel1::EnvelopeClockState;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 14-bit noise counter (Pan Docs / SameBoy `noise_channel.counter`).
    #[serde(default)]
    counter: u16,
    /// T-cycles until the next counter increment.
    #[serde(default)]
    counter_countdown: u16,
    /// `true` while NR42 DAC is on AND channel was triggered. Resets on APU
    /// off and DAC disable. Mirrors SameBoy `noise_counter_active`.
    #[serde(default)]
    noise_counter_active: bool,
    /// `true` while the counter should keep ticking even when the channel
    /// is inactive. Mirrors SameBoy `noise_background_counter_active`.
    #[serde(default)]
    noise_background_counter_active: bool,
    /// `true` if the most-recent trigger happened with the DAC disabled.
    #[serde(default)]
    noise_started_with_dac_disabled: bool,
    /// T-cycle stream alignment counter (only the low 2 bits are read).
    #[serde(default)]
    alignment: u16,
    /// Set after the noise counter has stepped at least once since reset.
    #[serde(default)]
    did_step_counter: bool,
    pub(crate) length_counter: u8,
    volume: u8,
    env_timer: u8,
    /// Envelope clock state for zombie mode glitch tracking.
    #[serde(default)]
    env_clock_state: EnvelopeClockState,
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
            lfsr: 0,
            counter: 0,
            counter_countdown: 0,
            noise_counter_active: false,
            noise_background_counter_active: false,
            noise_started_with_dac_disabled: false,
            alignment: 0,
            did_step_counter: false,
            length_counter: 0,
            volume: 0,
            env_timer: 0,
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
        // LFSR bit 0 low = channel output high.
        if self.lfsr & 0x01 == 0 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }

    /// Digital output (0-15) before DAC conversion (for PCM34 register).
    pub fn digital_output(&self) -> u8 {
        if !self.active || !self.dac_on {
            return 0;
        }
        // LFSR bit 0 low = channel output high.
        if self.lfsr & 0x01 == 0 {
            self.volume
        } else {
            0
        }
    }

    /// SameBoy `divisor`: number of T-cycles per noise-counter increment.
    /// `divisor = (NR43 & 7) << 2`, with code 0 being the special-case 2.
    fn divisor_t_cycles(&self) -> u16 {
        let d = (self.divisor_code as u16) << 2;
        if d == 0 { 2 } else { d }
    }

    /// Mask of the counter bit observed for the rising-edge LFSR step.
    fn counter_bit_mask(&self) -> u16 {
        1u16 << self.clock_shift
    }

    /// Advance the noise subsystem by one M-cycle (= 4 T-cycles).
    ///
    /// Mirrors the inner loop of SameBoy `GB_apu_run` for the noise channel.
    pub fn tick(&mut self) {
        const CYCLES: u16 = 4;
        // Track APU stream alignment (only the low 2 bits ever matter).
        self.alignment = self.alignment.wrapping_add(CYCLES);

        if !(self.noise_counter_active || self.noise_background_counter_active) {
            return;
        }

        let divisor = self.divisor_t_cycles();
        if self.counter_countdown == 0 {
            self.counter_countdown = divisor;
        }

        let mut cycles_left: u16 = CYCLES;
        while cycles_left >= self.counter_countdown {
            cycles_left -= self.counter_countdown;
            self.counter_countdown = divisor;

            let mask = self.counter_bit_mask();
            let old_bit = (self.counter & mask) != 0;
            self.counter = (self.counter.wrapping_add(1)) & 0x3FFF;
            self.did_step_counter = true;
            let new_bit = (self.counter & mask) != 0;

            if new_bit && !old_bit && self.active {
                trace_apu!(5; "GB APU CH4 counter rising edge -> step LFSR (counter=0x{:04X})", self.counter);
                self.clock_lfsr();
            }
        }
        if cycles_left > 0 {
            self.counter_countdown -= cycles_left;
        }
    }

    /// Clock the LFSR (exposed for testing).
    ///
    /// Per Pan Docs / SameBoy `step_lfsr`: feedback is XNOR of bits 0 and 1,
    /// written to bit 14 (15-bit mode) or **both** bits 14 and 6 (narrow /
    /// 7-bit mode). The explicit clear in the `else` branch matters: it keeps
    /// bit 6 in the right state when the mode is later switched.
    pub fn clock_lfsr(&mut self) {
        let new_high_bit = ((self.lfsr & 0x01) ^ ((self.lfsr >> 1) & 0x01) ^ 1) & 1;
        let high_bit_mask: u16 = if self.lfsr_7bit { 0x4040 } else { 0x4000 };
        self.lfsr >>= 1;
        if new_high_bit != 0 {
            self.lfsr |= high_bit_mask;
        } else {
            self.lfsr &= !high_bit_mask;
        }
        trace_apu!(5; "GB APU CH4 LFSR shift mode={} lfsr=0x{:04X}", 
            if self.lfsr_7bit { "7-bit" } else { "15-bit" }, self.lfsr);
    }

    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        trace_apu!(3; "GB APU CH4 length_counter={} active={}", self.length_counter, self.length_counter > 0);
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
                trace_apu!(3; "GB APU CH4 envelope volume {} -> {}", old_volume, self.volume);
            }
        }
    }

    /// Clear envelope clock flag after frame sequencer step completes.
    pub fn clear_envelope_clock(&mut self) {
        self.env_clock_state.clock = false;
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
        self.counter = 0;
        self.counter_countdown = 0;
        self.noise_counter_active = false;
        self.noise_background_counter_active = false;
        self.noise_started_with_dac_disabled = false;
        self.did_step_counter = false;
        self.length_counter = 0;
        self.volume = 0;
        self.env_timer = 0;
        self.env_clock_state = EnvelopeClockState::default();
        // Note: `alignment` is intentionally preserved across power off in
        // SameBoy, since it tracks the APU stream phase.
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
        trace_apu!(2; "GB APU CH4 write NR41=0x{:02X} length={}", val, val & 0x3F);
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr42(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH4 write NR42=0x{:02X} volume={} env_add={} env_period={}", 
            val, (val >> 4) & 0x0F, (val & 0x08) != 0, val & 0x07);

        let old_val = self.read_nr42();

        self.init_volume = (val >> 4) & 0x0F;
        self.env_add = val & 0x08 != 0;
        self.env_period = val & 0x07;
        let dac_now_on = val & 0xF8 != 0;

        if !dac_now_on {
            // SameBoy: disabling the DAC clears noise_counter_active and, when
            // the divisor is non-zero, the background counter as well.
            // Per-revision counter "kick" quirk (CGB-E special edge case where
            // counter_countdown ≤ 2 forces an extra increment) is intentionally
            // omitted — it is non-deterministic and revision-specific.
            if self.active && (self.divisor_code & 0x07) != 0 {
                self.noise_background_counter_active = false;
            }
            self.active = false;
            self.noise_counter_active = false;
            self.dac_on = false;
        } else {
            self.dac_on = true;
            if self.active {
                // Apply zombie mode glitch when writing NRx2 while channel is active.
                self.apply_nrx2_glitch(old_val, val);
            }
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
            trace_apu!(3; "GB APU CH4 zombie invert volume {} -> {}", old_volume, self.volume);
        }

        if should_tick {
            let old_volume = self.volume;
            if new_direction_add {
                self.volume = (self.volume + 1) & 0x0F;
            } else {
                self.volume = self.volume.wrapping_sub(1) & 0x0F;
            }
            trace_apu!(3; "GB APU CH4 zombie tick volume {} -> {}", old_volume, self.volume);
        } else if new_period == 0 && self.env_clock_state.clock {
            self.env_clock_state.clock = false;
        }
    }

    pub fn write_nr43(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH4 write NR43=0x{:02X} shift={} mode={} divisor={}", 
            val, (val >> 4) & 0x0F, if (val & 0x08) != 0 { "7-bit" } else { "15-bit" }, val & 0x07);
        self.clock_shift = (val >> 4) & 0x0F;
        self.lfsr_7bit = val & 0x08 != 0;
        self.divisor_code = val & 0x07;
    }

    pub fn write_nr44(&mut self, val: u8, extra_clk: bool) {
        trace_apu!(2; "GB APU CH4 write NR44=0x{:02X} trigger={} length_en={}", 
            val, (val & 0x80) != 0, (val & 0x40) != 0);
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
        trace_apu!(1; "GB APU CH4 trigger volume={} shift={} mode={} divisor={}",
            self.init_volume, self.clock_shift,
            if self.lfsr_7bit { "7-bit" } else { "15-bit" }, self.divisor_code);
        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();
        self.prepare_noise_start();
    }

    /// Port of SameBoy `prepare_noise_start`. Aligns the noise counter to the
    /// APU T-cycle stream, sets `counter_countdown`, and seeds the LFSR.
    ///
    /// This implements the deterministic CGB-E variant. Branches that are
    /// only relevant for CGB-C, DMG, AGB, double-speed, or non-deterministic
    /// instance-specific behavior are intentionally omitted; SameBoy itself
    /// carries TODOs for those cases.
    fn prepare_noise_start(&mut self) {
        // `noise_counter_active` resets on APU off and DAC disable; it is
        // (re)enabled here only when the DAC is on.
        self.noise_counter_active = self.dac_on;
        self.noise_started_with_dac_disabled = !self.noise_counter_active;
        let mut divisor = self.divisor_code as i32 & 0x07;
        let was_background_counting = self.noise_background_counter_active;
        self.noise_background_counter_active = true;
        let mut instant_step = false;

        // Pre-trigger countdown peephole quirks.
        if divisor > 1 && self.counter_countdown == 1 {
            self.counter = (self.counter.wrapping_add(1)) & 0x3FFF;
        } else if self.counter_countdown == 2 && (self.alignment & 3) == 0 && self.active {
            if divisor == 0 {
                divisor = 8; // SameBoy explicit override for this edge case.
            } else if divisor == 1 {
                let mask = 1u16 << self.clock_shift;
                let old_bit = (self.counter & mask) != 0;
                self.counter = (self.counter.wrapping_add(1)) & 0x3FFF;
                let new_bit = (self.counter & mask) != 0;
                if new_bit && !old_bit {
                    instant_step = true;
                }
            }
        }

        // Base reload value (SameBoy: `divisor == 0 ? 6 : divisor*4 + 6`).
        let mut countdown: i32 = if divisor == 0 { 6 } else { divisor * 4 + 6 };

        // Alignment-based offset table (CGB-E branches only).
        if (self.alignment & 1) != 0 {
            if divisor == 0 {
                // CGB-E (model > CGB_C): branch on whether we were already
                // background counting.
                if was_background_counting {
                    countdown -= 1;
                } else {
                    countdown += 1;
                }
            } else if (self.alignment & 2) != 0 {
                if divisor == 1 && !self.active {
                    countdown += 1;
                } else {
                    countdown -= 3;
                }
            } else {
                countdown -= 1;
                if divisor == 1 && self.active {
                    countdown -= 4;
                }
            }
        } else if divisor != 0 {
            if (self.alignment & 2) != 0 {
                countdown -= 2;
            } else if divisor > 1 {
                countdown -= 4;
            } else if divisor == 1 && self.active && (self.clock_shift == 0) {
                // SameBoy: `!(NR43 & 0xf0)` — only when shift bits are zero.
                countdown -= 4;
            }
        }

        // Background-counting glitches.
        if divisor > 1 {
            if !self.noise_counter_active && (self.alignment & 3) == 0 {
                countdown += 4;
            }
        } else if was_background_counting && !self.active && (self.alignment & 3) == 0 {
            if divisor == 0 {
                if self.noise_started_with_dac_disabled {
                    countdown += 28;
                }
            } else {
                countdown -= 4;
            }
        }

        // SameBoy seeds 0x0055 only for divisor=0 / alignment&3==3 / active.
        if divisor == 0 && self.active && (self.alignment & 3) == 3 {
            self.lfsr = 0x0055;
        } else {
            self.lfsr = 0;
        }

        // Clamp to non-negative — SameBoy uses unsigned arithmetic that
        // implicitly wraps; the documented branches above are designed to
        // never underflow on CGB-E in practice.
        self.counter_countdown = countdown.max(1) as u16;

        if instant_step {
            self.clock_lfsr();
        }
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
    fn test_trigger_resets_lfsr_to_zero() {
        // Pan Docs / SameBoy: the LFSR is cleared to 0 on (re)trigger.
        assert_eq!(triggered_ch4().lfsr, 0);
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
        // LFSR = 0 (post-trigger): bits 0 and 1 are 0, XNOR feedback = 1.
        // After `>>1` then setting bit 14: result = 0x4000.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // 15-bit
        ch.write_nr44(0x80, false); // trigger -> LFSR = 0
        ch.clock_lfsr();
        assert_eq!(ch.lfsr, 0x4000);
    }

    #[test]
    fn test_7bit_lfsr_writes_bits_14_and_6() {
        // Narrow mode: feedback is written to BOTH bit 14 and bit 6.
        // From LFSR=0, XNOR feedback = 1, so both bits are set.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x08); // 7-bit
        ch.write_nr44(0x80, false);
        ch.clock_lfsr();
        assert_eq!(ch.lfsr & (1 << 6), 1 << 6);
        assert_eq!(ch.lfsr & (1 << 14), 1 << 14);
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

    // ── Noise-counter precision tests ─────────────────────────────────────

    #[test]
    fn test_tick_decrements_countdown_within_mcycle() {
        // divisor_code=7 → divisor = 28 T-cycles. One tick (4 T-cycles)
        // should not increment the counter.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x07); // divisor=28, shift=0
        ch.write_nr44(0x80, false); // trigger
        let counter_before = ch.counter;
        let countdown_before = ch.counter_countdown;
        let lfsr_before = ch.lfsr;
        ch.tick();
        assert_eq!(
            ch.counter, counter_before,
            "counter must not increment when no countdown wraparound occurs"
        );
        assert!(
            ch.counter_countdown < countdown_before,
            "countdown should decrement"
        );
        assert_eq!(
            ch.lfsr, lfsr_before,
            "LFSR must not step when counter doesn't increment"
        );
    }

    #[test]
    fn test_tick_increments_counter_when_countdown_expires() {
        // divisor_code=0 → divisor = 2 T-cycles. One M-cycle (4 T-cycles)
        // should increment the counter twice.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // divisor=2, shift=0
        ch.write_nr44(0x80, false);
        let counter_before = ch.counter;
        // Force countdown to 2 so we get exactly 2 increments per M-cycle.
        ch.counter_countdown = 2;
        ch.tick();
        assert_eq!(
            ch.counter,
            counter_before.wrapping_add(2) & 0x3FFF,
            "counter should increment twice for divisor=2"
        );
    }

    #[test]
    fn test_lfsr_steps_only_on_rising_edge_of_observed_bit() {
        // shift=0 → mask = bit 0. Counter goes 0→1 (rising), 1→2 (falling).
        // Across two increments the LFSR should step exactly once.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // divisor=2, shift=0
        ch.write_nr44(0x80, false); // trigger → counter=0, lfsr=0
        ch.counter = 0;
        ch.counter_countdown = 2;
        ch.tick();
        assert_eq!(ch.counter & 0x3, 2, "counter incremented twice");
        // Rising edge of bit 0 happened on 0→1; LFSR stepped once: 0 → 0x4000.
        assert_eq!(
            ch.lfsr, 0x4000,
            "LFSR should step exactly once across two counter increments (one rising edge)"
        );
    }

    #[test]
    fn test_lfsr_progresses_over_many_mcycles() {
        // Sanity check: with code=0/shift=4 (typical), LFSR should produce
        // a varying sequence of bit-0 values over many M-cycles.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x40); // divisor=2, shift=4 → period = 2 * 32 = 64 T-cycles
        ch.write_nr44(0x80, false);
        let mut bits = Vec::new();
        for _ in 0..500 {
            ch.tick();
            bits.push((ch.lfsr & 1) as u8);
        }
        let zeros = bits.iter().filter(|&&b| b == 0).count();
        let ones = bits.iter().filter(|&&b| b == 1).count();
        assert!(
            zeros > 50 && ones > 50,
            "LFSR output should vary: zeros={zeros} ones={ones}"
        );
    }
}
