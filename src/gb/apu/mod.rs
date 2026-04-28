#[allow(clippy::module_inception)]
mod apu;
pub mod channel1;
pub mod channel2;
pub mod channel3;
pub mod channel4;

pub use apu::Apu;
