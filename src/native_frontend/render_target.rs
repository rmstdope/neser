#![allow(dead_code)] // Public API for future use in native frontend
use crate::rendering::RenderTarget;

use glutin::context::PossiblyCurrentContext;
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};
use winit::window::Window;

use std::num::NonZeroU32;
use std::sync::Arc;

/// Winit/glutin-backed render target implementing the backend-agnostic
/// [`RenderTarget`] trait used by [`GlBackend`](crate::rendering::GlBackend).
pub struct WinitRenderTarget {
    window: Arc<Window>,
    surface: Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
}

impl WinitRenderTarget {
    pub fn new(
        window: Arc<Window>,
        surface: Surface<WindowSurface>,
        gl_context: PossiblyCurrentContext,
    ) -> Self {
        Self {
            window,
            surface,
            gl_context,
        }
    }

    /// Returns a reference to the underlying winit window.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Sets the swap interval (vsync) on the GL surface.
    pub fn set_swap_interval(&self, vsync: bool) -> Result<(), String> {
        let interval = if vsync {
            SwapInterval::Wait(NonZeroU32::new(1).expect("non-zero"))
        } else {
            SwapInterval::DontWait
        };
        self.surface
            .set_swap_interval(&self.gl_context, interval)
            .map_err(|e| format!("failed to set swap interval: {e}"))
    }
}

impl RenderTarget for WinitRenderTarget {
    fn window_size(&self) -> (u32, u32) {
        let size = self
            .window
            .inner_size()
            .to_logical::<u32>(self.window.scale_factor());
        (size.width, size.height)
    }

    fn drawable_size(&self) -> (u32, u32) {
        // inner_size() returns physical (HiDPI-aware) pixels.
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    fn swap_buffers(&self) {
        if let Err(e) = self.surface.swap_buffers(&self.gl_context) {
            eprintln!("failed to swap GL buffers: {e}");
        }
    }

    fn make_current(&self) -> Result<(), String> {
        self.gl_context
            .make_current(&self.surface)
            .map_err(|e| format!("failed to make GL context current: {e}"))
    }

    fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        let mode = if enabled {
            Some(winit::window::Fullscreen::Borderless(None))
        } else {
            None
        };
        self.window.set_fullscreen(mode);
        Ok(())
    }

    fn set_mouse_grab(&mut self, enabled: bool) -> Result<(), String> {
        let mode = if enabled {
            winit::window::CursorGrabMode::Confined
        } else {
            winit::window::CursorGrabMode::None
        };
        self.window
            .set_cursor_grab(mode)
            .or_else(|_| {
                // Confined not supported on all platforms, fall back to Locked
                if enabled {
                    self.window
                        .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                } else {
                    Ok(())
                }
            })
            .map_err(|e| format!("failed to set cursor grab: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time verification that WinitRenderTarget implements RenderTarget.
    fn _assert_render_target_impl(_: &dyn RenderTarget) {}

    #[allow(dead_code)]
    fn _type_check(rt: &WinitRenderTarget) {
        _assert_render_target_impl(rt);
    }
}
