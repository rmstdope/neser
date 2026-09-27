//! SNES-specific configuration.

use crate::platform::config::{CliFlag, parse_cli_string_arg};
use crate::snes::input::{GamePeripheral, SnesControllerType};

/// Names accepted by `--snes-filter` / `snes-filter` (see `platform::shaders::SHADER_PRESETS`),
/// in the order F4 cycles them: the NES looks.
pub(crate) const SNES_FILTER_NAMES: &[&str] = &["none", "crt", "smooth", "ntsc", "pal"];

pub(crate) const SNES_CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--snes-filter",
        help: Some("SNES shader filter: crt, ntsc, smooth, pal, or none"),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-spc-ipl-path",
        help: Some(
            "Path to custom 64-byte SNES SPC IPL ROM (falls back to embedded clean-room IPL)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-firmware-dir",
        help: Some(
            "Folder holding SNES coprocessor firmware such as dsp1b.rom (default: ~/.neser/firmware)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-hardware",
        help: Some("SNES hardware timing mode: snes-ntsc or snes-pal"),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-controller-port1",
        help: Some("SNES port 1 controller: standard, multitap, mouse or superscope"),
        has_value: true,
    },
    CliFlag {
        flag: "--snes-controller-port2",
        help: Some("SNES port 2 controller: standard, multitap, mouse or superscope"),
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
    /// Folder holding coprocessor firmware such as `dsp1b.rom` (`snes-firmware-dir`); `None`
    /// means `~/.neser/firmware`. See [`SnesConfig::resolved_firmware_dir`].
    pub firmware_dir: Option<String>,
    /// Device plugged into controller port 1.
    pub controller_port1: SnesControllerType,
    /// Device plugged into controller port 2.
    pub controller_port2: SnesControllerType,
    /// Whether the player chose port 1's device (CLI flag or config file). A choice wins
    /// over plugging the SNES Mouse in for a game that needs it.
    pub controller_port1_explicit: bool,
    /// Whether the player chose port 2's device (CLI flag or config file). A choice wins
    /// over plugging the Super Scope in for a Super Scope game.
    pub controller_port2_explicit: bool,
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
    /// The firmware folder: the configured one, or `$HOME/.neser/firmware`. The default is built
    /// from `HOME` so every message shows the full path; a configured value is used as written.
    pub fn resolved_firmware_dir(&self) -> std::path::PathBuf {
        match &self.firmware_dir {
            Some(dir) => std::path::PathBuf::from(dir),
            None => std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default()
                .join(".neser")
                .join("firmware"),
        }
    }

    /// The devices to plug into ports 1 and 2 for a game: the configured ones, except that a
    /// Super Scope game gets the Super Scope on port 2 and a game that needs the SNES Mouse
    /// gets the mouse on port 1, unless the player chose that port's device. Decided per
    /// load; the configuration itself never changes.
    pub fn effective_controller_ports(
        &self,
        game: GamePeripheral,
    ) -> (SnesControllerType, SnesControllerType) {
        let port1 = if game == GamePeripheral::Mouse && !self.controller_port1_explicit {
            SnesControllerType::Mouse
        } else {
            self.controller_port1
        };
        let port2 = if game == GamePeripheral::SuperScope && !self.controller_port2_explicit {
            SnesControllerType::SuperScope
        } else {
            self.controller_port2
        };
        (port1, port2)
    }

    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        if let Some(path) = parse_cli_string_arg(args, "--snes-spc-ipl-path") {
            self.spc_ipl_path = Some(path);
        }
        if let Some(dir) = parse_cli_string_arg(args, "--snes-firmware-dir") {
            self.firmware_dir = (!dir.is_empty()).then_some(dir);
        }
        if let Some(hardware) = parse_cli_string_arg(args, "--snes-hardware") {
            self.hardware = Some(SnesHardware::parse(&hardware).ok_or_else(|| {
                format!(
                    "Invalid --snes-hardware value: '{hardware}'. Valid options are: snes-ntsc, snes-pal"
                )
            })?);
        }
        if let Some(value) = parse_cli_string_arg(args, "--snes-controller-port1") {
            self.controller_port1 = parse_controller_type("--snes-controller-port1", &value)?;
            self.controller_port1_explicit = true;
        }
        if let Some(value) = parse_cli_string_arg(args, "--snes-controller-port2") {
            self.controller_port2 = parse_controller_type("--snes-controller-port2", &value)?;
            self.controller_port2_explicit = true;
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
            "snes_firmware_dir" => {
                self.firmware_dir = (!value.is_empty()).then(|| value.to_string());
            }
            "snes_hardware" => {
                self.hardware = Some(SnesHardware::parse(value).ok_or_else(|| {
                    format!(
                        "Invalid snes_hardware value: '{value}'. Valid options are: snes-ntsc, snes-pal"
                    )
                })?);
            }
            "snes_controller_port1" | "controller_port1" => {
                self.controller_port1 = parse_controller_type("snes_controller_port1", value)?;
                self.controller_port1_explicit = true;
            }
            "snes_controller_port2" | "controller_port2" => {
                self.controller_port2 = parse_controller_type("snes_controller_port2", value)?;
                self.controller_port2_explicit = true;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Parse a controller-type value, producing a descriptive error on failure.
fn parse_controller_type(key: &str, value: &str) -> Result<SnesControllerType, String> {
    SnesControllerType::parse(value).ok_or_else(|| {
        format!(
            "Invalid {key} value: '{value}'. Valid options are: standard, multitap, mouse, superscope"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{SnesConfig, SnesHardware};
    use crate::snes::input::{GamePeripheral, SnesControllerType};

    #[test]
    fn controller_ports_default_to_standard() {
        let cfg = SnesConfig::default();
        assert_eq!(cfg.controller_port1, SnesControllerType::Standard);
        assert_eq!(cfg.controller_port2, SnesControllerType::Standard);
    }

    #[test]
    fn controller_port_parses_from_cli_flag() {
        let mut cfg = SnesConfig::default();
        cfg.apply_args(&[
            "neser".to_string(),
            "--snes-controller-port2".to_string(),
            "multitap".to_string(),
        ])
        .expect("args parse");
        assert_eq!(cfg.controller_port2, SnesControllerType::Multitap);
    }

    #[test]
    fn controller_port_parses_from_config_key() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes-controller-port1", "mouse")
            .expect("config parse");
        assert_eq!(cfg.controller_port1, SnesControllerType::Mouse);
    }

    #[test]
    fn port2_is_explicit_only_once_the_player_sets_it() {
        assert!(!SnesConfig::default().controller_port2_explicit);

        let mut from_cli = SnesConfig::default();
        from_cli
            .apply_args(&[
                "neser".to_string(),
                "--snes-controller-port2".to_string(),
                "standard".to_string(),
            ])
            .expect("args parse");
        assert!(from_cli.controller_port2_explicit);

        let mut from_file = SnesConfig::default();
        from_file
            .apply_config_value("snes_controller_port2", "standard")
            .expect("config parse");
        assert!(from_file.controller_port2_explicit);
    }

    #[test]
    fn a_super_scope_game_gets_the_scope_on_port2_and_keeps_port1() {
        let cfg = SnesConfig {
            controller_port1: SnesControllerType::Mouse,
            ..SnesConfig::default()
        };
        assert_eq!(
            cfg.effective_controller_ports(GamePeripheral::SuperScope),
            (SnesControllerType::Mouse, SnesControllerType::SuperScope)
        );
        // The player's settings are untouched, so the next game starts from them again.
        assert_eq!(cfg.controller_port2, SnesControllerType::Standard);
    }

    #[test]
    fn the_players_port2_choice_wins_over_a_super_scope_game() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes_controller_port2", "standard")
            .expect("config parse");
        assert_eq!(
            cfg.effective_controller_ports(GamePeripheral::SuperScope),
            (SnesControllerType::Standard, SnesControllerType::Standard)
        );
    }

    #[test]
    fn port1_is_explicit_only_once_the_player_sets_it() {
        assert!(!SnesConfig::default().controller_port1_explicit);

        let mut from_cli = SnesConfig::default();
        from_cli
            .apply_args(&[
                "neser".to_string(),
                "--snes-controller-port1".to_string(),
                "standard".to_string(),
            ])
            .expect("args parse");
        assert!(from_cli.controller_port1_explicit);

        let mut from_file = SnesConfig::default();
        from_file
            .apply_config_value("controller_port1", "standard")
            .expect("config parse");
        assert!(from_file.controller_port1_explicit);
    }

    #[test]
    fn a_mouse_game_gets_the_mouse_on_port1_and_keeps_port2() {
        let cfg = SnesConfig {
            controller_port2: SnesControllerType::Multitap,
            ..SnesConfig::default()
        };
        assert_eq!(
            cfg.effective_controller_ports(GamePeripheral::Mouse),
            (SnesControllerType::Mouse, SnesControllerType::Multitap)
        );
        // The player's settings are untouched, so the next game starts from them again.
        assert_eq!(cfg.controller_port1, SnesControllerType::Standard);
    }

    #[test]
    fn the_players_port1_choice_wins_over_a_mouse_game() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes_controller_port1", "standard")
            .expect("config parse");
        assert_eq!(
            cfg.effective_controller_ports(GamePeripheral::Mouse),
            (SnesControllerType::Standard, SnesControllerType::Standard)
        );
    }

    #[test]
    fn other_games_use_the_configured_ports() {
        let cfg = SnesConfig::default();
        assert_eq!(
            cfg.effective_controller_ports(GamePeripheral::None),
            (SnesControllerType::Standard, SnesControllerType::Standard)
        );
    }

    #[test]
    fn invalid_controller_port_value_is_rejected() {
        let mut cfg = SnesConfig::default();
        assert!(
            cfg.apply_config_value("snes-controller-port1", "bogus")
                .is_err()
        );
    }

    #[test]
    fn snes_firmware_dir_parses_from_config_key_and_cli_flag() {
        let mut cfg = SnesConfig::default();
        cfg.apply_config_value("snes-firmware-dir", "/fw/from/config")
            .expect("config parse");
        assert_eq!(
            cfg.resolved_firmware_dir(),
            std::path::PathBuf::from("/fw/from/config")
        );
        cfg.apply_args(&[
            "neser".to_string(),
            "--snes-firmware-dir".to_string(),
            "/fw/from/cli".to_string(),
        ])
        .expect("args parse");
        assert_eq!(cfg.firmware_dir.as_deref(), Some("/fw/from/cli"));
        cfg.apply_config_value("snes-firmware-dir", "")
            .expect("config parse");
        assert_eq!(cfg.firmware_dir, None);
    }

    #[test]
    fn resolved_firmware_dir_defaults_to_home_neser_firmware() {
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        assert_eq!(
            SnesConfig::default().resolved_firmware_dir(),
            home.join(".neser").join("firmware")
        );
    }

    #[test]
    fn snes_firmware_dir_flag_has_the_agreed_help() {
        let flag = super::SNES_CLI_FLAGS
            .iter()
            .find(|f| f.flag == "--snes-firmware-dir")
            .expect("flag declared");
        assert_eq!(
            flag.help,
            Some(
                "Folder holding SNES coprocessor firmware such as dsp1b.rom (default: ~/.neser/firmware)"
            )
        );
        assert!(flag.has_value);
    }

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
