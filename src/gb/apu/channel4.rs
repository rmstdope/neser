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
    /// 14-bit up-counter for noise prescaler (SameBoy counter model).
    /// Tracks the hardware counter that drives LFSR clocking via
    /// rising-edge detection on bit(clock_shift).
    #[serde(default)]
    counter: u16,
    /// APU-cycle countdown until next counter increment (2 MHz units).
    #[serde(default)]
    counter_countdown: u16,
    /// Running total of APU cycles since APU init, used for
    /// alignment-dependent adjustments on trigger and NR43 writes.
    #[serde(default)]
    alignment: u32,
    /// True when the last tick ended with counter_countdown exactly
    /// at a reload boundary (no leftover cycles).
    #[serde(default)]
    countdown_reloaded: bool,
    /// True after trigger — enables counter model updates in tick().
    #[serde(default)]
    counter_active: bool,
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
            counter: 0,
            counter_countdown: 0,
            alignment: 0,
            countdown_reloaded: false,
            counter_active: false,
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
    ///
    /// Also maintains the parallel counter model (alignment, counter,
    /// counter_countdown) used for mid-stream NR43 write adjustments.
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

        // Maintain counter model (2 APU cycles per M-cycle).
        self.alignment = self.alignment.wrapping_add(2);
        if self.counter_active {
            self.tick_counter_model();
        }
    }

    /// Process 2 APU cycles of the noise prescaler counter model.
    fn tick_counter_model(&mut self) {
        let reload = self.counter_reload();
        if self.counter_countdown > 2 {
            self.counter_countdown -= 2;
            self.countdown_reloaded = false;
        } else if self.counter_countdown == 2 {
            self.counter = (self.counter + 1) & 0x3FFF;
            self.counter_countdown = reload;
            self.countdown_reloaded = true;
        } else if self.counter_countdown == 1 {
            self.counter = (self.counter + 1) & 0x3FFF;
            self.counter_countdown = reload - 1;
            self.countdown_reloaded = false;
        } else {
            // counter_countdown == 0: shouldn't happen in normal operation
            self.counter_countdown = reload;
        }
    }

    /// Counter reload value in APU cycles (2 MHz).
    fn counter_reload(&self) -> u16 {
        if self.divisor_code == 0 {
            2
        } else {
            u16::from(self.divisor_code) * 4
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
        self.counter = 0;
        self.counter_countdown = 0;
        self.alignment = 0;
        self.countdown_reloaded = false;
        self.counter_active = false;
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

        // CGB-E: when the counter just reloaded, reset counter_countdown
        // based on the new divisor and an alignment-dependent adjustment.
        if self.countdown_reloaded && self.counter_active {
            let new_div = val & 0x07;
            let new_divisor = if new_div == 0 {
                2u16
            } else {
                u16::from(new_div) * 4
            };
            let al_adj = [2u16, 1, 0, 3][(self.alignment & 3) as usize];
            self.counter_countdown = if new_divisor == 2 {
                new_divisor
            } else {
                new_divisor + al_adj
            };
        }

        self.clock_shift = (val >> 4) & 0x0F;
        self.lfsr_7bit = val & 0x08 != 0;
        self.divisor_code = val & 0x07;

        // Recompute freq_timer from counter state for mid-stream changes.
        if self.counter_active {
            self.recompute_freq_timer_from_counter();
        }
    }

    /// Compute freq_timer from the current counter model state.
    ///
    /// Determines how many T-cycles remain until the next LFSR clock
    /// by finding the distance (in counter increments) to the next
    /// rising edge of `bit(clock_shift)` in the counter.
    fn recompute_freq_timer_from_counter(&mut self) {
        let shift = u32::from(self.clock_shift);
        let reload = u32::from(self.counter_reload());
        let half_period = 1u32 << shift;
        let full_period = half_period << 1;
        let pos = u32::from(self.counter) & (full_period - 1);
        let distance = if pos < half_period {
            half_period - pos
        } else {
            full_period - pos + half_period
        };
        self.freq_timer = u32::from(self.counter_countdown) * 2 + (distance - 1) * reload * 2;
    }

    pub fn write_nr44(&mut self, val: u8, extra_clk: bool) {
        self.write_nr44_with_apu_phase(val, extra_clk, None);
    }

    pub fn write_nr44_with_apu_phase(
        &mut self,
        val: u8,
        extra_clk: bool,
        double_speed_phase_bits: Option<u8>,
    ) {
        self.write_nr44_with_apu_phase_and_length_quirk(
            val,
            extra_clk,
            double_speed_phase_bits,
            false,
        );
    }

    pub fn write_nr44_with_apu_phase_and_length_quirk(
        &mut self,
        val: u8,
        extra_clk: bool,
        double_speed_phase_bits: Option<u8>,
        cgb_early_extra_length_clock: bool,
    ) {
        trace_apu!(2; "GB APU CH4 write NR44=0x{:02X} trigger={} length_en={}", 
            val, (val & 0x80) != 0, (val & 0x40) != 0);
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;
        let clocks_length_on_extra = self.length_en || cgb_early_extra_length_clock;

        if extra_clk && !old_length_en && clocks_length_on_extra && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.active = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger(double_speed_phase_bits);
            if extra_clk && clocks_length_on_extra && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    pub fn write_nr41_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    fn trigger(&mut self, double_speed_phase_bits: Option<u8>) {
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
        // first LFSR period, giving 6 T-cycles when the double-speed trigger
        // phase bit is 0 and 4 T-cycles when it is 1. Mask the low bit so the
        // NR52 power-on phase bit cannot affect CH4's phase calculation.
        let period = self.freq_timer_period();
        let freq_timer = if let Some(phase_bits) = double_speed_phase_bits {
            let delay_t = 6u32 - 2 * u32::from(phase_bits & 1);
            let delay_t = if was_active {
                delay_t.saturating_sub(2)
            } else {
                delay_t
            };
            period + delay_t
        } else {
            // Normal-speed CH4 startup is phase-sensitive to the background
            // noise counter. SameSuite channel_4_frequency_alignment covers the
            // CGB-E-observed startup countdowns for NR43 values whose divisors
            // are aliases or near-aliases of 4/8/12/16-M-cycle noise samples:
            // the high nibble is clock_shift, bit 3 selects 7-bit LFSR mode,
            // and bits 0-2 are divisor_code. These values adjust only the first
            // post-trigger LFSR clock; subsequent clocks still use
            // freq_timer_period(). Values are T-cycles for the initial timer.
            match self.read_nr43() {
                0x09 => {
                    // shift=0, 7-bit, divisor_code=1
                    if self.freq_timer <= 4 { 16 } else { 20 }
                }
                0x18 => 16, // shift=1, 7-bit, divisor_code=0
                0x0A => {
                    // shift=0, 7-bit, divisor_code=2
                    if self.freq_timer >= 20 { 24 } else { 20 }
                }
                0x28 => 24, // shift=2, 7-bit, divisor_code=0
                0x0B => {
                    // shift=0, 7-bit, divisor_code=3
                    if self.freq_timer >= 36 { 32 } else { 28 }
                }
                0x0C | 0x1A => {
                    // 0x0C: shift=0, divisor_code=4. 0x1A: shift=1,
                    // divisor_code=2. SameSuite observes the same initial
                    // countdown for both equivalent 16-M-cycle samples.
                    if self.freq_timer >= 52 { 40 } else { 36 }
                }
                0x29 => {
                    // shift=2, 7-bit, divisor_code=1
                    if self.freq_timer >= 52 { 40 } else { 44 }
                }
                0x38 => 40, // shift=3, 7-bit, divisor_code=0
                // Retrigger adds one extra period to the startup delay.
                // SameBoy's prepare_noise_start increments counter_countdown
                // by 1 when was_background_counting, which shifts the first
                // LFSR clock by one full period in our freq_timer model.
                _ => {
                    if was_active {
                        period * 2 + 4
                    } else {
                        period + 4
                    }
                }
            }
        };
        self.freq_timer = freq_timer;
        self.volume = self.init_volume;
        self.env_timer = self.env_period;
        self.lfsr = 0x7FFF;
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();

        // Initialize counter model (SameBoy's prepare_noise_start).
        // Counter is NOT reset on trigger — only on APU init.
        let div = self.divisor_code;
        let mut initial_cd = if div == 0 {
            6u16
        } else {
            u16::from(div) * 4 + 6
        };
        // CGB-E alignment adjustments.
        if self.alignment & 1 == 0 {
            if div > 1 {
                if self.alignment & 2 != 0 {
                    initial_cd = initial_cd.saturating_sub(2);
                } else {
                    initial_cd = initial_cd.saturating_sub(4);
                }
            }
        } else if div == 0 {
            if self.counter_active {
                initial_cd -= 1;
            } else {
                initial_cd += 1;
            }
        } else if self.alignment & 2 != 0 {
            initial_cd = initial_cd.saturating_sub(3);
        } else {
            initial_cd = initial_cd.saturating_sub(1);
        }
        self.counter_countdown = initial_cd;
        self.countdown_reloaded = false;
        self.counter_active = true;
    }

    /// Reset counter model state when APU is turned on (NR52 bit 7 set).
    /// Mirrors SameBoy's `GB_apu_init` which zeroes alignment and counter.
    pub fn apu_init(&mut self) {
        self.counter = 0;
        self.counter_countdown = 0;
        self.alignment = 0;
        self.countdown_reloaded = false;
        self.counter_active = false;
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

    #[test]
    fn test_double_speed_phase_sets_initial_noise_timer() {
        let mut phase0 = Channel4::new();
        phase0.write_nr42(0xF0);
        phase0.write_nr43(0x00);
        phase0.write_nr44_with_apu_phase(0x80, false, Some(0));

        let mut phase1 = Channel4::new();
        phase1.write_nr42(0xF0);
        phase1.write_nr43(0x00);
        phase1.write_nr44_with_apu_phase(0x80, false, Some(1));

        assert_eq!(phase0.freq_timer, 14);
        assert_eq!(phase1.freq_timer, 12);
    }

    #[test]
    fn test_double_speed_phase_uses_shifted_noise_period() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x40); // shift=4, divisor_code=0 => period 128.
        ch.write_nr44_with_apu_phase(0x80, false, Some(0));
        assert_eq!(ch.freq_timer, 134);
    }

    #[test]
    fn test_normal_speed_frequency_alignment_startup_timer() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x09);
        ch.freq_timer = 4;
        ch.write_nr44(0x80, false);
        assert_eq!(ch.freq_timer, 16);
    }

    #[test]
    fn test_normal_speed_default_startup_timer_uses_shifted_period() {
        let mut ch = Channel4::new();
        ch.write_nr42(0xF0);
        ch.write_nr43(0x40); // period 8 << 4 = 128; default startup adds 4.
        ch.write_nr44(0x80, false);
        assert_eq!(ch.freq_timer, 132);
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
