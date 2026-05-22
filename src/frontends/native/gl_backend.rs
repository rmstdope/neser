use crate::frontends::native::egui_renderer::{EguiFrameInput, NativeEguiRenderer};
use crate::frontends::native::egui_texture::NativeTextureName;
use crate::frontends::native::input::{EguiInputState, InputEvent, apply_imgui_input};
use crate::frontends::native::shader_manager::ShaderManager;
use crate::gb::debugging::GbDebuggerViewState;
use crate::gb::debugging::debugger_ui as gb_debugger_ui;
use crate::nes::console::Nes;
use crate::nes::debugging::DebuggerViewState;
use crate::nes::debugging::ppu_viewer::{
    PpuViewerSnapshot, render_nametables_rgba, render_pattern_tables_rgba,
};
use crate::nes::debugging::ui::{
    self as debugger_ui, BreakpointAddUiState, HexdumpUiState, WatchlistUiState,
};
use crate::platform::app_context::SharedAppContext;
use crate::platform::debugging::breakpoints::BreakpointList;
use crate::platform::debugging::log_info;
use crate::platform::emulator::Console;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Instant;

const PPU_VIEWER_NT_TEXTURE_WIDTH: i32 = 512;
const PPU_VIEWER_NT_TEXTURE_HEIGHT: i32 = 480;
const PPU_VIEWER_TILES_TEXTURE_WIDTH: i32 = 256;
const PPU_VIEWER_TILES_TEXTURE_HEIGHT: i32 = 128;

const PPU_VIEWER_WINDOW_INITIAL_WIDTH: f32 = 560.0;
const PPU_VIEWER_WINDOW_INITIAL_HEIGHT: f32 = 854.0;

// GB PPU viewer texture dimensions
const GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_DMG: i32 = 256;
const GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_DMG: i32 = 128;
const GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_CGB: i32 = 512;
const GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_CGB: i32 = 128;
const GB_PPU_VIEWER_BG_MAPS_TEXTURE_WIDTH: i32 = 512;
const GB_PPU_VIEWER_BG_MAPS_TEXTURE_HEIGHT: i32 = 256;

const GB_PPU_VIEWER_WINDOW_INITIAL_WIDTH: f32 = 560.0;
const GB_PPU_VIEWER_WINDOW_INITIAL_HEIGHT: f32 = 600.0;

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
    /// Enables or disables mouse confinement to the render target window.
    fn set_mouse_grab(&mut self, enabled: bool) -> Result<(), String>;
    /// Notifies the render target that the physical window size has changed.
    ///
    /// Must be called whenever the OS sends a resize event (including fullscreen
    /// transitions) so that the platform surface can be updated to match the new
    /// physical pixel dimensions. Without this, the presented surface may remain
    /// at the old size, causing the rendered image to be clipped.
    ///
    /// The default implementation is a no-op for platforms that resize automatically.
    fn notify_resize(&mut self, _w: u32, _h: u32) {}
}

/// Loader for GL procedure addresses used by OpenGL and related backends.
pub type ProcAddressLoader = Rc<dyn Fn(&str) -> *const c_void>;

/// OpenGL renderer that draws the frame and optional debugger UI.
pub struct GlBackend {
    render_target: Box<dyn RenderTarget>,
    glow_context: std::sync::Arc<glow::Context>,
    egui_renderer: NativeEguiRenderer,
    egui_input: EguiInputState,
    imgui: imgui::Context,
    imgui_renderer: imgui_glow_renderer::Renderer,
    nes_texture: gl::types::GLuint,
    nes_texture_id: imgui::TextureId,
    nes_egui_texture_id: egui::TextureId,
    ppu_viewer_nt_texture: gl::types::GLuint,
    ppu_viewer_nt_texture_id: imgui::TextureId,
    ppu_viewer_tiles_texture: gl::types::GLuint,
    ppu_viewer_tiles_texture_id: imgui::TextureId,
    overlay_font: imgui::FontId,
    overlay_text_color: OverlayTextColor,
    app_context: SharedAppContext,
    framebuffer: Vec<u8>,
    last_frame: Instant,
    // NES debugger state
    debugger_view_state: DebuggerViewState,
    debugger_alpha: f32,
    breakpoints: BreakpointList,
    bp_add_state: BreakpointAddUiState,
    hexdump_ui_state: HexdumpUiState,
    watchlist_ui_state: WatchlistUiState,
    // GB debugger state
    gb_debugger_view_state: GbDebuggerViewState,
    gb_debugger_alpha: f32,
    gb_breakpoints: BreakpointList,
    gb_bp_add_state: gb_debugger_ui::BreakpointAddUiState,
    gb_hexdump_ui_state: gb_debugger_ui::HexdumpUiState,
    gb_watchlist_ui_state: gb_debugger_ui::WatchlistUiState,
    last_gb_action: gb_debugger_ui::GbDebuggerUiAction,
    // GB PPU viewer textures
    gb_ppu_viewer_tiles_texture: gl::types::GLuint,
    gb_ppu_viewer_tiles_texture_id: imgui::TextureId,
    gb_ppu_viewer_bg_maps_texture: gl::types::GLuint,
    gb_ppu_viewer_bg_maps_texture_id: imgui::TextureId,
    shader_manager: ShaderManager,
    /// Current GL texture width (updated when console type changes).
    tex_w: u32,
    /// Current GL texture height (updated when console type changes).
    tex_h: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameBackgroundRenderer {
    Egui,
    ImGui,
}

fn frame_background_renderer(
    shader_output_texture_id: Option<imgui::TextureId>,
) -> FrameBackgroundRenderer {
    if shader_output_texture_id.is_some() {
        FrameBackgroundRenderer::ImGui
    } else {
        FrameBackgroundRenderer::Egui
    }
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
    let layout = crate::frontends::native::ui_geometry::top_left_text_panel(
        [x0, y0],
        text_size,
        [8.0, 8.0],
        [6.0, 4.0],
    );

    draw_list
        .add_rect(
            layout.rect_min,
            layout.rect_max,
            overlay_background_color_for(text_color),
        )
        .filled(true)
        .build();
    draw_list.add_text(
        layout.text_pos,
        overlay_text_rgba(text_color, blink_red),
        text,
    );
}

fn draw_crosshair(ui: &imgui::Ui, crosshair: Crosshair, draw_ctx: &CrosshairDrawContext) {
    let color = [1.0, 0.2, 0.2, 1.0];
    let draw_list = ui.get_background_draw_list();

    let (ix, iy) = project_crosshair_to_cropped_indices(crosshair, draw_ctx);
    let rects = crate::frontends::native::ui_geometry::crosshair_marker_rects(
        [draw_ctx.x0, draw_ctx.y0],
        [draw_ctx.draw_w, draw_ctx.draw_h],
        [draw_ctx.cropped_w, draw_ctx.cropped_h],
        [ix, iy],
    );

    for rect in rects {
        draw_list
            .add_rect(rect.rect_min, rect.rect_max, color)
            .filled(true)
            .build();
    }
}

struct CrosshairDrawContext {
    x0: f32,
    y0: f32,
    draw_w: f32,
    draw_h: f32,
    cropped_w: u32,
    cropped_h: u32,
    h_overscan: u32,
    v_overscan: u32,
}

fn project_crosshair_to_cropped_indices(
    crosshair: Crosshair,
    draw_ctx: &CrosshairDrawContext,
) -> (f32, f32) {
    let nes_x = crosshair.x.floor();
    let nes_y = crosshair.y.floor();
    let ix = (nes_x - draw_ctx.h_overscan as f32)
        .clamp(0.0, (draw_ctx.cropped_w.saturating_sub(1)) as f32);
    let iy = (nes_y - draw_ctx.v_overscan as f32)
        .clamp(0.0, (draw_ctx.cropped_h.saturating_sub(1)) as f32);
    (ix, iy)
}

fn draw_fps_counter(
    ui: &imgui::Ui,
    font: imgui::FontId,
    fps: usize,
    x0: f32,
    y0: f32,
    draw_w: f32,
) {
    let draw_list = ui.get_background_draw_list();
    let _font = ui.push_font(font);
    let text = fps_counter_text(fps);
    let text_size = ui.calc_text_size(&text);
    let layout = crate::frontends::native::ui_geometry::top_right_text_panel(
        [x0, y0],
        draw_w,
        text_size,
        [8.0, 8.0],
        [6.0, 4.0],
    );
    draw_list
        .add_rect(layout.rect_min, layout.rect_max, [0.0, 0.0, 0.0, 0.6])
        .filled(true)
        .rounding(3.0)
        .build();
    draw_list.add_text(layout.text_pos, [1.0, 1.0, 0.0, 1.0], &text);
}

fn fps_counter_text(fps: usize) -> String {
    format!("{fps} FPS")
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
        let layout = crate::frontends::native::ui_geometry::bottom_center_text_panel(
            [x0, y0],
            [draw_w, draw_h],
            text_size,
            stack_index,
            bottom_margin,
            spacing,
            padding,
        );

        draw_list
            .add_rect(layout.rect_min, layout.rect_max, toast_background_rgba())
            .filled(true)
            .build();
        draw_list.add_text(layout.text_pos, toast_text_rgba(), toast_text);
    }
}

impl OverlayTextColor {
    fn toggle(self) -> Self {
        match self {
            OverlayTextColor::White => OverlayTextColor::Black,
            OverlayTextColor::Black => OverlayTextColor::White,
        }
    }

    fn rgba(self) -> [f32; 4] {
        match self {
            OverlayTextColor::White => [1.0, 1.0, 1.0, 1.0],
            OverlayTextColor::Black => [0.0, 0.0, 0.0, 1.0],
        }
    }
}

fn draw_egui_frame_background(
    ui: &mut egui::Ui,
    texture_id: egui::TextureId,
    x0: f32,
    y0: f32,
    draw_w: f32,
    draw_h: f32,
) {
    let rect = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x0 + draw_w, y0 + draw_h));
    ui.painter().image(
        texture_id,
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

fn draw_egui_overlay_text(
    ui: &mut egui::Ui,
    text: &str,
    text_color: OverlayTextColor,
    blink_red: bool,
    x0: f32,
    y0: f32,
) {
    let painter = ui.painter();
    let overlay_style = text_color;
    let egui_text_color = egui_color_from_rgba(overlay_text_rgba(overlay_style, blink_red));
    let background_color = egui_color_from_rgba(overlay_background_color_for(overlay_style));
    let galley = painter.layout_no_wrap(text.to_owned(), overlay_egui_font_id(), egui_text_color);
    let text_size = [galley.size().x, galley.size().y];
    let layout = crate::frontends::native::ui_geometry::top_left_text_panel(
        [x0, y0],
        text_size,
        [8.0, 8.0],
        [6.0, 4.0],
    );

    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(layout.rect_min[0], layout.rect_min[1]),
            egui::pos2(layout.rect_max[0], layout.rect_max[1]),
        ),
        0.0,
        background_color,
    );
    painter.galley(
        egui::pos2(layout.text_pos[0], layout.text_pos[1]),
        galley,
        egui_text_color,
    );
}

fn draw_egui_fps_counter(ui: &mut egui::Ui, fps: usize, x0: f32, y0: f32, draw_w: f32) {
    let painter = ui.painter();
    let text = fps_counter_text(fps);
    let text_color = egui::Color32::YELLOW;
    let galley = painter.layout_no_wrap(text, overlay_egui_font_id(), text_color);
    let text_size = [galley.size().x, galley.size().y];
    let layout = crate::frontends::native::ui_geometry::top_right_text_panel(
        [x0, y0],
        draw_w,
        text_size,
        [8.0, 8.0],
        [6.0, 4.0],
    );

    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(layout.rect_min[0], layout.rect_min[1]),
            egui::pos2(layout.rect_max[0], layout.rect_max[1]),
        ),
        3.0,
        egui_color_from_rgba([0.0, 0.0, 0.0, 0.6]),
    );
    painter.galley(
        egui::pos2(layout.text_pos[0], layout.text_pos[1]),
        galley,
        text_color,
    );
}

fn overlay_egui_font_id() -> egui::FontId {
    egui::FontId::proportional(26.0)
}

fn egui_color_from_rgba(rgba: [f32; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        (rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
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

fn egui_frame_input_for_window(
    window_size: (u32, u32),
    drawable_size: (u32, u32),
    predicted_dt: f32,
) -> EguiFrameInput {
    let (win_w, win_h) = window_size;
    let (drawable_w, drawable_h) = drawable_size;
    EguiFrameInput::new(
        [win_w as f32, win_h as f32],
        [drawable_w, drawable_h],
        egui_pixels_per_point(window_size, drawable_size),
        predicted_dt,
    )
}

fn egui_pixels_per_point(window_size: (u32, u32), drawable_size: (u32, u32)) -> f32 {
    let (win_w, win_h) = window_size;
    let (drawable_w, drawable_h) = drawable_size;
    if win_w > 0 {
        drawable_w as f32 / win_w as f32
    } else if win_h > 0 {
        drawable_h as f32 / win_h as f32
    } else {
        1.0
    }
}

impl GlBackend {
    /// Returns the aspect ratio used for rendering the current frame.
    fn target_aspect(&self, pixel_aspect: f32) -> f32 {
        let w = self.tex_w as f32;
        let h = self.tex_h as f32;
        if h == 0.0 {
            return 1.0;
        }
        (w / h) * pixel_aspect
    }

    /// Returns the logical window size in pixels reported by the render target.
    pub fn window_size(&self) -> (u32, u32) {
        self.render_target.window_size()
    }

    /// Enables or disables fullscreen mode for the underlying render target.
    pub fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        self.render_target.set_fullscreen(enabled)
    }

    /// Enables or disables mouse grab on the render target window.
    pub fn set_mouse_grab(&mut self, enabled: bool) -> Result<(), String> {
        self.render_target.set_mouse_grab(enabled)
    }

    /// Notifies the render target of a physical window resize.
    ///
    /// Must be called from the OS window-resize event handler (including fullscreen
    /// transitions) so the underlying GL surface stays in sync with the new dimensions.
    pub fn notify_resize(&mut self, w: u32, h: u32) {
        self.render_target.notify_resize(w, h);
    }

    /// Creates a new OpenGL renderer bound to the provided render target.
    pub fn new(
        render_target: Box<dyn RenderTarget>,
        proc_address: ProcAddressLoader,
        shader_path: Option<&str>,
        app_context: SharedAppContext,
        allowed_shaders: &[&str],
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

        // Create glow context (shared by imgui renderer and librashader).
        let glow_context = unsafe {
            let proc_address = proc_address.clone();
            std::sync::Arc::new(glow::Context::from_loader_function(|s| {
                (proc_address)(s) as *const _
            }))
        };

        let egui_glow_context = unsafe {
            let proc_address = proc_address.clone();
            std::sync::Arc::new(egui_glow::glow::Context::from_loader_function(|s| {
                (proc_address)(s) as *const _
            }))
        };
        let mut egui_renderer = NativeEguiRenderer::new(egui_glow_context)?;

        let imgui_renderer = imgui_glow_renderer::Renderer::new(
            &glow_context,
            &mut imgui,
            &mut imgui_glow_renderer::SimpleTextureMap::default(),
            true,
        )
        .map_err(|e| format!("Failed to initialise imgui glow renderer: {e:?}"))?;

        let (nes_texture, nes_texture_id, nes_egui_texture_id) = unsafe {
            let mut tex: gl::types::GLuint = 0;
            gl::GenTextures(1, &mut tex);
            let texture_name = NativeTextureName::from_gl_id(tex)
                .ok_or_else(|| "Failed to create emulator frame texture".to_owned())?;
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

            // Allocate a 1×1 placeholder; the render loop resizes on first frame.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB8 as i32,
                1,
                1,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );

            let id: imgui::TextureId = (tex as usize).into();
            let egui_id = egui_renderer.register_native_texture(texture_name);
            (tex, id, egui_id)
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

        // GB PPU viewer textures
        let (gb_ppu_viewer_tiles_texture, gb_ppu_viewer_tiles_texture_id) = unsafe {
            create_rgba_texture(
                GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_CGB,
                GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_CGB,
            )
        };
        let (gb_ppu_viewer_bg_maps_texture, gb_ppu_viewer_bg_maps_texture_id) = unsafe {
            create_rgba_texture(
                GB_PPU_VIEWER_BG_MAPS_TEXTURE_WIDTH,
                GB_PPU_VIEWER_BG_MAPS_TEXTURE_HEIGHT,
            )
        };

        let mut shader_manager = ShaderManager::new(allowed_shaders);

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
            egui_renderer,
            egui_input: EguiInputState::default(),
            imgui,
            imgui_renderer,
            nes_texture,
            nes_texture_id,
            nes_egui_texture_id,
            ppu_viewer_nt_texture,
            ppu_viewer_nt_texture_id,
            ppu_viewer_tiles_texture,
            ppu_viewer_tiles_texture_id,
            overlay_font,
            overlay_text_color: OverlayTextColor::White,
            app_context,
            framebuffer: Vec::new(),
            last_frame: Instant::now(),
            // NES debugger state
            debugger_view_state: DebuggerViewState::default(),
            debugger_alpha: 1.0,
            breakpoints: BreakpointList::default(),
            bp_add_state: BreakpointAddUiState::default(),
            hexdump_ui_state: HexdumpUiState::default(),
            watchlist_ui_state: WatchlistUiState::default(),
            // GB debugger state
            gb_debugger_view_state: GbDebuggerViewState::default(),
            gb_debugger_alpha: 1.0,
            gb_breakpoints: BreakpointList::default(),
            gb_bp_add_state: gb_debugger_ui::BreakpointAddUiState::default(),
            gb_hexdump_ui_state: gb_debugger_ui::HexdumpUiState::default(),
            gb_watchlist_ui_state: gb_debugger_ui::WatchlistUiState::default(),
            last_gb_action: gb_debugger_ui::GbDebuggerUiAction::default(),
            // GB PPU viewer textures
            gb_ppu_viewer_tiles_texture,
            gb_ppu_viewer_tiles_texture_id,
            gb_ppu_viewer_bg_maps_texture,
            gb_ppu_viewer_bg_maps_texture_id,
            shader_manager,
            tex_w: 1,
            tex_h: 1,
        })
    }

    /// Applies an input event to ImGui and handles renderer-local shortcuts.
    pub fn handle_input(&mut self, event: &InputEvent) {
        if let InputEvent::Key {
            key: crate::frontends::native::input::UiKey::F1,
            down: true,
        } = event
        {
            self.overlay_text_color = self.overlay_text_color.toggle();
        }

        apply_imgui_input(self.imgui.io_mut(), event);
        self.egui_input.apply_input(event);
    }

    /// Sets the debugger window background opacity (clamped to 0.1–1.0).
    pub fn set_debugger_alpha(&mut self, alpha: f32) {
        self.debugger_alpha = alpha.clamp(0.1, 1.0);
    }

    /// Updates the breakpoint list used by the debugger UI.
    pub fn update_breakpoints(&mut self, breakpoints: &BreakpointList) {
        self.breakpoints = breakpoints.clone();
    }

    // GB debugger methods

    /// Sets the GB debugger window background opacity (clamped to 0.1–1.0).
    #[allow(dead_code)] // Used by debugger controller, not yet wired up
    pub fn set_gb_debugger_alpha(&mut self, alpha: f32) {
        self.gb_debugger_alpha = alpha.clamp(0.1, 1.0);
    }

    /// Updates the GB breakpoint list used by the GB debugger UI.
    pub fn update_gb_breakpoints(&mut self, breakpoints: &BreakpointList) {
        self.gb_breakpoints = breakpoints.clone();
    }

    /// Takes the last GB debugger UI action, replacing it with default.
    pub fn take_gb_debugger_action(&mut self) -> gb_debugger_ui::GbDebuggerUiAction {
        std::mem::take(&mut self.last_gb_action)
    }

    /// Returns a copy of the watch addresses currently tracked in the debugger.
    pub fn watch_addresses(&self) -> Vec<u16> {
        self.debugger_view_state.watch_addresses()
    }

    /// Replaces the debugger's watch address list with the provided addresses.
    pub fn set_watch_addresses(&mut self, addresses: Vec<u16>) {
        self.debugger_view_state.clear_watch_addresses();
        for addr in addresses {
            self.debugger_view_state.add_watch_address(addr);
        }
    }

    /// Renders the current frame and optional debugger overlay.
    ///
    /// Accepts a `&Console` so it works for both NES and Game Boy. The debugger
    /// overlay is only drawn when `console` is a `Console::Nes` variant.
    pub fn render(
        &mut self,
        console: &Console,
        show_debugger: bool,
        overlay_text: Option<&str>,
        overlay_blink_red: bool,
        crosshair: Option<Crosshair>,
        fps: Option<usize>,
    ) -> debugger_ui::DebuggerUiAction {
        let mut action = debugger_ui::DebuggerUiAction::default();

        // Per-frame IO updates
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.imgui.io_mut().delta_time = dt.as_secs_f32().max(1.0 / 1000.0);
        let predicted_dt = self.imgui.io().delta_time;

        let (win_w, win_h) = self.render_target.window_size();
        let (drawable_w, drawable_h) = self.render_target.drawable_size();
        let egui_frame_input =
            egui_frame_input_for_window((win_w, win_h), (drawable_w, drawable_h), predicted_dt);

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

        // Compute overscan and pixel dimensions from the console.
        let (h_overscan, v_overscan) = console.overscan();
        let (frame_w, frame_h) = console.cropped_dims(h_overscan, v_overscan);

        // Update texture (keep the borrow short-lived).
        {
            let cropped = console.cropped_screen_snapshot(h_overscan, v_overscan);
            // Resize framebuffer if dimensions changed (e.g. NES → GB switch).
            let required = (frame_w * frame_h * 3) as usize;
            if self.framebuffer.len() != required {
                self.framebuffer.resize(required, 0);
            }
            self.framebuffer.copy_from_slice(&cropped);
        }

        let tex_w = frame_w as i32;
        let tex_h = frame_h as i32;

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.nes_texture);
            // Reallocate texture storage if dimensions changed.
            if self.tex_w != frame_w || self.tex_h != frame_h {
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGB8 as i32,
                    tex_w,
                    tex_h,
                    0,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    std::ptr::null(),
                );
                self.tex_w = frame_w;
                self.tex_h = frame_h;
            }
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                tex_w,
                tex_h,
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

        let target_aspect = self.target_aspect(console.pixel_aspect());

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
                crate::frontends::native::ui_geometry::letterbox_size(
                    drawable_w_f,
                    drawable_h_f,
                    target_aspect,
                );
            let shader_out_w = (shader_out_w_f as u32).max(1);
            let shader_out_h = (shader_out_h_f as u32).max(1);

            if let Err(e) = self.shader_manager.apply_shader(
                self.nes_texture,
                tex_w as u32,
                tex_h as u32,
                shader_out_w,
                shader_out_h,
            ) {
                log_info(format!("Shader application error: {}", e));
            } else if let Some(tex) = self.shader_manager.output_texture() {
                shader_output_texture_id = Some((tex as usize).into());
            }
        }

        let visible_toasts = self.app_context.borrow_mut().visible_toasts(now);
        let cropped_w = self.tex_w;
        let cropped_h = self.tex_h;

        let win_w = win_w as f32;
        let win_h = win_h as f32;
        let frame_rect = crate::frontends::native::ui_geometry::letterbox_rect(
            [0.0, 0.0],
            [win_w, win_h],
            target_aspect,
        );
        let x0 = frame_rect.rect_min[0];
        let y0 = frame_rect.rect_min[1];
        let [draw_w, draw_h] = frame_rect.size();
        let background_renderer = frame_background_renderer(shader_output_texture_id);

        let egui_texture_id = self.nes_egui_texture_id;
        let egui_paint_data =
            self.egui_renderer
                .run(egui_frame_input, &mut self.egui_input, |ui| {
                    if background_renderer == FrameBackgroundRenderer::Egui {
                        draw_egui_frame_background(ui, egui_texture_id, x0, y0, draw_w, draw_h);
                        if let Some(text) = overlay_text {
                            draw_egui_overlay_text(
                                ui,
                                text,
                                self.overlay_text_color,
                                overlay_blink_red,
                                x0,
                                y0,
                            );
                        }
                        if let Some(fps_value) = fps {
                            draw_egui_fps_counter(ui, fps_value, x0, y0, draw_w);
                        }
                    }
                });
        self.egui_renderer.paint(egui_paint_data);

        // Start ImGui frame
        {
            let ui = self.imgui.frame();

            let background_texture = shader_output_texture_id.unwrap_or(self.nes_texture_id);
            if background_renderer == FrameBackgroundRenderer::ImGui {
                draw_frame_background(ui, background_texture, x0, y0, draw_w, draw_h);
            }

            if background_renderer == FrameBackgroundRenderer::ImGui
                && let Some(text) = overlay_text
            {
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
                let draw_ctx = CrosshairDrawContext {
                    x0,
                    y0,
                    draw_w,
                    draw_h,
                    cropped_w,
                    cropped_h,
                    h_overscan,
                    v_overscan,
                };
                draw_crosshair(ui, crosshair, &draw_ctx);
            }

            if background_renderer == FrameBackgroundRenderer::ImGui
                && let Some(fps_value) = fps
            {
                draw_fps_counter(ui, self.overlay_font, fps_value, x0, y0, draw_w);
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

            if show_debugger && let Console::Nes(nes) = console {
                let snapshot = self.debugger_view_state.snapshot(nes);
                action = debugger_ui::render(
                    ui,
                    &snapshot,
                    self.debugger_alpha,
                    &self.breakpoints,
                    &mut self.bp_add_state,
                    &mut self.hexdump_ui_state,
                    &mut self.watchlist_ui_state,
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
                if let Some(address) = action.add_watch_address {
                    self.debugger_view_state.add_watch_address(address);
                }
                if let Some(index) = action.remove_watch_address {
                    self.debugger_view_state.remove_watch_address(index);
                }
                if let Some(update) = action.update_watch_address {
                    self.debugger_view_state
                        .update_watch_address(update.index, update.address);
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
            } else if show_debugger && let Console::GameBoy(gb) = console {
                let snapshot = gb.create_debugger_snapshot(&mut self.gb_debugger_view_state);
                let gb_action = gb_debugger_ui::render(
                    ui,
                    &snapshot,
                    self.gb_debugger_alpha,
                    &self.gb_breakpoints,
                    &mut self.gb_bp_add_state,
                    &mut self.gb_hexdump_ui_state,
                    &mut self.gb_watchlist_ui_state,
                );
                if gb_action.toggle_ppu_viewer {
                    self.gb_debugger_view_state.toggle_ppu_viewer();
                }
                if let Some(base) = gb_action.set_wram_hexdump_base {
                    self.gb_debugger_view_state.set_wram_hexdump_base(base);
                }
                if let Some(delta) = gb_action.nudge_wram_hexdump_base_by_bytes {
                    self.gb_debugger_view_state
                        .nudge_wram_hexdump_base_by_bytes_from(snapshot.wram_hexdump_base, delta);
                }
                if let Some(base) = gb_action.set_vram_hexdump_base {
                    self.gb_debugger_view_state.set_vram_hexdump_base(base);
                }
                if let Some(delta) = gb_action.nudge_vram_hexdump_base_by_bytes {
                    self.gb_debugger_view_state
                        .nudge_vram_hexdump_base_by_bytes_from(snapshot.vram_hexdump_base, delta);
                }
                if let Some(address) = gb_action.add_watch_address {
                    self.gb_debugger_view_state.add_watch_address(address);
                }
                if let Some(index) = gb_action.remove_watch_address {
                    self.gb_debugger_view_state.remove_watch_address(index);
                }
                if let Some(update) = gb_action.update_watch_address {
                    self.gb_debugger_view_state
                        .update_watch_address(update.index, update.address);
                }
                if gb_action.increase_opacity {
                    self.gb_debugger_alpha = (self.gb_debugger_alpha + 0.1).min(1.0);
                }
                if gb_action.decrease_opacity {
                    self.gb_debugger_alpha = (self.gb_debugger_alpha - 0.1).max(0.1);
                }
                // Store action for processing by controller
                self.last_gb_action = gb_action;
                if self.gb_debugger_view_state.is_ppu_viewer_visible() {
                    let ppu_snap = gb.create_ppu_viewer_snapshot();
                    update_gb_ppu_viewer_textures_from_snapshot(
                        &ppu_snap,
                        self.gb_ppu_viewer_tiles_texture,
                        self.gb_ppu_viewer_bg_maps_texture,
                    );
                    draw_gb_ppu_viewer_window(
                        ui,
                        self.gb_ppu_viewer_tiles_texture_id,
                        self.gb_ppu_viewer_bg_maps_texture_id,
                        &ppu_snap,
                    );
                }
            }
        }

        let draw_data = self.imgui.render();
        if let Err(err) = self.imgui_renderer.render(
            &self.glow_context,
            &imgui_glow_renderer::SimpleTextureMap::default(),
            draw_data,
        ) {
            log_info(format!("imgui render failed: {}", err));
        }

        self.render_target.swap_buffers();

        action
    }

    /// Cycles through available shader presets, if any.
    ///
    /// Returns the short preset name (e.g. `"crt"`, `"dmg"`) of the newly
    /// active preset, or `None` when cycling has landed on stock (no shader).
    pub fn cycle_shader(&mut self) -> Option<String> {
        if let Err(e) = self.shader_manager.cycle_shader(self.glow_context.clone()) {
            log_info(format!("Error cycling shader: {}", e));
        }
        let path = self.shader_manager.current_preset_name();
        let short_name = path.and_then(|p| {
            crate::platform::shaders::SHADER_PRESETS
                .iter()
                .find(|(_, preset_path)| *preset_path == p)
                .map(|(name, _)| (*name).to_owned())
        });
        log_info(format!(
            "Switched to filter: {}",
            short_name.as_deref().unwrap_or("off")
        ));
        short_name
    }
}

/// Returns the toast message to display when the visual filter changes.
///
/// * `Some(name)` → `"Visual Filter: <NAME>"` (short preset name, uppercased)
/// * `None`       → `"Visual Filter: off"`
pub fn shader_toast_message(preset_name: Option<&str>) -> String {
    match preset_name {
        Some(name) => format!("Visual Filter: {}", name.to_uppercase()),
        None => "Visual Filter: off".to_owned(),
    }
}

#[cfg(test)]
mod tests_letterbox {
    use crate::frontends::native::ui_geometry::letterbox_size;

    // NES pixel aspect (8:7) times NTSC display correction (16:15).
    const NTSC_ASPECT: f32 = 8.0 / 7.0 * 16.0 / 15.0;

    #[test]
    fn test_letterbox_size_wide_container() {
        let (w, h) = letterbox_size(1920.0, 1080.0, NTSC_ASPECT);
        assert!((w - 1316.5714).abs() < 0.01);
        assert_eq!(h, 1080.0);
    }

    #[test]
    fn test_letterbox_size_matches_aspect() {
        let (w, h) = letterbox_size(800.0, 600.0, NTSC_ASPECT);
        assert!((w - 731.4286).abs() < 0.01);
        assert_eq!(h, 600.0);
    }

    #[test]
    fn test_letterbox_size_zero_height() {
        let (w, h) = letterbox_size(800.0, 0.0, NTSC_ASPECT);
        assert_eq!(w, 800.0);
        assert_eq!(h, 0.0);
    }
}

#[cfg(test)]
mod tests_crosshair_projection {
    use super::{Crosshair, CrosshairDrawContext, project_crosshair_to_cropped_indices};

    #[test]
    fn test_crosshair_projection_without_overscan() {
        let draw_ctx = CrosshairDrawContext {
            x0: 0.0,
            y0: 0.0,
            draw_w: 256.0,
            draw_h: 240.0,
            cropped_w: 256,
            cropped_h: 240,
            h_overscan: 0,
            v_overscan: 0,
        };
        let (ix, iy) =
            project_crosshair_to_cropped_indices(Crosshair { x: 10.0, y: 20.0 }, &draw_ctx);
        assert_eq!(ix, 10.0);
        assert_eq!(iy, 20.0);
    }

    #[test]
    fn test_crosshair_projection_applies_vertical_overscan_offset() {
        let draw_ctx = CrosshairDrawContext {
            x0: 0.0,
            y0: 0.0,
            draw_w: 256.0,
            draw_h: 224.0,
            cropped_w: 256,
            cropped_h: 224,
            h_overscan: 0,
            v_overscan: 8,
        };
        let (ix, iy) =
            project_crosshair_to_cropped_indices(Crosshair { x: 100.0, y: 40.0 }, &draw_ctx);
        assert_eq!(ix, 100.0);
        assert_eq!(iy, 32.0);
    }

    #[test]
    fn test_crosshair_projection_clamps_to_visible_region() {
        let draw_ctx = CrosshairDrawContext {
            x0: 0.0,
            y0: 0.0,
            draw_w: 240.0,
            draw_h: 208.0,
            cropped_w: 240,
            cropped_h: 208,
            h_overscan: 8,
            v_overscan: 16,
        };
        let (ix, iy) =
            project_crosshair_to_cropped_indices(Crosshair { x: 255.0, y: 239.0 }, &draw_ctx);
        assert_eq!(ix, 239.0);
        assert_eq!(iy, 207.0);
    }
}

#[cfg(test)]
mod tests_egui_frame_input {
    use super::{
        FrameBackgroundRenderer, egui_color_from_rgba, egui_frame_input_for_window,
        fps_counter_text, frame_background_renderer,
    };

    #[test]
    fn egui_frame_input_uses_logical_and_drawable_sizes() {
        let frame_input = egui_frame_input_for_window((320, 240), (640, 480), 1.0 / 60.0);

        assert_eq!(frame_input.drawable_size(), [640, 480]);
        assert_eq!(frame_input.pixels_per_point(), 2.0);
    }

    #[test]
    fn egui_frame_input_falls_back_to_height_scale_when_width_is_zero() {
        let frame_input = egui_frame_input_for_window((0, 240), (640, 480), 1.0 / 60.0);

        assert_eq!(frame_input.pixels_per_point(), 2.0);
    }

    #[test]
    fn egui_frame_input_uses_unit_scale_for_empty_window() {
        let frame_input = egui_frame_input_for_window((0, 0), (640, 480), 1.0 / 60.0);

        assert_eq!(frame_input.pixels_per_point(), 1.0);
    }

    #[test]
    fn stock_frame_background_uses_egui_renderer() {
        assert_eq!(
            frame_background_renderer(None),
            FrameBackgroundRenderer::Egui
        );
    }

    #[test]
    fn shader_frame_background_keeps_imgui_until_shader_texture_is_registered() {
        assert_eq!(
            frame_background_renderer(Some(7usize.into())),
            FrameBackgroundRenderer::ImGui
        );
    }

    #[test]
    fn egui_color_from_rgba_converts_unit_floats_to_color32() {
        assert_eq!(
            egui_color_from_rgba([1.0, 0.5, 0.0, 0.25]),
            egui::Color32::from_rgba_unmultiplied(255, 128, 0, 64)
        );
    }

    #[test]
    fn egui_color_from_rgba_clamps_out_of_range_channels() {
        assert_eq!(
            egui_color_from_rgba([2.0, -1.0, 0.0, 1.5]),
            egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255)
        );
    }

    #[test]
    fn fps_counter_text_formats_value() {
        assert_eq!(fps_counter_text(60), "60 FPS");
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

/// Update GB PPU viewer textures with a snapshot.
fn update_gb_ppu_viewer_textures_from_snapshot(
    ppu_snap: &crate::gb::debugging::ppu_viewer::GbPpuViewerSnapshot,
    tiles_texture: gl::types::GLuint,
    bg_maps_texture: gl::types::GLuint,
) {
    use crate::gb::debugging::ppu_viewer::{render_bg_maps_rgba, render_tiles_rgba};
    let tiles_pixels = render_tiles_rgba(
        &ppu_snap.vram,
        &ppu_snap.vram_bank1,
        ppu_snap.bgp,
        &ppu_snap.bg_palette_ram,
        ppu_snap.cgb_mode,
    );
    let bg_maps_pixels = render_bg_maps_rgba(
        &ppu_snap.vram,
        &ppu_snap.vram_bank1,
        ppu_snap.lcdc,
        ppu_snap.bgp,
        &ppu_snap.bg_palette_ram,
        ppu_snap.cgb_mode,
    );

    // Pad DMG tiles to full CGB texture size to avoid stale data
    let tiles_upload_width = GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_CGB;
    let tiles_upload_height = GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_CGB;
    let tiles_upload_pixels = if ppu_snap.cgb_mode {
        tiles_pixels
    } else {
        let src_width = GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_DMG as usize;
        let src_height = GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_DMG as usize;
        let dst_width = GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_CGB as usize;
        let dst_height = GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_CGB as usize;
        let mut padded_pixels = vec![0u8; dst_width * dst_height * 4];

        for row in 0..src_height {
            let src_start = row * src_width * 4;
            let src_end = src_start + src_width * 4;
            let dst_start = row * dst_width * 4;
            let dst_end = dst_start + src_width * 4;
            padded_pixels[dst_start..dst_end].copy_from_slice(&tiles_pixels[src_start..src_end]);
        }

        padded_pixels
    };

    unsafe {
        upload_rgba_texture(
            tiles_texture,
            tiles_upload_width,
            tiles_upload_height,
            &tiles_upload_pixels,
        );
        upload_rgba_texture(
            bg_maps_texture,
            GB_PPU_VIEWER_BG_MAPS_TEXTURE_WIDTH,
            GB_PPU_VIEWER_BG_MAPS_TEXTURE_HEIGHT,
            &bg_maps_pixels,
        );
    }
}

/// Render the GB PPU viewer ImGui window showing tiles, BG maps, OAM, and palettes.
fn draw_gb_ppu_viewer_window(
    ui: &imgui::Ui,
    tiles_texture_id: imgui::TextureId,
    bg_maps_texture_id: imgui::TextureId,
    ppu_snap: &crate::gb::debugging::ppu_viewer::GbPpuViewerSnapshot,
) {
    use crate::gb::debugging::ppu_viewer::{format_oam_entries, format_palette_info};

    let is_cgb = ppu_snap.cgb_mode;
    // Aspect ratios for GB PPU viewer textures
    let tiles_aspect = if is_cgb {
        GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_CGB as f32 / GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_CGB as f32
    } else {
        GB_PPU_VIEWER_TILES_TEXTURE_HEIGHT_DMG as f32 / GB_PPU_VIEWER_TILES_TEXTURE_WIDTH_DMG as f32
    };
    const BG_MAPS_ASPECT: f32 =
        GB_PPU_VIEWER_BG_MAPS_TEXTURE_HEIGHT as f32 / GB_PPU_VIEWER_BG_MAPS_TEXTURE_WIDTH as f32;

    ui.window("GB PPU Viewer")
        .size(
            [
                GB_PPU_VIEWER_WINDOW_INITIAL_WIDTH,
                GB_PPU_VIEWER_WINDOW_INITIAL_HEIGHT,
            ],
            imgui::Condition::FirstUseEver,
        )
        .build(|| {
            let mode_label = if is_cgb { "CGB" } else { "DMG" };
            ui.text(format!("Tiles ({})", mode_label));
            ui.separator();
            let avail_w = ui.content_region_avail()[0];
            let tiles_uv1 = if is_cgb { [1.0, 1.0] } else { [0.5, 1.0] };
            imgui::Image::new(tiles_texture_id, [avail_w, avail_w * tiles_aspect])
                .uv0([0.0, 0.0])
                .uv1(tiles_uv1)
                .build(ui);

            ui.dummy([0.0, 6.0]);
            ui.text("BG Maps (0x9800 | 0x9C00)");
            ui.separator();
            let avail_w = ui.content_region_avail()[0];
            imgui::Image::new(bg_maps_texture_id, [avail_w, avail_w * BG_MAPS_ASPECT]).build(ui);

            ui.dummy([0.0, 6.0]);
            ui.text("OAM Sprites");
            ui.separator();
            let oam_lines = format_oam_entries(&ppu_snap.oam, is_cgb);
            for line in oam_lines.iter().take(10) {
                ui.text(line);
            }
            if oam_lines.len() > 10 {
                ui.text(format!("... {} more sprites", oam_lines.len() - 10));
            }

            ui.dummy([0.0, 6.0]);
            ui.text("Palettes");
            ui.separator();
            let palette_lines = format_palette_info(
                ppu_snap.bgp,
                ppu_snap.obp0,
                ppu_snap.obp1,
                &ppu_snap.bg_palette_ram,
                &ppu_snap.obj_palette_ram,
                is_cgb,
            );
            for line in palette_lines {
                ui.text(line);
            }
        });
}

impl Drop for GlBackend {
    fn drop(&mut self) {
        // Best-effort: make current and clean up GL resources.
        if self.render_target.make_current().is_ok() {
            self.egui_renderer.destroy();
            self.imgui_renderer.destroy(&self.glow_context);
            unsafe {
                gl::DeleteTextures(1, &self.ppu_viewer_nt_texture);
                gl::DeleteTextures(1, &self.ppu_viewer_tiles_texture);
                gl::DeleteTextures(1, &self.gb_ppu_viewer_tiles_texture);
                gl::DeleteTextures(1, &self.gb_ppu_viewer_bg_maps_texture);
            }
        }
    }
}

#[cfg(test)]
mod tests_notify_resize {
    use super::RenderTarget;

    /// Mock render target that records whether notify_resize was called and
    /// with which dimensions.
    struct TrackingMockRenderTarget {
        resize_called_with: Option<(u32, u32)>,
    }

    impl TrackingMockRenderTarget {
        fn new() -> Self {
            Self {
                resize_called_with: None,
            }
        }
    }

    impl RenderTarget for TrackingMockRenderTarget {
        fn notify_resize(&mut self, w: u32, h: u32) {
            self.resize_called_with = Some((w, h));
        }
        fn window_size(&self) -> (u32, u32) {
            (800, 600)
        }
        fn drawable_size(&self) -> (u32, u32) {
            (800, 600)
        }
        fn swap_buffers(&self) {}
        fn make_current(&self) -> Result<(), String> {
            Ok(())
        }
        fn set_fullscreen(&mut self, _enabled: bool) -> Result<(), String> {
            Ok(())
        }
        fn set_mouse_grab(&mut self, _enabled: bool) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_render_target_notify_resize_records_new_dimensions() {
        let mut mock = TrackingMockRenderTarget::new();
        mock.notify_resize(1920, 1080);
        assert_eq!(mock.resize_called_with, Some((1920, 1080)));
    }

    #[test]
    fn test_render_target_notify_resize_updates_on_each_call() {
        let mut mock = TrackingMockRenderTarget::new();
        mock.notify_resize(800, 600);
        mock.notify_resize(1920, 1080);
        assert_eq!(mock.resize_called_with, Some((1920, 1080)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_text_color_toggle() {
        assert_eq!(OverlayTextColor::White.toggle(), OverlayTextColor::Black);
        assert_eq!(OverlayTextColor::Black.toggle(), OverlayTextColor::White);
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

    #[test]
    fn test_shader_toast_message_with_name_shows_shader_name() {
        // cycle_shader returns the short preset name; the toast uppercases it.
        assert_eq!(
            super::shader_toast_message(Some("ntsc")),
            "Visual Filter: NTSC"
        );
    }

    #[test]
    fn test_shader_toast_message_with_none_shows_no_shader() {
        // When no shader is active (cycled to "no shader"), the toast says so.
        assert_eq!(super::shader_toast_message(None), "Visual Filter: off");
    }
}
