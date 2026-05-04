//! Game Boy Advance (GBA) emulation.
//!
//! This module provides the platform infrastructure for GBA emulation.
//! The actual CPU, PPU, APU, and memory implementations will be added
//! in subsequent phases.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
#[cfg(not(target_arch = "wasm32"))]
pub mod debugging;
pub mod input;
#[cfg(test)]
pub mod integration_tests;
pub mod ppu;

#[allow(unused_imports)]
pub use apu::Apu;
pub use bus::GbaBus;
#[allow(unused_imports)]
pub use cartridge::{GbaCartridge, SaveType, load_cartridge};
pub use console::gba::Gba;
#[allow(unused_imports)]
pub use input::Keypad;
#[allow(unused_imports)]
pub use ppu::Ppu;
