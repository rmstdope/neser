//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! Configuration values are loaded with the following priority (highest to lowest):
//! 1. Command-line arguments
//! 2. Config file (neser.conf)
//! 3. Default values

use crate::console::TvSystem;
use crate::debugging::Tracing;
use crate::input::ControllerType;
use bitflags::bitflags;
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
    // Aligned flags matching config file keys with same value ranges
    // Support both value-based (--audio true) and prefix negation (--no-audio, --disable-audio)
    CliFlag {
        flag: "--tv-system",
        help: Some("TV system: ntsc or pal (default: ntsc)"),
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
        flag: "--no-gamepads",
        help: Some("Disable gamepad/joystick support (equivalent to --gamepads false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-gamepads",
        help: Some("Disable gamepad/joystick support (equivalent to --gamepads false)"),
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
        flag: "--no-debugger",
        help: Some("Do not open debugger on startup (equivalent to --debugger false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-debugger",
        help: Some("Do not open debugger on startup (equivalent to --debugger false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--load-state",
        help: Some(
            "Load save-state on startup (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-load-state",
        help: Some("Do not load save-state on startup (equivalent to --load-state false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-load-state",
        help: Some("Do not load save-state on startup (equivalent to --load-state false)"),
        has_value: false,
    },
];

/// Boolean flags that accept optional values (shared by validate_args and parse_rom_arg).
const OPTIONAL_BOOL_FLAGS: &[&str] = &[
    "--audio",
    "--vsync",
    "--gamepads",
    "--pulse1",
    "--pulse2",
    "--triangle",
    "--noise",
    "--dmc",
    "--debugger",
    "--load-state",
    "--fullscreen",
];

/// Result of parsing command-line arguments.
#[derive(Debug)]
pub enum ParseResult {
    /// User requested help - print and exit.
    Help,
    /// Successfully parsed configuration.
    Config(Config),
}

/// Emulator configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// TV system (NTSC or PAL).
    pub tv_system: TvSystem,
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
    /// APU channel enable flags.
    pub apu_channels: ApuChannels,
    /// Window height in pixels (windowed mode only).
    pub window_height: u32,
    /// Emulation speed multiplier.
    pub timing_scale: f32,
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
            tv_system: TvSystem::Ntsc,
            audio_enabled: true,
            vsync_enabled: true,
            gamepads_enabled: true,
            fullscreen: false,
            fullscreen_display: None,
            shader_path: None,
            debugger_enabled: false,
            load_state: false,
            tracing: Tracing::default(),
            apu_channels: ApuChannels::ALL,
            window_height: 960,
            timing_scale: 1.0,
            rom_path: None,
            controller_port1: ControllerType::Joypad,
            controller_port2: ControllerType::Joypad,
            controller_port1_explicit: false,
            controller_port2_explicit: false,
        }
    }
}

impl Config {
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
        // TV system (value-based, aligned with config file)
        if let Some(tv_system) = Self::parse_string_arg(args, "--tv-system") {
            if tv_system.eq_ignore_ascii_case("pal") {
                self.tv_system = TvSystem::Pal;
            } else if tv_system.eq_ignore_ascii_case("ntsc") {
                self.tv_system = TvSystem::Ntsc;
            } else {
                return Err(format!(
                    "Invalid --tv-system value: '{}'. Valid options are: ntsc, pal",
                    tv_system
                ));
            }
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

        // Gamepads: --gamepads true/false, --no-gamepads, --disable-gamepads
        if let Some(gamepads) = Self::parse_bool_arg(args, "--gamepads")? {
            self.gamepads_enabled = gamepads;
        }
        if Self::has_negation_flag(args, &["--no-gamepads", "--disable-gamepads"]) {
            self.gamepads_enabled = false;
        }

        // Debugger: --debugger true/false, --no-debugger, --disable-debugger
        if let Some(debugger) = Self::parse_bool_arg(args, "--debugger")? {
            self.debugger_enabled = debugger;
        }
        if Self::has_negation_flag(args, &["--no-debugger", "--disable-debugger"]) {
            self.debugger_enabled = false;
        }

        // Load state: --load-state true/false, --no-load-state, --disable-load-state
        if let Some(load_state) = Self::parse_bool_arg(args, "--load-state")? {
            self.load_state = load_state;
        }
        if Self::has_negation_flag(args, &["--no-load-state", "--disable-load-state"]) {
            self.load_state = false;
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

        Ok(())
    }

    /// Print help text to stdout.
    pub fn print_help() {
        println!("NES Emulator");
        println!("\nUsage: neser [OPTIONS] [ROM]");
        println!("\nOptions:");

        for flag in CLI_FLAGS {
            if let Some(help) = flag.help {
                println!("  {:<19} {}", flag.flag, help);
            }
        }

        println!("\nExamples:");
        println!("  neser game.nes                               # Load and run a ROM");
        println!("  neser --tv-system pal game.nes               # Use PAL timing");
        println!(
            "  neser --debugger game.nes                    # Enable debugger (no value = true)"
        );
        println!("  neser --audio game.nes                       # Enable audio (no value = true)");
        println!("  neser --audio=1 game.nes                     # Enable audio (equals syntax)");
        println!("  neser --audio false game.nes                 # Disable audio (value-based)");
        println!("  neser --audio=0 game.nes                     # Disable audio (equals syntax)");
        println!(
            "  neser --no-audio game.nes                    # Disable audio (prefix negation)"
        );
        println!("  neser --disable-pulse1 --disable-pulse2 game.nes # Disable specific channels");
        println!();
        println!(
            "Note: Boolean flags can be used without value (defaults to true), with value (true/false/yes/no/1/0),"
        );
        println!(
            "      or with prefix negation (--no-*, --disable-*). All forms: --audio, --audio=1, --audio true are equivalent."
        );
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
            if args[i] == flag && i + 1 < args.len() {
                let value = &args[i + 1];
                let parsed: u32 = value
                    .parse()
                    .map_err(|_| format!("Invalid {} value: {}", flag, value))?;
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
    /// # TV system: ntsc or pal
    /// tv_system=ntsc
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
            "tv_system" => {
                if value.eq_ignore_ascii_case("pal") {
                    self.tv_system = TvSystem::Pal;
                } else if value.eq_ignore_ascii_case("ntsc") {
                    self.tv_system = TvSystem::Ntsc;
                }
            }
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
            "controller_port1" => {
                if let Some(controller) = ControllerType::parse(value) {
                    self.controller_port1 = controller;
                    self.controller_port1_explicit = true;
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'controller_port1' in configuration; \
                         keeping default controller type.",
                        value
                    );
                }
            }
            "controller_port2" => {
                if let Some(controller) = ControllerType::parse(value) {
                    self.controller_port2 = controller;
                    self.controller_port2_explicit = true;
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'controller_port2' in configuration; \
                         keeping default controller type.",
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
            // NOTE: timing_scale is disabled as it doesn't work with the current eventloop design
            // "timing_scale" => {
            //     if let Ok(s) = value.parse::<f32>() {
            //         self.timing_scale = s;
            //     }
            // }
            _ => {} // Unknown keys are silently ignored
        }
        Ok(())
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
        let paddle_count = [self.controller_port1, self.controller_port2]
            .iter()
            .filter(|controller| **controller == ControllerType::Paddle)
            .count();

        if paddle_count > 1 {
            return Err("No more than one controller simulated using Mouse can be configured (Arkanoid/Zapper)".to_string());
        }

        Ok(())
    }
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
        assert_eq!(config.tv_system, TvSystem::Ntsc);
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
        assert_eq!(config.tv_system, TvSystem::Ntsc);
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
    fn test_config_tv_system_pal() {
        let args = vec![
            "neser".to_string(),
            "--tv-system".to_string(),
            "pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Pal);
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
        let args = vec![
            "neser".to_string(),
            "--load-state".to_string(),
            "true".to_string(),
        ];
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
            "--tv-system".to_string(),
            "pal".to_string(),
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
        assert_eq!(config.tv_system, TvSystem::Pal);
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

    // NOTE: timing_scale tests disabled as the feature doesn't work with current eventloop design
    // #[test]
    // fn test_config_timing_scale() {
    //     let args = vec![
    //         "neser".to_string(),
    //         "--timing-scale".to_string(),
    //         "2.0".to_string(),
    //     ];
    //     let config = parse_config(args);
    //     assert!((config.timing_scale - 2.0).abs() < 0.001);
    // }

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
    fn test_config_video_scale_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--video-scale".to_string(),
            "2.5".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    // #[test]
    // fn test_config_timing_scale_invalid_errors() {
    //     let args = vec![
    //         "neser".to_string(),
    //         "--timing-scale".to_string(),
    //         "abc".to_string(),
    //     ];
    //     let result = Config::new(&args);
    //     assert!(result.is_err());
    // }

    // Config file tests

    #[test]
    fn test_config_file_tv_system_pal() {
        let mut config = Config::default();
        config.apply_config_value("tv_system", "pal").unwrap();
        assert_eq!(config.tv_system, TvSystem::Pal);
    }

    #[test]
    fn test_config_file_tv_system_ntsc() {
        let mut config = Config {
            tv_system: TvSystem::Pal,
            ..Default::default()
        };
        config.apply_config_value("tv_system", "ntsc").unwrap();
        assert_eq!(config.tv_system, TvSystem::Ntsc);
    }

    #[test]
    fn test_config_file_tv_system_case_insensitive() {
        let mut config = Config::default();
        config.apply_config_value("tv_system", "PAL").unwrap();
        assert_eq!(config.tv_system, TvSystem::Pal);

        config.apply_config_value("tv_system", "NTSC").unwrap();
        assert_eq!(config.tv_system, TvSystem::Ntsc);
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

        assert_eq!(config.controller_port1, ControllerType::Paddle);
        assert_eq!(config.controller_port2, ControllerType::Joypad);
        assert!(config.controller_port1_explicit);
        assert!(config.controller_port2_explicit);
    }

    #[test]
    fn test_config_file_controller_port_invalid_value_ignored() {
        let mut config = Config::default();
        let _ = config.apply_config_value("controller_port1", "unknown");

        assert_eq!(config.controller_port1, ControllerType::Joypad);
        assert!(!config.controller_port1_explicit);
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

    // #[test]
    // fn test_config_file_timing_scale() {
    //     let mut config = Config::default();
    //     config.apply_config_value("timing_scale", "1.5").unwrap();
    //     assert!((config.timing_scale - 1.5).abs() < 0.001);
    // }

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
        assert_eq!(config.tv_system, TvSystem::Ntsc);
    }

    #[test]
    fn test_config_file_load_from_string_content() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
# Test config file
tv_system=pal
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

        assert_eq!(config.tv_system, TvSystem::Pal);
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
tv_system=pal
audio=false
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Start with default config
        let mut config = Config::default();

        // Load from config file
        config.load_from_file(file.path()).unwrap();
        assert_eq!(config.tv_system, TvSystem::Pal);
        assert!(!config.audio_enabled);

        // Apply args - no args means config file values should remain
        let args = vec!["neser".to_string()];
        config.apply_args(&args).unwrap();

        // Config file values should persist since args don't override them
        assert_eq!(config.tv_system, TvSystem::Pal);
        assert!(!config.audio_enabled);
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
        assert_eq!(config.tv_system, TvSystem::Ntsc);
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_flag_loads_specified_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "tv_system=pal\naudio=false\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Pal);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_file_invalid_filter_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
tv_system=pal
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
        let content = "tv_system=pal\n";
        let mut explicit_file = NamedTempFile::new().unwrap();
        explicit_file.write_all(content.as_bytes()).unwrap();

        // The --config file should be used
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            explicit_file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Pal);
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
    fn test_config_tv_system_flag_pal() {
        let args = vec![
            "neser".to_string(),
            "--tv-system".to_string(),
            "pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Pal);
    }

    #[test]
    fn test_config_tv_system_flag_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--tv-system".to_string(),
            "ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Ntsc);
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
        let config = parse_config(args);
        assert!(!config.gamepads_enabled);
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
        let config = parse_config(args);
        assert!(!config.load_state);
    }

    #[test]
    fn test_config_disable_load_state_disables_load_state() {
        let args = vec!["neser".to_string(), "--disable-load-state".to_string()];
        let config = parse_config(args);
        assert!(!config.load_state);
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
        let config = parse_config(args);
        assert!(!config.load_state);
        assert_eq!(config.rom_path.as_deref(), Some("game.nes"));
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
}
