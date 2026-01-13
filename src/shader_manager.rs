use librashader::presets::ShaderPreset;
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
        }
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

    pub fn load_preset(&mut self, preset_path: &Path, gl_context: Arc<glow::Context>) -> Result<(), String> {
        // Load the shader preset
        let preset = ShaderPreset::try_parse(preset_path)
            .map_err(|e| format!("Failed to parse shader preset: {}", e))?;

        // Create filter chain with OpenGL runtime
        let options = FilterChainOptions {
            glsl_version: 0, // Auto-detect
            use_dsa: false, // Don't use direct state access for compatibility
            force_no_mipmaps: false,
            disable_cache: false,
        };

        let filter_chain = unsafe {
            FilterChain::load_from_preset(preset, gl_context, Some(&options))
                .map_err(|e| format!("Failed to load filter chain: {}", e))?
        };

        self.filter_chain = Some(filter_chain);
        self.current_preset = Some(preset_path.to_string_lossy().to_string());

        Ok(())
    }

    /// Apply the loaded shader to transform input texture to output framebuffer.
    pub fn apply_shader(
        &mut self,
        input_texture: gl::types::GLuint,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), String> {
        let Some(ref mut filter_chain) = self.filter_chain else {
            // No shader loaded, nothing to do
            return Ok(());
        };

        // Create GLImage from input NES texture
        let image = GLImage {
            handle: Some(glow::NativeTexture(std::num::NonZero::new(input_texture).ok_or("Invalid texture ID")?)),
            format: gl::RGB8 as u32,
            size: Size::new(256, 240),
        };

        // Create viewport for shader output
        // The viewport output is where the shader will render to
        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            size: Size::new(viewport_width, viewport_height),
            output: &image, // The viewport's output type parameter needs the image reference
            mvp: None,
        };

        // Apply filter chain - this will render the filtered image to the current framebuffer
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
