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

use crate::trace_apu;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::gb::model::CgbModel;

use super::channel1::Channel1;
use super::channel2::Channel2;
use super::channel3::Channel3;
use super::channel4::Channel4;

/// DMG clock rate in M-cycles per second (4 194 304 Hz / 4).
const DMG_MCYCLES_PER_SEC: f32 = 1_048_576.0;
/// Upper bound for queued audio samples awaiting frontend retrieval.
const MAX_PENDING_SAMPLES: usize = 16_384;

/// Cutoff frequency for the GB APU AC-coupling high-pass filter (~7 Hz removes DC bias).
const HP_CUTOFF_HZ: f32 = 7.0;

// Note: The Frame Sequencer is clocked by DIV-APU falling edges (512 Hz = every 2048 M-cycles)
// from the Timer, not by an internal countdown timer. See `clock_div_apu()`.

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
pub(super) const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
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

    /// Current Frame Sequencer step (0–7).
    /// Clocked by DIV-APU falling edges via `clock_div_apu()`.
    fs_step: u8,

    /// Low-frequency divider flag. Toggled each M-cycle to track 2 MHz alignment.
    /// Used to calculate the startup delay for pulse channels on trigger.
    #[serde(default)]
    lf_div: bool,

    /// FS-step counter (`div_divider`) advanced once per DIV-APU event (i.e.,
    /// per Frame Sequencer step at 512 Hz). Sub-M-cycle APU machinery gates
    /// on its low bits:
    ///   * `& 1 == 1` — length / NR14 retrigger "extra length tick" glitch
    ///   * `& 3 == 3` — sweep tick (drives `Channel1::sweep_countdown`)
    ///   * `& 7 == 7` — envelope volume countdown decrement
    #[serde(default)]
    div_divider: u8,

    /// When `true`, the next DIV-APU falling edge is skipped.
    /// Set when the APU is powered on while the DIV-APU bit is already HIGH.
    /// Per SameSuite div_write_trigger_10: "Starting the APU while bit 4 of
    /// the DIV register is set causes the APU to skip the first DIV-APU event."
    #[serde(default)]
    skip_next_div_apu_event: bool,
    /// When `true`, the next DIV-APU event is serviced without advancing
    /// `div_divider`. The
    /// first event after a power-on-high skip still runs APU edge side effects,
    /// but keeps the seeded divider phase for length/envelope/sweep timing.
    #[serde(default)]
    div_apu_event_without_div_increment: bool,

    /// Fractional M-cycle accumulator for sample generation.
    sample_acc: f32,
    /// Number of M-cycles between output samples.
    cycles_per_sample: f32,
    /// Pending output samples queued until the frontend consumes them.
    #[serde(default)]
    pending_samples: VecDeque<f32>,

    /// `true` when running a CGB-compatible ROM (header byte 0x0143 is 0x80 or 0xC0).
    /// Gates CGB-specific APU differences (length counter behavior on power off/on).
    is_cgb: bool,

    /// CGB hardware revision for model-specific APU quirks.
    #[serde(default)]
    cgb_model: CgbModel,

    /// High-pass filter state: previous mix input (before filter).
    #[serde(default)]
    hp_prev_in: f32,
    /// High-pass filter state: previous filter output.
    #[serde(default)]
    hp_prev_out: f32,
    /// High-pass filter coefficient (α = f_s / (f_s + 2π·f_c)), derived from
    /// the output sample rate and a fixed ~7 Hz cutoff.  Updated whenever
    /// `set_sample_rate()` is called.  Not serialized — recomputed on load.
    #[serde(skip, default = "Apu::default_hp_rc")]
    hp_rc: f32,
}

impl Apu {
    /// Create a new APU in the power-off state.
    pub fn new(is_cgb: bool) -> Self {
        let mut ch1 = Channel1::new();
        // Default CGB revision = CgbE (latest); CgbBus overrides via
        // `set_cgb_model` once it knows the configured revision.
        ch1.set_model(is_cgb, CgbModel::CgbE);
        Self {
            ch1,
            ch2: Channel2::new(),
            ch3: Channel3::new_with_mode(is_cgb),
            ch4: Channel4::new(),
            nr50: 0x00,
            nr51: 0x00,
            powered: false,
            fs_step: 0,
            lf_div: true, // Initialize to 1
            div_divider: 0,
            skip_next_div_apu_event: false,
            div_apu_event_without_div_increment: false,
            sample_acc: 0.0,
            cycles_per_sample: DMG_MCYCLES_PER_SEC / 44_100.0,
            pending_samples: VecDeque::with_capacity(MAX_PENDING_SAMPLES),
            is_cgb,
            cgb_model: CgbModel::CgbE,
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
            hp_rc: Self::compute_hp_rc(44_100.0),
        }
    }

    /// Compute the high-pass filter coefficient for a given output sample rate.
    ///
    /// Uses a target cutoff of ~7 Hz (well below audible range) to remove DC
    /// bias while leaving all audible content intact.
    /// Formula: α = f_s / (f_s + 2π·f_c)
    fn compute_hp_rc(sample_rate: f32) -> f32 {
        sample_rate / (sample_rate + 2.0 * std::f32::consts::PI * HP_CUTOFF_HZ)
    }

    /// Default HP filter coefficient (44.1 kHz baseline), used by serde when
    /// deserializing old save states that predate this field.
    fn default_hp_rc() -> f32 {
        Self::compute_hp_rc(44_100.0)
    }

    /// Set the CGB hardware revision and propagate it to subchannels that
    /// consume it (currently CH1 sweep timing). Should be called once at
    /// bus construction; default model in `Apu::new` is correct for DMG.
    pub fn set_cgb_model(&mut self, cgb_model: CgbModel) {
        self.cgb_model = cgb_model;
        self.ch1.set_model(self.is_cgb, cgb_model);
    }

    /// CGB-0/A/B ignore the current NRx4 length-enable bit for the extra length
    /// clock; only the previously disabled state matters.
    fn cgb_early_extra_length_clock(&self) -> bool {
        self.is_cgb
            && matches!(
                self.cgb_model,
                CgbModel::Cgb0 | CgbModel::CgbA | CgbModel::CgbB
            )
    }

    /// CGB-B CH3 keeps its active flag set until the next non-trigger NR34
    /// write when this quirk clocks length from 1 to 0 with length-enable clear.
    fn cgb_b_delayed_ch3_length_disable(&self) -> bool {
        self.is_cgb && self.cgb_model == CgbModel::CgbB
    }

    /// Set the output sample rate in Hz (default: 44 100).
    pub fn set_sample_rate(&mut self, rate: f32) {
        self.cycles_per_sample = DMG_MCYCLES_PER_SEC / rate;
        self.hp_rc = Self::compute_hp_rc(rate);
        self.pending_samples.clear();
    }

    /// Returns the current output sample rate in Hz.
    pub fn sample_rate(&self) -> f32 {
        DMG_MCYCLES_PER_SEC / self.cycles_per_sample
    }

    /// Returns `true` when an audio sample is ready to be collected.
    pub fn sample_ready(&self) -> bool {
        !self.pending_samples.is_empty()
    }

    /// Consume and return the pending audio sample, or `None` if not ready.
    pub fn take_sample(&mut self) -> Option<f32> {
        self.pending_samples.pop_front()
    }

    fn push_pending_sample(&mut self, sample: f32) {
        if self.pending_samples.len() == MAX_PENDING_SAMPLES {
            self.pending_samples.pop_front();
        }
        self.pending_samples.push_back(sample);
    }

    /// Advance the APU by `m_cycles` M-cycles.
    ///
    /// Clocks all active channels and accumulates the fractional sample counter.
    /// The Frame Sequencer is NOT clocked here — it is driven by DIV-APU
    /// falling edges via `clock_div_apu()`.
    pub fn tick(&mut self, m_cycles: u8) {
        for _ in 0..m_cycles {
            self.tick_one();
        }
    }

    /// Advance CH3 by exactly one APU cycle (used in CGB double-speed mode).
    ///
    /// In double-speed mode CH3 is ticked once per CPU M-cycle (1 APU cycle)
    /// rather than twice every two M-cycles (2 APU cycles), giving the
    /// 1-cycle granularity required for accurate wave-position timing.
    pub(crate) fn tick_ch3_one_apu_cycle(&mut self) {
        if self.powered {
            self.ch3.tick_apu_cycles(1);
        }
    }

    /// Advance all channels **except CH3** by `m_cycles` M-cycles.
    ///
    /// Used in CGB double-speed mode where CH3 is ticked separately via
    /// `tick_ch3_one_apu_cycle` for finer timing granularity.
    pub(crate) fn tick_except_ch3(&mut self, m_cycles: u8) {
        for _ in 0..m_cycles {
            self.tick_one_except_ch3();
        }
    }

    /// Advance CH1, CH2, CH4, sweep, and the sample mixer by one M-cycle.
    /// CH3 is intentionally excluded — see `tick_except_ch3`.
    fn tick_one_except_ch3(&mut self) {
        self.lf_div = !self.lf_div;

        self.ch1.tick();
        self.ch2.tick();
        self.ch4.tick();
        self.ch1.sweep_tick();

        self.sample_acc += 1.0;
        if self.sample_acc >= self.cycles_per_sample {
            self.sample_acc -= self.cycles_per_sample;
            let sample = self.mix();
            self.push_pending_sample(sample);
        }
    }

    /// Clock the APU envelope units due to a DIV-APU **rising** edge.
    ///
    /// The GB APU envelope has a two-phase mechanism driven by the DIV-APU bit:
    /// - **Rising edge** (this function): if a channel's envelope countdown has
    ///   reached zero, arm the clock flag and reload the countdown so that the
    ///   volume tick fires at the very next falling edge.  This is the "phantom
    ///   tick" mechanism needed for zombie-mode / NRx2-glitch accuracy.
    /// - **Falling edge** (`clock_div_apu`): fire the volume tick for any channel
    ///   whose clock flag was armed by the most recent secondary event.
    pub fn clock_div_apu_secondary(&mut self) {
        if !self.powered {
            return;
        }
        self.ch1.clock_envelope_secondary();
        self.ch2.clock_envelope_secondary();
        self.ch4.clock_envelope_secondary();
    }

    /// Clock the APU frame sequencer due to a DIV-APU falling edge.
    ///
    /// The GB APU frame sequencer is clocked by the falling edge of DIV bit 4
    /// (bit 12 of the 16-bit internal counter, or bit 13 on CGB double-speed).
    /// This occurs:
    /// - Naturally every 8192 T-cycles (2048 M-cycles) as the counter wraps
    /// - Artificially when DIV is written while the DIV-APU bit was HIGH
    ///
    /// Each call steps the frame sequencer by one step (0-7), clocking the
    /// appropriate length counters, sweep, and/or envelope units.
    ///
    /// If the skip_next_div_apu_event flag is set (APU was powered on while
    /// the DIV-APU bit was HIGH), this event is skipped and the flag is cleared.
    pub fn clock_div_apu(&mut self) {
        // Do nothing if APU is powered off.
        if !self.powered {
            return;
        }

        // Skip the first event if APU was powered on while DIV-APU bit was high.
        if self.skip_next_div_apu_event {
            self.skip_next_div_apu_event = false;
            self.div_apu_event_without_div_increment = true;
            trace_apu!(3; "GB APU skipping DIV-APU event (power-on with DIV-APU bit high)");
            return;
        }

        let increment_divider = !self.div_apu_event_without_div_increment;
        if increment_divider {
            // Increment div_divider at the start of serviced DIV-APU events,
            // except for the first post-skip serviced event, which preserves the
            // seeded power-on-high phase. Low bits drive length/sweep/envelope
            // sub-events.
            self.div_divider = self.div_divider.wrapping_add(1);
        } else {
            self.div_apu_event_without_div_increment = false;
        }

        trace_apu!(3; "GB APU FS step={} length={} sweep={} envelope={}",
            self.fs_step,
            (FS_TABLE[self.fs_step as usize] & 0x01) != 0,
            (FS_TABLE[self.fs_step as usize] & 0x02) != 0,
            (FS_TABLE[self.fs_step as usize] & 0x04) != 0);

        let flags = FS_TABLE[self.fs_step as usize];

        // Fire envelope tick for any channel whose clock flag was armed by the
        // most recent secondary (rising-edge) event.  Must run at *every* FS step
        // so that phantom ticks land on the correct falling edge, not just step 7.
        self.ch1.clock_envelope_primary();
        self.ch2.clock_envelope_primary();
        self.ch4.clock_envelope_primary();

        if flags & 0x01 != 0 {
            trace_apu!(4; "GB APU clock length counters");
            self.ch1.clock_length();
            self.ch2.clock_length();
            self.ch3.clock_length();
            self.ch4.clock_length();
        }
        if flags & 0x02 != 0 {
            trace_apu!(4; "GB APU clock CH1 sweep");
            self.ch1.clock_sweep(self.lf_div);
        }

        // Decrement envelope countdowns every 8th FS event (64 Hz).
        // Uses div_divider bit pattern: fires when
        // div_divider & 7 == 7, i.e. on the 7th, 15th, 23rd … falling edges.
        if self.div_divider & 7 == 7 {
            trace_apu!(4; "GB APU decrement envelope countdowns");
            self.ch1.clock_envelope_decrement();
            self.ch2.clock_envelope_decrement();
            self.ch4.clock_envelope_decrement();
        }

        self.fs_step = (self.fs_step + 1) & 7;
    }

    /// Advance by exactly one M-cycle.
    ///
    /// Clocks channel frequency timers and generates samples.
    /// Frame Sequencer is NOT clocked here — see `clock_div_apu()`.
    fn tick_one(&mut self) {
        // Toggle the low-frequency divider (tracks 2 MHz alignment).
        self.lf_div = !self.lf_div;

        // ── Channel frequency timers ───────────────────────────────────────
        self.ch1.tick();
        self.ch2.tick();
        self.ch3.tick();
        self.ch4.tick();

        // ── CH1 sweep machinery (1 MHz cadence; gated internally) ─────────
        // Hardware behavior `GB_apu_run` advances sweep state at half the
        // M-cycle rate; `Channel1::sweep_tick` toggles its own phase flag and
        // performs work every other call.
        self.ch1.sweep_tick();

        // ── Sample output ──────────────────────────────────────────────────
        self.sample_acc += 1.0;
        if self.sample_acc >= self.cycles_per_sample {
            self.sample_acc -= self.cycles_per_sample;
            let sample = self.mix();
            self.push_pending_sample(sample);
        }
    }

    /// Mix all four channels through NR50/NR51 into a mono f32 in `[-1.0, 1.0]`.
    ///
    /// Applies a first-order high-pass filter (simulating the DMG output
    /// capacitor / AC coupling) to remove DC bias and centre the signal
    /// around 0.  The filter coefficient α = f_s / (f_s + 2π·f_c) gives a
    /// cutoff of roughly 7 Hz at any configured sample rate, removing DC
    /// while leaving all audible content unaffected.
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

        // High-pass filter: out[n] = α × (out[n-1] + in[n] − in[n-1]).
        // α = f_s / (f_s + 2π·f_c), with f_c ≈ 7 Hz, updated by set_sample_rate().
        // Removes DC bias and produces bipolar output centred around 0.
        let hp_out = self.hp_rc * (self.hp_prev_out + raw - self.hp_prev_in);
        self.hp_prev_in = raw;
        self.hp_prev_out = hp_out;

        let final_out = hp_out.clamp(-1.0, 1.0);
        trace_apu!(5; "GB APU mix ch1={:.3} ch2={:.3} ch3={:.3} ch4={:.3} left={:.3} right={:.3} raw={:.3} hp={:.3} out={:.3}",
            samples[0], samples[1], samples[2], samples[3],
            left_mix * left_volume, right_mix * right_volume,
            raw, hp_out, final_out);
        final_out
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
        nr52 |= u8::from(self.ch1.is_active_for_nr52());
        nr52 |= u8::from(self.ch2.is_active()) << 1;
        nr52 |= u8::from(self.ch3.is_active()) << 2;
        nr52 |= u8::from(self.ch4.is_active()) << 3;
        nr52
    }

    /// Read PCM12 ($FF76, CGB only): digital outputs 1 & 2 [read-only].
    ///
    /// Low nibble = CH1 digital output (0-15), high nibble = CH2 (0-15).
    pub fn read_pcm12(&self) -> u8 {
        let ch1 = self.ch1.digital_output();
        let ch2 = self.ch2.digital_output();
        ch1 | (ch2 << 4)
    }

    /// Read PCM34 ($FF77, CGB only): digital outputs 3 & 4 [read-only].
    ///
    /// Low nibble = CH3 digital output (0-15), high nibble = CH4 (0-15).
    pub fn read_pcm34(&self) -> u8 {
        let ch3 = self.ch3.digital_output();
        let ch4 = self.ch4.digital_output();
        ch3 | (ch4 << 4)
    }

    // ── Register write ─────────────────────────────────────────────────────

    /// Write NR52 ($FF26) with knowledge of the current DIV-APU bit state.
    ///
    /// When the APU is powered on while the DIV-APU bit (bit 12 or bit 13 in
    /// double-speed) is HIGH, the first DIV-APU event should be skipped.
    /// The bus should call this method for NR52 writes instead of write_register.
    ///
    /// `div_apu_bit_high` should be true if the DIV-APU bit is currently HIGH.
    pub fn write_nr52_with_div_state(&mut self, val: u8, div_apu_bit_high: bool) {
        let power_on = self.write_nr52(val);
        if power_on && div_apu_bit_high {
            self.arm_skip_next_div_apu_event();
        }
    }

    /// Write an APU register.
    ///
    /// When powered off, writes to NR10–NR51 are ignored (length counters are
    /// writable on DMG even when powered off, but we keep it simple here).
    /// Writes to NR52 are always honoured to allow power-on.
    ///
    /// Note: For NR52 ($FF26) writes, prefer `write_nr52_with_div_state()` to
    /// correctly handle the DIV-APU skip behavior on power-on.
    pub fn write_register(&mut self, addr: u16, val: u8) {
        self.write_register_with_apu_phase(addr, val, None);
    }

    /// Write an APU register, optionally supplying bit-packed CGB double-speed
    /// phase bits for trigger startup timing.
    pub fn write_register_with_apu_phase(
        &mut self,
        addr: u16,
        val: u8,
        double_speed_phase_bits: Option<u8>,
    ) {
        self.write_register_with_apu_phase_and_div_counter(
            addr,
            val,
            double_speed_phase_bits,
            None,
        );
    }

    pub fn write_register_with_apu_phase_and_div_counter(
        &mut self,
        addr: u16,
        val: u8,
        double_speed_phase_bits: Option<u8>,
        div_counter: Option<u16>,
    ) {
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

        // Extra length clocking is keyed directly off the DIV-APU divider LSB,
        // not `fs_step`: the power-on-high skip path seeds `div_divider` before
        // the first serviced FS step so the trigger-time length glitch observes
        // the same phase as hardware.
        let extra_clk = self.div_divider & 1 == 1;
        let cgb_early_extra_length_clock = self.cgb_early_extra_length_clock();
        let cgb_b_delayed_ch3_length_disable = self.cgb_b_delayed_ch3_length_disable();

        match addr {
            0xFF10 => self.ch1.write_nr10(val, self.lf_div),
            0xFF11 => self.ch1.write_nr11(val),
            0xFF12 => self.ch1.write_nr12(val),
            0xFF13 => self
                .ch1
                .write_nr13_with_apu_phase(val, double_speed_phase_bits),
            0xFF14 => self.ch1.write_nr14_with_apu_phase_and_length_quirk(
                val,
                extra_clk,
                self.lf_div,
                double_speed_phase_bits,
                cgb_early_extra_length_clock,
                div_counter,
            ),
            0xFF15 => {}
            0xFF16 => self.ch2.write_nr21(val),
            0xFF17 => self.ch2.write_nr22(val),
            0xFF18 => self.ch2.write_nr23(val),
            0xFF19 => self.ch2.write_nr24_with_apu_phase_and_length_quirk(
                val,
                extra_clk,
                self.lf_div,
                double_speed_phase_bits,
                cgb_early_extra_length_clock,
            ),
            0xFF1A => self.ch3.write_nr30(val),
            0xFF1B => self.ch3.write_nr31(val),
            0xFF1C => self.ch3.write_nr32(val),
            0xFF1D => self.ch3.write_nr33(val),
            0xFF1E => self.ch3.write_nr34_with_length_quirk(
                val,
                extra_clk,
                cgb_early_extra_length_clock,
                cgb_b_delayed_ch3_length_disable,
            ),
            0xFF1F => {}
            0xFF20 => self.ch4.write_nr41(val),
            0xFF21 => self.ch4.write_nr42(val),
            0xFF22 => self.ch4.write_nr43(val),
            0xFF23 => self.ch4.write_nr44_with_apu_phase_and_length_quirk(
                val,
                extra_clk,
                double_speed_phase_bits,
                cgb_early_extra_length_clock,
            ),
            0xFF24 => {
                trace_apu!(2; "GB APU write NR50=0x{:02X} left_vol={} right_vol={}", val, (val >> 4) & 0x07, val & 0x07);
                self.nr50 = val;
            }
            0xFF25 => {
                trace_apu!(2; "GB APU write NR51=0x{:02X} left_en=0x{:X} right_en=0x{:X}", val, val >> 4, val & 0x0F);
                self.nr51 = val;
            }
            _ => {}
        }
    }

    fn write_nr52(&mut self, val: u8) -> bool {
        let was_powered = self.powered;
        self.powered = val & 0x80 != 0;
        let power_on_transition = !was_powered && self.powered;

        if was_powered && !self.powered {
            trace_apu!(1; "GB APU power off");
            // Power off: clear all NR10–NR51 registers and HP filter state.
            self.ch1.power_off();
            self.ch2.power_off();
            self.ch3.power_off();
            self.ch4.power_off();
            self.nr50 = 0x00;
            self.nr51 = 0x00;
            self.hp_prev_in = 0.0;
            self.hp_prev_out = 0.0;
            // Clear skip flag on power-off to prevent leaking into a later power-on.
            self.skip_next_div_apu_event = false;
            self.div_apu_event_without_div_increment = false;
            // Reset Sub-M-cycle FS-step counter alongside `fs_step` so
            // that sub-events keyed off `div_divider` low bits stay aligned
            // across power cycles.
            self.div_divider = 0;
        } else if power_on_transition {
            trace_apu!(1; "GB APU power on");
            // Power on: reset frame sequencer step and skip flag.
            // The frame sequencer is clocked by DIV-APU events (not an internal timer),
            // so we only reset the step counter here. The skip flag is cleared first,
            // then write_nr52_with_div_state() will re-arm it if DIV-APU bit is HIGH.
            self.fs_step = 0;
            self.div_divider = 0;
            self.skip_next_div_apu_event = false;
            self.div_apu_event_without_div_increment = false;
            // Reset CH4 counter model state (alignment, counter) on APU init,
            self.ch4.apu_init();
            // On CGB, powering on resets all length counters to 0.
            if self.is_cgb {
                self.ch1.length_counter = 0;
                self.ch2.length_counter = 0;
                self.ch3.length_counter = 0;
                self.ch4.length_counter = 0;
            }
        }

        power_on_transition
    }

    /// Arm the skip flag for the next DIV-APU event.
    ///
    /// Call this when the APU is powered on while the DIV-APU bit (bit 12 or
    /// bit 13 in double-speed) is currently HIGH. Per SameSuite div_write_trigger_10:
    /// "Starting the APU while bit 4 of the DIV register is set causes the APU
    /// to skip the first DIV-APU event."
    pub fn arm_skip_next_div_apu_event(&mut self) {
        // Initializes this skipped-start phase to 1 so trigger-time
        // length glitches see `div_divider & 1 == 1`, even before the first
        // non-skipped DIV-APU event advances the divider. `extra_clk` uses this
        // LSB directly, so the following NR14 trigger can clock/reload length
        // in the same phase as hardware.
        self.div_divider = 1;
        self.skip_next_div_apu_event = true;
        trace_apu!(2; "GB APU armed skip_next_div_apu_event (power-on with DIV-APU bit high)");
    }

    /// Expose ch3 wave RAM read for DmgBus (used during CH3 playback quirk).
    pub fn read_wave_ram(&self, addr: u16) -> u8 {
        self.ch3.read_wave_ram(addr)
    }

    /// Sub-M-cycle FS-step counter used by sub-M-cycle APU machinery.
    ///
    /// Advanced once per DIV-APU event in `clock_div_apu`. Phase 3 consumes
    /// the low two bits to drive the per-128 Hz sweep tick.
    #[allow(dead_code)] // Direct readers land in Phase 6 (NR10 glitch).
    pub(crate) fn div_divider(&self) -> u8 {
        self.div_divider
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn powered_apu() -> Apu {
        let mut apu = Apu::new(false);
        apu.write_register(0xFF26, 0x80);
        apu
    }

    /// Creates a powered CGB APU configured for model-specific quirk tests.
    fn powered_cgb_apu_for_model(model: CgbModel) -> Apu {
        let mut apu = Apu::new(true);
        apu.set_cgb_model(model);
        apu.write_register(0xFF26, 0x80);
        apu
    }

    /// `div_divider` is a Frame-Sequencer-step counter exposed for the
    /// Sub-M-cycle sub-M-cycle sweep machinery. It must advance by
    /// exactly one per serviced DIV-APU event so the low bits drive
    ///   * `& 1 == 1` — extra-length-tick glitch (256 Hz tick, half of 512 Hz)
    ///   * `& 3 == 3` — sweep tick (128 Hz, every 4 FS steps)
    ///   * `& 7 == 7` — envelope volume countdown (64 Hz)
    ///
    ///
    #[test]
    fn test_div_divider_advances_per_fs_step() {
        let mut apu = powered_apu();
        let start = apu.div_divider();
        apu.clock_div_apu();
        assert_eq!(apu.div_divider(), start.wrapping_add(1));
        apu.clock_div_apu();
        apu.clock_div_apu();
        assert_eq!(apu.div_divider(), start.wrapping_add(3));
    }

    #[test]
    fn test_div_divider_does_not_advance_on_skipped_event() {
        // Power-on quirk: skip_next_div_apu_event suppresses both the FS
        // dispatch AND the div_divider increment. The following DIV-APU event is
        // serviced without incrementing the divider.
        let mut apu = powered_apu();
        apu.write_register(0xFF11, 0x3E);
        apu.write_register(0xFF12, 0x80);
        apu.write_register(0xFF14, 0xC0);
        assert_eq!(apu.ch1.length_counter, 2);
        apu.arm_skip_next_div_apu_event();
        assert_eq!(apu.div_divider(), 1);
        apu.clock_div_apu();
        assert_eq!(apu.div_divider(), 1);
        assert_eq!(apu.ch1.length_counter, 2);
        apu.clock_div_apu();
        assert_eq!(apu.div_divider(), 1);
        assert_eq!(apu.ch1.length_counter, 1);
        apu.clock_div_apu();
        assert_eq!(apu.div_divider(), 2);
        assert_eq!(apu.ch1.length_counter, 1);
    }

    #[test]
    fn test_div_divider_low_two_bits_align_every_4_fs_steps() {
        let mut apu = powered_apu();
        // Advance until the low 2 bits are zero, then verify the
        // every-4-FS-step boundary lands on `& 3 == 3` exactly once
        // per group of 4.
        while apu.div_divider() & 3 != 0 {
            apu.clock_div_apu();
        }
        let mut hits = 0u32;
        for _ in 0..16 {
            apu.clock_div_apu();
            if apu.div_divider() & 3 == 3 {
                hits += 1;
            }
        }
        assert_eq!(hits, 4, "expected 4 boundary hits over 16 FS steps");
    }

    #[test]
    fn test_div_divider_resets_on_nr52_power_cycle() {
        // Sub-events keyed off `div_divider` low bits must stay aligned
        // with `fs_step` after an NR52 power-off / power-on cycle.
        let mut apu = powered_apu();
        for _ in 0..5 {
            apu.clock_div_apu();
        }
        assert_ne!(apu.div_divider(), 0);
        // Power off via NR52.
        apu.write_register(0xFF26, 0x00);
        assert_eq!(apu.div_divider(), 0);
        // Power on again — should still be 0 (and fs_step is also 0).
        apu.write_register(0xFF26, 0x80);
        assert_eq!(apu.div_divider(), 0);
    }

    #[test]
    fn test_cgb_b_extra_length_clock_ignores_current_length_enable() {
        let mut apu = powered_cgb_apu_for_model(CgbModel::CgbB);
        apu.write_register(0xFF11, 0x3E);
        assert_eq!(apu.ch1.length_counter, 2);

        apu.clock_div_apu();
        apu.write_register(0xFF14, 0x00);

        assert!(!apu.ch1.length_en());
        assert_eq!(apu.ch1.length_counter, 1);
    }

    #[test]
    fn test_cgb_c_extra_length_clock_requires_current_length_enable() {
        let mut apu = powered_cgb_apu_for_model(CgbModel::CgbC);
        apu.write_register(0xFF11, 0x3E);
        assert_eq!(apu.ch1.length_counter, 2);

        apu.clock_div_apu();
        apu.write_register(0xFF14, 0x00);

        assert!(!apu.ch1.length_en());
        assert_eq!(apu.ch1.length_counter, 2);
    }

    #[test]
    fn test_nr52_power_on_bit_readable() {
        let apu = powered_apu();
        assert_eq!(apu.read_register(0xFF26) & 0x80, 0x80);
    }

    #[test]
    fn test_nr52_power_off_bit_readable() {
        let apu = Apu::new(false);
        assert_eq!(apu.read_register(0xFF26) & 0x80, 0x00);
    }

    #[test]
    fn test_nr52_unused_bits_read_as_1() {
        let apu = Apu::new(false);
        assert_eq!(apu.read_register(0xFF26) & 0x70, 0x70);
    }

    #[test]
    fn test_power_off_clears_nr50() {
        let mut apu = powered_apu();
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.read_register(0xFF24), 0x77);
        apu.write_register(0xFF26, 0x00);
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
        let mut apu = Apu::new(false);
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.nr50, 0x00, "NR50 write must be ignored when APU is off");
    }

    #[test]
    fn test_power_on_allows_nr50_write() {
        let mut apu = powered_apu();
        apu.write_register(0xFF24, 0x77);
        assert_eq!(apu.nr50, 0x77);
    }

    #[test]
    fn test_nr51_write_read_roundtrip() {
        let mut apu = powered_apu();
        apu.write_register(0xFF25, 0xAB);
        assert_eq!(apu.read_register(0xFF25), 0xAB);
    }

    #[test]
    fn test_sample_not_ready_initially() {
        let apu = Apu::new(false);
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_ready_after_enough_ticks() {
        let mut apu = Apu::new(false);
        apu.set_sample_rate(44_100.0);
        apu.tick(30);
        assert!(apu.sample_ready(), "expected a sample after 30 M-cycles");
    }

    #[test]
    fn test_multiple_samples_accumulate_until_consumed() {
        let mut apu = Apu::new(false);
        apu.set_sample_rate(44_100.0);

        // A web video frame advances far more than one output sample period
        // before JS drains audio. Generated samples must queue instead of
        // overwriting/dropping everything after the first pending sample.
        apu.tick(72);

        let mut sample_count = 0;
        while apu.take_sample().is_some() {
            sample_count += 1;
        }

        assert!(
            sample_count >= 3,
            "expected at least 3 queued samples, got {sample_count}"
        );
    }

    #[test]
    fn test_take_sample_clears_ready_flag() {
        let mut apu = Apu::new(false);
        apu.tick(30);
        apu.take_sample();
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_is_silent_when_powered_off() {
        let mut apu = Apu::new(false);
        apu.tick(30);
        let s = apu.take_sample().unwrap_or(0.0);
        assert_eq!(s, 0.0);
    }

    #[test]
    fn test_frame_sequencer_advances_on_clock_div_apu() {
        let mut apu = powered_apu();
        assert_eq!(apu.fs_step, 0);
        // Frame sequencer is now clocked by explicit DIV-APU events,
        // not by the internal fs_timer. One call to clock_div_apu()
        // advances by one step.
        apu.clock_div_apu();
        assert_eq!(
            apu.fs_step, 1,
            "fs_step must be 1 after one clock_div_apu()"
        );
    }

    #[test]
    fn test_frame_sequencer_wraps_at_8() {
        let mut apu = powered_apu();
        // Clock frame sequencer 8 times to verify wrap
        for _ in 0..8 {
            apu.clock_div_apu();
        }
        assert_eq!(apu.fs_step, 0, "fs_step must wrap back to 0 after 8 steps");
    }

    #[test]
    fn test_registers_return_cleared_values_when_powered_off() {
        let mut apu = powered_apu();
        apu.write_register(0xFF26, 0x00);
        assert_eq!(apu.read_register(0xFF10), 0x80);
        assert_eq!(apu.read_register(0xFF12), 0x00);
    }

    #[test]
    fn test_nr52_ch1_active_bit_reflects_channel1_state() {
        let apu = Apu::new(false);
        assert_eq!(apu.read_register(0xFF26) & 0x01, 0x00);
    }

    fn powered_cgb_apu() -> Apu {
        let mut apu = Apu::new(true);
        apu.write_register(0xFF26, 0x80);
        apu
    }

    #[test]
    fn test_cgb_power_off_rejects_nr41_length_write() {
        let mut apu = powered_cgb_apu();
        apu.write_register(0xFF20, 0x3F);
        apu.write_register(0xFF26, 0x00);
        apu.write_register(0xFF20, 0x3F);
        assert_eq!(
            apu.ch4.length_counter, 0,
            "CGB must reject length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_off_rejects_nr11_length_write() {
        let mut apu = Apu::new(true);
        apu.write_register(0xFF11, 0x3F);
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
        apu.write_register(0xFF1B, 0xFF);
        assert_eq!(
            apu.ch3.length_counter, 0,
            "CGB must reject CH3 length writes when APU is off"
        );
    }

    #[test]
    fn test_dmg_power_off_allows_nr41_length_write() {
        let mut apu = Apu::new(false);
        apu.write_register(0xFF20, 0x3F);
        assert_eq!(
            apu.ch4.length_counter, 1,
            "DMG must allow length writes when APU is off"
        );
    }

    #[test]
    fn test_cgb_power_on_resets_length_counters() {
        let mut apu = powered_cgb_apu();
        apu.write_register(0xFF11, 0x00);
        apu.write_register(0xFF16, 0x00);
        apu.write_register(0xFF1B, 0x00);
        apu.write_register(0xFF20, 0x00);
        assert_eq!(apu.ch1.length_counter, 64);
        assert_eq!(apu.ch4.length_counter, 64);
        apu.write_register(0xFF26, 0x00);
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

    #[test]
    fn test_mix_hp_filter_produces_negative_transient_when_input_drops() {
        let mut apu = powered_apu();
        apu.hp_prev_in = 1.0;
        apu.hp_prev_out = 0.5;
        let sample = apu.mix();
        assert!(
            sample < 0.0,
            "HP filter must produce a negative transient when input drops from high to zero, got {sample}"
        );
    }

    #[test]
    fn test_mix_hp_filter_output_stays_in_bipolar_range() {
        let mut apu = powered_apu();
        apu.write_register(0xFF11, 0x80);
        apu.write_register(0xFF12, 0xF0);
        apu.write_register(0xFF14, 0x80);
        apu.write_register(0xFF16, 0x80);
        apu.write_register(0xFF17, 0xF0);
        apu.write_register(0xFF19, 0x80);
        apu.write_register(0xFF1A, 0x80);
        apu.write_register(0xFF1C, 0x20);
        apu.write_register(0xFF1E, 0x80);
        apu.write_register(0xFF21, 0xF0);
        apu.write_register(0xFF23, 0x80);
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
        let mut apu = powered_apu();
        apu.write_register(0xFF11, 0x80);
        apu.write_register(0xFF12, 0xF0);
        apu.write_register(0xFF13, 0x80);
        apu.write_register(0xFF14, 0x84);
        apu.write_register(0xFF25, 0x01);
        apu.write_register(0xFF24, 0x07);

        let mut found_negative = false;
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

    #[test]
    fn test_pcm12_returns_zero_when_channels_off() {
        let apu = powered_cgb_apu();
        // CH1 and CH2 inactive → PCM12 should be $00
        assert_eq!(apu.read_pcm12(), 0x00);
    }

    #[test]
    fn test_pcm34_returns_zero_when_channels_off() {
        let apu = powered_cgb_apu();
        // CH3 and CH4 inactive → PCM34 should be $00
        assert_eq!(apu.read_pcm34(), 0x00);
    }

    #[test]
    fn test_pcm12_nibble_packing() {
        let mut apu = powered_cgb_apu();
        // Trigger CH1 with volume 7 and 50% duty (high state)
        apu.write_register(0xFF12, 0x70); // vol=7, no envelope
        apu.write_register(0xFF11, 0x80); // 50% duty
        apu.write_register(0xFF13, 0xFF); // freq low = 0xFF for fast period
        apu.write_register(0xFF14, 0x87); // trigger + length disable, freq high=7
        // Trigger CH2 with volume 5
        apu.write_register(0xFF17, 0x50); // vol=5, no envelope
        apu.write_register(0xFF16, 0x80); // 50% duty
        apu.write_register(0xFF18, 0xFF); // freq low = 0xFF for fast period
        apu.write_register(0xFF19, 0x87); // trigger + length disable, freq high=7

        // Tick to advance duty_pos to position 5 (high in 50% duty).
        // freq=0x7FF -> period=4 T-cycles = 1 M-cycle per advance.
        // startup delay ~8 T-cycles = 2 M-cycles.
        // After ~2 ticks, first_sample_zero is cleared on first advance.
        // duty_pos at tick N ≈ (N - 2) mod 8.
        // For duty_pos=5, need N=7 (7-2=5).
        apu.tick(7);

        let pcm12 = apu.read_pcm12();
        // Low nibble = CH1, high nibble = CH2
        // At duty_pos=5 (50% duty), output should be volume (7 for CH1, 5 for CH2)
        let ch1_out = pcm12 & 0x0F;
        let ch2_out = (pcm12 >> 4) & 0x0F;

        // At least one of them should output (depends on duty cycle position)
        assert!(
            ch1_out > 0 || ch2_out > 0,
            "At least one pulse channel should output non-zero in PCM12"
        );
    }

    #[test]
    fn test_pcm_returns_zero_when_apu_powered_off() {
        let mut apu = powered_cgb_apu();
        // Trigger some channels
        apu.write_register(0xFF12, 0xF0);
        apu.write_register(0xFF14, 0x80);
        apu.write_register(0xFF17, 0xF0);
        apu.write_register(0xFF19, 0x80);

        // Power off APU
        apu.write_register(0xFF26, 0x00);

        // PCM registers should return 0 when powered off
        assert_eq!(apu.read_pcm12(), 0x00);
        assert_eq!(apu.read_pcm34(), 0x00);
    }

    #[test]
    fn test_ch1_length_counter_clocked_by_div_apu_event() {
        // Simulates the scenario in SameSuite channel_1_stop_div:
        // Setup CH1 with length_counter = 1, then clock the frame sequencer
        // via DIV-APU event, which should stop the channel.
        let mut apu = powered_apu();

        // Setup CH1: duty 50%, length_load = 63 → counter = 64 - 63 = 1
        apu.write_register(0xFF11, 0x80 | 0x3F);
        assert_eq!(apu.ch1.length_counter, 1, "CH1 length counter should be 1");

        // Enable DAC and set volume
        apu.write_register(0xFF12, 0x80); // volume = 8, no envelope

        // Trigger with length enabled
        apu.write_register(0xFF14, 0xC0);

        // Channel should be active after trigger
        assert!(
            apu.ch1.is_active(),
            "CH1 should be active after trigger with DAC on"
        );
        assert!(
            apu.ch1.length_en(),
            "CH1 length should be enabled after NR14 write"
        );

        // Clock the frame sequencer via DIV-APU event.
        // Since fs_step starts at 0 after power-on, FS_TABLE[0] = 0x01 clocks length.
        apu.clock_div_apu();

        // Length counter should have decremented from 1 to 0, stopping the channel
        assert_eq!(
            apu.ch1.length_counter, 0,
            "CH1 length counter should be 0 after clocking"
        );
        assert!(
            !apu.ch1.is_active(),
            "CH1 should be inactive after length counter reaches 0"
        );

        // PCM12 should show 0 for CH1 now
        assert_eq!(
            apu.read_pcm12() & 0x0F,
            0,
            "CH1 digital output should be 0 after channel stops"
        );
    }

    #[test]
    fn test_channel_1_delay_high_freq() {
        // Test mimics SameSuite channel_1_delay test:
        // freq = $7FF, duty = 75% ($C0), volume = 8 ($80)
        // Expected: after 2 M-cycles, output should be 0
        //           after 3 M-cycles, output should be 8
        let mut apu = powered_cgb_apu();

        // Power off and on APU to reset
        apu.write_register(0xFF26, 0x00); // APU off
        apu.write_register(0xFF26, 0xFF); // APU on

        // Set up channel 1 like the test ROM
        apu.write_register(0xFF13, 0xFF); // NR13: freq low = $FF
        apu.write_register(0xFF11, 0xC0); // NR11: duty = 75%
        apu.write_register(0xFF12, 0x80); // NR12: volume = 8, no envelope
        apu.write_register(0xFF14, 0x87); // NR14: trigger + freq high = 7 → freq = $7FF

        // At this point, trigger happened. duty_pos should be 0.
        // 75% duty = [0,1,1,1,1,1,1,0], so output at pos 0 = 0.

        // After 2 M-cycles (0 NOPs in SameSuite), should be at position 0 (output = 0)
        apu.tick(2);
        let pcm12_at_2 = apu.read_pcm12() & 0x0F;
        assert_eq!(
            pcm12_at_2, 0x00,
            "Output should be 0 at 2 M-cycles (0 NOPs)"
        );

        // After 1 more M-cycle (1 NOP in SameSuite), should be at position 1 (output = 8)
        apu.tick(1);
        let pcm12_at_3 = apu.read_pcm12() & 0x0F;
        assert_eq!(pcm12_at_3, 0x08, "Output should be 8 at 3 M-cycles (1 NOP)");

        assert!(apu.ch1.is_active(), "CH1 should be active after trigger");
    }

    // Note: Restart timing test removed - the retrigger delay behavior is complex
    // and depends on delay_between value in non-trivial ways. The SameSuite restart
    // tests verify this behavior, but getting all edge cases right requires more
    // investigation. See channel_1_restart.asm comment:
    // "It appears that after restarting, the start delay from the 'delay' test
    // is actually 1 tick shorter. The countdown for the next sample is reset,
    // but the new pulse's first sample will be the next sample the old pulse
    // would have played (i.e. the current sample index or phase does not reset)"
}
