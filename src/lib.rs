// Internal library for testing purposes only
// This is not published or exposed externally

pub mod apu;
pub mod blargg_tests;
pub mod cartridge;
pub mod cpu;
pub mod debugger;
pub mod game_verification_tests;
pub mod input;
pub mod manual_test_cartridges;
pub mod mem_controller;
pub mod nes;
pub mod ppu; // Modular PPU structure
pub mod screen_buffer;
pub mod tracing;

#[cfg(feature = "sdl")]
pub mod audio;

#[cfg(test)]
pub mod golden_screenshots;

#[cfg(feature = "sdl")]
pub mod eventloop;

#[cfg(feature = "sdl")]
pub(crate) mod gl_backend;

#[cfg(feature = "sdl")]
pub(crate) mod shader_manager;
