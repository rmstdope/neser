use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum EnvelopeMode {
    #[default]
    Release,
    Attack,
    Decay,
    Sustain,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VoiceState {
    #[serde(default)]
    pub pitch: u16,
    #[serde(default)]
    pub sample_pos: u32,
    #[serde(default)]
    pub brr_addr: u16,
    #[serde(default)]
    pub brr_next_addr: u16,
    #[serde(default)]
    pub brr_loop_addr: u16,
    #[serde(default)]
    pub brr_block_index: u32,
    #[serde(default)]
    pub brr_header: u8,
    #[serde(default)]
    pub brr_prev1: i16,
    #[serde(default)]
    pub brr_prev2: i16,
    #[serde(default)]
    pub brr_samples: [i16; 16],
    #[serde(default)]
    pub brr_history: [i16; 3],
    #[serde(default)]
    pub brr_initialized: bool,
    #[serde(default)]
    pub vol_l: i8,
    #[serde(default)]
    pub vol_r: i8,
    #[serde(default)]
    pub adsr1: u8,
    #[serde(default)]
    pub adsr1_latch: u8,
    #[serde(default)]
    pub adsr2: u8,
    #[serde(default)]
    pub gain: u8,
    #[serde(default)]
    pub env_level: u16,
    #[serde(default)]
    pub hidden_env: i32,
    #[serde(default)]
    pub envx: u8,
    #[serde(default)]
    pub outx: i8,
    #[serde(default)]
    pub current_output: i16,
    #[serde(default)]
    pub mod_source: i8,
    #[serde(default)]
    pub mode: EnvelopeMode,
    #[serde(default)]
    pub kon_delay: u8,
    #[serde(default)]
    pub pitch_step: u16,
}
