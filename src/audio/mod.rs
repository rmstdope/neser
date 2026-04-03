//! Shared audio infrastructure for NES emulator frontends.
//!
//! This module provides backend-agnostic audio types and utilities
//! shared between the SDL and native frontend audio implementations.

mod audio_trait;
mod resampler;
pub(crate) mod types;

pub use audio_trait::NesAudio;
pub use resampler::AudioResampler;
// AudioConsumer and AudioStats are used by the SDL backend; AudioProducer by both.
#[allow(unused_imports)]
pub use types::{AudioConsumer, AudioProducer, AudioStats};
