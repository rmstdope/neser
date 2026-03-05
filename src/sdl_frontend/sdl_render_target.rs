use crate::rendering::RenderTarget;

use sdl2::video::{FullscreenType, GLContext, Window};

/// SDL-backed render target that exposes window/GL context operations
/// to the renderer without leaking SDL types into the rendering module.
pub struct SdlRenderTarget {
    pub window: Window,
    pub gl_context: GLContext,
}

impl RenderTarget for SdlRenderTarget {
    /// Returns the logical window size in pixels.
    fn window_size(&self) -> (u32, u32) {
        self.window.size()
    }

    /// Returns the drawable framebuffer size in pixels (HiDPI aware).
    fn drawable_size(&self) -> (u32, u32) {
        self.window.drawable_size()
    }

    /// Swaps the OpenGL buffers to present the frame.
    fn swap_buffers(&self) {
        self.window.gl_swap_window();
    }

    /// Makes the SDL OpenGL context current.
    fn make_current(&self) -> Result<(), String> {
        self.window
            .gl_make_current(&self.gl_context)
            .map_err(|e| e.to_string())
    }

    /// Enables or disables fullscreen mode for the SDL window.
    fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        let mode = if enabled {
            FullscreenType::Desktop
        } else {
            FullscreenType::Off
        };
        self.window.set_fullscreen(mode).map_err(|e| e.to_string())
    }

    fn set_mouse_grab(&mut self, enabled: bool) -> Result<(), String> {
        self.window.set_grab(enabled);
        Ok(())
    }
}
