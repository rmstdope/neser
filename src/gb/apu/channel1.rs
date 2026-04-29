//! CH1 – Pulse channel with frequency sweep (NR10–NR14).

use crate::trace_apu;
use serde::{Deserialize, Serialize};

/// Envelope clock state for zombie mode glitch tracking.
/// Per SameBoy: envelope clock affects how NRx2 writes modify volume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvelopeClockState {
    /// True when envelope is "locked" - it has stopped automatic updates
    /// (volume reached 0 in decrease mode or 15 in increase mode).
    #[serde(default)]
    pub locked: bool,
    /// True on the M-cycle when the envelope timer just fired.
    /// Reset after the frame sequencer step completes.
    #[serde(default)]
    pub clock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel1 {
    // NR10 fields
    sweep_period: u8, // bits 6-4 (0-7)
    sweep_negate: bool,
    sweep_shift: u8, // bits 2-0 (0-7)

    // NR11 fields
    duty: u8,        // bits 7-6 (0-3)
    length_load: u8, // bits 5-0 (0-63)

    // NR12 fields
    init_volume: u8, // bits 7-4 (0-15)
    env_add: bool,
    env_period: u8, // bits 2-0 (0-7)

    // NR13/NR14 fields
    freq: u16, // 11-bit frequency (NR13 low + NR14 bits 2-0 high)
    length_en: bool,

    // Internal state
    active: bool,
    dac_on: bool,                  // NR12 bits 7-3 != 0
    duty_pos: u8,                  // 0-7
    freq_timer: u16,               // countdown; reloads to (2048 - freq) * 4
    pub(crate) length_counter: u8, // 0-64; silences when reaches 0
    volume: u8,                    // current volume 0-15
    env_timer: u8,                 // envelope period countdown
    sweep_timer: u8,               // sweep period countdown
    sweep_shadow: u16,             // shadow frequency register
    sweep_enabled: bool,
    /// Set when a negate-mode calculation is performed; cleared on trigger.
    /// If NR10 clears the negate bit after this was set, the channel is disabled.
    negate_used: bool,
    /// Gate flag: duty step clock is disabled until the first trigger after
    /// APU power-on (Pan Docs "Obscure Behavior").
    triggered_once: bool,
    /// Envelope clock state for zombie mode glitch tracking.
    #[serde(default)]
    env_clock_state: EnvelopeClockState,
}

impl Default for Channel1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel1 {
    pub fn new() -> Self {
        Self {
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
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
            sweep_timer: 0,
            sweep_shadow: 0,
            sweep_enabled: false,
            negate_used: false,
            triggered_once: false,
            env_clock_state: EnvelopeClockState::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn length_en(&self) -> bool {
        self.length_en
    }

    /// Output sample in 0.0–1.0 range.
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

    /// Advance the frequency timer by one M-cycle (= 4 T-cycles).
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
                trace_apu!(5; "GB APU CH1 tick duty_pos {} -> {} period=0x{:03X}", old_pos, self.duty_pos, self.freq);
            }
        }
    }

    /// Clock length counter at 256 Hz (Frame Sequencer steps 0/2/4/6).
    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        trace_apu!(3; "GB APU CH1 length_counter={} active={}", self.length_counter, self.length_counter > 0);
        if self.length_counter == 0 {
            self.active = false;
        }
    }

    fn sweep_timer_reload(&self) -> u8 {
        if self.sweep_period > 0 {
            self.sweep_period
        } else {
            8
        }
    }

    /// Clock frequency sweep at 128 Hz (Frame Sequencer steps 2/6).
    pub fn clock_sweep(&mut self) {
        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }
        if self.sweep_timer == 0 {
            self.sweep_timer = self.sweep_timer_reload();
            if self.sweep_enabled && self.sweep_period > 0 {
                let new_freq = self.compute_swept_frequency();
                if self.sweep_negate {
                    self.negate_used = true;
                }
                if new_freq > 2047 {
                    trace_apu!(3; "GB APU CH1 sweep overflow freq=0x{:03X} -> muted", new_freq);
                    self.active = false;
                } else if self.sweep_shift > 0 {
                    trace_apu!(3; "GB APU CH1 sweep update shadow=0x{:03X} -> new=0x{:03X}", self.sweep_shadow, new_freq);
                    self.sweep_shadow = new_freq;
                    self.freq = new_freq;
                    // Re-check overflow against the newly loaded frequency.
                    self.disable_if_sweep_overflows();
                }
            }
        }
    }

    fn compute_swept_frequency(&self) -> u16 {
        let delta = self.sweep_shadow >> self.sweep_shift;
        if self.sweep_negate {
            self.sweep_shadow.wrapping_sub(delta)
        } else {
            self.sweep_shadow + delta
        }
    }

    fn disable_if_sweep_overflows(&mut self) {
        if self.compute_swept_frequency() > 2047 {
            self.active = false;
        }
    }

    /// Clock volume envelope at 64 Hz (Frame Sequencer step 7).
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
                trace_apu!(3; "GB APU CH1 envelope volume {} -> {}", old_volume, self.volume);
            }
        }
    }

    /// Clear envelope clock flag after frame sequencer step completes.
    pub fn clear_envelope_clock(&mut self) {
        self.env_clock_state.clock = false;
    }

    /// Reset channel state when APU is powered off.
    pub fn power_off(&mut self) {
        self.sweep_period = 0;
        self.sweep_negate = false;
        self.sweep_shift = 0;
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
        self.sweep_timer = 0;
        self.sweep_shadow = 0;
        self.sweep_enabled = false;
        self.triggered_once = false;
        self.env_clock_state = EnvelopeClockState::default();
    }

    // ── Register reads ────────────────────────────────────────────────────

    /// NR10 read: bits 6-0 meaningful, bit 7 reads as 1.
    pub fn read_nr10(&self) -> u8 {
        0x80 | ((self.sweep_period & 0x07) << 4)
            | (u8::from(self.sweep_negate) << 3)
            | (self.sweep_shift & 0x07)
    }

    /// NR11 read: only duty bits 7-6 readable; length bits read as 0xFF.
    pub fn read_nr11(&self) -> u8 {
        0x3F | ((self.duty & 0x03) << 6)
    }

    /// NR12 read: all bits readable.
    pub fn read_nr12(&self) -> u8 {
        ((self.init_volume & 0x0F) << 4) | (u8::from(self.env_add) << 3) | (self.env_period & 0x07)
    }

    /// NR14 read: only length-enable bit is readable; others read as 1.
    pub fn read_nr14(&self) -> u8 {
        0xBF | (u8::from(self.length_en) << 6)
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr10(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH1 write NR10=0x{:02X} period={} negate={} shift={}", 
            val, (val >> 4) & 0x07, (val & 0x08) != 0, val & 0x07);
        let old_negate = self.sweep_negate;
        self.sweep_period = (val >> 4) & 0x07;
        self.sweep_negate = val & 0x08 != 0;
        self.sweep_shift = val & 0x07;
        // Hardware quirk: if negate mode was used since last trigger and is now cleared,
        // the channel is immediately disabled.
        if old_negate && !self.sweep_negate && self.negate_used {
            self.active = false;
        }
    }

    pub fn write_nr11(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH1 write NR11=0x{:02X} duty={} length={}", val, (val >> 6) & 0x03, val & 0x3F);
        self.duty = (val >> 6) & 0x03;
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    pub fn write_nr12(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH1 write NR12=0x{:02X} volume={} env_add={} env_period={}", 
            val, (val >> 4) & 0x0F, (val & 0x08) != 0, val & 0x07);

        let old_val = self.read_nr12();

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
    /// Per Pan Docs "Obscure Behavior": writing to NRx2 while channel is playing
    /// can immediately modify the volume based on old and new register values.
    fn apply_nrx2_glitch(&mut self, old_val: u8, new_val: u8) {
        let old_period = old_val & 0x07;
        let new_period = new_val & 0x07;
        let old_direction_add = (old_val & 0x08) != 0;
        let new_direction_add = (new_val & 0x08) != 0;

        // If envelope clock just fired, reload the countdown.
        if self.env_clock_state.clock {
            self.env_timer = new_period;
        }

        // Determine if we should tick volume.
        let mut should_tick =
            (new_period != 0) && (old_period == 0) && !self.env_clock_state.locked;

        // Special case: both $x8 patterns (period=0, add=true) → tick
        if (new_val & 0x0F) == 0x08 && (old_val & 0x0F) == 0x08 && !self.env_clock_state.locked {
            should_tick = true;
        }

        // Check if direction changed.
        let should_invert = old_direction_add != new_direction_add;

        if should_invert {
            let old_volume = self.volume;
            if new_direction_add {
                // Switching to increase mode.
                if old_period == 0 && !self.env_clock_state.locked {
                    self.volume ^= 0x0F;
                } else {
                    self.volume = (0x0E_u8.wrapping_sub(self.volume)) & 0x0F;
                }
                should_tick = false; // Inversion prevents ticking.
            } else {
                // Switching to decrease mode.
                self.volume = (0x10_u8.wrapping_sub(self.volume)) & 0x0F;
            }
            trace_apu!(3; "GB APU CH1 zombie invert volume {} -> {}", old_volume, self.volume);
        }

        if should_tick {
            let old_volume = self.volume;
            if new_direction_add {
                self.volume = (self.volume + 1) & 0x0F;
            } else {
                self.volume = self.volume.wrapping_sub(1) & 0x0F;
            }
            trace_apu!(3; "GB APU CH1 zombie tick volume {} -> {}", old_volume, self.volume);
        } else if new_period == 0 && self.env_clock_state.clock {
            // Clear envelope clock state when period set to 0 during clock.
            self.env_clock_state.clock = false;
        }
    }

    pub fn write_nr13(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
        trace_apu!(2; "GB APU CH1 write NR13=0x{:02X} freq=0x{:03X}", val, self.freq);
    }

    pub fn write_nr14(&mut self, val: u8, extra_clk: bool) {
        trace_apu!(2; "GB APU CH1 write NR14=0x{:02X} trigger={} length_en={} freq_high={}", 
            val, (val & 0x80) != 0, (val & 0x40) != 0, val & 0x07);
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);

        // Extra length clocking: when length_en transitions 0→1 while the FS
        // next step does NOT clock length, the counter is immediately clocked.
        if extra_clk && !old_length_en && self.length_en && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.active = false;
            }
        }

        if val & 0x80 != 0 {
            self.trigger();
            // If trigger reloaded counter to max AND length_en AND extra-clock
            // window, decrement the freshly-loaded counter by 1.
            if extra_clk && self.length_en && self.length_counter == 64 {
                self.length_counter = 63;
            }
        }
    }

    /// Length counter write when APU is powered off (DMG quirk).
    pub fn write_nr11_length_only(&mut self, val: u8) {
        self.length_load = val & 0x3F;
        self.length_counter = 64 - self.length_load;
    }

    // ── Trigger ───────────────────────────────────────────────────────────

    fn trigger(&mut self) {
        trace_apu!(1; "GB APU CH1 trigger freq=0x{:03X} volume={} sweep_period={} sweep_shift={}", 
            self.freq, self.init_volume, self.sweep_period, self.sweep_shift);
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
        // Reset envelope clock state on trigger.
        self.env_clock_state = EnvelopeClockState::default();
        self.sweep_shadow = self.freq;
        self.sweep_timer = self.sweep_timer_reload();
        self.sweep_enabled = self.sweep_period > 0 || self.sweep_shift > 0;
        self.negate_used = false;
        // Trigger-time overflow check required by hardware even when sweep is otherwise idle.
        if self.sweep_shift > 0 {
            if self.sweep_negate {
                self.negate_used = true;
            }
            self.disable_if_sweep_overflows();
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch1() -> Channel1 {
        let mut ch = Channel1::new();
        // NR12: volume=15, add=false, period=0 (no envelope ramp) → DAC on
        ch.write_nr12(0xF0);
        // NR11: 50% duty, length = 0 (max)
        ch.write_nr11(0x80);
        // NR14: trigger, no length enable, freq high = 0
        ch.write_nr14(0x80, false);
        ch
    }

    // ── DAC / active state ────────────────────────────────────────────────

    #[test]
    fn test_trigger_makes_channel_active() {
        // Given: CH1 with DAC on; When: trigger (NR14 bit 7); Then: is_active = true
        let ch = triggered_ch1();
        assert!(ch.is_active());
    }

    #[test]
    fn test_dac_off_prevents_activation() {
        // Given: NR12 = 0x00 (DAC off); When: trigger; Then: channel stays inactive
        let mut ch = Channel1::new();
        ch.write_nr12(0x00); // no volume, no envelope → DAC off
        ch.write_nr14(0x80, false); // trigger
        assert!(!ch.is_active());
    }

    #[test]
    fn test_dac_off_disables_active_channel() {
        // Given: active channel; When: NR12 written to 0x00; Then: channel becomes inactive
        let mut ch = triggered_ch1();
        assert!(ch.is_active());
        ch.write_nr12(0x00);
        assert!(!ch.is_active());
    }

    // ── Length counter ────────────────────────────────────────────────────

    #[test]
    fn test_length_counter_loaded_from_nr11() {
        // NR11 length field = 0x3F (63); counter = 64 - 63 = 1
        let mut ch = Channel1::new();
        ch.write_nr11(0xFF); // duty=11, length=63
        assert_eq!(ch.length_counter, 1);
    }

    #[test]
    fn test_length_counter_expiry_silences_channel_when_enabled() {
        // Given: length counter = 1, length_en = true;
        // When: clock_length once; Then: channel becomes inactive.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0xFF); // length = 63 → counter = 1
        ch.write_nr14(0xC0, false); // trigger + length enable
        assert!(ch.is_active());
        ch.clock_length();
        assert!(
            !ch.is_active(),
            "channel must be silenced when length counter expires"
        );
    }

    #[test]
    fn test_length_counter_does_not_expire_when_disabled() {
        // Given: length counter = 1, length_en = false;
        // When: clock_length; Then: channel remains active.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0xFF); // counter = 1
        ch.write_nr14(0x80, false); // trigger, no length enable
        ch.clock_length();
        assert!(
            ch.is_active(),
            "channel must stay active when length enable is off"
        );
    }

    #[test]
    fn test_trigger_reloads_length_counter_when_zero() {
        // If the length counter reaches 0 before triggering, trigger reloads it to 64.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x3F); // length = 63 → counter = 1
        ch.write_nr14(0x40, false); // length enable, NO trigger → counter = 1
        ch.clock_length(); // expires to 0, channel inactive
        assert!(!ch.is_active());
        // Trigger again – counter should reload to 64.
        ch.write_nr14(0x80, false); // trigger (no length enable)
        assert!(ch.is_active());
        assert_eq!(ch.length_counter, 64);
    }

    // ── Extra length clocking on NRx4 write ──────────────────────────────

    #[test]
    fn test_enabling_length_in_first_half_clocks_length() {
        // Blargg 03-trigger sub-test 3: enabling length_en when the FS next
        // step does NOT clock length must extra-clock the length counter.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0xBE); // length_load=62 → counter = 64-62 = 2
        ch.write_nr14(0x80, false); // trigger, no length enable
        assert_eq!(ch.length_counter, 2);
        // Enable length with extra_clk=true (FS in first half).
        ch.write_nr14(0x40, true); // length enable, no trigger, extra clock
        assert_eq!(
            ch.length_counter, 1,
            "enabling length in first half must extra-clock (2 → 1)"
        );
    }

    #[test]
    fn test_enabling_length_in_second_half_does_not_clock() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0xBE); // counter = 2
        ch.write_nr14(0x80, false); // trigger, no length enable
        // Enable length with extra_clk=false (FS in second half).
        ch.write_nr14(0x40, false); // no extra clock
        assert_eq!(
            ch.length_counter, 2,
            "enabling length in second half must NOT extra-clock"
        );
    }

    #[test]
    fn test_trigger_unfreezes_and_extra_clocks_when_enabled() {
        // Blargg 03-trigger sub-test 8: trigger reloads length to max,
        // and if length_en is set and FS extra-clocks, decrement by 1.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr11(0x3F); // counter = 1
        ch.write_nr14(0x40, true); // enable → extra clock → counter 1→0, channel disabled
        assert!(!ch.is_active());
        // Trigger + length enable with extra clock: counter was 0, reloads to 64,
        // then extra-clock decrements to 63.
        ch.write_nr14(0xC0, true); // trigger + length enable
        assert_eq!(
            ch.length_counter, 63,
            "trigger reload + extra clock: 64 → 63"
        );
    }

    // ── Volume envelope ───────────────────────────────────────────────────

    #[test]
    fn test_envelope_decrements_volume() {
        // NR12: vol=7, dir=subtract, period=1
        let mut ch = Channel1::new();
        ch.write_nr12(0x71); // vol=7, add=0, period=1
        ch.write_nr14(0x80, false); // trigger
        assert_eq!(ch.volume, 7);
        ch.clock_envelope();
        assert_eq!(ch.volume, 6);
    }

    #[test]
    fn test_envelope_increments_volume() {
        // NR12: vol=7, dir=add, period=1
        let mut ch = Channel1::new();
        ch.write_nr12(0x79); // vol=7, add=1, period=1
        ch.write_nr14(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 8);
    }

    #[test]
    fn test_envelope_does_not_go_below_zero() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x01); // vol=0, add=0, period=1
        ch.write_nr14(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 0);
    }

    #[test]
    fn test_envelope_does_not_exceed_15() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF9); // vol=15, add=1, period=1
        ch.write_nr14(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 15);
    }

    #[test]
    fn test_envelope_frozen_when_period_zero() {
        let mut ch = Channel1::new();
        ch.write_nr12(0x70); // vol=7, period=0
        ch.write_nr14(0x80, false);
        ch.clock_envelope();
        assert_eq!(ch.volume, 7, "envelope must not change when period = 0");
    }

    // ── Frequency sweep ───────────────────────────────────────────────────

    #[test]
    fn test_sweep_shifts_freq_up() {
        // NR10: period=1, negate=0, shift=1 → freq doubles each sweep clock
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x11); // period=1, negate=0, shift=1
        ch.write_nr13(0x00);
        ch.write_nr14(0x87, false); // trigger, freq_hi=7 → freq = 0x700 = 1792
        // After trigger, sweep_shadow = 1792; one clock → new_freq = 1792 + (1792>>1) = 1792+896 = 2688 > 2047 → disable
        // Let's use a lower frequency so it doesn't overflow.
        // Reset with freq = 100.
        ch.write_nr13(0x64); // low byte = 100
        ch.write_nr14(0x80, false); // trigger (freq_hi=0) → freq = 100
        let initial_shadow = ch.sweep_shadow;
        assert_eq!(initial_shadow, 100);
        ch.clock_sweep();
        // new_freq = 100 + (100 >> 1) = 100 + 50 = 150
        assert_eq!(
            ch.sweep_shadow, 150,
            "sweep shadow must be updated after sweep clock"
        );
        assert_eq!(ch.freq, 150, "freq must be updated after sweep clock");
    }

    #[test]
    fn test_sweep_shifts_freq_down() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x19); // period=1, negate=1, shift=1
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false);
        ch.clock_sweep();
        // new_freq = 100 - (100 >> 1) = 50
        assert_eq!(ch.sweep_shadow, 50);
        assert_eq!(ch.freq, 50);
    }

    #[test]
    fn test_sweep_overflow_disables_channel() {
        // freq = 2000, shift = 1, negate = false → next = 2000 + 1000 = 3000 > 2047
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x11); // period=1, negate=0, shift=1
        // freq 2000 = 0x7D0 → hi=7, lo=0xD0
        ch.write_nr13(0xD0);
        ch.write_nr14(0x87, false); // trigger, hi=7
        assert_eq!(ch.freq, 0x7D0);
        ch.clock_sweep();
        assert!(
            !ch.is_active(),
            "channel must be disabled on sweep overflow"
        );
    }

    #[test]
    fn test_sweep_disabled_when_period_and_shift_zero() {
        // NR10 = 0x00: period=0, shift=0 → sweep disabled; freq stays constant.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr10(0x00);
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false);
        ch.clock_sweep();
        assert_eq!(ch.freq, 100, "freq must not change when sweep is disabled");
        assert!(ch.is_active());
    }

    // ── NR10/NR11/NR12/NR14 read back ────────────────────────────────────

    #[test]
    fn test_nr10_read_back() {
        let mut ch = Channel1::new();
        ch.write_nr10(0x5E); // period=5, negate=1, shift=6
        // NR10 read: bit 7 always 1; bits 6-4 = period; bit 3 = negate; bits 2-0 = shift
        let r = ch.read_nr10();
        assert_eq!(r & 0x7F, 0x5E & 0x7F);
    }

    #[test]
    fn test_nr11_read_returns_duty_only() {
        let mut ch = Channel1::new();
        ch.write_nr11(0xBF); // duty=10, length=63
        // Bits 7-6 must match; bits 5-0 always read as 1
        assert_eq!(ch.read_nr11() >> 6, 0b10);
        assert_eq!(ch.read_nr11() & 0x3F, 0x3F);
    }

    #[test]
    fn test_nr12_read_back() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF3); // vol=15, add=1, period=3
        assert_eq!(ch.read_nr12(), 0xF3);
    }

    #[test]
    fn test_nr14_reads_length_en_bit() {
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0);
        ch.write_nr14(0x40, false); // length_en=1, no trigger
        assert_eq!(ch.read_nr14() & 0x40, 0x40);
        ch.write_nr14(0x00, false);
        assert_eq!(ch.read_nr14() & 0x40, 0x00);
    }

    // ── Sweep correctness (hardware quirks) ──────────────────────────────

    #[test]
    fn test_clock_sweep_disables_channel_on_overflow_when_shift_zero() {
        // Given: sweep period=1, negate=false, shift=0 and freq=1400;
        // When: clock_sweep fires;
        // Then: new_freq = 1400+1400 = 2800 > 2047 → channel disabled even though shift=0.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        // NR10: period=1 (0x10>>4 & 0x07 = 1), negate=0, shift=0
        ch.write_nr10(0x10);
        // freq = 1400 = 0x578 → lo=0x78, hi=0x05
        ch.write_nr13(0x78);
        ch.write_nr14(0x85, false); // trigger + freq_hi=5
        assert!(ch.is_active(), "channel must be active after trigger");
        // sweep fires: timer 1→0, reload=1, enabled=true, period=1
        // new_freq = 1400 + (1400>>0) = 2800 > 2047 → must disable
        ch.clock_sweep();
        assert!(
            !ch.is_active(),
            "channel must be disabled when sweep overflows even with shift=0"
        );
    }

    #[test]
    fn test_disabling_negate_after_calculation_disables_channel() {
        // Given: CH1 with sweep period=1, negate=true, shift=1, freq=100;
        //   After trigger and one sweep clock, a negate calculation is done (negate_used=true);
        // When: NR10 is written clearing negate;
        // Then: channel is disabled (hardware "negate-used" quirk).
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // DAC on
        // NR10: period=1, negate=1, shift=1 → 0x19
        ch.write_nr10(0x19);
        ch.write_nr13(0x64); // freq = 100
        ch.write_nr14(0x80, false); // trigger
        assert!(ch.is_active(), "channel must be active after trigger");
        // One sweep clock in negate mode → negate_used must be set
        ch.clock_sweep();
        assert!(
            ch.is_active(),
            "channel still active after negate calculation"
        );
        // Clear negate in NR10: period=1, negate=0, shift=1 → 0x11
        ch.write_nr10(0x11);
        assert!(
            !ch.is_active(),
            "channel must be disabled when negate is cleared after a negate calculation"
        );
    }

    #[test]
    fn test_trigger_overflow_check_sets_negate_used() {
        // Blargg 05-sweep details, sub-test 4:
        // NR10=$09 (period=0, negate=1, shift=1): trigger performs a negate-mode
        // overflow check (shift>0). Even though sweep_period=0 prevents periodic
        // clocking, the trigger-time calculation uses negate mode, so `negate_used`
        // must be set. Clearing negate in NR10 afterwards must disable the channel.
        let mut ch = Channel1::new();
        ch.write_nr12(0x08); // vol=0, add=1, period=0 → DAC on (0x08 & 0xF8 = 0x08 ≠ 0)
        ch.write_nr10(0x09); // period=0, negate=true, shift=1
        ch.write_nr13(0x00); // freq low = 0
        ch.write_nr14(0xC0, false); // trigger + length enable; freq_hi = 0
        assert!(ch.is_active(), "channel must be active after trigger");
        // No sweep clock needed — the trigger itself performed a negate calculation.
        // Now clear negate: period=1, negate=false, shift=0 → $10
        ch.write_nr10(0x10);
        assert!(
            !ch.is_active(),
            "channel must be disabled: negate was used during trigger overflow check"
        );
    }

    // ── Output level ──────────────────────────────────────────────────────

    #[test]
    fn test_output_is_zero_when_inactive() {
        let ch = Channel1::new();
        assert_eq!(ch.output(), 0.0);
    }

    #[test]
    fn test_output_nonzero_when_active_duty_high() {
        // 50% duty at position 0 is high.
        let mut ch = Channel1::new();
        ch.write_nr12(0xF0); // vol=15, DAC on
        ch.write_nr11(0x80); // 50% duty
        ch.write_nr14(0x80, false); // trigger
        ch.duty_pos = 0; // 50% duty: step 0 → high
        // DUTY_TABLE[2][0] = 1
        assert!(
            ch.output() > 0.0,
            "output must be positive at duty-high step"
        );
    }

    #[test]
    fn test_duty_phase_is_not_clocked_before_first_trigger() {
        // Pan Docs: just after APU power-on, pulse duty clocking is disabled
        // until first trigger. Verify CH1 duty phase does not advance before
        // first trigger, then advances normally afterwards.
        let mut ch = Channel1::new();

        for _ in 0..4096 {
            ch.tick();
        }
        assert_eq!(
            ch.duty_pos, 0,
            "duty phase should remain at reset position before first trigger"
        );

        ch.write_nr12(0xF0); // DAC on
        ch.write_nr11(0x80); // 50% duty
        ch.write_nr14(0x80, false); // trigger

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
