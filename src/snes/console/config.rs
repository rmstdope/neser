//! SNES-specific configuration.

use crate::platform::config::{CliFlag, parse_cli_string_arg};

pub(crate) const SNES_CLI_FLAGS: &[CliFlag] = &[CliFlag {
    flag: "--snes-spc-ipl-path",
    help: Some("Path to custom 64-byte SNES SPC IPL ROM (falls back to embedded clean-room IPL)"),
    has_value: true,
}];

/// Configuration options for SNES emulation.
#[derive(Debug, Clone, Default)]
pub struct SnesConfig {
    /// Optional path to an external 64-byte SPC IPL ROM.
    pub spc_ipl_path: Option<String>,
}

impl SnesConfig {
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(path) = parse_cli_string_arg(args, "--snes-spc-ipl-path") {
            self.spc_ipl_path = Some(path);
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
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::SnesConfig;

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
}
