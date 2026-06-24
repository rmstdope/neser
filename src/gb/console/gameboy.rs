//! Platform-facing Game Boy wrapper for the `Console` enum.
//!
//! `GameBoy` owns a [`GbConsole`] (created lazily on [`load_rom`]) and a
//! [`SharedAppContext`], providing the same interface that the NES side
//! exposes so that frontends can drive both systems through `Console`.
//!
//! On load, the bus type is determined by combining the `--gb-hardware` config
//! option with the ROM's CGB flag byte (0x0143): CGB-only (0xC0) and CGB-compat
//! (0x80) ROMs default to [`CgbBus`]; all others default to [`DmgBus`].

use crate::gb::bus::{CgbBus, DmgBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::console::save_state::{GB_SAVESTATE_VERSION, GbSaveState};
use crate::gb::model::GbHardware;
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::emulator::{Emulator, SystemType};
use std::path::PathBuf;

/// Wraps either a DMG or CGB console, dispatching all platform operations.
enum GbConsole {
    Dmg(Box<Gb<DmgBus>>),
    Cgb(Box<Gb<CgbBus>>),
}

impl GbConsole {
    fn step(&mut self) -> u8 {
        match self {
            Self::Dmg(gb) => gb.step(),
            Self::Cgb(gb) => gb.step(),
        }
    }

    fn is_frame_ready(&self) -> bool {
        match self {
            Self::Dmg(gb) => gb.is_frame_ready(),
            Self::Cgb(gb) => gb.is_frame_ready(),
        }
    }

    fn clear_frame_ready(&mut self) {
        match self {
            Self::Dmg(gb) => gb.clear_frame_ready(),
            Self::Cgb(gb) => gb.clear_frame_ready(),
        }
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        match self {
            Self::Dmg(gb) => gb.screen_snapshot(),
            Self::Cgb(gb) => gb.screen_snapshot(),
        }
    }

    fn screen_crc32(&self) -> u32 {
        match self {
            Self::Dmg(gb) => gb.screen_crc32(),
            Self::Cgb(gb) => gb.screen_crc32(),
        }
    }

    fn reset(&mut self, soft_reset: bool) {
        match self {
            Self::Dmg(gb) => gb.reset(),
            Self::Cgb(gb) => gb.reset(soft_reset),
        }
    }

    fn set_joypad_button(&mut self, id: u8, pressed: bool) {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.set_joypad_button(id, pressed),
            Self::Cgb(gb) => gb.cpu.bus.set_joypad_button(id, pressed),
        }
    }

    fn get_joypad_button_states(&self) -> u8 {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.joypad.get_states(),
            Self::Cgb(gb) => gb.cpu.bus.joypad.get_states(),
        }
    }

    fn sample_ready(&self) -> bool {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.sample_ready(),
            Self::Cgb(gb) => gb.cpu.bus.sample_ready(),
        }
    }

    fn take_sample(&mut self) -> Option<f32> {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.take_sample(),
            Self::Cgb(gb) => gb.cpu.bus.take_sample(),
        }
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.set_audio_sample_rate(rate),
            Self::Cgb(gb) => gb.cpu.bus.set_audio_sample_rate(rate),
        }
    }

    fn has_battery(&self) -> bool {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.has_battery(),
            Self::Cgb(gb) => gb.cpu.bus.has_battery(),
        }
    }

    fn cart_ram_snapshot(&self) -> Vec<u8> {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.cart_ram_snapshot(),
            Self::Cgb(gb) => gb.cpu.bus.cart_ram_snapshot(),
        }
    }

    fn restore_cart_ram(&mut self, data: &[u8]) {
        match self {
            Self::Dmg(gb) => gb.cpu.bus.restore_cart_ram(data),
            Self::Cgb(gb) => gb.cpu.bus.restore_cart_ram(data),
        }
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        let state = match self {
            Self::Dmg(gb) => GbSaveState {
                version: GB_SAVESTATE_VERSION,
                cpu: gb.cpu.capture_state(),
                bus: gb.cpu.bus.capture_bus_state(),
                cart_ram: gb.cpu.bus.cart_ram_snapshot(),
                mbc_state: gb.cpu.bus.mbc_state_snapshot(),
            },
            Self::Cgb(gb) => GbSaveState {
                version: GB_SAVESTATE_VERSION,
                cpu: gb.cpu.capture_state(),
                bus: gb.cpu.bus.capture_bus_state(),
                cart_ram: gb.cpu.bus.cart_ram_snapshot(),
                mbc_state: gb.cpu.bus.mbc_state_snapshot(),
            },
        };
        state
            .to_bytes()
            .map_err(|e| format!("save state serialization failed: {e}"))
    }

    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let state = GbSaveState::from_bytes(data)
            .map_err(|e| format!("save state deserialization failed: {e}"))?;
        match self {
            Self::Dmg(gb) => {
                gb.cpu.restore_state(&state.cpu);
                gb.cpu.bus.restore_bus_state(&state.bus)?;
                gb.reconcile_stop_display_after_state_load();
                gb.cpu.bus.restore_cart_ram(&state.cart_ram);
                gb.cpu.bus.restore_mbc_state(&state.mbc_state);
                gb.cpu.bus.joypad.clear_buttons();
            }
            Self::Cgb(gb) => {
                gb.cpu.restore_state(&state.cpu);
                gb.cpu.bus.restore_bus_state(&state.bus)?;
                gb.reconcile_stop_display_after_state_load();
                gb.cpu.bus.restore_cart_ram(&state.cart_ram);
                gb.cpu.bus.restore_mbc_state(&state.mbc_state);
                gb.cpu.bus.joypad.clear_buttons();
            }
        }
        Ok(())
    }
}

/// Platform-facing Game Boy (DMG/CGB) console wrapper.
pub struct GameBoy {
    gb: Option<GbConsole>,
    app_context: SharedAppContext,
    /// Path of the currently loaded ROM; used for deriving the save-state path.
    rom_path: Option<PathBuf>,
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
            rom_path: None,
        }
    }

    /// Load a `.gb` or `.gbc` ROM image. Replaces any previously loaded ROM.
    ///
    /// Hardware mode selection combines the `--gb-hardware` config option with
    /// the ROM's CGB flag byte at 0x0143:
    /// - `None` (auto-detect): DMG-only ROMs use [`DmgBus`], dual/CGB-only use [`CgbBus`]
    /// - `Dmg`: Forces [`DmgBus`] for DMG/dual ROMs, errors on CGB-only
    /// - `Cgb`/`Gba`: Forces [`CgbBus`] for all ROMs
    pub fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        let cart = load_cartridge(bytes).map_err(|e| format!("{e:?}"))?;
        let is_cgb_rom = cart.is_cgb();
        let hardware = self.app_context.borrow().config().gb.hardware;

        // Determine bus type based on hardware config and ROM flags
        let use_cgb_bus = match hardware {
            None => {
                // Auto-detect: DMG-only ROMs use DmgBus, dual/CGB-only use CgbBus
                is_cgb_rom
            }
            Some(GbHardware::Dmg) => {
                // Force DMG: error on CGB-only, otherwise use DmgBus
                if is_cgb_rom && cart.read(0x0143) == 0xC0 {
                    return Err(
                        "This CGB-only game requires --gb-hardware cgb or --gb-hardware gba"
                            .to_string(),
                    );
                }
                false
            }
            Some(GbHardware::Cgb) | Some(GbHardware::Gba) => {
                // Force CGB/GBA: always use CgbBus
                true
            }
        };

        self.gb = Some(if use_cgb_bus {
            let config = self.app_context.borrow().config().gb.clone();
            let skip_boot_rom = !config.boot_animation;
            let mut gb = Gb::new(CgbBus::new(cart, config.cgb_variant, skip_boot_rom));
            if skip_boot_rom {
                // Only set registers if skipping boot ROM (boot ROM sets them otherwise)
                gb.cpu.reset_registers_cgb();
            }
            GbConsole::Cgb(Box::new(gb))
        } else {
            let dmg_variant = self.app_context.borrow().config().gb.dmg_variant;
            GbConsole::Dmg(Box::new(Gb::new(DmgBus::new(cart, dmg_variant))))
        });
        self.rom_path = Some(PathBuf::from(name));

        // Load battery-backed save RAM from disk if a .sav file exists.
        self.load_save_ram_from_disk();

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

    /// Set a single button state.
    ///
    /// Uses NES-convention IDs: A=0, B=1, Select=2, Start=3, Up=4, Down=5,
    /// Left=6, Right=7.
    pub fn set_button(&mut self, id: u8, pressed: bool) {
        if let Some(gb) = &mut self.gb {
            gb.set_joypad_button(id, pressed);
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
            .map_or(0, |gb| gb.get_joypad_button_states())
    }

    /// Serialize emulator state to bytes (JSON).
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        match &self.gb {
            Some(gb) => gb.save_state_bytes(),
            None => Err("No ROM loaded".into()),
        }
    }

    /// Restore emulator state from previously serialized bytes (JSON).
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        match &mut self.gb {
            Some(gb) => gb.load_state_bytes(data),
            None => Err("No ROM loaded".into()),
        }
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
        self.gb.as_ref().is_some_and(|gb| gb.sample_ready())
    }

    /// Retrieve the next APU audio sample, or `None` if not ready.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.gb.as_mut().and_then(|gb| gb.take_sample())
    }

    /// Set the APU output sample rate in Hz.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        if let Some(gb) = &mut self.gb {
            gb.set_audio_sample_rate(rate);
        }
    }

    /// Access the shared application context.
    pub fn app_context(&self) -> &SharedAppContext {
        &self.app_context
    }

    /// Returns the save-state file path (`{rom_path}.state`), or `None`
    /// if no ROM is loaded.
    pub fn state_path(&self) -> Option<PathBuf> {
        self.rom_path.as_ref().map(|p| p.with_extension("state"))
    }

    /// Returns `true` when the loaded cartridge has battery-backed RAM.
    pub fn has_battery(&self) -> bool {
        self.gb.as_ref().is_some_and(|gb| gb.has_battery())
    }

    /// Snapshot cartridge RAM contents (battery-backed SRAM).
    pub fn cart_ram_snapshot(&self) -> Vec<u8> {
        self.gb
            .as_ref()
            .map_or_else(Vec::new, |gb| gb.cart_ram_snapshot())
    }

    /// Returns the `.sav` path derived from the ROM path, or `None`.
    fn sav_path(&self) -> Option<PathBuf> {
        self.rom_path.as_ref().map(|p| p.with_extension("sav"))
    }

    /// Save battery-backed cartridge RAM to a `.sav` file.
    ///
    /// Uses a temp file + rename for atomic writes to prevent corruption.
    pub fn save_ram_to_disk(&self) -> Result<(), String> {
        if !self.has_battery() {
            return Ok(());
        }
        let Some(sav_path) = self.sav_path() else {
            return Ok(());
        };
        let data = self.cart_ram_snapshot();
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

    /// Load battery-backed cartridge RAM from a `.sav` file if one exists.
    fn load_save_ram_from_disk(&mut self) {
        if !self.has_battery() {
            return;
        }
        let Some(sav_path) = self.sav_path() else {
            return;
        };
        if !sav_path.exists() {
            return;
        }
        match std::fs::read(&sav_path) {
            Ok(data) => {
                if let Some(gb) = &mut self.gb {
                    gb.restore_cart_ram(&data);
                }
            }
            Err(e) => {
                crate::platform::debugging::log_info(format!(
                    "Warning: failed to read save file {}: {e}",
                    sav_path.display()
                ));
            }
        }
    }

    // ── Debugger support ───────────────────────────────────────────────

    #[cfg(feature = "native")]
    /// Create a GB debugger snapshot from the current state.
    pub fn create_debugger_snapshot(
        &self,
        view_state: &mut crate::gb::debugging::GbDebuggerViewState,
    ) -> crate::gb::debugging::GbDebuggerSnapshot {
        self.gb.as_ref().map_or_else(
            || {
                // Return a default snapshot when no ROM is loaded
                // Use Default to avoid per-frame Vec allocations
                crate::gb::debugging::GbDebuggerSnapshot::default()
            },
            |gb_console| match gb_console {
                GbConsole::Dmg(gb) => view_state.snapshot(gb.as_ref()),
                GbConsole::Cgb(gb) => view_state.snapshot(gb.as_ref()),
            },
        )
    }

    #[cfg(feature = "native")]
    /// Create a GB PPU viewer snapshot from the current state.
    pub fn create_ppu_viewer_snapshot(
        &self,
    ) -> crate::gb::debugging::ppu_viewer::GbPpuViewerSnapshot {
        use crate::gb::debugging::ppu_viewer::GbPpuViewerSnapshot;
        self.gb.as_ref().map_or_else(
            || {
                // Return a default snapshot when no ROM is loaded
                GbPpuViewerSnapshot {
                    vram: [0; 0x2000],
                    vram_bank1: [0; 0x2000],
                    oam: [0; 0xA0],
                    bg_palette_ram: [0; 64],
                    obj_palette_ram: [0; 64],
                    lcdc: 0,
                    scx: 0,
                    scy: 0,
                    bgp: 0,
                    obp0: 0,
                    obp1: 0,
                    cgb_mode: false,
                }
            },
            |gb_console| match gb_console {
                GbConsole::Dmg(gb) => GbPpuViewerSnapshot::from_gb(gb.as_ref()),
                GbConsole::Cgb(gb) => GbPpuViewerSnapshot::from_gb(gb.as_ref()),
            },
        )
    }

    #[cfg(feature = "native")]
    /// Check if the emulator is running in CGB mode.
    pub fn is_cgb_mode(&self) -> bool {
        self.gb
            .as_ref()
            .is_some_and(|gb_console| matches!(gb_console, GbConsole::Cgb(_)))
    }

    #[cfg(feature = "native")]
    /// Run one frame with the debugger controller.
    pub fn run_frame_with_debugger(
        &mut self,
        controller: &mut crate::gb::debugging::control::GbDebuggerController,
        audio_cell: &std::cell::RefCell<Option<crate::frontends::native::NativeAudio>>,
    ) {
        use crate::platform::audio::EmulatorAudio;

        if let Some(gb_console) = self.gb.as_mut() {
            match gb_console {
                GbConsole::Dmg(gb) => {
                    controller.run_frame(gb.as_mut(), &mut |gb| {
                        if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                            while gb.cpu.bus.sample_ready() {
                                if let Some(sample) = gb.cpu.bus.take_sample() {
                                    audio.queue_sample(sample);
                                }
                            }
                        }
                    });
                }
                GbConsole::Cgb(gb) => {
                    controller.run_frame(gb.as_mut(), &mut |gb| {
                        if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                            while gb.cpu.bus.sample_ready() {
                                if let Some(sample) = gb.cpu.bus.take_sample() {
                                    audio.queue_sample(sample);
                                }
                            }
                        }
                    });
                }
            }
        }
    }

    #[cfg(feature = "native")]
    /// Toggle the debugger open/closed.
    pub fn toggle_debugger_with_controller(
        &mut self,
        controller: &mut crate::gb::debugging::control::GbDebuggerController,
    ) {
        if let Some(gb_console) = self.gb.as_mut() {
            match gb_console {
                GbConsole::Dmg(gb) => controller.toggle_debugger(gb.as_mut()),
                GbConsole::Cgb(gb) => controller.toggle_debugger(gb.as_mut()),
            }
        }
    }

    #[cfg(feature = "native")]
    /// Step over the current instruction.
    pub fn step_over_with_controller(
        &mut self,
        controller: &mut crate::gb::debugging::control::GbDebuggerController,
    ) {
        if let Some(gb_console) = self.gb.as_mut() {
            match gb_console {
                GbConsole::Dmg(gb) => controller.step_over(gb.as_mut()),
                GbConsole::Cgb(gb) => controller.step_over(gb.as_mut()),
            }
        }
    }

    #[cfg(feature = "native")]
    /// Step into the current instruction.
    pub fn step_into_with_controller(
        &mut self,
        controller: &mut crate::gb::debugging::control::GbDebuggerController,
    ) {
        if let Some(gb_console) = self.gb.as_mut() {
            match gb_console {
                GbConsole::Dmg(gb) => controller.step_into(gb.as_mut()),
                GbConsole::Cgb(gb) => controller.step_into(gb.as_mut()),
            }
        }
    }

    #[cfg(feature = "native")]
    /// Apply UI action from the debugger.
    pub fn apply_ui_action_with_controller(
        &mut self,
        controller: &mut crate::gb::debugging::control::GbDebuggerController,
        action: crate::gb::debugging::ui::GbDebuggerUiAction,
    ) {
        if let Some(gb_console) = self.gb.as_mut() {
            match gb_console {
                GbConsole::Dmg(gb) => controller.apply_ui_action(gb.as_mut(), action),
                GbConsole::Cgb(gb) => controller.apply_ui_action(gb.as_mut(), action),
            }
        }
    }
}

impl Emulator for GameBoy {
    fn system_type(&self) -> SystemType {
        SystemType::GameBoy
    }

    fn allowed_shaders(&self) -> &'static [&'static str] {
        &["none", "dmg"]
    }

    fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        GameBoy::load_rom(self, bytes, name)
    }

    fn run_tick(&mut self) -> u8 {
        GameBoy::run_tick(self)
    }

    fn is_ready_to_render(&self) -> bool {
        self.is_frame_ready()
    }

    fn clear_ready_to_render(&mut self) {
        self.clear_frame_ready()
    }

    fn screen_width(&self) -> u32 {
        GameBoy::SCREEN_WIDTH
    }

    fn screen_height(&self) -> u32 {
        GameBoy::SCREEN_HEIGHT
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        GameBoy::screen_snapshot(self)
    }

    fn screen_crc32(&self) -> u32 {
        GameBoy::screen_crc32(self)
    }

    fn sample_ready(&self) -> bool {
        GameBoy::sample_ready(self)
    }

    fn get_sample(&mut self) -> Option<f32> {
        GameBoy::get_sample(self)
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        GameBoy::set_audio_sample_rate(self, rate)
    }

    fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        if port == 0 || port == 1 {
            GameBoy::set_button(self, button_id, pressed);
        }
    }

    fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        if port == 0 || port == 1 {
            GameBoy::set_joypad_button_states(self, state);
        }
    }

    fn get_joypad_button_states(&self, port: u8) -> u8 {
        if port == 0 || port == 1 {
            GameBoy::get_joypad_button_states(self)
        } else {
            0
        }
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        GameBoy::save_state_bytes(self)
    }

    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        GameBoy::load_state_bytes(self, data)
    }

    fn reset(&mut self, soft_reset: bool) {
        GameBoy::reset(self, soft_reset)
    }

    fn save_ram(&self) -> Result<(), String> {
        self.save_ram_to_disk()
    }

    fn app_context(&self) -> &SharedAppContext {
        GameBoy::app_context(self)
    }

    fn target_frame_duration(&self) -> std::time::Duration {
        // DMG: 4,194,304 Hz clock / 70,224 cycles per frame ≈ 59.7275 fps
        std::time::Duration::from_secs_f64(70_224.0 / 4_194_304.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::model::GbHardware;
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

    fn minimal_cgb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0xC0; // CGB-only flag
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn minimal_dual_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0x80; // CGB+DMG compatible flag
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn make_gameboy_with_hardware(hardware: GbHardware) -> GameBoy {
        let mut config = Config::default();
        config.gb.hardware = Some(hardware);
        let app_context = AppContext::new_with_config(config).into_shared();
        GameBoy::new(app_context)
    }

    /// Build a ROM with MBC5+RAM+BATTERY (type 0x1B), 8 KB RAM.
    fn mbc5_battery_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x1B; // MBC5+RAM+BATTERY
        rom[0x0148] = 0x00; // 32 KB ROM
        rom[0x0149] = 0x02; // 8 KB RAM
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
    fn test_load_cgb_rom_succeeds() {
        let mut gb = make_gameboy();
        assert!(gb.load_rom(&minimal_cgb_rom(), "test.gbc").is_ok());
    }

    #[test]
    fn test_cgb_rom_run_tick_returns_nonzero_cycles() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        let cycles = gb.run_tick();
        assert!(cycles > 0, "expected non-zero cycles, got {cycles}");
    }

    #[test]
    fn test_cgb_rom_joypad_roundtrip() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        let mask: u8 = 0b0000_0001; // A pressed
        gb.set_joypad_button_states(mask);
        assert_eq!(gb.get_joypad_button_states(), mask);
    }

    #[test]
    fn test_cgb_rom_audio_sample_rate_does_not_panic() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        gb.set_audio_sample_rate(48_000.0);
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
    fn test_get_sample_consumes_one_queued_sample() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..30 {
            gb.run_tick();
        }
        assert!(gb.sample_ready());
        assert!(gb.get_sample().is_some());
    }

    #[test]
    fn test_set_audio_sample_rate_does_not_panic() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        gb.set_audio_sample_rate(48_000.0);
    }

    // ── save / load state ──────────────────────────────────────────────────

    #[test]
    fn test_dmg_save_state_returns_ok_after_rom_load() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..10 {
            gb.run_tick();
        }
        assert!(gb.save_state_bytes().is_ok());
    }

    #[test]
    fn test_cgb_save_state_returns_ok_after_rom_load() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        for _ in 0..10 {
            gb.run_tick();
        }
        assert!(gb.save_state_bytes().is_ok());
    }

    #[test]
    fn test_dmg_save_load_roundtrip() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..10 {
            gb.run_tick();
        }
        let snap1 = gb.screen_crc32();
        let state = gb.save_state_bytes().unwrap();

        // Run more ticks to change state
        for _ in 0..50 {
            gb.run_tick();
        }

        // Restore and verify
        gb.load_state_bytes(&state).unwrap();
        assert_eq!(gb.screen_crc32(), snap1);
    }

    #[test]
    fn test_cgb_save_load_roundtrip() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        for _ in 0..10 {
            gb.run_tick();
        }
        let snap1 = gb.screen_crc32();
        let state = gb.save_state_bytes().unwrap();

        for _ in 0..50 {
            gb.run_tick();
        }

        gb.load_state_bytes(&state).unwrap();
        assert_eq!(gb.screen_crc32(), snap1);
    }

    #[test]
    fn test_load_state_clears_joypad_button_states() {
        // Regression: pressing a D-pad button, saving state, then loading that
        // state used to leave the button stuck pressed even with no key held.
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        // Press Up (button id 4) and save while it is held.
        gb.set_button(4, true); // Up
        assert_ne!(
            gb.get_joypad_button_states() & (1 << 4),
            0,
            "Up should be pressed before save"
        );
        let state = gb.save_state_bytes().unwrap();

        // Restore state — Up must not remain pressed.
        gb.load_state_bytes(&state).unwrap();

        assert_eq!(
            gb.get_joypad_button_states(),
            0,
            "All joypad buttons must be cleared after loading a save state"
        );
    }

    #[test]
    fn test_load_invalid_state_returns_err() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let result = gb.load_state_bytes(b"invalid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_incompatible_version_returns_err() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..5 {
            gb.run_tick();
        }
        let mut state = gb.save_state_bytes().unwrap();
        // Corrupt the version field in the JSON
        let json_str = String::from_utf8(state).unwrap();
        let corrupted = json_str.replacen(
            &format!("\"version\":{}", GB_SAVESTATE_VERSION),
            "\"version\":9999",
            1,
        );
        state = corrupted.into_bytes();
        let result = gb.load_state_bytes(&state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("incompatible"));
    }

    #[test]
    fn test_state_path_with_rom_loaded() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "roms/test.gb").unwrap();
        let path = gb.state_path().expect("state_path should be Some");
        assert_eq!(path.to_str().unwrap(), "roms/test.state");
    }

    #[test]
    fn test_state_path_without_rom_loaded() {
        let gb = make_gameboy();
        assert!(gb.state_path().is_none());
    }

    #[test]
    fn test_load_dmg_state_into_cgb_returns_err() {
        let mut dmg_gb = make_gameboy();
        dmg_gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        for _ in 0..10 {
            dmg_gb.run_tick();
        }
        let dmg_state = dmg_gb.save_state_bytes().unwrap();

        let mut cgb_gb = make_gameboy();
        cgb_gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        let result = cgb_gb.load_state_bytes(&dmg_state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bus type mismatch"));
    }

    #[test]
    fn test_load_cgb_state_into_dmg_returns_err() {
        let mut cgb_gb = make_gameboy();
        cgb_gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        for _ in 0..10 {
            cgb_gb.run_tick();
        }
        let cgb_state = cgb_gb.save_state_bytes().unwrap();

        let mut dmg_gb = make_gameboy();
        dmg_gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let result = dmg_gb.load_state_bytes(&cgb_state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bus type mismatch"));
    }

    // ── Emulator trait port-1 tests (autorun recording/playback) ──────────
    // These tests verify that the Emulator trait's port-1 interface works for
    // the Game Boy. The autorun system uses port 1 for player 1, so the GB
    // must expose its single joypad on both port 0 (keyboard) and port 1
    // (autorun convention).

    #[test]
    fn test_emulator_trait_port1_reads_buttons_set_via_port0() {
        // Simulates: keyboard sets buttons via port 0; autorun recording reads
        // them via port 1.  Before the fix, port 1 always returned 0.
        use crate::platform::emulator::Emulator;
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let emu: &mut dyn Emulator = &mut gb;

        let state: u8 = 0b0000_1001; // A + Start pressed
        // Keyboard handler sets buttons via port 0
        emu.set_joypad_button_states(0, state);
        // Autorun recording reads via port 1
        assert_eq!(
            emu.get_joypad_button_states(1),
            state,
            "port 1 must reflect buttons set via port 0 (keyboard)"
        );
    }

    #[test]
    fn test_emulator_trait_port1_set_joypad_states_applied_to_gb() {
        // Simulates: autorun playback writes buttons via port 1; verify that
        // the GB joypad state is actually updated.  Before the fix, port 1
        // was silently ignored and the joypad stayed at 0.
        use crate::platform::emulator::Emulator;
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let emu: &mut dyn Emulator = &mut gb;

        let state: u8 = 0b0100_0010; // B + Left pressed
        // Autorun playback writes via port 1
        emu.set_joypad_button_states(1, state);
        // Verify via port 0 (keyboard read) and port 1 (autorun read)
        assert_eq!(
            emu.get_joypad_button_states(0),
            state,
            "port 0 must reflect buttons set via port 1 (autorun playback)"
        );
        assert_eq!(
            emu.get_joypad_button_states(1),
            state,
            "port 1 must reflect buttons set via port 1 (autorun playback)"
        );
    }

    #[test]
    fn test_emulator_trait_port2_returns_zero_for_gb() {
        // GB has no second controller; port 2 must always return 0.
        use crate::platform::emulator::Emulator;
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let emu: &mut dyn Emulator = &mut gb;

        emu.set_joypad_button_states(2, 0xFF);
        assert_eq!(
            emu.get_joypad_button_states(2),
            0,
            "port 2 must return 0 for GB (no second controller)"
        );
    }

    #[test]
    fn test_emulator_trait_port1_set_button_applied_to_gb() {
        // Verify individual button via port 1 is also applied correctly.
        use crate::platform::emulator::Emulator;
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        let emu: &mut dyn Emulator = &mut gb;

        // Press A (button id 0) via port 1
        emu.set_button(1, 0, true);
        assert_ne!(
            emu.get_joypad_button_states(1),
            0,
            "setting a button via port 1 must update the GB joypad"
        );
    }

    #[test]
    fn test_gb_allowed_shaders_includes_expected_presets() {
        use crate::platform::emulator::Emulator;
        let gb = make_gameboy();
        let shaders = gb.allowed_shaders();
        assert!(
            shaders.contains(&"none"),
            "GB must allow the 'none' (stock) shader"
        );
        assert!(shaders.contains(&"dmg"), "GB must allow the 'dmg' shader");
        assert!(
            !shaders.contains(&"crt"),
            "GB must NOT allow the 'crt' shader"
        );
        assert!(
            !shaders.contains(&"ntsc"),
            "GB must NOT allow the 'ntsc' shader"
        );
        assert!(
            !shaders.contains(&"pal"),
            "GB must NOT allow the 'pal' shader"
        );
        assert!(
            !shaders.contains(&"smooth"),
            "GB must NOT allow the 'smooth' shader"
        );
    }

    // ── gb-hardware mode selection tests ───────────────────────────────────

    #[test]
    fn test_dmg_rom_with_no_hardware_option_uses_dmg_bus() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Dmg(_))));
    }

    #[test]
    fn test_dual_rom_with_no_hardware_option_uses_cgb_bus() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_dual_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_cgb_only_rom_with_no_hardware_option_uses_cgb_bus() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_dmg_rom_with_dmg_hardware_uses_dmg_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Dmg);
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Dmg(_))));
    }

    #[test]
    fn test_dual_rom_with_dmg_hardware_uses_dmg_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Dmg);
        gb.load_rom(&minimal_dual_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Dmg(_))));
    }

    #[test]
    fn test_cgb_only_rom_with_dmg_hardware_returns_error() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Dmg);
        let result = gb.load_rom(&minimal_cgb_rom(), "test.gbc");
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("CGB-only"));
        assert!(err_msg.contains("--gb-hardware cgb"));
    }

    #[test]
    fn test_dmg_rom_with_cgb_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Cgb);
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_dual_rom_with_cgb_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Cgb);
        gb.load_rom(&minimal_dual_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_cgb_only_rom_with_cgb_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Cgb);
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_dmg_rom_with_gba_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Gba);
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_dual_rom_with_gba_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Gba);
        gb.load_rom(&minimal_dual_rom(), "test.gb").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    #[test]
    fn test_cgb_only_rom_with_gba_hardware_uses_cgb_bus() {
        let mut gb = make_gameboy_with_hardware(GbHardware::Gba);
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        assert!(matches!(gb.gb, Some(GbConsole::Cgb(_))));
    }

    // ── Debugger wrapper tests ──────────────────────────────────────────────

    #[cfg(feature = "native")]
    #[test]
    fn test_create_debugger_snapshot_with_no_rom_returns_default() {
        let gb = make_gameboy();
        let mut view_state = crate::gb::debugging::GbDebuggerViewState::default();

        let snapshot = gb.create_debugger_snapshot(&mut view_state);

        // Should return default snapshot with zeroed CPU regs
        assert_eq!(snapshot.cpu_regs.pc, 0);
        assert_eq!(snapshot.cpu_regs.sp, 0);
        assert_eq!(snapshot.wram_hexdump_base, 0xC000);
        assert_eq!(snapshot.vram_hexdump_base, 0x8000);
        // Hexdump bytes should be allocated but zeroed
        assert_eq!(snapshot.wram_hexdump_bytes.len(), 256);
        assert_eq!(snapshot.vram_hexdump_bytes.len(), 256);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_create_debugger_snapshot_with_rom_captures_state() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        // Run a few cycles to get non-zero state
        for _ in 0..10 {
            gb.run_tick();
        }

        let mut view_state = crate::gb::debugging::GbDebuggerViewState::default();
        let snapshot = gb.create_debugger_snapshot(&mut view_state);

        // Should have captured actual emulator state
        // PC should have advanced from boot (or be at a valid address)
        assert!(snapshot.cpu_regs.pc > 0 || snapshot.cpu_regs.cycles > 0);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_create_ppu_viewer_snapshot_with_no_rom_returns_defaults() {
        let gb = make_gameboy();

        let snapshot = gb.create_ppu_viewer_snapshot();

        // Should return default snapshot with zeroed VRAM
        assert_eq!(snapshot.vram, [0; 0x2000]);
        assert_eq!(snapshot.vram_bank1, [0; 0x2000]);
        assert_eq!(snapshot.oam, [0; 0xA0]);
        assert_eq!(snapshot.lcdc, 0);
        assert!(!snapshot.cgb_mode);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_create_ppu_viewer_snapshot_with_rom_captures_ppu_state() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        // Run to get some PPU state
        for _ in 0..100 {
            gb.run_tick();
        }

        let snapshot = gb.create_ppu_viewer_snapshot();

        // Should have created snapshot successfully
        // Check that we got valid arrays (not just the check would be that they're allocated)
        assert_eq!(snapshot.vram.len(), 0x2000);
        assert_eq!(snapshot.oam.len(), 0xA0);
        // cgb_mode should match the ROM type (DMG in this case)
        assert!(!snapshot.cgb_mode);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_is_cgb_mode_with_no_rom_returns_false() {
        let gb = make_gameboy();
        assert!(!gb.is_cgb_mode());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_is_cgb_mode_with_dmg_rom_returns_false() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();
        assert!(!gb.is_cgb_mode());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_is_cgb_mode_with_cgb_rom_returns_true() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_cgb_rom(), "test.gbc").unwrap();
        assert!(gb.is_cgb_mode());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_toggle_debugger_with_controller_opens_when_closed() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], false);
        assert!(!controller.is_debugger_open());

        gb.toggle_debugger_with_controller(&mut controller);

        assert!(controller.is_debugger_open());
        assert!(controller.is_paused());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_toggle_debugger_with_controller_closes_when_open() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], true);
        assert!(controller.is_debugger_open());

        gb.toggle_debugger_with_controller(&mut controller);

        assert!(!controller.is_debugger_open());
        assert!(!controller.is_paused());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_toggle_debugger_with_no_rom_does_not_panic() {
        let mut gb = make_gameboy();
        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], false);

        // Should not panic even without ROM loaded
        gb.toggle_debugger_with_controller(&mut controller);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_step_into_with_controller_advances_instruction() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], true);

        let mut view_state = crate::gb::debugging::GbDebuggerViewState::default();
        let snapshot_before = gb.create_debugger_snapshot(&mut view_state);
        let pc_before = snapshot_before.cpu_regs.pc;

        gb.step_into_with_controller(&mut controller);

        let snapshot_after = gb.create_debugger_snapshot(&mut view_state);
        let pc_after = snapshot_after.cpu_regs.pc;

        // PC should have advanced by at least 1
        assert!(
            pc_after != pc_before
                || snapshot_after.cpu_regs.cycles > snapshot_before.cpu_regs.cycles
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_step_over_with_controller_executes_instruction() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], true);

        let mut view_state = crate::gb::debugging::GbDebuggerViewState::default();
        let snapshot_before = gb.create_debugger_snapshot(&mut view_state);
        let cycles_before = snapshot_before.cpu_regs.cycles;

        gb.step_over_with_controller(&mut controller);

        let snapshot_after = gb.create_debugger_snapshot(&mut view_state);
        let cycles_after = snapshot_after.cpu_regs.cycles;

        // Cycles should have progressed
        assert!(cycles_after > cycles_before);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_apply_ui_action_step_into_advances_execution() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], true);

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            step_into: true,
            ..Default::default()
        };

        let mut view_state = crate::gb::debugging::GbDebuggerViewState::default();
        let cycles_before = gb.create_debugger_snapshot(&mut view_state).cpu_regs.cycles;

        gb.apply_ui_action_with_controller(&mut controller, action);

        let cycles_after = gb.create_debugger_snapshot(&mut view_state).cpu_regs.cycles;

        // Should have stepped
        assert!(cycles_after > cycles_before);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_apply_ui_action_continue_unpauses_execution() {
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], true);
        assert!(controller.is_paused());

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            continue_run: true,
            ..Default::default()
        };

        gb.apply_ui_action_with_controller(&mut controller, action);

        // Should be unpaused after continue
        assert!(!controller.is_paused());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_wrapper_methods_with_no_rom_do_not_panic() {
        let mut gb = make_gameboy();
        let mut controller = crate::gb::debugging::control::GbDebuggerController::new(&[], false);

        // All methods should handle no-ROM case gracefully
        gb.toggle_debugger_with_controller(&mut controller);
        gb.step_into_with_controller(&mut controller);
        gb.step_over_with_controller(&mut controller);

        let action = crate::gb::debugging::ui::GbDebuggerUiAction::default();
        gb.apply_ui_action_with_controller(&mut controller, action);

        // If we got here without panicking, test passes
    }

    // ── Save RAM persistence ─────────────────────────────────────────────────

    #[test]
    fn test_has_battery_returns_true_for_mbc5_battery_cart() {
        // Given: a GameBoy loaded with an MBC5+RAM+BATTERY ROM
        let mut gb = make_gameboy();
        gb.load_rom(&mbc5_battery_rom(), "test.gb").unwrap();

        // Then: has_battery reports true
        assert!(
            gb.has_battery(),
            "MBC5+RAM+BATTERY cart should report has_battery=true"
        );
    }

    #[test]
    fn test_has_battery_returns_false_for_rom_only_cart() {
        // Given: a GameBoy loaded with a ROM-only cartridge (no battery)
        let mut gb = make_gameboy();
        gb.load_rom(&minimal_rom(), "test.gb").unwrap();

        // Then: has_battery reports false
        assert!(
            !gb.has_battery(),
            "ROM-only cart should report has_battery=false"
        );
    }

    #[test]
    fn test_save_ram_writes_sav_file_for_battery_cart() {
        // Given: a GameBoy with MBC5+RAM+BATTERY ROM loaded from a temp dir
        let dir = tempfile::tempdir().unwrap();
        let rom_path = dir.path().join("test_battery.gb");
        let sav_path = dir.path().join("test_battery.sav");
        std::fs::write(&rom_path, mbc5_battery_rom()).unwrap();

        let mut gb = make_gameboy();
        gb.load_rom(
            &std::fs::read(&rom_path).unwrap(),
            rom_path.to_str().unwrap(),
        )
        .unwrap();

        // When: save_ram is called
        let result = gb.save_ram();

        // Then: succeeds and .sav file exists
        assert!(result.is_ok(), "save_ram should succeed");
        assert!(sav_path.exists(), ".sav file should be created");
    }

    #[test]
    fn test_load_rom_restores_sav_file_for_battery_cart() {
        // Given: a .sav file with known data exists alongside the ROM
        let dir = tempfile::tempdir().unwrap();
        let rom_path = dir.path().join("test_battery.gb");
        let sav_path = dir.path().join("test_battery.sav");

        // Write a ROM and a .sav file with recognizable pattern
        std::fs::write(&rom_path, mbc5_battery_rom()).unwrap();
        let mut save_data = vec![0u8; 8 * 1024]; // 8 KB matching the cart RAM size
        save_data[0] = 0xAA;
        save_data[1] = 0xBB;
        save_data[42] = 0xCC;
        std::fs::write(&sav_path, &save_data).unwrap();

        // When: load_rom is called with the ROM path
        let mut gb = make_gameboy();
        gb.load_rom(
            &std::fs::read(&rom_path).unwrap(),
            rom_path.to_str().unwrap(),
        )
        .unwrap();

        // Then: cart RAM should contain the saved data
        let ram_snapshot = gb.cart_ram_snapshot();
        assert_eq!(ram_snapshot[0], 0xAA, "save data byte 0 should be restored");
        assert_eq!(ram_snapshot[1], 0xBB, "save data byte 1 should be restored");
        assert_eq!(
            ram_snapshot[42], 0xCC,
            "save data byte 42 should be restored"
        );
    }
}
