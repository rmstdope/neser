use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum EnvelopeMode {
    #[default]
    Release,
    Attack,
    Decay,
    Sustain,
}

fn default_brr_offset() -> u8 {
    1
}

/// Per-voice S-DSP state, modeled after the hardware pipeline (Mesen2
/// `DspVoice`): BRR data is decoded lazily in 4-sample groups into a
/// 12-entry ring buffer as the 15-bit interpolation position crosses a
/// group boundary, rather than a block at a time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VoiceState {
    pub pitch: u16,
    /// 15-bit interpolation position; low 12 bits are the gaussian fraction,
    /// bits 12-14 index the sample ring buffer relative to `buffer_pos`.
    pub interpolation_pos: u16,
    /// Ring buffer of the last 3 decoded BRR groups (12 samples).
    pub sample_buffer: [i16; 12],
    /// Write position of the next decoded group (0, 4 or 8).
    pub buffer_pos: u8,
    /// Address of the BRR block currently being decoded.
    pub brr_addr: u16,
    /// Next-block pointer, re-read from the DIR table every sample (start
    /// entry during the key-on delay, loop entry afterwards).
    pub brr_next_addr: u16,
    /// Byte offset of the next 2-byte group inside the block (1/3/5/7).
    pub brr_offset: u8,
    /// Block header, re-read from ARAM every sample at stage 3b.
    pub brr_header: u8,
    /// First data byte of the next group, read at stage 3b.
    pub brr_data: u8,
    /// SRCN value latched at stage 1 (used by stage 2 one slot later).
    pub srcn_latch: u8,
    pub vol_l: i8,
    pub vol_r: i8,
    pub adsr1: u8,
    pub adsr1_latch: u8,
    pub adsr2: u8,
    pub gain: u8,
    pub env_level: u16,
    pub hidden_env: i32,
    pub envx: u8,
    pub outx: i8,
    pub current_output: i16,
    /// Full-resolution voice output used as the pitch-modulation source for
    /// the next voice (hardware uses the 15-bit output, not the OUTX byte).
    pub mod_source: i16,
    pub mode: EnvelopeMode,
    pub kon_delay: u8,
    /// Effective pitch step for this sample (after PMON, zero during the
    /// key-on delay).
    pub pitch_step: u16,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            pitch: 0,
            interpolation_pos: 0,
            sample_buffer: [0; 12],
            buffer_pos: 0,
            brr_addr: 0,
            brr_next_addr: 0,
            brr_offset: default_brr_offset(),
            brr_header: 0,
            brr_data: 0,
            srcn_latch: 0,
            vol_l: 0,
            vol_r: 0,
            adsr1: 0,
            adsr1_latch: 0,
            adsr2: 0,
            gain: 0,
            env_level: 0,
            hidden_env: 0,
            envx: 0,
            outx: 0,
            current_output: 0,
            mod_source: 0,
            mode: EnvelopeMode::default(),
            kon_delay: 0,
            pitch_step: 0,
        }
    }
}
