use crate::app_context::SharedAppContext;
use crate::console::Nes;
use crate::debugging::DebuggerViewState;
use crate::debugging::breakpoints::BreakpointList;
use crate::debugging::log_info;
use crate::debugging::ppu_viewer::{
    PpuViewerSnapshot, render_nametables_rgba, render_pattern_tables_rgba,
};
use crate::debugging::ui::{self as debugger_ui, BreakpointAddUiState, HexdumpUiState};
use crate::rendering::input::{InputEvent, apply_input};
use crate::rendering::shader_manager::ShaderManager;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Instant;

const PPU_VIEWER_NT_TEXTURE_WIDTH: i32 = 512;
const PPU_VIEWER_NT_TEXTURE_HEIGHT: i32 = 480;
const PPU_VIEWER_TILES_TEXTURE_WIDTH: i32 = 256;
const PPU_VIEWER_TILES_TEXTURE_HEIGHT: i32 = 128;

const PPU_VIEWER_WINDOW_INITIAL_WIDTH: f32 = 560.0;
const PPU_VIEWER_WINDOW_INITIAL_HEIGHT: f32 = 854.0;

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
    /// Toggles fullscreen mode for the render target.
    fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String>;
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
    ppu_viewer_nt_texture: gl::types::GLuint,
    ppu_viewer_nt_texture_id: imgui::TextureId,
    ppu_viewer_tiles_texture: gl::types::GLuint,
    ppu_viewer_tiles_texture_id: imgui::TextureId,
    overlay_font: imgui::FontId,
    overlay_text_color: OverlayTextColor,
    app_context: SharedAppContext,
    framebuffer: Vec<u8>,
    last_frame: Instant,
    debugger_view_state: DebuggerViewState,
    debugger_alpha: f32,
    breakpoints: BreakpointList,
    bp_add_state: BreakpointAddUiState,
    hexdump_ui_state: HexdumpUiState,
    shader_manager: ShaderManager,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crosshair {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayTextColor {
    White,
    Black,
}

fn draw_frame_background(
    ui: &imgui::Ui,
    texture_id: imgui::TextureId,
    x0: f32,
    y0: f32,
    draw_w: f32,
    draw_h: f32,
) {
    ui.get_background_draw_list()
        .add_image(texture_id, [x0, y0], [x0 + draw_w, y0 + draw_h])
        .build();
}

fn draw_overlay_text(
    ui: &imgui::Ui,
    text: &str,
    font: imgui::FontId,
    text_color: OverlayTextColor,
    blink_red: bool,
    x0: f32,
    y0: f32,
) {
    let draw_list = ui.get_background_draw_list();
    let _font = ui.push_font(font);
    let text_size = ui.calc_text_size(text);
    let padding = [6.0, 4.0];
    let text_pos = [x0 + 8.0, y0 + 8.0];
    let rect_min = [text_pos[0] - padding[0], text_pos[1] - padding[1]];
    let rect_max = [
        text_pos[0] + text_size[0] + padding[0],
        text_pos[1] + text_size[1] + padding[1],
    ];

    draw_list
        .add_rect(rect_min, rect_max, overlay_background_color_for(text_color))
        .filled(true)
        .build();
    draw_list.add_text(text_pos, overlay_text_rgba(text_color, blink_red), text);
}

fn draw_crosshair(ui: &imgui::Ui, crosshair: Crosshair) {
    const CROSSHAIR_SIZE: f32 = 12.0;
    let color = [1.0, 0.2, 0.2, 1.0];
    let draw_list = ui.get_background_draw_list();

    draw_list
        .add_line(
            [crosshair.x - CROSSHAIR_SIZE, crosshair.y],
            [crosshair.x + CROSSHAIR_SIZE, crosshair.y],
            color,
        )
        .thickness(2.0)
        .build();
    draw_list
        .add_line(
            [crosshair.x, crosshair.y - CROSSHAIR_SIZE],
            [crosshair.x, crosshair.y + CROSSHAIR_SIZE],
            color,
        )
        .thickness(2.0)
        .build();
    draw_list
        .add_circle([crosshair.x, crosshair.y], 7.0, color)
        .thickness(2.0)
        .build();
}

fn draw_toasts(
    ui: &imgui::Ui,
    font: imgui::FontId,
    visible_toasts: &[String],
    x0: f32,
    y0: f32,
    draw_w: f32,
    draw_h: f32,
) {
    let draw_list = ui.get_background_draw_list();
    let _font = ui.push_font(font);
    let padding = [8.0, 6.0];
    let spacing = 8.0;
    let bottom_margin = 12.0;

    for (stack_index, toast_text) in visible_toasts.iter().rev().enumerate() {
        let text_size = ui.calc_text_size(toast_text);
        let rect_w = text_size[0] + padding[0] * 2.0;
        let rect_h = text_size[1] + padding[1] * 2.0;
        let rect_x = x0 + (draw_w - rect_w) * 0.5;
        let rect_max_y = y0 + draw_h - bottom_margin - stack_index as f32 * (rect_h + spacing);
        let rect_min = [rect_x, rect_max_y - rect_h];
        let rect_max = [rect_x + rect_w, rect_max_y];
        let text_pos = [rect_min[0] + padding[0], rect_min[1] + padding[1]];

        draw_list
            .add_rect(rect_min, rect_max, toast_background_rgba())
            .filled(true)
            .build();
        draw_list.add_text(text_pos, toast_text_rgba(), toast_text);
    }
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

fn toast_text_rgba() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn toast_background_rgba() -> [f32; 4] {
    [0.35, 0.35, 0.35, 0.7]
}

impl GlBackend {
    // NES pixel aspect (8:7) times NTSC display correction (16:15).
    const NTSC_ASPECT: f32 = 8.0 / 7.0 * 16.0 / 15.0;

    /// Returns the aspect ratio used for rendering the NES output.
    fn target_aspect(&self) -> f32 {
        Self::NTSC_ASPECT
    }

    /// Returns the logical window size in pixels reported by the render target.
    pub fn window_size(&self) -> (u32, u32) {
        self.render_target.window_size()
    }

    /// Enables or disables fullscreen mode for the underlying render target.
    pub fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        self.render_target.set_fullscreen(enabled)
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
        app_context: SharedAppContext,
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

        let (ppu_viewer_nt_texture, ppu_viewer_nt_texture_id) = unsafe {
            create_rgba_texture(PPU_VIEWER_NT_TEXTURE_WIDTH, PPU_VIEWER_NT_TEXTURE_HEIGHT)
        };
        let (ppu_viewer_tiles_texture, ppu_viewer_tiles_texture_id) = unsafe {
            create_rgba_texture(
                PPU_VIEWER_TILES_TEXTURE_WIDTH,
                PPU_VIEWER_TILES_TEXTURE_HEIGHT,
            )
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
            log_info(format!(
                "Warning: Failed to load shader preset '{}': {}",
                path, e
            ));
        }

        Ok(Self {
            render_target,
            glow_context,
            imgui,
            renderer,
            nes_texture,
            nes_texture_id,
            ppu_viewer_nt_texture,
            ppu_viewer_nt_texture_id,
            ppu_viewer_tiles_texture,
            ppu_viewer_tiles_texture_id,
            overlay_font,
            overlay_text_color: OverlayTextColor::White,
            app_context,
            framebuffer: vec![0u8; 256 * 240 * 3],
            last_frame: Instant::now(),
            debugger_view_state: DebuggerViewState::default(),
            debugger_alpha: 1.0,
            breakpoints: BreakpointList::default(),
            bp_add_state: BreakpointAddUiState::default(),
            hexdump_ui_state: HexdumpUiState::default(),
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

    /// Sets the debugger window background opacity (clamped to 0.1–1.0).
    pub fn set_debugger_alpha(&mut self, alpha: f32) {
        self.debugger_alpha = alpha.clamp(0.1, 1.0);
    }

    /// Updates the breakpoint list used by the debugger UI.
    pub fn update_breakpoints(&mut self, breakpoints: &BreakpointList) {
        self.breakpoints = breakpoints.clone();
    }

    /// Renders the current NES frame and optional debugger overlay.
    pub fn render(
        &mut self,
        nes: &Nes,
        show_debugger: bool,
        overlay_text: Option<&str>,
        overlay_blink_red: bool,
        crosshair: Option<Crosshair>,
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
                log_info(format!("Shader application error: {}", e));
            } else if let Some(tex) = self.shader_manager.output_texture() {
                shader_output_texture_id = Some((tex as usize).into());
            }
        }

        let visible_toasts = self.app_context.borrow_mut().visible_toasts(now);
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
            let background_texture = shader_output_texture_id.unwrap_or(self.nes_texture_id);
            draw_frame_background(ui, background_texture, x0, y0, draw_w, draw_h);

            if let Some(text) = overlay_text {
                draw_overlay_text(
                    ui,
                    text,
                    self.overlay_font,
                    self.overlay_text_color,
                    overlay_blink_red,
                    x0,
                    y0,
                );
            }

            if let Some(crosshair) = crosshair {
                draw_crosshair(ui, crosshair);
            }

            if !visible_toasts.is_empty() {
                draw_toasts(
                    ui,
                    self.overlay_font,
                    &visible_toasts,
                    x0,
                    y0,
                    draw_w,
                    draw_h,
                );
            }

            if show_debugger {
                let snapshot = self.debugger_view_state.snapshot(nes);
                action = debugger_ui::render(
                    ui,
                    &snapshot,
                    self.debugger_alpha,
                    &self.breakpoints,
                    &mut self.bp_add_state,
                    &mut self.hexdump_ui_state,
                );
                if action.toggle_ppu_viewer {
                    self.debugger_view_state.toggle_ppu_viewer();
                }
                if let Some(base) = action.set_prg_hexdump_base {
                    self.debugger_view_state.set_prg_hexdump_base(base);
                }
                if let Some(delta) = action.nudge_prg_hexdump_base_by_bytes {
                    self.debugger_view_state
                        .nudge_prg_hexdump_base_by_bytes_from(snapshot.prg_hexdump_base, delta);
                }
                if action.increase_opacity {
                    self.debugger_alpha = (self.debugger_alpha + 0.1).min(1.0);
                }
                if action.decrease_opacity {
                    self.debugger_alpha = (self.debugger_alpha - 0.1).max(0.1);
                }
                if self.debugger_view_state.is_ppu_viewer_visible() {
                    let scroll = update_ppu_viewer_textures(
                        nes,
                        self.ppu_viewer_nt_texture,
                        self.ppu_viewer_tiles_texture,
                    );
                    draw_ppu_viewer_window(
                        ui,
                        self.ppu_viewer_nt_texture_id,
                        self.ppu_viewer_tiles_texture_id,
                        scroll,
                    );
                }
            }
        }

        self.renderer.render(&mut self.imgui);

        self.render_target.swap_buffers();

        action
    }

    /// Cycles through available shader presets, if any.
    pub fn cycle_shader(&mut self) {
        if let Err(e) = self.shader_manager.cycle_shader(self.glow_context.clone()) {
            log_info(format!("Error cycling shader: {}", e));
        } else if let Some(name) = self.shader_manager.current_preset_name() {
            log_info(format!("Switched to shader: {}", name));
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

/// Create a RGBA GL texture of the given dimensions with NEAREST filtering.
///
/// # Safety
/// Must be called with an active GL context.
unsafe fn create_rgba_texture(width: i32, height: i32) -> (gl::types::GLuint, imgui::TextureId) {
    unsafe {
        let mut tex: gl::types::GLuint = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA8 as i32,
            width,
            height,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            std::ptr::null(),
        );
        let id: imgui::TextureId = (tex as usize).into();
        (tex, id)
    }
}

/// Upload RGBA pixel data to an existing GL texture.
///
/// # Safety
/// Must be called with an active GL context.
unsafe fn upload_rgba_texture(texture: gl::types::GLuint, width: i32, height: i32, pixels: &[u8]) {
    unsafe {
        gl::BindTexture(gl::TEXTURE_2D, texture);
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::TexSubImage2D(
            gl::TEXTURE_2D,
            0,
            0,
            0,
            width,
            height,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const c_void,
        );
    }
}

/// Render the PPU viewer ImGui window showing pattern tables and nametables.
fn draw_ppu_viewer_window(
    ui: &imgui::Ui,
    nt_texture_id: imgui::TextureId,
    tiles_texture_id: imgui::TextureId,
    scroll: (u16, u16),
) {
    // Aspect ratios of the underlying textures.
    const TILES_ASPECT: f32 =
        PPU_VIEWER_TILES_TEXTURE_HEIGHT as f32 / PPU_VIEWER_TILES_TEXTURE_WIDTH as f32;
    const NT_ASPECT: f32 = PPU_VIEWER_NT_TEXTURE_HEIGHT as f32 / PPU_VIEWER_NT_TEXTURE_WIDTH as f32;

    // NES visible area in nametable-texture pixels.
    const VISIBLE_W: f32 = 256.0;
    const VISIBLE_H: f32 = 240.0;
    const NT_TEX_W: f32 = PPU_VIEWER_NT_TEXTURE_WIDTH as f32;
    const NT_TEX_H: f32 = PPU_VIEWER_NT_TEXTURE_HEIGHT as f32;

    ui.window("PPU Viewer")
        .size(
            [
                PPU_VIEWER_WINDOW_INITIAL_WIDTH,
                PPU_VIEWER_WINDOW_INITIAL_HEIGHT,
            ],
            imgui::Condition::FirstUseEver,
        )
        .build(|| {
            ui.text("Pattern Tables");
            ui.separator();
            let avail_w = ui.content_region_avail()[0];
            imgui::Image::new(tiles_texture_id, [avail_w, avail_w * TILES_ASPECT]).build(ui);

            ui.dummy([0.0, 6.0]);
            ui.text("Nametables (2\u{00D7}2)");
            ui.separator();
            let avail_w = ui.content_region_avail()[0];
            let img_h = avail_w * NT_ASPECT;
            let img_origin = ui.cursor_screen_pos();
            imgui::Image::new(nt_texture_id, [avail_w, img_h]).build(ui);

            // Scale factors from nametable-texture pixels to screen pixels.
            let sx = avail_w / NT_TEX_W;
            let sy = img_h / NT_TEX_H;

            draw_scroll_rect(
                ui,
                img_origin,
                scroll,
                (sx, sy),
                (VISIBLE_W, VISIBLE_H),
                (NT_TEX_W, NT_TEX_H),
            );
        });
}

/// Draw a scroll-position rectangle (with wrapping) over the nametable image.
///
/// Handles cases where the 256×240 view window wraps past the 512×480 boundary by
/// splitting into up to four partial rectangles.
fn draw_scroll_rect(
    ui: &imgui::Ui,
    img_origin: [f32; 2],
    scroll: (u16, u16),
    scale: (f32, f32),
    visible_size: (f32, f32),
    nametable_size: (f32, f32),
) {
    let ox = img_origin[0];
    let oy = img_origin[1];
    let scroll_x = scroll.0 as f32;
    let scroll_y = scroll.1 as f32;
    let (sx, sy) = scale;
    let (visible_w, visible_h) = visible_size;
    let (nt_w, nt_h) = nametable_size;

    // Each x-segment carries (x_start, width, draw_left_edge, draw_right_edge).
    // At a wrap seam the "inside" edge is omitted: the right segment loses its right edge
    // and the left (wrapped) segment loses its left edge.
    let x_wraps = scroll_x + visible_w > nt_w;
    let x_segs: &[(f32, f32, bool, bool)] = if x_wraps {
        let left_w = nt_w - scroll_x;
        let right_w = visible_w - left_w;
        &[(scroll_x, left_w, true, false), (0.0, right_w, false, true)]
    } else {
        &[(scroll_x, visible_w, true, true)]
    };

    // Each y-segment carries (y_start, height, draw_top_edge, draw_bottom_edge).
    let y_wraps = scroll_y + visible_h > nt_h;
    let y_segs: &[(f32, f32, bool, bool)] = if y_wraps {
        let top_h = nt_h - scroll_y;
        let bot_h = visible_h - top_h;
        &[(scroll_y, top_h, true, false), (0.0, bot_h, false, true)]
    } else {
        &[(scroll_y, visible_h, true, true)]
    };

    let draw_list = ui.get_window_draw_list();
    let color = [1.0f32, 1.0, 0.0, 1.0]; // yellow
    let thickness = 1.5;

    for &(xs, xw, draw_left, draw_right) in x_segs {
        for &(ys, yh, draw_top, draw_bottom) in y_segs {
            let x0 = ox + xs * sx;
            let y0 = oy + ys * sy;
            let x1 = x0 + xw * sx;
            let y1 = y0 + yh * sy;

            if draw_top {
                draw_list
                    .add_line([x0, y0], [x1, y0], color)
                    .thickness(thickness)
                    .build();
            }
            if draw_bottom {
                draw_list
                    .add_line([x0, y1], [x1, y1], color)
                    .thickness(thickness)
                    .build();
            }
            if draw_left {
                draw_list
                    .add_line([x0, y0], [x0, y1], color)
                    .thickness(thickness)
                    .build();
            }
            if draw_right {
                draw_list
                    .add_line([x1, y0], [x1, y1], color)
                    .thickness(thickness)
                    .build();
            }
        }
    }
}

/// Upload pixel data for the PPU nametable and pattern table textures from the current NES state.
fn update_ppu_viewer_textures(
    nes: &Nes,
    nt_texture: gl::types::GLuint,
    tiles_texture: gl::types::GLuint,
) -> (u16, u16) {
    let ppu_snap = PpuViewerSnapshot::from_nes(nes);
    let nt_pixels = render_nametables_rgba(
        &ppu_snap.chr,
        &ppu_snap.nametables,
        &ppu_snap.palette,
        ppu_snap.bg_pattern_table,
    );
    let tiles_pixels = render_pattern_tables_rgba(&ppu_snap.chr, &ppu_snap.palette);
    unsafe {
        upload_rgba_texture(
            nt_texture,
            PPU_VIEWER_NT_TEXTURE_WIDTH,
            PPU_VIEWER_NT_TEXTURE_HEIGHT,
            &nt_pixels,
        );
        upload_rgba_texture(
            tiles_texture,
            PPU_VIEWER_TILES_TEXTURE_WIDTH,
            PPU_VIEWER_TILES_TEXTURE_HEIGHT,
            &tiles_pixels,
        );
    }
    ppu_snap.scroll
}

impl Drop for GlBackend {
    fn drop(&mut self) {
        // Best-effort: make current and delete textures.
        let _ = self.render_target.make_current();
        unsafe {
            gl::DeleteTextures(1, &self.nes_texture);
            gl::DeleteTextures(1, &self.ppu_viewer_nt_texture);
            gl::DeleteTextures(1, &self.ppu_viewer_tiles_texture);
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
