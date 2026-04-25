//! Configuration for the Game Boy emulator.
//!
//! [`GbConfig`] holds Game Boy-specific configuration options such as the
//! emulated DMG hardware variant and hardware target selection.

use crate::gb::model::{DmgModel, GbHardware};
use crate::platform::config::CliFlag;

/// GB-specific CLI flags, defined here so that the GB module owns its flag
/// declarations and parsing logic. These are chained into the global flag list
/// by the platform config parser for validation and help-text generation.
pub(crate) const GB_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--gb-filter",
        help: Some("Game Boy shader filter: dmg or none"),
        has_value: true,
    },
    CliFlag {
        flag: "--gb-dmg-variant",
        help: Some("DMG hardware variant: dmg-0, dmg-a, dmg-b, dmg-c (default: dmg-b)"),
        has_value: true,
    },
    CliFlag {
        flag: "--gb-hardware",
        help: Some("Game Boy hardware target: dmg, cgb, gba (default: auto-detect from ROM)"),
        has_value: true,
    },
];

/// Valid values for the `gb-dmg-variant` option (used in error messages).
const VALID_DMG_VARIANTS: &str = "dmg-0, dmg-a, dmg-b, dmg-c";

/// Valid values for the `gb-hardware` option (used in error messages).
const VALID_HARDWARE_TARGETS: &str = "dmg, cgb, gba";

/// Game Boy-specific hardware configuration.
#[derive(Debug, Clone)]
pub struct GbConfig {
    /// Emulated DMG hardware variant.
    pub dmg_variant: DmgModel,
    /// Game Boy hardware target selection (DMG/CGB/GBA).
    /// `None` means auto-detect from ROM flags.
    pub hardware: Option<GbHardware>,
}

impl Default for GbConfig {
    fn default() -> Self {
        Self {
            dmg_variant: DmgModel::DmgB,
            hardware: None,
        }
    }
}

impl GbConfig {
    /// Parse GB-specific CLI arguments and apply them to this config.
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(variant) =
            crate::platform::config::parse_cli_string_arg(args, "--gb-dmg-variant")
        {
            self.dmg_variant = DmgModel::parse(&variant).ok_or_else(|| {
                format!(
                    "Invalid --gb-dmg-variant value: '{variant}'. Valid options are: {VALID_DMG_VARIANTS}",
                )
            })?;
        }

        if let Some(hardware) = crate::platform::config::parse_cli_string_arg(args, "--gb-hardware")
        {
            self.hardware = Some(GbHardware::parse(&hardware).ok_or_else(|| {
                format!(
                    "Invalid --gb-hardware value: '{hardware}'. Valid options are: {VALID_HARDWARE_TARGETS}",
                )
            })?);
        }

        Ok(())
    }

    /// Apply a config file key-value pair to this config.
    ///
    /// Accepts `gb-dmg-variant` and `gb-hardware` keys.
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "gb-dmg-variant" => {
                self.dmg_variant = DmgModel::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid gb-dmg-variant value: '{value}'. Valid options are: {VALID_DMG_VARIANTS}",
                    )
                })?;
            }
            "gb-hardware" => {
                self.hardware = Some(GbHardware::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid gb-hardware value: '{value}'. Valid options are: {VALID_HARDWARE_TARGETS}",
                    )
                })?);
            }
            _ => {
                return Err(format!("Unknown GB config key: {key}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gb_config_default_values() {
        let config = GbConfig::default();
        assert_eq!(config.dmg_variant, DmgModel::DmgB);
        assert_eq!(config.hardware, None);
    }

    #[test]
    fn test_cli_parse_gb_hardware_dmg() {
        let mut config = GbConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "dmg".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Dmg));
    }

    #[test]
    fn test_cli_parse_gb_hardware_cgb() {
        let mut config = GbConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "cgb".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Cgb));
    }

    #[test]
    fn test_cli_parse_gb_hardware_gba() {
        let mut config = GbConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "gba".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Gba));
    }

    #[test]
    fn test_cli_parse_gb_hardware_invalid() {
        let mut config = GbConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "invalid".to_string(),
        ];
        let result = config.apply_args(&args);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Invalid --gb-hardware value"));
        assert!(err_msg.contains("dmg, cgb, gba"));
    }

    #[test]
    fn test_config_file_parse_gb_hardware_dmg() {
        let mut config = GbConfig::default();
        config.apply_config_value("gb-hardware", "dmg").unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Dmg));
    }

    #[test]
    fn test_config_file_parse_gb_hardware_cgb() {
        let mut config = GbConfig::default();
        config.apply_config_value("gb-hardware", "cgb").unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Cgb));
    }

    #[test]
    fn test_config_file_parse_gb_hardware_gba() {
        let mut config = GbConfig::default();
        config.apply_config_value("gb-hardware", "gba").unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Gba));
    }

    #[test]
    fn test_config_file_parse_gb_hardware_invalid() {
        let mut config = GbConfig::default();
        let result = config.apply_config_value("gb-hardware", "invalid");
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Invalid gb-hardware value"));
        assert!(err_msg.contains("dmg, cgb, gba"));
    }

    #[test]
    fn test_gb_hardware_independent_of_dmg_variant() {
        let mut config = GbConfig::default();
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "dmg".to_string(),
            "--gb-dmg-variant".to_string(),
            "dmg-0".to_string(),
        ];
        config.apply_args(&args).unwrap();
        assert_eq!(config.hardware, Some(GbHardware::Dmg));
        assert_eq!(config.dmg_variant, DmgModel::Dmg0);
    }
}
