// Internal library for testing purposes only
// This is not published or exposed externally

pub mod gb;
pub mod gba;
pub mod nes;
pub mod platform;

#[cfg(feature = "wasm")]
#[path = "frontends/web/wasm.rs"]
pub mod wasm;
#[path = "frontends/web/wasm_autorun_state.rs"]
pub mod wasm_autorun;
#[cfg(feature = "wasm")]
#[path = "frontends/web/wasm_gb.rs"]
pub mod wasm_gb;
#[cfg(feature = "wasm")]
#[path = "frontends/web/wasm_gba.rs"]
pub mod wasm_gba;
#[cfg(all(test, feature = "wasm", target_arch = "wasm32"))]
#[path = "frontends/web/wasm_tests.rs"]
mod wasm_tests;

pub mod frontends;
