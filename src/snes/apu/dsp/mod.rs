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
            flg: 0xE0,
            endx: 0,
            dir: 0,
            esa: 0,
            edl: 0,
            fir_coeffs: [0; 8],
            echo_state: EchoState::new(),
            noise_lfsr: default_noise_lfsr(),
            noise_counter: 0,
            envelope_counter: 0,
            last_output_l: 0,
            last_output_r: 0,
        }
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
        self.voices[voice_index(voice)].sample_pos
    }

    pub fn step_voice_pitch(&mut self, voice: usize) {
        let idx = voice_index(voice);
        self.voices[idx].sample_pos = self.voices[idx]
            .sample_pos
            .wrapping_add(u32::from(self.voices[idx].pitch));
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
        self.render_stereo_sample_internal(None)
    }

    #[must_use]
    pub fn render_stereo_sample_with_memory(&mut self, aram: &mut [u8]) -> (f32, f32) {
        self.render_stereo_sample_internal(Some(aram))
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
    fn render_stereo_sample_internal(&mut self, aram: Option<&mut [u8]>) -> (f32, f32) {
        let mut dry_l = 0i32;
        let mut dry_r = 0i32;
        let mut echo_voice_l = 0i32;
        let mut echo_voice_r = 0i32;

        for voice in 0..8usize {
            let sample = self.voices[voice].current_output;
            let (left, right) = self.mix_voice_sample(voice, sample);
            dry_l = clamp_i16_i32(dry_l + i32::from(left));
            dry_r = clamp_i16_i32(dry_r + i32::from(right));
            if self.echo_enable & (1 << voice) != 0 {
                echo_voice_l = clamp_i16_i32(echo_voice_l + i32::from(left));
                echo_voice_r = clamp_i16_i32(echo_voice_r + i32::from(right));
            }
        }

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
        self.last_output_l = left;
        self.last_output_r = right;
        self.current_stereo_sample()
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
        let divider = noise_clock_divider(self.regs[usize::from(FLG_REG)] & 0x1F);
        if divider == 0 {
            return;
        }
        self.noise_counter = self.noise_counter.wrapping_add(1);
        if self.noise_counter < divider {
            return;
        }
        self.noise_counter = 0;
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
            if self.noise_lfsr & 1 == 0 {
                -0x4000
            } else {
                0x3FFF
            }
        } else if aram.is_some() {
            self.current_voice_brr_sample(voice).unwrap_or(0x3FFF)
        } else {
            0x3FFF
        };
        let env = i32::from(self.voices[voice].env_level);
        (((i32::from(raw) * env) >> 11).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16) & !1
    }

    fn effective_pitch_for_voice(&self, voice: usize, pmon: u8) -> u16 {
        let base = self.voices[voice].pitch;
        if voice == 0 || (pmon & (1 << voice)) == 0 {
            return base;
        }
        let prev = i32::from(self.voices[voice - 1].mod_source);
        let factor = (prev >> 4) + 0x400;
        let modulated = (i32::from(base) * factor) >> 10;
        modulated.clamp(0, 0x3FFF) as u16
    }

    fn step_voice_envelope(&mut self, voice: usize, aram: Option<&[u8]>) {
        let mut begin_brr = false;
        {
            let v = &mut self.voices[voice];
            if v.kon_delay > 0 {
                v.kon_delay -= 1;
                if v.kon_delay == 0 {
                    v.mode = EnvelopeMode::Attack;
                    v.env_level = 0;
                    begin_brr = aram.is_some();
                }
            }
            envelope::step_voice_envelope(v, self.envelope_counter);
        }
        if begin_brr && let Some(aram) = aram {
            self.begin_voice_brr_stream(voice, aram);
        }
    }

    fn step_phase_internal(&mut self, mut aram: Option<&mut [u8]>) {
        let sample_tick = self.phase == 31;
        if self.phase == 28 {
            self.echo_state.sample_left_echo_write_enable(self.flg);
        }
        if self.phase == 29 {
            self.echo_state.sample_right_echo_write_enable(self.flg);
            self.echo_state.sample_echo_registers(self.esa, self.edl);
        }
        self.phase = self.phase.wrapping_add(1) & 0x1F;
        if !sample_tick {
            return;
        }

        let soft_reset = self.flg & 0x80 != 0;
        self.envelope_counter = self.envelope_counter.wrapping_add(1);
        self.step_noise_lfsr();
        let pmon = self.regs[usize::from(PMON_REG)];
        let non = self.regs[usize::from(NON_REG)];

        {
            let aram_read = aram.as_deref();
            for voice in 0..8usize {
                if soft_reset {
                    self.soft_reset_voice(voice);
                } else {
                    self.step_voice_envelope(voice, aram_read);
                }
                let effective_pitch = self.effective_pitch_for_voice(voice, pmon);
                self.voices[voice].sample_pos = self.voices[voice]
                    .sample_pos
                    .wrapping_add(u32::from(effective_pitch));
                if let Some(aram) = aram_read {
                    self.advance_voice_brr_stream(voice, aram);
                }
                let sample = if soft_reset {
                    0
                } else {
                    self.voice_sample(voice, non, aram_read)
                };
                let out_before_mix = (sample >> 8).clamp(-128, 127) as i8;
                self.voices[voice].mod_source = out_before_mix;
                self.voices[voice].current_output = sample;
                let (left, right) = self.mix_voice_sample(voice, sample);
                let _mixed =
                    (((i32::from(left) + i32::from(right)) / 2) >> 8).clamp(-128, 127) as i8;
                self.voices[voice].outx = out_before_mix;
                self.regs[(voice << 4) + 8] = self.voices[voice].envx;
                self.regs[(voice << 4) + 9] = out_before_mix as u8;
            }
        }

        if let Some(aram) = aram.as_deref_mut() {
            let _ = self.render_stereo_sample_internal(Some(aram));
        }
    }

    fn begin_voice_brr_stream(&mut self, voice: usize, aram: &[u8]) {
        let (start_addr, loop_addr) = match self.voice_brr_entry(voice, aram) {
            Some(entry) => entry,
            None => {
                self.voices[voice].brr_initialized = false;
                return;
            }
        };
        let v = &mut self.voices[voice];
        v.brr_addr = start_addr;
        v.brr_next_addr = start_addr.wrapping_add(9);
        v.brr_loop_addr = loop_addr;
        v.brr_block_index = 0;
        v.sample_pos = 0;
        v.brr_prev1 = 0;
        v.brr_prev2 = 0;
        v.brr_history = [0; 3];
        v.brr_initialized = true;
        self.load_current_voice_brr_block(voice, aram);
    }

    fn advance_voice_brr_stream(&mut self, voice: usize, aram: &[u8]) {
        if !self.voices[voice].brr_initialized {
            return;
        }
        let target_block = self.voices[voice].sample_pos >> 16;
        while self.voices[voice].brr_block_index < target_block {
            self.load_next_voice_brr_block(voice, aram);
        }
    }

    fn load_current_voice_brr_block(&mut self, voice: usize, aram: &[u8]) {
        let addr = self.voices[voice].brr_addr;
        self.decode_voice_brr_block_at(voice, aram, addr);
    }

    fn load_next_voice_brr_block(&mut self, voice: usize, aram: &[u8]) {
        let next_addr = self.voices[voice].brr_next_addr;
        self.voices[voice].brr_history = [
            self.voices[voice].brr_samples[13],
            self.voices[voice].brr_samples[14],
            self.voices[voice].brr_samples[15],
        ];
        self.voices[voice].brr_addr = next_addr;
        self.voices[voice].brr_block_index = self.voices[voice].brr_block_index.wrapping_add(1);
        self.decode_voice_brr_block_at(voice, aram, next_addr);
    }

    fn decode_voice_brr_block_at(&mut self, voice: usize, aram: &[u8], addr: u16) {
        let Some((header, data)) = read_brr_block_from_aram(aram, addr) else {
            self.voices[voice].brr_samples = [0; 16];
            self.voices[voice].brr_prev1 = 0;
            self.voices[voice].brr_prev2 = 0;
            return;
        };
        let decoded = decode_brr_block(
            header,
            data,
            self.voices[voice].brr_prev1,
            self.voices[voice].brr_prev2,
        );
        self.voices[voice].brr_samples = decoded.samples;
        self.voices[voice].brr_prev1 = decoded.samples[15];
        self.voices[voice].brr_prev2 = decoded.samples[14];
        self.voices[voice].brr_next_addr = if decoded.end_flag {
            if decoded.loop_flag {
                self.voices[voice].brr_loop_addr
            } else {
                addr.wrapping_add(9)
            }
        } else {
            addr.wrapping_add(9)
        };
        if decoded.end_flag {
            self.endx |= 1 << voice;
            if header & 0x03 == 0x01 {
                self.voices[voice].mode = EnvelopeMode::Release;
                self.voices[voice].env_level = 0;
            }
        }
    }

    fn voice_brr_entry(&self, voice: usize, aram: &[u8]) -> Option<(u16, u16)> {
        let srcn = self.regs[(voice << 4) + 4];
        let table_base = usize::from(self.dir) << 8;
        let entry = table_base + usize::from(srcn) * 4;
        let start = read_u16_le(aram, entry)?;
        let loop_addr = read_u16_le(aram, entry + 2)?;
        Some((start, loop_addr))
    }

    fn current_voice_brr_sample(&self, voice: usize) -> Option<i16> {
        let v = &self.voices[voice];
        if !v.brr_initialized {
            return None;
        }
        let index = ((v.sample_pos >> 12) & 0x0F) as usize;
        let frac = ((v.sample_pos >> 4) & 0xFF) as u8;
        Some(gaussian::gaussian_interpolate(
            voice_interpolation_sample(v, index, 3),
            voice_interpolation_sample(v, index, 2),
            voice_interpolation_sample(v, index, 1),
            voice_interpolation_sample(v, index, 0),
            frac,
        ))
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
                if value & 0x80 != 0 {
                    self.soft_reset_voices();
                }
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
                for voice in 0..8 {
                    if value & (1 << voice) != 0 {
                        let v = &mut self.voices[voice];
                        v.kon_delay = 5;
                    }
                }
                return;
            }
            KOFF_REG => {
                for voice in 0..8 {
                    if value & (1 << voice) != 0 {
                        let v = &mut self.voices[voice];
                        v.kon_delay = 0;
                        v.mode = EnvelopeMode::Release;
                    }
                }
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

    fn soft_reset_voices(&mut self) {
        for voice in 0..8usize {
            self.soft_reset_voice(voice);
        }
    }

    fn soft_reset_voice(&mut self, voice: usize) {
        let v = &mut self.voices[voice];
        v.kon_delay = 0;
        v.mode = EnvelopeMode::Release;
        v.env_level = 0;
        v.envx = 0;
        v.outx = 0;
        v.current_output = 0;
        v.mod_source = 0;
        self.regs[(voice << 4) + 8] = 0;
        self.regs[(voice << 4) + 9] = 0;
    }
}

fn voice_index(voice: usize) -> usize {
    assert!(voice < 8, "voice index out of range: {voice}");
    voice
}

fn voice_interpolation_sample(voice: &VoiceState, index: usize, previous: usize) -> i16 {
    if index >= previous {
        return voice.brr_samples[index - previous];
    }
    voice.brr_history[voice.brr_history.len() - (previous - index)]
}

fn apply_voice_volume(sample: i16, voice_vol: i8) -> i16 {
    clamp_i16_i32((i32::from(sample) * i32::from(voice_vol)) >> 7) as i16
}

fn clamp_i16_i32(value: i32) -> i32 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

fn noise_clock_divider(clock_select: u8) -> u16 {
    const NOISE_RATE_TO_DIV: [u16; 32] = [
        0, 2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 64, 48, 40,
        32, 24, 20, 16, 12, 10, 8, 6, 5, 4, 3, 2, 1,
    ];
    NOISE_RATE_TO_DIV[usize::from(clock_select.min(31))]
}

fn read_u16_le(data: &[u8], index: usize) -> Option<u16> {
    let lo = *data.get(index)?;
    let hi = *data.get(index + 1)?;
    Some(u16::from(lo) | (u16::from(hi) << 8))
}

fn read_brr_block_from_aram(aram: &[u8], addr: u16) -> Option<(u8, [u8; 8])> {
    let start = usize::from(addr);
    let header = *aram.get(start)?;
    let mut data = [0u8; 8];
    data.copy_from_slice(aram.get(start + 1..start + 9)?);
    Some((header, data))
}

#[cfg(test)]
mod tests;
