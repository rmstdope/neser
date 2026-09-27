//! Parsing of the full [`Config`]: defaults, then the config files, then the
//! command line.
//!
//! The frontend and each core parse their own section: every command line and
//! every config-file key is offered to `FrontendConfig`, `NesConfig`,
//! `GbConfig`, `GbaConfig` and `SnesConfig`, and each takes only its own keys.
//! What stays here is what belongs to no single section: the file locations,
//! the positional ROM path, `--display`, and the `*-filter` keys that pick the
//! frontend shader from a core's list of names.

use super::{Config, ParseResult, all_cli_flags, is_optional_bool_flag, parse_bool};
use crate::gb::console::config::GB_FILTER_NAMES;
use crate::gba::console::config::GBA_FILTER_NAMES;
use crate::nes::console::NES_FILTER_NAMES;
use crate::platform::shaders::SHADER_PRESETS;
use std::fs;
use std::path::Path;

impl Config {
    /// Default config file name.
    pub(crate) const CONFIG_FILE_NAME: &'static str = "neser.conf";

    /// Load configuration from a config file.
    ///
    /// The config file uses a simple key=value format, one setting per line.
    /// Lines starting with '#' are treated as comments.
    /// Unknown keys are ignored.
    ///
    /// # Example config file:
    /// ```text
    /// # Hardware mode: nes-ntsc, nes-pal, famicom, or dendy
    /// nes-hardware=nes-ntsc
    ///
    /// # Expansion port: none or famicom-four-players
    /// nes-expansion-port=none
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
    /// window_height=896
    ///
    /// # Shader/filter
    /// # NES valid values: crt, ntsc, smooth, pal, none
    /// nes-filter=crt
    /// # GB valid values: dmg, none
    /// gb-filter=dmg
    ///
    /// # APU channel toggles
    /// nes-pulse1=true
    /// nes-pulse2=true
    /// nes-triangle=true
    /// nes-noise=true
    /// nes-dmc=true
    /// ```
    pub(crate) fn load_from_file(&mut self, path: &Path) -> Result<(), String> {
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

    /// Map a filter name to a shader path, validating against an allowed list.
    ///
    /// `allowed` must be a subset of names defined in [`crate::platform::shaders::SHADER_PRESETS`].
    pub(crate) fn map_filter_name_for(name: &str, allowed: &[&str]) -> Result<String, String> {
        if !allowed.contains(&name) {
            return Err(format!(
                "Invalid filter name: '{}'. Valid options are: {}",
                name,
                allowed.join(", ")
            ));
        }
        SHADER_PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, path)| (*path).to_string())
            .ok_or_else(|| format!("Filter '{}' has no shader path defined", name))
    }

    /// Apply a single config file key-value pair.
    ///
    /// Keys are normalized: dashes are treated as underscores, so both
    /// `nes-hardware` and `nes_hardware` are accepted. Every key is offered to
    /// the frontend and to each core, and each takes only its own; unknown
    /// keys are ignored.
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        // Delegate to sub-configs first
        self.frontend.apply_config_value(&key, value)?;
        self.nes.apply_config_value(&key, value)?;
        self.gb.apply_config_value(&key, value)?;
        self.gba.apply_config_value(&key, value)?;
        self.snes.apply_config_value(&key, value)?;

        // The filter keys pick the frontend shader, so they are the platform's;
        // each core owns only the list of names it accepts.
        let allowed = match key.as_str() {
            "nes_filter" => NES_FILTER_NAMES,
            "gb_filter" => GB_FILTER_NAMES,
            "gba_filter" => GBA_FILTER_NAMES,
            _ => return Ok(()),
        };
        if !value.is_empty() {
            self.frontend.shader_path = Some(Self::map_filter_name_for(value, allowed)?);
        }
        Ok(())
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
    /// - `Ok(ParseResult::Version)` if --version was specified
    /// - `Ok(ParseResult::Config(config))` on successful parse
    /// - `Err(message)` on validation error
    #[allow(clippy::new_ret_no_self)]
    pub fn new(args: &[String]) -> Result<ParseResult, String> {
        // Check for help first
        if args.iter().any(|a| a == "--help" || a == "-h") {
            return Ok(ParseResult::Help);
        }
        if args.iter().any(|a| a == "--version") {
            return Ok(ParseResult::Version);
        }

        // Validate arguments
        super::validate_args(args)?;

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

        config.nes.validate()?;

        Ok(ParseResult::Config(Box::new(config)))
    }

    /// Print help text to stdout.
    pub fn print_help() {
        super::print_help();
    }

    /// Apply command-line arguments to the config.
    /// Arguments override any values set by defaults or config file.
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        // Delegate to sub-config apply_args() methods
        self.frontend.apply_args(args)?;
        self.nes.apply_args(args)?;

        // Display argument (only applies if fullscreen is set)
        if self.frontend.fullscreen
            && let Some(display) = Self::parse_display_arg(args)?
        {
            self.frontend.fullscreen_display = Some(display);
        }

        // Shader paths: each core's filter flag, checked against that core's names
        for (flag, allowed) in [
            ("--nes-filter", NES_FILTER_NAMES),
            ("--gb-filter", GB_FILTER_NAMES),
            ("--gba-filter", GBA_FILTER_NAMES),
        ] {
            if let Some(filter_name) = Self::parse_named_arg(args, flag) {
                self.frontend.shader_path = Some(Self::map_filter_name_for(&filter_name, allowed)?);
            }
        }

        // ROM path from positional argument
        if let Some(path) = Self::parse_rom_arg(args)? {
            self.frontend.rom_path = Some(path);
        }

        // GB hardware (parsed by GB config module)
        self.gb.apply_args(args)?;

        // GBA hardware (parsed by GBA config module)
        self.gba.apply_args(args)?;

        // SNES hardware (parsed by SNES config module)
        self.snes.apply_args(args)?;

        // Must follow the positional ROM argument above, which is what the
        // headless capture mode requires.
        super::headless::validate_rom_path(&self.frontend)?;

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

    /// Parse a named flag argument (e.g., `--nes-filter`) from command-line args.
    fn parse_named_arg(args: &[String], flag: &str) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == flag && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    }

    /// Parse the --config argument from command-line args.
    pub(crate) fn parse_config_arg(args: &[String]) -> Option<String> {
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

            if let Some(flag) = all_cli_flags().find(|f| f.flag == arg) {
                if flag.has_value {
                    i += 2;
                }
                // For optional boolean flags, check if next arg is a boolean value
                else if is_optional_bool_flag(arg) {
                    i += 1;
                    // Peek at next argument to see if it's a boolean value
                    if i < args.len() && parse_bool(&args[i]).is_ok() {
                        i += 1; // Skip the boolean value
                    }
                } else {
                    i += 1;
                }
                continue;
            }

            if let Some((flag_part, _)) = arg.split_once('=')
                && all_cli_flags().any(|f| f.flag == flag_part)
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
}

#[cfg(test)]
mod tests {
    use crate::platform::config::Config;

    #[test]
    fn gb_filter_names_are_none_and_dmg() {
        assert_eq!(
            crate::gb::console::config::GB_FILTER_NAMES,
            &["none", "dmg"]
        );
    }

    #[test]
    fn nes_filter_names_are_the_nes_shaders() {
        assert_eq!(
            crate::nes::console::NES_FILTER_NAMES,
            &["none", "crt", "smooth", "ntsc", "pal"]
        );
    }

    #[test]
    fn config_file_key_reaches_each_core() {
        let mut config = Config::default();
        config.apply_config_value("gb-hardware", "cgb").unwrap();
        config.apply_config_value("gba-hardware", "sp").unwrap();
        config
            .apply_config_value("snes-hardware", "snes-pal")
            .unwrap();
        config
            .apply_config_value("nes-hardware", "famicom")
            .unwrap();
        config.apply_config_value("audio", "false").unwrap();
        assert_eq!(config.gb.hardware, Some(crate::gb::model::GbHardware::Cgb));
        assert_eq!(
            config.gba.hardware,
            crate::gba::console::config::GbaModel::Sp
        );
        assert!(config.snes.hardware.is_some());
        assert_eq!(
            config.nes.hardware_mode,
            crate::nes::console::HardwareMode::Famicom
        );
        assert!(!config.frontend.audio_enabled);
    }

    /// The `*-filter` keys and flags, which pick the frontend shader.
    mod config_parsing {
        use crate::platform::config::Config;
        use crate::platform::config::test_support::{config_new, parse_config};

        #[test]
        fn test_config_cmdline_filter_crt() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "crt".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_filter_ntsc() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "ntsc".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_filter_smooth() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "smooth".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some(
                    "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                        .to_string()
                )
            );
        }

        #[test]
        fn test_config_cmdline_filter_none() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "none".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("shaders/stock.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_nes_filter_crt_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "crt".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_nes_filter_rejects_dmg_shader() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "dmg".to_string(),
            ];
            let result = config_new(args);
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(
                msg.contains("dmg"),
                "Error should mention the invalid value: {msg}"
            );
        }

        #[test]
        fn test_config_cmdline_nes_filter_accepts_smooth() {
            let args = vec![
                "neser".to_string(),
                "--nes-filter".to_string(),
                "smooth".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some(
                    "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                        .to_string()
                )
            );
        }

        #[test]
        fn test_config_cmdline_gb_filter_dmg_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--gb-filter".to_string(),
                "dmg".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/gameboy.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_gb_filter_rejects_crt_shader() {
            let args = vec![
                "neser".to_string(),
                "--gb-filter".to_string(),
                "crt".to_string(),
            ];
            let result = config_new(args);
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(
                msg.contains("crt"),
                "Error should mention the invalid value: {msg}"
            );
        }

        #[test]
        fn test_config_cmdline_gb_filter_none_sets_stock_shader() {
            let args = vec![
                "neser".to_string(),
                "--gb-filter".to_string(),
                "none".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("shaders/stock.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_gba_filter_agb001_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--gba-filter".to_string(),
                "agb001".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/agb001.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_gba_filter_nso_gba_color_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--gba-filter".to_string(),
                "nso-gba-color".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_gba_filter_sp101_color_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--gba-filter".to_string(),
                "sp101-color".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/color-mod/sp101-color.slangp".to_string())
            );
        }

        #[test]
        fn test_config_cmdline_gba_filter_gba_lcd_grid_sets_shader_path() {
            let args = vec![
                "neser".to_string(),
                "--gba-filter".to_string(),
                "gba-lcd-grid".to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(
                config.frontend.shader_path,
                Some(
                    "vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp"
                        .to_string()
                )
            );
        }

        #[test]
        fn test_config_cmdline_gba_filter_rejects_bogus_shader_with_valid_options() {
            let args = vec![
                "neser".to_string(),
                "--gba-filter".to_string(),
                "bogus".to_string(),
            ];
            let result = config_new(args);
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(msg.contains("bogus"));
            assert!(
                msg.contains("none, gba-lcd, agb001, nso-gba-color, sp101-color, gba-lcd-grid")
            );
        }

        #[test]
        fn test_config_file_filter_invalid_errors() {
            let mut config = Config::default();
            let result = config.apply_config_value("nes-filter", "invalid-filter");
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err(),
                "Invalid filter name: 'invalid-filter'. Valid options are: none, crt, smooth, ntsc, pal"
            );
        }

        #[test]
        fn test_config_file_filter_empty_ignored() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "").unwrap();
            assert_eq!(config.frontend.shader_path, None);
        }

        #[test]
        fn test_config_file_filter_crt() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "crt").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_filter_ntsc() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "ntsc").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_filter_smooth() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "smooth").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some(
                    "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                        .to_string()
                )
            );
        }

        #[test]
        fn test_config_file_filter_none() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "none").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("shaders/stock.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_invalid_filter_errors() {
            use std::io::Write;
            use tempfile::NamedTempFile;

            let content = r#"
        hardware=nes-pal
    nes-filter=invalid-shader
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
                "Invalid filter name: 'invalid-shader'. Valid options are: none, crt, smooth, ntsc, pal"
            );
        }

        #[test]
        fn test_config_file_nes_filter_ntsc_sets_shader_path() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "ntsc").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_nes_filter_rejects_dmg_shader() {
            let mut config = Config::default();
            let result = config.apply_config_value("nes-filter", "dmg");
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(
                msg.contains("dmg"),
                "Error should mention the invalid value: {msg}"
            );
        }

        #[test]
        fn test_config_file_nes_filter_empty_ignored() {
            let mut config = Config::default();
            config.apply_config_value("nes-filter", "").unwrap();
            assert_eq!(config.frontend.shader_path, None);
        }

        #[test]
        fn test_config_file_gb_filter_dmg_sets_shader_path() {
            let mut config = Config::default();
            config.apply_config_value("gb-filter", "dmg").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/gameboy.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_gb_filter_rejects_crt_shader() {
            let mut config = Config::default();
            let result = config.apply_config_value("gb-filter", "crt");
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(
                msg.contains("crt"),
                "Error should mention the invalid value: {msg}"
            );
        }

        #[test]
        fn test_config_file_gb_filter_empty_ignored() {
            let mut config = Config::default();
            config.apply_config_value("gb-filter", "").unwrap();
            assert_eq!(config.frontend.shader_path, None);
        }

        #[test]
        fn test_config_file_gba_filter_agb001_sets_shader_path() {
            let mut config = Config::default();
            config.apply_config_value("gba-filter", "agb001").unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/agb001.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_gba_filter_nso_gba_color_sets_shader_path() {
            let mut config = Config::default();
            config
                .apply_config_value("gba-filter", "nso-gba-color")
                .unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_gba_filter_sp101_color_sets_shader_path() {
            let mut config = Config::default();
            config
                .apply_config_value("gba-filter", "sp101-color")
                .unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some("vendor/slang-shaders/handheld/color-mod/sp101-color.slangp".to_string())
            );
        }

        #[test]
        fn test_config_file_gba_filter_gba_lcd_grid_sets_shader_path() {
            let mut config = Config::default();
            config
                .apply_config_value("gba-filter", "gba-lcd-grid")
                .unwrap();
            assert_eq!(
                config.frontend.shader_path,
                Some(
                    "vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp"
                        .to_string()
                )
            );
        }

        #[test]
        fn test_config_file_gba_filter_rejects_bogus_shader_with_valid_options() {
            let mut config = Config::default();
            let result = config.apply_config_value("gba-filter", "bogus");
            assert!(result.is_err());
            let msg = result.unwrap_err();
            assert!(msg.contains("bogus"));
            assert!(
                msg.contains("none, gba-lcd, agb001, nso-gba-color, sp101-color, gba-lcd-grid")
            );
        }
    }
}
