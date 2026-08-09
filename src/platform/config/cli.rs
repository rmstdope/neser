//! Shared CLI parsing machinery for the platform configuration layer.
//!
//! Domain-agnostic pieces used to parse and validate command-line flags and to
//! render help text: [`CliFlag`] descriptors and the central
//! [`PLATFORM_CLI_FLAGS`] table, generic value parsers (`parse_bool`,
//! `parse_u32_arg`, ...), flag aggregation ([`all_cli_flags`]), validation
//! ([`validate_args`]), and help generation ([`help_text`]/[`print_help`]).
//!
//! Domain-specific parsing glue lives in the per-domain submodules
//! (`audio`, `video`, `autorun`, `debugger`, `cartridge`).

use super::Config;

/// CLI flag definition for help text and validation.
///
/// Used by both NES and GB config modules to declare their supported flags.
pub(crate) struct CliFlag {
    pub flag: &'static str,
    pub help: Option<&'static str>,
    pub has_value: bool,
}

/// Result of parsing command-line arguments.
#[derive(Debug)]
pub enum ParseResult {
    /// User requested help - print and exit.
    Help,
    /// User requested version information - print and exit.
    Version,
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
        flag: "--version",
        help: Some("Print version information and exit"),
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
        flag: "--audio-buffer-ms",
        help: Some("Target native audio buffering in milliseconds (20-500, default: 60)"),
        has_value: true,
    },
    CliFlag {
        flag: "--audio-sample-rate",
        help: Some(
            "Target native audio sample rate in Hz (22050, 44100, 48000, 96000, 192000; default: 44100)",
        ),
        has_value: true,
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
        help: Some(
            "RAM initialization mode for NES and SNES: zero, random, or seeded-random:SEED (default: random; zero on wasm)",
        ),
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
    CliFlag {
        flag: "--metadata-db-path",
        help: Some("Path to TheGamesDB metadata SQLite database (default: ~/.neser/metadata.db)"),
        has_value: true,
    },
    CliFlag {
        flag: "--image-cache-path",
        help: Some("Path to cover art image cache directory (default: ~/.neser/image_cache/)"),
        has_value: true,
    },
    CliFlag {
        flag: "--include-unofficial-roms",
        help: Some("Include unofficial ROMs (hacks, homebrew, etc.) in the browser catalog"),
        has_value: false,
    },
    CliFlag {
        flag: "--headless",
        help: Some("Run the ROM without a window and write one frame to a PNG (requires --output)"),
        has_value: false,
    },
    CliFlag {
        flag: "--frames",
        help: Some("Frames to run before capturing with --headless (default: 60)"),
        has_value: true,
    },
    CliFlag {
        flag: "--output",
        help: Some("Destination PNG path for --headless capture"),
        has_value: true,
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
    "--include-unofficial-roms",
    "--gba-color-correction",
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

/// Parse a comma-separated list of search paths.
pub(crate) fn parse_search_paths(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Get an iterator over all CLI flags (platform, NES, GB, and GBA).
pub(crate) fn all_cli_flags() -> impl Iterator<Item = &'static CliFlag> {
    PLATFORM_CLI_FLAGS
        .iter()
        .chain(crate::nes::console::CLI_FLAGS.iter())
        .chain(crate::gb::console::config::GB_CLI_FLAGS.iter())
        .chain(crate::gba::console::config::GBA_CLI_FLAGS.iter())
        .chain(crate::snes::console::config::SNES_CLI_FLAGS.iter())
}

/// Categorize a flag into its help section.
fn help_section_for_flag(flag: &str) -> &'static str {
    if flag.starts_with("--trace")
        || matches!(flag, "--debugger" | "--debugger-alpha" | "--breakpoint")
    {
        "Trace and Debugging"
    } else if matches!(
        flag,
        "--nes-controller-port1"
            | "--nes-controller-port2"
            | "--nes-expansion-port"
            | "--nes-zapper-detection-size"
            | "--snes-controller-port1"
            | "--snes-controller-port2"
            | "--gamepads"
            | "--nes-enable-4-score"
            | "--no-nes-4-score"
            | "--disable-nes-4-score"
    ) {
        "Input"
    } else if matches!(
        flag,
        "--audio"
            | "--no-audio"
            | "--disable-audio"
            | "--audio-buffer-ms"
            | "--audio-sample-rate"
            | "--nes-pulse1"
            | "--no-nes-pulse1"
            | "--disable-nes-pulse1"
            | "--nes-pulse2"
            | "--no-nes-pulse2"
            | "--disable-nes-pulse2"
            | "--nes-triangle"
            | "--no-nes-triangle"
            | "--disable-nes-triangle"
            | "--nes-noise"
            | "--no-nes-noise"
            | "--disable-nes-noise"
            | "--nes-dmc"
            | "--no-nes-dmc"
            | "--disable-nes-dmc"
    ) {
        "Sound"
    } else if matches!(
        flag,
        "--fullscreen"
            | "--display"
            | "--nes-filter"
            | "--gb-filter"
            | "--gba-filter"
            | "--window-height"
            | "--vsync"
            | "--no-vsync"
            | "--disable-vsync"
            | "--nes-horizontal-overscan"
            | "--nes-vertical-overscan"
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

/// Generate the help text for all CLI flags.
pub(crate) fn help_text() -> String {
    use std::fmt::Write as _;

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
        for flag in all_cli_flags() {
            if flag.help.is_none() || help_section_for_flag(flag.flag) != section {
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
        "  neser --nes-hardware nes-pal game.nes        # Use NES PAL hardware"
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
        "  neser --disable-nes-pulse1 --disable-nes-pulse2 game.nes # Disable specific channels"
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

/// Print help text to stdout.
pub(crate) fn print_help() {
    print!("{}", help_text());
}

/// Validate command-line arguments.
pub(crate) fn validate_args(args: &[String]) -> Result<(), String> {
    let mut i = 1; // Skip program name
    let mut seen_positional = false;
    while i < args.len() {
        let arg = &args[i];

        // Check for exact flag match
        if let Some(flag) = all_cli_flags().find(|f| f.flag == arg) {
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
                    if parse_bool(next_arg).is_ok() {
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
            && all_cli_flags().any(|f| f.flag == flag_part)
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

#[cfg(test)]
mod tests {
    use super::{ParseResult, help_text};
    use crate::nes::console::Config;
    use crate::platform::config::test_support::{config_new, parse_config};

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
        let help = help_text();

        assert!(help.contains("--version"));
        assert!(help.contains("Print version information and exit"));
    }

    #[test]
    fn test_help_text_groups_flags_into_readable_sections() {
        let help = help_text();

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
        let help = help_text();

        assert!(help.contains("--load-state"));
        assert!(!help.contains("--no-load-state"));
        assert!(!help.contains("--disable-load-state"));
    }

    #[test]
    fn test_help_text_lists_audio_buffer_ms_flag() {
        let help = help_text();

        assert!(help.contains("--audio-buffer-ms"));
        assert!(help.contains("Target native audio buffering in milliseconds"));
    }

    #[test]
    fn test_help_text_lists_audio_sample_rate_flag() {
        let help = help_text();

        assert!(help.contains("--audio-sample-rate"));
        assert!(help.contains("Target native audio sample rate in Hz"));
        assert!(help.contains("22050, 44100, 48000, 96000, 192000"));
        assert!(help.contains("default: 44100"));
    }

    #[test]
    fn test_help_text_oam_dram_decay_shows_default_and_no_negation_aliases() {
        let help = help_text();

        assert!(help.contains("--nes-oam-dram-decay"));
        assert!(help.contains("default: false"));
        assert!(!help.contains("--no-oam-dram-decay"));
        assert!(!help.contains("--disable-oam-dram-decay"));
    }

    #[test]
    fn test_help_text_examples_use_hardware_flag() {
        let help = help_text();

        assert!(help.contains("neser --nes-hardware nes-pal game.nes"));
        assert!(!help.contains("--tv-system"));
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
}
