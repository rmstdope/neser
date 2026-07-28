//! Rust-native frontend using winit + glutin + glow.
//!
//! Replaces the SDL2 frontend with pure-Rust crates for windowing,
//! OpenGL context management, and input handling.

mod app_state;
mod audio;
pub(crate) mod egui_renderer;
pub(crate) mod egui_texture;
pub(crate) mod egui_theme;
mod event_loop;
mod frame_runner;
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

/// Create the winit event loop for the native frontend.
///
/// On macOS the default menubar is disabled: winit's menu wires its Quit
/// item straight to `-[NSApplication terminate:]` with key equivalent "q",
/// and AppKit routes a plain Q press to that item whenever the emulator
/// window is not the key window. That terminates the whole process (exit
/// code 0) from inside the event loop — skipping battery-RAM saves and the
/// return-to-browser loop — instead of reaching the graceful Q quit hotkey.
pub fn create_event_loop() -> Result<winit::event_loop::EventLoop<()>, String> {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        winit::event_loop::EventLoop::builder()
            .with_default_menu(false)
            .build()
            .map_err(|e| format!("Failed to create event loop: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        winit::event_loop::EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))
    }
}
