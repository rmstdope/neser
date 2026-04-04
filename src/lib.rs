// Internal library for testing purposes only
// This is not published or exposed externally

pub mod app_context;
pub mod apu;
pub mod autorun;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod debugging;
pub mod frontend_toasts;
pub mod input;
pub mod ppu;
#[cfg(feature = "wasm")]
#[path = "web_frontend/wasm.rs"]
pub mod wasm;
#[path = "web_frontend/wasm_autorun_state.rs"]
pub mod wasm_autorun;
#[cfg(all(test, feature = "wasm", target_arch = "wasm32"))]
#[path = "web_frontend/wasm_tests.rs"]
mod wasm_tests;

#[cfg(any(feature = "sdl", feature = "native"))]
pub mod audio;
pub mod integration_tests;
#[cfg(feature = "native")]
pub mod native_frontend;
#[cfg(any(feature = "sdl", feature = "native"))]
pub mod rendering;
#[cfg(feature = "sdl")]
pub mod sdl_frontend;
#[cfg(feature = "tui")]
pub mod tui_frontend;
