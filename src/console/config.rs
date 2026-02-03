//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! Configuration values are loaded with the following priority (highest to lowest):
//! 1. Command-line arguments
//! 2. Config file (neser.conf)
//! 3. Default values

use crate::console::TvSystem;
use crate::debugging::Tracing;
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
        flag: "--pal",
        help: Some("Use PAL TV system (default: NTSC)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-audio",
        help: Some("Disable audio output"),
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
        flag: "--disable-pulse1",
        help: Some("Mute pulse 1 channel"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-pulse2",
        help: Some("Mute pulse 2 channel"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-triangle",
        help: Some("Mute triangle channel"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-noise",
        help: Some("Mute noise channel"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-dmc",
        help: Some("Mute DMC channel"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-vsync",
        help: Some("Disable VSync (default: enabled)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-gamepads",
        help: Some("Disable gamepad/joystick support"),
        has_value: false,
    },
    CliFlag {
        flag: "--start-in-debugger",
        help: Some("Open debugger windows (CPU/PPU/APU) on startup"),
        has_value: false,
    },
    CliFlag {
        flag: "--load-state",
        help: Some("Load save-state on startup (uses ROM .state path)"),
        has_value: false,
    },
    CliFlag {
        flag: "--fullscreen",
        help: Some("Run emulator in fullscreen mode"),
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
    // NOTE: --timing-scale is disabled as it doesn't work with the current eventloop design
    // CliFlag {
    //     flag: "--timing-scale",
    //     help: Some("Emulation speed multiplier (e.g., --timing-scale 2.0)"),
    //     has_value: true,
    // },
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
            config.load_from_file(path);
        } else {
            // Load from default locations (later overrides earlier)
            // First: ~/.neser/neser.conf
            if let Some(home) = std::env::var_os("HOME") {
                let home_config = Path::new(&home).join(".neser").join(Self::CONFIG_FILE_NAME);
                config.load_from_file(&home_config);
            }
            // Second: ./neser.conf (overrides user config)
            config.load_from_file(Path::new(Self::CONFIG_FILE_NAME));
        }

        // Step 3: Apply command-line arguments (override config file and defaults)
        config.apply_args(args)?;

        Ok(ParseResult::Config(config))
    }

    /// Apply command-line arguments to the config.
    /// Arguments override any values set by defaults or config file.
    fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        // TV system
        if Self::has_flag(args, "--pal") {
            self.tv_system = TvSystem::Pal;
        }

        // Boolean flags (only override if explicitly specified)
        if Self::has_flag(args, "--no-audio") {
            self.audio_enabled = false;
        }
        if Self::has_flag(args, "--no-vsync") {
            self.vsync_enabled = false;
        }
        if Self::has_flag(args, "--no-gamepads") {
            self.gamepads_enabled = false;
        }
        if Self::has_flag(args, "--fullscreen") {
            self.fullscreen = true;
        }
        if Self::has_flag(args, "--start-in-debugger") {
            self.debugger_enabled = true;
        }
        if Self::has_flag(args, "--load-state") {
            self.load_state = true;
        }

        // Display argument (only applies if fullscreen is set)
        if self.fullscreen
            && let Some(display) = Self::parse_display_arg(args)?
        {
            self.fullscreen_display = Some(display);
        }

        // Shader path
        if let Some(filter_name) = Self::parse_shader_arg(args) {
            self.shader_path = Self::map_filter_name(&filter_name);
        }

        if let Some(path) = Self::parse_rom_arg(args)? {
            self.rom_path = Some(path);
        }

        // Tracing (merge with existing config file values)
        self.tracing.apply_args(args);

        // APU channel disable flags
        if Self::has_flag(args, "--disable-pulse1") {
            self.apu_channels.remove(ApuChannels::PULSE1);
        }
        if Self::has_flag(args, "--disable-pulse2") {
            self.apu_channels.remove(ApuChannels::PULSE2);
        }
        if Self::has_flag(args, "--disable-triangle") {
            self.apu_channels.remove(ApuChannels::TRIANGLE);
        }
        if Self::has_flag(args, "--disable-noise") {
            self.apu_channels.remove(ApuChannels::NOISE);
        }
        if Self::has_flag(args, "--disable-dmc") {
            self.apu_channels.remove(ApuChannels::DMC);
        }

        // Window height
        if let Some(height) = Self::parse_u32_arg(args, "--window-height")? {
            self.window_height = height;
        }

        // NOTE: timing_scale is disabled as it doesn't work with the current eventloop design
        // if let Some(scale) = Self::parse_float_arg(args, "--timing-scale")? {
        //     self.timing_scale = scale;
        // }

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

        println!("\nExample:");
        println!("  neser --disable-pulse2 --disable-triangle    # Only pulse1, noise, and DMC");
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
    fn load_from_file(&mut self, path: &Path) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return, // File doesn't exist or can't be read - silently ignore
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
                self.apply_config_value(key, value);
            }
        }
    }

    /// Map simplified filter names to shader paths.
    ///
    /// Supported values: crt, ntsc, smooth, none
    ///
    /// Returns `Some(String)` with the full shader path for valid filter names,
    /// or `None` for invalid/unknown names.
    fn map_filter_name(name: &str) -> Option<String> {
        match name {
            "crt" => Some("shaders/crt-lottes.slangp".to_string()),
            "ntsc" => Some("shaders/ntsc-256px-composite.slangp".to_string()),
            "smooth" => Some("shaders/xbrz-freescale.slangp".to_string()),
            "none" => Some("shaders/stock.slangp".to_string()),
            _ => None,
        }
    }

    /// Apply a single config file key-value pair.
    fn apply_config_value(&mut self, key: &str, value: &str) {
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
                    self.shader_path = Self::map_filter_name(value);
                }
            }
            "debugger" => {
                if let Ok(b) = Self::parse_bool(value) {
                    self.debugger_enabled = b;
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

    /// Check if a flag is present in the arguments.
    fn has_flag(args: &[String], flag: &str) -> bool {
        args.iter().any(|a| a == flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(args: Vec<String>) -> Config {
        match Config::new(&args).unwrap() {
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
    }

    #[test]
    fn test_config_new_defaults() {
        let args = vec!["neser".to_string()];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Ntsc);
        assert!(config.audio_enabled);
        assert!(config.vsync_enabled);
        assert!(config.gamepads_enabled);
        assert!(!config.fullscreen);
        assert_eq!(config.window_height, 960);
    }

    #[test]
    fn test_config_help_flag() {
        let args = vec!["neser".to_string(), "--help".to_string()];
        match Config::new(&args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_config_help_flag_short() {
        let args = vec!["neser".to_string(), "-h".to_string()];
        match Config::new(&args).unwrap() {
            ParseResult::Help => {}
            ParseResult::Config(_) => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_config_pal_mode() {
        let args = vec!["neser".to_string(), "--pal".to_string()];
        let config = parse_config(args);
        assert_eq!(config.tv_system, TvSystem::Pal);
    }

    #[test]
    fn test_config_no_audio() {
        let args = vec!["neser".to_string(), "--no-audio".to_string()];
        let config = parse_config(args);
        assert!(!config.audio_enabled);
    }

    #[test]
    fn test_config_no_vsync() {
        let args = vec!["neser".to_string(), "--no-vsync".to_string()];
        let config = parse_config(args);
        assert!(!config.vsync_enabled);
    }

    #[test]
    fn test_config_no_gamepads() {
        let args = vec!["neser".to_string(), "--no-gamepads".to_string()];
        let config = parse_config(args);
        assert!(!config.gamepads_enabled);
    }

    #[test]
    fn test_config_fullscreen() {
        let args = vec!["neser".to_string(), "--fullscreen".to_string()];
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
        let result = Config::new(&args);
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
        let result = Config::new(&args);
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
        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_cmdline_filter_invalid_ignored() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "invalid-filter".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.shader_path, None);
    }

    #[test]
    fn test_config_debugger_enabled() {
        let args = vec!["neser".to_string(), "--start-in-debugger".to_string()];
        let config = parse_config(args);
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_disable_pulse1() {
        let args = vec!["neser".to_string(), "--disable-pulse1".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_disable_pulse2() {
        let args = vec!["neser".to_string(), "--disable-pulse2".to_string()];
        let config = parse_config(args);
        assert!(config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_disable_triangle() {
        let args = vec!["neser".to_string(), "--disable-triangle".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_disable_noise() {
        let args = vec!["neser".to_string(), "--disable-noise".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_disable_dmc() {
        let args = vec!["neser".to_string(), "--disable-dmc".to_string()];
        let config = parse_config(args);
        assert!(!config.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_unknown_argument_errors() {
        let args = vec![
            "neser".to_string(),
            "--definitely-not-a-real-flag".to_string(),
        ];
        let result = Config::new(&args);
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
            "--pal".to_string(),
            "--no-audio".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "2".to_string(),
            "--disable-pulse1".to_string(),
            "--disable-noise".to_string(),
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
        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_video_scale_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--video-scale".to_string(),
            "2.5".to_string(),
        ];
        let result = Config::new(&args);
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
        config.apply_config_value("tv_system", "pal");
        assert_eq!(config.tv_system, TvSystem::Pal);
    }

    #[test]
    fn test_config_file_tv_system_ntsc() {
        let mut config = Config {
            tv_system: TvSystem::Pal,
            ..Default::default()
        };
        config.apply_config_value("tv_system", "ntsc");
        assert_eq!(config.tv_system, TvSystem::Ntsc);
    }

    #[test]
    fn test_config_file_tv_system_case_insensitive() {
        let mut config = Config::default();
        config.apply_config_value("tv_system", "PAL");
        assert_eq!(config.tv_system, TvSystem::Pal);

        config.apply_config_value("tv_system", "NTSC");
        assert_eq!(config.tv_system, TvSystem::Ntsc);
    }

    #[test]
    fn test_config_file_audio() {
        let mut config = Config::default();
        config.apply_config_value("audio", "false");
        assert!(!config.audio_enabled);

        config.apply_config_value("audio", "true");
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_file_vsync() {
        let mut config = Config::default();
        config.apply_config_value("vsync", "false");
        assert!(!config.vsync_enabled);

        config.apply_config_value("vsync", "true");
        assert!(config.vsync_enabled);
    }

    #[test]
    fn test_config_file_gamepads() {
        let mut config = Config::default();
        config.apply_config_value("gamepads", "false");
        assert!(!config.gamepads_enabled);

        config.apply_config_value("gamepads", "true");
        assert!(config.gamepads_enabled);
    }

    #[test]
    fn test_config_file_fullscreen() {
        let mut config = Config::default();
        config.apply_config_value("fullscreen", "true");
        assert!(config.fullscreen);

        config.apply_config_value("fullscreen", "false");
        assert!(!config.fullscreen);
    }

    #[test]
    fn test_config_file_display() {
        let mut config = Config::default();
        config.apply_config_value("display", "1");
        assert_eq!(config.fullscreen_display, Some(1));

        config.apply_config_value("display", "0");
        assert_eq!(config.fullscreen_display, Some(0));
    }

    #[test]
    fn test_config_file_display_negative_ignored() {
        let mut config = Config::default();
        config.apply_config_value("display", "-1");
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_file_filter_invalid_ignored() {
        let mut config = Config::default();
        config.apply_config_value("filter", "invalid-filter");
        assert_eq!(config.shader_path, None);
    }

    #[test]
    fn test_config_file_filter_empty_ignored() {
        let mut config = Config::default();
        config.apply_config_value("filter", "");
        assert_eq!(config.shader_path, None);
    }

    #[test]
    fn test_config_file_filter_crt() {
        let mut config = Config::default();
        config.apply_config_value("filter", "crt");
        assert_eq!(config.shader_path, Some("shaders/crt-lottes.slangp".to_string()));
    }

    #[test]
    fn test_config_file_filter_ntsc() {
        let mut config = Config::default();
        config.apply_config_value("filter", "ntsc");
        assert_eq!(config.shader_path, Some("shaders/ntsc-256px-composite.slangp".to_string()));
    }

    #[test]
    fn test_config_file_filter_smooth() {
        let mut config = Config::default();
        config.apply_config_value("filter", "smooth");
        assert_eq!(config.shader_path, Some("shaders/xbrz-freescale.slangp".to_string()));
    }

    #[test]
    fn test_config_file_filter_none() {
        let mut config = Config::default();
        config.apply_config_value("filter", "none");
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
        assert_eq!(config.shader_path, Some("shaders/crt-lottes.slangp".to_string()));
    }

    #[test]
    fn test_config_cmdline_filter_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.shader_path, Some("shaders/ntsc-256px-composite.slangp".to_string()));
    }

    #[test]
    fn test_config_cmdline_filter_smooth() {
        let args = vec![
            "neser".to_string(),
            "--filter".to_string(),
            "smooth".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.shader_path, Some("shaders/xbrz-freescale.slangp".to_string()));
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
        config.apply_config_value("debugger", "true");
        assert!(config.debugger_enabled);
    }

    #[test]
    fn test_config_file_apu_channels() {
        let mut config = Config::default();
        config.apply_config_value("pulse1", "false");
        config.apply_config_value("pulse2", "false");
        config.apply_config_value("triangle", "false");
        config.apply_config_value("noise", "false");
        config.apply_config_value("dmc", "false");

        assert!(!config.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(!config.apu_channels.contains(ApuChannels::NOISE));
        assert!(!config.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_file_window_height() {
        let mut config = Config::default();
        config.apply_config_value("window_height", "720");
        assert_eq!(config.window_height, 720);
    }

    #[test]
    fn test_config_file_trace_cpu() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "2");
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 2);
    }

    #[test]
    fn test_config_file_trace_ppu() {
        let mut config = Config::default();
        config.apply_config_value("trace-ppu", "3");
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.ppu, 3);
    }

    #[test]
    fn test_config_file_trace_apu() {
        let mut config = Config::default();
        config.apply_config_value("trace-apu", "1");
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.apu, 1);
    }

    #[test]
    fn test_config_file_trace_mapper() {
        let mut config = Config::default();
        config.apply_config_value("trace-mapper", "4");
        assert!(config.tracing.enabled);
        assert_eq!(config.tracing.mapper, 4);
    }

    #[test]
    fn test_config_file_trace_nestest() {
        let mut config = Config::default();
        config.apply_config_value("trace-nestest", "true");
        assert!(config.tracing.enabled);
        assert!(config.tracing.nestest);
    }

    #[test]
    fn test_config_file_trace_zero_does_not_enable() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "0");
        assert!(!config.tracing.enabled);
        assert_eq!(config.tracing.cpu, 0);
    }

    // #[test]
    // fn test_config_file_timing_scale() {
    //     let mut config = Config::default();
    //     config.apply_config_value("timing_scale", "1.5");
    //     assert!((config.timing_scale - 1.5).abs() < 0.001);
    // }

    #[test]
    fn test_config_file_bool_formats() {
        let mut config = Config::default();

        // Test "yes"/"no"
        config.apply_config_value("audio", "no");
        assert!(!config.audio_enabled);
        config.apply_config_value("audio", "yes");
        assert!(config.audio_enabled);

        // Test "1"/"0"
        config.apply_config_value("audio", "0");
        assert!(!config.audio_enabled);
        config.apply_config_value("audio", "1");
        assert!(config.audio_enabled);
    }

    #[test]
    fn test_config_file_unknown_key_ignored() {
        let mut config = Config::default();
        // Should not panic
        config.apply_config_value("unknown_key", "some_value");
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
        config.load_from_file(file.path());

        assert_eq!(config.tv_system, TvSystem::Pal);
        assert!(!config.audio_enabled);
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, Some(2));
        assert_eq!(config.shader_path, Some("shaders/crt-lottes.slangp".to_string()));
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
        config.load_from_file(file.path());
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
    fn test_config_file_nonexistent_silently_ignored() {
        let mut config = Config::default();
        config.load_from_file(Path::new("/nonexistent/path/neser.conf"));
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
}
