//! SNES S-DSP voice pipeline (work in progress).

mod brr;
mod echo;
mod envelope;
mod gaussian;
mod voice;

use echo::EchoState;
use serde::{Deserialize, Serialize};
use voice::{EnvelopeMode, VoiceState};

pub use brr::DecodedBrrBlock;
pub use brr::decode_brr_block;
use brr::decode_brr_group;

const DSP_REGISTER_COUNT: usize = 0x80;
const KON_REG: u8 = 0x4C;
const KOFF_REG: u8 = 0x5C;
const PMON_REG: u8 = 0x2D;
const NON_REG: u8 = 0x3D;
const FLG_REG: u8 = 0x6C;
const ENDX_REG: u8 = 0x7C;

fn default_regs() -> Vec<u8> {
    vec![0; DSP_REGISTER_COUNT]
}

/// Temporary diagnostic for #2914: mirrors the `NESER_SPC_DSP6_TRACE` hook
/// patched into the local Mesen2 build (DspVoice::Step3c) so voice-0 state
/// can be diffed line-by-line between the two emulators.
pub(crate) fn spc_dsp6_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("NESER_SPC_DSP6_TRACE").is_some())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sdsp {
    #[serde(default)]
    phase: u8,
    #[serde(default = "default_regs")]
    regs: Vec<u8>,
    #[serde(default)]
    voices: [VoiceState; 8],
    #[serde(default)]
    master_vol_l: i8,
    #[serde(default)]
    master_vol_r: i8,
    #[serde(default)]
    echo_vol_l: i8,
    #[serde(default)]
    echo_vol_r: i8,
    #[serde(default)]
    echo_feedback: i8,
    #[serde(default)]
    echo_enable: u8,
    #[serde(default)]
    echo_enable_current: u8,
    #[serde(default)]
    flg: u8,
    #[serde(default)]
    endx: u8,
    #[serde(default)]
    dir: u8,
    #[serde(default)]
    esa: u8,
    #[serde(default)]
    edl: u8,
    #[serde(default)]
    kon_pending: u8,
    #[serde(default)]
    kon_active: u8,
    #[serde(default)]
    kon_latched: u8,
    #[serde(default)]
    koff_latched: u8,
    #[serde(default = "default_kon_poll_slot")]
    kon_poll_slot: bool,
    #[serde(default)]
    fir_coeffs: [i8; 8],
    #[serde(default)]
    echo_state: EchoState,
    #[serde(default = "default_noise_lfsr")]
    noise_lfsr: u16,
    #[serde(default)]
    noise_counter: u16,
    #[serde(default)]
    envelope_counter: u16,
    #[serde(default)]
    main_out_l: i32,
    #[serde(default)]
    main_out_r: i32,
    #[serde(default)]
    echo_out_l: i32,
    #[serde(default)]
    echo_out_r: i32,
    #[serde(default)]
    output_accumulated: bool,
    #[serde(default)]
    envx_latch: u8,
    #[serde(default)]
    outx_latch: u8,
    #[serde(default)]
    last_output_l: i32,
    #[serde(default)]
    last_output_r: i32,
}

impl Default for Sdsp {
    fn default() -> Self {
        Self::new()
    }
}

fn default_noise_lfsr() -> u16 {
    0x4000
}

fn default_kon_poll_slot() -> bool {
    true
}

impl Sdsp {
    #[must_use]
    pub fn new() -> Self {
        let mut regs = default_regs();
        regs[usize::from(FLG_REG)] = 0xE0;
        Self {
            phase: 0,
            regs,
            voices: std::array::from_fn(|_| VoiceState::default()),
            master_vol_l: 0,
            master_vol_r: 0,
            echo_vol_l: 0,
            echo_vol_r: 0,
            echo_feedback: 0,
            echo_enable: 0,
            echo_enable_current: 0,
            flg: 0xE0,
            endx: 0,
            dir: 0,
            esa: 0,
            edl: 0,
            kon_pending: 0,
            kon_active: 0,
            kon_latched: 0,
            koff_latched: 0,
            kon_poll_slot: true,
            fir_coeffs: [0; 8],
            echo_state: EchoState::new(),
            noise_lfsr: default_noise_lfsr(),
            noise_counter: 0,
            envelope_counter: 0,
            main_out_l: 0,
            main_out_r: 0,
            echo_out_l: 0,
            echo_out_r: 0,
            output_accumulated: false,
            envx_latch: 0,
            outx_latch: 0,
            last_output_l: 0,
            last_output_r: 0,
        }
    }

    /// Console-reset behavior (Mesen `Dsp::Reset`): FLG acts as if written
    /// with $E0, the pipeline restarts at phase 0 with power-on counters and
    /// noise LFSR, and echo positions restart. All other DSP registers keep
    /// their values across a reset.
    pub fn reset(&mut self) {
        self.regs[usize::from(FLG_REG)] = 0xE0;
        self.flg = 0xE0;
        self.phase = 0;
        self.envelope_counter = 0;
        self.noise_lfsr = default_noise_lfsr();
        self.noise_counter = 0;
        self.kon_poll_slot = default_kon_poll_slot();
        self.echo_state = EchoState::new();
    }

    pub fn normalize_after_restore(&mut self) -> Result<(), String> {
        self.phase &= 0x1F;
        if self.regs.is_empty() {
            self.regs = default_regs();
        }
        if self.regs.len() != DSP_REGISTER_COUNT {
            return Err(format!(
                "APU DSP register file size mismatch (expected {DSP_REGISTER_COUNT}, found {})",
                self.regs.len()
            ));
        }
        if self.noise_lfsr == 0 || self.noise_lfsr > 0x7FFF {
            self.noise_lfsr = default_noise_lfsr();
        }
        self.echo_state.normalize_after_restore();
        self.rebuild_cached_fields_from_regs();
        Ok(())
    }

    fn rebuild_cached_fields_from_regs(&mut self) {
        self.master_vol_l = self.regs[0x0C] as i8;
        self.master_vol_r = self.regs[0x1C] as i8;
        self.echo_feedback = self.regs[0x0D] as i8;
        self.echo_vol_l = self.regs[0x2C] as i8;
        self.echo_vol_r = self.regs[0x3C] as i8;
        self.echo_enable = self.regs[0x4D];
        self.dir = self.regs[0x5D];
        self.flg = self.regs[0x6C];
        self.esa = self.regs[0x6D];
        self.edl = self.regs[0x7D];

        for voice in 0..8usize {
            let base = voice << 4;
            let v = &mut self.voices[voice];
            v.vol_l = self.regs[base] as i8;
            v.vol_r = self.regs[base + 1] as i8;
            v.pitch = u16::from(self.regs[base + 2]) | (u16::from(self.regs[base + 3] & 0x3F) << 8);
            v.adsr1 = self.regs[base + 5];
            v.adsr1_latch = v.adsr1;
            v.adsr2 = self.regs[base + 6];
            v.gain = self.regs[base + 7];
            self.fir_coeffs[voice] = self.regs[base + 0x0F] as i8;
        }
    }

    #[must_use]
    pub fn phase(&self) -> u8 {
        self.phase
    }

    pub fn set_voice_pitch(&mut self, voice: usize, pitch: u16) {
        let idx = voice_index(voice);
        self.voices[idx].pitch = pitch & 0x3FFF;
    }

    #[must_use]
    pub fn voice_sample_pos(&self, voice: usize) -> u32 {
        u32::from(self.voices[voice_index(voice)].interpolation_pos)
    }

    pub fn step_voice_pitch(&mut self, voice: usize) {
        let idx = voice_index(voice);
        let v = &mut self.voices[idx];
        v.interpolation_pos = ((v.interpolation_pos & 0x3FFF) + v.pitch).min(0x7FFF);
    }

    pub fn set_voice_volume(&mut self, voice: usize, left: i8, right: i8) {
        let idx = voice_index(voice);
        self.voices[idx].vol_l = left;
        self.voices[idx].vol_r = right;
    }

    pub fn set_master_volume(&mut self, left: i8, right: i8) {
        self.master_vol_l = left;
        self.master_vol_r = right;
    }

    #[must_use]
    pub fn render_stereo_sample(&mut self) -> (f32, f32) {
        self.render_stereo_sample_internal(None, self.echo_enable)
    }

    #[must_use]
    pub fn render_stereo_sample_with_memory(&mut self, aram: &mut [u8]) -> (f32, f32) {
        self.render_stereo_sample_internal(Some(aram), self.echo_enable)
    }

    #[must_use]
    pub fn current_stereo_sample(&self) -> (f32, f32) {
        (
            self.last_output_l
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as f32
                / 32768.0,
            self.last_output_r
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as f32
                / 32768.0,
        )
    }

    #[must_use]
    fn render_stereo_sample_internal(
        &mut self,
        aram: Option<&mut [u8]>,
        _echo_enable: u8,
    ) -> (f32, f32) {
        let (dry_l, dry_r, echo_voice_l, echo_voice_r) = if self.output_accumulated {
            (
                self.main_out_l,
                self.main_out_r,
                self.echo_out_l,
                self.echo_out_r,
            )
        } else {
            self.compute_voice_mix_from_current_outputs(self.echo_enable)
        };

        let (left, right) = self.echo_state.process_sample(
            aram,
            self.esa,
            self.edl,
            &self.fir_coeffs,
            self.echo_feedback,
            self.echo_vol_l,
            self.echo_vol_r,
            self.master_vol_l,
            self.master_vol_r,
            self.flg,
            echo_voice_l,
            echo_voice_r,
            dry_l,
            dry_r,
        );
        self.main_out_l = 0;
        self.main_out_r = 0;
        self.echo_out_l = 0;
        self.echo_out_r = 0;
        self.output_accumulated = false;
        self.last_output_l = left;
        self.last_output_r = right;
        self.current_stereo_sample()
    }

    fn compute_voice_mix_from_current_outputs(&self, echo_enable: u8) -> (i32, i32, i32, i32) {
        let mut dry_l = 0i32;
        let mut dry_r = 0i32;
        let mut echo_voice_l = 0i32;
        let mut echo_voice_r = 0i32;
        for voice in 0..8usize {
            let sample = self.voices[voice].current_output;
            let (left, right) = self.mix_voice_sample(voice, sample);
            dry_l = clamp_i16_i32(dry_l + i32::from(left));
            dry_r = clamp_i16_i32(dry_r + i32::from(right));
            if echo_enable & (1 << voice) != 0 {
                echo_voice_l = clamp_i16_i32(echo_voice_l + i32::from(left));
                echo_voice_r = clamp_i16_i32(echo_voice_r + i32::from(right));
            }
        }
        (dry_l, dry_r, echo_voice_l, echo_voice_r)
    }

    #[must_use]
    pub fn mix_voice_sample(&self, voice: usize, sample: i16) -> (i16, i16) {
        let idx = voice_index(voice);
        let left = apply_voice_volume(sample, self.voices[idx].vol_l);
        let right = apply_voice_volume(sample, self.voices[idx].vol_r);
        (left, right)
    }

    #[must_use]
    pub fn gaussian_interpolate(&self, s0: i16, s1: i16, s2: i16, s3: i16, frac: u8) -> i16 {
        gaussian::gaussian_interpolate(s0, s1, s2, s3, frac)
    }

    pub fn step_phase(&mut self) {
        self.step_phase_internal(None);
    }

    pub fn step_phase_with_memory(&mut self, aram: &mut [u8]) {
        self.step_phase_internal(Some(aram));
    }

    fn step_noise_lfsr(&mut self) {
        // The noise clock is the GLOBAL rate counter checked against
        // FLG bits 0-4 (Mesen Dsp::Exec case 30: UpdateCounter, then
        // CheckCounter(FLG & 0x1F)), so step instants follow the counter
        // phase, not a private divider.
        if !envelope::envelope_tick_due(
            self.envelope_counter,
            self.regs[usize::from(FLG_REG)] & 0x1F,
        ) {
            return;
        }
        let bit0 = self.noise_lfsr & 1;
        let bit1 = (self.noise_lfsr >> 1) & 1;
        let feedback = bit0 ^ bit1;
        self.noise_lfsr = (self.noise_lfsr >> 1) | (feedback << 14);
        self.noise_lfsr &= 0x7FFF;
        if self.noise_lfsr == 0 {
            self.noise_lfsr = default_noise_lfsr();
        }
    }

    fn voice_sample(&self, voice: usize, non: u8, aram: Option<&[u8]>) -> i16 {
        let raw: i16 = if non & (1 << voice) != 0 {
            // The full 15-bit LFSR value, left-shifted to "restore" the low
            // bit (Mesen DspVoice::Step3c: `output = (int16)(NoiseLfsr * 2)`).
            self.noise_lfsr.wrapping_shl(1) as i16
        } else if aram.is_some() {
            let v = &self.voices[voice];
            gaussian::gaussian_interpolate_ring(v.interpolation_pos, &v.sample_buffer, v.buffer_pos)
        } else {
            0x3FFF
        };
        let env = i32::from(self.voices[voice].env_level);
        (((i32::from(raw) * env) >> 11).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16) & !1
    }

    /// Effective pitch step for this sample. Hardware modulates by the full
    /// 15-bit output of the previous voice (Mesen2 DspVoice::Step3c:
    /// `Pitch += ((VoiceOutput >> 5) * Pitch) >> 10`), not the OUTX byte.
    /// The result is NOT clamped to 14 bits: with maximum modulation the
    /// step reaches ~$7FEB, which is what lets the interpolation position
    /// hit its $7FFF ceiling (blargg dsp6 "interp pos clamped at $7FFF").
    /// The modulation term can at most negate the base pitch, so the value
    /// stays in $0000-$7FEB.
    fn effective_pitch_for_voice(&self, voice: usize, pmon: u8) -> u16 {
        let base = i32::from(self.voices[voice].pitch);
        if voice == 0 || (pmon & (1 << voice)) == 0 {
            return base as u16;
        }
        let prev = i32::from(self.voices[voice - 1].mod_source);
        let modulated = base + (((prev >> 5) * base) >> 10);
        modulated as u16
    }

    fn prepare_voice_for_output(&mut self, voice: usize) {
        let v = &mut self.voices[voice];
        if v.kon_delay == 0 {
            return;
        }
        if v.kon_delay == 5 {
            // Key-on start: point at the sample's first block; the first
            // group decodes at stage 4 once the header/data were loaded.
            v.brr_addr = v.brr_next_addr;
            v.brr_offset = 1;
            v.buffer_pos = 0;
            v.brr_header = 0;
        }
        v.env_level = 0;
        v.hidden_env = 0;
        v.kon_delay -= 1;
        // Hardware quirk: the interpolation position is forced to $4000 on
        // the middle key-on delay samples (decoding one group each) and 0 on
        // the others (Mesen2: `(_keyOnDelay & 0x03) ? 0x4000 : 0`).
        v.interpolation_pos = if v.kon_delay & 0x03 != 0 { 0x4000 } else { 0 };
    }

    fn step_voice_envelope_after_output(&mut self, voice: usize) {
        if self.voices[voice].kon_delay == 0 {
            envelope::step_voice_envelope(&mut self.voices[voice], self.envelope_counter);
        }
    }

    fn voice3c_phase_voice(phase: u8) -> Option<usize> {
        match phase {
            30 => Some(0),
            1 => Some(1),
            4 => Some(2),
            7 => Some(3),
            10 => Some(4),
            13 => Some(5),
            16 => Some(6),
            19 => Some(7),
            _ => None,
        }
    }

    fn voice4_phase_voice(phase: u8) -> Option<usize> {
        match phase {
            31 => Some(0),
            2 => Some(1),
            5 => Some(2),
            8 => Some(3),
            11 => Some(4),
            14 => Some(5),
            17 => Some(6),
            20 => Some(7),
            _ => None,
        }
    }

    fn voice1_phase_voice(phase: u8) -> Option<usize> {
        match phase {
            17 => Some(0),
            20 => Some(1),
            31 => Some(2),
            2 => Some(3),
            5 => Some(4),
            8 => Some(5),
            11 => Some(6),
            14 => Some(7),
            _ => None,
        }
    }

    fn voice2_phase_voice(phase: u8) -> Option<usize> {
        match phase {
            21 => Some(0),
            0 => Some(1),
            3 => Some(2),
            6 => Some(3),
            9 => Some(4),
            12 => Some(5),
            15 => Some(6),
            18 => Some(7),
            _ => None,
        }
    }

    fn voice5_phase_voice(phase: u8) -> Option<usize> {
        match phase {
            0 => Some(0),
            3 => Some(1),
            6 => Some(2),
            9 => Some(3),
            12 => Some(4),
            15 => Some(5),
            18 => Some(6),
            21 => Some(7),
            _ => None,
        }
    }

    fn sample_control_tick(&mut self) {
        // Temporary #2938 diagnostic (remove before merge): absolute sample
        // index of every latched KON, for eos-parity comparison vs Mesen.
        if spc_dsp6_trace_enabled() {
            static SAMPLE_NO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = SAMPLE_NO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if !self.kon_poll_slot && self.kon_pending != 0 {
                // This tick will latch (slot toggles to true below).
                eprintln!("neser konlatch sample={} kon={:02X}", n, self.kon_pending);
            }
        }
        self.envelope_counter = if self.envelope_counter == 0 {
            30_719
        } else {
            self.envelope_counter - 1
        };
        self.step_noise_lfsr();
        self.kon_poll_slot = !self.kon_poll_slot;
        if self.kon_poll_slot {
            self.kon_latched = std::mem::take(&mut self.kon_pending);
            self.kon_active = self.kon_latched;
        } else {
            self.kon_latched = 0;
        }
        self.koff_latched = if self.kon_poll_slot {
            self.regs[usize::from(KOFF_REG)]
        } else {
            0
        };
    }

    fn clear_pending_kon_for_active_key_on_delay(&mut self) {
        self.kon_pending &= !self.kon_active;
    }

    fn process_voice3c(
        &mut self,
        voice: usize,
        soft_reset: bool,
        pmon: u8,
        non: u8,
        aram: Option<&[u8]>,
    ) {
        let in_kon_delay = self.voices[voice].kon_delay > 0;
        self.voices[voice].pitch_step = if in_kon_delay {
            0
        } else {
            self.effective_pitch_for_voice(voice, pmon)
        };
        self.prepare_voice_for_output(voice);
        let sample = self.voice_sample(voice, non, aram);
        self.voices[voice].mod_source = sample;
        self.voices[voice].envx = ((self.voices[voice].env_level >> 4).min(0x7F)) as u8;
        self.voices[voice].current_output = sample;
        self.voices[voice].outx = (sample >> 8).clamp(-128, 127) as i8;
        if soft_reset || self.voices[voice].brr_header & 0x03 == 0x01 {
            // FLG.7 soft reset or a BRR end-without-loop block: release and
            // silence the envelope; the current sample's output is kept.
            self.voices[voice].mode = EnvelopeMode::Release;
            self.voices[voice].env_level = 0;
            if soft_reset {
                self.voices[voice].hidden_env = 0;
                self.voices[voice].adsr1_latch = self.voices[voice].adsr1;
            }
        }
        if self.koff_latched & (1 << voice) != 0 {
            self.voices[voice].mode = EnvelopeMode::Release;
        }
        if self.kon_latched & (1 << voice) != 0 {
            let v = &mut self.voices[voice];
            v.kon_delay = 5;
            v.mode = EnvelopeMode::Attack;
        } else if !soft_reset {
            self.step_voice_envelope_after_output(voice);
        }
        if voice == 0 && spc_dsp6_trace_enabled() {
            eprintln!(
                "neser v0 step3c env_volume=${:03X} envx=${:02X} outx=${:02X} mode={} key_on_delay={} key_on=${:02X} key_off=${:02X} eos={} ctr={}",
                self.voices[0].env_level,
                self.voices[0].envx,
                (self.voices[0].current_output >> 8) as u8,
                match self.voices[0].mode {
                    EnvelopeMode::Release => 0,
                    EnvelopeMode::Attack => 1,
                    EnvelopeMode::Decay => 2,
                    EnvelopeMode::Sustain => 3,
                },
                self.voices[0].kon_delay,
                self.kon_latched,
                self.koff_latched,
                u8::from(self.kon_poll_slot),
                self.envelope_counter,
            );
        }
    }

    fn process_voice4(&mut self, voice: usize, aram: Option<&[u8]>) {
        if self.voices[voice].interpolation_pos >= 0x4000
            && let Some(aram) = aram
        {
            self.decode_voice_brr_group(voice, aram);
            let v = &mut self.voices[voice];
            if v.brr_offset >= 7 {
                if v.brr_header & 0x01 != 0 {
                    // End block reached: jump to the (re-fetched) loop
                    // pointer and raise ENDX for this voice.
                    v.brr_addr = v.brr_next_addr;
                    self.endx |= 1 << voice;
                } else {
                    v.brr_addr = v.brr_addr.wrapping_add(9);
                }
                v.brr_offset = 1;
            } else {
                v.brr_offset += 2;
            }
        }
        let v = &mut self.voices[voice];
        v.interpolation_pos = ((v.interpolation_pos & 0x3FFF) + v.pitch_step).min(0x7FFF);
        self.accumulate_voice_output(voice, false);
    }

    /// Decode the next 4-sample BRR group into the voice ring buffer
    /// (Mesen2 `DspVoice::DecodeBrrSample`). The first data byte was loaded
    /// at stage 3b; the second is read here.
    fn decode_voice_brr_group(&mut self, voice: usize, aram: &[u8]) {
        let v = &mut self.voices[voice];
        let next_byte = aram[usize::from(
            v.brr_addr
                .wrapping_add(u16::from(v.brr_offset))
                .wrapping_add(1),
        ) % aram.len().max(1)];
        let prev1 = v.sample_buffer[if v.buffer_pos > 0 {
            usize::from(v.buffer_pos) - 1
        } else {
            11
        }];
        let prev2 = v.sample_buffer[if v.buffer_pos > 1 {
            usize::from(v.buffer_pos) - 2
        } else {
            10
        }];
        let group = decode_brr_group(v.brr_header, v.brr_data, next_byte, prev1, prev2);
        let base = usize::from(v.buffer_pos);
        v.sample_buffer[base..base + 4].copy_from_slice(&group);
        v.buffer_pos = if v.buffer_pos <= 4 {
            v.buffer_pos + 4
        } else {
            0
        };
    }

    /// Stage 1: latch SRCN for use by stage 2 (one slot later on hardware).
    fn process_voice1(&mut self, voice: usize) {
        self.voices[voice].srcn_latch = self.regs[(voice << 4) + 4];
    }

    /// Stage 2: re-fetch the DIR-table pointer for this voice every sample —
    /// the start entry while the key-on delay runs, the loop entry after.
    fn process_voice2(&mut self, voice: usize, aram: Option<&[u8]>) {
        if let Some(aram) = aram {
            let base =
                (usize::from(self.dir) << 8) + usize::from(self.voices[voice].srcn_latch) * 4;
            let entry = if self.voices[voice].kon_delay == 0 {
                base + 2
            } else {
                base
            };
            if let Some(addr) = read_u16_le(aram, entry) {
                self.voices[voice].brr_next_addr = addr;
            }
        }
        self.voices[voice].adsr1_latch = self.voices[voice].adsr1;
    }

    /// Stage 3b: re-read the BRR header and the first data byte of the next
    /// group every sample.
    fn process_voice3b(&mut self, voice: usize, aram: Option<&[u8]>) {
        let Some(aram) = aram else {
            return;
        };
        let v = &mut self.voices[voice];
        if let Some(&header) = aram.get(usize::from(v.brr_addr)) {
            v.brr_header = header;
        }
        if let Some(&data) = aram.get(usize::from(
            v.brr_addr.wrapping_add(u16::from(v.brr_offset)),
        )) {
            v.brr_data = data;
        }
    }

    fn process_voice5(&mut self, voice: usize) {
        self.accumulate_voice_output(voice, true);
        // Key-on clears this voice's ENDX bit (Mesen2 DspVoice::Step5:
        // `if(_keyOnDelay == 5) voiceEnd &= ~_voiceBit`). kon_delay is still 5
        // here because the voice-3c stage of the same sample set it and the
        // countdown only starts next sample.
        if self.voices[voice].kon_delay == 5 {
            self.endx &= !(1 << voice);
        }
    }

    fn accumulate_voice_output(&mut self, voice: usize, right: bool) {
        self.output_accumulated = true;
        let sample = self.voices[voice].current_output;
        let (left, right_sample) = self.mix_voice_sample(voice, sample);
        let value = if right { right_sample } else { left };
        if right {
            self.main_out_r = clamp_i16_i32(self.main_out_r + i32::from(value));
            if self.echo_enable_current & (1 << voice) != 0 {
                self.echo_out_r = clamp_i16_i32(self.echo_out_r + i32::from(value));
            }
        } else {
            self.main_out_l = clamp_i16_i32(self.main_out_l + i32::from(value));
            if self.echo_enable_current & (1 << voice) != 0 {
                self.echo_out_l = clamp_i16_i32(self.echo_out_l + i32::from(value));
            }
        }
    }

    fn step_phase_internal(&mut self, mut aram: Option<&mut [u8]>) {
        let control_tick = self.phase == 30;
        self.refresh_status_registers_for_phase(self.phase);
        if control_tick {
            self.sample_control_tick();
        }
        if let Some(voice) = Self::voice1_phase_voice(self.phase) {
            self.process_voice1(voice);
        }
        if let Some(voice) = Self::voice2_phase_voice(self.phase) {
            let aram_read = aram.as_deref();
            self.process_voice2(voice, aram_read);
        }
        // Voice 0's Step3 is split across phases 22/25/30; voices 1-7 run
        // 3a+3b+3c in a single slot at their voice3c phase.
        if self.phase == 25 || Self::voice3c_phase_voice(self.phase).is_some_and(|v| v != 0) {
            let voice = if self.phase == 25 {
                0
            } else {
                Self::voice3c_phase_voice(self.phase).unwrap_or(0)
            };
            let aram_read = aram.as_deref();
            self.process_voice3b(voice, aram_read);
        }
        if let Some(voice) = Self::voice3c_phase_voice(self.phase) {
            let aram_read = aram.as_deref();
            let soft_reset = self.flg & 0x80 != 0;
            let pmon = self.regs[usize::from(PMON_REG)];
            let non = self.regs[usize::from(NON_REG)];
            self.process_voice3c(voice, soft_reset, pmon, non, aram_read);
        }
        if let Some(voice) = Self::voice4_phase_voice(self.phase) {
            let aram_read = aram.as_deref();
            self.process_voice4(voice, aram_read);
        }
        if let Some(voice) = Self::voice5_phase_voice(self.phase) {
            self.process_voice5(voice);
        }
        if self.phase == 28 {
            self.echo_enable_current = self.echo_enable;
            self.echo_state.sample_left_echo_write_enable(self.flg);
        }
        if self.phase == 29 {
            if !self.kon_poll_slot {
                self.clear_pending_kon_for_active_key_on_delay();
            }
            self.echo_state.sample_right_echo_write_enable(self.flg);
        }
        // Echo pipeline slots (Mesen Dsp::Exec cases 22-30, each at the END
        // of its slot): the ring word is loaded and the FIR sums are built
        // across slots 22-25 reading each FFC coefficient at its own slot,
        // the output samples are assembled at 26/27 (MVOL/EVOL) with the
        // echo feedback applied at 26, the DAC latches the finished sample
        // at 27, and the echo buffer writes happen at 29/30 — all BEFORE
        // voice 0's Step4/5 volume accumulation at slot 31, so V0's fresh
        // output only reaches the echo buffer with the NEXT sample.
        match self.phase {
            22 => {
                let aram_read = aram.as_deref();
                self.echo_state
                    .step_22(aram_read, self.esa, self.fir_coeffs[0]);
            }
            23 => {
                let aram_read = aram.as_deref();
                self.echo_state
                    .step_23(aram_read, self.fir_coeffs[1], self.fir_coeffs[2]);
            }
            24 => {
                self.echo_state
                    .step_24(self.fir_coeffs[3], self.fir_coeffs[4], self.fir_coeffs[5]);
            }
            25 => {
                self.echo_state
                    .step_25(self.fir_coeffs[6], self.fir_coeffs[7]);
            }
            26 => {
                let (echo_in_l, echo_in_r) = self.echo_state.echo_in();
                self.main_out_l = clamp_i16_i32(
                    echo::volume_term(self.main_out_l, self.master_vol_l)
                        + echo::volume_term(echo_in_l, self.echo_vol_l),
                );
                self.echo_out_l = clamp_i16_i32(
                    self.echo_out_l + echo::volume_term(echo_in_l, self.echo_feedback),
                ) & !1;
                self.echo_out_r = clamp_i16_i32(
                    self.echo_out_r + echo::volume_term(echo_in_r, self.echo_feedback),
                ) & !1;
            }
            27 => {
                let (_, echo_in_r) = self.echo_state.echo_in();
                self.main_out_r = clamp_i16_i32(
                    echo::volume_term(self.main_out_r, self.master_vol_r)
                        + echo::volume_term(echo_in_r, self.echo_vol_r),
                );
                // DAC latch (Mesen Dsp::Exec case 27); FLG.6 mutes only the
                // DAC output, not the echo buffer writes.
                if self.flg & 0x40 != 0 {
                    self.last_output_l = 0;
                    self.last_output_r = 0;
                } else {
                    self.last_output_l = self.main_out_l;
                    self.last_output_r = self.main_out_r;
                }
                self.main_out_l = 0;
                self.main_out_r = 0;
                self.output_accumulated = false;
            }
            29 => {
                let value = self.echo_out_l as i16;
                self.echo_state
                    .step_29(aram.as_deref_mut(), self.esa, self.edl, value);
                self.echo_out_l = 0;
            }
            30 => {
                let value = self.echo_out_r as i16;
                self.echo_state.step_30(aram, value);
                self.echo_out_r = 0;
            }
            _ => {}
        }
        self.phase = self.phase.wrapping_add(1) & 0x1F;
    }

    pub fn write_reg(&mut self, addr: u8, value: u8) {
        if addr >= 0x80 {
            return;
        }
        let reg = addr;
        let index = usize::from(reg);
        if reg == ENDX_REG {
            self.endx = 0;
            if index < self.regs.len() {
                self.regs[index] = 0;
            }
            return;
        }
        if index >= self.regs.len() {
            return;
        }
        let voice = usize::from(reg >> 4);
        if voice < 8 && matches!(reg & 0x0F, 0x08 | 0x09) {
            self.regs[index] = value;
            if reg & 0x0F == 0x08 {
                self.envx_latch = value;
            } else {
                self.outx_latch = value;
            }
            return;
        }
        self.regs[index] = value;

        match reg {
            0x0C => {
                self.master_vol_l = value as i8;
                return;
            }
            0x1C => {
                self.master_vol_r = value as i8;
                return;
            }
            0x0D => {
                self.echo_feedback = value as i8;
                return;
            }
            0x2C => {
                self.echo_vol_l = value as i8;
                return;
            }
            0x3C => {
                self.echo_vol_r = value as i8;
                return;
            }
            0x4D => {
                self.echo_enable = value;
                return;
            }
            0x5D => {
                self.dir = value;
                return;
            }
            0x6C => {
                self.flg = value;
                return;
            }
            0x6D => {
                self.esa = value;
                return;
            }
            0x7D => {
                self.edl = value;
                return;
            }
            KON_REG => {
                self.kon_pending = value;
                return;
            }
            KOFF_REG => {
                return;
            }
            _ => {}
        }

        if voice >= 8 {
            return;
        }
        let v = &mut self.voices[voice];
        match reg & 0x0F {
            0x00 => v.vol_l = value as i8,
            0x01 => v.vol_r = value as i8,
            0x02 => {
                let prev = v.pitch;
                v.pitch = (prev & 0x3F00) | u16::from(value);
            }
            0x03 => {
                let prev = v.pitch;
                v.pitch = (prev & 0x00FF) | (u16::from(value & 0x3F) << 8);
            }
            0x05 => v.adsr1 = value,
            0x06 => v.adsr2 = value,
            0x07 => v.gain = value,
            0x08 | 0x09 => {}
            0x0F => self.fir_coeffs[voice] = value as i8,
            _ => {}
        }
    }

    #[must_use]
    pub fn read_reg(&self, addr: u8) -> u8 {
        let index = usize::from(addr & 0x7F);
        if index == usize::from(ENDX_REG) {
            return self.endx;
        }
        self.regs.get(index).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn decode_brr_block(header: u8, data: [u8; 8], prev1: i16, prev2: i16) -> DecodedBrrBlock {
        decode_brr_block(header, data, prev1, prev2)
    }

    fn refresh_status_registers_for_phase(&mut self, phase: u8) {
        for voice in 0..8usize {
            let voice6_phase = 1 + voice as u8 * 3;
            let voice7_phase = 2 + voice as u8 * 3;
            let voice8_phase = 3 + voice as u8 * 3;
            let voice9_phase = 4 + voice as u8 * 3;
            if phase == voice6_phase {
                self.outx_latch = self.voices[voice].outx as u8;
            }
            if phase == voice7_phase {
                self.envx_latch = self.voices[voice].envx;
            }
            if phase == voice8_phase {
                self.regs[(voice << 4) + 9] = self.outx_latch;
            }
            if phase == voice9_phase {
                self.regs[(voice << 4) + 8] = self.envx_latch;
            }
        }
    }
}

fn voice_index(voice: usize) -> usize {
    assert!(voice < 8, "voice index out of range: {voice}");
    voice
}

fn apply_voice_volume(sample: i16, voice_vol: i8) -> i16 {
    clamp_i16_i32((i32::from(sample) * i32::from(voice_vol)) >> 7) as i16
}

fn clamp_i16_i32(value: i32) -> i32 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

fn read_u16_le(data: &[u8], index: usize) -> Option<u16> {
    let lo = *data.get(index)?;
    let hi = *data.get(index + 1)?;
    Some(u16::from(lo) | (u16::from(hi) << 8))
}

#[cfg(test)]
mod tests;
