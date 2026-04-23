//! Game Boy DMG APU (Audio Processing Unit).
//!
//! Implements all four DMG sound channels, the Frame Sequencer, master volume
//! and panning (NR50/NR51/NR52), wave RAM, and a mono sample output pipeline
//! compatible with the existing `sample_ready()` / `get_sample()` interface.
//!
//! ## Register map
//!
//! | Address      | Register | Description                            |
//! |---|---|---|
//! | $FF10–$FF14  | NR10–NR14 | CH1 Pulse + sweep                  |
//! | $FF16–$FF19  | NR21–NR24 | CH2 Pulse                           |
//! | $FF1A–$FF1E  | NR30–NR34 | CH3 Wave                            |
//! | $FF20–$FF23  | NR41–NR44 | CH4 Noise                           |
//! | $FF24        | NR50      | Master volume / VIN routing         |
//! | $FF25        | NR51      | Channel terminal panning            |
//! | $FF26        | NR52      | APU power / channel status          |
//! | $FF30–$FF3F  | —         | Wave RAM (32 × 4-bit samples)       |

pub mod channel1;
pub mod channel2;
pub mod channel3;
pub mod channel4;

use serde::{Deserialize, Serialize};

use channel1::Channel1;
use channel2::Channel2;
use channel3::Channel3;
use channel4::Channel4;

/// DMG clock rate in M-cycles per second (4 194 304 Hz / 4).
const DMG_MCYCLES_PER_SEC: f32 = 1_048_576.0;

/// M-cycles between Frame Sequencer steps (512 Hz → 1 step per 2 048 M-cycles).
const FS_MCYCLES_PER_STEP: u16 = 2048;

/// Frame Sequencer 8-step table.
///
/// Each entry is a bitmask:
/// - bit 0 = clock length counters
/// - bit 1 = clock frequency sweep (CH1 only)
/// - bit 2 = clock volume envelopes
#[rustfmt::skip]
const FS_TABLE: [u8; 8] = [
    0b001, // step 0: length
    0b000, // step 1: —
    0b011, // step 2: length + sweep
    0b000, // step 3: —
    0b001, // step 4: length
    0b000, // step 5: —
    0b011, // step 6: length + sweep
    0b100, // step 7: envelope
];

/// 8-step duty-cycle waveforms shared by CH1 and CH2 (bit 7 = first step output).
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 1, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

/// Game Boy DMG APU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apu {
    ch1: Channel1,
    ch2: Channel2,
    ch3: Channel3,
    ch4: Channel4,

    /// NR50 ($FF24): master volume and VIN routing.
    nr50: u8,
    /// NR51 ($FF25): channel terminal panning.
    nr51: u8,
    /// APU power flag (NR52 bit 7). When `false` most registers are frozen.
    powered: bool,

    /// Frame Sequencer M-cycle countdown (reloads to `FS_MCYCLES_PER_STEP`).
    fs_timer: u16,
    /// Current Frame Sequencer step (0–7).
    fs_step: u8,

    /// Fractional M-cycle accumulator for sample generation.
    sample_acc: f32,
    /// Number of M-cycles between output samples.
    cycles_per_sample: f32,
    /// Pending output sample (`Some` when `sample_ready()` is true).
    pending_sample: Option<f32>,

    /// `true` when running a CGB-compatible ROM (header byte 0x0143 is 0x80 or 0xC0).
    /// Gates CGB-specific APU differences (length counter behavior on power off/on).
    is_cgb: bool,

    /// High-pass filter state: previous mix input (before filter).
    #[serde(default)]
    hp_prev_in: f32,
    /// High-pass filter state: previous filter output.
    #[serde(default)]
    hp_prev_out: f32,
}

impl Apu {
    /// Create a new APU in the power-off state.
    pub fn new(is_cgb: bool) -> Self {
        Self {
            ch1: Channel1::new(),
            ch2: Channel2::new(),
            ch3: Channel3::new_with_mode(is_cgb),
            ch4: Channel4::new(),
            nr50: 0x00,
            nr51: 0x00,
            powered: false,
            fs_timer: FS_MCYCLES_PER_STEP,
            fs_step: 0,
            sample_acc: 0.0,
            cycles_per_sample: DMG_MCYCLES_PER_SEC / 44_100.0,
            pending_sample: None,
            is_cgb,
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
        }
    }

    /// Set the output sample rate in Hz (default: 44 100).
    pub fn set_sample_rate(&mut self, rate: f32) {
        self.cycles_per_sample = DMG_MCYCLES_PER_SEC / rate;
    }

    /// Returns `true` when an audio sample is ready to be collected.
    pub fn sample_ready(&self) -> bool {
        self.pending_sample.is_some()
    }

    /// Consume and return the pending audio sample, or `None` if not ready.
    pub fn take_sample(&mut self) -> Option<f32> {
        self.pending_sample.take()
    }

    /// Advance the APU by `m_cycles` M-cycles.
    ///
    /// Clocks all active channels and the Frame Sequencer. Accumulates the
    /// fractional sample counter and emits a sample when the threshold is reached.
    pub fn tick(&mut self, m_cycles: u8) {
        for _ in 0..m_cycles {
            self.tick_one();
        }
    }

    /// Advance by exactly one M-cycle.
    fn tick_one(&mut self) {
        // ── Frame Sequencer ────────────────────────────────────────────────
        self.fs_timer -= 1;
        if self.fs_timer == 0 {
            self.fs_timer = FS_MCYCLES_PER_STEP;
            let flags = FS_TABLE[self.fs_step as usize];
            if flags & 0x01 != 0 {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
                self.ch4.clock_length();
            }
            if flags & 0x02 != 0 {
                self.ch1.clock_sweep();
            }
            if flags & 0x04 != 0 {
                self.ch1.clock_envelope();
                self.ch2.clock_envelope();
                self.ch4.clock_envelope();
            }
            self.fs_step = (self.fs_step + 1) & 7;
        }

        // ── Channel frequency timers ───────────────────────────────────────
        self.ch1.tick();
        self.ch2.tick();
        self.ch3.tick();
        self.ch4.tick();

        // ── Sample output ──────────────────────────────────────────────────
        self.sample_acc += 1.0;
        if self.sample_acc >= self.cycles_per_sample {
            self.sample_acc -= self.cycles_per_sample;
            // Only overwrite if the previous sample was already consumed.
            if self.pending_sample.is_none() {
                self.pending_sample = Some(self.mix());
            }
        }
    }

    /// Mix all four channels through NR50/NR51 into a mono f32 in `[-1.0, 1.0]`.
    ///
    /// Applies a first-order high-pass filter (simulating the DMG output
    /// capacitor / AC coupling) to remove DC bias and centre the signal
    /// around 0.  The filter coefficient `RC = 0.999` gives a cutoff of
    /// roughly 7 Hz at 44.1 kHz, removing DC while leaving all audible
    /// content unaffected.
    fn mix(&mut self) -> f32 {
        if !self.powered {
            return 0.0;
        }

        let samples = [
            self.ch1.output(),
            self.ch2.output(),
            self.ch3.output(),
            self.ch4.output(),
        ];

        // NR51 bits 7-4 = left enable (CH4/3/2/1), bits 3-0 = right enable.
        let left_mix = Self::mix_terminal(samples, self.nr51 >> 4);
        let right_mix = Self::mix_terminal(samples, self.nr51 & 0x0F);

        // NR50 bits 6-4 = left master volume (0-7), bits 2-0 = right (0-7).
        let left_volume = ((self.nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_volume = (self.nr50 & 0x07) as f32 / 7.0;

        let raw = (left_mix * left_volume + right_mix * right_volume) / 2.0;

        // High-pass filter: out[n] = RC × (out[n-1] + in[n] − in[n-1]).
        // Removes DC (low-frequency bias) and produces bipolar output in [-1, 1].
        const RC: f32 = 0.999;
        let hp_out = RC * (self.hp_prev_out + raw - self.hp_prev_in);
        self.hp_prev_in = raw;
        self.hp_prev_out = hp_out;

        hp_out.clamp(-1.0, 1.0)
    }

    /// Sum channel samples gated by a 4-bit enable mask (bit i enables samples[i]).
    fn mix_terminal(samples: [f32; 4], enable_mask: u8) -> f32 {
        samples
            .iter()
            .enumerate()
            .map(|(i, &s)| if enable_mask & (1 << i) != 0 { s } else { 0.0 })
            .sum::<f32>()
            / 4.0
    }

    // ── Register read ──────────────────────────────────────────────────────

    /// Read an APU register.
    ///
    /// Returns $FF for write-only bits / unused registers.
    /// When powered off, registers NR10–NR51 return their cleared values
    /// (with unused bits set per the normal masking), consistent with DMG hardware.
    /// NR52 and wave RAM are always readable.
    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            // CH1
            0xFF10 => self.ch1.read_nr10(),
            0xFF11 => self.ch1.read_nr11(),
            0xFF12 => self.ch1.read_nr12(),
            0xFF13 => 0xFF, // write-only
            0xFF14 => self.ch1.read_nr14(),
            // CH2
            0xFF15 => 0xFF, // unused
            0xFF16 => self.ch2.read_nr21(),
            0xFF17 => self.ch2.read_nr22(),
            0xFF18 => 0xFF, // write-only
            0xFF19 => self.ch2.read_nr24(),
            // CH3
            0xFF1A => self.ch3.read_nr30(),
            0xFF1B => 0xFF, // write-only
            0xFF1C => self.ch3.read_nr32(),
            0xFF1D => 0xFF, // write-only
            0xFF1E => self.ch3.read_nr34(),
            // CH4
            0xFF1F => 0xFF, // unused
            0xFF20 => 0xFF, // write-only
            0xFF21 => self.ch4.read_nr42(),
            0xFF22 => self.ch4.read_nr43(),
            0xFF23 => self.ch4.read_nr44(),
            // Master
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => self.read_nr52(),
            // Wave RAM
            0xFF30..=0xFF3F => self.ch3.read_wave_ram(addr),
            _ => 0xFF,
        }
    }

    fn read_nr52(&self) -> u8 {
        // Bits 6-4 are unused and read as 1 on DMG hardware.
        let mut nr52 = 0x70;
        nr52 |= u8::from(self.powered) << 7;
        nr52 |= u8::from(self.ch1.is_active());
        nr52 |= u8::from(self.ch2.is_active()) << 1;
        nr52 |= u8::from(self.ch3.is_active()) << 2;
        nr52 |= u8::from(self.ch4.is_active()) << 3;
        nr52
    }

    // ── Register write ─────────────────────────────────────────────────────

    /// Write an APU register.
    ///
    /// When powered off, writes to NR10–NR51 are ignored (length counters are
    /// writable on DMG even when powered off, but we keep it simple here).
    /// Writes to NR52 are always honoured to allow power-on.
    pub fn write_register(&mut self, addr: u16, val: u8) {
        // NR52 is always writable.
        if addr == 0xFF26 {
            self.write_nr52(val);
            return;
        }

        // Wave RAM is accessible regardless of APU power state.
        if (0xFF30..=0xFF3F).contains(&addr) {
            self.ch3.write_wave_ram(addr, val);
            return;
        }

        if !self.powered {
            // On DMG, length counters remain writable when APU is off.
            // On CGB, all writes (including length) are rejected when off.
            if !self.is_cgb {
                match addr {
                    0xFF11 => self.ch1.write_nr11_length_only(val),
                    0xFF16 => self.ch2.write_nr21_length_only(val),
                    0xFF1B => self.ch3.write_nr31_length_only(val),
                    0xFF20 => self.ch4.write_nr41_length_only(val),
                    _ => {}
                }
            }
            return;
        }

        // Extra length clocking: when the current FS step does NOT clock length
        // (i.e. FS_TABLE[fs_step] has bit 0 clear — steps 1, 3, 5, 7), NRx4
        // writes that enable length_en or trigger with length_en get an extra clock.
        let extra_clk = FS_TABLE[self.fs_step as usize] & 0x01 == 0;

        match addr {
            0xFF10 => self.ch1.write_nr10(val),
            0xFF11 => self.ch1.write_nr11(val),
            0xFF12 => self.ch1.write_nr12(val),
            0xFF13 => self.ch1.write_nr13(val),
            0xFF14 => self.ch1.write_nr14(val, extra_clk),
            0xFF15 => {}
            0xFF16 => self.ch2.write_nr21(val),
            0xFF17 => self.ch2.write_nr22(val),
            0xFF18 => self.ch2.write_nr23(val),
            0xFF19 => self.ch2.write_nr24(val, extra_clk),
            0xFF1A => self.ch3.write_nr30(val),
            0xFF1B => self.ch3.write_nr31(val),
            0xFF1C => self.ch3.write_nr32(val),
            0xFF1D => self.ch3.write_nr33(val),
            0xFF1E => self.ch3.write_nr34(val, extra_clk),
            0xFF1F => {}
            0xFF20 => self.ch4.write_nr41(val),
            0xFF21 => self.ch4.write_nr42(val),
            0xFF22 => self.ch4.write_nr43(val),
            0xFF23 => self.ch4.write_nr44(val, extra_clk),
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            _ => {}
        }
    }

    fn write_nr52(&mut self, val: u8) {
        let was_powered = self.powered;
        self.powered = val & 0x80 != 0;

        if was_powered && !self.powered {
            // Power off: clear all NR10–NR51 registers and HP filter state.
            self.ch1.power_off();
            self.ch2.power_off();
            self.ch3.power_off();
            self.ch4.power_off();
            self.nr50 = 0x00;
            self.nr51 = 0x00;
            self.hp_prev_in = 0.0;
            self.hp_prev_out = 0.0;
        } else if !was_powered && self.powered {
            // Power on: reset frame sequencer.
            self.fs_step = 0;
            self.fs_timer = FS_MCYCLES_PER_STEP;
            // On CGB, powering on resets all length counters to 0.
            if self.is_cgb {
                self.ch1.length_counter = 0;
                self.ch2.length_counter = 0;
                self.ch3.length_counter = 0;
                self.ch4.length_counter = 0;
            }
        }
    }

    /// Expose ch3 wave RAM read for DmgBus (used during CH3 playback quirk).
    pub fn read_wave_ram(&self, addr: u16) -> u8 {
        self.ch3.read_wave_ram(addr)
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(false)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn powered_apu() -> Apu {
        let mut apu = Apu::new(false);
        apu.write_register(0xFF26, 0x80); // power on
        apu
    }

    // ── NR52 power control ────────────────────────────────────────────────

    #[test]
    fn test_nr52_power_on_bit_readable() {
        // Given: APU powered on; When: read NR52; Then: bit 7 is set, bits 6-4 = 0b111 (unused)
        let apu = powered_apu();
        assert_eq!(apu.read_register(0xFF26) & 0x80, 0x80);
    }

    #[test]
    fn test_nr52_power_off_bit_readable() {
        // Given: APU powered off (default); When: read NR52; Then: bit 7 is clear
        let apu = Apu::new(false);
        assert_eq!(apu.read_register(0xFF26) & 0x80, 0x00);
    }

    #[test]
    fn test_nr52_unused_bits_read_as_1() {
        // Bits 6-4 of NR52 read as 1 (open bus on DMG).
        let apu = Apu::new(false);
        assert_eq!(apu.read_register(0xFF26) & 0x70, 0x70);
    }

    // ── Power-off clears NR10–NR51 ────────────────────────────────────────

    #[test]
    fn test_power_off_clears_nr50() {
        // Given: APU powered on with NR50=$77; When: power off; Then: NR50 reads $FF (because
        // reads of NR10-NR51 return $FF when powered off — but nr50 itself is zeroed).
        let mut apu = powered_apu();
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.read_register(0xFF24), 0x77);
        apu.write_register(0xFF26, 0x00); // power off
        // After power-off, NR50 register is $00 but reads still work (NR50 is special)
        // Actually: when powered off, NR50 reads its internal value (which was cleared to $00)
        assert_eq!(apu.nr50, 0x00);
    }

    #[test]
    fn test_power_off_clears_nr51() {
        let mut apu = powered_apu();
        apu.write_register(0xFF25, 0xF3);
        apu.write_register(0xFF26, 0x00);
        assert_eq!(apu.nr51, 0x00);
    }

    #[test]
    fn test_power_off_ignores_nr50_write() {
        // When powered off, writes to NR50 are ignored.
        let mut apu = Apu::new(false); // starts powered off
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.nr50, 0x00, "NR50 write must be ignored when APU is off");
    }

    #[test]
    fn test_power_on_allows_nr50_write() {
        let mut apu = powered_apu();
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.nr50, 0x77);
    }

    // ── NR51 panning R/W ──────────────────────────────────────────────────

    #[test]
    fn test_nr51_write_read_roundtrip() {
        let mut apu = powered_apu();
        apu.write_register(0xFF25, 0xAB);
        assert_eq!(apu.read_register(0xFF25), 0xAB);
    }

    // ── Sample output ─────────────────────────────────────────────────────

    #[test]
    fn test_sample_not_ready_initially() {
        let apu = Apu::new(false);
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_ready_after_enough_ticks() {
        // At 44100 Hz sample rate and DMG 1 048 576 M-cycles/s,
        // one sample ≈ every 23.77 M-cycles.
        let mut apu = Apu::new(false);
        apu.set_sample_rate(44_100.0);
        // Tick 30 M-cycles — must have produced at least one sample.
        apu.tick(30);
        assert!(apu.sample_ready(), "expected a sample after 30 M-cycles");
    }

    #[test]
    fn test_take_sample_clears_ready_flag() {
        let mut apu = Apu::new(false);
        apu.tick(30);
        apu.take_sample();
        // After consuming the sample the flag must be clear.
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_is_silent_when_powered_off() {
        // With APU off, all samples must be 0.0.
        let mut apu = Apu::new(false);
        apu.tick(30);
        let s = apu.take_sample().unwrap_or(0.0);
        assert_eq!(s, 0.0);
    }

    // ── Frame Sequencer timing ────────────────────────────────────────────

    #[test]
    fn test_frame_sequencer_advances_after_2048_mcycles() {
        // The FS step should increment once per 2048 M-cycles.
        let mut apu = powered_apu();
        assert_eq!(apu.fs_step, 0);
        apu.tick(100); // partial — still step 0 (2048 M-cycles needed)
        // We haven't reached 2048 yet; step is still 0.
        // (The timer starts at 2048 and counts down to 0, then advances.)
        // After 2048 ticks step becomes 1.
        // tick 2048 - already ticked 100; tick 1948 more.
        for _ in 0..1948u16 {
            apu.tick(1);
        }
        assert_eq!(apu.fs_step, 1, "fs_step must be 1 after 2048 M-cycles");
    }

    #[test]
    fn test_frame_sequencer_wraps_at_8() {
        let mut apu = powered_apu();
        // Tick 8 × 2048 = 16384 M-cycles to complete a full revolution.
        for _ in 0u32..16_384 {
            apu.tick(1);
        }
        assert_eq!(apu.fs_step, 0, "fs_step must wrap back to 0 after 8 steps");
    }

    // ── Register reads when powered off ──────────────────────────────────

    #[test]
    fn test_registers_return_cleared_values_when_powered_off() {
        // Given: APU powered on then off;
        // When: read NR10 (0xFF10) and NR12 (0xFF12);
        // Then: they return the cleared register value (with unused bits set),
        //   not 0xFF across the board.
        let mut apu = powered_apu();
        apu.write_register(0xFF26, 0x00); // power off
        // ch1 cleared → read_nr10() = 0x80 (bit 7 always 1, all others 0)
        assert_eq!(
            apu.read_register(0xFF10),
            0x80,
            "NR10 must return cleared 0x80 when powered off (not 0xFF)"
        );
        // ch1 cleared → read_nr12() = 0x00 (all fields zero)
        assert_eq!(
            apu.read_register(0xFF12),
            0x00,
            "NR12 must return cleared 0x00 when powered off (not 0xFF)"
        );
    }

    // ── NR52 channel-active status bits ───────────────────────────────────

    #[test]
    fn test_nr52_ch1_active_bit_reflects_channel1_state() {
        let apu = Apu::new(false);
        // CH1 is inactive by default → bit 0 clear
        assert_eq!(apu.read_register(0xFF26) & 0x01, 0x00);
    }

    // ── CGB-specific power behavior ─────────────────────────────────────

    fn powered_cgb_apu() -> Apu {
        let mut apu = Apu::new(true);
        apu.write_register(0xFF26, 0x80); // power on
        apu
    }

    #[test]
    fn test_cgb_power_off_rejects_nr41_length_write() {
        // On CGB, length counter writes are rejected when APU is off.
        // Given: CGB APU powered on, set CH4 length, then power off
        let mut apu = powered_cgb_apu();
        apu.write_register(0xFF20, 0x3F); // NR41: length_load = 63 → counter = 1
        apu.write_register(0xFF26, 0x00); // power off → clears length_counter to 0
        // When: write NR41 while powered off
        apu.write_register(0xFF20, 0x3F); // on DMG this sets counter=1, on CGB it's rejected
        // Then: length_counter must remain 0
        assert_eq!(
            apu.ch4.length_counter, 0,
            "CGB must reject length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_off_rejects_nr11_length_write() {
        // Given: CGB APU powered off
        let mut apu = Apu::new(true);
        // When: write NR11 while powered off
        apu.write_register(0xFF11, 0x3F); // length_load = 63 → counter would be 1 on DMG
        // Then: length_counter must remain 0
        assert_eq!(
            apu.ch1.length_counter, 0,
            "CGB must reject CH1 length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_off_rejects_nr21_length_write() {
        let mut apu = Apu::new(true);
        apu.write_register(0xFF16, 0x3F);
        assert_eq!(
            apu.ch2.length_counter, 0,
            "CGB must reject CH2 length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_off_rejects_nr31_length_write() {
        let mut apu = Apu::new(true);
        apu.write_register(0xFF1B, 0xFF); // length_load = 255 → counter would be 1 on DMG
        assert_eq!(
            apu.ch3.length_counter, 0,
            "CGB must reject CH3 length writes when APU is off"
        );
    }

    #[test]
    fn test_dmg_power_off_allows_nr41_length_write() {
        // On DMG, length counter writes are allowed when APU is off.
        let mut apu = Apu::new(false);
        apu.write_register(0xFF20, 0x3F); // NR41: length_load = 63 → counter = 1
        assert_eq!(
            apu.ch4.length_counter, 1,
            "DMG must allow length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_on_resets_length_counters() {
        // On CGB, powering on the APU resets all length counters to 0.
        // Given: CGB APU on, set length counters, power off then on
        let mut apu = powered_cgb_apu();
        apu.write_register(0xFF11, 0x00); // CH1: length_load=0 → counter=64
        apu.write_register(0xFF16, 0x00); // CH2: length_load=0 → counter=64
        apu.write_register(0xFF1B, 0x00); // CH3: length_load=0 → counter=256
        apu.write_register(0xFF20, 0x00); // CH4: length_load=0 → counter=64
        // Verify they're set
        assert_eq!(apu.ch1.length_counter, 64);
        assert_eq!(apu.ch4.length_counter, 64);
        // Power off → counters cleared
        apu.write_register(0xFF26, 0x00);
        // Power on → on CGB, counters must be reset to 0
        apu.write_register(0xFF26, 0x80);
        assert_eq!(
            apu.ch1.length_counter, 0,
            "CGB power-on must reset CH1 length"
        );
        assert_eq!(
            apu.ch2.length_counter, 0,
            "CGB power-on must reset CH2 length"
        );
        assert_eq!(
            apu.ch3.length_counter, 0,
            "CGB power-on must reset CH3 length"
        );
        assert_eq!(
            apu.ch4.length_counter, 0,
            "CGB power-on must reset CH4 length"
        );
    }

    // ── High-pass (AC coupling) filter / bipolar output ───────────────────

    #[test]
    fn test_mix_hp_filter_produces_negative_transient_when_input_drops() {
        // Given: APU powered on; HP filter state primed as if channels were
        // running at high output and then suddenly silenced.
        // Expected: HP filter output is negative (compensates for prior DC).
        // This test FAILS before the HP filter is implemented because mix()
        // returns 0.0 for silent channels regardless of previous state.
        let mut apu = powered_apu();
        // Manually prime the filter state: previous input was 1.0,
        // previous output was 0.5 (as if all channels ran at full volume).
        apu.hp_prev_in = 1.0;
        apu.hp_prev_out = 0.5;

        // Current channels produce 0.0 (no channels active → unipolar = 0.0).
        // HP filter: out = 0.999 * (0.5 + 0.0 - 1.0) = 0.999 * -0.5 ≈ -0.4995
        let sample = apu.mix();

        assert!(
            sample < 0.0,
            "HP filter must produce a negative transient when input drops from high to zero, got {sample}"
        );
    }

    #[test]
    fn test_mix_hp_filter_output_stays_in_bipolar_range() {
        // Given: all four channels at maximum volume, all panning bits set.
        // When: ticking 60 000 M-cycles (≈ 2 524 samples at 44 100 Hz).
        // Then: every sample must be in [-1.0, 1.0].
        let mut apu = powered_apu();

        // CH1: max volume, 50% duty, trigger.
        apu.write_register(0xFF11, 0x80); // NR11: duty=10 (50%)
        apu.write_register(0xFF12, 0xF0); // NR12: vol=15, DAC on
        apu.write_register(0xFF14, 0x80); // NR14: trigger

        // CH2: max volume, 50% duty, trigger.
        apu.write_register(0xFF16, 0x80); // NR21: duty=10
        apu.write_register(0xFF17, 0xF0); // NR22: vol=15, DAC on
        apu.write_register(0xFF19, 0x80); // NR24: trigger

        // CH3: DAC on, 100% output level, trigger.
        apu.write_register(0xFF1A, 0x80); // NR30: DAC on
        apu.write_register(0xFF1C, 0x20); // NR32: output=100%
        apu.write_register(0xFF1E, 0x80); // NR34: trigger

        // CH4: max volume, trigger.
        apu.write_register(0xFF21, 0xF0); // NR42: vol=15, DAC on
        apu.write_register(0xFF23, 0x80); // NR44: trigger

        // NR51=0xFF: all channels to both terminals. NR50=0x77: max master vol.
        apu.write_register(0xFF25, 0xFF);
        apu.write_register(0xFF24, 0x77);

        let mut violations = 0u32;
        for _ in 0..60_000 {
            apu.tick(1);
            if let Some(s) = apu.take_sample()
                && !(-1.0..=1.0).contains(&s)
            {
                violations += 1;
            }
        }

        assert_eq!(
            violations, 0,
            "mix() must never exceed [-1.0, 1.0]; found {violations} violation(s)"
        );
    }

    #[test]
    fn test_mix_active_channel_produces_negative_samples_after_filter_converges() {
        // Given: CH1 oscillating at 50% duty, max volume.
        // When: running long enough for the HP filter to converge.
        // Then: at least one sample must be negative (DC removed, signal centred at 0).
        //
        // Without the HP filter mix() never returns negative values ([0, 1] only),
        // so this test is RED before the implementation.
        let mut apu = powered_apu();
        apu.write_register(0xFF11, 0x80); // NR11: duty=10 (50%)
        apu.write_register(0xFF12, 0xF0); // NR12: vol=15, DAC on
        apu.write_register(0xFF13, 0x80); // NR13: freq low (moderate frequency)
        apu.write_register(0xFF14, 0x84); // NR14: trigger, freq high bits
        apu.write_register(0xFF25, 0x01); // NR51: CH1 to right terminal only
        apu.write_register(0xFF24, 0x07); // NR50: right master vol = max

        let mut found_negative = false;
        // 3 000 samples ≈ 71 340 M-cycles; the HP filter (RC≈0.999) converges
        // in ~1 000–2 000 samples, so the LOW phase of the duty cycle must
        // produce negative output well within this window.
        for _ in 0..71_340 {
            apu.tick(1);
            if let Some(s) = apu.take_sample()
                && s < 0.0
            {
                found_negative = true;
                break;
            }
        }

        assert!(
            found_negative,
            "HP filter must produce negative samples once DC is removed from an oscillating channel"
        );
    }
}
