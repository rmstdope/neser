//! Configuration for the Game Boy emulator.
//!
//! [`GbConfig`] holds Game Boy-specific configuration options such as the
//! emulated DMG hardware variant.

use crate::gb::model::DmgModel;
use crate::platform::config::CliFlag;

/// GB-specific CLI flags, defined here so that the GB module owns its flag
/// declarations and parsing logic. These are chained into the global flag list
/// by the platform config parser for validation and help-text generation.
pub(crate) const GB_CLI_FLAGS: &[CliFlag] = &[CliFlag {
    flag: "--gb-dmg-variant",
    help: Some("DMG hardware variant: dmg-0, dmg-a, dmg-b, dmg-c (default: dmg-b)"),
    has_value: true,
}];

/// Valid values for the `gb-dmg-variant` option (used in error messages).
const VALID_DMG_VARIANTS: &str = "dmg-0, dmg-a, dmg-b, dmg-c";

/// Game Boy-specific hardware configuration.
#[derive(Debug, Clone)]
pub struct GbConfig {
    /// Emulated DMG hardware variant.
    pub dmg_variant: DmgModel,
}

impl Default for GbConfig {
    fn default() -> Self {
        Self {
            dmg_variant: DmgModel::DmgB,
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
        Ok(())
    }

    /// Apply a `gb-dmg-variant` config file value to this config.
    pub(crate) fn apply_config_value(&mut self, value: &str) -> Result<(), String> {
        self.dmg_variant = DmgModel::parse(value).ok_or_else(|| {
            format!(
                "Invalid gb-dmg-variant value: '{value}'. Valid options are: {VALID_DMG_VARIANTS}",
            )
        })?;
        Ok(())
    }
}
