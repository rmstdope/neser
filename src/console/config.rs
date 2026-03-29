//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! Configuration values are loaded with the following priority (highest to lowest):
//! 1. Command-line arguments
//! 2. Config file (neser.conf)
//! 3. Default values

use crate::autorun::AutorunFormat;
use crate::console::TimingMode;
use crate::debugging::Tracing;
use crate::debugging::breakpoints::BreakpointKind;
use crate::input::ControllerType;
use bitflags::bitflags;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// CLI flag definition for help text and validation.
struct CliFlag {
    flag: &'static str,
    help: Option<&'static str>,
    has_value: bool,
}

/// All supported CLI flags.
const CLI_FLAGS: &[CliFlag] = &[
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
        flag: "--trace-nestest",
        help: Some("Enable CPU trace output (nestest.log format)"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-cpu",
        help: Some("Enable CPU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-ppu",
        help: Some("Enable PPU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-apu",
        help: Some("Enable APU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-mapper",
        help: Some("Enable mapper trace output"),
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
        flag: "--filter",
        help: Some("Specify shader filter: crt, ntsc, smooth, or none"),
        has_value: true,
    },
    CliFlag {
        flag: "--config",
        help: Some("Specify config file path (default: ./neser.conf or ~/.neser/neser.conf)"),
        has_value: true,
    },
    CliFlag {
        flag: "--window-height",
        help: Some("Window height in pixels (windowed mode only, e.g., --window-height 720)"),
        has_value: true,
    },
    CliFlag {
        flag: "--debugger-alpha",
        help: Some("Debugger window opacity: 0.1 (transparent) to 1.0 (opaque, default: 0.7)"),
        has_value: true,
    },
    CliFlag {
        flag: "--controller-port1",
        help: Some(
            "Controller type for port 1: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--controller-port2",
        help: Some(
            "Controller type for port 2: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--hardware",
        help: Some("Hardware mode: nes-ntsc, nes-pal, or famicom (default: nes-ntsc)"),
        has_value: true,
    },
    CliFlag {
        flag: "--expansion-port",
        help: Some(
            "Expansion port controller: none, famicom-four-players, arkanoid, zapper, or power-pad (default: none)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--zapper-detection-size",
        help: Some(
            "Zapper light detection square radius in pixels (0..=255, default: 0; higher values are more tolerant but slower)",
        ),
        has_value: true,
    },
    // Aligned flags matching config file keys with same value ranges
    // Support both value-based (--audio true) and prefix negation (--no-audio, --disable-audio)
    CliFlag {
        flag: "--oam-dram-decay",
        help: Some("Enable OAM DRAM decay emulation (true/false, default: false)"),
        has_value: false,
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
        flag: "--enable-4-score",
        help: Some(
            "Enable Four Score mode (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-4-score",
        help: Some("Disable Four Score mode (equivalent to --enable-4-score false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-4-score",
        help: Some("Disable Four Score mode (equivalent to --enable-4-score false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--pulse1",
        help: Some(
            "Enable pulse 1 channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-pulse1",
        help: Some("Disable pulse 1 channel (equivalent to --pulse1 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-pulse1",
        help: Some("Disable pulse 1 channel (equivalent to --pulse1 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--pulse2",
        help: Some(
            "Enable pulse 2 channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-pulse2",
        help: Some("Disable pulse 2 channel (equivalent to --pulse2 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-pulse2",
        help: Some("Disable pulse 2 channel (equivalent to --pulse2 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--triangle",
        help: Some(
            "Enable triangle channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-triangle",
        help: Some("Disable triangle channel (equivalent to --triangle false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-triangle",
        help: Some("Disable triangle channel (equivalent to --triangle false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--noise",
        help: Some(
            "Enable noise channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-noise",
        help: Some("Disable noise channel (equivalent to --noise false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-noise",
        help: Some("Disable noise channel (equivalent to --noise false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--dmc",
        help: Some("Enable DMC channel (optionally: true/false, default when flag present: true)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-dmc",
        help: Some("Disable DMC channel (equivalent to --dmc false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-dmc",
        help: Some("Disable DMC channel (equivalent to --dmc false)"),
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
        help: Some(
            "Serialization format for autorun files: binary (default) or json",
        ),
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
        flag: "--horizontal-overscan",
        help: Some(
            "Horizontal overscan removal in pixels (0..=8, default: 0; removed from left and right)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--vertical-overscan",
        help: Some(
            "Vertical overscan removal in pixels (0..=16, default: 8; removed from top and bottom)",
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

/// Boolean flags that accept optional values (shared by validate_args and parse_rom_arg).
const OPTIONAL_BOOL_FLAGS: &[&str] = &[
    "--oam-dram-decay",
    "--audio",
    "--vsync",
    "--gamepads",
    "--enable-4-score",
    "--pulse1",
    "--pulse2",
    "--triangle",
    "--noise",
    "--dmc",
    "--debugger",
    "--load-state",
    "--fullscreen",
    "--scan-cartridges",
    "--convert-autorun",
    "--recalculate-autorun",
    "--autorun-format",
];

/// Result of parsing command-line arguments.
#[derive(Debug)]
pub enum ParseResult {
    /// User requested help - print and exit.
    Help,
    /// Successfully parsed configuration.
    Config(Config),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareModel {
    NesNtsc,
    NesPal,
}

impl HardwareModel {
    pub const fn from_timing_mode(timing_mode: TimingMode) -> Self {
        match timing_mode {
            TimingMode::Pal => Self::NesPal,
            TimingMode::Ntsc
            | TimingMode::MultiRegion
            | TimingMode::Dendy
            | TimingMode::Unknown(_) => Self::NesNtsc,
        }
    }

    pub const fn timing_mode(self) -> TimingMode {
        match self {
            Self::NesNtsc => TimingMode::Ntsc,
            Self::NesPal => TimingMode::Pal,
        }
    }

    #[allow(dead_code)] // Used by the wasm frontend
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NesNtsc => "nes-ntsc",
            Self::NesPal => "nes-pal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareMode {
    Nes,
    Famicom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionPort {
    None,
    FamicomFourPlayers,
    ArkanoidFamicom,
    ZapperFamicom,
    PowerPadFamicom,
}

impl ExpansionPort {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("none") {
            Some(Self::None)
        } else if value.eq_ignore_ascii_case("famicom-four-players") {
            Some(Self::FamicomFourPlayers)
        } else if value.eq_ignore_ascii_case("arkanoid") {
            Some(Self::ArkanoidFamicom)
        } else if value.eq_ignore_ascii_case("zapper") {
            Some(Self::ZapperFamicom)
        } else if value.eq_ignore_ascii_case("power-pad") || value.eq_ignore_ascii_case("powerpad")
        {
            Some(Self::PowerPadFamicom)
        } else {
            None
        }
    }

    fn is_famicom_only(self) -> bool {
        matches!(
            self,
            Self::FamicomFourPlayers
                | Self::ArkanoidFamicom
                | Self::ZapperFamicom
                | Self::PowerPadFamicom
        )
    }
}

/// Emulator configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Emulated hardware family mode.
    pub hardware_mode: HardwareMode,
    /// Whether hardware mode was explicitly configured.
    pub hardware_mode_explicit: bool,
    /// Controller type connected to expansion port.
    pub expansion_port: ExpansionPort,
    /// Whether expansion port type was explicitly configured.
    pub expansion_port_explicit: bool,
    /// Emulated hardware model.
    pub hardware_model: HardwareModel,
    /// Whether the hardware model was explicitly configured.
    pub hardware_model_explicit: bool,
    /// Whether audio is enabled.
    pub audio_enabled: bool,
    /// Whether VSync is enabled.
    pub vsync_enabled: bool,
    /// Whether gamepad support is enabled.
    pub gamepads_enabled: bool,
    /// Whether Four Score mode is enabled.
    pub four_score_enabled: bool,
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
    /// APU channel enable flags.
    pub apu_channels: ApuChannels,
    /// Window height in pixels (windowed mode only).
    pub window_height: u32,
    /// Debugger window background opacity (0.1 = nearly transparent, 0.7 = opaque).
    pub debugger_alpha: f32,
    /// Optional ROM path from CLI positional argument.
    pub rom_path: Option<String>,
    /// Controller type connected to port 1.
    pub controller_port1: ControllerType,
    /// Controller type connected to port 2.
    pub controller_port2: ControllerType,
    /// Whether controller_port1 was explicitly configured (not default).
    pub controller_port1_explicit: bool,
    /// Whether controller_port2 was explicitly configured (not default).
    pub controller_port2_explicit: bool,
    /// Zapper light detection size (config key: `zapper_detection_size`).
    ///
    /// This controls the side length of the square area sampled around the Zapper
    /// cursor when checking for light hits:
    /// - `0` = single pixel
    /// - `1` = 3×3 square
    /// - `2` = 5×5 square
    /// - `n` = (2·n + 1)² pixels sampled
    ///
    /// Default: `0` (single pixel).
    ///
    /// Valid range: `0..=255` (full `u8` range). Larger values increase the
    /// number of pixels sampled per check and can noticeably impact performance,
    /// so values above 10 are generally not recommended.
    pub zapper_detection_size: u8,
    /// Autorun mode (None, Record, or Playback).
    pub autorun_mode: AutorunMode,
    /// Whether to run in headless mode (no display, requires playback).
    pub autorun_headless: bool,
    /// Whether to extend an existing recording (requires record mode).
    pub autorun_extend: bool,
    /// Whether to overwrite an existing recording (requires record mode).
    pub autorun_overwrite: bool,
    /// Start playback from this checkpoint index (0-based, or negative for from-end).
    /// Negative: -1 = second-to-last, -2 = third-to-last, etc.
    pub autorun_from_checkpoint: Option<i64>,
    /// Trim this many checkpoints from the end of the recording file and exit (no emulation).
    pub autorun_trim_checkpoints: Option<usize>,
    /// Convert an existing autorun file to the latest format and exit (no emulation).
    pub autorun_convert: bool,
    /// Recalculate checkpoint CRCs in an existing autorun file and exit (no emulation).
    pub autorun_recalculate: bool,
    /// Serialization format used when saving autorun files (default: binary).
    pub autorun_format: AutorunFormat,
    /// RAM initialization mode (config key: `ram_init_mode`).
    ///
    /// Controls how all emulated RAM is initialized on power-on/hard reset:
    /// - `zero`: All RAM initialized to 0x00
    /// - `random`: All RAM initialized to pseudo-random values (hardware-accurate)
    /// - `seeded-random`: All RAM initialized to pseudo-random values with a fixed seed
    ///
    /// Default: `Zero` (deterministic for testing; users can configure Random for hardware accuracy).
    ///
    /// Soft resets preserve RAM contents (no re-initialization).
    pub ram_init_mode: RamInitMode,
    /// Enable dynamic OAM DRAM decay emulation.
    ///
    /// This models NTSC primary OAM row decay behavior; PAL behavior is handled
    /// by the PPU implementation and this flag acts as a global on/off switch.
    ///
    /// Default: `false`.
    pub oam_dram_decay_enabled: bool,
    /// Breakpoints to set on startup (from --breakpoint / --frame CLI flags).
    pub breakpoints: Vec<BreakpointKind>,
    /// Horizontal overscan removal in pixels (removed from both left and right edges).
    ///
    /// Valid range: `0..=8`. Default: `0`.
    pub horizontal_overscan: u8,
    /// Vertical overscan removal in pixels (removed from both top and bottom edges).
    ///
    /// Valid range: `0..=16`. Default: `8`.
    pub vertical_overscan: u8,
    /// Comma-separated configured search paths for cartridge discovery.
    pub cartridge_search_paths: Vec<String>,
    /// Whether startup cartridge scanning is enabled.
    pub scan_cartridges: bool,
    /// Whether to rebuild the cartridge catalog from scratch on startup.
    pub rebuild_cartridge_catalog: bool,
    /// Whether to launch the TUI ROM browser instead of the emulator.
    pub tui_mode: bool,
}

/// Autorun operating mode.
///
/// This enum defines the primary operating mode for the emulator's autorun feature,
/// which enables recording and playback of controller input for deterministic testing
/// and automation.
///
/// The mode is typically derived from command-line flags (`--create-recording`, `--extend-recording`, `--playback`, `--playback-headless`)
/// and determines how controller input is processed:
/// - In `None` mode, controller input is handled interactively as normal
/// - In `Record` mode, controller input is captured and saved to an `.autorun` file
/// - In `Playback` mode, controller input is read from an existing `.autorun` file
///
/// Additional behavior is configured via related fields on [`Config`]:
///
/// - [`Config::autorun_headless`]: Run without display during playback (set by `--playback-headless`)
/// - [`Config::autorun_extend`]: When `Record` is active, load an existing recording,
///   play it back to the end, then continue recording new input (set by `--extend-recording`)
/// - [`Config::autorun_overwrite`]: When `Record` is active, replace any existing
///   recording file instead of failing if it already exists (set by `--create-recording`)
///
/// # Example Usage
///
/// ```bash
/// # Record gameplay to game.autorun (overwrite if exists)
/// neser --create-recording game.nes
///
/// # Extend existing recording
/// neser --extend-recording game.nes
///
/// # Play back and verify checksum
/// neser --playback game.nes
///
/// # Headless playback for CI
/// neser --playback-headless game.nes
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutorunMode {
    /// Normal interactive operation (no autorun).
    ///
    /// Controller input is handled interactively from keyboard, gamepad, or mouse.
    /// No `.autorun` file is read or written.
    None,
    /// Record controller input to an autorun file.
    ///
    /// Captures controller button states every frame and saves them to a `.autorun` file
    /// alongside the ROM. When recording completes, a CRC32 checksum of the final screen
    /// buffer is also saved for verification during playback.
    ///
    /// If [`Config::autorun_extend`] is true, any existing recording is loaded first,
    /// played back to the end, then new input is appended (extend mode).
    Record,
    /// Play back controller input from an existing autorun file.
    ///
    /// Replays controller input from a previously recorded `.autorun` file. Live controller
    /// input from keyboard/gamepad is ignored. When playback completes, the final screen
    /// buffer's CRC32 checksum is verified against the stored value, and the emulator exits
    /// with success (0) or failure (non-zero) status based on whether the checksum matches.
    ///
    /// Can be combined with [`Config::autorun_headless`] to run without display.
    Playback,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ApuChannels: u8 {
        const PULSE1 = 0b00001;
        const PULSE2 = 0b00010;
        const TRIANGLE = 0b00100;
        const NOISE = 0b01000;
        const DMC = 0b10000;
        const ALL = 0b11111;
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hardware_mode: HardwareMode::Nes,
            hardware_mode_explicit: false,
            expansion_port: ExpansionPort::None,
            expansion_port_explicit: false,
            hardware_model: HardwareModel::NesNtsc,
            hardware_model_explicit: false,
            audio_enabled: true,
            vsync_enabled: true,
            gamepads_enabled: true,
            four_score_enabled: false,
            fullscreen: false,
            fullscreen_display: None,
            shader_path: None,
            debugger_enabled: false,
            load_state: false,
            tracing: Tracing::default(),
            apu_channels: ApuChannels::ALL,
            window_height: 960,
            debugger_alpha: 0.7,
            rom_path: None,
            controller_port1: ControllerType::Joypad,
            controller_port2: ControllerType::Joypad,
            controller_port1_explicit: false,
            controller_port2_explicit: false,
            zapper_detection_size: 0,
            autorun_mode: AutorunMode::None,
            autorun_headless: false,
            autorun_extend: false,
            autorun_overwrite: false,
            autorun_from_checkpoint: None,
            autorun_trim_checkpoints: None,
            autorun_convert: false,
            autorun_recalculate: false,
            autorun_format: AutorunFormat::Binary,
            // Use Zero for WASM to avoid issues with getrandom in test environments
            #[cfg(target_arch = "wasm32")]
            ram_init_mode: RamInitMode::Zero,
            #[cfg(not(target_arch = "wasm32"))]
            ram_init_mode: RamInitMode::Random,
            oam_dram_decay_enabled: false,
            breakpoints: Vec::new(),
            horizontal_overscan: 0,
            vertical_overscan: 8,
            cartridge_search_paths: Vec::new(),
            scan_cartridges: true,
            rebuild_cartridge_catalog: false,
            tui_mode: false,
        }
    }
}

impl Config {
    pub(crate) fn parse_hardware_value(value: &str) -> Option<(HardwareMode, HardwareModel)> {
        if value.eq_ignore_ascii_case("nes-ntsc") {
            Some((HardwareMode::Nes, HardwareModel::NesNtsc))
        } else if value.eq_ignore_ascii_case("nes-pal") {
            Some((HardwareMode::Nes, HardwareModel::NesPal))
        } else if value.eq_ignore_ascii_case("famicom") {
            Some((HardwareMode::Famicom, HardwareModel::NesNtsc))
        } else {
            None
        }
    }

    fn parse_hardware_arg(
        args: &[String],
    ) -> Result<Option<(HardwareMode, HardwareModel)>, String> {
        if let Some(hardware) = Self::parse_string_arg(args, "--hardware") {
            if let Some(parsed) = Self::parse_hardware_value(&hardware) {
                Ok(Some(parsed))
            } else {
                Err(format!(
                    "Invalid --hardware value: '{}'. Valid options are: nes-ntsc, nes-pal, famicom",
                    hardware
                ))
            }
        } else {
            Ok(None)
        }
    }

    fn parse_expansion_port_arg(args: &[String]) -> Result<Option<ExpansionPort>, String> {
        if let Some(expansion_port) = Self::parse_string_arg(args, "--expansion-port") {
            let parsed = ExpansionPort::parse(&expansion_port).ok_or_else(|| {
                format!(
                    "Invalid --expansion-port value: '{}'. Valid options are: none, famicom-four-players, arkanoid, zapper, power-pad",
                    expansion_port
                )
            })?;
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn apply_hardware_value(&mut self, value: &str) -> Result<(), String> {
        let (hardware_mode, hardware_model) =
            Self::parse_hardware_value(value).ok_or_else(|| {
                format!(
                    "Invalid hardware value: '{}'. Valid options are: nes-ntsc, nes-pal, famicom",
                    value
                )
            })?;
        self.hardware_mode = hardware_mode;
        self.hardware_mode_explicit = true;
        self.hardware_model = hardware_model;
        self.hardware_model_explicit = true;
        Ok(())
    }

    pub(crate) fn apply_expansion_port_value(&mut self, value: &str) -> Result<(), String> {
        self.expansion_port = ExpansionPort::parse(value)
            .ok_or_else(|| format!("Invalid expansion_port value: '{}'", value))?;
        self.expansion_port_explicit = true;
        Ok(())
    }

    fn help_section_for_flag(flag: &str) -> &'static str {
        if flag.starts_with("--trace")
            || matches!(flag, "--debugger" | "--debugger-alpha" | "--breakpoint")
        {
            "Trace and Debugging"
        } else if matches!(
            flag,
            "--controller-port1"
                | "--controller-port2"
                | "--expansion-port"
                | "--zapper-detection-size"
                | "--gamepads"
                | "--enable-4-score"
                | "--no-4-score"
                | "--disable-4-score"
        ) {
            "Input"
        } else if matches!(
            flag,
            "--audio"
                | "--no-audio"
                | "--disable-audio"
                | "--pulse1"
                | "--no-pulse1"
                | "--disable-pulse1"
                | "--pulse2"
                | "--no-pulse2"
                | "--disable-pulse2"
                | "--triangle"
                | "--no-triangle"
                | "--disable-triangle"
                | "--noise"
                | "--no-noise"
                | "--disable-noise"
                | "--dmc"
                | "--no-dmc"
                | "--disable-dmc"
        ) {
            "Sound"
        } else if matches!(
            flag,
            "--fullscreen"
                | "--display"
                | "--filter"
                | "--window-height"
                | "--vsync"
                | "--no-vsync"
                | "--disable-vsync"
                | "--horizontal-overscan"
                | "--vertical-overscan"
        ) {
            "Video and Display"
        } else if matches!(
            flag,
            "--create-recording"
                | "--extend-recording"
                | "--playback"
                | "--playback-headless"
                | "--playback-from-checkpoint"
                | "--playback-headless-from-checkpoint"
                | "--trim-checkpoints"
                | "--convert-autorun"
                | "--recalculate-autorun"
        ) {
            "Autorun"
        } else if matches!(
            flag,
            "--cartridge-search-paths"
                | "--scan-cartridges"
                | "--no-scan-cartridges"
                | "--rebuild-cartridge-catalog"
        ) {
            "Cartridge Catalog"
        } else {
            "General"
        }
    }

    fn help_text() -> String {
        const HELP_SECTIONS: [&str; 7] = [
            "General",
            "Input",
            "Trace and Debugging",
            "Sound",
            "Video and Display",
            "Autorun",
            "Cartridge Catalog",
        ];

        let mut help = String::new();
        writeln!(&mut help, "NES Emulator").unwrap();
        writeln!(&mut help, "\nUsage: neser [OPTIONS] [ROM]").unwrap();

        for section in HELP_SECTIONS {
            let mut wrote_section = false;
            for flag in CLI_FLAGS {
                if flag.help.is_none() || Self::help_section_for_flag(flag.flag) != section {
                    continue;
                }

                if !wrote_section {
                    writeln!(&mut help, "\n{section}:").unwrap();
                    wrote_section = true;
                }

                if let Some(flag_help) = flag.help {
                    writeln!(&mut help, "  {:<19} {}", flag.flag, flag_help).unwrap();
                }
            }
        }

        writeln!(&mut help, "\nExamples:").unwrap();
        writeln!(
            &mut help,
            "  neser game.nes                               # Load and run a ROM"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --hardware nes-pal game.nes            # Use NES PAL hardware"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --debugger game.nes                    # Enable debugger (no value = true)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --breakpoint frame=120 game.nes           # Break on frame 120"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --audio game.nes                       # Enable audio (no value = true)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --audio=1 game.nes                     # Enable audio (equals syntax)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --audio false game.nes                 # Disable audio (value-based)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --audio=0 game.nes                     # Disable audio (equals syntax)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --no-audio game.nes                    # Disable audio (prefix negation)"
        )
        .unwrap();
        writeln!(
            &mut help,
            "  neser --disable-pulse1 --disable-pulse2 game.nes # Disable specific channels"
        )
        .unwrap();
        writeln!(&mut help).unwrap();
        writeln!(
            &mut help,
            "Note: Boolean flags can be used without value (defaults to true), with value (true/false/yes/no/1/0),"
        )
        .unwrap();
        writeln!(
            &mut help,
            "      or with prefix negation (--no-*, --disable-*). All forms: --audio, --audio=1, --audio true are equivalent."
        )
        .unwrap();

        help
    }

    fn valid_controller_values() -> &'static str {
        "joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
    }

    fn parse_controller_arg(flag: &str, value: &str) -> Result<ControllerType, String> {
        ControllerType::parse(value).ok_or_else(|| {
            format!(
                "Invalid value '{}' for '{}'. Valid options are: {}",
                value,
                flag,
                Self::valid_controller_values()
            )
        })
    }

    /// Create a new Config with only default values (no config files or args).
    #[cfg(test)]
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// Create a new Config from command-line arguments.
    ///
    /// Configuration is loaded in the following order (later overrides earlier):
    /// 1. Default values
    /// 2. ~/.neser/neser.conf (user-wide config, if it exists)
    /// 3. ./neser.conf (project-specific config, if it exists)
    /// 4. --config <file> (explicit config file, if specified)
    /// 5. Command-line arguments
    ///
    /// If --config is specified with a non-existent file, an error is returned.
    ///
    /// # Arguments
    /// * `args` - Command-line arguments (including program name at index 0).
    ///
    /// # Returns
    /// - `Ok(ParseResult::Help)` if --help or -h was specified
    /// - `Ok(ParseResult::Config(config))` on successful parse
    /// - `Err(message)` on validation error
    #[allow(clippy::new_ret_no_self)]
    pub fn new(args: &[String]) -> Result<ParseResult, String> {
        // Check for help first
        if args.iter().any(|a| a == "--help" || a == "-h") {
            return Ok(ParseResult::Help);
        }

        // Validate arguments
        Self::validate_args(args)?;

        // Step 1: Start with defaults
        let mut config = Self::default();

        // Step 2: Load config files in priority order
        // Check if --config was specified
        if let Some(config_path) = Self::parse_config_arg(args) {
            // Explicit config file - must exist
            let path = Path::new(&config_path);
            if !path.exists() {
                return Err(format!("Config file not found: {}", config_path));
            }
            config.load_from_file(path)?;
        } else {
            // Load from default locations (later overrides earlier)
            // First: ~/.neser/neser.conf
            if let Some(home) = std::env::var_os("HOME") {
                let home_config = Path::new(&home).join(".neser").join(Self::CONFIG_FILE_NAME);
                config.load_from_file(&home_config)?;
            }
            // Second: ./neser.conf (overrides user config)
            config.load_from_file(Path::new(Self::CONFIG_FILE_NAME))?;
        }

        // Step 3: Apply command-line arguments (override config file and defaults)
        config.apply_args(args)?;

        config.validate_controller_ports()?;

        Ok(ParseResult::Config(config))
    }

    /// Apply command-line arguments to the config.
    /// Arguments override any values set by defaults or config file.
    fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some((hardware_mode, hardware_model)) = Self::parse_hardware_arg(args)? {
            self.hardware_mode = hardware_mode;
            self.hardware_mode_explicit = true;
            self.hardware_model = hardware_model;
            self.hardware_model_explicit = true;
        }

        if let Some(expansion_port) = Self::parse_expansion_port_arg(args)? {
            self.expansion_port = expansion_port;
            self.expansion_port_explicit = true;
        }

        // OAM DRAM decay: --oam-dram-decay true/false
        if let Some(oam_dram_decay) = Self::parse_bool_arg(args, "--oam-dram-decay")? {
            self.oam_dram_decay_enabled = oam_dram_decay;
        }

        // Boolean flags (support both value-based and prefix negation)
        // Audio: --audio true/false, --no-audio, --disable-audio
        if let Some(audio) = Self::parse_bool_arg(args, "--audio")? {
            self.audio_enabled = audio;
        }
        if Self::has_negation_flag(args, &["--no-audio", "--disable-audio"]) {
            self.audio_enabled = false;
        }

        // VSync: --vsync true/false, --no-vsync, --disable-vsync
        if let Some(vsync) = Self::parse_bool_arg(args, "--vsync")? {
            self.vsync_enabled = vsync;
        }
        if Self::has_negation_flag(args, &["--no-vsync", "--disable-vsync"]) {
            self.vsync_enabled = false;
        }

        // Gamepads: --gamepads true/false
        if let Some(gamepads) = Self::parse_bool_arg(args, "--gamepads")? {
            self.gamepads_enabled = gamepads;
        }

        // Four Score: --enable-4-score true/false, --no-4-score, --disable-4-score
        if let Some(four_score) = Self::parse_bool_arg(args, "--enable-4-score")? {
            self.four_score_enabled = four_score;
        }
        if Self::has_negation_flag(args, &["--no-4-score", "--disable-4-score"]) {
            self.four_score_enabled = false;
        }

        // Debugger: --debugger true/false
        if let Some(debugger) = Self::parse_bool_arg(args, "--debugger")? {
            self.debugger_enabled = debugger;
        }

        // Load state: --load-state true/false
        if let Some(load_state) = Self::parse_bool_arg(args, "--load-state")? {
            self.load_state = load_state;
        }

        // Fullscreen (value-based)
        if let Some(fullscreen) = Self::parse_bool_arg(args, "--fullscreen")? {
            self.fullscreen = fullscreen;
        }

        // Display argument (only applies if fullscreen is set)
        if self.fullscreen
            && let Some(display) = Self::parse_display_arg(args)?
        {
            self.fullscreen_display = Some(display);
        }

        // Shader path
        if let Some(filter_name) = Self::parse_shader_arg(args) {
            self.shader_path = Some(Self::map_filter_name(&filter_name)?);
        }

        if let Some(path) = Self::parse_rom_arg(args)? {
            self.rom_path = Some(path);
        }

        // Tracing (merge with existing config file values)
        self.tracing.apply_args(args);

        // APU channel enable/disable flags (support both value-based and prefix negation)
        // Pulse1: --pulse1 true/false, --no-pulse1, --disable-pulse1
        if let Some(pulse1) = Self::parse_bool_arg(args, "--pulse1")? {
            if pulse1 {
                self.apu_channels.insert(ApuChannels::PULSE1);
            } else {
                self.apu_channels.remove(ApuChannels::PULSE1);
            }
        }
        if Self::has_negation_flag(args, &["--no-pulse1", "--disable-pulse1"]) {
            self.apu_channels.remove(ApuChannels::PULSE1);
        }

        // Pulse2: --pulse2 true/false, --no-pulse2, --disable-pulse2
        if let Some(pulse2) = Self::parse_bool_arg(args, "--pulse2")? {
            if pulse2 {
                self.apu_channels.insert(ApuChannels::PULSE2);
            } else {
                self.apu_channels.remove(ApuChannels::PULSE2);
            }
        }
        if Self::has_negation_flag(args, &["--no-pulse2", "--disable-pulse2"]) {
            self.apu_channels.remove(ApuChannels::PULSE2);
        }

        // Triangle: --triangle true/false, --no-triangle, --disable-triangle
        if let Some(triangle) = Self::parse_bool_arg(args, "--triangle")? {
            if triangle {
                self.apu_channels.insert(ApuChannels::TRIANGLE);
            } else {
                self.apu_channels.remove(ApuChannels::TRIANGLE);
            }
        }
        if Self::has_negation_flag(args, &["--no-triangle", "--disable-triangle"]) {
            self.apu_channels.remove(ApuChannels::TRIANGLE);
        }

        // Noise: --noise true/false, --no-noise, --disable-noise
        if let Some(noise) = Self::parse_bool_arg(args, "--noise")? {
            if noise {
                self.apu_channels.insert(ApuChannels::NOISE);
            } else {
                self.apu_channels.remove(ApuChannels::NOISE);
            }
        }
        if Self::has_negation_flag(args, &["--no-noise", "--disable-noise"]) {
            self.apu_channels.remove(ApuChannels::NOISE);
        }

        // DMC: --dmc true/false, --no-dmc, --disable-dmc
        if let Some(dmc) = Self::parse_bool_arg(args, "--dmc")? {
            if dmc {
                self.apu_channels.insert(ApuChannels::DMC);
            } else {
                self.apu_channels.remove(ApuChannels::DMC);
            }
        }
        if Self::has_negation_flag(args, &["--no-dmc", "--disable-dmc"]) {
            self.apu_channels.remove(ApuChannels::DMC);
        }

        // Window height
        if let Some(height) = Self::parse_u32_arg(args, "--window-height")? {
            self.window_height = height;
        }

        // Debugger alpha
        if let Some(alpha) = Self::parse_f32_arg(args, "--debugger-alpha")? {
            self.debugger_alpha = alpha.clamp(0.1, 1.0);
        }

        // Controller ports
        if let Some(controller_port1) = Self::parse_string_arg(args, "--controller-port1") {
            self.controller_port1 =
                Self::parse_controller_arg("--controller-port1", &controller_port1)?;
            self.controller_port1_explicit = true;
        }
        if let Some(controller_port2) = Self::parse_string_arg(args, "--controller-port2") {
            self.controller_port2 =
                Self::parse_controller_arg("--controller-port2", &controller_port2)?;
            self.controller_port2_explicit = true;
        }

        // Zapper detection size
        if let Some(size) = Self::parse_u32_arg(args, "--zapper-detection-size")? {
            let size_u8 = u8::try_from(size).map_err(|_| {
                format!(
                    "Invalid --zapper-detection-size value: {} (must be between 0 and 255)",
                    size
                )
            })?;
            self.zapper_detection_size = size_u8;

            if size_u8 > 10 {
                eprintln!(
                    "Warning: --zapper-detection-size={} may cause performance issues. \
                     Large values sample (2*size + 1)^2 = {} pixels per controller read. \
                     Consider values <= 10 for better performance.",
                    size_u8,
                    (2 * size_u8 as u32 + 1).pow(2)
                );
            }
        }

        // RAM initialization mode
        let cli_ram_init_mode = Self::parse_string_arg(args, "--ram-init-mode");
        if let Some(value) = cli_ram_init_mode.as_ref() {
            self.apply_config_value("ram_init_mode", value)?;
        }

        // Overscan
        if let Some(v) = Self::parse_u32_arg(args, "--horizontal-overscan")? {
            self.horizontal_overscan = (v as u8).min(8);
        }
        if let Some(v) = Self::parse_u32_arg(args, "--vertical-overscan")? {
            self.vertical_overscan = (v as u8).min(16);
        }

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

        if let Some(v) = Self::parse_i64_arg(args, "--playback-from-checkpoint")? {
            self.autorun_from_checkpoint = Some(v);
            // Implies playback mode if no explicit mode was set
            if self.autorun_mode == AutorunMode::None {
                self.autorun_mode = AutorunMode::Playback;
            }
        }

        if let Some(v) = Self::parse_i64_arg(args, "--playback-headless-from-checkpoint")? {
            self.autorun_from_checkpoint = Some(v);
            self.autorun_mode = AutorunMode::Playback;
            self.autorun_headless = true;
        }

        if let Some(v) = Self::parse_u32_arg(args, "--trim-checkpoints")? {
            self.autorun_trim_checkpoints = Some(v as usize);
        }

        if let Some(convert_autorun_requested) = Self::parse_bool_arg(args, "--convert-autorun")? {
            self.autorun_convert = convert_autorun_requested;
        }

        if let Some(recalculate_autorun_requested) =
            Self::parse_bool_arg(args, "--recalculate-autorun")?
        {
            self.autorun_recalculate = recalculate_autorun_requested;
        }

        if let Some(format_str) = Self::parse_string_arg(args, "--autorun-format") {
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
        if let Some(value) = Self::parse_string_arg(args, "--breakpoint") {
            self.breakpoints =
                parse_breakpoint_list(&value).map_err(|e| format!("--breakpoint: {e}"))?;
        }

        Ok(())
    }
    pub fn print_help() {
        print!("{}", Self::help_text());
    }

    /// Validate command-line arguments.
    fn validate_args(args: &[String]) -> Result<(), String> {
        let mut i = 1; // Skip program name
        let mut seen_positional = false;
        while i < args.len() {
            let arg = &args[i];

            // Check for exact flag match
            if let Some(flag) = CLI_FLAGS.iter().find(|f| f.flag == arg) {
                if flag.has_value {
                    if i + 1 >= args.len() {
                        return Err(format!("Missing value for {arg}\nTry --help for usage."));
                    }
                    i += 1; // Skip the value
                }
                // For optional boolean flags, check if next arg is a boolean value
                else if OPTIONAL_BOOL_FLAGS.contains(&arg.as_str()) {
                    // Peek at next argument to see if it's a boolean value
                    if i + 1 < args.len() {
                        let next_arg = &args[i + 1];
                        // If next arg is a valid boolean value, skip it
                        if Self::parse_bool(next_arg).is_ok() {
                            i += 1; // Skip the boolean value
                        }
                        // Otherwise leave it for processing as another flag or positional
                    }
                }
                i += 1;
                continue;
            }

            // Check for --flag=value syntax (e.g., --trace-cpu=2)
            if let Some((flag_part, _)) = arg.split_once('=')
                && CLI_FLAGS.iter().any(|f| f.flag == flag_part)
            {
                i += 1;
                continue;
            }

            if arg.starts_with('-') {
                return Err(format!("Unknown argument: {arg}\nTry --help for usage."));
            }

            if seen_positional {
                return Err(format!(
                    "Unexpected positional argument: {arg}\nTry --help for usage."
                ));
            }

            seen_positional = true;
            i += 1;
            continue;
        }

        Ok(())
    }

    /// Parse the --display argument from command-line args.
    fn parse_display_arg(args: &[String]) -> Result<Option<i32>, String> {
        for i in 0..args.len() {
            if args[i] == "--display" {
                if i + 1 >= args.len() {
                    return Err("Missing value for --display".to_string());
                }
                let value = &args[i + 1];
                let parsed: i32 = value
                    .parse()
                    .map_err(|_| format!("Invalid --display value: {value}"))?;
                if parsed < 0 {
                    return Err("--display must be >= 0".to_string());
                }
                return Ok(Some(parsed));
            }
        }
        Ok(None)
    }

    /// Parse the --filter argument from command-line args.
    fn parse_shader_arg(args: &[String]) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == "--filter" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    }

    /// Parse the --config argument from command-line args.
    fn parse_config_arg(args: &[String]) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == "--config" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    }

    /// Parse a positional ROM path from command-line args.
    fn parse_rom_arg(args: &[String]) -> Result<Option<String>, String> {
        let mut i = 1; // Skip program name
        let mut rom_path: Option<String> = None;
        while i < args.len() {
            let arg = &args[i];

            if let Some(flag) = CLI_FLAGS.iter().find(|f| f.flag == arg) {
                if flag.has_value {
                    i += 2;
                }
                // For optional boolean flags, check if next arg is a boolean value
                else if OPTIONAL_BOOL_FLAGS.contains(&arg.as_str()) {
                    i += 1;
                    // Peek at next argument to see if it's a boolean value
                    if i < args.len() && Self::parse_bool(&args[i]).is_ok() {
                        i += 1; // Skip the boolean value
                    }
                } else {
                    i += 1;
                }
                continue;
            }

            if let Some((flag_part, _)) = arg.split_once('=')
                && CLI_FLAGS.iter().any(|f| f.flag == flag_part)
            {
                i += 1;
                continue;
            }

            if arg.starts_with('-') {
                i += 1;
                continue;
            }

            if rom_path.is_some() {
                return Err(format!(
                    "Unexpected positional argument: {arg}\nTry --help for usage."
                ));
            }

            rom_path = Some(arg.clone());
            i += 1;
        }

        Ok(rom_path)
    }

    /// Parse a u32 argument from command-line args.
    fn parse_u32_arg(args: &[String], flag: &str) -> Result<Option<u32>, String> {
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
    fn parse_i64_arg(args: &[String], flag: &str) -> Result<Option<i64>, String> {
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
    fn parse_f32_arg(args: &[String], flag: &str) -> Result<Option<f32>, String> {
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

    /// Parse a string argument from command-line args.
    ///
    /// Supports both `--flag value` and `--flag=value` forms.
    fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
        for i in 0..args.len() {
            // Handle `--flag value`
            if args[i] == flag && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }

            // Handle `--flag=value`
            if let Some((flag_part, value_part)) = args[i].split_once('=')
                && flag_part == flag
            {
                return Some(value_part.to_string());
            }
        }
        None
    }

    /// Default config file name.
    const CONFIG_FILE_NAME: &'static str = "neser.conf";

    /// Load configuration from a config file.
    ///
    /// The config file uses a simple key=value format, one setting per line.
    /// Lines starting with '#' are treated as comments.
    /// Unknown keys are ignored.
    ///
    /// # Example config file:
    /// ```text
    /// # Hardware mode: nes-ntsc, nes-pal, or famicom
    /// hardware=nes-ntsc
    ///
    /// # Expansion port: none or famicom-four-players
    /// expansion_port=none
    ///
    /// # Audio settings
    /// audio=true
    /// vsync=true
    ///
    /// # Fullscreen settings
    /// fullscreen=false
    /// display=0
    ///
    /// # Window settings (windowed mode only)
    /// window_height=960
    ///
    /// # Shader/filter
    /// # Valid values: crt, ntsc, smooth, none
    /// filter=crt
    ///
    /// # APU channel toggles
    /// pulse1=true
    /// pulse2=true
    /// triangle=true
    /// noise=true
    /// dmc=true
    /// ```
    fn load_from_file(&mut self, path: &Path) -> Result<(), String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(()), // File doesn't exist or can't be read - silently ignore
        };

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key=value
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                self.apply_config_value(key, value)?;
            }
        }
        Ok(())
    }

    /// Map simplified filter names to shader paths.
    ///
    /// Supported values: crt, ntsc, smooth, none
    ///
    /// Returns `Ok(String)` with the full shader path for valid filter names,
    /// or `Err(String)` with an error message for invalid/unknown names.
    fn map_filter_name(name: &str) -> Result<String, String> {
        match name {
            "crt" => Ok("shaders/crt-lottes.slangp".to_string()),
            "ntsc" => Ok("shaders/ntsc-256px-composite.slangp".to_string()),
            "smooth" => Ok("shaders/xbrz-freescale.slangp".to_string()),
            "none" => Ok("shaders/stock.slangp".to_string()),
            _ => Err(format!(
                "Invalid filter name: '{}'. Valid options are: crt, ntsc, smooth, none",
                name
            )),
        }
    }

    /// Apply a single config file key-value pair.
    fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "hardware" => self.apply_hardware_value(value)?,
            "expansion_port" => self.apply_expansion_port_value(value)?,
            "audio" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.audio_enabled = b;
                }
            }
            "vsync" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.vsync_enabled = b;
                }
            }
            "gamepads" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.gamepads_enabled = b;
                }
            }
            "enable_4_score" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.four_score_enabled = b;
                }
            }
            "fullscreen" => {
                if let Ok(b) = Self::parse_bool(value) {
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
            "filter" => {
                if !value.is_empty() {
                    self.shader_path = Some(Self::map_filter_name(value)?);
                }
            }
            "debugger" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.debugger_enabled = b;
                }
            }
            "load_state" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.load_state = b;
                }
            }
            "pulse1" => {
                if let Ok(b) = Self::parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE1);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE1);
                    }
                }
            }
            "pulse2" => {
                if let Ok(b) = Self::parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE2);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE2);
                    }
                }
            }
            "triangle" => {
                if let Ok(b) = Self::parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::TRIANGLE);
                    } else {
                        self.apu_channels.remove(ApuChannels::TRIANGLE);
                    }
                }
            }
            "noise" => {
                if let Ok(b) = Self::parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::NOISE);
                    } else {
                        self.apu_channels.remove(ApuChannels::NOISE);
                    }
                }
            }
            "dmc" => {
                if let Ok(b) = Self::parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::DMC);
                    } else {
                        self.apu_channels.remove(ApuChannels::DMC);
                    }
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
            "controller_port1" => {
                self.controller_port1 = Self::parse_controller_arg("controller_port1", value)?;
                self.controller_port1_explicit = true;
            }
            "controller_port2" => {
                self.controller_port2 = Self::parse_controller_arg("controller_port2", value)?;
                self.controller_port2_explicit = true;
            }
            "zapper_detection_size" => {
                if let Ok(size) = value.parse::<u8>() {
                    self.zapper_detection_size = size;
                    // Warn about performance implications of large values
                    if size > 10 {
                        eprintln!(
                            "Warning: zapper_detection_size={} may cause performance issues. \
                             Large values sample (2*size + 1)² = {} pixels per controller read. \
                             Consider using values ≤ 10 for better performance.",
                            size,
                            (2 * size as u32 + 1).pow(2)
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'zapper_detection_size' in configuration; \
                         ignoring. Must be a number between 0 and 255.",
                        value
                    );
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
                    self.tracing.ppu = crate::debugging::Tracing::clamp_ppu_level(level);
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
                    self.tracing.mapper = crate::debugging::Tracing::clamp_mapper_level(level);
                    if level > 0 {
                        self.tracing.enabled = true;
                    }
                }
            }
            "trace-nestest" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.tracing.nestest = b;
                    if b {
                        self.tracing.enabled = true;
                    }
                }
            }
            "ram_init_mode" => {
                match value.to_lowercase().as_str() {
                    "zero" => self.ram_init_mode = RamInitMode::Zero,
                    "random" => self.ram_init_mode = RamInitMode::Random,
                    _ => {
                        // Try parsing as seeded-random with a seed value
                        if let Some(seed_str) = value
                            .strip_prefix("seeded-random:")
                            .or_else(|| value.strip_prefix("seeded_random:"))
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
                }
            }
            "oam_dram_decay" | "oam_dram_decay_enabled" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.oam_dram_decay_enabled = b;
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'oam_dram_decay'; keeping current value. \
                         Valid values: true/false/yes/no/1/0",
                        value
                    );
                }
            }
            "horizontal_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.horizontal_overscan = v.min(8);
                }
            }
            "vertical_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.vertical_overscan = v.min(16);
                }
            }
            "cartridge_search_paths" | "scan_cartridges" | "rebuild_cartridge_catalog" => {
                self.apply_cartridge_catalog_config_value(key, value);
            }
            _ => {} // Unknown keys are silently ignored
        }
        Ok(())
    }

    pub fn apply_rom_timing_mode(&mut self, rom_timing_mode: crate::cartridge::TimingMode) -> bool {
        if self.hardware_model_explicit {
            return false;
        }

        if rom_timing_mode.is_ntsc_or_pal() {
            self.hardware_model = HardwareModel::from_timing_mode(rom_timing_mode);
            true
        } else {
            false
        }
    }

    pub fn apply_rom_db_famicom_four_players_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        let mut changed = false;

        if !self.hardware_mode_explicit && self.hardware_mode != HardwareMode::Famicom {
            self.hardware_mode = HardwareMode::Famicom;
            changed = true;
        }

        if !self.hardware_model_explicit && self.hardware_model != HardwareModel::NesNtsc {
            self.hardware_model = HardwareModel::NesNtsc;
            changed = true;
        }

        if !self.expansion_port_explicit
            && self.hardware_mode == HardwareMode::Famicom
            && self.expansion_port != ExpansionPort::FamicomFourPlayers
        {
            self.expansion_port = ExpansionPort::FamicomFourPlayers;
            changed = true;
        }

        changed
    }

    pub fn apply_rom_db_arkanoid_famicom_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        let mut changed = false;

        if !self.hardware_mode_explicit && self.hardware_mode != HardwareMode::Famicom {
            self.hardware_mode = HardwareMode::Famicom;
            changed = true;
        }

        if !self.hardware_model_explicit && self.hardware_model != HardwareModel::NesNtsc {
            self.hardware_model = HardwareModel::NesNtsc;
            changed = true;
        }

        if !self.expansion_port_explicit
            && self.hardware_mode == HardwareMode::Famicom
            && self.expansion_port != ExpansionPort::ArkanoidFamicom
        {
            self.expansion_port = ExpansionPort::ArkanoidFamicom;
            changed = true;
        }

        changed
    }

    pub fn apply_rom_db_zapper_famicom_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        // Only set expansion port if hardware mode is already Famicom.
        // For NES mode, the Zapper is handled via standard controller ports.
        if !self.expansion_port_explicit
            && self.hardware_mode == HardwareMode::Famicom
            && self.expansion_port != ExpansionPort::ZapperFamicom
        {
            self.expansion_port = ExpansionPort::ZapperFamicom;
            return true;
        }

        false
    }

    pub fn apply_rom_db_famicom_region_hint(&mut self, is_japan: bool) -> bool {
        if !is_japan {
            return false;
        }

        if !self.hardware_mode_explicit && self.hardware_mode != HardwareMode::Famicom {
            self.hardware_mode = HardwareMode::Famicom;
            return true;
        }

        false
    }

    /// Parse a boolean value from config file.
    /// Accepts: true, false, yes, no, 1, 0 (case-insensitive)
    fn parse_bool(value: &str) -> Result<bool, ()> {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(true),
            "false" | "no" | "0" => Ok(false),
            _ => Err(()),
        }
    }

    fn parse_search_paths(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    fn apply_cartridge_catalog_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(paths) = Self::parse_string_arg(args, "--cartridge-search-paths") {
            self.cartridge_search_paths = Self::parse_search_paths(&paths);
        }

        if let Some(scan) = Self::parse_bool_arg(args, "--scan-cartridges")? {
            self.scan_cartridges = scan;
        }
        if Self::has_negation_flag(args, &["--no-scan-cartridges"]) {
            self.scan_cartridges = false;
        }

        if args.iter().any(|arg| arg == "--rebuild-cartridge-catalog") {
            self.rebuild_cartridge_catalog = true;
        }

        Ok(())
    }

    fn apply_cartridge_catalog_config_value(&mut self, key: &str, value: &str) {
        match key {
            "cartridge_search_paths" => {
                self.cartridge_search_paths = Self::parse_search_paths(value);
            }
            "scan_cartridges" => {
                if let Ok(scan) = Self::parse_bool(value) {
                    self.scan_cartridges = scan;
                }
            }
            "rebuild_cartridge_catalog" => {
                if let Ok(rebuild) = Self::parse_bool(value) {
                    self.rebuild_cartridge_catalog = rebuild;
                }
            }
            _ => {}
        }
    }

    /// Parse a boolean argument from command-line args.
    /// Supports three forms:
    /// - `--flag` (no value) → defaults to true
    /// - `--flag value` (space-separated) → parse value
    /// - `--flag=value` (equals syntax) → parse value
    ///   Returns Ok(None) if flag not present, Ok(Some(bool)) if valid, Err(msg) if invalid.
    fn parse_bool_arg(args: &[String], flag: &str) -> Result<Option<bool>, String> {
        for i in 0..args.len() {
            // Check for --flag=value syntax
            if let Some((flag_part, value_part)) = args[i].split_once('=') {
                if flag_part == flag {
                    match Self::parse_bool(value_part) {
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
                    match Self::parse_bool(next_arg) {
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
    fn has_negation_flag(args: &[String], flags: &[&str]) -> bool {
        args.iter().any(|a| flags.contains(&a.as_str()))
    }

    fn validate_controller_ports(&self) -> Result<(), String> {
        if self.hardware_mode == HardwareMode::Famicom
            && (self.controller_port1_explicit || self.controller_port2_explicit)
        {
            return Err(
                "In Famicom mode, --controller-port1 and --controller-port2 are not allowed because ports 1 and 2 are hardwired joypads".to_string(),
            );
        }

        if self.hardware_mode == HardwareMode::Nes && self.expansion_port.is_famicom_only() {
            return Err("famicom expansion_port requires hardware=famicom".to_string());
        }

        let mouse_emulated_controller_count = [self.controller_port1, self.controller_port2]
            .iter()
            .filter(|controller| {
                matches!(
                    **controller,
                    ControllerType::Arkanoid | ControllerType::Zapper | ControllerType::SnesMouse
                )
            })
            .count();

        if mouse_emulated_controller_count > 1 {
            return Err(
                "No more than one mouse-emulated controller can be configured (Arkanoid/Zapper)"
                    .to_string(),
            );
        }

        Ok(())
    }
}

/// Parse a comma-separated breakpoint specification string into a list of `BreakpointKind`.
///
/// Each entry is in the format `type=value`:
/// - `pc=ADDR` — PC breakpoint (hex address, e.g. `C000` or `0xC000`)
/// - `cycle=N` — Cycle breakpoint (decimal number)
/// - `frame=N` — Frame breakpoint (decimal number)
/// - `write=ADDR` — Write-address breakpoint (hex address)
///
/// Returns an error string if any entry is unrecognised or malformed.
fn parse_breakpoint_list(spec: &str) -> Result<Vec<BreakpointKind>, String> {
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

fn parse_hex_addr(s: &str) -> Option<u16> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_new(mut args: Vec<String>) -> Result<ParseResult, String> {
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

    fn parse_config(args: Vec<String>) -> Config {
        match config_new(args).unwrap() {
            ParseResult::Config(c) => c,
            ParseResult::Help => panic!("Expected Config, got Help"),
        }
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::with_defaults();
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.audio_enabled);
        assert!(config.vsync_enabled);
        assert!(config.gamepads_enabled);
        assert!(!config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
        assert_eq!(config.shader_path, None);
        assert!(!config.debugger_enabled);
        assert!(!config.load_state);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
        assert!(config.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(config.apu_channels.contains(ApuChannels::NOISE));
        assert!(config.apu_channels.contains(ApuChannels::DMC));
        assert_eq!(config.window_height, 960);
        assert_eq!(config.rom_path, None);
        assert_eq!(config.controller_port1, ControllerType::Joypad);
        assert_eq!(config.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_new_defaults() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.audio_enabled);
        assert!(config.vsync_enabled);
        assert!(config.gamepads_enabled);
        assert!(!config.fullscreen);
        assert_eq!(config.window_height, 960);
        assert_eq!(config.controller_port1, ControllerType::Joypad);
        assert_eq!(config.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_help_flag() {
        let args = vec!["neser".to_string(), "--help".to_string()];
        match config_new(args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_config_help_flag_short() {
        let args = vec!["neser".to_string(), "-h".to_string()];
        match config_new(args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_help_text_groups_flags_into_readable_sections() {
        let help = Config::help_text();

        assert!(help.contains("\nInput:"));
        assert!(help.contains("\nTrace and Debugging:"));
        assert!(help.contains("\nSound:"));
        assert!(help.contains("\nVideo and Display:"));
        assert!(help.contains("\nAutorun:"));
        assert!(help.contains("\nCartridge Catalog:"));

        let input_section = help.find("\nInput:").unwrap();
        let input_flag = help.find("--controller-port1").unwrap();
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
        let help = Config::help_text();

        assert!(help.contains("--load-state"));
        assert!(!help.contains("--no-load-state"));
        assert!(!help.contains("--disable-load-state"));
    }

    #[test]
    fn test_help_text_oam_dram_decay_shows_default_and_no_negation_aliases() {
        let help = Config::help_text();

        assert!(help.contains("--oam-dram-decay"));
        assert!(help.contains("default: false"));
        assert!(!help.contains("--no-oam-dram-decay"));
        assert!(!help.contains("--disable-oam-dram-decay"));
    }

    #[test]
    fn test_help_text_examples_use_hardware_flag() {
        let help = Config::help_text();

        assert!(help.contains("neser --hardware nes-pal game.nes"));
        assert!(!help.contains("--tv-system"));
    }

    #[test]
    fn test_config_hardware_nes_pal() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "nes-pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(config.hardware_model_explicit);
    }

    #[test]
    fn test_timing_mode_values_match_expected_cpu_clock_and_scanlines() {
        let ntsc_timing = HardwareModel::NesNtsc.timing_mode();
        assert_eq!(ntsc_timing.cpu_clock_hz(), 1_789_773.0);
        assert_eq!(ntsc_timing.scanlines_per_frame(), 262);

        let pal_timing = HardwareModel::NesPal.timing_mode();
        assert_eq!(pal_timing.cpu_clock_hz(), 1_662_607.0);
        assert_eq!(pal_timing.scanlines_per_frame(), 312);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_when_not_explicit() {
        let mut config = Config::default();
        let applied = config.apply_rom_timing_mode(crate::cartridge::TimingMode::Pal);
        assert!(applied);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_does_not_override_explicit() {
        let mut config = Config {
            hardware_model: HardwareModel::NesPal,
            hardware_model_explicit: true,
            ..Default::default()
        };
        let applied = config.apply_rom_timing_mode(crate::cartridge::TimingMode::Ntsc);
        assert!(!applied);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_unknown_keeps_default() {
        let mut config = Config::default();
        let applied = config.apply_rom_timing_mode(crate::cartridge::TimingMode::Unknown(0));
        assert!(!applied);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_sets_hardware_and_expansion() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(changed);
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.expansion_port, ExpansionPort::FamicomFourPlayers);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_respects_explicit_nes_hardware_override()
    {
        let mut config = Config {
            hardware_mode: HardwareMode::Nes,
            hardware_mode_explicit: true,
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(!changed);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_sets_expansion_when_hardware_explicit_famicom()
     {
        let mut config = Config {
            hardware_mode: HardwareMode::Famicom,
            hardware_mode_explicit: true,
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(changed);
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.expansion_port, ExpansionPort::FamicomFourPlayers);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_sets_hardware_and_expansion() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_arkanoid_famicom_hint(true);

        assert!(changed);
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.expansion_port, ExpansionPort::ArkanoidFamicom);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_respects_explicit_expansion_override() {
        let mut config = Config {
            expansion_port: ExpansionPort::None,
            expansion_port_explicit: true,
            ..Default::default()
        };

        let changed = config.apply_rom_db_arkanoid_famicom_hint(true);

        // Hardware mode changes but expansion stays explicit
        assert!(changed);
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_false_is_noop() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_arkanoid_famicom_hint(false);

        assert!(!changed);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_sets_famicom_mode() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_region_hint(true);

        assert!(changed);
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_respects_explicit_hardware() {
        let mut config = Config {
            hardware_mode: HardwareMode::Nes,
            hardware_mode_explicit: true,
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_region_hint(true);

        assert!(!changed);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_false_is_noop() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_region_hint(false);

        assert!(!changed);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_audio_false() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_vsync_false() {
        let args = vec![
            "neser".to_string(),
            "--vsync".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.vsync_enabled);
    }

    #[test]
    fn test_config_gamepads_false() {
        let args = vec![
            "neser".to_string(),
            "--gamepads".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.gamepads_enabled);
    }

    #[test]
    fn test_config_fullscreen_true() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_load_state_flag() {
        let args = vec!["neser".to_string(), "--load-state".to_string()];
        let config = parse_config(args);
        assert!(config.load_state);
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
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, Some(1));
    }

    #[test]
    fn test_config_display_without_fullscreen_is_ignored() {
        let args = vec![
            "neser".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
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
            "--filter".to_string(),
            "invalid-filter".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-filter'. Valid options are: crt, ntsc, smooth, none"
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
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_pulse1_false() {
        let args = vec![
            "neser".to_string(),
            "--pulse1".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_pulse2_false() {
        let args = vec![
            "neser".to_string(),
            "--pulse2".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_triangle_false() {
        let args = vec![
            "neser".to_string(),
            "--triangle".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_noise_false() {
        let args = vec![
            "neser".to_string(),
            "--noise".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_dmc_false() {
        let args = vec![
            "neser".to_string(),
            "--dmc".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::DMC));
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
        assert_eq!(config.rom_path.as_deref(), Some("somefile.nes"));
    }

    #[test]
    fn test_config_tracing_enabled() {
        let args = vec!["neser".to_string(), "--trace".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 1); // --trace enables CPU tracing at level 1
    }

    #[test]
    fn test_config_tracing_nestest() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert!(config.tracing.nestest);
    }

    #[test]
    fn test_config_tracing_cpu() {
        let args = vec!["neser".to_string(), "--trace-cpu".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 1);
    }

    #[test]
    fn test_config_tracing_ppu() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.ppu, 1);
    }

    #[test]
    fn test_config_tracing_apu() {
        let args = vec!["neser".to_string(), "--trace-apu".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.apu, 1);
    }

    #[test]
    fn test_config_tracing_mapper() {
        let args = vec!["neser".to_string(), "--trace-mapper".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.mapper, 1);
    }

    #[test]
    fn test_config_tracing_cpu_with_level() {
        let args = vec!["neser".to_string(), "--trace-cpu=2".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 2);
    }

    #[test]
    fn test_config_tracing_ppu_with_level() {
        let args = vec!["neser".to_string(), "--trace-ppu=3".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.ppu, 3);
    }

    #[test]
    fn test_config_tracing_ppu_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-ppu=9".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.ppu, 5);
    }

    #[test]
    fn test_config_tracing_apu_with_level() {
        let args = vec!["neser".to_string(), "--trace-apu=4".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.apu, 4);
    }

    #[test]
    fn test_config_tracing_mapper_with_level() {
        let args = vec!["neser".to_string(), "--trace-mapper=5".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.mapper, 5);
    }

    #[test]
    fn test_config_tracing_mapper_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-mapper=9".to_string()];
        let config = parse_config(args);
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.mapper, 5);
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
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 3);
        assert_eq!(config.tracing.ppu, 2);
        assert_eq!(config.tracing.apu, 1);
        assert_eq!(config.tracing.mapper, 0);
    }

    #[test]
    fn test_config_multiple_flags() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "nes-pal".to_string(),
            "--audio".to_string(),
            "false".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
            "--display".to_string(),
            "2".to_string(),
            "--pulse1".to_string(),
            "false".to_string(),
            "--noise".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(!config.audio_enabled);
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, Some(2));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_window_height() {
        let args = vec![
            "neser".to_string(),
            "--window-height".to_string(),
            "720".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.window_height, 720);
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

    #[test]
    fn test_config_zapper_detection_size_cli_space_syntax() {
        let args = vec![
            "neser".to_string(),
            "--zapper-detection-size".to_string(),
            "3".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.zapper_detection_size, 3);
    }

    #[test]
    fn test_config_zapper_detection_size_cli_equals_syntax() {
        let args = vec!["neser".to_string(), "--zapper-detection-size=7".to_string()];
        let config = parse_config(args);
        assert_eq!(config.zapper_detection_size, 7);
    }

    #[test]
    fn test_config_zapper_detection_size_cli_out_of_range_errors() {
        let args = vec![
            "neser".to_string(),
            "--zapper-detection-size".to_string(),
            "300".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default_horizontal_overscan_is_8() {
        let config = Config::default();
        assert_eq!(config.horizontal_overscan, 0);
    }

    #[test]
    fn test_config_default_vertical_overscan_is_8() {
        let config = Config::default();
        assert_eq!(config.vertical_overscan, 8);
    }

    #[test]
    fn test_config_file_horizontal_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("horizontal_overscan", "4")
            .unwrap();
        assert_eq!(config.horizontal_overscan, 4);
    }

    #[test]
    fn test_config_file_vertical_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("vertical_overscan", "12")
            .unwrap();
        assert_eq!(config.vertical_overscan, 12);
    }

    #[test]
    fn test_config_file_overscan_zero() {
        let mut config = Config::default();
        config
            .apply_config_value("horizontal_overscan", "0")
            .unwrap();
        config.apply_config_value("vertical_overscan", "0").unwrap();
        assert_eq!(config.horizontal_overscan, 0);
        assert_eq!(config.vertical_overscan, 0);
    }

    #[test]
    fn test_config_file_horizontal_overscan_max_is_8() {
        let mut config = Config::default();
        config
            .apply_config_value("horizontal_overscan", "8")
            .unwrap();
        assert_eq!(config.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_max_is_16() {
        let mut config = Config::default();
        config
            .apply_config_value("vertical_overscan", "16")
            .unwrap();
        assert_eq!(config.vertical_overscan, 16);
    }

    #[test]
    fn test_config_file_horizontal_overscan_above_max_is_clamped_to_8() {
        let mut config = Config::default();
        config
            .apply_config_value("horizontal_overscan", "9")
            .unwrap();
        assert_eq!(config.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_above_max_is_clamped_to_16() {
        let mut config = Config::default();
        config
            .apply_config_value("vertical_overscan", "17")
            .unwrap();
        assert_eq!(config.vertical_overscan, 16);
    }

    #[test]
    fn test_config_cli_horizontal_overscan() {
        let args = vec![
            "neser".to_string(),
            "--horizontal-overscan".to_string(),
            "4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.horizontal_overscan, 4);
    }

    #[test]
    fn test_config_cli_vertical_overscan() {
        let args = vec![
            "neser".to_string(),
            "--vertical-overscan".to_string(),
            "0".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.vertical_overscan, 0);
    }

    #[test]
    fn test_config_video_scale_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--video-scale".to_string(),
            "2.5".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    // Config file tests

    #[test]
    fn test_config_file_hardware_nes_pal() {
        let mut config = Config::default();
        config.apply_config_value("hardware", "nes-pal").unwrap();
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(config.hardware_model_explicit);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
        assert!(config.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_nes_ntsc() {
        let mut config = Config {
            hardware_model: HardwareModel::NesPal,
            ..Default::default()
        };
        config.apply_config_value("hardware", "nes-ntsc").unwrap();
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.hardware_model_explicit);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
        assert!(config.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_case_insensitive() {
        let mut config = Config::default();
        config.apply_config_value("hardware", "NES-PAL").unwrap();
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);

        config.apply_config_value("hardware", "NES-NTSC").unwrap();
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_file_hardware_famicom_sets_mode_and_model() {
        let mut config = Config {
            hardware_model: HardwareModel::NesPal,
            ..Default::default()
        };
        config.apply_config_value("hardware", "famicom").unwrap();
        assert_eq!(config.hardware_mode, HardwareMode::Famicom);
        assert!(config.hardware_mode_explicit);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_audio() {
        let mut config = Config::default();
        config.apply_config_value("audio", "false").unwrap();
        assert!(!config.audio_enabled);

        config.apply_config_value("audio", "true").unwrap();
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_file_vsync() {
        let mut config = Config::default();
        config.apply_config_value("vsync", "false").unwrap();
        assert!(!config.vsync_enabled);

        config.apply_config_value("vsync", "true").unwrap();
        assert!(config.vsync_enabled);
    }

    #[test]
    fn test_config_file_gamepads() {
        let mut config = Config::default();
        config.apply_config_value("gamepads", "false").unwrap();
        assert!(!config.gamepads_enabled);

        config.apply_config_value("gamepads", "true").unwrap();
        assert!(config.gamepads_enabled);
    }

    #[test]
    fn test_config_file_fullscreen() {
        let mut config = Config::default();
        config.apply_config_value("fullscreen", "true").unwrap();
        assert!(config.fullscreen);

        config.apply_config_value("fullscreen", "false").unwrap();
        assert!(!config.fullscreen);
    }

    #[test]
    fn test_config_file_display() {
        let mut config = Config::default();
        config.apply_config_value("display", "1").unwrap();
        assert_eq!(config.fullscreen_display, Some(1));

        config.apply_config_value("display", "0").unwrap();
        assert_eq!(config.fullscreen_display, Some(0));
    }

    #[test]
    fn test_config_file_display_negative_ignored() {
        let mut config = Config::default();
        config.apply_config_value("display", "-1").unwrap();
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_file_filter_invalid_errors() {
        let mut config = Config::default();
        let result = config.apply_config_value("filter", "invalid-filter");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-filter'. Valid options are: crt, ntsc, smooth, none"
        );
    }

    #[test]
    fn test_config_file_filter_empty_ignored() {
        let mut config = Config::default();
        config.apply_config_value("filter", "").unwrap();
        assert_eq!(config.shader_path, None);
    }

    #[test]
    fn test_config_file_filter_crt() {
        let mut config = Config::default();
        config.apply_config_value("filter", "crt").unwrap();
        assert_eq!(
            config.shader_path,
            Some("shaders/crt-lottes.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_filter_ntsc() {
        let mut config = Config::default();
        config.apply_config_value("filter", "ntsc").unwrap();
        assert_eq!(
            config.shader_path,
            Some("shaders/ntsc-256px-composite.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_filter_smooth() {
        let mut config = Config::default();
        config.apply_config_value("filter", "smooth").unwrap();
        assert_eq!(
            config.shader_path,
            Some("shaders/xbrz-freescale.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_filter_none() {
        let mut config = Config::default();
        config.apply_config_value("filter", "none").unwrap();
        assert_eq!(config.shader_path, Some("shaders/stock.slangp".to_string()));
    }

    #[test]
    fn test_config_cmdline_filter_crt() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "crt".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.shader_path,
            Some("shaders/crt-lottes.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.shader_path,
            Some("shaders/ntsc-256px-composite.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_smooth() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "smooth".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.shader_path,
            Some("shaders/xbrz-freescale.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_none() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "none".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.shader_path, Some("shaders/stock.slangp".to_string()));
    }

    #[test]
    fn test_config_file_debugger() {
        let mut config = Config::default();
        config.apply_config_value("debugger", "true").unwrap();
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_file_apu_channels() {
        let mut config = Config::default();
        config.apply_config_value("pulse1", "false").unwrap();
        config.apply_config_value("pulse2", "false").unwrap();
        config.apply_config_value("triangle", "false").unwrap();
        config.apply_config_value("noise", "false").unwrap();
        config.apply_config_value("dmc", "false").unwrap();

        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(!config.apu_channels.contains(ApuChannels::NOISE));
        assert!(!config.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_file_window_height() {
        let mut config = Config::default();
        config.apply_config_value("window_height", "720").unwrap();
        assert_eq!(config.window_height, 720);
    }

    #[test]
    fn test_config_file_controller_ports() {
        let mut config = Config::default();
        let _ = config.apply_config_value("controller_port1", "arkanoid");
        let _ = config.apply_config_value("controller_port2", "joypad");

        assert_eq!(config.controller_port1, ControllerType::Arkanoid);
        assert_eq!(config.controller_port2, ControllerType::Joypad);
        assert!(config.controller_port1_explicit);
        assert!(config.controller_port2_explicit);
    }

    #[test]
    fn test_config_file_controller_port_invalid_value_errors() {
        let mut config = Config::default();
        let result = config.apply_config_value("controller_port1", "unknown");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for 'controller_port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port1_flag_zapper() {
        let args = vec!["neser".to_string(), "--controller-port1=zapper".to_string()];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::Zapper);
    }

    #[test]
    fn test_config_controller_port1_flag_arkanoid() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=arkanoid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::Arkanoid);
    }

    #[test]
    fn test_config_controller_port1_flag_snes_controller() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=snes-controller".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::SnesController);
        assert!(config.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port1_flag_snes_mouse() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=snes-mouse".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::SnesMouse);
        assert!(config.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port2_flag_zapper() {
        let args = vec!["neser".to_string(), "--controller-port2=zapper".to_string()];
        let config = parse_config(args);
        assert_eq!(config.controller_port2, ControllerType::Zapper);
    }

    #[test]
    fn test_config_controller_port1_flag_power_pad() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=power-pad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::PowerPad);
        assert!(config.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port_cli_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--controller-port1=joypad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.controller_port1, ControllerType::Joypad);
    }

    #[test]
    fn test_config_controller_port_flag_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=unknown".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for '--controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_invalid_cli_value_does_not_override_config_file_and_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--controller-port1=unknown".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for '--controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_flag_snes_adapter_is_rejected() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=snes-adapter".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'snes-adapter' for '--controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_flags_two_mouse_controllers_error() {
        let args = vec![
            "neser".to_string(),
            "--controller-port1=zapper".to_string(),
            "--controller-port2=arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_zapper_detection_size() {
        let mut config = Config::default();
        let _ = config.apply_config_value("zapper_detection_size", "2");
        assert_eq!(config.zapper_detection_size, 2);
    }

    #[test]
    fn test_config_zapper_detection_size_invalid() {
        let mut config = Config::default();
        let _ = config.apply_config_value("zapper_detection_size", "invalid");
        assert_eq!(config.zapper_detection_size, 0); // Should remain default
    }

    #[test]
    fn test_config_zapper_detection_size_from_file() {
        let config_content = "zapper_detection_size=1\n";
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("neser.conf");
        std::fs::write(&config_path, config_content).unwrap();

        let mut config = Config::default();
        config.load_from_file(&config_path).unwrap();

        assert_eq!(config.zapper_detection_size, 1);
    }

    #[test]
    fn test_config_file_trace_cpu() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "2").unwrap();
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 2);
    }

    #[test]
    fn test_config_file_trace_ppu() {
        let mut config = Config::default();
        config.apply_config_value("trace-ppu", "3").unwrap();
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.ppu, 3);
    }

    #[test]
    fn test_config_file_trace_apu() {
        let mut config = Config::default();
        config.apply_config_value("trace-apu", "1").unwrap();
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.apu, 1);
    }

    #[test]
    fn test_config_file_trace_mapper() {
        let mut config = Config::default();
        config.apply_config_value("trace-mapper", "4").unwrap();
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.mapper, 4);
    }

    #[test]
    fn test_config_file_trace_nestest() {
        let mut config = Config::default();
        config.apply_config_value("trace-nestest", "true").unwrap();
        assert!(config.tracing.enabled);
        assert!(config.tracing.nestest);
    }

    #[test]
    fn test_config_file_trace_zero_does_not_enable() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "0").unwrap();
        assert!(!config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 0);
    }

    #[test]
    fn test_config_file_bool_formats() {
        let mut config = Config::default();

        // Test "yes"/"no"
        config.apply_config_value("audio", "no").unwrap();
        assert!(!config.audio_enabled);
        config.apply_config_value("audio", "yes").unwrap();
        assert!(config.audio_enabled);

        // Test "1"/"0"
        config.apply_config_value("audio", "0").unwrap();
        assert!(!config.audio_enabled);
        config.apply_config_value("audio", "1").unwrap();
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_file_unknown_key_ignored() {
        let mut config = Config::default();
        // Should not panic
        config
            .apply_config_value("unknown_key", "some_value")
            .unwrap();
        // Config should remain unchanged
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_file_load_from_string_content() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
# Test config file
hardware=nes-pal
audio=false
fullscreen=true
display=2
filter=crt
pulse1=false
"#;

        // Create a temporary file with config content
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let mut config = Config::default();
        config.load_from_file(file.path()).unwrap();

        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(!config.audio_enabled);
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, Some(2));
        assert_eq!(
            config.shader_path,
            Some("shaders/crt-lottes.slangp".to_string())
        );
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        // Other values should remain default
        assert!(config.vsync_enabled);
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_args_override_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create config file that sets PAL and disables audio
        let content = r#"
    hardware=nes-pal
audio=false
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Start with default config
        let mut config = Config::default();

        // Load from config file
        config.load_from_file(file.path()).unwrap();
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(!config.audio_enabled);

        // Apply args - no args means config file values should remain
        let args = vec!["neser".to_string()];
        config.apply_args(&args).unwrap();

        // Config file values should persist since args don't override them
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_hardware_flag_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "hardware=nes-pal\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--hardware".to_string(),
            "nes-ntsc".to_string(),
        ];

        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_two_arkanoid_controllers_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
controller_port1=arkanoid
controller_port2=arkanoid
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];

        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_nonexistent_silently_ignored() {
        let mut config = Config::default();
        config
            .load_from_file(Path::new("/nonexistent/path/neser.conf"))
            .unwrap();
        // Should not panic, config should remain default
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_flag_loads_specified_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "hardware=nes-pal\naudio=false\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_file_invalid_filter_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
    hardware=nes-pal
filter=invalid-shader
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-shader'. Valid options are: crt, ntsc, smooth, none"
        );
    }

    #[test]
    fn test_config_flag_invalid_file_errors() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "/nonexistent/path/config.conf".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("/nonexistent/path/config.conf"));
    }

    #[test]
    fn test_config_flag_missing_value_errors() {
        let args = vec!["neser".to_string(), "--config".to_string()];
        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_flag_overrides_default_locations() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a config file with --config that sets PAL
        let content = "hardware=nes-pal\n";
        let mut explicit_file = NamedTempFile::new().unwrap();
        explicit_file.write_all(content.as_bytes()).unwrap();

        // The --config file should be used
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            explicit_file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_parse_config_arg() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "my_config.conf".to_string(),
        ];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, Some("my_config.conf".to_string()));
    }

    #[test]
    fn test_parse_config_arg_not_present() {
        let args = vec!["neser".to_string()];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, None);
    }

    // Tests for aligned command line arguments

    #[test]
    fn test_config_audio_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_audio_flag_yes() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "yes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_vsync_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--vsync".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.vsync_enabled);
    }

    #[test]
    fn test_config_gamepads_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--gamepads".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.gamepads_enabled);
    }

    #[test]
    fn test_config_pulse1_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--pulse1".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_pulse2_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--pulse2".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_triangle_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--triangle".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_noise_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--noise".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_dmc_flag_true() {
        let args = vec!["neser".to_string(), "--dmc".to_string(), "true".to_string()];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_debugger_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--debugger".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_hardware_flag_nes_pal() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "nes-pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_hardware_flag_nes_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "nes-ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_hardware_flag_invalid_timing_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_hardware_flag_famicom_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_hardware_flag_nes_ntsc_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "nes-ntsc".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_hardware_flag_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_expansion_port_flag_famicom_four_players_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
            "--expansion-port".to_string(),
            "famicom-four-players".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_expansion_port_flag_arkanoid_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
            "--expansion-port".to_string(),
            "arkanoid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.expansion_port, ExpansionPort::ArkanoidFamicom);
    }

    #[test]
    fn test_config_expansion_port_flag_power_pad_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
            "--expansion-port".to_string(),
            "power-pad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.expansion_port, ExpansionPort::PowerPadFamicom);
    }

    #[test]
    fn test_config_expansion_port_flag_arkanoid_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--expansion-port".to_string(),
            "invalid-arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_famicom_mode_rejects_controller_port_overrides() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
            "--controller-port1".to_string(),
            "zapper".to_string(),
            "--controller-port2".to_string(),
            "arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_tv_system_key_is_ignored() {
        let mut config = Config::default();
        config
            .apply_config_value("tv_system", "pal")
            .expect("legacy key should be ignored gracefully");
        assert_eq!(config.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_tv_system_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--tv-system".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_bool_flag_invalid_value_treated_as_positional() {
        // When an invalid (non-boolean) value follows a boolean flag, it's treated as a positional arg
        // and the flag defaults to true
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        // Flag defaults to true, and "game.nes" is treated as ROM path
        assert!(config.audio_enabled);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_bool_flag_no_value_at_end() {
        // When a boolean flag is the last argument, it defaults to true
        let args = vec!["neser".to_string(), "--audio".to_string()];
        let config = parse_config(args);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_file_load_state() {
        let mut config = Config::default();
        config.apply_config_value("load_state", "true").unwrap();
        assert!(config.load_state);
    }

    // Tests for negation flags (--no-*, --disable-*)

    #[test]
    fn test_config_no_audio_disables_audio() {
        let args = vec!["neser".to_string(), "--no-audio".to_string()];
        let config = parse_config(args);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_disable_audio_disables_audio() {
        let args = vec!["neser".to_string(), "--disable-audio".to_string()];
        let config = parse_config(args);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_no_vsync_disables_vsync() {
        let args = vec!["neser".to_string(), "--no-vsync".to_string()];
        let config = parse_config(args);
        assert!(!config.vsync_enabled);
    }

    #[test]
    fn test_config_disable_vsync_disables_vsync() {
        let args = vec!["neser".to_string(), "--disable-vsync".to_string()];
        let config = parse_config(args);
        assert!(!config.vsync_enabled);
    }

    #[test]
    fn test_config_no_gamepads_disables_gamepads() {
        let args = vec!["neser".to_string(), "--no-gamepads".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_disable_gamepads_is_rejected() {
        let args = vec!["neser".to_string(), "--disable-gamepads".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_enable_4_score_enables_four_score() {
        let args = vec!["neser".to_string(), "--enable-4-score".to_string()];
        let config = parse_config(args);
        assert!(config.four_score_enabled);
    }

    #[test]
    fn test_config_enable_4_score_with_value_false_disables_four_score() {
        let args = vec![
            "neser".to_string(),
            "--enable-4-score".to_string(),
            "false".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.four_score_enabled);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_no_4_score_disables_four_score() {
        let args = vec!["neser".to_string(), "--no-4-score".to_string()];
        let config = parse_config(args);
        assert!(!config.four_score_enabled);
    }

    #[test]
    fn test_config_disable_4_score_disables_four_score() {
        let args = vec!["neser".to_string(), "--disable-4-score".to_string()];
        let config = parse_config(args);
        assert!(!config.four_score_enabled);
    }

    #[test]
    fn test_config_no_4_score_overrides_enable_4_score() {
        let args = vec![
            "neser".to_string(),
            "--enable-4-score".to_string(),
            "--no-4-score".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.four_score_enabled);
    }

    #[test]
    fn test_config_disable_pulse1_removes_channel() {
        let args = vec!["neser".to_string(), "--disable-pulse1".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_no_pulse2_removes_channel() {
        let args = vec!["neser".to_string(), "--no-pulse2".to_string()];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_disable_triangle_removes_channel() {
        let args = vec!["neser".to_string(), "--disable-triangle".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_audio_value_equals_syntax() {
        let args = vec!["neser".to_string(), "--audio=0".to_string()];
        let config = parse_config(args);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_audio_value_equals_syntax_true() {
        let args = vec!["neser".to_string(), "--audio=1".to_string()];
        let config = parse_config(args);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_mixed_value_and_negation() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "true".to_string(),
            "--no-vsync".to_string(),
            "--disable-pulse1".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.audio_enabled);
        assert!(!config.vsync_enabled);
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
    }

    // Tests for valueless boolean flags (defaults to true)

    #[test]
    fn test_config_audio_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--audio".to_string()];
        let config = parse_config(args);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_vsync_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--vsync".to_string()];
        let config = parse_config(args);
        assert!(config.vsync_enabled);
    }

    #[test]
    fn test_config_debugger_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--debugger".to_string()];
        let config = parse_config(args);
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_audio_no_value_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.audio_enabled);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_fullscreen_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--fullscreen".to_string()];
        let config = parse_config(args);
        assert!(config.fullscreen);
    }

    #[test]
    fn test_config_load_state_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--load-state".to_string()];
        let config = parse_config(args);
        assert!(config.load_state);
    }

    #[test]
    fn test_config_no_load_state_disables_load_state() {
        let args = vec!["neser".to_string(), "--no-load-state".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_disable_load_state_disables_load_state() {
        let args = vec!["neser".to_string(), "--disable-load-state".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_state_false_disables() {
        let args = vec![
            "neser".to_string(),
            "--load-state".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.load_state);
    }

    #[test]
    fn test_config_load_state_equals_zero() {
        let args = vec!["neser".to_string(), "--load-state=0".to_string()];
        let config = parse_config(args);
        assert!(!config.load_state);
    }

    #[test]
    fn test_config_load_state_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--load-state".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.load_state);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_no_load_state_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--no-load-state".to_string(),
            "game.nes".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_pulse1_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--pulse1".to_string()];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_audio_with_another_flag() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "--vsync".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.audio_enabled);
        assert!(config.vsync_enabled);
    }

    #[test]
    fn test_config_ram_init_mode_default() {
        let config = Config::with_defaults();
        #[cfg(target_arch = "wasm32")]
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(config.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_config_file_ram_init_mode_zero() {
        let mut config = Config::default();
        config.apply_config_value("ram_init_mode", "zero").unwrap();
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_config_file_ram_init_mode_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "random")
            .unwrap();
        assert_eq!(config.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded-random:42")
            .unwrap();
        assert_eq!(config.ram_init_mode, RamInitMode::SeededRandom(42));
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random_underscore() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded_random:12345")
            .unwrap();
        assert_eq!(config.ram_init_mode, RamInitMode::SeededRandom(12345));
    }

    #[test]
    fn test_config_cmdline_ram_init_mode_zero() {
        let args = vec![
            "neser".to_string(),
            "--ram-init-mode".to_string(),
            "zero".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_config_cmdline_ram_init_mode_seeded() {
        let args = vec![
            "neser".to_string(),
            "--ram-init-mode".to_string(),
            "seeded-random:999".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.ram_init_mode, RamInitMode::SeededRandom(999));
    }

    #[test]
    fn test_config_oam_dram_decay_default_disabled() {
        let config = Config::with_defaults();
        assert!(!config.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_file_oam_dram_decay_enabled() {
        let mut config = Config::default();
        config.apply_config_value("oam_dram_decay", "true").unwrap();
        assert!(config.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_enabled() {
        let args = vec![
            "neser".to_string(),
            "--oam-dram-decay".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_disabled() {
        let args = vec![
            "neser".to_string(),
            "--oam-dram-decay".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--oam-dram-decay".to_string(),
            "true".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.oam_dram_decay_enabled);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_cli_breakpoint_pc_flag_adds_pc_breakpoint() {
        use crate::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=C000".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config.breakpoints.contains(&BreakpointKind::Pc(0xC000)),
            "expected PC breakpoint at 0xC000"
        );
    }

    #[test]
    fn test_cli_breakpoint_cycle_flag_adds_cycle_breakpoint() {
        use crate::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycle=12345".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config.breakpoints.contains(&BreakpointKind::Cycle(12345)),
            "expected Cycle breakpoint at 12345"
        );
    }

    #[test]
    fn test_cli_frame_flag_adds_frame_breakpoint() {
        use crate::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "frame=42".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config.breakpoints.contains(&BreakpointKind::Frame(42)),
            "expected Frame breakpoint at frame 42"
        );
    }

    #[test]
    fn test_cli_breakpoint_frame_flag_adds_frame_breakpoint() {
        use crate::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "frame=42".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config.breakpoints.contains(&BreakpointKind::Frame(42)),
            "expected Frame breakpoint at frame 42"
        );
    }

    #[test]
    fn test_cli_breakpoint_write_flag_adds_write_breakpoint() {
        use crate::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "write=2006".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .breakpoints
                .contains(&BreakpointKind::WriteAddress(0x2006)),
            "expected WriteAddress breakpoint at 0x2006"
        );
    }

    #[test]
    fn test_cli_multiple_breakpoints_are_all_added() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=C000,write=2006".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.breakpoints.len(),
            2,
            "expected 2 breakpoints from comma-separated list"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_kind_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycfdsfd".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid breakpoint spec"
        );
        assert!(
            result.unwrap_err().contains("breakpoint"),
            "error message should mention 'breakpoint'"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_pc_address_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=ZZZZ".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid PC address in breakpoint"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_cycle_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycle=notanumber".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid cycle value in breakpoint"
        );
    }

    // --- Issue #649: Config argument cleanup ---

    // --frame should no longer be a standalone flag
    #[test]
    fn test_cli_frame_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--frame".to_string(), "42".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--frame should be rejected; use --breakpoint frame=N instead"
        );
    }

    // --no-debugger and --disable-debugger should be removed
    #[test]
    fn test_cli_no_debugger_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--no-debugger".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--no-debugger should be rejected; use --debugger false instead"
        );
    }

    #[test]
    fn test_cli_disable_debugger_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--disable-debugger".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--disable-debugger should be rejected; use --debugger false instead"
        );
    }

    // Old autorunner flags should be rejected
    #[test]
    fn test_cli_record_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--record".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--record should be rejected; use --create-recording or --extend-recording instead"
        );
    }

    #[test]
    fn test_cli_extend_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--extend".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--extend should be rejected; use --extend-recording instead"
        );
    }

    #[test]
    fn test_cli_headless_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--headless".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--headless should be rejected; use --playback-headless instead"
        );
    }

    #[test]
    fn test_cli_overwrite_recording_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--overwrite-recording".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--overwrite-recording should be rejected; use --create-recording instead"
        );
    }

    // New autorunner flags should work
    #[test]
    fn test_cli_create_recording_sets_record_mode_and_overwrite() {
        let args = vec!["neser".to_string(), "--create-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Record,
            "--create-recording should set Record mode"
        );
        assert!(
            config.autorun_overwrite,
            "--create-recording should set autorun_overwrite"
        );
        assert!(
            !config.autorun_extend,
            "--create-recording should not set autorun_extend"
        );
    }

    #[test]
    fn test_cli_extend_recording_sets_record_mode_and_extend() {
        let args = vec!["neser".to_string(), "--extend-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Record,
            "--extend-recording should set Record mode"
        );
        assert!(
            config.autorun_extend,
            "--extend-recording should set autorun_extend"
        );
        assert!(
            !config.autorun_overwrite,
            "--extend-recording should not set autorun_overwrite"
        );
    }

    #[test]
    fn test_cli_playback_headless_sets_playback_mode_and_headless() {
        let args = vec!["neser".to_string(), "--playback-headless".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Playback,
            "--playback-headless should set Playback mode"
        );
        assert!(
            config.autorun_headless,
            "--playback-headless should set autorun_headless"
        );
    }

    #[test]
    fn test_cli_playback_still_works() {
        let args = vec!["neser".to_string(), "--playback".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Playback,
            "--playback should still set Playback mode"
        );
        assert!(
            !config.autorun_headless,
            "--playback alone should not set autorun_headless"
        );
    }

    #[test]
    fn test_cli_create_recording_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--create-recording and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_extend_recording_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--extend-recording".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--extend-recording and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_create_recording_and_extend_recording_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--extend-recording".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--create-recording and --extend-recording should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_playback_from_checkpoint_sets_autorun_from_checkpoint() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "game".to_string(),
            "--playback-from-checkpoint".to_string(),
            "3".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_from_checkpoint, Some(3));
    }

    #[test]
    fn test_cli_trim_checkpoints_sets_autorun_trim_checkpoints() {
        let args = vec![
            "neser".to_string(),
            "--trim-checkpoints".to_string(),
            "2".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_trim_checkpoints, Some(2));
    }

    #[test]
    fn test_cli_playback_from_checkpoint_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "game".to_string(),
            "--playback-from-checkpoint=4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_from_checkpoint,
            Some(4),
            "--playback-from-checkpoint=N (equals syntax) should be parsed"
        );
    }

    #[test]
    fn test_cli_trim_checkpoints_equals_syntax() {
        let args = vec!["neser".to_string(), "--trim-checkpoints=3".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_trim_checkpoints,
            Some(3),
            "--trim-checkpoints=N (equals syntax) should be parsed"
        );
    }

    #[test]
    fn test_cli_convert_autorun_sets_autorun_convert_true() {
        let args = vec!["neser".to_string(), "--convert-autorun".to_string()];
        let config = parse_config(args);
        assert!(config.autorun_convert);
    }

    #[test]
    fn test_cli_convert_autorun_equals_syntax_true() {
        let args = vec!["neser".to_string(), "--convert-autorun=true".to_string()];
        let config = parse_config(args);
        assert!(config.autorun_convert);
    }

    #[test]
    fn test_cli_convert_autorun_equals_syntax_false() {
        let args = vec!["neser".to_string(), "--convert-autorun=false".to_string()];
        let config = parse_config(args);
        assert!(!config.autorun_convert);
    }

    #[test]
    fn test_cli_recalculate_autorun_sets_autorun_recalculate_true() {
        let args = vec!["neser".to_string(), "--recalculate-autorun".to_string()];
        let config = parse_config(args);
        assert!(config.autorun_recalculate);
    }

    #[test]
    fn test_cli_recalculate_autorun_equals_syntax_false() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun=false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.autorun_recalculate);
    }

    #[test]
    fn test_cli_trim_checkpoints_and_convert_autorun_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--trim-checkpoints".to_string(),
            "1".to_string(),
            "--convert-autorun".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--trim-checkpoints and --convert-autorun should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_recalculate_autorun_and_convert_autorun_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun".to_string(),
            "--convert-autorun".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--recalculate-autorun and --convert-autorun should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_recalculate_autorun_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--recalculate-autorun and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_playback_from_checkpoint_implies_playback_mode() {
        // --playback-from-checkpoint alone (without --playback) should enable Playback mode
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint".to_string(),
            "4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Playback,
            "--playback-from-checkpoint should imply Playback mode"
        );
        assert_eq!(config.autorun_from_checkpoint, Some(4));
    }

    #[test]
    fn test_cli_playback_from_checkpoint_negative_value() {
        // Negative value -1 means second-to-last checkpoint
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint".to_string(),
            "-1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_from_checkpoint,
            Some(-1),
            "--playback-from-checkpoint=-1 should parse as -1"
        );
        assert_eq!(config.autorun_mode, AutorunMode::Playback);
    }

    #[test]
    fn test_cli_playback_from_checkpoint_negative_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint=-2".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_from_checkpoint, Some(-2));
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_sets_playback_headless_and_checkpoint() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint".to_string(),
            "3".to_string(),
            "game".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.autorun_mode,
            AutorunMode::Playback,
            "--playback-headless-from-checkpoint should set Playback mode"
        );
        assert!(
            config.autorun_headless,
            "--playback-headless-from-checkpoint should set headless mode"
        );
        assert_eq!(
            config.autorun_from_checkpoint,
            Some(3),
            "--playback-headless-from-checkpoint should set checkpoint index"
        );
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint=5".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_mode, AutorunMode::Playback);
        assert!(config.autorun_headless);
        assert_eq!(config.autorun_from_checkpoint, Some(5));
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_negative_value() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint=-1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_mode, AutorunMode::Playback);
        assert!(config.autorun_headless);
        assert_eq!(config.autorun_from_checkpoint, Some(-1));
    }

    #[test]
    fn test_cli_create_recording_forces_zero_ram_init_mode() {
        let args = vec!["neser".to_string(), "--create-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(config.autorun_mode, AutorunMode::Record);
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_playback_forces_zero_ram_init_mode() {
        let args = vec!["neser".to_string(), "--playback".to_string()];
        let config = parse_config(args);
        assert_eq!(config.autorun_mode, AutorunMode::Playback);
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_playback_from_checkpoint_forces_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint=1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_mode, AutorunMode::Playback);
        assert_eq!(config.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_create_recording_rejects_non_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--ram-init-mode".to_string(),
            "random".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .contains("Autorun recording/playback requires --ram-init-mode zero")
        );
    }

    #[test]
    fn test_cli_playback_rejects_non_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "--ram-init-mode=seeded-random:42".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .contains("Autorun recording/playback requires --ram-init-mode zero")
        );
    }

    #[test]
    fn given_cartridge_search_paths_cli_when_parsed_then_paths_are_configured() {
        let args = vec![
            "neser".to_string(),
            "--cartridge-search-paths".to_string(),
            "roms,games/custom".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.cartridge_search_paths,
            vec!["roms".to_string(), "games/custom".to_string()]
        );
    }

    #[test]
    fn given_no_cartridge_paths_when_using_defaults_then_paths_are_empty() {
        let config = Config::with_defaults();
        assert!(config.cartridge_search_paths.is_empty());
    }

    #[test]
    fn given_no_scan_cartridges_cli_when_parsed_then_startup_scan_is_disabled() {
        let args = vec!["neser".to_string(), "--no-scan-cartridges".to_string()];
        let config = parse_config(args);
        assert!(!config.scan_cartridges);
    }

    #[test]
    fn given_rebuild_catalog_config_key_when_loaded_then_rebuild_is_enabled() {
        let mut config = Config::default();
        config
            .apply_config_value("rebuild_cartridge_catalog", "true")
            .unwrap();
        assert!(config.rebuild_cartridge_catalog);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_sets_expansion_when_already_famicom() {
        let mut config = Config {
            hardware_mode: HardwareMode::Famicom,
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(changed);
        assert_eq!(config.expansion_port, ExpansionPort::ZapperFamicom);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_no_change_when_nes_mode() {
        let mut config = Config::default(); // Default is NES mode

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_respects_explicit_expansion_override() {
        let mut config = Config {
            hardware_mode: HardwareMode::Famicom,
            expansion_port: ExpansionPort::None,
            expansion_port_explicit: true,
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_false_is_noop() {
        let mut config = Config {
            hardware_mode: HardwareMode::Famicom,
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(false);

        assert!(!changed);
        assert_eq!(config.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_expansion_port_parse_zapper() {
        assert_eq!(
            ExpansionPort::parse("zapper"),
            Some(ExpansionPort::ZapperFamicom)
        );
    }

    #[test]
    fn test_config_expansion_port_parse_power_pad() {
        assert_eq!(
            ExpansionPort::parse("power-pad"),
            Some(ExpansionPort::PowerPadFamicom)
        );
        assert_eq!(
            ExpansionPort::parse("powerpad"),
            Some(ExpansionPort::PowerPadFamicom)
        );
    }

    #[test]
    fn test_autorun_format_default_is_binary() {
        let config = Config::default();
        assert_eq!(config.autorun_format, AutorunFormat::Binary);
    }

    #[test]
    fn test_autorun_format_cli_binary() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "binary".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_format, AutorunFormat::Binary);
    }

    #[test]
    fn test_autorun_format_cli_json() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "json".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.autorun_format, AutorunFormat::Json);
    }

    #[test]
    fn test_autorun_format_cli_unknown_value_returns_error() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "xml".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err(), "unknown format should return an error");
        assert!(
            result.unwrap_err().contains("xml"),
            "error should mention the unknown format"

    #[test]
    fn test_tui_mode_default_is_false() {
        let config = Config::with_defaults();
        assert!(!config.tui_mode, "tui_mode should default to false");
    }

    #[cfg(feature = "tui")]
    #[test]
    fn test_tui_flag_sets_tui_mode_true() {
        let config = parse_config(vec!["neser".to_string(), "--tui".to_string()]);
        assert!(config.tui_mode, "--tui flag should set tui_mode to true");
    }

    #[cfg(not(feature = "tui"))]
    #[test]
    fn test_tui_flag_errors_without_tui_feature() {
        let result = Config::new(&["neser".to_string(), "--tui".to_string()]);
        assert!(
            result.is_err(),
            "--tui should return an error when tui feature is not enabled"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("tui"),
            "error message should mention 'tui': {err}"
        );
    }

    #[test]
    fn test_no_tui_flag_leaves_tui_mode_false() {
        let config = parse_config(vec!["neser".to_string()]);
        assert!(
            !config.tui_mode,
            "tui_mode should remain false without --tui"
        );
    }
}
