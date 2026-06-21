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
use crate::snes::console::config::SnesHardware;
use crate::snes::console::save_state::SnesSaveState;
use crate::snes::cpu::Cpu;
use crate::snes::ppu::SnesVideoRegion;
use std::path::PathBuf;
use std::time::Duration;

/// SNES display width in pixels (standard NTSC mode).
const SCREEN_WIDTH: u32 = 256;
/// SNES display height in pixels (standard NTSC mode).
const SCREEN_HEIGHT: u32 = 224;

/// SNES frame duration (~60.098 Hz refresh rate).
/// Master clock: 21.477272 MHz, 357366 cycles per frame → 16.639 ms per frame.
const FRAME_DURATION_NANOS: u64 = 16_639_000;
/// SNES PAL frame duration (~50.007 Hz refresh rate).
/// Master clock: 21.28137 MHz, 425568 cycles per frame (341*312*4).
const FRAME_DURATION_PAL_NANOS: u64 = 19_997_209;

/// Super Nintendo Entertainment System emulator wrapper.
///
/// This struct wraps the SNES emulation core and provides the [`Emulator`] trait
/// implementation for platform integration.
pub struct Snes {
    app_context: SharedAppContext,
    cpu: Option<Cpu<SnesSystemBus>>,
    rom_path: Option<PathBuf>,
    ready_to_render: bool,
    active_hardware: SnesHardware,
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
            active_hardware: SnesHardware::Ntsc,
        }
    }

    fn country_implies_pal(country: u8) -> bool {
        // Fullsnes FFD9 country codes: 0x02..=0x0C are PAL/50Hz territories.
        (0x02..=0x0C).contains(&country)
    }

    fn resolve_hardware_mode(
        config_override: Option<SnesHardware>,
        cartridge_country: u8,
    ) -> SnesHardware {
        config_override.unwrap_or_else(|| {
            if Self::country_implies_pal(cartridge_country) {
                SnesHardware::Pal
            } else {
                SnesHardware::Ntsc
            }
        })
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
    #[cfg(not(target_arch = "wasm32"))]
    fn sav_path(&self) -> Option<PathBuf> {
        self.rom_path
            .as_ref()
            .map(|path| path.with_extension("sav"))
    }

    /// Load battery-backed cartridge SRAM from a `.sav` file if one exists.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_save_ram_from_disk(&mut self) {
        let Some(sav_path) = self.sav_path() else {
            return;
        };

        let Some(cpu) = self.cpu.as_mut() else {
            return;
        };

        if !cpu.has_battery() {
            return;
        }

        if !sav_path.exists() {
            return;
        }

        match std::fs::read(&sav_path) {
            Ok(data) => {
                let expected_len = cpu.sram_size();
                if data.len() != expected_len {
                    crate::platform::debugging::log_info(format!(
                        "Warning: ignoring save file {} due to size mismatch (expected {}, got {})",
                        sav_path.display(),
                        expected_len,
                        data.len()
                    ));
                    return;
                }
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

        let Some(sav_path) = self.sav_path() else {
            return Ok(());
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
        let config = self.app_context.borrow().config().snes.clone();
        self.active_hardware = Self::resolve_hardware_mode(config.hardware, cartridge.country());

        let spc_ipl_path = config.spc_ipl_path;
        let video_region = match self.active_hardware {
            SnesHardware::Ntsc => SnesVideoRegion::Ntsc,
            SnesHardware::Pal => SnesVideoRegion::Pal,
        };
        let bus = SnesSystemBus::new_with_spc_ipl_path_and_region(
            cartridge,
            spc_ipl_path.as_deref(),
            video_region,
        );
        let mut cpu = Cpu::new(bus);
        cpu.configure_controllers(config.controller_port1, config.controller_port2);
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
        if cpu.take_frame_complete() {
            self.ready_to_render = true;
        }
        cycles
    }

    fn is_ready_to_render(&self) -> bool {
        self.ready_to_render
    }

    fn clear_ready_to_render(&mut self) {
        self.ready_to_render = false;
    }

    fn screen_width(&self) -> u32 {
        self.cpu
            .as_ref()
            .map(|cpu| cpu.screen_dimensions().0)
            .unwrap_or(SCREEN_WIDTH)
    }

    fn screen_height(&self) -> u32 {
        self.cpu
            .as_ref()
            .map(|cpu| cpu.screen_dimensions().1)
            .unwrap_or(SCREEN_HEIGHT)
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        match self.cpu.as_ref() {
            Some(cpu) => cpu.screen_snapshot(),
            None => vec![0; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize],
        }
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
        self.cpu
            .as_ref()
            .is_some_and(|cpu| cpu.bus().sample_ready())
    }

    fn get_sample(&mut self) -> Option<f32> {
        self.cpu
            .as_mut()
            .and_then(|cpu| cpu.bus_mut().take_sample())
    }

    fn get_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.cpu
            .as_mut()
            .and_then(|cpu| cpu.bus_mut().take_stereo_sample())
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.bus_mut().set_audio_sample_rate(rate);
        }
    }

    fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        let Some(cpu) = self.cpu.as_mut() else {
            return;
        };
        if let Some(button) = crate::snes::input::button_from_id(button_id) {
            cpu.set_controller_button(port, button, pressed);
        }
    }

    fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.set_joypad_button_states(port, state);
        }
    }

    fn get_joypad_button_states(&self, port: u8) -> u8 {
        self.cpu
            .as_ref()
            .map(|cpu| cpu.joypad_button_states(port))
            .unwrap_or(0)
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        let Some(cpu) = self.cpu.as_ref() else {
            return Err("No ROM loaded".to_string());
        };

        cpu.capture_save_state()
            .to_bytes()
            .map_err(|e| format!("save state serialization failed: {e}"))
    }

    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let Some(cpu) = self.cpu.as_mut() else {
            return Err("No ROM loaded".to_string());
        };

        let state = SnesSaveState::from_bytes(data)
            .map_err(|e| format!("save state deserialization failed: {e}"))?;
        cpu.restore_save_state(&state).map_err(|e| e.to_string())
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
        match self.active_hardware {
            SnesHardware::Ntsc => Duration::from_nanos(FRAME_DURATION_NANOS),
            SnesHardware::Pal => Duration::from_nanos(FRAME_DURATION_PAL_NANOS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config;
    use crate::snes::console::config::SnesHardware;

    fn valid_lorom_nop_rom() -> Vec<u8> {
        valid_lorom_nop_rom_with_country(0x00)
    }

    fn valid_lorom_nop_rom_with_country(country: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0xD5] = 0x20;
        rom[header + 0xD6] = 0x00;
        rom[header + 0xD7] = 0x07;
        rom[header + 0xD8] = 0x00;
        rom[header + 0xD9] = country;
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

    fn make_snes_with_hardware(hardware: Option<SnesHardware>) -> Snes {
        let mut config = Config::default();
        config.snes.hardware = hardware;
        let app_context = AppContext::new_with_config(config);
        Snes::new(app_context)
    }

    fn ticks_until_ready_to_render(snes: &mut Snes) -> u32 {
        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 2_000_000 {
            snes.run_tick();
            ticks += 1;
        }
        ticks
    }

    fn ticks_between_frame_completions(snes: &mut Snes) -> u32 {
        let _ = ticks_until_ready_to_render(snes);
        assert!(snes.is_ready_to_render());
        snes.clear_ready_to_render();

        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 2_000_000 {
            snes.run_tick();
            ticks += 1;
        }
        ticks
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
    fn target_frame_duration_uses_pal_when_forced_by_config() {
        let mut config = Config::default();
        config.snes.hardware = Some(SnesHardware::Pal);
        let app_context = AppContext::new_with_config(config);
        let mut snes = Snes::new(app_context);
        snes.load_rom(&valid_lorom_nop_rom_with_country(0x01), "test.sfc")
            .unwrap();

        assert_eq!(
            snes.target_frame_duration(),
            Duration::from_nanos(FRAME_DURATION_PAL_NANOS)
        );
    }

    #[test]
    fn target_frame_duration_auto_detects_pal_from_header_country() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom_with_country(0x02), "test.sfc")
            .unwrap();

        assert_eq!(
            snes.target_frame_duration(),
            Duration::from_nanos(FRAME_DURATION_PAL_NANOS)
        );
    }

    #[test]
    fn target_frame_duration_prefers_config_ntsc_over_pal_header_country() {
        let mut config = Config::default();
        config.snes.hardware = Some(SnesHardware::Ntsc);
        let app_context = AppContext::new_with_config(config);
        let mut snes = Snes::new(app_context);
        snes.load_rom(&valid_lorom_nop_rom_with_country(0x02), "test.sfc")
            .unwrap();

        assert_eq!(
            snes.target_frame_duration(),
            Duration::from_nanos(FRAME_DURATION_NANOS)
        );
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
    fn set_button_round_trips_through_joypad_states() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        // ids: 0=A, 4=Up.
        snes.set_button(0, 0, true);
        snes.set_button(0, 4, true);
        let states = snes.get_joypad_button_states(0);
        assert_ne!(states & 0x01, 0, "A pressed (bit 0)");
        assert_ne!(states & 0x10, 0, "Up pressed (bit 4)");

        snes.set_button(0, 0, false);
        assert_eq!(snes.get_joypad_button_states(0) & 0x01, 0, "A released");
    }

    #[test]
    fn bulk_joypad_states_round_trip() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        snes.set_joypad_button_states(1, 0b1010_0101);
        assert_eq!(snes.get_joypad_button_states(1), 0b1010_0101);
    }

    #[test]
    fn set_button_without_rom_is_a_no_op() {
        let mut snes = make_snes();
        snes.set_button(0, 0, true);
        assert_eq!(snes.get_joypad_button_states(0), 0);
    }

    #[test]
    fn snes_eventually_produces_stereo_audio_after_execution_advances() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "audio.sfc")
            .expect("load ROM");
        snes.set_audio_sample_rate(32_000.0);

        let mut ticks = 0u32;
        while !snes.sample_ready() && ticks < 100_000 {
            let step_cycles = snes.run_tick();
            assert!(step_cycles > 0, "loaded SNES should keep running");
            ticks += 1;
        }

        assert!(
            snes.sample_ready(),
            "SNES should eventually expose a ready audio sample once emulation advances"
        );

        let stereo = snes.get_stereo_sample();
        assert!(
            stereo.is_some(),
            "SNES should provide a stereo sample instead of relying on the mono default"
        );
    }

    #[test]
    fn run_tick_sets_ready_to_render_after_a_full_frame() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        assert!(!snes.is_ready_to_render());

        // Run until the PPU completes a frame (a frame is ~357k master cycles).
        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 1_000_000 {
            snes.run_tick();
            ticks += 1;
        }

        assert!(
            snes.is_ready_to_render(),
            "a frame should complete within the cap"
        );
    }

    #[test]
    fn pal_hardware_mode_takes_more_ticks_per_frame_than_ntsc() {
        let mut ntsc = make_snes_with_hardware(Some(SnesHardware::Ntsc));
        ntsc.load_rom(&valid_lorom_nop_rom_with_country(0x00), "ntsc.sfc")
            .unwrap();
        let ntsc_ticks = ticks_between_frame_completions(&mut ntsc);
        assert!(ntsc.is_ready_to_render(), "NTSC frame should complete");

        let mut pal = make_snes_with_hardware(Some(SnesHardware::Pal));
        pal.load_rom(&valid_lorom_nop_rom_with_country(0x00), "pal.sfc")
            .unwrap();
        let pal_ticks = ticks_between_frame_completions(&mut pal);
        assert!(pal.is_ready_to_render(), "PAL frame should complete");

        assert!(
            pal_ticks > ntsc_ticks,
            "PAL should require more CPU steps per frame than NTSC"
        );
    }

    #[test]
    fn pal_country_auto_detect_takes_more_ticks_per_frame_than_ntsc_country() {
        let mut ntsc = make_snes_with_hardware(None);
        ntsc.load_rom(&valid_lorom_nop_rom_with_country(0x00), "ntsc-auto.sfc")
            .unwrap();
        let ntsc_ticks = ticks_between_frame_completions(&mut ntsc);
        assert!(ntsc.is_ready_to_render());

        let mut pal = make_snes_with_hardware(None);
        pal.load_rom(&valid_lorom_nop_rom_with_country(0x02), "pal-auto.sfc")
            .unwrap();
        let pal_ticks = ticks_between_frame_completions(&mut pal);
        assert!(pal.is_ready_to_render());

        assert!(
            pal_ticks > ntsc_ticks,
            "PAL auto-detect should produce longer frame timing than NTSC auto-detect"
        );
    }

    #[test]
    fn clear_ready_to_render_clears_flag() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();

        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 1_000_000 {
            snes.run_tick();
            ticks += 1;
        }
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
        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 1_000_000 {
            snes.run_tick();
            ticks += 1;
        }
        assert!(snes.is_ready_to_render());
        snes.load_rom(&rom, "test.sfc").unwrap();
        assert!(!snes.is_ready_to_render());
    }

    #[test]
    fn reset_clears_ready_to_render_flag() {
        let mut snes = make_snes();
        snes.load_rom(&valid_lorom_nop_rom(), "test.sfc").unwrap();
        let mut ticks = 0;
        while !snes.is_ready_to_render() && ticks < 1_000_000 {
            snes.run_tick();
            ticks += 1;
        }
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
    fn save_ram_returns_ok_when_no_rom_loaded() {
        let snes = make_snes();
        // save_ram should still return Ok even if no ROM is loaded
        let result = snes.save_ram();
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn sram_round_trip_preserves_contents() {
        use std::fs;

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let rom_path = temp_dir.path().join("test.sfc");
        let sav_path = temp_dir.path().join("test.sav");

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
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn load_rom_ignores_incompatible_sav_size() {
        use std::fs;

        let temp = tempfile::tempdir().expect("tempdir");
        let rom_path = temp.path().join("test.sfc");
        let sav_path = temp.path().join("test.sav");

        let rom = lorom_rom_with_battery_sram(0x05); // 32 KB SRAM
        fs::write(&rom_path, &rom).expect("write rom");

        // Wrong size (should be 32 KB), must be ignored.
        fs::write(&sav_path, vec![0xAB; 64]).expect("write mismatched sav");

        let mut snes = Snes::new(AppContext::new_with_config(Config::default()));
        snes.load_rom(&rom, rom_path.to_str().expect("rom path utf8"))
            .expect("load rom");

        let snapshot = snes.cpu.as_ref().expect("cpu present").sram_snapshot();
        assert_eq!(snapshot.len(), 32 * 1024);
        assert_eq!(
            snapshot[0], 0x00,
            "mismatched save file should be ignored entirely"
        );
        assert_eq!(snapshot[63], 0x00);
    }

    fn dirty_cpu_for_save_state(cpu: &mut Cpu<SnesSystemBus>) {
        cpu.load_state_for_processor_test(
            0xA1B2, 0xC3D4, 0xE5F6, 0x1234, 0x56, 0x78, 0x9ABC, 0xDEF0, 0x2D, false,
        );
        cpu.set_nmi(true);
        cpu.set_irq(true);
        cpu.set_abort(true);
        cpu.restore_sram(&[0x11, 0x22, 0x33, 0x44, 0x55]);
    }

    #[test]
    fn save_state_bytes_round_trips_cpu_and_sram() {
        let rom = lorom_rom_with_battery_sram(0x05);

        let mut source = make_snes();
        source
            .load_rom(&rom, "roundtrip_source.sfc")
            .expect("load source ROM");
        dirty_cpu_for_save_state(source.cpu.as_mut().expect("cpu present"));
        let original_pc = source.cpu.as_ref().expect("cpu present").read_pc();
        let original_sram = source.cpu.as_ref().expect("cpu present").sram_snapshot();

        let bytes = source
            .save_state_bytes()
            .expect("save state serialization should work");

        let mut restored = make_snes();
        restored
            .load_rom(&rom, "roundtrip_restored.sfc")
            .expect("load restore ROM");
        dirty_cpu_for_save_state(restored.cpu.as_mut().expect("cpu present"));

        restored
            .load_state_bytes(&bytes)
            .expect("restore from bytes should work");

        let restored_cpu = restored.cpu.as_ref().expect("cpu present");
        assert_eq!(restored_cpu.read_pc(), original_pc);
        assert_eq!(restored_cpu.sram_snapshot(), original_sram);
        assert!(!restored_cpu.emulation_mode());
        assert_eq!(restored_cpu.read_a(), 0xA1B2);
        assert_eq!(restored_cpu.read_x(), 0xC3D4);
        assert_eq!(restored_cpu.read_y(), 0xE5F6);
    }

    #[test]
    fn save_state_bytes_requires_rom_loaded() {
        let snes = make_snes();
        let result = snes.save_state_bytes();
        assert!(matches!(result, Err(msg) if msg.contains("No ROM loaded")));
    }

    #[test]
    fn save_state_round_trips_ppu_state() {
        let rom = lorom_rom_with_battery_sram(0x05);

        // Source: run a while so the PPU advances past its power-on state, then save.
        let mut source = make_snes();
        source
            .load_rom(&rom, "ppu_source.sfc")
            .expect("load source ROM");
        for _ in 0..5000 {
            source.run_tick();
        }
        let bytes = source.save_state_bytes().expect("save state");

        // Restore into a fresh SNES and re-save. If PPU state (position, VRAM/CGRAM/OAM, flags)
        // were not part of the round-trip, the re-saved bytes would differ from the originals.
        let mut restored = make_snes();
        restored
            .load_rom(&rom, "ppu_restore.sfc")
            .expect("load restore ROM");
        restored.load_state_bytes(&bytes).expect("restore");
        let bytes2 = restored.save_state_bytes().expect("re-save state");

        assert_eq!(bytes, bytes2, "PPU state survives the save/load round-trip");
    }

    #[test]
    fn load_state_bytes_rejects_version_mismatch() {
        let rom = lorom_rom_with_battery_sram(0x05);
        let mut snes = make_snes();
        snes.load_rom(&rom, "version_check.sfc").expect("load ROM");

        let result = snes.load_state_bytes(br#"{"version":9999}"#);
        assert!(matches!(result, Err(msg) if msg.contains("incompatible")));
    }

    #[test]
    fn load_state_bytes_rejects_rom_mismatch() {
        let mut source = make_snes();
        let source_rom = lorom_rom_with_battery_sram(0x05);
        source
            .load_rom(&source_rom, "source.sfc")
            .expect("load source ROM");
        let bytes = source
            .save_state_bytes()
            .expect("serialize source save state");

        let mut other = make_snes();
        let mut other_rom = lorom_rom_with_battery_sram(0x05);
        other_rom[0x0000] = 0xEB;
        other
            .load_rom(&other_rom, "other.sfc")
            .expect("load other ROM");

        let result = other.load_state_bytes(&bytes);
        assert!(matches!(result, Err(msg) if msg.contains("ROM mismatch")));
    }

    #[test]
    fn save_state_from_bytes_allows_missing_newer_fields() {
        let rom = lorom_rom_with_battery_sram(0x05);
        let mut snes = make_snes();
        snes.load_rom(&rom, "compat.sfc").expect("load ROM");

        let save = snes.cpu.as_ref().expect("cpu present").capture_save_state();
        let mut json = serde_json::to_value(&save).expect("serialize save state");
        json["bus"]
            .as_object_mut()
            .expect("bus object")
            .get_mut("dma")
            .expect("dma object")
            .as_object_mut()
            .expect("dma map")
            .remove("hdma_lines_left");
        let ppu = json["ppu"].as_object_mut().expect("ppu object");
        ppu.remove("irq_mode");
        ppu.remove("htime");
        ppu.remove("vtime");
        ppu.remove("timeup_flag");
        ppu.remove("irq_line");

        let bytes = serde_json::to_vec(&json).expect("serialize compat state");
        let loaded = SnesSaveState::from_bytes(&bytes).expect("compat state should load");
        let mut restored = make_snes();
        restored
            .load_rom(&rom, "compat_restore.sfc")
            .expect("load ROM");
        restored
            .cpu
            .as_mut()
            .expect("cpu present")
            .restore_save_state(&loaded)
            .expect("restore should succeed");

        assert_eq!(
            loaded.version,
            crate::snes::console::save_state::SNES_SAVESTATE_VERSION
        );
        assert_eq!(loaded.bus.dma.hdma_lines_left, vec![0; 8]);
        assert_eq!(loaded.ppu.irq_mode, 0);
        assert_eq!(loaded.ppu.htime, 0);
        assert_eq!(loaded.ppu.vtime, 0);
        assert!(!loaded.ppu.timeup_flag);
        assert!(!loaded.ppu.irq_line);
    }
}
