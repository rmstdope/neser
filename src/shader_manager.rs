use librashader::presets::ShaderPreset;
use librashader::runtime::gl::{FilterChain, FilterChainOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ShaderManager {
    filter_chain: Option<FilterChain>,
    current_preset: Option<String>,
    available_presets: Vec<PathBuf>,
    current_index: usize,
}

impl ShaderManager {
    pub fn new() -> Self {
        let available_presets = Self::discover_presets();
        
        ShaderManager {
            filter_chain: None,
            current_preset: None,
            available_presets,
            current_index: 0,
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

    pub fn apply_shader(
        &mut self,
        _input_texture: gl::types::GLuint,
        _output_framebuffer: gl::types::GLuint,
        _viewport_width: u32,
        _viewport_height: u32,
    ) -> Result<(), String> {
        // TODO: Implement shader application
        // For now, this is just a placeholder to get the code compiling
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
