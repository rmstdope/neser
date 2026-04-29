//! Game Boy Advance (GBA) emulation.
//!
//! This module provides the platform infrastructure for GBA emulation.
//! The actual CPU, PPU, APU, and memory implementations will be added
//! in subsequent phases.

pub mod console;

pub use console::gba::Gba;
