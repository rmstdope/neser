//! Configuration for the Game Boy Advance emulator.
//!
//! [`GbaConfig`] holds GBA-specific configuration options such as the
//! emulated hardware model selection.

use crate::platform::config::CliFlag;

/// Ordered list of GBA shader filter short names accepted by CLI/config and
/// used for runtime shader cycling.
pub(crate) const GBA_FILTER_NAMES: &[&str] = &[
    "none",
    "gba-lcd",
    "agb001",
    "nso-gba-color",
    "sp101-color",
    "gba-lcd-grid",
];

const GBA_FILTER_HELP: &str =
    "GBA shader filter: none, gba-lcd, agb001, nso-gba-color, sp101-color, gba-lcd-grid";

/// GBA-specific CLI flags, defined here so that the GBA module owns its flag
/// declarations and parsing logic. These are chained into the global flag list
/// by the platform config parser for validation and help-text generation.
pub(crate) const GBA_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--gba-filter",
        help: Some(GBA_FILTER_HELP),
        has_value: true,
    },
    CliFlag {
        flag: "--gba-hardware",
        help: Some("GBA hardware model: agb, sp, micro (default: agb)"),
        has_value: true,
    },
    CliFlag {
        flag: "--gba-bios-path",
        help: Some("Path to external GBA BIOS image (exactly 16384 bytes)"),
        has_value: true,
    },
    CliFlag {
        flag: "--skip-bios-intro",
        help: Some("Skip GBA BIOS intro (logo + jingle) but keep full hardware init"),
        has_value: false,
    },
    CliFlag {
        flag: "--gba-color-correction",
        help: Some(
            "Enable GBA LCD color correction (simulates TFT gamma; values 0-14 nearly black)",
        ),
        has_value: false,
    },
];

/// Valid values for the `gba-hardware` option (used in error messages).
const VALID_HARDWARE_MODELS: &str = "agb, sp, micro";

/// GBA hardware model variants.
///
/// Represents the different GBA hardware revisions. While the differences
/// between models are mostly cosmetic (form factor, screen type), some
/// software may behave differently based on hardware detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GbaModel {
    /// Original Game Boy Advance (AGB-001).
    #[default]
    Agb,
    /// Game Boy Advance SP (AGS-001/AGS-101).
    Sp,
    /// Game Boy Micro (OXY-001).
    Micro,
}

impl GbaModel {
    /// Parse a hardware model string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("agb") {
            Some(Self::Agb)
        } else if s.eq_ignore_ascii_case("sp") {
            Some(Self::Sp)
        } else if s.eq_ignore_ascii_case("micro") {
            Some(Self::Micro)
        } else {
            None
        }
    }

    /// Return the short name used in config files and CLI.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Agb => "agb",
            Self::Sp => "sp",
            Self::Micro => "micro",
        }
    }
}

/// Game Boy Advance-specific hardware configuration.
#[derive(Debug, Clone, Default)]
pub struct GbaConfig {
    /// Emulated GBA hardware model.
    pub hardware: GbaModel,
    /// Optional path to an external GBA BIOS image.
    pub bios_path: Option<String>,
    /// When true, skip the BIOS intro (logo + jingle) but still perform
    /// full hardware state setup (stacks, POSTFLG, SoundBias, etc.).
    pub skip_bios_intro: bool,
    /// When true, enable GBA LCD color correction.
    ///
    /// Applies a gamma ≈ 4 curve to simulate the GBA TFT LCD's physical
    /// non-linear response. Per GBATek, values 0–14 appear nearly black
    /// on the GBA TFT; this correction concentrates most visible output
    /// in the upper half of the intensity range.
    ///
    /// Default: false (linear expansion, technically correct for digital
    /// color values).
    pub color_correction: bool,
}

impl GbaConfig {
    fn set_bios_path_from_input(&mut self, value: &str) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            self.bios_path = None;
        } else {
            self.bios_path = Some(trimmed.to_string());
        }
    }

    /// Parse GBA-specific CLI arguments and apply them to this config.
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(hardware) =
            crate::platform::config::parse_cli_string_arg(args, "--gba-hardware")
        {
            self.hardware = GbaModel::parse(&hardware).ok_or_else(|| {
                format!(
                    "Invalid --gba-hardware value: '{hardware}'. Valid options are: {VALID_HARDWARE_MODELS}",
                )
            })?;
        }

        if let Some(path) = crate::platform::config::parse_cli_string_arg(args, "--gba-bios-path") {
            self.set_bios_path_from_input(&path);
        }

        if let Some(skip) = crate::platform::config::parse_bool_arg(args, "--skip-bios-intro")? {
            self.skip_bios_intro = skip;
        }

        if let Some(cc) = crate::platform::config::parse_bool_arg(args, "--gba-color-correction")? {
            self.color_correction = cc;
        }

        Ok(())
    }

    /// Apply a config file key-value pair to this config.
    ///
    /// Accepts `gba-hardware`, `gba-bios-path`, `skip-bios-intro`, and
    /// `gba-color-correction` keys.
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        match key.as_str() {
            "gba_hardware" => {
                self.hardware = GbaModel::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid gba_hardware value: '{value}'. Valid options are: {VALID_HARDWARE_MODELS}",
                    )
                })?;
            }
            "gba_bios_path" => {
                self.set_bios_path_from_input(value);
            }
            "skip_bios_intro" => {
                self.skip_bios_intro = crate::platform::config::parse_bool(value)
                    .map_err(|_| format!("Invalid skip_bios_intro value: '{value}'"))?;
            }
            "gba_color_correction" => {
                self.color_correction = crate::platform::config::parse_bool(value)
                    .map_err(|_| format!("Invalid gba_color_correction value: '{value}'"))?;
            }
            _ => {
                return Err(format!("Unknown GBA config key: {key}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gba_config_default_values() {
        let config = GbaConfig::default();
        assert_eq!(config.hardware, GbaModel::Agb);
    }

    #[test]
    fn test_gba_model_parse_agb() {
        assert_eq!(GbaModel::parse("agb"), Some(GbaModel::Agb));
        assert_eq!(GbaModel::parse("AGB"), Some(GbaModel::Agb));
    }

    #[test]
    fn test_gba_model_parse_sp() {
        assert_eq!(GbaModel::parse("sp"), Some(GbaModel::Sp));
        assert_eq!(GbaModel::parse("SP"), Some(GbaModel::Sp));
    }

    #[test]
    fn test_gba_model_parse_micro() {
        assert_eq!(GbaModel::parse("micro"), Some(GbaModel::Micro));
        assert_eq!(GbaModel::parse("MICRO"), Some(GbaModel::Micro));
    }

    #[test]
    fn test_gba_model_parse_invalid() {
        assert_eq!(GbaModel::parse("invalid"), None);
        assert_eq!(GbaModel::parse(""), None);
    }

    #[test]
    fn test_gba_model_as_str() {
        assert_eq!(GbaModel::Agb.as_str(), "agb");
        assert_eq!(GbaModel::Sp.as_str(), "sp");
        assert_eq!(GbaModel::Micro.as_str(), "micro");
    }

    #[test]
    fn test_cli_parse_gba_hardware_agb() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-hardware".to_string(),
            "agb".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, GbaModel::Agb);
    }

    #[test]
    fn test_cli_parse_gba_hardware_sp() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-hardware".to_string(),
            "sp".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, GbaModel::Sp);
    }

    #[test]
    fn test_cli_parse_gba_hardware_micro() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-hardware".to_string(),
            "micro".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, GbaModel::Micro);
    }

    #[test]
    fn test_cli_parse_gba_hardware_invalid() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-hardware".to_string(),
            "invalid".to_string(),
        ];
        let result = config.apply_args(&args);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Invalid --gba-hardware value"));
        assert!(err_msg.contains("agb, sp, micro"));
    }

    #[test]
    fn test_config_file_parse_gba_hardware_agb() {
        let mut config = GbaConfig::default();
        config.apply_config_value("gba-hardware", "agb").unwrap();
        assert_eq!(config.hardware, GbaModel::Agb);
    }

    #[test]
    fn test_config_file_parse_gba_hardware_sp() {
        let mut config = GbaConfig::default();
        config.apply_config_value("gba-hardware", "sp").unwrap();
        assert_eq!(config.hardware, GbaModel::Sp);
    }

    #[test]
    fn test_config_file_parse_gba_hardware_micro() {
        let mut config = GbaConfig::default();
        config.apply_config_value("gba-hardware", "micro").unwrap();
        assert_eq!(config.hardware, GbaModel::Micro);
    }

    #[test]
    fn test_config_file_parse_gba_hardware_invalid() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("gba-hardware", "invalid");
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Invalid gba_hardware value"));
        assert!(err_msg.contains("agb, sp, micro"));
    }

    #[test]
    fn test_config_file_unknown_key() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("unknown-key", "value");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown GBA config key"));
    }

    #[test]
    fn test_gba_cli_flags_include_gba_bios_path() {
        assert!(
            GBA_CLI_FLAGS.iter().any(|f| f.flag == "--gba-bios-path"),
            "GBA CLI flags should include --gba-bios-path"
        );
    }

    #[test]
    fn test_config_file_parse_gba_bios_path_supported() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("gba-bios-path", "/tmp/gba_bios.bin");
        assert!(
            result.is_ok(),
            "gba-bios-path should be accepted as a valid GBA config key"
        );
    }

    #[test]
    fn test_gba_config_skip_bios_intro_default_false() {
        let config = GbaConfig::default();
        assert!(
            !config.skip_bios_intro,
            "skip_bios_intro should default to false"
        );
    }

    #[test]
    fn test_cli_parse_skip_bios_intro_flag() {
        let mut config = GbaConfig::default();
        let args = vec!["neser".to_string(), "--skip-bios-intro".to_string()];
        config.apply_args(&args).unwrap();
        assert!(
            config.skip_bios_intro,
            "--skip-bios-intro should set to true"
        );
    }

    #[test]
    fn test_cli_parse_skip_bios_intro_explicit_true() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--skip-bios-intro".to_string(),
            "true".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert!(config.skip_bios_intro);
    }

    #[test]
    fn test_cli_parse_skip_bios_intro_explicit_false() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--skip-bios-intro".to_string(),
            "false".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert!(!config.skip_bios_intro);
    }

    #[test]
    fn test_config_file_parse_skip_bios_intro_true() {
        let mut config = GbaConfig::default();
        config
            .apply_config_value("skip-bios-intro", "true")
            .unwrap();
        assert!(config.skip_bios_intro);
    }

    #[test]
    fn test_config_file_parse_skip_bios_intro_false() {
        let mut config = GbaConfig::default();
        config
            .apply_config_value("skip-bios-intro", "false")
            .unwrap();
        assert!(!config.skip_bios_intro);
    }

    #[test]
    fn test_config_file_parse_skip_bios_intro_invalid() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("skip-bios-intro", "maybe");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Invalid skip_bios_intro value")
        );
    }

    #[test]
    fn test_gba_cli_flags_include_skip_bios_intro() {
        assert!(
            GBA_CLI_FLAGS.iter().any(|f| f.flag == "--skip-bios-intro"),
            "GBA CLI flags should include --skip-bios-intro"
        );
    }

    #[test]
    fn test_gba_config_color_correction_default_false() {
        let config = GbaConfig::default();
        assert!(
            !config.color_correction,
            "color_correction should default to false"
        );
    }

    #[test]
    fn test_cli_parse_gba_color_correction_flag() {
        let mut config = GbaConfig::default();
        let args = vec!["neser".to_string(), "--gba-color-correction".to_string()];
        config.apply_args(&args).unwrap();
        assert!(
            config.color_correction,
            "--gba-color-correction should enable color correction"
        );
    }

    #[test]
    fn test_cli_parse_gba_color_correction_explicit_true() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-color-correction".to_string(),
            "true".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert!(config.color_correction);
    }

    #[test]
    fn test_cli_parse_gba_color_correction_explicit_false() {
        let mut config = GbaConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gba-color-correction".to_string(),
            "false".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert!(!config.color_correction);
    }

    #[test]
    fn test_config_file_parse_gba_color_correction_true() {
        let mut config = GbaConfig::default();
        config
            .apply_config_value("gba-color-correction", "true")
            .unwrap();
        assert!(config.color_correction);
    }

    #[test]
    fn test_config_file_parse_gba_color_correction_false() {
        let mut config = GbaConfig::default();
        config
            .apply_config_value("gba-color-correction", "false")
            .unwrap();
        assert!(!config.color_correction);
    }

    #[test]
    fn test_config_file_parse_gba_color_correction_invalid() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("gba-color-correction", "maybe");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Invalid gba_color_correction value")
        );
    }

    #[test]
    fn test_gba_cli_flags_include_gba_color_correction() {
        assert!(
            GBA_CLI_FLAGS
                .iter()
                .any(|f| f.flag == "--gba-color-correction"),
            "GBA CLI flags should include --gba-color-correction"
        );
    }
}
