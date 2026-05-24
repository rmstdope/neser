// ── CH3 — Wave ────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// Number of 4-bit samples per wave RAM bank.
pub(super) const SAMPLES_PER_BANK: u8 = 32;

/// Channel 3: wave playback (32 × 4-bit samples, or 64 × 4-bit samples in two-bank mode).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Channel3 {
    pub dac_on: bool,
    /// Bit 5 of SOUND3CNT_L: 0 = one bank (32 samples), 1 = two banks (64 samples).
    pub two_banks: bool,
    /// Bit 6 of SOUND3CNT_L: selects which bank (0 or 1) is played back.
    /// The OTHER bank is accessible via Wave RAM register reads/writes.
    pub bank_select: bool,
    pub length_counter: u16,
    pub output_level: u8, // 0=mute, 1=100%, 2=50%, 3=25%
    /// Bit 15 of SOUND3CNT_H: when true, forces 75% volume regardless of output_level.
    pub force_volume: bool,
    pub freq: u16,
    pub length_en: bool,

    pub active: bool,
    pub wave_pos: u8, // 0-31 (single bank) or 0-63 (two banks)
    pub freq_timer: u32,
    /// Wave RAM: 2 banks × 16 bytes = 32 bytes total (64 × 4-bit samples).
    pub wave_ram: [[u8; 16]; 2],
    /// Current 4-bit output nibble.
    pub current_sample: u8,
}

impl Channel3 {
    /// Analogue output in `[-1.0, +1.0]`; 0.0 when inactive, DAC is off
    /// (disconnected), or output_level mutes the channel.
    ///
    /// Per GBATek, the PSG DAC converts digital value D (0–15) to bipolar:
    ///   output = (D / 7.5) − 1.0
    pub fn output(&self) -> f32 {
        if !self.active || !self.dac_on {
            return 0.0;
        }
        // Bit 15 of SOUND3CNT_H: force 75% volume regardless of output_level.
        let d = if self.force_volume {
            ((self.current_sample as u16 * 3) / 4) as f32
        } else {
            let shift: u8 = match self.output_level {
                1 => 0, // 100 %
                2 => 1, // 50 %
                3 => 2, // 25 %
                _ => return 0.0,
            };
            (self.current_sample >> shift) as f32
        };
        d / 7.5 - 1.0
    }

    /// Advance channel by `cycles` GBA cycles.
    /// Each wave_pos step takes (2048 − freq) × 8 GBA cycles.
    pub fn tick(&mut self, cycles: u32) {
        if !self.active {
            return;
        }
        let period = (2048_u32.wrapping_sub(self.freq as u32)) * 8;
        if period == 0 {
            return;
        }
        // Use a bitmask to wrap wave_pos: single-bank → mask 0x1F (& 31), two-bank → 0x3F (& 63).
        let pos_mask: u8 = if self.two_banks { 0x3F } else { 0x1F };
        let mut rem = cycles;
        while rem > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = period;
            }
            let advance = rem.min(self.freq_timer);
            self.freq_timer -= advance;
            rem -= advance;
            if self.freq_timer == 0 {
                self.wave_pos = (self.wave_pos + 1) & pos_mask;
                // Samples 0..(SAMPLES_PER_BANK-1) come from bank_select; the rest from the other bank.
                let bank = if self.wave_pos < SAMPLES_PER_BANK {
                    self.bank_select as usize
                } else {
                    (self.bank_select as usize) ^ 1
                };
                let pos_in_bank = self.wave_pos & (SAMPLES_PER_BANK - 1);
                let byte = self.wave_ram[bank][(pos_in_bank / 2) as usize];
                self.current_sample = if pos_in_bank & 1 == 0 {
                    (byte >> 4) & 0x0F // high nibble
                } else {
                    byte & 0x0F // low nibble
                };
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

    fn trigger(&mut self) {
        self.active = self.dac_on;
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        let period = (2048_u32.wrapping_sub(self.freq as u32)) * 8;
        self.freq_timer = period;
        self.wave_pos = 0;
        // Read first sample from the playing bank (position 0 = high nibble of byte 0).
        let bank = self.bank_select as usize;
        self.current_sample = (self.wave_ram[bank][0] >> 4) & 0x0F;
    }

    // ── Register writes ───────────────────────────────────────────────────

    /// SOUND3CNT_L: wave RAM dimension (bit 5), bank select (bit 6), DAC enable (bit 7).
    pub fn write_cnt_l(&mut self, val: u16) {
        self.two_banks = (val & 0x0020) != 0;
        self.bank_select = (val & 0x0040) != 0;
        self.dac_on = (val & 0x0080) != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    /// SOUND3CNT_H: sound length (bits 7-0), output level (bits 14-13), force volume (bit 15).
    pub fn write_cnt_h(&mut self, val: u16) {
        // Lower byte = sound length (0-255, counter = 256 - length)
        self.length_counter = 256 - (val & 0x00FF);
        // Bits 14-13: output level
        self.output_level = ((val >> 13) & 0x03) as u8;
        // Bit 15: force 75% volume regardless of output_level
        self.force_volume = (val & 0x8000) != 0;
    }

    /// SOUND3CNT_X: frequency and trigger.
    ///
    /// `extra_clk` — see Channel1::write_cnt_x for the full description.
    /// Note: CH3's maximum length counter is 256 (not 64).
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
            // CH3 reloads to 256 (not 64).
            if extra_clk && self.length_en && reloaded_length {
                self.length_counter = 255;
            }
        }
    }

    /// Write one byte to wave RAM (address offset 0x00–0x0F from wave RAM base).
    /// Always accesses the bank NOT currently selected for playback.
    pub fn write_wave_ram(&mut self, offset: usize, val: u8) {
        let other_bank = (self.bank_select as usize) ^ 1;
        if offset < 16 {
            self.wave_ram[other_bank][offset] = val;
        }
    }

    /// Read one byte from wave RAM.
    /// Always accesses the bank NOT currently selected for playback.
    ///
    /// Per mGBA / GBATek: the shift-register rotation only physically affects the
    /// **playing** bank. The non-playing bank is never rotated, so CPU reads always
    /// return the raw stored byte at the given offset regardless of playback state.
    pub fn read_wave_ram(&self, offset: usize) -> u8 {
        let other_bank = (self.bank_select as usize) ^ 1;
        if offset >= 16 {
            return 0xFF;
        }
        self.wave_ram[other_bank][offset]
    }

    pub fn power_off(&mut self) {
        let wave_ram = self.wave_ram;
        *self = Self::default();
        self.wave_ram = wave_ram; // both wave RAM banks survive power-off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ch3_force_volume_output_is_75_percent() {
        // When force_volume is set, output() must apply 75% scaling then bipolar formula.
        // Sample = 1 -> (1*3)/4 = 0 (integer truncation), D=0, output = 0/7.5 - 1.0 = -1.0.
        let ch3 = Channel3 {
            dac_on: true,
            active: true,
            force_volume: true,
            current_sample: 1,
            output_level: 0, // would normally mute — force_volume overrides
            ..Channel3::default()
        };
        let got = ch3.output();
        let d = (3_u16 / 4) as f32;
        let expected = d / 7.5 - 1.0;
        assert!(
            (got - expected).abs() < 1e-5,
            "force_volume output mismatch: expected {expected}, got {got}"
        );
    }

    #[test]
    fn test_ch3_force_volume_overrides_output_level() {
        // When force_volume is set and output_level=3 (25%), 75% applies instead.
        // Sample = 12 → force 75%: D = 12*3/4 = 9; output = 9/7.5 - 1.0 = 0.2
        // Normal 25%: 12 >> 2 = 3; 3/7.5 - 1.0 = -0.6
        let ch3 = Channel3 {
            dac_on: true,
            active: true,
            force_volume: true,
            current_sample: 12,
            output_level: 3, // 25% — should be overridden
            ..Channel3::default()
        };
        let got = ch3.output();
        let d = ((12_u16 * 3) / 4) as f32; // = 9.0
        let expected = d / 7.5 - 1.0; // = 0.2
        assert!(
            (got - expected).abs() < 1e-5,
            "force_volume should override output_level: expected {expected}, got {got}"
        );
    }

    // ── Bipolar output formula tests ──────────────────────────────────────────

    #[test]
    fn test_ch3_dac_off_outputs_zero() {
        let ch3 = Channel3 {
            dac_on: false,
            active: false,
            ..Channel3::default()
        };
        assert_eq!(ch3.output(), 0.0);
    }

    #[test]
    fn test_ch3_sample_zero_outputs_minus_one() {
        // D=0 → output = 0/7.5 - 1.0 = -1.0
        let ch3 = Channel3 {
            dac_on: true,
            active: true,
            output_level: 1, // 100%
            current_sample: 0,
            ..Channel3::default()
        };
        let got = ch3.output();
        assert!(
            (got - (-1.0_f32)).abs() < 1e-5,
            "sample=0 must produce -1.0, got {got}"
        );
    }

    #[test]
    fn test_ch3_sample_fifteen_outputs_plus_one() {
        // D=15 → output = 15/7.5 - 1.0 = +1.0
        let ch3 = Channel3 {
            dac_on: true,
            active: true,
            output_level: 1, // 100%
            current_sample: 15,
            ..Channel3::default()
        };
        let got = ch3.output();
        assert!(
            (got - 1.0_f32).abs() < 1e-5,
            "sample=15 must produce +1.0, got {got}"
        );
    }

    #[test]
    fn test_ch3_sample_eight_at_50pct_is_bipolar() {
        // output_level=2 (50%): D = 8 >> 1 = 4; output = 4/7.5 - 1.0 ≈ -0.4667
        let ch3 = Channel3 {
            dac_on: true,
            active: true,
            output_level: 2,
            current_sample: 8,
            ..Channel3::default()
        };
        let expected = 4.0_f32 / 7.5 - 1.0;
        let got = ch3.output();
        assert!(
            (got - expected).abs() < 1e-5,
            "50% level sample=8: expected {expected}, got {got}"
        );
    }

    // ── Shift-register read-during-playback tests ─────────────────────────

    #[test]
    fn test_wave_ram_read_not_active_returns_direct_offset() {
        // When CH3 is stopped, reads return data at the exact written offset (no shift).
        let mut ch3 = Channel3::default();
        // bank_select=0 → other bank = 1
        for i in 0..16u8 {
            ch3.wave_ram[1][i as usize] = i * 0x11;
        }
        ch3.wave_pos = 4; // even with wave_pos set, no shift when stopped
        // active=false (default)
        assert_eq!(
            ch3.read_wave_ram(0),
            0x00,
            "stopped: offset 0 must return wave_ram[1][0]"
        );
        assert_eq!(
            ch3.read_wave_ram(2),
            0x22,
            "stopped: offset 2 must return wave_ram[1][2]"
        );
    }

    #[test]
    fn test_wave_ram_read_active_wave_pos_0_returns_direct_offset() {
        // Active playback with wave_pos=0: reads return raw data from the non-playing bank.
        let mut ch3 = Channel3::default();
        for i in 0..16u8 {
            ch3.wave_ram[1][i as usize] = i + 1;
        }
        ch3.active = true;
        ch3.wave_pos = 0;
        assert_eq!(
            ch3.read_wave_ram(0),
            1, // wave_ram[1][0] = 1
            "wave_pos=0, active: offset 0 must return wave_ram[1][0]"
        );
        assert_eq!(
            ch3.read_wave_ram(3),
            4, // wave_ram[1][3] = 4
            "wave_pos=0, active: offset 3 must return wave_ram[1][3]"
        );
    }

    #[test]
    fn test_wave_ram_read_active_non_zero_wave_pos_returns_direct_offset() {
        // wave_pos must NOT shift reads. The non-playing bank is never rotated by the
        // hardware's shift register (only the playing bank rotates), so every offset
        // returns the raw stored byte from the non-playing bank regardless of wave_pos.
        let mut ch3 = Channel3::default();
        // bank_select=0 → other_bank=1
        for i in 0..16u8 {
            ch3.wave_ram[1][i as usize] = 0xA0 + i;
        }
        ch3.active = true;
        ch3.wave_pos = 4;
        assert_eq!(
            ch3.read_wave_ram(0),
            0xA0,
            "wave_pos=4: offset 0 must return wave_ram[1][0] (no shift)"
        );
        assert_eq!(
            ch3.read_wave_ram(1),
            0xA1,
            "wave_pos=4: offset 1 must return wave_ram[1][1] (no shift)"
        );
    }

    #[test]
    fn test_wave_ram_read_active_all_offsets_match_stored_bytes() {
        // All 16 offsets should return exactly what was written to the non-playing bank,
        // regardless of playback state or wave_pos value.
        let mut ch3 = Channel3::default();
        for i in 0..16u8 {
            let sample_base = i.wrapping_mul(2);
            ch3.wave_ram[1][i as usize] = (sample_base << 4) | ((sample_base + 1) & 0x0F);
        }
        ch3.active = true;
        ch3.wave_pos = 1;

        for offset in 0..16usize {
            let sample_base = (offset as u8).wrapping_mul(2);
            let expected = (sample_base << 4) | ((sample_base + 1) & 0x0F);
            assert_eq!(
                ch3.read_wave_ram(offset),
                expected,
                "wave_pos=1: offset {offset} must return stored byte {expected:#04x}"
            );
        }
    }

    #[test]
    fn test_wave_ram_read_active_returns_stored_not_adjacent() {
        // Confirm the non-playing bank's last byte is returned as-is (no wrapping/rotation).
        let mut ch3 = Channel3::default();
        ch3.wave_ram[1][0] = 0xBB;
        ch3.wave_ram[1][15] = 0xFF;
        ch3.active = true;
        ch3.wave_pos = 2;
        assert_eq!(
            ch3.read_wave_ram(15),
            0xFF,
            "wave_pos=2: offset 15 must return wave_ram[1][15] = 0xFF (no wrap/rotation)"
        );
        assert_eq!(
            ch3.read_wave_ram(0),
            0xBB,
            "wave_pos=2: offset 0 must return wave_ram[1][0] = 0xBB (no wrap/rotation)"
        );
    }

    #[test]
    fn test_wave_ram_read_active_no_shift_raw_offset() {
        // Per mGBA (GBAAudioReadWaveRAM): the physical shift-register rotation only affects
        // the PLAYING bank; the non-playing bank is never rotated. CPU reads always target
        // the non-playing bank and must return raw, unshifted data regardless of wave_pos.
        let mut ch3 = Channel3::default();
        // bank_select=0 → other_bank=1
        for i in 0..16u8 {
            ch3.wave_ram[1][i as usize] = 0xA0 + i;
        }
        ch3.active = true;
        ch3.wave_pos = 4; // must NOT rotate the read
        assert_eq!(
            ch3.read_wave_ram(0),
            0xA0,
            "active playback: wave_pos must not rotate wave RAM reads; offset 0 = wave_ram[1][0]"
        );
        assert_eq!(
            ch3.read_wave_ram(5),
            0xA5,
            "active playback: wave_pos must not rotate wave RAM reads; offset 5 = wave_ram[1][5]"
        );
        // Verify the same holds for an odd wave_pos value.
        ch3.wave_pos = 7;
        assert_eq!(
            ch3.read_wave_ram(0),
            0xA0,
            "active, wave_pos=7: offset 0 must still return wave_ram[1][0] (no nibble rotation)"
        );
    }
}
