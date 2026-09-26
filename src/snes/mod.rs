//! Super Nintendo Entertainment System (SNES) emulation.
//!
//! This module provides SNES hardware emulation including CPU (65816), PPU,
//! APU (SPC700 + DSP), bus architecture, cartridge support, and input handling.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod cx4;
pub mod dsp;
pub mod gsu;
pub mod input;
pub mod obc1;
pub mod ppu;
pub mod sa1;
pub mod sdd1;
pub mod upd77c25;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
pub(crate) mod test_support;
