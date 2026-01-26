// Internal library for testing purposes only
// This is not published or exposed externally

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod debugging;
pub mod input;
pub mod ppu;
#[cfg(feature = "wasm")]
#[path = "web_frontend/wasm.rs"]
pub mod wasm;
#[cfg(all(test, feature = "wasm", target_arch = "wasm32"))]
#[path = "web_frontend/wasm_tests.rs"]
mod wasm_tests;

pub mod integration_tests;
#[cfg(feature = "sdl")]
pub mod rendering;
#[cfg(feature = "sdl")]
pub mod sdl_frontend;
