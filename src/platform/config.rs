//! Generic frontend configuration.
//!
//! [`FrontendConfig`] holds configuration options that are not specific to any
//! emulated system — audio, video, window settings, autorun, debugging UI, etc.
//! System-specific configuration (e.g., NES hardware mode) lives in the
//! respective system module.

use crate::platform::autorun::{AutorunFormat, AutorunMode};
use crate::platform::debugging::Tracing;
use crate::platform::debugging::breakpoints::BreakpointKind;

/// CLI flag definition for help text and validation.
///
/// Used by both NES and GB config modules to declare their supported flags.
pub(crate) struct CliFlag {
    pub flag: &'static str,
    pub help: Option<&'static str>,
    pub has_value: bool,
}

/// RAM initialization mode for power-on/hard reset.
///
/// Controls how all emulated RAM (CPU, PRG, CHR, PPU nametable, and palette) is
/// initialized when the emulator powers on or performs a hard reset. This affects
/// hardware-accuracy and determinism for testing.
///
/// Soft resets preserve RAM contents and do not re-initialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamInitMode {
    /// Initialize all RAM to 0x00.
    ///
    /// Provides a clean, predictable startup state. Useful for debugging and
    /// testing, though not hardware-accurate (real NES hardware has random RAM on power-on).
    Zero,
    /// Initialize all RAM to pseudo-random values.
    ///
    /// Hardware-accurate: real NES consoles have unpredictable RAM contents on power-on.
    /// Each run will have different initial RAM values.
    Random,
    /// Initialize all RAM to pseudo-random values using a fixed seed.
    ///
    /// Combines hardware-accuracy with determinism: RAM appears random but is
    /// identical across runs with the same seed. Useful for reproducible testing
    /// of RAM-sensitive code paths.
    SeededRandom(u64),
}

/// Generic frontend configuration (not system-specific).
///
/// These settings control the emulator's frontend behavior: audio, video,
/// window management, autorun recording/playback, debugging UI, and
/// cartridge discovery.
#[derive(Debug, Clone)]
pub struct FrontendConfig {
    /// Whether audio is enabled.
    pub audio_enabled: bool,
    /// Whether VSync is enabled.
    pub vsync_enabled: bool,
    /// Whether gamepad support is enabled.
    pub gamepads_enabled: bool,
    /// Whether to run in fullscreen mode.
    pub fullscreen: bool,
    /// Which display to use for fullscreen (None = auto-select).
    pub fullscreen_display: Option<i32>,
    /// Path to shader preset file.
    pub shader_path: Option<String>,
    /// Whether to open debugger on startup.
    pub debugger_enabled: bool,
    /// Whether to load save-state on startup.
    pub load_state: bool,
    /// Tracing configuration.
    pub tracing: Tracing,
    /// Window height in pixels (windowed mode only).
    pub window_height: u32,
    /// Debugger window background opacity (0.1 = nearly transparent, 0.7 = opaque).
    pub debugger_alpha: f32,
    /// Optional ROM path from CLI positional argument.
    pub rom_path: Option<String>,
    /// Autorun mode (None, Record, or Playback).
    pub autorun_mode: AutorunMode,
    /// Whether to run in headless mode (no display, requires playback).
    pub autorun_headless: bool,
    /// Whether to extend an existing recording (requires record mode).
    pub autorun_extend: bool,
    /// Whether to overwrite an existing recording (requires record mode).
    pub autorun_overwrite: bool,
    /// Start playback from this checkpoint index (0-based, or negative for from-end).
    pub autorun_from_checkpoint: Option<i64>,
    /// Trim this many checkpoints from the end of the recording file and exit.
    pub autorun_trim_checkpoints: Option<usize>,
    /// Convert an existing autorun file to the latest format and exit.
    pub autorun_convert: bool,
    /// Recalculate checkpoint CRCs in an existing autorun file and exit.
    pub autorun_recalculate: bool,
    /// Serialization format used when saving autorun files (default: binary).
    pub autorun_format: AutorunFormat,
    /// Comma-separated configured search paths for cartridge discovery.
    pub cartridge_search_paths: Vec<String>,
    /// Whether startup cartridge scanning is enabled.
    pub scan_cartridges: bool,
    /// Whether to rebuild the cartridge catalog from scratch on startup.
    pub rebuild_cartridge_catalog: bool,
    /// Whether to launch the TUI ROM browser instead of the emulator.
    #[cfg_attr(not(feature = "tui"), allow(dead_code))]
    pub tui_mode: bool,
    /// RAM initialization mode for power-on/hard reset (generic across emulators).
    pub ram_init_mode: RamInitMode,
    /// Breakpoints to set on startup (from --breakpoint CLI flag).
    pub breakpoints: Vec<BreakpointKind>,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            audio_enabled: true,
            vsync_enabled: true,
            gamepads_enabled: true,
            fullscreen: false,
            fullscreen_display: None,
            shader_path: None,
            debugger_enabled: false,
            load_state: false,
            tracing: Tracing::default(),
            window_height: 896,
            debugger_alpha: 0.7,
            rom_path: None,
            autorun_mode: AutorunMode::None,
            autorun_headless: false,
            autorun_extend: false,
            autorun_overwrite: false,
            autorun_from_checkpoint: None,
            autorun_trim_checkpoints: None,
            autorun_convert: false,
            autorun_recalculate: false,
            autorun_format: AutorunFormat::Binary,
            cartridge_search_paths: Vec::new(),
            scan_cartridges: true,
            rebuild_cartridge_catalog: false,
            tui_mode: false,
            #[cfg(target_arch = "wasm32")]
            ram_init_mode: RamInitMode::Zero,
            #[cfg(not(target_arch = "wasm32"))]
            ram_init_mode: RamInitMode::Random,
            breakpoints: Vec::new(),
        }
    }
}

/// Full emulator configuration (frontend + system-specific).
///
/// Composed of [`FrontendConfig`] (generic frontend settings),
/// [`NesConfig`](crate::nes::console::NesConfig) (NES hardware-specific settings),
/// and [`GbConfig`](crate::gb::console::config::GbConfig) (Game Boy-specific settings).
/// Parsing from CLI arguments and config files populates all sub-configs.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Generic frontend configuration.
    pub frontend: FrontendConfig,
    /// NES-specific hardware configuration.
    pub nes: crate::nes::console::NesConfig,
    /// Game Boy-specific hardware configuration.
    pub gb: crate::gb::console::config::GbConfig,
}

/// Look up the value for a CLI flag in an argument list.
///
/// Handles both `--flag value` and `--flag=value` forms. Returns `None` if
/// the flag is not present.
pub(crate) fn parse_cli_string_arg(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find_map(|w| (w[0] == flag).then(|| w[1].clone()))
        .or_else(|| {
            args.iter().find_map(|arg| {
                arg.split_once('=')
                    .filter(|(f, _)| *f == flag)
                    .map(|(_, v)| v.to_string())
            })
        })
}
