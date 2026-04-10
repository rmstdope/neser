//! Shared audio infrastructure for NES emulator frontends.
//!
//! This module provides backend-agnostic audio types and utilities
//! shared audio infrastructure for the native frontend.

mod audio_trait;
mod resampler;
pub(crate) mod types;

pub use audio_trait::EmulatorAudio;
pub use resampler::AudioResampler;
// AudioConsumer and AudioStats are used by the audio callback; AudioProducer by the emulation loop.
#[allow(unused_imports)]
pub use types::{AudioConsumer, AudioProducer, AudioStats};
