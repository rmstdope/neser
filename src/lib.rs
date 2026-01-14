// Internal library for testing purposes only
// This is not published or exposed externally

pub mod apu;
pub mod blargg_tests;
pub mod cartridge;
pub mod config;
pub mod cpu;
pub mod debugger;
pub mod input;
pub mod mem_controller;
pub mod nes;
pub mod ppu; // Modular PPU structure
pub mod screen_buffer;
pub mod tracing;

#[cfg(feature = "sdl")]
pub mod audio;

#[cfg(test)]
pub mod game_verification;

#[cfg(feature = "sdl")]
pub mod eventloop;

#[cfg(feature = "sdl")]
pub(crate) mod rendering;

