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
    pub vol_l: i8,
    #[serde(default)]
    pub vol_r: i8,
    #[serde(default)]
    pub adsr1: u8,
    #[serde(default)]
    pub adsr2: u8,
    #[serde(default)]
    pub gain: u8,
    #[serde(default)]
    pub env_level: u16,
    #[serde(default)]
    pub envx: u8,
    #[serde(default)]
    pub outx: i8,
    #[serde(default)]
    pub mod_source: i8,
    #[serde(default)]
    pub mode: EnvelopeMode,
    #[serde(default)]
    pub kon_delay: u8,
    #[serde(default)]
    pub env_divider_counter: u16,
}
