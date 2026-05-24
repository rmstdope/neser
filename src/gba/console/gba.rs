//! Platform-facing Game Boy Advance wrapper for the `Console` enum.
//!
//! `Gba` provides the platform interface for GBA emulation, implementing the
//! [`Emulator`] trait so frontends can drive it through [`Console`].
//!
//! ROM loading is implemented so GBA cartridges can be inserted from the
//! platform startup flow. `run_tick` executes one ARM7TDMI instruction and
//! advances bus peripherals by the consumed cycles.
//!
//! [`Emulator`]: crate::platform::emulator::Emulator
//! [`Console`]: crate::platform::emulator::Console

use crate::gba::GbaBus;
use crate::gba::cartridge::load_cartridge;
use crate::gba::console::config::GBA_FILTER_NAMES;
use crate::gba::cpu::Arm7tdmi;
use crate::gba::cpu::bus::Bus;
#[cfg(not(target_arch = "wasm32"))]
use crate::gba::debugging::{disasm_arm, disasm_thumb};
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::emulator::{Emulator, SystemType};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// GBA display width in pixels.
const SCREEN_WIDTH: u32 = 240;
/// GBA display height in pixels.
const SCREEN_HEIGHT: u32 = 160;

/// GBA frame duration (~59.7275 Hz refresh rate).
/// CPU clock: 16.78 MHz, 280896 cycles per frame → 16.743 ms per frame.
const FRAME_DURATION_NANOS: u64 = 16_743_000;

#[cfg(test)]
thread_local! {
    static GBA_CPU_TRACE_LINES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn default_gba_bios_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".neser").join("gba_bios.bin"))
}

fn bios_size() -> usize {
    crate::gba::bus::memory::BIOS_SIZE
}

fn load_bios_image(path: &PathBuf) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|_| "GBA BIOS file could not be read".to_string())?;

    if !metadata.is_file() {
        return Err("GBA BIOS path must point to a regular file".to_string());
    }

    if metadata.len() != bios_size() as u64 {
        return Err(format!(
            "GBA BIOS has invalid size: expected {} bytes",
            bios_size()
        ));
    }

    let bytes = fs::read(path).map_err(|_| "GBA BIOS file could not be read".to_string())?;
    if bytes.len() != bios_size() {
        return Err(format!(
            "GBA BIOS has invalid size: expected {} bytes",
            bios_size()
        ));
    }

    Ok(bytes)
}

#[cfg(test)]
fn clear_gba_cpu_trace_lines_for_tests() {
    GBA_CPU_TRACE_LINES.with(|lines| lines.borrow_mut().clear());
}

#[cfg(test)]
fn take_gba_cpu_trace_lines_for_tests() -> Vec<String> {
    GBA_CPU_TRACE_LINES.with(|lines| lines.borrow_mut().drain(..).collect())
}

#[cfg(test)]
fn emit_gba_cpu_trace_line(line: String) {
    GBA_CPU_TRACE_LINES.with(|lines| lines.borrow_mut().push(line));
}

#[cfg(not(test))]
fn emit_gba_cpu_trace_line(line: String) {
    println!("{line}");
}

/// Game Boy Advance emulator wrapper.
///
/// This struct wraps the GBA emulation core and provides the [`Emulator`] trait
/// implementation for platform integration.
pub struct Gba {
    app_context: SharedAppContext,
    /// ARM7TDMI CPU core.
    cpu: Arm7tdmi,
    /// System bus owning all peripheral state including APU, keypad and
    /// interrupt controller.
    bus: GbaBus,
    rom_path: Option<PathBuf>,
    bios_load_error: Option<String>,
}

impl Gba {
    /// GBA display width in pixels.
    pub const SCREEN_WIDTH: u32 = SCREEN_WIDTH;
    /// GBA display height in pixels.
    pub const SCREEN_HEIGHT: u32 = SCREEN_HEIGHT;

    /// Create a new GBA emulator instance.
    pub fn new(app_context: impl IntoSharedAppContext) -> Self {
        let app_context = app_context.into_shared();
        let mut bus = GbaBus::new();

        let (configured_bios_path, color_correction, trace_config) = {
            let cfg = app_context.borrow();
            (
                cfg.config().gba.bios_path.clone(),
                cfg.config().gba.color_correction,
                cfg.config().gba.tracing,
            )
        };
        bus.set_trace_config(trace_config);

        bus.ppu.set_color_correction(color_correction);

        // "embedded" is a sentinel value: skip the default-path fallback and
        // use the built-in open-source BIOS unconditionally.
        let use_embedded = configured_bios_path.as_deref() == Some("embedded");
        let bios_path = if use_embedded {
            None
        } else {
            configured_bios_path
                .map(PathBuf::from)
                .or_else(default_gba_bios_path)
        };

        let bios_load_error = if let Some(path) = bios_path {
            match load_bios_image(&path) {
                Ok(bytes) => {
                    bus.load_bios(&bytes);
                    None
                }
                Err(e) => {
                    if path.exists() {
                        // File exists but is invalid (wrong size, unreadable, etc.)
                        // — report the error so the user can fix their config.
                        Some(e)
                    } else {
                        // File not found — fall back to embedded BIOS silently.
                        bus.load_bios(crate::gba::bios::EMBEDDED_BIOS);
                        None
                    }
                }
            }
        } else {
            // No BIOS path configured — use embedded BIOS
            bus.load_bios(crate::gba::bios::EMBEDDED_BIOS);
            None
        };

        let cpu = Arm7tdmi::new();

        Self {
            app_context,
            cpu,
            bus,
            rom_path: None,
            bios_load_error,
        }
    }

    /// Borrow the underlying system bus.
    pub fn bus(&self) -> &GbaBus {
        &self.bus
    }

    /// Mutably borrow the underlying system bus.
    pub fn bus_mut(&mut self) -> &mut GbaBus {
        &mut self.bus
    }

    /// Returns the save-state file path (`{rom_path}.state`), or `None`
    /// if no ROM is loaded.
    pub fn state_path(&self) -> Option<PathBuf> {
        self.rom_path.as_ref().map(|p| p.with_extension("state"))
    }

    #[cfg(test)]
    pub(crate) fn cpu_pc(&self) -> u32 {
        self.cpu.regs.r[15]
    }

    #[cfg(test)]
    pub(crate) fn cpu_reg(&self, index: usize) -> u32 {
        assert!(index < 16, "cpu register index out of range: {index}");
        self.cpu.regs.r[index]
    }

    #[cfg(test)]
    pub(crate) fn cpu_cpsr(&self) -> u32 {
        self.cpu.regs.cpsr
    }

    #[cfg(test)]
    pub(crate) fn cpu_thumb(&self) -> bool {
        self.cpu.regs.thumb()
    }

    #[cfg(test)]
    pub(crate) fn run_tick_for_tests(&mut self) -> u8 {
        if !self.bus.has_cart() {
            return 0;
        }

        if self.bus.ic.halt_exit_line() {
            self.cpu.signal_halt_exit();
        }

        if self.bus.ic.irq_line() {
            self.cpu.raise_irq();
        } else {
            self.cpu.clear_irq();
        }

        if self.bus.halt_requested() {
            self.cpu.halt();
            self.bus.clear_halt_request();
        }

        let cycles = self.cpu.step(&mut self.bus);
        self.bus.step(cycles);
        cycles as u8
    }

    fn current_instruction_trace_line(&mut self) -> Option<String> {
        if !self.bus.has_cart() {
            return None;
        }

        let pc = self.cpu.regs.r[15];
        if self.cpu.thumb() {
            let raw = self.bus.peek16(pc);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let asm = disasm_thumb(raw, pc);
                Some(format!("GBA THUMB PC={pc:08X} RAW={raw:04X} ASM={asm}"))
            }
            #[cfg(target_arch = "wasm32")]
            {
                Some(format!("GBA THUMB PC={pc:08X} RAW={raw:04X}"))
            }
        } else {
            let raw = self.bus.peek32(pc);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let asm = disasm_arm(raw, pc);
                Some(format!("GBA ARM   PC={pc:08X} RAW={raw:08X} ASM={asm}"))
            }
            #[cfg(target_arch = "wasm32")]
            {
                Some(format!("GBA ARM   PC={pc:08X} RAW={raw:08X}"))
            }
        }
    }

    fn cpu_trace_level(&self) -> u8 {
        self.app_context.borrow().config().gba.tracing.cpu
    }

    fn swi_trace_level(&self) -> u8 {
        self.app_context.borrow().config().gba.tracing.swi
    }

    fn current_swi_trace_line(&mut self) -> Option<String> {
        if !self.bus.has_cart() {
            return None;
        }

        let pc = self.cpu.regs.r[15];
        if self.cpu.thumb() {
            let raw = self.bus.peek16(pc);
            if raw & 0xFF00 == 0xDF00 {
                Some(format!("THUMB PC={pc:08X} IMM={:02X}", raw & 0x00FF))
            } else {
                None
            }
        } else {
            let raw = self.bus.peek32(pc);
            if raw & 0x0F00_0000 == 0x0F00_0000 {
                Some(format!("ARM PC={pc:08X} IMM={:02X}", (raw >> 16) & 0x00FF))
            } else {
                None
            }
        }
    }
}

impl Emulator for Gba {
    fn system_type(&self) -> SystemType {
        SystemType::Gba
    }

    fn allowed_shaders(&self) -> &'static [&'static str] {
        GBA_FILTER_NAMES
    }

    fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        if !self.bus.has_bios_image() {
            return Err(self
                .bios_load_error
                .clone()
                .unwrap_or_else(|| "GBA BIOS is required but was not loaded".to_string()));
        }

        let cart = load_cartridge(bytes).map_err(|e| format!("{e:?}"))?;
        let (rom, save) = cart.into_rom_and_save();
        self.bus.load_rom_with_save(&rom, save);

        // Write skip-intro flag to IWRAM so the BIOS can read it during boot.
        let skip_intro = {
            let cfg = self.app_context.borrow();
            cfg.config().gba.skip_bios_intro
        };
        if skip_intro {
            self.bus.write8(0x03007FFC, 1);
        }

        self.rom_path = Some(PathBuf::from(name));
        self.cpu.regs.r[15] = 0x0000_0000;
        Ok(())
    }

    fn run_tick(&mut self) -> u8 {
        if !self.bus.has_cart() {
            return 0;
        }

        if self.cpu_trace_level() > 0
            && let Some(line) = self.current_instruction_trace_line()
        {
            emit_gba_cpu_trace_line(format!("[GBA CPU] {line}"));
        }
        if self.swi_trace_level() > 0
            && let Some(line) = self.current_swi_trace_line()
        {
            emit_gba_cpu_trace_line(format!("[GBA SWI] {line}"));
        }
        if self.bus.ic.halt_exit_line() {
            self.cpu.signal_halt_exit();
        }

        if self.bus.ic.irq_line() {
            self.cpu.raise_irq();
        } else {
            self.cpu.clear_irq();
        }

        if self.bus.halt_requested() {
            self.cpu.halt();
            self.bus.clear_halt_request();
        }

        let cycles = self.cpu.step(&mut self.bus);
        self.bus.step(cycles);
        cycles as u8
    }

    fn is_ready_to_render(&self) -> bool {
        self.bus.ppu.frame_ready()
    }

    fn clear_ready_to_render(&mut self) {
        self.bus.ppu.clear_frame_ready();
    }

    fn screen_width(&self) -> u32 {
        SCREEN_WIDTH
    }

    fn screen_height(&self) -> u32 {
        SCREEN_HEIGHT
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        self.bus.ppu.framebuffer().to_vec()
    }

    fn cropped_screen_snapshot(&self, _h_overscan: u32, _v_overscan: u32) -> Vec<u8> {
        // GBA has no overscan, return full screen
        self.screen_snapshot()
    }

    fn screen_crc32(&self) -> u32 {
        crate::platform::crc32::crc32(&[self.bus.ppu.framebuffer()])
    }

    fn sample_ready(&self) -> bool {
        self.bus.apu.sample_ready()
    }

    fn get_sample(&mut self) -> Option<f32> {
        self.bus.apu.take_sample()
    }

    fn get_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.bus.apu.take_stereo_sample()
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        self.bus.apu.set_sample_rate(rate);
    }

    fn set_button(&mut self, _port: u8, button_id: u8, pressed: bool) {
        // GBA has a single keypad; ignore `port`.
        self.bus
            .keypad
            .set_button(button_id, pressed, &mut self.bus.ic);
    }

    fn set_joypad_button_states(&mut self, _port: u8, state: u8) {
        self.bus.keypad.set_states(state, &mut self.bus.ic);
    }

    fn get_joypad_button_states(&self, _port: u8) -> u8 {
        self.bus.keypad.get_states()
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        self.save_state()
            .to_bytes()
            .map_err(|e| format!("save state serialization failed: {e}"))
    }

    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let state = super::save_state::GbaSaveState::from_bytes(data)
            .map_err(|e| format!("save state deserialization failed: {e}"))?;
        self.load_state(&state).map_err(|e| e.to_string())
    }

    fn reset(&mut self, _soft_reset: bool) {
        // Current reset model: reset CPU core state and restart execution
        // from cart space when a ROM is present (or BIOS reset vector with no cart).
        self.cpu = Arm7tdmi::new();
        if self.bus.has_cart() {
            self.cpu.regs.r[15] = 0x0000_0000;
        }
        self.bus.ppu.clear_frame_ready();
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
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
    };
    use crate::gba::cpu::bus::Bus;
    use crate::gba::ppu;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config;

    fn make_gba_without_bios() -> Gba {
        let mut config = Config::default();
        config.gba.bios_path = Some("/__neser_missing_gba_bios.bin".to_string());
        Gba::new(AppContext::new_with_config(config))
    }

    fn make_gba() -> Gba {
        let mut gba = make_gba_without_bios();
        let bios = vec![0u8; crate::gba::bus::memory::BIOS_SIZE];
        gba.bus.load_bios(&bios);
        gba.bios_load_error = None;
        gba
    }

    fn make_gba_with_config(mut config: Config) -> Gba {
        config.gba.bios_path = Some("embedded".to_string());
        let mut gba = Gba::new(AppContext::new_with_config(config));
        let bios = vec![0u8; crate::gba::bus::memory::BIOS_SIZE];
        gba.bus.load_bios(&bios);
        gba.bios_load_error = None;
        gba
    }

    fn make_minimal_valid_gba_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0xC0];
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    fn set_mode3_bg2_enabled(gba: &mut Gba) {
        gba.bus
            .write16(ppu::REG_DISPCNT, 3 | ppu::dispcnt::BG2_ENABLE);
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
    fn test_load_rom_loads_valid_cartridge_into_bus() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        let result = gba.load_rom(&rom, "test.gba");
        assert!(result.is_ok());
        assert!(gba.bus().has_cart());
    }

    #[test]
    fn test_load_rom_succeeds_when_external_bios_is_missing() {
        // With the embedded BIOS fallback, ROM loading should always succeed
        // even when the configured external BIOS path is invalid.
        let mut gba = make_gba_without_bios();
        let rom = make_minimal_valid_gba_rom();

        let result = gba.load_rom(&rom, "test.gba");
        assert!(
            result.is_ok(),
            "ROM loading should succeed with embedded BIOS fallback"
        );
    }

    #[test]
    fn test_run_tick_advances_to_frame_ready_and_clear_acknowledges() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        assert!(!gba.is_ready_to_render());

        for _ in 0..400_000 {
            let _ = gba.run_tick();
            if gba.is_ready_to_render() {
                break;
            }
        }

        assert!(gba.is_ready_to_render(), "a frame should become ready");
        gba.clear_ready_to_render();
        assert!(!gba.is_ready_to_render());
    }

    #[test]
    fn test_run_tick_uses_cpu_instruction_cycles_not_fixed_chunk() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        let cycles = gba.run_tick();
        assert!(
            cycles <= 16,
            "run_tick should return per-instruction cycles, got {cycles}"
        );
    }

    #[test]
    fn test_reset_with_loaded_rom_rewinds_cpu_to_cart_start() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        let _ = gba.run_tick();
        let _ = gba.run_tick();
        assert_ne!(gba.cpu.regs.r[15], 0x0000_0000);

        gba.reset(true);
        assert_eq!(gba.cpu.regs.r[15], 0x0000_0000);

        let _ = gba.run_tick();
        gba.reset(false);
        assert_eq!(gba.cpu.regs.r[15], 0x0000_0000);
    }

    #[test]
    fn test_reset_without_rom_rewinds_cpu_to_reset_vector() {
        let mut gba = make_gba();

        let _ = gba.run_tick();
        gba.cpu.regs.r[15] = 0x0800_0000;
        gba.reset(false);

        assert_eq!(gba.cpu.regs.r[15], 0x0000_0000);
    }

    #[test]
    fn test_current_instruction_trace_line_reports_pc_and_raw_opcode() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");

        let line = gba.current_instruction_trace_line();
        assert!(
            line.is_some(),
            "trace line should be available when ROM is loaded"
        );
        let line = line.unwrap_or_default();
        assert!(line.contains("PC=00000000"));
        assert!(line.contains("RAW="));
        #[cfg(not(target_arch = "wasm32"))]
        assert!(line.contains("ASM="));
    }

    #[test]
    fn test_run_tick_emits_gba_cpu_trace_from_gba_config() {
        let mut config = Config::default();
        config.frontend.tracing.cpu = 0;
        config.gba.tracing.cpu = 1;
        let mut gba = make_gba_with_config(config);
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");

        clear_gba_cpu_trace_lines_for_tests();
        let _ = gba.run_tick();
        let lines = take_gba_cpu_trace_lines_for_tests();

        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("[GBA CPU] GBA ARM"));
        assert!(lines[0].contains("PC=00000000"));
        assert!(lines[0].contains("RAW="));
    }

    #[test]
    fn test_run_tick_emits_gba_swi_trace_from_gba_config() {
        let mut config = Config::default();
        config.gba.tracing.swi = 1;
        let mut gba = make_gba_with_config(config);
        let mut bios = vec![0u8; crate::gba::bus::memory::BIOS_SIZE];
        bios[..4].copy_from_slice(&0xEF12_0000u32.to_le_bytes());
        gba.bus.load_bios(&bios);
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");

        clear_gba_cpu_trace_lines_for_tests();
        let _ = gba.run_tick();
        let lines = take_gba_cpu_trace_lines_for_tests();

        assert_eq!(lines, vec!["[GBA SWI] ARM PC=00000000 IMM=12".to_string()]);
    }

    #[test]
    fn test_new_copies_gba_trace_config_to_bus() {
        let mut config = Config::default();
        config.gba.tracing.bus = 1;
        config.gba.tracing.dma = 2;
        let gba = make_gba_with_config(config);

        let tracing = gba.bus().trace_config_for_tests();
        assert_eq!(tracing.bus, 1);
        assert_eq!(tracing.dma, 2);
    }

    #[test]
    fn test_run_tick_mode0_executes_without_panicking() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        // Default DISPCNT mode after reset is 0.

        let cycles = gba.run_tick();
        assert!(
            cycles <= 16,
            "mode 0 should execute normally, got {cycles} cycles"
        );
    }

    #[test]
    fn test_run_tick_mode3_executes_without_panicking() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        let cycles = gba.run_tick();
        assert!(
            cycles <= 16,
            "mode 3 should execute normally, got {cycles} cycles"
        );
    }

    #[test]
    fn test_run_tick_unimplemented_modes_render_backdrop() {
        // Modes 6 and 7 are "prohibited" per GBATek. They render OBJ sprites
        // over the backdrop (no BG layers), matching mGBA behavior.
        // Modes 1 and 5 are fully implemented but included to ensure they
        // don't regress to panicking either.
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");

        for mode in [1, 5, 6, 7] {
            gba.bus.write16(ppu::REG_DISPCNT, mode);
            let cycles = gba.run_tick();
            assert!(
                cycles <= 16,
                "mode {mode} should execute normally (backdrop), got {cycles} cycles"
            );
        }
    }

    #[test]
    fn test_run_tick_mode2_does_not_panic() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");

        // Mode 2 is now supported (backdrop-only rendering).
        gba.bus.write16(ppu::REG_DISPCNT, 2);
        let cycles = gba.run_tick();
        assert!(
            cycles <= 16,
            "mode 2 should execute normally, got {cycles} cycles"
        );
    }

    #[test]
    fn test_target_frame_duration() {
        let gba = make_gba();
        let duration = gba.target_frame_duration();
        // Should be approximately 16.743ms (59.73 Hz)
        assert!(duration.as_millis() >= 16 && duration.as_millis() <= 17);
    }

    #[test]
    fn test_save_state_returns_bytes() {
        let gba = make_gba();
        let result = gba.save_state_bytes();
        let bytes = result.expect("save_state_bytes should succeed");
        assert!(!bytes.is_empty(), "save state must contain data");
    }

    #[test]
    fn test_load_state_round_trips() {
        let mut gba = make_gba();
        let bytes = gba.save_state_bytes().expect("save should succeed");
        gba.load_state_bytes(&bytes)
            .expect("load_state_bytes should succeed");
    }

    #[test]
    fn test_load_state_rejects_invalid_data() {
        let mut gba = make_gba();
        let result = gba.load_state_bytes(b"not a valid save state");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_button_routes_to_keypad() {
        let mut gba = make_gba();
        // Default: no buttons pressed → bitmask is 0.
        assert_eq!(gba.get_joypad_button_states(0), 0);
        // Press A (id=0).
        gba.set_button(0, 0, true);
        assert_eq!(gba.get_joypad_button_states(0) & 0x01, 0x01);
        // Release.
        gba.set_button(0, 0, false);
        assert_eq!(gba.get_joypad_button_states(0), 0);
    }

    #[test]
    fn test_set_joypad_button_states_round_trips() {
        let mut gba = make_gba();
        gba.set_joypad_button_states(0, 0b1010_0101);
        assert_eq!(gba.get_joypad_button_states(0), 0b1010_0101);
    }

    #[test]
    fn test_unknown_button_id_does_not_panic() {
        let mut gba = make_gba();
        // L (id=8) and R (id=9) are GBA-only buttons.
        gba.set_button(0, 8, true);
        gba.set_button(0, 9, true);
        gba.set_button(0, 250, true);
    }

    #[test]
    fn test_embedded_bios_used_when_no_external_bios_configured() {
        // A Gba instance with no valid bios_path should fall back to the
        // embedded open-source BIOS and successfully load a ROM.
        let mut config = Config::default();
        // Point to a non-existent BIOS file so external loading fails
        let tmp = std::env::temp_dir().join("neser_nonexistent_bios_test.bin");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_minimal_valid_gba_rom();
        let result = gba.load_rom(&rom, "test.gba");
        assert!(
            result.is_ok(),
            "ROM loading should succeed with embedded BIOS as fallback, got: {:?}",
            result.err()
        );
        assert!(gba.bus().has_bios_image());
    }

    #[test]
    fn test_skip_bios_intro_config_writes_iwram_flag() {
        // When skip_bios_intro is true in config, load_rom should write 1
        // to IWRAM address 0x03007FFC so the BIOS skips the intro.
        let mut config = Config::default();
        config.gba.skip_bios_intro = true;
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid ROM");

        let flag = gba.bus_mut().read8(0x03007FFC);
        assert_eq!(flag, 1, "skip flag should be set at 0x03007FFC");
    }

    #[test]
    fn test_no_skip_bios_intro_leaves_iwram_flag_zero() {
        // When skip_bios_intro is false (default), IWRAM 0x03007FFC stays 0.
        let config = Config::default();
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid ROM");

        let flag = gba.bus_mut().read8(0x03007FFC);
        assert_eq!(flag, 0, "skip flag should NOT be set when config is false");
    }

    #[test]
    fn test_bios_path_embedded_forces_embedded_bios() {
        // Setting bios_path to "embedded" should use the embedded BIOS
        // even if an external BIOS file exists at the default path.
        let mut config = Config::default();
        config.gba.bios_path = Some("embedded".to_string());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_minimal_valid_gba_rom();
        let result = gba.load_rom(&rom, "test.gba");
        assert!(
            result.is_ok(),
            "ROM loading should succeed with 'embedded' BIOS path"
        );
        assert!(gba.bus().has_bios_image());
    }

    #[test]
    fn test_embedded_sentinel_loads_embedded_bios_bytes() {
        // When bios_path is "embedded", the actual BIOS bytes loaded into
        // memory must match our EMBEDDED_BIOS constant, even if a proprietary
        // BIOS exists at the default path (~/.neser/gba_bios.bin).
        let mut config = Config::default();
        config.gba.bios_path = Some("embedded".to_string());
        let gba = Gba::new(AppContext::new_with_config(config));

        // Compare the first 16 bytes of the loaded BIOS against EMBEDDED_BIOS.
        let embedded = crate::gba::bios::EMBEDDED_BIOS;
        for (i, &expected) in embedded.iter().enumerate().take(16) {
            assert_eq!(
                gba.bus().debug_read_bios(i),
                expected,
                "BIOS byte at offset {i} should match EMBEDDED_BIOS when 'embedded' is configured"
            );
        }
    }

    // ---------------------------------------------------------------
    // HALTCNT — Gba mediates halt between bus and CPU
    // ---------------------------------------------------------------

    #[test]
    fn test_haltcnt_write_halts_cpu_on_next_tick() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        // Precondition: CPU is not halted.
        assert!(!gba.cpu.is_halted(), "CPU should start running");

        // Software writes HALTCNT with bit 7 clear → halt mode.
        gba.bus.write8(0x0400_0301, 0x00);
        assert!(
            gba.bus.halt_requested(),
            "bus should have halt_requested set"
        );

        // Run one tick — Gba should pick up the flag and halt the CPU.
        gba.run_tick_for_tests();

        assert!(gba.cpu.is_halted(), "CPU should be halted after tick");
        assert!(
            !gba.bus.halt_requested(),
            "halt_requested should be cleared after Gba consumed it"
        );
    }

    #[test]
    fn test_halted_cpu_wakes_on_irq() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        // Enable IRQs in CPSR (clear I flag).
        gba.cpu.regs.cpsr &= !crate::gba::cpu::registers::FLAG_I;

        // Halt the CPU via HALTCNT.
        gba.bus.write8(0x0400_0301, 0x00);
        gba.run_tick_for_tests();
        assert!(gba.cpu.is_halted(), "CPU should be halted");

        // Enable VBlank interrupt in IE and IME.
        gba.bus.ic.write_ime(1);
        gba.bus.ic.write_ie(0x0001); // VBlank
        // Raise VBlank in IF.
        gba.bus.ic.raise(0x0001);

        // Run a tick — the IRQ should wake the CPU.
        gba.run_tick_for_tests();

        assert!(
            !gba.cpu.is_halted(),
            "CPU should wake from halt when IRQ fires"
        );
    }

    #[test]
    fn test_halted_cpu_wakes_when_irq_pending_but_cpu_irq_masked() {
        let mut gba = make_gba();
        let rom = make_minimal_valid_gba_rom();
        gba.load_rom(&rom, "test.gba").expect("valid GBA ROM");
        set_mode3_bg2_enabled(&mut gba);

        // BIOS SWI Halt runs in SVC with the CPU I bit set. HALT still exits
        // once IE & IF is non-zero; the IRQ handler is dispatched only after
        // BIOS returns and restores an unmasked CPSR.
        gba.cpu.regs.cpsr |= crate::gba::cpu::registers::FLAG_I;

        gba.bus.write8(0x0400_0301, 0x00);
        gba.run_tick_for_tests();
        assert!(gba.cpu.is_halted(), "CPU should be halted");

        gba.bus.ic.write_ime(1);
        gba.bus.ic.write_ie(0x0010); // Timer 1
        gba.bus.ic.raise(0x0010);

        gba.run_tick_for_tests();

        assert!(
            !gba.cpu.is_halted(),
            "HALT should exit when IE & IF is non-zero even if CPSR.I masks IRQ dispatch"
        );
    }
}
