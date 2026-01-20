// Internal library for testing purposes only
// This is not published or exposed externally

pub mod apu;
pub mod autorun;
pub mod blargg_tests;
pub mod bus;
pub mod cartridge;
pub mod config;
pub mod cpu;
pub mod debugger;
pub mod input;
pub use bus::bus::Bus;
pub mod nes;
pub mod ppu; // Modular PPU structure
pub mod savestate;
pub mod tracing;

#[cfg(feature = "sdl")]
pub mod audio;

#[cfg(test)]
pub mod game_verification;

#[cfg(feature = "sdl")]
pub mod eventloop;

#[cfg(feature = "sdl")]
pub mod rendering;
