// Internal library for testing purposes only
// This is not published or exposed externally

pub mod nes;

// Re-exports for backward compatibility during migration.
// Callers can use `crate::cpu`, `crate::ppu`, etc. as before.
pub use nes::apu;
pub use nes::bus;
pub use nes::cartridge;
pub use nes::console;
pub use nes::cpu;
pub use nes::integration_tests;
pub use nes::ppu;

pub mod app_context;
pub mod autorun;
pub mod config;
pub mod debugging;
pub mod emulator;
pub mod frontend_toasts;
pub mod input;
#[cfg(feature = "wasm")]
#[path = "web_frontend/wasm.rs"]
pub mod wasm;
#[path = "web_frontend/wasm_autorun_state.rs"]
pub mod wasm_autorun;
#[cfg(all(test, feature = "wasm", target_arch = "wasm32"))]
#[path = "web_frontend/wasm_tests.rs"]
mod wasm_tests;

#[cfg(feature = "native")]
pub mod audio;
#[cfg(feature = "native")]
pub mod native_frontend;
#[cfg(feature = "native")]
pub mod rendering;
#[cfg(feature = "tui")]
pub mod tui_frontend;
