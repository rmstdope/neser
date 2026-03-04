#[allow(clippy::module_inception)]
mod apu;
pub mod dmc;
pub mod envelope;
pub mod frame_counter;
pub mod length_counter;
pub mod noise;
pub mod pulse;
pub mod triangle;

#[cfg(test)]
mod audio_analysis_noise_test;
#[cfg(test)]
mod audio_analysis_pulse_test;
#[cfg(test)]
mod audio_analysis_triangle_test;

pub use apu::Apu;
pub use apu::{ApuState, FrameCounterState};
pub use dmc::DmcState;
pub use envelope::EnvelopeState;
pub use noise::NoiseState;
pub use pulse::PulseState;
pub use triangle::TriangleState;
