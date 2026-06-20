//! SNES-specific configuration.

use crate::platform::config::{CliFlag, parse_cli_string_arg};

pub(crate) const SNES_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--snes-spc-ipl-path",
        help: Some(
            "Path to custom 64-byte SNES SPC IPL ROM (falls back to embedded clean-room IPL)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-hardware",
        help: Some("SNES hardware timing mode: snes-ntsc or snes-pal"),
        has_value: true,
    },
];

/// Configuration options for SNES emulation.
#[derive(Debug, Clone, Default)]
pub struct SnesConfig {
    /// Optional SNES video hardware mode override.
    pub hardware: Option<SnesHardware>,
    /// Optional path to an external 64-byte SPC IPL ROM.
    pub spc_ipl_path: Option<String>,
}

/// SNES video hardware timing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnesHardware {
    Ntsc,
    Pal,
}

impl SnesHardware {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "snes-ntsc" => Some(Self::Ntsc),
            "snes-pal" => Some(Self::Pal),
            _ => None,
        }
    }
}

impl SnesConfig {
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(path) = parse_cli_string_arg(args, "--snes-spc-ipl-path") {
            self.spc_ipl_path = Some(path);
        }
        if let Some(hardware) = parse_cli_string_arg(args, "--snes-hardware") {
            self.hardware = Some(SnesHardware::parse(&hardware).ok_or_else(|| {
                format!(
                    "Invalid --snes-hardware value: '{hardware}'. Valid options are: snes-ntsc, snes-pal"
                )
            })?);
        }
        Ok(())
    }

    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        match key.as_str() {
            "snes_spc_ipl_path" | "spc_ipl_path" => {
                if value.is_empty() {
                    self.spc_ipl_path = None;
                } else {
                    self.spc_ipl_path = Some(value.to_string());
                }
            }
            "snes_hardware" => {
                self.hardware = Some(SnesHardware::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid snes_hardware value: '{value}'. Valid options are: snes-ntsc, snes-pal"
                    )
                })?);
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{SnesConfig, SnesHardware};

    #[test]
    fn snes_spc_ipl_path_parses_from_config_key() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes-spc-ipl-path", "/tmp/custom-ipl.bin")
            .expect("config parse");
        assert_eq!(cfg.spc_ipl_path.as_deref(), Some("/tmp/custom-ipl.bin"));
    }

    #[test]
    fn snes_spc_ipl_path_parses_from_cli_flag() {
        let mut cfg = SnesConfig::default();
        cfg.apply_args(&[
            "neser".to_string(),
            "--snes-spc-ipl-path".to_string(),
            "/tmp/ipl.bin".to_string(),
        ])
        .expect("args parse");
        assert_eq!(cfg.spc_ipl_path.as_deref(), Some("/tmp/ipl.bin"));
    }

    #[test]
    fn snes_hardware_parses_from_config_key() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes-hardware", "snes-pal")
            .expect("config parse");
        assert_eq!(cfg.hardware, Some(SnesHardware::Pal));
    }

    #[test]
    fn snes_hardware_parses_from_cli_flag() {
        let mut cfg = SnesConfig::default();
        cfg.apply_args(&[
            "neser".to_string(),
            "--snes-hardware".to_string(),
            "snes-pal".to_string(),
        ])
        .expect("args parse");
        assert_eq!(cfg.hardware, Some(SnesHardware::Pal));
    }

    #[test]
    fn snes_hardware_parser_is_case_insensitive() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes-hardware", "SNES-PAL")
            .expect("config parse");
        assert_eq!(cfg.hardware, Some(SnesHardware::Pal));
    }

    #[test]
    fn snes_hardware_invalid_value_returns_error() {
        let mut cfg = SnesConfig::default();
        let err = cfg
            .apply_config_value("snes-hardware", "invalid")
            .expect_err("invalid hardware should be rejected");
        assert!(err.contains("Invalid snes_hardware value"));
    }
}
