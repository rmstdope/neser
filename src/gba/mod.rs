//! Game Boy Advance (GBA) emulation.
//!
//! This module provides the platform infrastructure for GBA emulation.
//! The actual CPU, PPU, APU, and memory implementations will be added
//! in subsequent phases.

pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod ppu;

pub use bus::GbaBus;
pub use cartridge::{GbaCartridge, SaveType, load_cartridge};
pub use console::gba::Gba;
pub use ppu::Ppu;
