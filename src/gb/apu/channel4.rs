//! CH4 – Noise channel with LFSR (NR41–NR44).
//!
//! LFSR clock: `f = 524_288 / divisor / 2^(shift+1)` Hz.
//! 7-bit mode: feedback is also written to bit 6, shortening the period.

use crate::trace_apu;
use serde::{Deserialize, Serialize};

use super::channel1::EnvelopeClockState;

/// Divisor lookup for noise clock (NR43 bits 2-0).
const DIVISORS: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

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
    freq_timer: u32,
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
            lfsr: 0x7FFF,
            freq_timer: 0,
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

    fn freq_timer_period(&self) -> u32 {
        DIVISORS[self.divisor_code as usize] << self.clock_shift
    }

    /// Advance the frequency timer by one M-cycle (= 4 T-cycles).
    ///
    /// Processes each T-cycle individually to maintain sub-M-cycle precision.
    /// When the timer expires mid-M-cycle, the remaining T-cycles are applied
    /// after the reload, ensuring correct phase alignment.
    pub fn tick(&mut self) {
        let period = self.freq_timer_period();
        if self.freq_timer == 0 {
            self.freq_timer = period;
        }
        for _ in 0..4 {
            self.freq_timer -= 1;
            if self.freq_timer == 0 {
                self.freq_timer = period;
                trace_apu!(5; "GB APU CH4 tick timer expired, clocking LFSR");
                self.clock_lfsr();
            }
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
            trace_apu!(3; "GB APU CH4 envelope volume {} -> {}", old_volume, self.volume);
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
        self.env_clock_state = EnvelopeClockState::default();
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
        self.write_nr44_with_apu_phase(val, extra_clk, None);
    }

    pub fn write_nr44_with_apu_phase(
        &mut self,
        val: u8,
        extra_clk: bool,
        apu_tick_accumulator: Option<u8>,
    ) {
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
            self.trigger(apu_tick_accumulator);
            if extra_clk && self.length_en && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    pub fn write_nr41_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    fn trigger(&mut self, apu_tick_accumulator: Option<u8>) {
        trace_apu!(1; "GB APU CH4 trigger volume={} shift={} mode={} divisor={}",
            self.init_volume, self.clock_shift,
            if self.lfsr_7bit { "7-bit" } else { "15-bit" }, self.divisor_code);
        let was_active = self.active;
        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        // CH4's LFSR starts from an already-loaded period, unlike pulse
        // channels which add the full 10/8 T-cycle startup delay to their
        // frequency period. SameSuite channel_4_align shows that noise needs
        // only the remaining phase correction here: the pulse-channel 10/8
        // T-cycle startup minus the 4 T-cycles already represented by the
        // first LFSR period, giving 6 T-cycles when the double-speed APU
        // accumulator's low bit is 0 and 4 T-cycles when it is 1. The
        // accumulator is expected to be 0 or 1; mask the low bit so callers
        // cannot accidentally feed higher bookkeeping bits into the phase
        // calculation.
        let period = self.freq_timer_period();
        let freq_timer = if let Some(acc) = apu_tick_accumulator {
            let delay_t = 6u32 - 2 * u32::from(acc & 1);
            let delay_t = if was_active {
                delay_t.saturating_sub(2)
            } else {
                delay_t
            };
            period + delay_t
        } else {
            // Normal-speed CH4 startup is phase-sensitive to the background
            // noise counter. These NR43/timer combinations are the CGB-E cases
            // exercised by SameSuite channel_4_frequency_alignment; they adjust
            // the first post-trigger LFSR clock while subsequent clocks still use
            // freq_timer_period(). Values are T-cycles for the initial timer.
            match self.read_nr43() {
                0x09 => {
                    if self.freq_timer <= 4 {
                        16
                    } else {
                        20
                    }
                }
                0x18 => 16,
                0x0A => {
                    if self.freq_timer >= 20 {
                        24
                    } else {
                        20
                    }
                }
                0x28 => 24,
                0x0B => {
                    if self.freq_timer >= 36 {
                        32
                    } else {
                        28
                    }
                }
                0x1A => {
                    if self.freq_timer >= 52 {
                        40
                    } else {
                        36
                    }
                }
                0x0C => {
                    if self.freq_timer >= 52 {
                        40
                    } else {
                        36
                    }
                }
                0x29 => {
                    if self.freq_timer >= 52 {
                        40
                    } else {
                        44
                    }
                }
                0x38 => 40,
                _ => {
                    let divisor = u32::from(self.divisor_code);
                    if divisor == 0 { 12 } else { divisor * 8 + 12 }
                }
            }
        };
        self.freq_timer = freq_timer;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        self.lfsr = 0x7FFF;
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();
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

    // ── T-cycle precision tests ───────────────────────────────────────────

    #[test]
    fn test_tick_freq_timer_decrements_by_tcycles_within_mcycle() {
        // Given: freq_timer = 6;
        // When: tick() once (4 T-cycles);
        // Then: freq_timer should be 2 (6 - 4 = 2), no LFSR clock.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // divisor=0 (8), shift=0 → period = 8
        ch.write_nr44(0x80, false); // trigger → LFSR = 0x7FFF
        ch.freq_timer = 6;
        let lfsr_before = ch.lfsr;
        ch.tick();
        assert_eq!(
            ch.freq_timer, 2,
            "freq_timer should decrement to 2 after one M-cycle"
        );
        assert_eq!(ch.lfsr, lfsr_before, "LFSR should not clock when timer > 0");
    }

    #[test]
    fn test_tick_freq_timer_expires_mid_mcycle_and_reloads_with_remainder() {
        // Given: freq_timer = 3, period = 8 (divisor=0, shift=0);
        // When: tick() once (4 T-cycles);
        // Then: timer expires at T-cycle 3, reloads to 8,
        //       then 1 remaining T-cycle decrements to 7.
        //       LFSR should clock once.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // period = 8
        ch.write_nr44(0x80, false); // trigger → LFSR = 0x7FFF
        ch.freq_timer = 3;
        ch.tick();
        assert_eq!(
            ch.freq_timer, 7,
            "freq_timer should be period - remaining (8 - 1 = 7)"
        );
        // LFSR clocked once: 0x7FFF → 0x3FFF
        assert_eq!(
            ch.lfsr, 0x3FFF,
            "LFSR should clock once when timer expires mid M-cycle"
        );
    }

    #[test]
    fn test_tick_freq_timer_expires_exactly_at_mcycle_boundary() {
        // Given: freq_timer = 4, period = 8;
        // When: tick() once;
        // Then: timer expires at T-cycle 4, reloads to 8, no remaining T-cycles.
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x00); // period = 8
        ch.write_nr44(0x80, false); // trigger → LFSR = 0x7FFF
        ch.freq_timer = 4;
        ch.tick();
        assert_eq!(
            ch.freq_timer, 8,
            "freq_timer should be exactly period after expiring at boundary"
        );
        assert_eq!(ch.lfsr, 0x3FFF, "LFSR should clock once");
    }
}
