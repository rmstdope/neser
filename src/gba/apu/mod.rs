//! GBA Audio Processing Unit (APU).
//!
//! Implements the GBA dual-source audio system:
//! - Four DMG-legacy channels (Pulse+Sweep, Pulse, Wave, Noise) clocked at
//!   the GBA's 16.78 MHz rate but producing the same frequencies as the
//!   original Game Boy channels.
//! - Two DMA-driven PCM FIFO channels (A and B) accepting 8-bit signed samples.
//!
//! The mixer produces a mono f32 sample stream compatible with the existing
//! platform audio pipeline (`sample_ready()` / `get_sample()`).
//!
//! ## GBA vs DMG timing
//!
//! | Channel | Duty step period (GBA cycles) | Equivalent DMG unit  |
//! |---------|-------------------------------|----------------------|
//! | CH1/CH2 | (2048 − freq) × 16            | (2048 − freq) M-cyc  |
//! | CH3     | (2048 − freq) × 8             | (2048 − freq) APU-cyc|
//! | CH4     | divisor × 2^shift × 4         | divisor × 2^shift T  |
//!
//! Frame Sequencer: 512 Hz → one step every 32 768 GBA cycles.

pub mod channel1;
pub mod channel2;
pub mod channel3;
pub mod channel4;
pub mod fifo;

#[allow(clippy::module_inception)]
pub mod apu;
pub use apu::{Apu, ApuState};
