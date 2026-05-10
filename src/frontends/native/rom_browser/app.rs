//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! window with a placeholder screen and accepts a ROM selection that
//! transitions the application into emulation mode.

use std::path::PathBuf;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::platform::app_context::SharedAppContext;

/// Result from running the ROM browser.
#[derive(Debug)]
pub enum BrowserResult {
    /// User selected a ROM to launch.
    RomSelected(PathBuf),
    /// User closed the browser window without selecting.
    Closed,
}

/// ROM browser winit application.
///
/// Manages the browser window and handles user interactions until a ROM is
/// selected or the window is closed.
pub struct RomBrowserApp {
    #[allow(dead_code)] // Will be used by ROM browser rendering (grid-view todo).
    app_context: SharedAppContext,
    window: Option<Window>,
    result: BrowserResult,
    default_width: u32,
    default_height: u32,
    fullscreen: bool,
}

impl RomBrowserApp {
    /// Create a new ROM browser application.
    pub fn new(app_context: SharedAppContext) -> Self {
        let (default_height, fullscreen) = {
            let ctx = app_context.borrow();
            let config = ctx.config();
            (config.frontend.window_height, config.frontend.fullscreen)
        };
        // Default browser window is 1280×720, or use configured height with 16:9 ratio.
        let default_width = (default_height as f64 * 16.0 / 9.0) as u32;

        Self {
            app_context,
            window: None,
            result: BrowserResult::Closed,
            default_width,
            default_height,
            fullscreen,
        }
    }

    /// Run the ROM browser and return the result.
    ///
    /// Returns `BrowserResult::RomSelected` if a ROM was chosen, or
    /// `BrowserResult::Closed` if the user closed the window.
    pub fn run(mut self) -> Result<BrowserResult, String> {
        let event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        event_loop
            .run_app(&mut self)
            .map_err(|e| format!("Browser event loop error: {e}"))?;
        Ok(self.result)
    }

    /// Set the selected ROM and trigger exit.
    ///
    /// Called when the user picks a ROM from the browser UI.
    pub fn select_rom(&mut self, path: PathBuf, event_loop: &ActiveEventLoop) {
        self.result = BrowserResult::RomSelected(path);
        event_loop.exit();
    }
}

impl ApplicationHandler for RomBrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attrs = Window::default_attributes()
            .with_title("NESER - ROM Browser")
            .with_inner_size(LogicalSize::new(self.default_width, self.default_height));

        if self.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        match event_loop.create_window(attrs) {
            Ok(window) => {
                self.window = Some(window);
            }
            Err(e) => {
                eprintln!("Failed to create browser window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.result = BrowserResult::Closed;
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // TODO: Render the ROM browser grid here (gl-browser-scaffold todo).
                // For now, just request another redraw to keep the window responsive.
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_result_default_is_closed() {
        let result = BrowserResult::Closed;
        assert!(matches!(result, BrowserResult::Closed));
    }

    #[test]
    fn browser_result_rom_selected_holds_path() {
        let path = PathBuf::from("/roms/game.nes");
        let result = BrowserResult::RomSelected(path.clone());
        match result {
            BrowserResult::RomSelected(p) => assert_eq!(p, path),
            BrowserResult::Closed => panic!("expected RomSelected"),
        }
    }
}
