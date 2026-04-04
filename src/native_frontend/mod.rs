//! Rust-native frontend using winit + glutin + glow.
//!
//! Replaces the SDL2 frontend with pure-Rust crates for windowing,
//! OpenGL context management, and input handling.

mod app_state;
mod audio;
mod event_loop;
mod gl_wrapper;
pub mod keyboard;
mod render_target;

pub use audio::NativeAudio;
pub use event_loop::NativeEventLoop;
