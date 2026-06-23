//! Generic frontend configuration.
//!
//! [`FrontendConfig`] holds configuration options that are not specific to any
//! emulated system — audio, video, window settings, autorun, debugging UI, etc.
//! System-specific configuration (e.g., NES hardware mode) lives in the
//! respective system module.

use crate::platform::autorun::{AutorunFormat, AutorunMode};
use crate::platform::debugging::Tracing;
use crate::platform::debugging::breakpoints::BreakpointKind;

pub mod cli;

mod audio;
mod autorun;
mod cartridge;
mod debugger;
mod video;

pub use cli::ParseResult;
pub(crate) use cli::{
    CliFlag, OPTIONAL_BOOL_FLAGS, all_cli_flags, has_negation_flag, parse_bool, parse_bool_arg,
    parse_cli_string_arg, parse_hex_u8, parse_u32_arg, print_help, validate_args,
};

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
    /// Target native audio buffering in milliseconds.
    pub audio_buffer_ms: u32,
    /// Target native audio sample rate in Hz.
    pub audio_sample_rate: u32,
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
    /// Path to the TheGamesDB metadata SQLite database.
    /// Default: `~/.neser/metadata.db`
    pub metadata_db_path: Option<String>,
    /// Path to the directory where downloaded cover art images are cached.
    /// Default: `~/.neser/image_cache/`
    pub image_cache_path: Option<String>,
    /// Whether to include unofficial ROMs (hacks, homebrew, etc.) in the catalog.
    /// Default: `false` (exclude unofficial ROMs).
    pub include_unofficial_roms: bool,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            audio_sample_rate: 44_100,
            audio_enabled: true,
            audio_buffer_ms: 60,
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
            metadata_db_path: None,
            image_cache_path: None,
            include_unofficial_roms: false,
        }
    }
}

/// Full emulator configuration (frontend + system-specific).
///
/// Composed of [`FrontendConfig`] (generic frontend settings),
/// [`NesConfig`](crate::nes::console::NesConfig) (NES hardware-specific settings),
/// [`GbConfig`](crate::gb::console::config::GbConfig) (Game Boy-specific settings),
/// [`GbaConfig`](crate::gba::console::config::GbaConfig) (GBA-specific settings),
/// and [`SnesConfig`](crate::snes::console::config::SnesConfig) (SNES-specific settings).
/// Parsing from CLI arguments and config files populates all sub-configs.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Generic frontend configuration.
    pub frontend: FrontendConfig,
    /// NES-specific hardware configuration.
    pub nes: crate::nes::console::NesConfig,
    /// Game Boy-specific hardware configuration.
    pub gb: crate::gb::console::config::GbConfig,
    /// GBA-specific hardware configuration.
    pub gba: crate::gba::console::config::GbaConfig,
    /// SNES-specific hardware configuration.
    pub snes: crate::snes::console::config::SnesConfig,
}

impl FrontendConfig {
    /// Apply command-line arguments to frontend configuration.
    ///
    /// Parses platform-level CLI flags (audio, vsync, fullscreen, display, window,
    /// debugger, gamepads, load-state, ram-init, breakpoints, TUI, cartridge discovery,
    /// autorun, tracing).
    ///
    /// Note: ROM path and shader path parsing remain in Config::apply_args() for now
    /// (they require complex flag validation logic).
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        // Boolean flags (support both value-based and prefix negation)
        audio::apply_args(self, args)?;
        video::apply_args(self, args)?;

        // Gamepads: --gamepads true/false
        if let Some(gamepads) = parse_bool_arg(args, "--gamepads")? {
            self.gamepads_enabled = gamepads;
        }

        debugger::apply_args(self, args)?;

        // RAM initialization mode
        let cli_ram_init_mode = parse_cli_string_arg(args, "--ram-init-mode");
        if let Some(value) = cli_ram_init_mode.as_ref() {
            self.apply_config_value("ram_init_mode", value)?;
        }

        // Cartridge catalog arguments
        cartridge::apply_args(self, args)?;

        // TUI mode
        #[cfg(feature = "tui")]
        if args.iter().any(|arg| arg == "--tui") {
            self.tui_mode = true;
        }

        #[cfg(not(feature = "tui"))]
        if args.iter().any(|arg| arg == "--tui") {
            return Err("--tui requires the `tui` feature (build with --features tui)".to_string());
        }

        autorun::apply_args(self, args, cli_ram_init_mode.as_deref())?;

        Ok(())
    }

    /// Apply a single config file key-value pair to frontend configuration.
    ///
    /// Handles platform-level config keys (audio, vsync, fullscreen, display,
    /// window_height, debugger_alpha, tracing keys, ram_init_mode, etc.).
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        if audio::apply_config_value(self, &key, value)? {
            return Ok(());
        }
        if video::apply_config_value(self, &key, value)? {
            return Ok(());
        }
        if debugger::apply_config_value(self, &key, value)? {
            return Ok(());
        }
        if cartridge::apply_config_value(self, &key, value)? {
            return Ok(());
        }
        match key.as_str() {
            "gamepads" => {
                if let Ok(b) = parse_bool(value) {
                    self.gamepads_enabled = b;
                }
            }
            "ram_init_mode" => match value.to_lowercase().as_str() {
                "zero" | "0" => self.ram_init_mode = RamInitMode::Zero,
                "random" => self.ram_init_mode = RamInitMode::Random,
                _ => {
                    // Accept both "seeded-random:N" and "seeded_random:N" for compatibility
                    if let Some(seed_str) = value
                        .strip_prefix("seeded-random:")
                        .or_else(|| value.strip_prefix("seeded_random:"))
                        .or_else(|| value.strip_prefix("seeded:"))
                    {
                        if let Ok(seed) = seed_str.parse::<u64>() {
                            self.ram_init_mode = RamInitMode::SeededRandom(seed);
                        } else {
                            eprintln!(
                                "Warning: invalid seed '{}' for 'ram_init_mode'; \
                                 keeping current mode. Use format 'seeded-random:12345'.",
                                seed_str
                            );
                        }
                    } else {
                        eprintln!(
                            "Warning: invalid value '{}' for 'ram_init_mode'; \
                             keeping current mode. Valid values: zero, random, seeded-random:SEED",
                            value
                        );
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::ParseResult;
    use crate::nes::console::Config;

    pub(crate) fn config_new(mut args: Vec<String>) -> Result<ParseResult, String> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        if args.iter().any(|a| a == "--config") {
            return Config::new(&args);
        }

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        args.push("--config".to_string());
        args.push(file.path().to_string_lossy().to_string());

        Config::new(&args)
    }

    pub(crate) fn parse_config(args: Vec<String>) -> Config {
        match config_new(args).unwrap() {
            ParseResult::Config(c) => *c,
            ParseResult::Help => panic!("Expected Config, got Help"),
            ParseResult::Version => panic!("Expected Config, got Version"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{config_new, parse_config};
    use super::*;
    use crate::nes::console::{Config, HardwareModel};

    // Help tests
    #[test]
    fn test_config_help_flag() {
        let args = vec!["neser".to_string(), "--help".to_string()];
        match config_new(args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Version => panic!("Expected Help, got Version"),
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_config_help_flag_short() {
        let args = vec!["neser".to_string(), "-h".to_string()];
        match config_new(args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Version => panic!("Expected Help, got Version"),
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_config_version_flag_returns_version_before_validation() {
        let args = vec![
            "neser".to_string(),
            "--version".to_string(),
            "--not-a-real-flag".to_string(),
        ];

        match Config::new(&args).unwrap() {
            ParseResult::Version => {}
            ParseResult::Help => panic!("Expected Version, got Help"),
            ParseResult::Config(_) => panic!("Expected Version, got Config"),
        }
    }

    #[test]
    fn test_help_text_lists_version_flag() {
        let help = cli::help_text();

        assert!(help.contains("--version"));
        assert!(help.contains("Print version information and exit"));
    }

    #[test]
    fn test_help_text_groups_flags_into_readable_sections() {
        let help = cli::help_text();

        assert!(help.contains("\nInput:"));
        assert!(help.contains("\nTrace and Debugging:"));
        assert!(help.contains("\nSound:"));
        assert!(help.contains("\nVideo and Display:"));
        assert!(help.contains("\nAutorun:"));
        assert!(help.contains("\nCartridge Catalog:"));

        let input_section = help.find("\nInput:").unwrap();
        let input_flag = help.find("--nes-controller-port1").unwrap();
        assert!(input_section < input_flag);

        let trace_section = help.find("\nTrace and Debugging:").unwrap();
        let trace_flag = help.find("--trace-cpu").unwrap();
        assert!(trace_section < trace_flag);

        let sound_section = help.find("\nSound:").unwrap();
        let sound_flag = help.find("--audio").unwrap();
        assert!(sound_section < sound_flag);
    }

    #[test]
    fn test_help_text_load_state_is_presence_only_flag() {
        let help = cli::help_text();

        assert!(help.contains("--load-state"));
        assert!(!help.contains("--no-load-state"));
        assert!(!help.contains("--disable-load-state"));
    }

    #[test]
    fn test_help_text_lists_audio_buffer_ms_flag() {
        let help = cli::help_text();

        assert!(help.contains("--audio-buffer-ms"));
        assert!(help.contains("Target native audio buffering in milliseconds"));
    }

    #[test]
    fn test_help_text_lists_audio_sample_rate_flag() {
        let help = cli::help_text();

        assert!(help.contains("--audio-sample-rate"));
        assert!(help.contains("Target native audio sample rate in Hz"));
        assert!(help.contains("22050, 44100, 48000, 96000, 192000"));
        assert!(help.contains("default: 44100"));
    }

    #[test]
    fn test_help_text_oam_dram_decay_shows_default_and_no_negation_aliases() {
        let help = cli::help_text();

        assert!(help.contains("--nes-oam-dram-decay"));
        assert!(help.contains("default: false"));
        assert!(!help.contains("--no-oam-dram-decay"));
        assert!(!help.contains("--disable-oam-dram-decay"));
    }

    #[test]
    fn test_help_text_examples_use_hardware_flag() {
        let help = cli::help_text();

        assert!(help.contains("neser --nes-hardware nes-pal game.nes"));
        assert!(!help.contains("--tv-system"));
    }

    // Audio/Video/Input CLI tests
    #[test]
    fn test_config_vsync_false() {
        let args = vec![
            "neser".to_string(),
            "--vsync".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_gamepads_false() {
        let args = vec![
            "neser".to_string(),
            "--gamepads".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.gamepads_enabled);
    }

    #[test]
    fn test_config_fullscreen_true() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, None);
    }

    #[test]
    fn test_config_load_state_flag() {
        let args = vec!["neser".to_string(), "--load-state".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.load_state);
    }

    #[test]
    fn test_config_fullscreen_with_display() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, Some(1));
    }

    #[test]
    fn test_config_display_without_fullscreen_is_ignored() {
        let args = vec![
            "neser".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, None);
    }

    #[test]
    fn test_config_display_missing_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_display_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "abc".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_display_negative_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "-1".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_cmdline_filter_invalid_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "invalid-filter".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-filter'. Valid options are: none, crt, smooth, ntsc, pal"
        );
    }

    #[test]
    fn test_config_debugger_enabled() {
        let args = vec![
            "neser".to_string(),
            "--debugger".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_unknown_argument_errors() {
        let args = vec![
            "neser".to_string(),
            "--definitely-not-a-real-flag".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_positional_argument_is_rom_path() {
        let args = vec!["neser".to_string(), "somefile.nes".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("somefile.nes"));
    }

    // Tracing CLI tests
    #[test]
    fn test_config_tracing_enabled() {
        let args = vec!["neser".to_string(), "--trace".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 1); // --trace enables CPU tracing at level 1
    }

    #[test]
    fn test_config_tracing_nestest() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert!(config.frontend.tracing.nestest);
    }

    #[test]
    fn test_config_tracing_cpu() {
        let args = vec!["neser".to_string(), "--trace-cpu".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 1);
    }

    #[test]
    fn test_config_tracing_ppu() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 1);
    }

    #[test]
    fn test_config_tracing_apu() {
        let args = vec!["neser".to_string(), "--trace-apu".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.apu, 1);
    }

    #[test]
    fn test_config_tracing_mapper() {
        let args = vec!["neser".to_string(), "--trace-mapper".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 1);
    }

    #[test]
    fn test_config_tracing_cpu_with_level() {
        let args = vec!["neser".to_string(), "--trace-cpu=2".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 2);
    }

    #[test]
    fn test_config_tracing_ppu_with_level() {
        let args = vec!["neser".to_string(), "--trace-ppu=3".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 3);
    }

    #[test]
    fn test_config_tracing_ppu_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-ppu=9".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 5);
    }

    #[test]
    fn test_config_tracing_apu_with_level() {
        let args = vec!["neser".to_string(), "--trace-apu=4".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.apu, 4);
    }

    #[test]
    fn test_config_tracing_mapper_with_level() {
        let args = vec!["neser".to_string(), "--trace-mapper=5".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 5);
    }

    #[test]
    fn test_config_tracing_mapper_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-mapper=9".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 5);
    }

    #[test]
    fn test_config_tracing_with_multiple_levels() {
        let args = vec![
            "neser".to_string(),
            "--trace-cpu=3".to_string(),
            "--trace-ppu=2".to_string(),
            "--trace-apu=1".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 3);
        assert_eq!(config.frontend.tracing.ppu, 2);
        assert_eq!(config.frontend.tracing.apu, 1);
        assert_eq!(config.frontend.tracing.mapper, 0);
    }

    #[test]
    fn test_config_multiple_flags() {
        use crate::nes::console::ApuChannels;
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "nes-pal".to_string(),
            "--audio".to_string(),
            "false".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
            "--display".to_string(),
            "2".to_string(),
            "--nes-pulse1".to_string(),
            "false".to_string(),
            "--nes-noise".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, Some(2));
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.nes.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_window_height() {
        let args = vec![
            "neser".to_string(),
            "--window-height".to_string(),
            "720".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.window_height, 720);
    }

    #[test]
    fn test_config_window_height_invalid_errors() {
        let args = vec![
            "neser".to_string(),
            "--window-height".to_string(),
            "not_a_number".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    // Config file tests
    // Note: Config file tests that use apply_config_value remain in nes::console::config
    // since apply_config_value is private and config file parsing is orchestrated there.
    // These CLI tests verify the platform flags work correctly.

    #[test]
    fn test_config_metadata_db_path_defaults_to_none() {
        let config = parse_config(vec!["neser".to_string(), "game.nes".to_string()]);
        assert!(config.frontend.metadata_db_path.is_none());
    }

    #[test]
    fn test_config_metadata_db_path_from_cli() {
        let config = parse_config(vec![
            "neser".to_string(),
            "--metadata-db-path".to_string(),
            "/custom/metadata.db".to_string(),
            "game.nes".to_string(),
        ]);
        assert_eq!(
            config.frontend.metadata_db_path.as_deref(),
            Some("/custom/metadata.db")
        );
    }

    #[test]
    fn test_config_image_cache_path_defaults_to_none() {
        let config = parse_config(vec!["neser".to_string(), "game.nes".to_string()]);
        assert!(config.frontend.image_cache_path.is_none());
    }

    #[test]
    fn test_config_image_cache_path_from_cli() {
        let config = parse_config(vec![
            "neser".to_string(),
            "--image-cache-path".to_string(),
            "/custom/cache".to_string(),
            "game.nes".to_string(),
        ]);
        assert_eq!(
            config.frontend.image_cache_path.as_deref(),
            Some("/custom/cache")
        );
    }

    #[test]
    fn test_config_metadata_db_path_from_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"metadata_db_path=/from/config/metadata.db\n")
            .unwrap();

        let config = parse_config(vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "game.nes".to_string(),
        ]);
        assert_eq!(
            config.frontend.metadata_db_path.as_deref(),
            Some("/from/config/metadata.db")
        );
    }

    #[test]
    fn test_config_image_cache_path_from_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"image_cache_path=/from/config/cache\n")
            .unwrap();

        let config = parse_config(vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "game.nes".to_string(),
        ]);
        assert_eq!(
            config.frontend.image_cache_path.as_deref(),
            Some("/from/config/cache")
        );
    }

    #[test]
    fn test_config_cli_overrides_config_file_metadata_db_path() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"metadata_db_path=/from/config/metadata.db\n")
            .unwrap();

        let config = parse_config(vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--metadata-db-path".to_string(),
            "/from/cli/metadata.db".to_string(),
            "game.nes".to_string(),
        ]);
        assert_eq!(
            config.frontend.metadata_db_path.as_deref(),
            Some("/from/cli/metadata.db")
        );
    }

    #[test]
    fn test_resolved_metadata_db_path_uses_configured_value() {
        let cfg = FrontendConfig {
            metadata_db_path: Some("/custom/metadata.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            cfg.resolved_metadata_db_path(),
            std::path::PathBuf::from("/custom/metadata.db")
        );
    }

    #[test]
    fn test_resolved_metadata_db_path_falls_back_to_default() {
        let cfg = FrontendConfig::default();
        let path = cfg.resolved_metadata_db_path();
        assert!(
            path.ends_with(".neser/metadata.db"),
            "expected path ending with .neser/metadata.db, got: {path:?}"
        );
    }

    #[test]
    fn test_resolved_image_cache_path_uses_configured_value() {
        let cfg = FrontendConfig {
            image_cache_path: Some("/custom/cache".to_string()),
            ..Default::default()
        };
        assert_eq!(
            cfg.resolved_image_cache_path(),
            std::path::PathBuf::from("/custom/cache")
        );
    }

    #[test]
    fn test_resolved_image_cache_path_falls_back_to_default() {
        let cfg = FrontendConfig::default();
        let path = cfg.resolved_image_cache_path();
        assert!(
            path.ends_with(".neser/image_cache"),
            "expected path ending with .neser/image_cache, got: {path:?}"
        );
    }

    #[test]
    fn test_apply_config_value_accepts_dashes_for_underscore_keys() {
        let mut cfg = FrontendConfig::default();
        cfg.apply_config_value("cartridge-search-paths", "/tmp/roms")
            .unwrap();
        assert_eq!(cfg.cartridge_search_paths, vec!["/tmp/roms"]);

        cfg.apply_config_value("metadata-db-path", "/tmp/meta.db")
            .unwrap();
        assert_eq!(cfg.metadata_db_path.as_deref(), Some("/tmp/meta.db"));

        cfg.apply_config_value("image-cache-path", "/tmp/cache")
            .unwrap();
        assert_eq!(cfg.image_cache_path.as_deref(), Some("/tmp/cache"));

        cfg.apply_config_value("include-unofficial-roms", "true")
            .unwrap();
        assert!(cfg.include_unofficial_roms);
    }
}
