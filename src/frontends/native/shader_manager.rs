use crate::platform::shaders::SHADER_PRESETS;
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
    output_texture_owned_by_manager: bool,
}

impl ShaderManager {
    pub fn new(allowed_names: &[&str]) -> Self {
        let available_presets = Self::discover_presets_filtered(allowed_names);
        let current_index = Self::initial_current_index(&available_presets);

        ShaderManager {
            filter_chain: None,
            current_preset: None,
            available_presets,
            current_index,
            frame_count: 0,
            output_texture: None,
            output_size: None,
            output_texture_owned_by_manager: true,
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

            if let Some(old) = self.output_texture.take()
                && self.output_texture_owned_by_manager
            {
                unsafe {
                    gl::DeleteTextures(1, &old);
                }
            }

            self.output_texture = Some(tex);
            self.output_size = Some(desired_size);
            self.output_texture_owned_by_manager = true;
        }

        Ok(self.output_texture.expect("output texture must be set"))
    }

    fn discover_presets_filtered(allowed_names: &[&str]) -> Vec<PathBuf> {
        // Like discover_presets but restricted to names allowed for the active emulator.
        Self::presets_for_names(allowed_names, SHADER_PRESETS)
            .into_iter()
            .filter(|p| p.exists())
            .collect()
    }

    /// Returns the paths from `all_presets` whose short names appear in `allowed_names`.
    ///
    /// This is the pure, disk-free subset of `discover_presets` used to restrict
    /// the cycling list to the shaders relevant to the active emulator.
    pub(crate) fn presets_for_names(
        allowed_names: &[&str],
        all_presets: &[(&str, &str)],
    ) -> Vec<PathBuf> {
        allowed_names
            .iter()
            .filter_map(|allowed| {
                all_presets
                    .iter()
                    .find(|(name, _)| name == allowed)
                    .map(|(_, path)| PathBuf::from(path))
            })
            .collect()
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

    /// Apply the loaded shader to transform `input_texture` into an internal
    /// output texture (retrievable via [`output_texture`](Self::output_texture)).
    ///
    /// `input_width` × `input_height` must match the actual texture dimensions
    /// (i.e. after overscan cropping). Shaders use these as `SourceSize` for
    /// pixel-level calculations (CRT scanlines, NTSC encoding, etc.), so
    /// passing incorrect dimensions causes visible artifacts (see bug #1851).
    pub fn apply_shader(
        &mut self,
        input_texture: gl::types::GLuint,
        input_width: u32,
        input_height: u32,
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

        let input_size = Size::new(input_width, input_height);

        // Create GLImage from input texture
        let image = GLImage {
            handle: Some(glow::NativeTexture(
                std::num::NonZero::new(input_texture).ok_or("Invalid texture ID")?,
            )),
            format: gl::RGB8,
            size: input_size,
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

    pub fn release_output_texture_ownership(&mut self, texture: gl::types::GLuint) -> bool {
        if self.output_texture == Some(texture) {
            self.output_texture_owned_by_manager = false;
            true
        } else {
            false
        }
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
        if let Some(tex) = self.output_texture.take()
            && self.output_texture_owned_by_manager
        {
            unsafe {
                gl::DeleteTextures(1, &tex);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::console::config::GBA_FILTER_NAMES;

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
                output_texture_owned_by_manager: true,
            }
        }

        fn with_output_texture(texture: gl::types::GLuint, owned_by_manager: bool) -> Self {
            let mut manager = ShaderManager::with_presets(Vec::new());
            manager.output_texture = Some(texture);
            manager.output_texture_owned_by_manager = owned_by_manager;
            manager
        }
    }

    #[test]
    fn test_cycle_starts_after_initially_loaded_preset() {
        // Presets are cycled in the same order they are discovered.
        let presets = vec![
            PathBuf::from("shaders/stock.slangp"),
            PathBuf::from("vendor/slang-shaders/crt/crt-lottes.slangp"),
            PathBuf::from(
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp",
            ),
            PathBuf::from("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp"),
        ];
        let mut mgr = ShaderManager::with_presets(presets);

        // Simulate startup loading "stock" (index 0)
        mgr.sync_current_index(Path::new("shaders/stock.slangp"));

        // Next F4 press must advance to crt (index 1)
        let next_index = (mgr.current_index + 1) % mgr.available_presets.len();
        assert_eq!(
            mgr.available_presets[next_index],
            PathBuf::from("vendor/slang-shaders/crt/crt-lottes.slangp")
        );
    }

    #[test]
    fn test_initial_cycle_without_loaded_shader_starts_after_stock() {
        // When no shader is configured at startup, cycling should behave as if
        // the current position is "stock" (the passthrough preset), so the first
        // F4 press goes to crt (the preset after stock in discovery order).
        let presets = vec![
            PathBuf::from("shaders/stock.slangp"), // 0
            PathBuf::from("vendor/slang-shaders/crt/crt-lottes.slangp"), // 1
            PathBuf::from(
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp",
            ), // 2
            PathBuf::from("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp"), // 3
        ];
        let mgr = ShaderManager::with_presets(presets); // no load_preset called

        let next_index = (mgr.current_index + 1) % mgr.available_presets.len();
        assert_eq!(
            mgr.available_presets[next_index],
            PathBuf::from("vendor/slang-shaders/crt/crt-lottes.slangp"),
            "first F4 from 'no shader' state should land on crt, not index {}",
            next_index
        );
    }

    #[test]
    fn release_output_texture_ownership_marks_current_texture_external() {
        let mut manager = ShaderManager::with_output_texture(7, true);

        assert!(manager.release_output_texture_ownership(7));
        assert!(!manager.output_texture_owned_by_manager);

        manager.output_texture = None;
    }

    #[test]
    fn release_output_texture_ownership_rejects_non_current_texture() {
        let mut manager = ShaderManager::with_output_texture(7, true);

        assert!(!manager.release_output_texture_ownership(8));
        assert!(manager.output_texture_owned_by_manager);

        manager.output_texture = None;
    }

    #[test]
    fn test_presets_for_names_returns_only_allowed_presets() {
        let all: &[(&str, &str)] = &[
            ("none", "shaders/stock.slangp"),
            ("crt", "vendor/slang-shaders/crt/crt-lottes.slangp"),
            (
                "smooth",
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp",
            ),
            (
                "ntsc",
                "vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp",
            ),
            (
                "pal",
                "vendor/slang-shaders/pal/decoupled-guest-advanced-pal 3-RF.slangp",
            ),
            ("dmg", "vendor/slang-shaders/handheld/gameboy.slangp"),
        ];

        let gb_allowed = &["none", "dmg"];
        let gb_presets = ShaderManager::presets_for_names(gb_allowed, all);
        assert_eq!(gb_presets.len(), 2, "GB should have exactly 2 presets");
        assert!(gb_presets.contains(&PathBuf::from("shaders/stock.slangp")));
        assert!(gb_presets.contains(&PathBuf::from(
            "vendor/slang-shaders/handheld/gameboy.slangp"
        )));
        assert!(
            !gb_presets
                .iter()
                .any(|p| p.to_str().unwrap_or("").contains("crt")),
            "GB must not include crt"
        );

        let nes_allowed = &["none", "crt", "smooth", "ntsc", "pal"];
        let nes_presets = ShaderManager::presets_for_names(nes_allowed, all);
        assert_eq!(nes_presets.len(), 5, "NES should have exactly 5 presets");
        assert!(
            !nes_presets
                .iter()
                .any(|p| p.to_str().unwrap_or("").contains("gameboy")),
            "NES must not include dmg"
        );
    }

    #[test]
    fn test_presets_for_names_preserves_allowed_names_order() {
        let all: &[(&str, &str)] = &[
            (
                "gba-lcd-grid",
                "vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp",
            ),
            (
                "sp101-color",
                "vendor/slang-shaders/handheld/color-mod/SP101-color.slangp",
            ),
            ("none", "shaders/stock.slangp"),
            (
                "nso-gba-color",
                "vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp",
            ),
            ("gba-lcd", "shaders/gba-lcd.slangp"),
            ("agb001", "vendor/slang-shaders/handheld/agb001.slangp"),
        ];

        let discovered = ShaderManager::presets_for_names(GBA_FILTER_NAMES, all);
        let expected = vec![
            PathBuf::from("shaders/stock.slangp"),
            PathBuf::from("shaders/gba-lcd.slangp"),
            PathBuf::from("vendor/slang-shaders/handheld/agb001.slangp"),
            PathBuf::from("vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp"),
            PathBuf::from("vendor/slang-shaders/handheld/color-mod/SP101-color.slangp"),
            PathBuf::from("vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp"),
        ];

        assert_eq!(discovered, expected);
    }
}
