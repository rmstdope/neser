//! Configuration for the Game Boy emulator.
//!
//! [`GbConfig`] holds Game Boy-specific configuration options such as the
//! emulated hardware variant (DMG or CGB).

use crate::platform::config::CliFlag;

/// GB-specific CLI flags, defined here so that the GB module owns its flag
/// declarations and parsing logic. These are chained into the global flag list
/// by the platform config parser for validation and help-text generation.
pub(crate) const GB_CLI_FLAGS: &[CliFlag] = &[CliFlag {
    flag: "--gb-hardware",
    help: Some("Game Boy hardware variant: dmg or cgb (default: dmg)"),
    has_value: true,
}];

/// Game Boy hardware variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbHardware {
    /// Original Game Boy (DMG).
    Dmg,
    /// Game Boy Color (CGB).
    Cgb,
}

impl GbHardware {
    /// Parse a hardware variant from a string value.
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("dmg") {
            Some(Self::Dmg)
        } else if value.eq_ignore_ascii_case("cgb") {
            Some(Self::Cgb)
        } else {
            None
        }
    }
}

/// Game Boy-specific hardware configuration.
#[derive(Debug, Clone)]
pub struct GbConfig {
    /// Emulated hardware variant.
    pub hardware: GbHardware,
    /// Whether hardware was explicitly configured (vs. auto-detected from ROM).
    pub hardware_explicit: bool,
}

impl Default for GbConfig {
    fn default() -> Self {
        Self {
            hardware: GbHardware::Dmg,
            hardware_explicit: false,
        }
    }
}

impl GbConfig {
    /// Parse GB-specific CLI arguments and apply them to this config.
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(gb_hardware) = Self::parse_string_arg(args, "--gb-hardware") {
            self.hardware = GbHardware::parse(&gb_hardware).ok_or_else(|| {
                format!(
                    "Invalid --gb-hardware value: '{}'. Valid options are: dmg, cgb",
                    gb_hardware
                )
            })?;
            self.hardware_explicit = true;
        }
        Ok(())
    }

    /// Apply a `gb-hardware` config file value to this config.
    pub(crate) fn apply_config_value(&mut self, value: &str) -> Result<(), String> {
        self.hardware = GbHardware::parse(value).ok_or_else(|| {
            format!(
                "Invalid gb-hardware value: '{}'. Valid options are: dmg, cgb",
                value
            )
        })?;
        self.hardware_explicit = true;
        Ok(())
    }

    /// Look up a CLI flag value (handles both `--flag value` and `--flag=value`).
    fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == flag && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
            if let Some((flag_part, value_part)) = args[i].split_once('=')
                && flag_part == flag
            {
                return Some(value_part.to_string());
            }
        }
        None
    }
}
