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
        // Audio: --audio true/false, --no-audio, --disable-audio
        if let Some(audio) = parse_bool_arg(args, "--audio")? {
            self.audio_enabled = audio;
        }
        if has_negation_flag(args, &["--no-audio", "--disable-audio"]) {
            self.audio_enabled = false;
        }

        // VSync: --vsync true/false, --no-vsync, --disable-vsync
        if let Some(vsync) = parse_bool_arg(args, "--vsync")? {
            self.vsync_enabled = vsync;
        }
        if has_negation_flag(args, &["--no-vsync", "--disable-vsync"]) {
            self.vsync_enabled = false;
        }

        // Gamepads: --gamepads true/false
        if let Some(gamepads) = parse_bool_arg(args, "--gamepads")? {
            self.gamepads_enabled = gamepads;
        }

        // Debugger: --debugger true/false
        if let Some(debugger) = parse_bool_arg(args, "--debugger")? {
            self.debugger_enabled = debugger;
        }

        // Load state: --load-state true/false
        if let Some(load_state) = parse_bool_arg(args, "--load-state")? {
            self.load_state = load_state;
        }

        // Fullscreen (value-based)
        if let Some(fullscreen) = parse_bool_arg(args, "--fullscreen")? {
            self.fullscreen = fullscreen;
        }

        // Tracing (merge with existing config file values)
        self.tracing.apply_args(args);

        // Window height
        if let Some(height) = parse_u32_arg(args, "--window-height")? {
            self.window_height = height;
        }

        // Debugger alpha
        if let Some(alpha) = parse_f32_arg(args, "--debugger-alpha")? {
            self.debugger_alpha = alpha.clamp(0.1, 1.0);
        }

        // RAM initialization mode
        let cli_ram_init_mode = parse_cli_string_arg(args, "--ram-init-mode");
        if let Some(value) = cli_ram_init_mode.as_ref() {
            self.apply_config_value("ram_init_mode", value)?;
        }

        // Cartridge catalog arguments
        self.apply_cartridge_catalog_args(args)?;

        // TUI mode
        #[cfg(feature = "tui")]
        if args.iter().any(|arg| arg == "--tui") {
            self.tui_mode = true;
        }

        #[cfg(not(feature = "tui"))]
        if args.iter().any(|arg| arg == "--tui") {
            return Err("--tui requires the `tui` feature (build with --features tui)".to_string());
        }

        // Autorun mode flags
        let has_create_recording = args.iter().any(|arg| arg == "--create-recording");
        let has_extend_recording = args.iter().any(|arg| arg == "--extend-recording");
        let has_playback = args.iter().any(|arg| arg == "--playback");
        let has_playback_headless = args.iter().any(|arg| arg == "--playback-headless");

        if has_create_recording && has_extend_recording {
            return Err(
                "Cannot specify both --create-recording and --extend-recording".to_string(),
            );
        }
        if (has_create_recording || has_extend_recording) && (has_playback || has_playback_headless)
        {
            return Err("Cannot specify both a recording flag and a playback flag".to_string());
        }

        if has_create_recording {
            self.autorun_mode = AutorunMode::Record;
            self.autorun_overwrite = true;
        } else if has_extend_recording {
            self.autorun_mode = AutorunMode::Record;
            self.autorun_extend = true;
        } else if has_playback || has_playback_headless {
            self.autorun_mode = AutorunMode::Playback;
            self.autorun_headless = has_playback_headless;
        }

        if let Some(v) = parse_i64_arg(args, "--playback-from-checkpoint")? {
            self.autorun_from_checkpoint = Some(v);
            // Implies playback mode if no explicit mode was set
            if self.autorun_mode == AutorunMode::None {
                self.autorun_mode = AutorunMode::Playback;
            }
        }

        if let Some(v) = parse_i64_arg(args, "--playback-headless-from-checkpoint")? {
            self.autorun_from_checkpoint = Some(v);
            self.autorun_mode = AutorunMode::Playback;
            self.autorun_headless = true;
        }

        if let Some(v) = parse_u32_arg(args, "--trim-checkpoints")? {
            self.autorun_trim_checkpoints = Some(v as usize);
        }

        if let Some(convert_autorun_requested) = parse_bool_arg(args, "--convert-autorun")? {
            self.autorun_convert = convert_autorun_requested;
        }

        if let Some(recalculate_autorun_requested) = parse_bool_arg(args, "--recalculate-autorun")?
        {
            self.autorun_recalculate = recalculate_autorun_requested;
        }

        if let Some(format_str) = parse_cli_string_arg(args, "--autorun-format") {
            self.autorun_format = match format_str.as_str() {
                "binary" => AutorunFormat::Binary,
                "json" => AutorunFormat::Json,
                other => {
                    return Err(format!(
                        "Unknown autorun format '{other}': expected 'binary' or 'json'"
                    ));
                }
            };
        }

        // Autorun validation
        if self.autorun_trim_checkpoints.is_some() && self.autorun_convert {
            return Err("Cannot specify both --trim-checkpoints and --convert-autorun".to_string());
        }

        if self.autorun_trim_checkpoints.is_some() && self.autorun_recalculate {
            return Err(
                "Cannot specify both --trim-checkpoints and --recalculate-autorun".to_string(),
            );
        }

        if self.autorun_convert && self.autorun_recalculate {
            return Err(
                "Cannot specify both --convert-autorun and --recalculate-autorun".to_string(),
            );
        }

        if self.autorun_recalculate && self.autorun_mode != AutorunMode::None {
            return Err(
                "Cannot combine --recalculate-autorun with recording/playback flags".to_string(),
            );
        }

        if self.autorun_recalculate && self.autorun_from_checkpoint.is_some() {
            return Err(
                "Cannot combine --recalculate-autorun with checkpoint playback flags".to_string(),
            );
        }

        // Autorun recording/playback must be deterministic.
        // Force zero-initialized RAM when autorun is active, and reject an explicit
        // non-zero CLI --ram-init-mode for these modes.
        if self.autorun_mode != AutorunMode::None || self.autorun_recalculate {
            if let Some(value) = cli_ram_init_mode.as_ref()
                && !value.eq_ignore_ascii_case("zero")
            {
                return Err("Autorun recording/playback requires --ram-init-mode zero".to_string());
            }
            self.ram_init_mode = RamInitMode::Zero;
        }

        // Breakpoints from --breakpoint flag (comma-separated list)
        if let Some(value) = parse_cli_string_arg(args, "--breakpoint") {
            self.breakpoints =
                parse_breakpoint_list(&value).map_err(|e| format!("--breakpoint: {e}"))?;
        }

        Ok(())
    }

    /// Apply a single config file key-value pair to frontend configuration.
    ///
    /// Handles platform-level config keys (audio, vsync, fullscreen, display,
    /// window_height, debugger_alpha, tracing keys, ram_init_mode, etc.).
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "audio" => {
                if let Ok(b) = parse_bool(value) {
                    self.audio_enabled = b;
                }
            }
            "vsync" => {
                if let Ok(b) = parse_bool(value) {
                    self.vsync_enabled = b;
                }
            }
            "gamepads" => {
                if let Ok(b) = parse_bool(value) {
                    self.gamepads_enabled = b;
                }
            }
            "fullscreen" => {
                if let Ok(b) = parse_bool(value) {
                    self.fullscreen = b;
                }
            }
            "display" => {
                if let Ok(d) = value.parse::<i32>()
                    && d >= 0
                {
                    self.fullscreen_display = Some(d);
                }
            }
            "debugger" => {
                if let Ok(b) = parse_bool(value) {
                    self.debugger_enabled = b;
                }
            }
            "load_state" => {
                if let Ok(b) = parse_bool(value) {
                    self.load_state = b;
                }
            }
            "window_height" => {
                if let Ok(s) = value.parse::<u32>() {
                    self.window_height = s;
                }
            }
            "debugger_alpha" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.debugger_alpha = v.clamp(0.1, 1.0);
                }
            }
            "trace-cpu" => {
                if let Ok(level) = value.parse::<u8>() {
                    self.tracing.cpu = level;
                    if level > 0 {
                        self.tracing.enabled = true;
                    }
                }
            }
            "trace-ppu" => {
                if let Ok(level) = value.parse::<u8>() {
                    self.tracing.ppu = Tracing::clamp_ppu_level(level);
                    if level > 0 {
                        self.tracing.enabled = true;
                    }
                }
            }
            "trace-apu" => {
                if let Ok(level) = value.parse::<u8>() {
                    self.tracing.apu = level;
                    if level > 0 {
                        self.tracing.enabled = true;
                    }
                }
            }
            "trace-mapper" => {
                if let Ok(level) = value.parse::<u8>() {
                    self.tracing.mapper = Tracing::clamp_mapper_level(level);
                    if level > 0 {
                        self.tracing.enabled = true;
                    }
                }
            }
            "trace-nestest" => {
                if let Ok(b) = parse_bool(value) {
                    self.tracing.nestest = b;
                    if b {
                        self.tracing.enabled = true;
                    }
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
            "cartridge_search_paths" => {
                self.cartridge_search_paths = parse_search_paths(value);
            }
            "scan_cartridges" => {
                if let Ok(scan) = parse_bool(value) {
                    self.scan_cartridges = scan;
                }
            }
            "rebuild_cartridge_catalog" => {
                if let Ok(rebuild) = parse_bool(value) {
                    self.rebuild_cartridge_catalog = rebuild;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Parse and apply cartridge catalog arguments.
    fn apply_cartridge_catalog_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(paths) = parse_cli_string_arg(args, "--cartridge-search-paths") {
            self.cartridge_search_paths = parse_search_paths(&paths);
        }

        if let Some(scan) = parse_bool_arg(args, "--scan-cartridges")? {
            self.scan_cartridges = scan;
        }
        if has_negation_flag(args, &["--no-scan-cartridges"]) {
            self.scan_cartridges = false;
        }

        if args.iter().any(|arg| arg == "--rebuild-cartridge-catalog") {
            self.rebuild_cartridge_catalog = true;
        }

        Ok(())
    }
}

/// Result of parsing command-line arguments.
#[derive(Debug)]
pub enum ParseResult {
    /// User requested help - print and exit.
    Help,
    /// Successfully parsed configuration.
    Config(Box<Config>),
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

// ============================================================================
// CLI Flag Definitions
// ============================================================================

/// Shared CLI flags used by the platform configuration layer.
///
/// This list primarily covers frontend behavior such as audio, display,
/// debugging, and autorun. It may also include shared validation entries for
/// flags that are only meaningful for specific systems (for example,
/// NES-focused debugging or filtering flags). System-specific configuration
/// options are still defined in their respective config modules
/// (e.g., `--nes-hardware` in `NesConfig`).
pub(crate) const PLATFORM_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--help",
        help: None,
        has_value: false,
    },
    CliFlag {
        flag: "-h",
        help: None,
        has_value: false,
    },
    CliFlag {
        flag: "--trace",
        help: Some("Enable CPU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-cpu",
        help: Some("Enable CPU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--fullscreen",
        help: Some(
            "Run emulator in fullscreen mode (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--display",
        help: Some("Select display index for fullscreen (e.g., --display 1)"),
        has_value: true,
    },
    CliFlag {
        flag: "--config",
        help: Some("Specify config file path (default: ./neser.conf or ~/.neser/neser.conf)"),
        has_value: true,
    },
    CliFlag {
        flag: "--window-height",
        help: Some("Window height in pixels (windowed mode only, e.g., --window-height 896)"),
        has_value: true,
    },
    CliFlag {
        flag: "--debugger-alpha",
        help: Some("Debugger window opacity: 0.1 (transparent) to 1.0 (opaque, default: 0.7)"),
        has_value: true,
    },
    CliFlag {
        flag: "--audio",
        help: Some("Enable audio output (optionally: true/false, default when flag present: true)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-audio",
        help: Some("Disable audio output (equivalent to --audio false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-audio",
        help: Some("Disable audio output (equivalent to --audio false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--vsync",
        help: Some("Enable VSync (optionally: true/false, default when flag present: true)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-vsync",
        help: Some("Disable VSync (equivalent to --vsync false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-vsync",
        help: Some("Disable VSync (equivalent to --vsync false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--gamepads",
        help: Some(
            "Enable gamepad/joystick support (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--debugger",
        help: Some(
            "Open debugger windows (CPU/PPU/APU) on startup (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--load-state",
        help: Some("Load save-state on startup"),
        has_value: false,
    },
    CliFlag {
        flag: "--create-recording",
        help: Some("Record controller input to <ROM>.autorun file (replaces existing)"),
        has_value: false,
    },
    CliFlag {
        flag: "--extend-recording",
        help: Some("Extend an existing autorun recording with new input"),
        has_value: false,
    },
    CliFlag {
        flag: "--playback",
        help: Some("Play back controller input from <ROM>.autorun file"),
        has_value: false,
    },
    CliFlag {
        flag: "--playback-headless",
        help: Some("Play back controller input from <ROM>.autorun file without display"),
        has_value: false,
    },
    CliFlag {
        flag: "--playback-from-checkpoint",
        help: Some("Start playback from checkpoint N (0-based index, negative counts from end)"),
        has_value: true,
    },
    CliFlag {
        flag: "--playback-headless-from-checkpoint",
        help: Some("Headless playback from checkpoint N (no display; negative counts from end)"),
        has_value: true,
    },
    CliFlag {
        flag: "--trim-checkpoints",
        help: Some("Remove last N checkpoints (and their frames) from <ROM>.autorun file and exit"),
        has_value: true,
    },
    CliFlag {
        flag: "--convert-autorun",
        help: Some("Convert <ROM>.autorun file from older versions to the current format and exit"),
        has_value: false,
    },
    CliFlag {
        flag: "--recalculate-autorun",
        help: Some(
            "Run headless playback, recalculate checkpoint CRCs in <ROM>.autorun file, save, and exit",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--autorun-format",
        help: Some("Serialization format for autorun files: binary (default) or json"),
        has_value: true,
    },
    CliFlag {
        flag: "--ram-init-mode",
        help: Some("RAM initialization mode: zero, random, or seeded-random:SEED (default: zero)"),
        has_value: true,
    },
    CliFlag {
        flag: "--breakpoint",
        help: Some(
            "Add breakpoints on startup: pc=ADDR, cycle=N, frame=N, write=ADDR (comma-separated)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--cartridge-search-paths",
        help: Some("Comma-separated search paths to scan recursively for .nes files on startup"),
        has_value: true,
    },
    CliFlag {
        flag: "--scan-cartridges",
        help: Some(
            "Enable cartridge scanning on startup (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-scan-cartridges",
        help: Some("Disable cartridge scanning on startup (equivalent to --scan-cartridges false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--rebuild-cartridge-catalog",
        help: Some("Rebuild cartridge catalog from scratch on startup"),
        has_value: false,
    },
    CliFlag {
        flag: "--tui",
        help: Some("Launch the interactive TUI ROM browser (requires tui feature)"),
        has_value: false,
    },
];

// ============================================================================
// Parsing Helper Functions
// ============================================================================

/// Boolean flags that accept optional values (used by validation and ROM-path parsing).
pub(crate) const OPTIONAL_BOOL_FLAGS: &[&str] = &[
    "--nes-oam-dram-decay",
    "--audio",
    "--vsync",
    "--gamepads",
    "--nes-enable-4-score",
    "--nes-pulse1",
    "--nes-pulse2",
    "--nes-triangle",
    "--nes-noise",
    "--nes-dmc",
    "--debugger",
    "--load-state",
    "--fullscreen",
    "--scan-cartridges",
    "--convert-autorun",
    "--recalculate-autorun",
];

/// Parse a boolean argument from command-line args.
///
/// Supports three forms:
/// - `--flag` (no value) → defaults to true
/// - `--flag value` (space-separated) → parse value
/// - `--flag=value` (equals syntax) → parse value
///
/// Returns `Ok(None)` if flag not present, `Ok(Some(bool))` if valid, `Err(msg)` if invalid.
pub(crate) fn parse_bool_arg(args: &[String], flag: &str) -> Result<Option<bool>, String> {
    for i in 0..args.len() {
        // Check for --flag=value syntax
        if let Some((flag_part, value_part)) = args[i].split_once('=') {
            if flag_part == flag {
                match parse_bool(value_part) {
                    Ok(b) => return Ok(Some(b)),
                    Err(_) => {
                        return Err(format!(
                            "Invalid value for {flag}: '{}'. Expected: true, false, yes, no, 1, or 0",
                            value_part
                        ));
                    }
                }
            }
        }
        // Check for --flag (with or without value)
        else if args[i] == flag {
            // Check if next argument is a value or another flag/positional
            if i + 1 < args.len() {
                let next_arg = &args[i + 1];
                // If next arg looks like another flag, treat current flag as valueless (default to true)
                if next_arg.starts_with('-') {
                    return Ok(Some(true));
                }
                // Try to parse as boolean value
                match parse_bool(next_arg) {
                    Ok(b) => return Ok(Some(b)),
                    // If it doesn't parse as boolean, treat flag as valueless (default to true)
                    Err(_) => return Ok(Some(true)),
                }
            } else {
                // Flag is last argument, default to true
                return Ok(Some(true));
            }
        }
    }
    Ok(None)
}

/// Check if any of the negation flags are present in the arguments.
pub(crate) fn has_negation_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| flags.contains(&a.as_str()))
}

/// Parse a u32 argument from command-line args.
///
/// Supports both `--flag value` and `--flag=value` forms.
pub(crate) fn parse_u32_arg(args: &[String], flag: &str) -> Result<Option<u32>, String> {
    for i in 0..args.len() {
        // Handle `--flag value`
        if args[i] == flag && i + 1 < args.len() {
            let value = &args[i + 1];
            let parsed: u32 = value
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value))?;
            return Ok(Some(parsed));
        }
        // Handle `--flag=value`
        if let Some((flag_part, value_part)) = args[i].split_once('=')
            && flag_part == flag
        {
            let parsed: u32 = value_part
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value_part))?;
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

/// Parse an i64 argument from command-line args (supports negative values).
///
/// Supports both `--flag value` and `--flag=value` forms.
pub(crate) fn parse_i64_arg(args: &[String], flag: &str) -> Result<Option<i64>, String> {
    for i in 0..args.len() {
        // Handle `--flag value` (value may start with '-' for negatives)
        if args[i] == flag && i + 1 < args.len() {
            let value = &args[i + 1];
            let parsed: i64 = value
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value))?;
            return Ok(Some(parsed));
        }
        // Handle `--flag=value`
        if let Some((flag_part, value_part)) = args[i].split_once('=')
            && flag_part == flag
        {
            let parsed: i64 = value_part
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value_part))?;
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

/// Parse an f32 argument from command-line args.
///
/// Supports both `--flag value` and `--flag=value` forms.
pub(crate) fn parse_f32_arg(args: &[String], flag: &str) -> Result<Option<f32>, String> {
    for i in 0..args.len() {
        // Handle `--flag value`
        if args[i] == flag && i + 1 < args.len() {
            let value = &args[i + 1];
            let parsed: f32 = value
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value))?;
            return Ok(Some(parsed));
        }
        // Handle `--flag=value`
        if let Some((flag_part, value_part)) = args[i].split_once('=')
            && flag_part == flag
        {
            let parsed: f32 = value_part
                .parse()
                .map_err(|_| format!("Invalid {} value: {}", flag, value_part))?;
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

/// Parse a boolean value from a string.
///
/// Accepts: `true`, `false`, `yes`, `no`, `1`, `0` (case-insensitive).
pub(crate) fn parse_bool(value: &str) -> Result<bool, ()> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(()),
    }
}

/// Parse a u8 from hex (`0x..`) or decimal string.
pub(crate) fn parse_hex_u8(value: &str) -> Result<u8, ()> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16).map_err(|_| ())
    } else {
        trimmed.parse::<u8>().map_err(|_| ())
    }
}

/// Parse a 16-bit hex address (with or without `0x` prefix).
pub(crate) fn parse_hex_addr(s: &str) -> Option<u16> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).ok()
}

/// Parse a comma-separated breakpoint specification string into a list of [`BreakpointKind`].
///
/// Each entry is in the format `type=value`:
/// - `pc=ADDR` — PC breakpoint (hex address, e.g. `C000` or `0xC000`)
/// - `cycle=N` — Cycle breakpoint (decimal number)
/// - `frame=N` — Frame breakpoint (decimal number)
/// - `write=ADDR` — Write-address breakpoint (hex address)
///
/// Returns an error string if any entry is unrecognised or malformed.
pub(crate) fn parse_breakpoint_list(spec: &str) -> Result<Vec<BreakpointKind>, String> {
    spec.split(',')
        .map(|entry| {
            let entry = entry.trim();
            let (kind, value) = entry
                .split_once('=')
                .ok_or_else(|| format!("invalid breakpoint '{entry}': expected format type=value (e.g. pc=C000, cycle=100, frame=60, write=2006)"))?;
            match kind.trim() {
                "pc" => parse_hex_addr(value)
                    .map(BreakpointKind::Pc)
                    .ok_or_else(|| format!("invalid breakpoint address '{value}': expected a hex address (e.g. C000)")),
                "cycle" => value
                    .trim()
                    .parse::<u64>()
                    .map(BreakpointKind::Cycle)
                    .map_err(|_| format!("invalid breakpoint cycle '{value}': expected a decimal number")),
                "frame" => value
                    .trim()
                    .parse::<u64>()
                    .map(BreakpointKind::Frame)
                    .map_err(|_| format!("invalid breakpoint frame '{value}': expected a decimal number")),
                "write" => parse_hex_addr(value)
                    .map(BreakpointKind::WriteAddress)
                    .ok_or_else(|| format!("invalid breakpoint address '{value}': expected a hex address (e.g. 2006)")),
                other => Err(format!(
                    "invalid breakpoint type '{other}': expected pc, cycle, frame, or write"
                )),
            }
        })
        .collect()
}

/// Parse a comma-separated list of search paths.
pub(crate) fn parse_search_paths(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect()
}
