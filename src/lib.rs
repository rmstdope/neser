// Internal library for testing purposes only
// This is not published or exposed externally

pub mod nes;

pub mod app_context;
pub mod autorun;
pub mod config;
pub mod debugging;
pub mod emulator;
#[cfg(feature = "wasm")]
#[path = "frontends/web/wasm.rs"]
pub mod wasm;
#[path = "frontends/web/wasm_autorun_state.rs"]
pub mod wasm_autorun;
#[cfg(all(test, feature = "wasm", target_arch = "wasm32"))]
#[path = "frontends/web/wasm_tests.rs"]
mod wasm_tests;

#[cfg(feature = "native")]
pub mod audio;
pub mod frontends;
#[cfg(feature = "native")]
pub mod rendering;
