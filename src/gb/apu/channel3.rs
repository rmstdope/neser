//! CH3 – Wave output channel (NR30–NR34, wave RAM $FF30–$FF3F).
//!
//! Wave RAM holds 32 × 4-bit samples packed in 16 bytes. The output level
//! register (NR32 bits 6-5) right-shifts the sample before output:
//!
//! | NR32 bits 6-5 | Shift | Effective output |
//! |---|---|---|
//! | 00 | mute   | 0 (silent)       |
//! | 01 | 0      | 100 %            |
//! | 10 | 1      | 50 %             |
//! | 11 | 2      | 25 %             |

use crate::trace_apu;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel3 {
    dac_on: bool,     // NR30 bit 7
    length_load: u16, // NR31 (0-255); length_counter = 256 - load
    output_level: u8, // NR32 bits 6-5 (0-3)
    freq: u16,        // NR33 + NR34 bits 2-0 (11-bit)
    length_en: bool,  // NR34 bit 6

    active: bool,
    wave_pos: u8,                   // 0-31 (position into 32-sample wave table)
    freq_timer: u16,                // countdown in APU cycles (2 MHz); reload = freq ^ 0x7FF
    pub(crate) length_counter: u16, // 0-256
    /// Wave RAM: 16 bytes = 32 × 4-bit samples.
    wave_ram: [u8; 16],
    /// Byte currently being shifted out (set on wave position advance).
    pub(crate) current_sample: u8,
    /// True when running a CGB-compatible ROM (gates wave RAM access behavior).
    is_cgb: bool,
    /// CGB-B CH3 keeps its active flag (reflected in NR52 bit 2) for one extra
    /// NRx4 write when this quirk clocks length from 1 to 0 with length-enable clear.
    #[serde(default)]
    cgb_b_length_disable_pending: bool,
    /// True when the last wave position advance consumed all remaining APU cycles
    /// in a tick — i.e. a sample read occurred on the very last APU cycle of that
    /// M-cycle. On DMG, CPU can only access wave RAM during this window.
    pub(crate) wave_just_read: bool,
}

impl Default for Channel3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel3 {
    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn new_with_mode(is_cgb: bool) -> Self {
        Self {
            dac_on: false,
            length_load: 0,
            output_level: 0,
            freq: 0,
            length_en: false,
            active: false,
            wave_pos: 0,
            freq_timer: 0,
            length_counter: 0,
            wave_ram: [0u8; 16],
            current_sample: 0,
            is_cgb,
            cgb_b_length_disable_pending: false,
            wave_just_read: false,
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
        let shift = match self.output_level {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => return 0.0, // output_level 0 = mute
        };
        (self.current_sample >> shift) as f32 / 15.0
    }

    /// Digital output (0-15) before DAC conversion (for PCM34 register).
    pub fn digital_output(&self) -> u8 {
        if !self.active || !self.dac_on {
            return 0;
        }
        let shift = match self.output_level {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => return 0, // output_level 0 = mute
        };
        self.current_sample >> shift
    }

    /// Advance the wave frequency timer by one M-cycle (= 2 APU cycles at 2 MHz).
    /// Process all sample advances within the given M-cycle.
    /// The countdown is in APU cycles; reload = `freq ^ 0x7FF` = `2047 - freq`.
    pub fn tick(&mut self) {
        self.tick_apu_cycles(2);
    }

    fn tick_apu_cycles(&mut self, cycles: u16) {
        self.wave_just_read = false;

        if !self.active {
            return;
        }

        let mut cycles_left = cycles;
        while cycles_left > self.freq_timer {
            cycles_left -= self.freq_timer + 1;
            self.freq_timer = self.freq ^ 0x7FF;
            let old_pos = self.wave_pos;
            self.wave_pos = (self.wave_pos + 1) & 31;
            self.current_sample = self.read_wave_nibble(self.wave_pos);
            trace_apu!(5; "GB APU CH3 tick wave_pos {} -> {} sample=0x{:X} freq=0x{:03X}", 
                old_pos, self.wave_pos, self.current_sample, self.freq);
            self.wave_just_read = true;
        }
        if cycles_left > 0 {
            self.freq_timer -= cycles_left;
            self.wave_just_read = false;
        }
    }

    pub fn clock_length(&mut self) {
        if !self.length_en || self.length_counter == 0 {
            return;
        }
        self.length_counter -= 1;
        trace_apu!(3; "GB APU CH3 length_counter={} active={}", self.length_counter, self.length_counter > 0);
        if self.length_counter == 0 {
            self.active = false;
        }
    }

    pub fn power_off(&mut self) {
        self.dac_on = false;
        self.length_load = 0;
        self.output_level = 0;
        self.freq = 0;
        self.length_en = false;
        self.active = false;
        self.wave_pos = 0;
        self.freq_timer = 0;
        self.length_counter = 0;
        self.current_sample = 0;
        self.cgb_b_length_disable_pending = false;
        self.wave_just_read = false;
        // Note: wave_ram is NOT cleared on power-off per hardware spec.
    }

    // ── Register reads ────────────────────────────────────────────────────

    pub fn read_nr30(&self) -> u8 {
        0x7F | (u8::from(self.dac_on) << 7)
    }

    pub fn read_nr32(&self) -> u8 {
        0x9F | ((self.output_level & 0x03) << 5)
    }

    pub fn read_nr34(&self) -> u8 {
        0xBF | (u8::from(self.length_en) << 6)
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr30(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH3 write NR30=0x{:02X} dac_on={}", val, (val & 0x80) != 0);
        self.dac_on = val & 0x80 != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    pub fn write_nr31(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH3 write NR31=0x{:02X} length={}", val, val);
        self.length_load = u16::from(val);
        self.length_counter = 256 - self.length_load;
    }

    pub fn write_nr32(&mut self, val: u8) {
        trace_apu!(2; "GB APU CH3 write NR32=0x{:02X} output_level={}", val, (val >> 5) & 0x03);
        self.output_level = (val >> 5) & 0x03;
    }

    pub fn write_nr33(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
        trace_apu!(2; "GB APU CH3 write NR33=0x{:02X} freq=0x{:03X}", val, self.freq);
    }

    pub fn write_nr34(&mut self, val: u8, extra_clk: bool) {
        self.write_nr34_with_length_quirk(val, extra_clk, false, false);
    }

    pub fn write_nr34_with_length_quirk(
        &mut self,
        val: u8,
        extra_clk: bool,
        cgb_early_extra_length_clock: bool,
        cgb_b_delayed_length_disable: bool,
    ) {
        let trigger = val & 0x80 != 0;
        trace_apu!(2; "GB APU CH3 write NR34=0x{:02X} trigger={} length_en={} freq_high={}", 
            val, trigger, (val & 0x40) != 0, val & 0x07);
        // The CGB-B pending disable is consumed by the next non-trigger NR34
        // write before that write can evaluate length-enable or clock length.
        if self.cgb_b_length_disable_pending && !trigger {
            self.active = false;
            self.cgb_b_length_disable_pending = false;
        }
        let old_length_en = self.length_en;
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);
        let clocks_length_on_extra = self.length_en || cgb_early_extra_length_clock;

        if extra_clk && !old_length_en && clocks_length_on_extra && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                let should_delay_disable =
                    cgb_b_delayed_length_disable && !self.length_en && !trigger;
                if should_delay_disable {
                    self.cgb_b_length_disable_pending = true;
                } else {
                    self.active = false;
                    self.cgb_b_length_disable_pending = false;
                }
            }
        }

        if trigger {
            self.trigger();
            self.cgb_b_length_disable_pending = false;
            if extra_clk && clocks_length_on_extra && self.length_counter == 256 {
                self.length_counter = 255;
            }
        }
    }

    pub fn write_nr31_length_only(&mut self, val: u8) {
        self.length_load = u16::from(val);
        self.length_counter = 256 - self.length_load;
    }

    fn trigger(&mut self) {
        trace_apu!(1; "GB APU CH3 trigger freq=0x{:03X} output_level={}", self.freq, self.output_level);
        // DMG retrigger corruption: if CH3 is currently active on DMG
        // and sample_countdown == 0 (about to read next sample)
        if !self.is_cgb && self.active && self.freq_timer == 0 {
            self.apply_dmg_retrigger_corruption();
        }

        self.wave_pos = 0;

        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        // Pan Docs: "triggering does not immediately start playing wave RAM;
        // the last sample ever read is output until the channel next reads a sample."

        // Trigger countdown: (freq ^ 0x7FF) + 3 APU cycles = (2047 - freq) + 3
        self.freq_timer = (self.freq ^ 0x7FF) + 3;
    }

    /// DMG-only: wave RAM corruption when retriggering CH3 while it just read a sample.
    /// Wave RAM offset: `offset = ((current_sample_index + 1) >> 1) & 0xF`.
    /// If offset < 4: wave_ram[0] = wave_ram[offset].
    /// If offset >= 4: wave_ram[0..4] = wave_ram[aligned..aligned+4] where aligned = offset & !3.
    fn apply_dmg_retrigger_corruption(&mut self) {
        let current_byte_index = ((self.wave_pos.wrapping_add(1)) / 2) & 0x0F;
        if current_byte_index < 4 {
            // Copy the single byte at current_byte_index to byte 0
            self.wave_ram[0] = self.wave_ram[current_byte_index as usize];
        } else {
            // Copy the 4-byte aligned block to bytes 0-3
            let aligned = (current_byte_index & !3) as usize;
            let block = [
                self.wave_ram[aligned],
                self.wave_ram[aligned + 1],
                self.wave_ram[aligned + 2],
                self.wave_ram[aligned + 3],
            ];
            self.wave_ram[0] = block[0];
            self.wave_ram[1] = block[1];
            self.wave_ram[2] = block[2];
            self.wave_ram[3] = block[3];
        }
    }

    // ── Wave RAM ──────────────────────────────────────────────────────────

    /// Extract the 4-bit sample at a given wave position from packed wave RAM.
    fn read_wave_nibble(&self, pos: u8) -> u8 {
        let byte = self.wave_ram[(pos / 2) as usize];
        if pos & 1 == 0 {
            (byte >> 4) & 0x0F
        } else {
            byte & 0x0F
        }
    }

    pub(crate) fn needs_cgb_read_sync(&self) -> bool {
        self.is_cgb && self.active && self.freq_timer == 0
    }

    pub(crate) fn sync_cgb_read_tick(&mut self) {
        if self.needs_cgb_read_sync() {
            self.tick_apu_cycles(2);
        }
    }

    pub fn read_wave_ram(&self, addr: u16) -> u8 {
        if self.active {
            if self.is_cgb || self.wave_just_read {
                // CGB: always return the byte at current wave position during playback.
                // DMG: only accessible during the M-cycle when CH3 reads wave RAM.
                self.wave_ram[(self.wave_pos / 2) as usize]
            } else {
                // DMG: outside the access window, reads return 0xFF.
                0xFF
            }
        } else {
            self.wave_ram[(addr - 0xFF30) as usize]
        }
    }

    pub fn write_wave_ram(&mut self, addr: u16, val: u8) {
        trace_apu!(2; "GB APU CH3 write wave_ram[0x{:04X}]=0x{:02X}", addr, val);
        if self.active {
            if self.is_cgb || self.wave_just_read {
                // CGB: always write to current wave position during playback.
                // DMG: only accessible during the M-cycle when CH3 reads wave RAM.
                self.wave_ram[(self.wave_pos / 2) as usize] = val;
            }
            // DMG: outside the access window, writes are ignored.
        } else {
            self.wave_ram[(addr - 0xFF30) as usize] = val;
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn triggered_ch3() -> Channel3 {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80); // DAC on
        ch.write_nr32(0x20); // output level = 1 (100%)
        ch.write_nr34(0x80, false); // trigger
        ch
    }

    // ── DAC / active state ────────────────────────────────────────────────

    #[test]
    fn test_trigger_makes_channel_active() {
        let ch = triggered_ch3();
        assert!(ch.is_active());
    }

    #[test]
    fn test_dac_off_prevents_activation() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x00); // DAC off
        ch.write_nr34(0x80, false); // trigger
        assert!(!ch.is_active());
    }

    #[test]
    fn test_dac_off_disables_active_channel() {
        let mut ch = triggered_ch3();
        ch.write_nr30(0x00);
        assert!(!ch.is_active());
    }

    // ── Length counter ────────────────────────────────────────────────────

    #[test]
    fn test_length_counter_loaded_from_nr31() {
        // NR31 = 255 → counter = 256 - 255 = 1
        let mut ch = Channel3::new();
        ch.write_nr31(255);
        assert_eq!(ch.length_counter, 1);
    }

    #[test]
    fn test_length_expiry_silences_when_enabled() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr31(255); // counter = 1
        ch.write_nr34(0xC0, false); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_no_expire_when_disabled() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr31(255);
        ch.write_nr34(0x80, false); // trigger, no length enable
        ch.clock_length();
        assert!(ch.is_active());
    }

    // ── Output level ──────────────────────────────────────────────────────

    #[test]
    fn test_output_muted_when_level_0() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x00); // level = 0 → mute
        ch.write_nr34(0x80, false);
        // Put a non-zero sample in wave RAM at position 0
        ch.wave_ram[0] = 0xFF;
        ch.trigger();
        assert_eq!(ch.output(), 0.0);
    }

    #[test]
    fn test_output_100_percent() {
        // Use freq=0 so period=(0^0x7FF)+3=2050 T-cycles on trigger.
        // After trigger, tick many times to get first advance.
        // Simpler: directly set current_sample and check output.
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x20); // level = 1 → 100%
        ch.active = true;
        ch.current_sample = 15;
        assert!((ch.output() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_output_50_percent() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x40); // level = 2 → 50% (shift right 1)
        ch.active = true;
        ch.current_sample = 15; // 15 >> 1 = 7
        // 7 / 15 ≈ 0.4667
        assert!((ch.output() - 7.0 / 15.0).abs() < 0.001);
    }

    #[test]
    fn test_output_25_percent() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x60); // level = 3 → 25% (shift right 2)
        ch.active = true;
        ch.current_sample = 15; // 15 >> 2 = 3
        assert!((ch.output() - 3.0 / 15.0).abs() < 0.001);
    }

    // ── Wave RAM ──────────────────────────────────────────────────────────

    #[test]
    fn test_wave_ram_write_read_roundtrip() {
        let mut ch = Channel3::new();
        ch.write_wave_ram(0xFF30, 0xAB);
        assert_eq!(ch.read_wave_ram(0xFF30), 0xAB);
        ch.write_wave_ram(0xFF3F, 0xCD);
        assert_eq!(ch.read_wave_ram(0xFF3F), 0xCD);
    }

    #[test]
    fn test_wave_ram_read_returns_current_byte_during_playback_cgb() {
        // On CGB, during active playback, reads always return the byte at wave_pos/2.
        let mut ch = Channel3::new_with_mode(true); // CGB
        ch.write_nr30(0x80);
        ch.wave_ram[0] = 0x12; // wave_pos 0-1 → byte 0
        ch.wave_ram[1] = 0x34;
        ch.write_nr34(0x80, false); // trigger → wave_pos = 0
        assert!(ch.is_active());
        // CGB: read any address during playback → returns wave_ram[wave_pos/2] = wave_ram[0]
        assert_eq!(
            ch.read_wave_ram(0xFF3F),
            0x12,
            "CGB: during playback, wave RAM read must return current byte"
        );
    }

    #[test]
    fn test_wave_ram_read_returns_ff_outside_window_dmg() {
        // On DMG, during active playback, reads return 0xFF unless wave_just_read.
        let mut ch = Channel3::new_with_mode(false); // DMG
        ch.write_nr30(0x80);
        ch.wave_ram[0] = 0x12;
        ch.active = true;
        ch.freq_timer = 10; // not about to read
        // wave_just_read is false → returns 0xFF
        assert_eq!(ch.read_wave_ram(0xFF30), 0xFF);
    }

    // ── NR30/NR32/NR34 register reads ────────────────────────────────────

    #[test]
    fn test_nr30_reads_dac_bit() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        assert_eq!(ch.read_nr30() & 0x80, 0x80);
        ch.write_nr30(0x00);
        assert_eq!(ch.read_nr30() & 0x80, 0x00);
    }

    #[test]
    fn test_nr32_reads_output_level() {
        let mut ch = Channel3::new();
        ch.write_nr32(0x60); // level = 3
        assert_eq!((ch.read_nr32() >> 5) & 0x03, 3);
    }

    #[test]
    fn test_nr34_reads_length_en() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr34(0x40, false); // length en, no trigger
        assert_eq!(ch.read_nr34() & 0x40, 0x40);
    }

    // ── Trigger playback delay ────────────────────────────────────────────

    #[test]
    fn test_trigger_playback_delay_first_advance_at_tick_3_for_max_freq() {
        // trigger countdown = (freq ^ 0x7FF) + 3 = (2046 ^ 0x7FF) + 3 = 4 APU cycles.
        // tick() processes 2 APU cycles per M-cycle using `while (cycles_left > countdown)`.
        // Tick 1: cl=2, cd=4. 2>4? No. cd=2.
        // Tick 2: cl=2, cd=2. 2>2? No. cd=0.
        // Tick 3: cl=2, cd=0. 2>0? Yes! Advance pos to 1. Reload=1. 1>1? No. cd=0.
        // So first advance happens at tick 3 (the +3 delay in APU cycles).
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.wave_ram = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
            0xCD, 0xEF,
        ];
        ch.write_nr33(0xFE); // freq low = 0xFE
        ch.write_nr34(0x87, false); // trigger, freq high = 7 → freq = 0x07FE = 2046

        assert_eq!(ch.wave_pos, 0, "wave_pos must be 0 after trigger");

        ch.tick(); // M-cycle 1: cd 4→2, no advance
        assert_eq!(ch.wave_pos, 0, "wave_pos must still be 0 at tick 1");

        ch.tick(); // M-cycle 2: cd 2→0, no advance
        assert_eq!(ch.wave_pos, 0, "wave_pos must still be 0 at tick 2");

        ch.tick(); // M-cycle 3: 2>0, advance to pos 1
        assert_eq!(ch.wave_pos, 1, "wave_pos must advance to 1 at tick 3");
    }

    #[test]
    fn test_trigger_does_not_immediately_read_sample() {
        // Pan Docs: "triggering does not immediately start playing wave RAM;
        // the last sample ever read is output until next read."
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x20); // 100% volume
        ch.wave_ram[0] = 0xF0; // nibble 0 = 0xF, nibble 1 = 0x0
        // current_sample should be 0 (default, as if APU was just powered on)
        ch.write_nr34(0x80, false); // trigger
        // Output should still use the OLD sample (0), not nibble at pos 0
        assert_eq!(
            ch.output(),
            0.0,
            "trigger must not immediately read new sample"
        );
    }

    #[test]
    fn test_first_sample_read_is_index_1() {
        // Pan Docs: "the first sample read is the one at index 1"
        // After trigger (wave_pos=0), the first timer expiry reads nibble at pos 1.
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x20); // 100% volume
        ch.wave_ram[0] = 0xA5; // nibble 0 = 0xA, nibble 1 = 0x5
        ch.write_nr33(0xFE);
        ch.write_nr34(0x87, false); // trigger, freq=2046

        // With 2 APU cycles/tick: countdown=4. Tick 1: cd=2. Tick 2: cd=0. Tick 3: advance to pos 1.
        ch.tick(); // cd 4→2
        ch.tick(); // cd 2→0
        ch.tick(); // advance to pos 1, current_sample = nibble 1 = 0x5
        assert_eq!(ch.wave_pos, 1, "first advance should reach pos 1");
        assert_eq!(
            ch.current_sample, 0x5,
            "first sample read must be nibble at index 1"
        );
    }

    #[test]
    fn test_cgb_read_sync_needed_at_sample_boundary() {
        let mut ch = Channel3::new_with_mode(true);
        ch.write_nr30(0x80);
        ch.active = true;
        ch.freq_timer = 0;

        assert!(
            ch.needs_cgb_read_sync(),
            "CGB CH3 reads should sync a pending half APU tick when the sample timer is at the boundary"
        );
    }

    #[test]
    fn test_cgb_read_sync_not_needed_before_sample_boundary() {
        let mut ch = Channel3::new_with_mode(true);
        ch.write_nr30(0x80);
        ch.active = true;
        ch.freq_timer = 1;

        assert!(
            !ch.needs_cgb_read_sync(),
            "CGB CH3 reads before the sample boundary must not advance early"
        );
    }

    #[test]
    fn test_dmg_read_sync_not_needed_at_sample_boundary() {
        let mut ch = Channel3::new_with_mode(false);
        ch.write_nr30(0x80);
        ch.active = true;
        ch.freq_timer = 0;

        assert!(
            !ch.needs_cgb_read_sync(),
            "the CGB read-sync path must stay disabled for DMG wave RAM behavior"
        );
    }

    #[test]
    fn test_cgb_read_sync_advances_ch3_without_waiting_for_global_tick() {
        let mut ch = Channel3::new_with_mode(true);
        ch.write_nr30(0x80);
        ch.write_nr32(0x20);
        ch.wave_ram[0] = 0xA5;
        ch.active = true;
        ch.freq = 0x07FE;
        ch.freq_timer = 0;

        ch.sync_cgb_read_tick();

        assert_eq!(ch.wave_pos, 1);
        assert_eq!(ch.current_sample, 0x5);
        assert_eq!(
            ch.freq_timer, 0,
            "sync consumes one CH3 tick so boundary reads observe the reloaded timer"
        );
    }

    // ── DMG retrigger corruption ──────────────────────────────────────────

    #[test]
    fn test_dmg_retrigger_corrupts_wave_ram_high_position() {
        // On DMG, retriggering CH3 while sample_countdown == 0
        // causes wave RAM corruption. offset = ((wave_pos+1) >> 1) & 0xF.
        // For offset >= 4: first 4 bytes get the 4-byte-aligned block.
        let mut ch = Channel3::new_with_mode(false); // DMG mode
        ch.write_nr30(0x80);
        ch.wave_ram = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ];

        // Set up channel as if it's playing and freq_timer just reached 0
        ch.active = true;
        ch.wave_pos = 9; // offset = ((9+1)>>1) & 0xF = 5. Aligned block = [4..8]
        ch.freq_timer = 0;
        ch.freq = 0;

        // Retrigger while freq_timer == 0 → should corrupt
        ch.write_nr34(0x80, false);

        // offset=5, aligned=4 → copy wave_ram[4..8] to wave_ram[0..4]
        assert_eq!(ch.wave_ram[0], 0x44, "byte 0 should be wave_ram[4]");
        assert_eq!(ch.wave_ram[1], 0x55, "byte 1 should be wave_ram[5]");
        assert_eq!(ch.wave_ram[2], 0x66, "byte 2 should be wave_ram[6]");
        assert_eq!(ch.wave_ram[3], 0x77, "byte 3 should be wave_ram[7]");
    }

    #[test]
    fn test_cgb_retrigger_does_not_corrupt_wave_ram() {
        // On CGB, retriggering does NOT corrupt wave RAM.
        let mut ch = Channel3::new_with_mode(true); // CGB mode
        ch.write_nr30(0x80);
        ch.wave_ram = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ];
        ch.active = true;
        ch.wave_pos = 9;
        ch.freq_timer = 0;
        ch.freq = 0;
        // Retrigger — no corruption on CGB
        ch.write_nr34(0x80, false);
        assert_eq!(ch.wave_ram[0], 0x00);
        assert_eq!(ch.wave_ram[1], 0x11);
        assert_eq!(ch.wave_ram[2], 0x22);
        assert_eq!(ch.wave_ram[3], 0x33);
    }
}
