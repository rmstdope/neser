use crate::console::Nes;
use crate::debugging::DebuggerViewState;
use crate::debugging::ui as debugger_ui;
use crate::rendering::input::{InputEvent, apply_input};
use crate::rendering::shader_manager::ShaderManager;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Instant;

/// Backend-agnostic surface for presenting rendered frames.
///
/// Implementations provide window sizing information and GL context handling
/// without exposing SDL or platform-specific types to the renderer.
pub trait RenderTarget {
    /// Returns the logical window size in pixels.
    fn window_size(&self) -> (u32, u32);
    /// Returns the drawable framebuffer size in pixels (may differ on HiDPI).
    fn drawable_size(&self) -> (u32, u32);
    /// Swaps the front/back buffers to present the rendered frame.
    fn swap_buffers(&self);
    /// Makes the render target's GL context current.
    fn make_current(&self) -> Result<(), String>;
}

/// Loader for GL procedure addresses used by OpenGL and related backends.
pub type ProcAddressLoader = Rc<dyn Fn(&str) -> *const c_void>;

/// OpenGL renderer that draws the NES frame and optional debugger UI.
pub struct GlBackend {
    render_target: Box<dyn RenderTarget>,
    glow_context: std::sync::Arc<glow::Context>,
    imgui: imgui::Context,
    renderer: imgui_opengl_renderer::Renderer,
    nes_texture: gl::types::GLuint,
    nes_texture_id: imgui::TextureId,
    overlay_font: imgui::FontId,
    overlay_text_color: OverlayTextColor,
    framebuffer: Vec<u8>,
    last_frame: Instant,
    debugger_view_state: DebuggerViewState,
    shader_manager: ShaderManager,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayTextColor {
    White,
    Black,
}

fn toggle_overlay_text_color(color: OverlayTextColor) -> OverlayTextColor {
    match color {
        OverlayTextColor::White => OverlayTextColor::Black,
        OverlayTextColor::Black => OverlayTextColor::White,
    }
}

impl OverlayTextColor {
    fn rgba(self) -> [f32; 4] {
        match self {
            OverlayTextColor::White => [1.0, 1.0, 1.0, 1.0],
            OverlayTextColor::Black => [0.0, 0.0, 0.0, 1.0],
        }
    }
}

fn overlay_text_rgba(text_color: OverlayTextColor, blink_red: bool) -> [f32; 4] {
    if blink_red {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        text_color.rgba()
    }
}

fn overlay_background_color_for(text_color: OverlayTextColor) -> [f32; 4] {
    match text_color {
        OverlayTextColor::White => [0.0, 0.0, 0.0, 0.5],
        OverlayTextColor::Black => [1.0, 1.0, 1.0, 0.5],
    }
}

impl GlBackend {
    // NES pixel aspect (8:7) times NTSC display correction (16:15).
    const NTSC_ASPECT: f32 = 8.0 / 7.0 * 16.0 / 15.0;

    /// Returns the aspect ratio used for rendering the NES output.
    fn target_aspect(&self) -> f32 {
        Self::NTSC_ASPECT
    }

    pub fn window_size(&self) -> (u32, u32) {
        self.render_target.window_size()
    }

    /// Computes windowed mode dimensions preserving the target aspect ratio.
    pub(crate) fn windowed_dimensions(height: u32) -> (u32, u32) {
        let clamped_height = height.max(1);
        let width = (clamped_height as f32 * Self::NTSC_ASPECT).round() as u32;
        (width.max(1), clamped_height)
    }

    /// Returns the largest size that fits inside the container while preserving aspect.
    fn letterbox_size(container_w: f32, container_h: f32, aspect: f32) -> (f32, f32) {
        if container_h == 0.0 {
            return (container_w, 0.0);
        }

        let container_aspect = container_w / container_h;
        if container_aspect > aspect {
            (container_h * aspect, container_h)
        } else {
            (container_w, container_w / aspect)
        }
    }

    /// Creates a new OpenGL renderer bound to the provided render target.
    pub fn new(
        render_target: Box<dyn RenderTarget>,
        proc_address: ProcAddressLoader,
        shader_path: Option<&str>,
    ) -> Result<Self, String> {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            let (drawable_w, drawable_h) = render_target.drawable_size();
            gl::Viewport(0, 0, drawable_w as i32, drawable_h as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
        }

        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);

        let overlay_font = {
            let font_size = 26.0;
            let sources = [imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    size_pixels: font_size,
                    ..Default::default()
                }),
            }];
            imgui.fonts().add_font(&sources)
        };

        let renderer = imgui_opengl_renderer::Renderer::new(&mut imgui, |s| (proc_address)(s) as _);

        let (nes_texture, nes_texture_id) = unsafe {
            let mut tex: gl::types::GLuint = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

            // Allocate texture storage.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB8 as i32,
                256,
                240,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );

            let id: imgui::TextureId = (tex as usize).into();
            (tex, id)
        };

        // Create glow context for librashader
        let glow_context = unsafe {
            let proc_address = proc_address.clone();
            std::sync::Arc::new(glow::Context::from_loader_function(|s| {
                (proc_address)(s) as *const _
            }))
        };

        let mut shader_manager = ShaderManager::new();

        // Load shader preset if specified
        if let Some(path) = shader_path
            && let Err(e) =
                shader_manager.load_preset(std::path::Path::new(path), glow_context.clone())
        {
            eprintln!("Warning: Failed to load shader preset '{}': {}", path, e);
        }

        Ok(Self {
            render_target,
            glow_context,
            imgui,
            renderer,
            nes_texture,
            nes_texture_id,
            overlay_font,
            overlay_text_color: OverlayTextColor::White,
            framebuffer: vec![0u8; 256 * 240 * 3],
            last_frame: Instant::now(),
            debugger_view_state: DebuggerViewState::default(),
            shader_manager,
        })
    }

    /// Applies an input event to ImGui and handles renderer-local shortcuts.
    pub fn handle_input(&mut self, event: &InputEvent) {
        if let InputEvent::Key {
            key: imgui::Key::F1,
            down: true,
        } = event
        {
            self.overlay_text_color = toggle_overlay_text_color(self.overlay_text_color);
        }

        apply_input(self.imgui.io_mut(), event);
    }

    /// Renders the current NES frame and optional debugger overlay.
    pub fn render(
        &mut self,
        nes: &Nes,
        show_debugger: bool,
        overlay_text: Option<&str>,
        overlay_blink_red: bool,
    ) -> debugger_ui::DebuggerUiAction {
        let mut action = debugger_ui::DebuggerUiAction::default();

        // Per-frame IO updates
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.imgui.io_mut().delta_time = dt.as_secs_f32().max(1.0 / 1000.0);

        let (win_w, win_h) = self.render_target.window_size();
        let (drawable_w, drawable_h) = self.render_target.drawable_size();

        // Keep the GL viewport in sync with the current drawable size.
        unsafe {
            gl::Viewport(0, 0, drawable_w as i32, drawable_h as i32);
        }

        let scale_x = if win_w == 0 {
            1.0
        } else {
            drawable_w as f32 / win_w as f32
        };
        let scale_y = if win_h == 0 {
            1.0
        } else {
            drawable_h as f32 / win_h as f32
        };

        {
            let io = self.imgui.io_mut();
            io.display_size = [win_w as f32, win_h as f32];
            io.display_framebuffer_scale = [scale_x, scale_y];
        }

        // Update NES texture (keep the PPU borrow short-lived so we can snapshot later).
        {
            let screen_buffer = nes.get_screen_buffer();
            screen_buffer.copy_buffer(&mut self.framebuffer);
        }

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.nes_texture);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                256,
                240,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                self.framebuffer.as_ptr() as *const _,
            );
            // librashader's OpenGL runtime uses sampler objects whose MIN_FILTER always
            // includes a mipmap variant (e.g. LINEAR_MIPMAP_LINEAR). Ensure the NES texture
            // is mipmap-complete, otherwise some drivers (notably macOS) will treat it as
            // unloadable and sample black.
            gl::GenerateMipmap(gl::TEXTURE_2D);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        let target_aspect = self.target_aspect();

        // Apply shader post-processing if a shader is loaded
        // The shader will render the NES texture to the screen with filtering applied
        // Note: librashader's OpenGL runtime renders into a texture-backed output, not
        // directly into the default framebuffer. We therefore render into an output texture
        // and draw that texture as the background.
        let mut shader_output_texture_id: Option<imgui::TextureId> = None;
        if self.shader_manager.has_shader() {
            // Compute a drawable-space letterbox size for the shader output.
            let drawable_w_f = drawable_w as f32;
            let drawable_h_f = drawable_h as f32;
            let (shader_out_w_f, shader_out_h_f) =
                Self::letterbox_size(drawable_w_f, drawable_h_f, target_aspect);
            let shader_out_w = (shader_out_w_f as u32).max(1);
            let shader_out_h = (shader_out_h_f as u32).max(1);

            if let Err(e) =
                self.shader_manager
                    .apply_shader(self.nes_texture, shader_out_w, shader_out_h)
            {
                eprintln!("Shader application error: {}", e);
            } else if let Some(tex) = self.shader_manager.output_texture() {
                shader_output_texture_id = Some((tex as usize).into());
            }
        }

        // Start ImGui frame
        {
            let ui = self.imgui.frame();

            // Draw NES frame as a background image, preserving aspect ratio with letterboxing.
            let win_w = win_w as f32;
            let win_h = win_h as f32;
            let (draw_w, draw_h) = Self::letterbox_size(win_w, win_h, target_aspect);

            let x0 = (win_w - draw_w) * 0.5;
            let y0 = (win_h - draw_h) * 0.5;

            // Only draw the NES texture as a background if no shader is active.
            // When a shader is active, we draw the shader output texture.
            //
            // Note: imgui 0.11 may produce draw_data with CmdLists=null when nothing is drawn.
            // imgui-opengl-renderer iterates draw_lists() unconditionally, which will panic
            // on a null pointer even when the count is 0. Ensure we always emit at least one
            // (invisible) draw command so the draw list pointer is non-null.
            if let Some(shader_tex_id) = shader_output_texture_id {
                ui.get_background_draw_list()
                    .add_image(shader_tex_id, [x0, y0], [x0 + draw_w, y0 + draw_h])
                    .build();
            } else {
                ui.get_background_draw_list()
                    .add_image(self.nes_texture_id, [x0, y0], [x0 + draw_w, y0 + draw_h])
                    .build();
            }

            if let Some(text) = overlay_text {
                let draw_list = ui.get_background_draw_list();
                let _font = ui.push_font(self.overlay_font);
                let text_size = ui.calc_text_size(text);
                let padding = [6.0, 4.0];
                let text_pos = [x0 + 8.0, y0 + 8.0];
                let rect_min = [text_pos[0] - padding[0], text_pos[1] - padding[1]];
                let rect_max = [
                    text_pos[0] + text_size[0] + padding[0],
                    text_pos[1] + text_size[1] + padding[1],
                ];
                draw_list
                    .add_rect(
                        rect_min,
                        rect_max,
                        overlay_background_color_for(self.overlay_text_color),
                    )
                    .filled(true)
                    .build();
                draw_list.add_text(
                    text_pos,
                    overlay_text_rgba(self.overlay_text_color, overlay_blink_red),
                    text,
                );
            }

            if show_debugger {
                let snapshot = self.debugger_view_state.snapshot(nes);
                action = debugger_ui::render(ui, &snapshot);
            }
        }

        self.renderer.render(&mut self.imgui);

        self.render_target.swap_buffers();

        action
    }

    /// Cycles through available shader presets, if any.
    pub fn cycle_shader(&mut self) {
        if let Err(e) = self.shader_manager.cycle_shader(self.glow_context.clone()) {
            eprintln!("Error cycling shader: {}", e);
        } else if let Some(name) = self.shader_manager.current_preset_name() {
            println!("Switched to shader: {}", name);
        }
    }
}

#[cfg(test)]
mod tests_letterbox {
    use super::GlBackend;

    #[test]
    fn test_letterbox_size_wide_container() {
        let (w, h) = GlBackend::letterbox_size(1920.0, 1080.0, GlBackend::NTSC_ASPECT);
        assert!((w - 1316.5714).abs() < 0.01);
        assert_eq!(h, 1080.0);
    }

    #[test]
    fn test_letterbox_size_matches_aspect() {
        let (w, h) = GlBackend::letterbox_size(800.0, 600.0, GlBackend::NTSC_ASPECT);
        assert!((w - 731.4286).abs() < 0.01);
        assert_eq!(h, 600.0);
    }

    #[test]
    fn test_letterbox_size_zero_height() {
        let (w, h) = GlBackend::letterbox_size(800.0, 0.0, GlBackend::NTSC_ASPECT);
        assert_eq!(w, 800.0);
        assert_eq!(h, 0.0);
    }
}

#[cfg(test)]
mod tests_windowed_dimensions {
    use super::GlBackend;

    #[test]
    fn test_windowed_dimensions_from_height_240() {
        let (w, h) = GlBackend::windowed_dimensions(240);
        assert_eq!(h, 240);
        assert_eq!(w, 293);
    }

    #[test]
    fn test_windowed_dimensions_from_height_960() {
        let (w, h) = GlBackend::windowed_dimensions(960);
        assert_eq!(h, 960);
        assert_eq!(w, 1170);
    }
}

impl Drop for GlBackend {
    fn drop(&mut self) {
        // Best-effort: make current and delete texture.
        let _ = self.render_target.make_current();
        unsafe {
            gl::DeleteTextures(1, &self.nes_texture);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_text_color_toggle() {
        assert_eq!(
            toggle_overlay_text_color(OverlayTextColor::White),
            OverlayTextColor::Black
        );
        assert_eq!(
            toggle_overlay_text_color(OverlayTextColor::Black),
            OverlayTextColor::White
        );
    }

    #[test]
    fn test_overlay_background_color_is_half_alpha_black() {
        assert_eq!(
            overlay_background_color_for(OverlayTextColor::White),
            [0.0, 0.0, 0.0, 0.5]
        );
        assert_eq!(
            overlay_background_color_for(OverlayTextColor::Black),
            [1.0, 1.0, 1.0, 0.5]
        );
    }

    #[test]
    fn test_overlay_text_color_blink_red() {
        assert_eq!(
            overlay_text_rgba(OverlayTextColor::White, true),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            overlay_text_rgba(OverlayTextColor::Black, false),
            [0.0, 0.0, 0.0, 1.0]
        );
    }
}
