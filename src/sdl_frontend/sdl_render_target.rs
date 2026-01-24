use crate::rendering::RenderTarget;

use sdl2::video::{GLContext, Window};

pub struct SdlRenderTarget {
    pub window: Window,
    pub gl_context: GLContext,
}

impl RenderTarget for SdlRenderTarget {
    fn window_size(&self) -> (u32, u32) {
        self.window.size()
    }

    fn drawable_size(&self) -> (u32, u32) {
        self.window.drawable_size()
    }

    fn swap_buffers(&self) {
        self.window.gl_swap_window();
    }

    fn make_current(&self) -> Result<(), String> {
        self.window
            .gl_make_current(&self.gl_context)
            .map_err(|e| e.to_string())
    }
}
