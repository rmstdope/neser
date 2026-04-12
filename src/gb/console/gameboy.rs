//! Platform-facing Game Boy wrapper for the `Console` enum.
//!
//! `GameBoy` owns a [`Gb<DmgBus>`] (created lazily on [`load_rom`]) and a
//! [`SharedAppContext`], providing the same interface that the NES side
//! exposes so that frontends can drive both systems through `Console`.

use crate::gb::bus::DmgBus;
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};

/// Platform-facing Game Boy (DMG) console wrapper.
pub struct GameBoy {
    gb: Option<Gb<DmgBus>>,
    app_context: SharedAppContext,
}

impl GameBoy {
    /// Screen width in pixels.
    pub const SCREEN_WIDTH: u32 = 160;
    /// Screen height in pixels.
    pub const SCREEN_HEIGHT: u32 = 144;

    /// Create a new Game Boy instance (no ROM loaded yet).
    pub fn new(app_context: impl IntoSharedAppContext) -> Self {
        Self {
            gb: None,
            app_context: app_context.into_shared(),
        }
    }

    /// Load a `.gb` ROM image. Replaces any previously loaded ROM.
    pub fn load_rom(&mut self, bytes: &[u8], _name: &str) -> Result<(), String> {
        let cart = load_cartridge(bytes).map_err(|e| format!("{e:?}"))?;
        self.gb = Some(Gb::new(DmgBus::new(cart)));
        Ok(())
    }

    /// Advance one CPU instruction. Returns the number of M-cycles consumed.
    pub fn run_tick(&mut self) -> u8 {
        match &mut self.gb {
            Some(gb) => gb.step(),
            None => 0,
        }
    }

    /// Returns `true` when the PPU has completed a full frame.
    pub fn is_frame_ready(&self) -> bool {
        self.gb.as_ref().is_some_and(|gb| gb.is_frame_ready())
    }

    /// Clear the frame-ready flag after the frontend has consumed the frame.
    pub fn clear_frame_ready(&mut self) {
        if let Some(gb) = &mut self.gb {
            gb.clear_frame_ready();
        }
    }

    /// Snapshot the current frame as a 160×144 RGB888 byte vector.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.gb.as_ref().map_or_else(
            || vec![0u8; (Self::SCREEN_WIDTH * Self::SCREEN_HEIGHT * 3) as usize],
            |gb| gb.screen_snapshot(),
        )
    }

    /// CRC32 of the current screen buffer.
    pub fn screen_crc32(&self) -> u32 {
        self.gb.as_ref().map_or(0, |gb| gb.screen_crc32())
    }

    /// Snapshot with no overscan (Game Boy has no overscan).
    pub fn cropped_screen_snapshot(&self) -> Vec<u8> {
        self.screen_snapshot()
    }

    /// Set a single button state.
    ///
    /// Uses NES-convention IDs: A=0, B=1, Select=2, Start=3, Up=4, Down=5,
    /// Left=6, Right=7.
    pub fn set_button(&mut self, id: u8, pressed: bool) {
        if let Some(gb) = &mut self.gb {
            gb.cpu.bus.set_joypad_button(id, pressed);
        }
    }

    /// Set all button states from a NES-convention bitmask.
    pub fn set_joypad_button_states(&mut self, state: u8) {
        for id in 0u8..8 {
            let pressed = state & (1 << id) != 0;
            self.set_button(id, pressed);
        }
    }

    /// Get all button states as a NES-convention bitmask.
    pub fn get_joypad_button_states(&self) -> u8 {
        self.gb
            .as_ref()
            .map_or(0, |gb| gb.cpu.bus.joypad.get_states())
    }

    /// Serialize emulator state (not supported for MVP — returns `Err`).
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        Err("Game Boy save states are not supported in MVP".into())
    }

    /// Restore emulator state (not supported for MVP — returns `Err`).
    pub fn load_state_bytes(&mut self, _data: &[u8]) -> Result<(), String> {
        Err("Game Boy save states are not supported in MVP".into())
    }

    /// Reset the console.
    ///
    /// - `soft_reset = true`: CPU registers only.
    /// - `soft_reset = false`: CPU registers + full bus state.
    pub fn reset(&mut self, soft_reset: bool) {
        if let Some(gb) = &mut self.gb {
            gb.reset(soft_reset);
        }
    }

    /// Returns `true` when the APU has a sample ready to retrieve.
    pub fn sample_ready(&self) -> bool {
        self.gb.as_ref().is_some_and(|gb| gb.cpu.bus.sample_ready())
    }

    /// Retrieve the next APU audio sample, or `None` if not ready.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.gb.as_mut().and_then(|gb| gb.cpu.bus.take_sample())
    }

    /// Set the APU output sample rate in Hz.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        if let Some(gb) = &mut self.gb {
            gb.cpu.bus.set_audio_sample_rate(rate);
        }
    }

    /// Access the shared application context.
    pub fn app_context(&self) -> &SharedAppContext {
        &self.app_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config;

    fn make_gameboy() -> GameBoy {
        let config = Config::default();
        let app_context = AppContext::new_with_config(config).into_shared();
        GameBoy::new(app_context)
    }

    fn minimal_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    // ── safety before ROM load ──────────────────────────────────────────────

    #[test]
    fn test_no_rom_run_tick_returns_zero() {
        let mut gb = make_gameboy();
        assert_eq!(gb.run_tick(), 0);
    }

    #[test]
    fn test_no_rom_is_frame_ready_returns_false() {
        let gb = make_gameboy();
        assert!(!gb.is_frame_ready());
    }

    #[test]
    fn test_no_rom_screen_snapshot_returns_correct_size() {
        let gb = make_gameboy();
        let snap = gb.screen_snapshot();
        assert_eq!(snap.len(), 160 * 144 * 3);
    }

    #[test]
    fn test_no_rom_get_joypad_button_states_returns_zero() {
        let gb = make_gameboy();
        assert_eq!(gb.get_joypad_button_states(), 0);
    }

    #[test]
    fn test_no_rom_set_button_does_not_panic() {
        let mut gb = make_gameboy();
        gb.set_button(0, true); // should not panic
    }

    #[test]
    fn test_no_rom_save_state_returns_err() {
        let gb = make_gameboy();
        assert!(gb.save_state_bytes().is_err());
    }

    #[test]
    fn test_no_rom_load_state_returns_err() {
        let mut gb = make_gameboy();
        assert!(gb.load_state_bytes(&[0u8; 4]).is_err());
    }

    // ── after ROM load ──────────────────────────────────────────────────────

    #[test]
    fn test_load_valid_rom_succeeds() {
        let mut gb = make_gameboy();
        assert!(gb.load_rom(&minimal_rom(), "test.gb").is_ok());
    }

    #[test]
    fn test_load_invalid_rom_returns_err() {
        let mut gb = make_gameboy();
        assert!(gb.load_rom(&[0u8; 16], "bad.gb").is_err());
    }

    #[test]
    fn test_run_tick_after_load_returns_nonzero_cycles() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let cycles = gb.run_tick();
        assert!(cycles > 0, "expected non-zero cycles, got {cycles}");
    }

    #[test]
    fn test_set_get_joypad_button_states_roundtrip() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let mask: u8 = 0b0000_1001; // A + Start pressed
        gb.set_joypad_button_states(mask);
        assert_eq!(gb.get_joypad_button_states(), mask);
    }

    #[test]
    fn test_screen_constants() {
        assert_eq!(GameBoy::SCREEN_WIDTH, 160);
        assert_eq!(GameBoy::SCREEN_HEIGHT, 144);
    }

    #[test]
    fn test_reset_soft_after_load_does_not_panic() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        gb.reset(true); // should not panic
    }

    #[test]
    fn test_reset_hard_after_load_does_not_panic() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        gb.reset(false); // should not panic
    }

    #[test]
    fn test_app_context_returns_reference() {
        let gb = make_gameboy();
        let _ = gb.app_context(); // should not panic
    }

    // ── APU sample output ──────────────────────────────────────────────────

    #[test]
    fn test_sample_not_ready_before_rom_load() {
        let gb = make_gameboy();
        assert!(!gb.sample_ready());
    }

    #[test]
    fn test_get_sample_returns_none_before_rom_load() {
        let mut gb = make_gameboy();
        assert!(gb.get_sample().is_none());
    }

    #[test]
    fn test_sample_ready_after_ticks_with_rom() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        // Run 30 ticks; APU should produce at least one sample.
        for _ in 0..30 {
            gb.run_tick();
        }
        assert!(
            gb.sample_ready(),
            "sample must be ready after running 30 ticks"
        );
    }

    #[test]
    fn test_get_sample_clears_ready_flag() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..30 {
            gb.run_tick();
        }
        assert!(gb.sample_ready());
        gb.get_sample();
        assert!(!gb.sample_ready());
    }

    #[test]
    fn test_set_audio_sample_rate_does_not_panic() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        gb.set_audio_sample_rate(48_000.0);
    }
}
