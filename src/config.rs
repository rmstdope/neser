//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! It can be created from command-line arguments using `Config::from_args`.

/// Emulator configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Whether to run in fullscreen mode.
    pub fullscreen: bool,
    /// Which display to use for fullscreen (None = auto-select).
    pub fullscreen_display: Option<i32>,
}

impl Config {
    /// Create a new Config with default values.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse configuration from command-line arguments.
    ///
    /// # Arguments
    /// * `args` - Command-line arguments (including program name at index 0).
    ///
    /// # Errors
    /// Returns an error if:
    /// - `--display` is specified without a value
    /// - `--display` value is not a valid non-negative integer
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let fullscreen = args.iter().any(|a| a == "--fullscreen");

        let fullscreen_display = if fullscreen {
            Self::parse_display_arg(args)?
        } else {
            None
        };

        Ok(Self {
            fullscreen,
            fullscreen_display,
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_is_not_fullscreen() {
        let config = Config::new();
        assert!(!config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_from_args_no_fullscreen() {
        let args = vec!["neser".to_string()];
        let config = Config::from_args(&args).unwrap();
        assert!(!config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_from_args_fullscreen_enabled() {
        let args = vec!["neser".to_string(), "--fullscreen".to_string()];
        let config = Config::from_args(&args).unwrap();
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_from_args_fullscreen_with_display() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = Config::from_args(&args).unwrap();
        assert!(config.fullscreen);
        assert_eq!(config.fullscreen_display, Some(1));
    }

    #[test]
    fn test_config_from_args_display_without_fullscreen_is_ignored() {
        // --display without --fullscreen should be ignored (display only matters for fullscreen)
        let args = vec![
            "neser".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = Config::from_args(&args).unwrap();
        assert!(!config.fullscreen);
        assert_eq!(config.fullscreen_display, None);
    }

    #[test]
    fn test_config_from_args_display_missing_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
        ];
        let result = Config::from_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing value"));
    }

    #[test]
    fn test_config_from_args_display_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "abc".to_string(),
        ];
        let result = Config::from_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid"));
    }

    #[test]
    fn test_config_from_args_display_negative_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "-1".to_string(),
        ];
        let result = Config::from_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(">= 0"));
    }
}
