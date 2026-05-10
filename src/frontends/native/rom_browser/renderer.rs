//! OpenGL rendering backend for the ROM browser.
//!
//! Manages the GL context, imgui integration, and texture loading for
//! cover art images displayed in the ROM browser grid.

use std::collections::HashMap;
use std::ffi::c_void;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::SurfaceAttributesBuilder;
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// Browser GL renderer managing the window, GL context, and imgui.
pub struct BrowserGl {
    window: Arc<Window>,
    surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    imgui: imgui::Context,
    imgui_renderer: imgui_glow_renderer::Renderer,
    #[allow(dead_code)] // Kept alive for GL resource ownership
    glow_context: Arc<glow::Context>,
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
    pub gl_id: gl::types::GLuint,
    pub imgui_id: imgui::TextureId,
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
            gl::ClearColor(0.08, 0.08, 0.12, 1.0); // Dark theme background
        }

        // Create imgui context.
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);

        // Add default font at a comfortable reading size.
        let font_size = 20.0;
        imgui
            .fonts()
            .add_font(&[imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    size_pixels: font_size,
                    ..Default::default()
                }),
            }]);

        // Create glow context for imgui renderer.
        let glow_context = unsafe {
            let display_clone = gl_display.clone();
            Arc::new(glow::Context::from_loader_function(|s| {
                display_clone
                    .get_proc_address(std::ffi::CString::new(s).expect("valid CString").as_c_str())
                    .cast()
            }))
        };

        let imgui_renderer = imgui_glow_renderer::Renderer::new(
            &glow_context,
            &mut imgui,
            &mut imgui_glow_renderer::SimpleTextureMap::default(),
            true,
        )
        .map_err(|e| format!("Failed to initialise imgui renderer: {e:?}"))?;

        // Enable vsync for the browser (no need for precise frame timing).
        let _ = surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        );

        Ok(Self {
            window,
            surface,
            gl_context,
            imgui,
            imgui_renderer,
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

    /// Create a GL texture and upload RGBA pixels.
    ///
    /// # Safety
    /// Must be called with an active GL context.
    unsafe fn create_and_upload_texture(
        &self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> LoadedTexture {
        unsafe {
            let mut tex: gl::types::GLuint = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            // Use LINEAR filtering for cover art (smooth scaling).
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_ptr() as *const c_void,
            );

            let imgui_id: imgui::TextureId = (tex as usize).into();
            LoadedTexture {
                gl_id: tex,
                imgui_id,
                width,
                height,
            }
        }
    }

    /// Begin a new frame for rendering. Returns the imgui Ui handle.
    ///
    /// Call `end_frame()` after drawing to swap buffers.
    pub fn begin_frame(&mut self) -> &imgui::Ui {
        let now = Instant::now();
        let delta = now - self.last_frame;
        self.last_frame = now;

        let io = self.imgui.io_mut();
        io.delta_time = delta.as_secs_f32();

        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        io.display_size = [size.width as f32 / scale, size.height as f32 / scale];
        io.display_framebuffer_scale = [scale, scale];

        unsafe {
            gl::Viewport(0, 0, size.width as i32, size.height as i32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        self.imgui.new_frame()
    }

    /// End the current frame: render imgui draw data and swap buffers.
    pub fn end_frame(&mut self) {
        let draw_data = self.imgui.render();
        self.imgui_renderer
            .render(
                &self.glow_context,
                &imgui_glow_renderer::SimpleTextureMap::default(),
                draw_data,
            )
            .expect("imgui render failed");

        self.surface
            .swap_buffers(&self.gl_context)
            .expect("swap_buffers failed");
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
        // Clean up GL textures.
        for loaded in self.textures.values() {
            unsafe {
                gl::DeleteTextures(1, &loaded.gl_id);
            }
        }
    }
}
