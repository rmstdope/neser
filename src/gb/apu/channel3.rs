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

pub struct Channel3 {
    dac_on: bool,     // NR30 bit 7
    length_load: u16, // NR31 (0-255); length_counter = 256 - load
    output_level: u8, // NR32 bits 6-5 (0-3)
    freq: u16,        // NR33 + NR34 bits 2-0 (11-bit)
    length_en: bool,  // NR34 bit 6

    active: bool,
    wave_pos: u8,        // 0-31 (position into 32-sample wave table)
    freq_timer: u16,     // countdown; reloads to (2048 - freq) * 2
    length_counter: u16, // 0-256
    /// Wave RAM: 16 bytes = 32 × 4-bit samples.
    wave_ram: [u8; 16],
    /// Byte currently being shifted out (set on wave position advance).
    current_sample: u8,
}

impl Default for Channel3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel3 {
    pub fn new() -> Self {
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
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
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

    /// Advance the wave frequency timer by one M-cycle (= 4 T-cycles).
    /// CH3 freq timer counts in T-cycles: reload = (2048 - freq) * 2.
    pub fn tick(&mut self) {
        if self.freq_timer == 0 {
            self.freq_timer = (2048 - self.freq) * 2;
        }
        if self.freq_timer > 4 {
            self.freq_timer -= 4;
        } else {
            self.freq_timer = (2048 - self.freq) * 2;
            self.wave_pos = (self.wave_pos + 1) & 31;
            self.current_sample = self.read_wave_nibble(self.wave_pos);
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
        // Note: wave_ram is NOT cleared on power-off per hardware spec.
    }

    // ── Register reads ────────────────────────────────────────────────────

    pub fn read_nr30(&self) -> u8 {
        0x7F | (if self.dac_on { 0x80 } else { 0x00 })
    }

    pub fn read_nr32(&self) -> u8 {
        0x9F | ((self.output_level & 0x03) << 5)
    }

    pub fn read_nr34(&self) -> u8 {
        0xBF | (if self.length_en { 0x40 } else { 0x00 })
    }

    // ── Register writes ───────────────────────────────────────────────────

    pub fn write_nr30(&mut self, val: u8) {
        self.dac_on = val & 0x80 != 0;
        if !self.dac_on {
            self.active = false;
        }
    }

    pub fn write_nr31(&mut self, val: u8) {
        self.length_load = u16::from(val);
        self.length_counter = 256 - self.length_load;
    }

    pub fn write_nr32(&mut self, val: u8) {
        self.output_level = (val >> 5) & 0x03;
    }

    pub fn write_nr33(&mut self, val: u8) {
        self.freq = (self.freq & 0x0700) | u16::from(val);
    }

    pub fn write_nr34(&mut self, val: u8) {
        self.length_en = val & 0x40 != 0;
        self.freq = (self.freq & 0x00FF) | (u16::from(val & 0x07) << 8);
        if val & 0x80 != 0 {
            self.trigger();
        }
    }

    pub fn write_nr31_length_only(&mut self, val: u8) {
        self.length_load = u16::from(val);
        self.length_counter = 256 - self.length_load;
    }

    fn trigger(&mut self) {
        if self.dac_on {
            self.active = true;
        }
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        self.freq_timer = (2048 - self.freq) * 2;
        self.wave_pos = 0;
        self.current_sample = self.read_wave_nibble(0);
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

    pub fn read_wave_ram(&self, addr: u16) -> u8 {
        if self.active {
            // During playback, CPU reads return the byte at the current wave position.
            self.wave_ram[(self.wave_pos / 2) as usize]
        } else {
            self.wave_ram[(addr - 0xFF30) as usize]
        }
    }

    pub fn write_wave_ram(&mut self, addr: u16, val: u8) {
        if self.active {
            // During playback, writes go to the byte at the current wave position.
            self.wave_ram[(self.wave_pos / 2) as usize] = val;
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
        ch.write_nr34(0x80); // trigger
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
        ch.write_nr34(0x80); // trigger
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
        ch.write_nr34(0xC0); // trigger + length enable
        ch.clock_length();
        assert!(!ch.is_active());
    }

    #[test]
    fn test_length_no_expire_when_disabled() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr31(255);
        ch.write_nr34(0x80); // trigger, no length enable
        ch.clock_length();
        assert!(ch.is_active());
    }

    // ── Output level ──────────────────────────────────────────────────────

    #[test]
    fn test_output_muted_when_level_0() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x00); // level = 0 → mute
        ch.write_nr34(0x80);
        // Put a non-zero sample in wave RAM at position 0
        ch.wave_ram[0] = 0xFF;
        ch.trigger();
        assert_eq!(ch.output(), 0.0);
    }

    #[test]
    fn test_output_100_percent() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x20); // level = 1 → 100%
        ch.wave_ram[0] = 0xF0; // first nibble = 15
        ch.write_nr34(0x80);
        assert!((ch.output() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_output_50_percent() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x40); // level = 2 → 50%
        ch.wave_ram[0] = 0xF0; // first nibble = 15; shifted right by 1 = 7
        ch.write_nr34(0x80);
        // 7 / 15 ≈ 0.4667
        assert!((ch.output() - 7.0 / 15.0).abs() < 0.001);
    }

    #[test]
    fn test_output_25_percent() {
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.write_nr32(0x60); // level = 3 → 25%
        ch.wave_ram[0] = 0xF0; // 15 >> 2 = 3
        ch.write_nr34(0x80);
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
    fn test_wave_ram_read_returns_current_byte_during_playback() {
        // During active playback, reads always return the byte at wave_pos/2.
        let mut ch = Channel3::new();
        ch.write_nr30(0x80);
        ch.wave_ram[0] = 0x12; // wave_pos 0-1 → byte 0
        ch.wave_ram[1] = 0x34;
        ch.write_nr34(0x80); // trigger → wave_pos = 0
        assert!(ch.is_active());
        // Read any address during playback → should return wave_ram[wave_pos/2] = wave_ram[0]
        assert_eq!(
            ch.read_wave_ram(0xFF3F),
            0x12,
            "during playback, wave RAM read must return current byte"
        );
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
        ch.write_nr34(0x40); // length en, no trigger
        assert_eq!(ch.read_nr34() & 0x40, 0x40);
    }
}
