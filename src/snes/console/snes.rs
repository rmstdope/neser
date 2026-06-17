//! Platform-facing SNES wrapper for the `Console` enum.
//!
//! `Snes` provides the platform interface for SNES emulation, implementing the
//! [`Emulator`] trait so frontends can drive it through [`Console`].
//!
//! [`Emulator`]: crate::platform::emulator::Emulator
//! [`Console`]: crate::platform::emulator::Console

use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::emulator::{Emulator, SystemType};
use crate::snes::bus::SnesSystemBus;
use crate::snes::cartridge::Cartridge;
use crate::snes::cpu::Cpu;
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
    cpu: Option<Cpu<SnesSystemBus>>,
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
            cpu: None,
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

    /// Returns the file path where battery-backed SRAM should be stored.
    #[allow(dead_code)]
    fn sav_path(&self) -> Option<PathBuf> {
        self.rom_path
            .as_ref()
            .map(|path| path.with_extension("sav"))
    }

    /// Load battery-backed cartridge SRAM from a `.sav` file if one exists.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_save_ram_from_disk(&mut self) {
        let Some(cpu) = self.cpu.as_mut() else {
            return;
        };

        if !cpu.has_battery() {
            return;
        }

        // Get the sav path before borrowing cpu mutably
        let sav_path = match &self.rom_path {
            Some(path) => path.with_extension("sav"),
            None => return,
        };

        if !sav_path.exists() {
            return;
        }

        match std::fs::read(&sav_path) {
            Ok(data) => {
                cpu.restore_sram(&data);
            }
            Err(e) => {
                crate::platform::debugging::log_info(format!(
                    "Warning: failed to read save file {}: {e}",
                    sav_path.display()
                ));
            }
        }
    }

    /// WASM stub: no filesystem operations on WASM.
    #[cfg(target_arch = "wasm32")]
    fn load_save_ram_from_disk(&mut self) {
        // No-op on WASM
    }

    /// Save battery-backed cartridge RAM to a `.sav` file.
    ///
    /// Uses a temp file + rename for atomic writes to prevent corruption.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_ram_to_disk(&self) -> Result<(), String> {
        let Some(cpu) = self.cpu.as_ref() else {
            return Ok(());
        };

        if !cpu.has_battery() {
            return Ok(());
        }

        // Get the sav path from rom_path
        let sav_path = match &self.rom_path {
            Some(path) => path.with_extension("sav"),
            None => return Ok(()),
        };

        // Read current SRAM from the CPU
        let data = cpu.sram_snapshot();
        if data.is_empty() {
            return Ok(());
        }

        // Atomic write: temp file → rename
        let mut temp_path = sav_path.clone();
        temp_path.set_extension(format!("sav.tmp.{}", std::process::id()));

        if let Some(parent) = sav_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dir {}: {e}", parent.display()))?;
        }

        std::fs::write(&temp_path, &data)
            .map_err(|e| format!("failed to write {}: {e}", temp_path.display()))?;

        if sav_path.exists() {
            let _ = std::fs::remove_file(&sav_path);
        }

        std::fs::rename(&temp_path, &sav_path)
            .map_err(|e| format!("failed to rename to {}: {e}", sav_path.display()))
    }

    /// WASM stub: no filesystem operations on WASM.
    #[cfg(target_arch = "wasm32")]
    fn save_ram_to_disk(&self) -> Result<(), String> {
        Ok(())
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

    fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        let cartridge = Cartridge::from_bytes(bytes).map_err(|e| format!("{e:?}"))?;
        let bus = SnesSystemBus::new(cartridge);
        let mut cpu = Cpu::new(bus);
        cpu.do_reset();
        self.cpu = Some(cpu);
        self.rom_path = Some(PathBuf::from(name));
        self.ready_to_render = false;

        // Load battery-backed save RAM from disk if a .sav file exists.
        self.load_save_ram_from_disk();

        Ok(())
    }

    fn run_tick(&mut self) -> u8 {
        let Some(cpu) = self.cpu.as_mut() else {
            return 0;
        };

        let cycles = cpu.step();
        self.ready_to_render = true;
        cycles
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
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.do_reset();
        }
        self.ready_to_render = false;
    }

    fn save_ram(&self) -> Result<(), String> {
        self.save_ram_to_disk()
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

    fn valid_lorom_nop_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0xD5] = 0x20;
        rom[header + 0xD6] = 0x00;
        rom[header + 0xD7] = 0x07;
        rom[header + 0xD8] = 0x00;
        rom[header + 0xDC] = 0x34;
        rom[header + 0xDD] = 0x12;
        rom[header + 0xDE] = 0xCB;
        rom[header + 0xDF] = 0xED;
        rom[0x0000] = 0xEA; // NOP at $00:8000
        rom
    }

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
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        let state_path = snes.state_path();
        assert!(state_path.is_some());
        assert_eq!(state_path.unwrap().to_string_lossy(), "test.state");
    }

    #[test]
    fn run_tick_returns_nonzero_cycles() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
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
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        assert!(!snes.is_ready_to_render());
        snes.run_tick();
        assert!(snes.is_ready_to_render());
    }

    #[test]
    fn clear_ready_to_render_clears_flag() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        snes.run_tick();
        assert!(snes.is_ready_to_render());
        snes.clear_ready_to_render();
        assert!(!snes.is_ready_to_render());
    }

    #[test]
    fn load_rom_rejects_invalid_snes_bytes() {
        let mut snes = make_snes();
        let result = snes.load_rom(&[0x00, 0x01, 0x02], "bad.sfc");
        assert!(result.is_err());
    }

    #[test]
    fn run_tick_after_rom_load_uses_cpu_step_cycles() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        let cycles = snes.run_tick();
        assert_ne!(cycles, 1);
    }

    #[test]
    fn load_rom_clears_ready_to_render_flag() {
        let mut snes = make_snes();
        let rom = valid_lorom_nop_rom();
        snes.load_rom(&rom, "test.sfc").unwrap();
        snes.run_tick();
        assert!(snes.is_ready_to_render());
        snes.load_rom(&rom, "test.sfc").unwrap();
        assert!(!snes.is_ready_to_render());
    }

    #[test]
    fn reset_clears_ready_to_render_flag() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        snes.run_tick();
        assert!(snes.is_ready_to_render());
        snes.reset(false);
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

    fn lorom_rom_with_battery_sram(ram_size_field: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x20000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES BATTERY TEST    ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0xD5] = 0x20;
        rom[header + 0xD6] = 0x02; // Battery-backed RAM chipset
        rom[header + 0xD7] = 0x07;
        rom[header + 0xD8] = ram_size_field;
        rom[header + 0xDC] = 0x34;
        rom[header + 0xDD] = 0x12;
        rom[header + 0xDE] = 0xCB;
        rom[header + 0xDF] = 0xED;
        rom[0x0000] = 0xEA; // NOP at $00:8000
        rom
    }

    #[test]
    fn save_ram_is_noop_for_non_battery_cartridge() {
        let mut snes = make_snes();
        let rom = valid_lorom_nop_rom(); // No battery
        snes.load_rom(&rom, "test.sfc").unwrap();

        // Should not fail
        let result = snes.save_ram();
        assert!(result.is_ok());
    }

    #[test]
    fn save_ram_fails_gracefully_when_no_rom_loaded() {
        let snes = make_snes();
        // save_ram should still return Ok even if no ROM is loaded
        let result = snes.save_ram();
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn sram_round_trip_preserves_contents() {
        use std::fs;

        // Create a temporary directory for test files
        let temp_dir = std::env::temp_dir().join("neser_sram_test");
        let _ = fs::create_dir_all(&temp_dir);

        let rom_path = temp_dir.join("test.sfc");
        let sav_path = temp_dir.join("test.sav");

        // Clean up any existing files
        let _ = fs::remove_file(&sav_path);

        // Create and write test data
        let rom = lorom_rom_with_battery_sram(0x05); // 32 KB SRAM
        fs::write(&rom_path, &rom).expect("failed to write test ROM");

        // Create test SRAM data
        let mut test_sram = vec![0u8; 32 * 1024];
        test_sram[0] = 0xAA;
        test_sram[1] = 0xBB;
        test_sram[2] = 0xCC;
        test_sram[100] = 0xDD;
        test_sram[1000] = 0xEE;

        // First console: load ROM, restore SRAM, save to disk
        {
            let mut snes1 = Snes::new(AppContext::new_with_config(Config::default()));
            snes1
                .load_rom(&rom, rom_path.to_str().unwrap())
                .expect("failed to load ROM");

            // Restore test SRAM data
            if let Some(cpu) = snes1.cpu.as_mut() {
                cpu.restore_sram(&test_sram);
            }

            // Save to disk
            snes1.save_ram().expect("failed to save RAM");
        }

        // Verify .sav file was created
        assert!(sav_path.exists(), ".sav file should be created");

        // Read the saved file and verify contents
        let saved_data = fs::read(&sav_path).expect("failed to read saved .sav file");
        assert_eq!(saved_data[0], 0xAA);
        assert_eq!(saved_data[1], 0xBB);
        assert_eq!(saved_data[2], 0xCC);
        assert_eq!(saved_data[100], 0xDD);
        assert_eq!(saved_data[1000], 0xEE);

        // Second console: load ROM (which auto-loads SRAM from .sav), verify contents
        {
            let mut snes2 = Snes::new(AppContext::new_with_config(Config::default()));
            snes2
                .load_rom(&rom, rom_path.to_str().unwrap())
                .expect("failed to load ROM");

            // Verify SRAM was loaded via snapshot
            if let Some(cpu) = snes2.cpu.as_ref() {
                let snapshot = cpu.sram_snapshot();
                assert_eq!(snapshot[0], 0xAA, "SRAM byte 0 should be restored");
                assert_eq!(snapshot[1], 0xBB, "SRAM byte 1 should be restored");
                assert_eq!(snapshot[2], 0xCC, "SRAM byte 2 should be restored");
                assert_eq!(snapshot[100], 0xDD, "SRAM byte 100 should be restored");
                assert_eq!(snapshot[1000], 0xEE, "SRAM byte 1000 should be restored");
            }
        }

        // Cleanup
        let _ = fs::remove_file(&rom_path);
        let _ = fs::remove_file(&sav_path);
    }
}
