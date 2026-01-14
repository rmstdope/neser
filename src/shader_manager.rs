use librashader::presets::ShaderPreset;
use librashader::presets::context::VideoDriver;
use librashader::runtime::gl::{FilterChain, FilterChainOptions, GLImage};
use librashader::runtime::{Size, Viewport};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ShaderManager {
    filter_chain: Option<FilterChain>,
    current_preset: Option<String>,
    available_presets: Vec<PathBuf>,
    current_index: usize,
    frame_count: usize,
    output_texture: Option<gl::types::GLuint>,
    output_size: Option<Size<u32>>,
}

impl ShaderManager {
    pub fn new() -> Self {
        let available_presets = Self::discover_presets();

        ShaderManager {
            filter_chain: None,
            current_preset: None,
            available_presets,
            current_index: 0,
            frame_count: 0,
            output_texture: None,
            output_size: None,
        }
    }

    fn ensure_output_texture(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<gl::types::GLuint, String> {
        if width == 0 || height == 0 {
            return Err("Invalid output size".to_string());
        }

        let desired_size = Size::new(width, height);
        let needs_realloc = self.output_size != Some(desired_size) || self.output_texture.is_none();

        if needs_realloc {
            let tex = unsafe {
                let mut tex: gl::types::GLuint = 0;
                gl::GenTextures(1, &mut tex);
                if tex == 0 {
                    return Err("Failed to create output texture".to_string());
                }

                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGB8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    std::ptr::null(),
                );

                tex
            };

            if let Some(old) = self.output_texture.take() {
                unsafe {
                    gl::DeleteTextures(1, &old);
                }
            }

            self.output_texture = Some(tex);
            self.output_size = Some(desired_size);
        }

        Ok(self.output_texture.expect("output texture must be set"))
    }

    fn discover_presets() -> Vec<PathBuf> {
        let mut presets = Vec::new();

        // Look for shaders in the shaders directory
        if let Ok(entries) = std::fs::read_dir("shaders") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("slangp") {
                    presets.push(path);
                }
            }
        }

        // Sort presets for consistent ordering
        presets.sort();
        presets
    }

    pub fn load_preset(
        &mut self,
        preset_path: &Path,
        gl_context: Arc<glow::Context>,
    ) -> Result<(), String> {
        // Load the shader preset
        let preset = ShaderPreset::try_parse_with_driver_context(preset_path, VideoDriver::GlCore)
            .map_err(|e| format!("Failed to parse shader preset: {}", e))?;

        // Create filter chain with OpenGL runtime
        let options = FilterChainOptions {
            glsl_version: 0, // Auto-detect
            use_dsa: false,  // Don't use direct state access for compatibility
            force_no_mipmaps: false,
            disable_cache: false,
        };

        let filter_chain = unsafe {
            FilterChain::load_from_preset(preset, gl_context, Some(&options))
                .map_err(|e| format!("Failed to load filter chain: {}", e))?
        };

        self.filter_chain = Some(filter_chain);
        self.current_preset = Some(preset_path.to_string_lossy().to_string());

        // Ensure we reset frame counter and output so size/animation doesn't carry across presets.
        self.frame_count = 0;

        Ok(())
    }

    /// Apply the loaded shader to transform input texture to output framebuffer.
    pub fn apply_shader(
        &mut self,
        input_texture: gl::types::GLuint,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), String> {
        if self.filter_chain.is_none() {
            // No shader loaded, nothing to do
            return Ok(());
        }

        let output_texture = self.ensure_output_texture(viewport_width, viewport_height)?;
        let filter_chain = self
            .filter_chain
            .as_mut()
            .expect("filter_chain must be present");

        // Create GLImage from input NES texture
        let image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(input_texture).ok_or("Invalid texture ID")?,
            )),
            format: gl::RGB8 as u32,
            size: Size::new(256, 240),
        };

        // Output image that will receive the final pass.
        let output_image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(output_texture).ok_or("Invalid output texture ID")?,
            )),
            format: gl::RGB8 as u32,
            size: Size::new(viewport_width, viewport_height),
        };

        // Create viewport for shader output
        // The viewport output is where the shader will render to
        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            size: Size::new(viewport_width, viewport_height),
            output: &output_image,
            mvp: None,
        };

        // Apply filter chain - this will render the filtered image into `output_texture`.
        unsafe {
            filter_chain
                .frame(
                    &image,
                    &viewport,
                    self.frame_count,
                    None, // options
                )
                .map_err(|e| format!("Failed to apply shader: {}", e))?;
        }

        // Increment frame count for animated shaders
        self.frame_count = self.frame_count.wrapping_add(1);

        Ok(())
    }

    pub fn output_texture(&self) -> Option<gl::types::GLuint> {
        self.output_texture
    }

    pub fn cycle_shader(&mut self, gl_context: Arc<glow::Context>) -> Result<(), String> {
        if self.available_presets.is_empty() {
            return Err("No shader presets available".to_string());
        }

        self.current_index = (self.current_index + 1) % self.available_presets.len();
        let preset_path = self.available_presets[self.current_index].clone();
        self.load_preset(&preset_path, gl_context)?;

        Ok(())
    }

    pub fn current_preset_name(&self) -> Option<&str> {
        self.current_preset.as_deref()
    }

    pub fn has_shader(&self) -> bool {
        self.filter_chain.is_some()
    }
}

impl Drop for ShaderManager {
    fn drop(&mut self) {
        if let Some(tex) = self.output_texture.take() {
            unsafe {
                gl::DeleteTextures(1, &tex);
            }
        }
    }
}
