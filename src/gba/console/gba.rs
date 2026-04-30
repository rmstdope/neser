//! Platform-facing Game Boy Advance wrapper for the `Console` enum.
//!
//! `Gba` provides the platform interface for GBA emulation, implementing the
//! [`Emulator`] trait so frontends can drive it through [`Console`].
//!
//! This is currently a stub implementation — actual CPU, PPU, APU, and memory
//! subsystems will be added in subsequent phases.
//!
//! [`Emulator`]: crate::platform::emulator::Emulator
//! [`Console`]: crate::platform::emulator::Console

use crate::gba::apu::Apu;
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::emulator::{Emulator, SystemType};
use std::time::Duration;

/// GBA display width in pixels.
const SCREEN_WIDTH: u32 = 240;
/// GBA display height in pixels.
const SCREEN_HEIGHT: u32 = 160;

/// GBA frame duration (~59.7275 Hz refresh rate).
/// CPU clock: 16.78 MHz, 280896 cycles per frame → 16.743 ms per frame.
const FRAME_DURATION_NANOS: u64 = 16_743_000;

/// Shader presets allowed for GBA.
const ALLOWED_SHADERS: &[&str] = &["none", "gba-lcd"];

/// Game Boy Advance emulator wrapper.
///
/// This struct wraps the GBA emulation core and provides the [`Emulator`] trait
/// implementation for platform integration. Currently implemented as a stub
/// that returns appropriate defaults without actual emulation.
pub struct Gba {
    app_context: SharedAppContext,
    /// Audio Processing Unit — produces mixed f32 samples at the configured rate.
    apu: Apu,
}

impl Gba {
    /// GBA display width in pixels.
    pub const SCREEN_WIDTH: u32 = SCREEN_WIDTH;
    /// GBA display height in pixels.
    pub const SCREEN_HEIGHT: u32 = SCREEN_HEIGHT;

    /// Create a new GBA emulator instance.
    pub fn new(app_context: impl IntoSharedAppContext) -> Self {
        Self {
            app_context: app_context.into_shared(),
            apu: Apu::new(),
        }
    }
}

impl Emulator for Gba {
    fn system_type(&self) -> SystemType {
        SystemType::Gba
    }

    fn allowed_shaders(&self) -> &'static [&'static str] {
        ALLOWED_SHADERS
    }

    fn load_rom(&mut self, _bytes: &[u8], _name: &str) -> Result<(), String> {
        Err("GBA emulation not yet implemented".to_string())
    }

    fn run_tick(&mut self) -> u8 {
        0
    }

    fn is_ready_to_render(&self) -> bool {
        false
    }

    fn clear_ready_to_render(&mut self) {
        // No-op: no rendering implemented yet
    }

    fn screen_width(&self) -> u32 {
        SCREEN_WIDTH
    }

    fn screen_height(&self) -> u32 {
        SCREEN_HEIGHT
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        // Return a blank RGB888 buffer (240×160×3 = 115200 bytes)
        vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize]
    }

    fn cropped_screen_snapshot(&self, _h_overscan: u32, _v_overscan: u32) -> Vec<u8> {
        // GBA has no overscan, return full screen
        self.screen_snapshot()
    }

    fn screen_crc32(&self) -> u32 {
        0
    }

    fn sample_ready(&self) -> bool {
        self.apu.sample_ready()
    }

    fn get_sample(&mut self) -> Option<f32> {
        self.apu.take_sample()
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        self.apu.set_sample_rate(rate);
    }

    fn set_button(&mut self, _port: u8, _button_id: u8, _pressed: bool) {
        // No-op: no input implemented yet
    }

    fn set_joypad_button_states(&mut self, _port: u8, _state: u8) {
        // No-op: no input implemented yet
    }

    fn get_joypad_button_states(&self, _port: u8) -> u8 {
        0
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        Err("GBA save states not yet implemented".to_string())
    }

    fn load_state_bytes(&mut self, _data: &[u8]) -> Result<(), String> {
        Err("GBA save states not yet implemented".to_string())
    }

    fn reset(&mut self, _soft_reset: bool) {
        // No-op: no state to reset yet
    }

    fn save_ram(&self) -> Result<(), String> {
        // No cartridge RAM to save yet
        Ok(())
    }

    fn app_context(&self) -> &SharedAppContext {
        &self.app_context
    }

    fn target_frame_duration(&self) -> Duration {
        Duration::from_nanos(FRAME_DURATION_NANOS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::AppContext;

    fn make_gba() -> Gba {
        Gba::new(AppContext::default())
    }

    #[test]
    fn test_system_type() {
        let gba = make_gba();
        assert_eq!(gba.system_type(), SystemType::Gba);
    }

    #[test]
    fn test_screen_dimensions() {
        let gba = make_gba();
        assert_eq!(gba.screen_width(), 240);
        assert_eq!(gba.screen_height(), 160);
    }

    #[test]
    fn test_screen_snapshot_size() {
        let gba = make_gba();
        let snapshot = gba.screen_snapshot();
        // 240 × 160 × 3 (RGB888)
        assert_eq!(snapshot.len(), 115200);
    }

    #[test]
    fn test_allowed_shaders() {
        let gba = make_gba();
        let shaders = gba.allowed_shaders();
        assert!(shaders.contains(&"none"));
        assert!(shaders.contains(&"gba-lcd"));
    }

    #[test]
    fn test_load_rom_returns_error() {
        let mut gba = make_gba();
        let result = gba.load_rom(&[0u8; 256], "test.gba");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not yet implemented"));
    }

    #[test]
    fn test_target_frame_duration() {
        let gba = make_gba();
        let duration = gba.target_frame_duration();
        // Should be approximately 16.743ms (59.73 Hz)
        assert!(duration.as_millis() >= 16 && duration.as_millis() <= 17);
    }

    #[test]
    fn test_save_state_returns_error() {
        let gba = make_gba();
        let result = gba.save_state_bytes();
        assert!(result.is_err());
    }

    #[test]
    fn test_load_state_returns_error() {
        let mut gba = make_gba();
        let result = gba.load_state_bytes(&[]);
        assert!(result.is_err());
    }
}
