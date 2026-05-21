//! OpenGL rendering backend for the ROM browser.
//!
//! Manages the GL context, egui integration, and texture loading for
//! cover art images displayed in the ROM browser grid.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui_glow::EguiGlow;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::SurfaceAttributesBuilder;
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// Browser GL renderer managing the window, GL context, and egui.
pub struct BrowserGl {
    window: Arc<Window>,
    surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    pub egui_glow: EguiGlow,
    #[allow(dead_code)]
    glow_context: Arc<egui_glow::glow::Context>,
    last_frame: Instant,
    textures: HashMap<TextureKey, LoadedTexture>,
}

/// Key for identifying loaded textures in the cache.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TextureKey {
    /// Cover art for a game, keyed by game ID.
    CoverArt(i64),
    /// Screenshot for a game, keyed by (game_id, index).
    Screenshot(i64, usize),
    /// The placeholder texture for games without cover art.
    Placeholder,
}

/// A loaded GL texture with its dimensions.
#[derive(Debug, Clone, Copy)]
pub struct LoadedTexture {
    pub egui_id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

impl BrowserGl {
    /// Create the browser GL window and rendering context.
    pub fn new(
        event_loop: &ActiveEventLoop,
        width: u32,
        height: u32,
        fullscreen: bool,
    ) -> Result<Self, String> {
        let mut window_attrs = WindowAttributes::default()
            .with_title("NESER - ROM Browser")
            .with_inner_size(LogicalSize::new(width, height))
            .with_resizable(true);

        if fullscreen {
            window_attrs =
                window_attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        let config_template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(0);

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

        let (window, gl_config) = display_builder
            .build(event_loop, config_template, |configs| {
                configs
                    .reduce(|accum, config| {
                        if config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .expect("no compatible GL configurations found")
            })
            .map_err(|e| format!("failed to build display: {e}"))?;

        let window = Arc::new(window.ok_or("failed to create browser window")?);
        let raw_window_handle = window
            .window_handle()
            .map_err(|e| format!("failed to get window handle: {e}"))?
            .as_raw();

        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 2))))
            .build(Some(raw_window_handle));

        let not_current_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .map_err(|e| format!("failed to create GL context: {e}"))?
        };

        let size = window.inner_size();
        let surface_attrs = SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
            .build(
                raw_window_handle,
                NonZeroU32::new(size.width.max(1)).expect("non-zero width"),
                NonZeroU32::new(size.height.max(1)).expect("non-zero height"),
            );

        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .map_err(|e| format!("failed to create window surface: {e}"))?
        };

        let gl_context = not_current_context
            .make_current(&surface)
            .map_err(|e| format!("failed to make GL context current: {e}"))?;

        // Load GL function pointers.
        gl::load_with(|s| {
            gl_display
                .get_proc_address(std::ffi::CString::new(s).expect("valid CString").as_c_str())
                as *const _
        });

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Viewport(0, 0, size.width as i32, size.height as i32);
            gl::ClearColor(0.08, 0.08, 0.12, 1.0);
        }

        // Create glow context for egui_glow.
        let glow_context = Arc::new(unsafe {
            egui_glow::glow::Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).expect("valid CString");
                gl_display.get_proc_address(s.as_c_str()).cast()
            })
        });

        let egui_glow = EguiGlow::new(event_loop, glow_context.clone(), None, None, true);

        egui_glow
            .egui_ctx
            .set_fonts(crate::frontends::native::egui_theme::native_font_definitions());
        egui_glow
            .egui_ctx
            .set_visuals(crate::frontends::native::egui_theme::native_dark_visuals());

        // Enable vsync.
        let _ = surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        );

        Ok(Self {
            window,
            surface,
            gl_context,
            egui_glow,
            glow_context,
            last_frame: Instant::now(),
            textures: HashMap::new(),
        })
    }

    /// Get a reference to the window.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Handle a window resize event.
    pub fn notify_resize(&mut self, width: u32, height: u32) {
        self.surface.resize(
            &self.gl_context,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );
    }

    /// Forward a winit window event to egui for processing.
    pub fn on_window_event(&mut self, event: &WindowEvent) -> egui_winit::EventResponse {
        self.egui_glow.on_window_event(&self.window, event)
    }

    /// Run one UI frame. `build_ui` receives a `&mut egui::Ui` covering the full window.
    pub fn run_frame<F>(&mut self, build_ui: F)
    where
        F: FnMut(&mut egui::Ui),
    {
        let size = self.window.inner_size();
        unsafe {
            gl::Viewport(0, 0, size.width as i32, size.height as i32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        self.egui_glow.run(&self.window, build_ui);
        self.egui_glow.paint(&self.window);
        self.surface
            .swap_buffers(&self.gl_context)
            .expect("swap_buffers failed");
        self.last_frame = Instant::now();
    }

    /// Load an RGBA image from a file path and create a GL texture.
    ///
    /// Returns the loaded texture info, or `None` if loading fails.
    pub fn load_texture_from_file(
        &mut self,
        key: TextureKey,
        path: &Path,
    ) -> Option<LoadedTexture> {
        if let Some(existing) = self.textures.get(&key) {
            return Some(*existing);
        }

        let img = image::open(path).ok()?.into_rgba8();
        let (w, h) = img.dimensions();
        let pixels = img.into_raw();

        let loaded = unsafe { self.create_and_upload_texture(w, h, &pixels) };
        self.textures.insert(key, loaded);
        Some(loaded)
    }

    /// Create a GL texture from raw RGBA pixel data.
    pub fn load_texture_from_rgba(
        &mut self,
        key: TextureKey,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> LoadedTexture {
        if let Some(existing) = self.textures.get(&key) {
            return *existing;
        }

        let loaded = unsafe { self.create_and_upload_texture(width, height, pixels) };
        self.textures.insert(key, loaded);
        loaded
    }

    /// Look up an already-loaded texture.
    pub fn get_texture(&self, key: &TextureKey) -> Option<&LoadedTexture> {
        self.textures.get(key)
    }

    /// Remove a texture from the cache and free its GL resources.
    pub fn remove_texture(&mut self, key: &TextureKey) {
        if let Some(loaded) = self.textures.remove(key) {
            self.egui_glow.painter.free_texture(loaded.egui_id);
        }
    }

    /// Number of currently loaded textures.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Get all currently loaded texture keys (for eviction decisions).
    pub fn texture_keys(&self) -> Vec<TextureKey> {
        self.textures.keys().cloned().collect()
    }

    /// Create a GL texture and upload RGBA pixels, registering it with egui's painter.
    ///
    /// Uses the glow API so the texture is tracked by `egui_glow::Painter` and
    /// referenced by a proper `egui::TextureId` (not a raw GL handle).
    ///
    /// # Safety
    /// Must be called with an active GL context.
    unsafe fn create_and_upload_texture(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> LoadedTexture {
        use egui_glow::glow::HasContext as _;
        let gl = &self.glow_context;
        let tex = unsafe { gl.create_texture().expect("GL texture creation failed") };
        unsafe {
            gl.bind_texture(egui_glow::glow::TEXTURE_2D, Some(tex));
            gl.tex_parameter_i32(
                egui_glow::glow::TEXTURE_2D,
                egui_glow::glow::TEXTURE_MIN_FILTER,
                egui_glow::glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                egui_glow::glow::TEXTURE_2D,
                egui_glow::glow::TEXTURE_MAG_FILTER,
                egui_glow::glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                egui_glow::glow::TEXTURE_2D,
                egui_glow::glow::TEXTURE_WRAP_S,
                egui_glow::glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                egui_glow::glow::TEXTURE_2D,
                egui_glow::glow::TEXTURE_WRAP_T,
                egui_glow::glow::CLAMP_TO_EDGE as i32,
            );
            gl.pixel_store_i32(egui_glow::glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                egui_glow::glow::TEXTURE_2D,
                0,
                egui_glow::glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                egui_glow::glow::RGBA,
                egui_glow::glow::UNSIGNED_BYTE,
                egui_glow::glow::PixelUnpackData::Slice(Some(pixels)),
            );
        }
        // Register with egui's painter to get a proper TextureId.
        // This is required: TextureId::User is an index into the painter's registry,
        // not a raw GL texture name.
        let egui_id = self.egui_glow.painter.register_native_texture(tex);
        LoadedTexture {
            egui_id,
            width,
            height,
        }
    }

    /// Get the current drawable (physical pixel) dimensions.
    pub fn drawable_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    /// Get the current logical window dimensions.
    pub fn logical_size(&self) -> (f32, f32) {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        (size.width as f32 / scale, size.height as f32 / scale)
    }

    /// Get the time delta since the last frame in seconds.
    pub fn delta_time(&self) -> f32 {
        self.last_frame.elapsed().as_secs_f32()
    }
}

impl Drop for BrowserGl {
    fn drop(&mut self) {
        // egui_glow.destroy() frees all registered textures (including ours) via glow.
        self.egui_glow.destroy();
    }
}
