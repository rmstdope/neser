//! Platform-facing SNES wrapper for the `Console` enum.
//!
//! `Snes` provides the platform interface for SNES emulation, implementing the
//! [`Emulator`] trait so frontends can drive it through [`Console`].
//!
//! [`Emulator`]: crate::platform::emulator::Emulator
//! [`Console`]: crate::platform::emulator::Console

use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::emulator::{Emulator, SystemType};
use crate::snes::bus::StubBus;
use std::path::PathBuf;
use std::time::Duration;

/// SNES display width in pixels (standard NTSC mode).
const SCREEN_WIDTH: u32 = 256;
/// SNES display height in pixels (standard NTSC mode).
const SCREEN_HEIGHT: u32 = 224;

/// SNES frame duration (~60.098 Hz refresh rate).
/// Master clock: 21.477272 MHz, 357366 cycles per frame → 16.639 ms per frame.
const FRAME_DURATION_NANOS: u64 = 16_639_000;

/// Super Nintendo Entertainment System emulator wrapper.
///
/// This struct wraps the SNES emulation core and provides the [`Emulator`] trait
/// implementation for platform integration.
pub struct Snes {
    app_context: SharedAppContext,
    _bus: StubBus,
    rom_path: Option<PathBuf>,
    ready_to_render: bool,
}

impl Snes {
    /// SNES display width in pixels.
    pub const SCREEN_WIDTH: u32 = SCREEN_WIDTH;
    /// SNES display height in pixels.
    pub const SCREEN_HEIGHT: u32 = SCREEN_HEIGHT;

    /// Create a new SNES emulator instance.
    pub fn new(app_context: impl IntoSharedAppContext) -> Self {
        Self {
            app_context: app_context.into_shared(),
            _bus: StubBus,
            rom_path: None,
            ready_to_render: false,
        }
    }

    /// Returns the file path where save-state data should be stored.
    pub fn state_path(&self) -> Option<PathBuf> {
        self.rom_path.as_ref().map(|path| {
            let mut state_path = path.clone();
            state_path.set_extension("state");
            state_path
        })
    }
}

impl Emulator for Snes {
    fn system_type(&self) -> SystemType {
        SystemType::Snes
    }

    fn allowed_shaders(&self) -> &'static [&'static str] {
        // TODO: Define SNES-specific shader presets
        &[]
    }

    fn load_rom(&mut self, _bytes: &[u8], name: &str) -> Result<(), String> {
        // TODO: Implement ROM loading
        self.rom_path = Some(PathBuf::from(name));
        Ok(())
    }

    fn run_tick(&mut self) -> u8 {
        if self.rom_path.is_none() {
            return 0;
        }

        // TODO: Implement CPU execution
        // Stub: signal a frame ready after every tick so the platform loop doesn't stall
        self.ready_to_render = true;
        1
    }

    fn is_ready_to_render(&self) -> bool {
        self.ready_to_render
    }

    fn clear_ready_to_render(&mut self) {
        self.ready_to_render = false;
    }

    fn screen_width(&self) -> u32 {
        SCREEN_WIDTH
    }

    fn screen_height(&self) -> u32 {
        SCREEN_HEIGHT
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        // TODO: Implement screen capture
        // Return black screen for now
        vec![0; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize]
    }

    fn cropped_screen_snapshot(&self, _h_overscan: u32, _v_overscan: u32) -> Vec<u8> {
        // SNES has no overscan, return full screen
        self.screen_snapshot()
    }

    fn screen_crc32(&self) -> u32 {
        let pixels = self.screen_snapshot();
        crate::platform::crc32::crc32(&[&pixels])
    }

    fn sample_ready(&self) -> bool {
        // TODO: Implement audio sample ready detection
        false
    }

    fn get_sample(&mut self) -> Option<f32> {
        // TODO: Implement audio sample retrieval
        None
    }

    fn set_audio_sample_rate(&mut self, _rate: f32) {
        // TODO: Implement audio sample rate configuration
    }

    fn set_button(&mut self, _port: u8, _button_id: u8, _pressed: bool) {
        // TODO: Implement button state setting
    }

    fn set_joypad_button_states(&mut self, _port: u8, _state: u8) {
        // TODO: Implement joypad state setting
    }

    fn get_joypad_button_states(&self, _port: u8) -> u8 {
        // TODO: Implement joypad state retrieval
        0
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        // TODO: Implement save state serialization
        Err("Save states not yet implemented for SNES".to_string())
    }

    fn load_state_bytes(&mut self, _data: &[u8]) -> Result<(), String> {
        // TODO: Implement save state deserialization
        Err("Save states not yet implemented for SNES".to_string())
    }

    fn reset(&mut self, _soft_reset: bool) {
        // TODO: Implement reset
    }

    fn save_ram(&self) -> Result<(), String> {
        // TODO: Implement battery-backed RAM saving
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
    use crate::platform::config::Config;

    fn make_snes() -> Snes {
        let app_context = AppContext::new_with_config(Config::default());
        Snes::new(app_context)
    }

    #[test]
    fn new_snes_constructs_successfully() {
        let _snes = make_snes();
    }

    #[test]
    fn system_type_returns_snes() {
        let snes = make_snes();
        assert_eq!(snes.system_type(), SystemType::Snes);
    }

    #[test]
    fn screen_dimensions_are_256x224() {
        let snes = make_snes();
        assert_eq!(snes.screen_width(), 256);
        assert_eq!(snes.screen_height(), 224);
    }

    #[test]
    fn target_frame_duration_is_approximately_60hz() {
        let snes = make_snes();
        let duration = snes.target_frame_duration();
        // ~60.098 Hz = ~16.639 ms per frame
        let expected_nanos = 16_639_000u64;
        assert_eq!(duration.as_nanos(), expected_nanos as u128);
    }

    #[test]
    fn state_path_returns_none_when_no_rom_loaded() {
        let snes = make_snes();
        assert_eq!(snes.state_path(), None);
    }

    #[test]
    fn state_path_returns_path_after_rom_loaded() {
        let mut snes = make_snes();
        snes.load_rom(&[], "test.sfc").unwrap();
        let state_path = snes.state_path();
        assert!(state_path.is_some());
        assert_eq!(state_path.unwrap().to_string_lossy(), "test.state");
    }

    #[test]
    fn run_tick_returns_nonzero_cycles() {
        let mut snes = make_snes();
        snes.load_rom(&[], "test.sfc").unwrap();
        let cycles = snes.run_tick();
        assert!(cycles > 0);
    }

    #[test]
    fn run_tick_returns_zero_when_no_rom_loaded() {
        let mut snes = make_snes();
        let cycles = snes.run_tick();
        assert_eq!(cycles, 0);
    }

    #[test]
    fn run_tick_sets_ready_to_render() {
        let mut snes = make_snes();
        snes.load_rom(&[], "test.sfc").unwrap();
        assert!(!snes.is_ready_to_render());
        snes.run_tick();
        assert!(snes.is_ready_to_render());
    }

    #[test]
    fn clear_ready_to_render_clears_flag() {
        let mut snes = make_snes();
        snes.load_rom(&[], "test.sfc").unwrap();
        snes.run_tick();
        assert!(snes.is_ready_to_render());
        snes.clear_ready_to_render();
        assert!(!snes.is_ready_to_render());
    }

    #[test]
    fn screen_crc32_matches_snapshot_crc() {
        let snes = make_snes();
        let pixels = snes.screen_snapshot();
        let expected = crate::platform::crc32::crc32(&[&pixels]);
        assert_eq!(snes.screen_crc32(), expected);
    }

    #[test]
    fn screen_snapshot_returns_correct_size_buffer() {
        let snes = make_snes();
        let snapshot = snes.screen_snapshot();
        assert_eq!(snapshot.len(), (256 * 224 * 3) as usize);
    }
}
