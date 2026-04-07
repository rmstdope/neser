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
        let current_index = Self::initial_current_index(&available_presets);

        ShaderManager {
            filter_chain: None,
            current_preset: None,
            available_presets,
            current_index,
            frame_count: 0,
            output_texture: None,
            output_size: None,
        }
    }

    /// Returns the index of the stock (passthrough) preset in `presets`, or 0
    /// if not found. This ensures cycling begins immediately after stock when
    /// no preset has been loaded at startup.
    fn initial_current_index(presets: &[PathBuf]) -> usize {
        presets
            .iter()
            .position(|p| p.file_name().and_then(|n| n.to_str()) == Some("stock.slangp"))
            .unwrap_or(0)
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
        self.sync_current_index(preset_path);

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
            format: gl::RGB8,
            size: Size::new(256, 240),
        };

        // Output image that will receive the final pass.
        let output_image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(output_texture).ok_or("Invalid output texture ID")?,
            )),
            format: gl::RGB8,
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

    fn sync_current_index(&mut self, preset_path: &Path) {
        if let Some(idx) = self.available_presets.iter().position(|p| p == preset_path) {
            self.current_index = idx;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    impl ShaderManager {
        fn with_presets(presets: Vec<PathBuf>) -> Self {
            let current_index = ShaderManager::initial_current_index(&presets);
            ShaderManager {
                filter_chain: None,
                current_preset: None,
                available_presets: presets,
                current_index,
                frame_count: 0,
                output_texture: None,
                output_size: None,
            }
        }
    }

    #[test]
    fn test_cycle_starts_after_initially_loaded_preset() {
        // Sorted alphabetical order matches discover_presets: crt(0) ntsc(1) stock(2) xbrz(3)
        let presets = vec![
            PathBuf::from("shaders/crt-lottes.slangp"),
            PathBuf::from("shaders/ntsc-256px-composite.slangp"),
            PathBuf::from("shaders/stock.slangp"),
            PathBuf::from("shaders/xbrz-freescale.slangp"),
        ];
        let mut mgr = ShaderManager::with_presets(presets);

        // Simulate startup loading "stock" (index 2)
        mgr.sync_current_index(Path::new("shaders/stock.slangp"));

        // Next F4 press must advance to xbrz (index 3), not ntsc (index 1)
        let next_index = (mgr.current_index + 1) % mgr.available_presets.len();
        assert_eq!(
            mgr.available_presets[next_index],
            PathBuf::from("shaders/xbrz-freescale.slangp")
        );
    }

    #[test]
    fn test_initial_cycle_without_loaded_shader_starts_after_stock() {
        // When no shader is configured at startup, cycling should behave as if
        // the current position is "stock" (the passthrough preset), so the first
        // F4 press goes to xbrz (the preset after stock in sorted order).
        let presets = vec![
            PathBuf::from("shaders/crt-lottes.slangp"),           // 0
            PathBuf::from("shaders/ntsc-256px-composite.slangp"), // 1
            PathBuf::from("shaders/stock.slangp"),                // 2
            PathBuf::from("shaders/xbrz-freescale.slangp"),       // 3
        ];
        let mgr = ShaderManager::with_presets(presets); // no load_preset called

        let next_index = (mgr.current_index + 1) % mgr.available_presets.len();
        assert_eq!(
            mgr.available_presets[next_index],
            PathBuf::from("shaders/xbrz-freescale.slangp"),
            "first F4 from 'no shader' state should land on xbrz, not index {}",
            next_index
        );
    }
}
