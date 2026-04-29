//! Configuration for the Game Boy Advance emulator.
//!
//! [`GbaConfig`] holds GBA-specific configuration options such as the
//! emulated hardware model selection.

use crate::platform::config::CliFlag;

/// GBA-specific CLI flags, defined here so that the GBA module owns its flag
/// declarations and parsing logic. These are chained into the global flag list
/// by the platform config parser for validation and help-text generation.
pub(crate) const GBA_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--gba-filter",
        help: Some("GBA shader filter: gba-lcd or none"),
        has_value: true,
    },
    CliFlag {
        flag: "--gba-hardware",
        help: Some("GBA hardware model: agb, sp, micro (default: agb)"),
        has_value: true,
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
        match s.to_lowercase().as_str() {
            "agb" => Some(Self::Agb),
            "sp" => Some(Self::Sp),
            "micro" => Some(Self::Micro),
            _ => None,
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
}

impl GbaConfig {
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

        Ok(())
    }

    /// Apply a config file key-value pair to this config.
    ///
    /// Accepts `gba-hardware` key.
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "gba-hardware" => {
                self.hardware = GbaModel::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid gba-hardware value: '{value}'. Valid options are: {VALID_HARDWARE_MODELS}",
                    )
                })?;
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
        assert!(err_msg.contains("Invalid gba-hardware value"));
        assert!(err_msg.contains("agb, sp, micro"));
    }

    #[test]
    fn test_config_file_unknown_key() {
        let mut config = GbaConfig::default();
        let result = config.apply_config_value("unknown-key", "value");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown GBA config key"));
    }
}
