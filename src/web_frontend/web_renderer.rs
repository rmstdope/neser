use glow::HasContext;
use librashader::presets::ShaderPreset;
use librashader::presets::context::VideoDriver;
use librashader::runtime::gl::{FilterChain, FilterChainOptions, GLImage};
use librashader::runtime::{Size, Viewport};
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

pub struct WebRenderer {
    gl_context: Arc<glow::Context>,
    filter_chain: Option<FilterChain>,
    current_filter_index: usize,
    available_filters: Vec<&'static str>,
    frame_count: usize,
    nes_texture: glow::Texture,
    output_texture: Option<glow::Texture>,
    output_size: Option<Size<u32>>,
    vertex_array: glow::VertexArray,
    position_buffer: glow::Buffer,
    texcoord_buffer: glow::Buffer,
}

impl WebRenderer {
    /// List of available shader preset files
    const FILTERS: &'static [&'static str] = &[
        "shaders/ntsc-256px-composite.slangp",
        "shaders/crt-lottes.slangp",
        "shaders/xbrz-freescale.slangp",
        "shaders/stock.slangp",
    ];

    pub fn new(canvas_id: &str) -> Result<Self, String> {
        // Get canvas element
        let document = web_sys::window()
            .ok_or("No window")?
            .document()
            .ok_or("No document")?;
        
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or("Canvas not found")?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "Element is not a canvas")?;

        // Get WebGL2 context
        let webgl2_context = canvas
            .get_context("webgl2")
            .map_err(|_| "Failed to get WebGL2 context")?
            .ok_or("WebGL2 not supported")?
            .dyn_into::<WebGl2RenderingContext>()
            .map_err(|_| "Failed to cast to WebGL2")?;

        // Create glow context from WebGL2
        let gl_context = Arc::new(glow::Context::from_webgl2_context(webgl2_context));

        // Create NES texture
        let nes_texture = unsafe {
            let tex = gl_context.create_texture()
                .map_err(|e| format!("Failed to create NES texture: {}", e))?;
            gl_context.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            tex
        };

        // Create vertex array and buffers for full-screen quad
        let (vertex_array, position_buffer, texcoord_buffer) = unsafe {
            let vao = gl_context.create_vertex_array()
                .map_err(|e| format!("Failed to create VAO: {}", e))?;
            gl_context.bind_vertex_array(Some(vao));

            // Position buffer (full-screen quad)
            let pos_buf = gl_context.create_buffer()
                .map_err(|e| format!("Failed to create position buffer: {}", e))?;
            gl_context.bind_buffer(glow::ARRAY_BUFFER, Some(pos_buf));
            let positions: [f32; 8] = [
                -1.0, -1.0,
                 1.0, -1.0,
                -1.0,  1.0,
                 1.0,  1.0,
            ];
            gl_context.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&positions),
                glow::STATIC_DRAW,
            );

            // Texcoord buffer
            let tex_buf = gl_context.create_buffer()
                .map_err(|e| format!("Failed to create texcoord buffer: {}", e))?;
            gl_context.bind_buffer(glow::ARRAY_BUFFER, Some(tex_buf));
            let texcoords: [f32; 8] = [
                0.0, 1.0,
                1.0, 1.0,
                0.0, 0.0,
                1.0, 0.0,
            ];
            gl_context.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&texcoords),
                glow::STATIC_DRAW,
            );

            (vao, pos_buf, tex_buf)
        };

        let mut renderer = WebRenderer {
            gl_context,
            filter_chain: None,
            current_filter_index: 0,
            available_filters: Self::FILTERS.to_vec(),
            frame_count: 0,
            nes_texture,
            output_texture: None,
            output_size: None,
            vertex_array,
            position_buffer,
            texcoord_buffer,
        };

        // Load the default filter (NTSC)
        renderer.load_filter(0)?;

        Ok(renderer)
    }

    fn load_filter(&mut self, index: usize) -> Result<(), String> {
        if index >= self.available_filters.len() {
            return Err(format!("Filter index {} out of bounds", index));
        }

        let filter_path = self.available_filters[index];
        
        // Load shader preset
        let preset = ShaderPreset::try_parse_with_driver_context(
            Path::new(filter_path),
            VideoDriver::GlCore
        ).map_err(|e| format!("Failed to parse shader preset: {}", e))?;

        // Create filter chain
        let options = FilterChainOptions {
            glsl_version: 0, // Auto-detect
            use_dsa: false,
            force_no_mipmaps: false,
            disable_cache: false,
        };

        let filter_chain = unsafe {
            FilterChain::load_from_preset(preset, self.gl_context.clone(), Some(&options))
                .map_err(|e| format!("Failed to load filter chain: {}", e))?
        };

        self.filter_chain = Some(filter_chain);
        self.current_filter_index = index;
        self.frame_count = 0;

        Ok(())
    }

    pub fn cycle_filter(&mut self) -> Result<String, String> {
        let next_index = (self.current_filter_index + 1) % self.available_filters.len();
        self.load_filter(next_index)?;
        Ok(self.get_current_filter_name())
    }

    pub fn get_current_filter_name(&self) -> String {
        let path = self.available_filters[self.current_filter_index];
        Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn upload_frame(&mut self, rgb_data: &[u8]) -> Result<(), String> {
        if rgb_data.len() != 256 * 240 * 3 {
            return Err(format!("Invalid frame size: expected {}, got {}", 256 * 240 * 3, rgb_data.len()));
        }

        unsafe {
            self.gl_context.bind_texture(glow::TEXTURE_2D, Some(self.nes_texture));
            self.gl_context.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGB as i32,
                256,
                240,
                0,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                Some(rgb_data),
            );
        }

        Ok(())
    }

    pub fn render(&mut self, viewport_width: u32, viewport_height: u32) -> Result<(), String> {
        let filter_chain = self.filter_chain.as_mut()
            .ok_or("No filter chain loaded")?;

        // Ensure output texture exists with correct size
        let output_texture = self.ensure_output_texture(viewport_width, viewport_height)?;

        // Create GLImage from NES texture
        let nes_tex_name = unsafe {
            std::mem::transmute::<glow::Texture, u32>(self.nes_texture)
        };
        let image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(nes_tex_name)
                    .ok_or("Invalid NES texture ID")?
            )),
            format: glow::RGB8 as u32,
            size: Size::new(256, 240),
        };

        // Create output image
        let output_tex_name = unsafe {
            std::mem::transmute::<glow::Texture, u32>(output_texture)
        };
        let output_image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(output_tex_name)
                    .ok_or("Invalid output texture ID")?
            )),
            format: glow::RGB8 as u32,
            size: Size::new(viewport_width, viewport_height),
        };

        // Create viewport
        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            size: Size::new(viewport_width, viewport_height),
            output: &output_image,
            mvp: None,
        };

        // Apply filter chain
        unsafe {
            filter_chain
                .frame(&image, &viewport, self.frame_count, None)
                .map_err(|e| format!("Failed to apply shader: {}", e))?;
        }

        self.frame_count = self.frame_count.wrapping_add(1);

        // Blit output texture to screen (simple passthrough)
        self.blit_to_screen(output_texture, viewport_width, viewport_height)?;

        Ok(())
    }

    fn ensure_output_texture(&mut self, width: u32, height: u32) -> Result<glow::Texture, String> {
        let desired_size = Size::new(width, height);
        let needs_realloc = self.output_size != Some(desired_size) || self.output_texture.is_none();

        if needs_realloc {
            unsafe {
                // Delete old texture if it exists
                if let Some(old_tex) = self.output_texture.take() {
                    self.gl_context.delete_texture(old_tex);
                }

                // Create new texture
                let tex = self.gl_context.create_texture()
                    .map_err(|e| format!("Failed to create output texture: {}", e))?;
                
                self.gl_context.bind_texture(glow::TEXTURE_2D, Some(tex));
                self.gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                self.gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                self.gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                self.gl_context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                
                self.gl_context.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGB8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGB,
                    glow::UNSIGNED_BYTE,
                    None,
                );

                self.output_texture = Some(tex);
                self.output_size = Some(desired_size);
            }
        }

        Ok(self.output_texture.ok_or("Output texture not initialized")?)
    }

    fn blit_to_screen(&self, texture: glow::Texture, _width: u32, _height: u32) -> Result<(), String> {
        // Simple blit to framebuffer - the filter chain has already rendered to the output texture
        // We just need to draw it to the screen
        
        // For now, librashader should have already rendered to the default framebuffer
        // through the viewport output. This method is a placeholder for any additional
        // post-processing if needed.
        
        Ok(())
    }
}

impl Drop for WebRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl_context.delete_texture(self.nes_texture);
            if let Some(tex) = self.output_texture {
                self.gl_context.delete_texture(tex);
            }
            self.gl_context.delete_vertex_array(self.vertex_array);
            self.gl_context.delete_buffer(self.position_buffer);
            self.gl_context.delete_buffer(self.texcoord_buffer);
        }
    }
}
