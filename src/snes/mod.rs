//! Super Nintendo Entertainment System (SNES) emulation.
//!
//! This module provides SNES hardware emulation including CPU (65816), PPU,
//! APU (SPC700 + DSP), bus architecture, cartridge support, and input handling.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod input;
pub mod ppu;

#[cfg(test)]
mod integration_tests;
