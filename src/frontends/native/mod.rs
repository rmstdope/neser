//! Rust-native frontend using winit + glutin + glow.
//!
//! Replaces the SDL2 frontend with pure-Rust crates for windowing,
//! OpenGL context management, and input handling.

mod app_state;
mod audio;
mod event_loop;
pub(crate) mod gamepad;
pub(crate) mod gl_backend;
mod gl_wrapper;
pub(crate) mod input;
pub mod keyboard;
mod mouse;
mod render_target;
pub mod rom_browser;
mod shader_manager;
mod sleep_inhibitor;
pub(crate) mod ui_geometry;

pub use audio::NativeAudio;
pub use event_loop::NativeEventLoop;
