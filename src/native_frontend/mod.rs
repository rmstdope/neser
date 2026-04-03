//! Rust-native frontend using winit + glutin + glow.
//!
//! Replaces the SDL2 frontend with pure-Rust crates for windowing,
//! OpenGL context management, and input handling.

mod audio;
mod gl_wrapper;
mod render_target;

pub use audio::NativeAudio;
pub use gl_wrapper::NativeGlWrapper;
pub use render_target::WinitRenderTarget;
