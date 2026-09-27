use super::*;

use crate::platform::config::parse_hex_u8;

impl NesConfig {
    /// Apply a single config file key-value pair to NES configuration.
    ///
    /// Handles every `nes-` config key: hardware (`nes-hardware`), expansion port,
    /// VS System DIP switches and wiring, controller ports, APU channel toggles
    /// (`nes-pulse1`, `nes-pulse2`, `nes-triangle`, `nes-noise`, `nes-dmc`),
    /// `nes-zapper_detection_size`, overscan (`nes-horizontal_overscan`,
    /// `nes-vertical_overscan`), `nes-palette`, `nes-oam_dram_decay`, and
    /// `nes-enable_4_score`. `nes-filter` is the platform's, as it picks the
    /// frontend shader. Keys this config does not own are ignored.
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        use crate::platform::config::parse_bool;
        let key = key.replace('-', "_");

        match key.as_str() {
            "nes_enable_4_score" => {
                if let Ok(b) = parse_bool(value) {
                    self.four_score_enabled = b;
                    self.four_score_enabled_explicit = true;
                }
            }
            "nes_pulse1" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE1);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE1);
                    }
                }
            }
            "nes_pulse2" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE2);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE2);
                    }
                }
            }
            "nes_triangle" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::TRIANGLE);
                    } else {
                        self.apu_channels.remove(ApuChannels::TRIANGLE);
                    }
                }
            }
            "nes_noise" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::NOISE);
                    } else {
                        self.apu_channels.remove(ApuChannels::NOISE);
                    }
                }
            }
            "nes_dmc" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::DMC);
                    } else {
                        self.apu_channels.remove(ApuChannels::DMC);
                    }
                }
            }
            "nes_zapper_detection_size" => {
                if let Ok(size) = value.parse::<u8>() {
                    self.zapper_detection_size = size;
                    if size > 10 {
                        eprintln!(
                            "Warning: nes-zapper_detection_size={} may cause performance issues. \
                             Large values sample (2*size + 1)² = {} pixels per controller read. \
                             Consider using values ≤ 10 for better performance.",
                            size,
                            (2 * size as u32 + 1).pow(2)
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'nes-zapper_detection_size' in configuration; \
                         ignoring. Must be a number between 0 and 255.",
                        value
                    );
                }
            }
            "nes_oam_dram_decay" | "nes_oam_dram_decay_enabled" => {
                if let Ok(b) = parse_bool(value) {
                    self.oam_dram_decay_enabled = b;
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'nes-oam_dram_decay'; keeping current value. \
                         Valid values: true/false/yes/no/1/0",
                        value
                    );
                }
            }
            "nes_horizontal_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.horizontal_overscan = v.min(8);
                }
            }
            "nes_vertical_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.vertical_overscan = v.min(16);
                }
            }
            "nes_hardware" => self.apply_hardware_value(value)?,
            "nes_expansion_port" => self.apply_expansion_port_value(value)?,
            "nes_vs_dip_switches" => {
                self.vs_dip_switches = parse_hex_u8(value).map_err(|_| {
                    format!(
                        "Invalid nes_vs_dip_switches value: '{}'. Expected hex (0x00-0xFF) or decimal (0-255)",
                        value
                    )
                })?;
            }
            "nes_vs_controllers_swapped" => {
                if let Ok(b) = parse_bool(value) {
                    self.vs_controllers_swapped = b;
                }
            }
            "nes_controller_port1" => {
                self.controller_port1 = Self::parse_controller_arg("nes_controller_port1", value)?;
                self.controller_port1_explicit = true;
            }
            "nes_controller_port2" => {
                self.controller_port2 = Self::parse_controller_arg("nes_controller_port2", value)?;
                self.controller_port2_explicit = true;
            }
            "nes_palette" => {
                if let Some(palette) = NesPalette::from_config_id(value) {
                    self.palette = palette;
                } else {
                    eprintln!("{}", invalid_nes_palette_warning(value, self.palette));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// The warning printed for an unknown `nes-palette` config-file value.
fn invalid_nes_palette_warning(value: &str, kept: NesPalette) -> String {
    format!(
        "Warning: invalid value '{value}' for 'nes-palette'; keeping default ('{}'). \
         Valid values: {}",
        kept.config_id(),
        NesPalette::config_id_list()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::config::{Config, ParseResult, RamInitMode};
    use std::path::Path;

    fn config_new(mut args: Vec<String>) -> Result<ParseResult, String> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        if args.iter().any(|a| a == "--config") {
            return Config::new(&args);
        }

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        args.push("--config".to_string());
        args.push(file.path().to_string_lossy().to_string());

        Config::new(&args)
    }

    fn parse_config(args: Vec<String>) -> Config {
        match config_new(args).unwrap() {
            ParseResult::Config(c) => *c,
            ParseResult::Help => panic!("Expected Config, got Help"),
            ParseResult::Version => panic!("Expected Config, got Version"),
        }
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::with_defaults();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
        assert!(config.frontend.vsync_enabled);
        assert!(config.frontend.gamepads_enabled);
        assert!(!config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, None);
        assert_eq!(config.frontend.shader_path, None);
        assert!(!config.frontend.debugger_enabled);
        assert!(!config.frontend.load_state);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
        assert!(config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(config.nes.apu_channels.contains(ApuChannels::NOISE));
        assert!(config.nes.apu_channels.contains(ApuChannels::DMC));
        assert_eq!(config.frontend.window_height, 896);
        assert_eq!(config.frontend.rom_path, None);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_new_defaults() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
        assert!(config.frontend.vsync_enabled);
        assert!(config.frontend.gamepads_enabled);
        assert!(!config.frontend.fullscreen);
        assert_eq!(config.frontend.window_height, 896);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_file_horizontal_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "4")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 4);
    }

    #[test]
    fn test_config_file_vertical_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "12")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 12);
    }

    #[test]
    fn test_config_file_overscan_zero() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "0")
            .unwrap();
        config
            .apply_config_value("nes-vertical_overscan", "0")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 0);
        assert_eq!(config.nes.vertical_overscan, 0);
    }

    #[test]
    fn test_config_file_horizontal_overscan_max_is_8() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "8")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_max_is_16() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "16")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 16);
    }

    #[test]
    fn test_config_file_horizontal_overscan_above_max_is_clamped_to_8() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "9")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_above_max_is_clamped_to_16() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "17")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 16);
    }

    #[test]
    fn test_config_file_hardware_nes_pal() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "nes-pal")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_nes_ntsc() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .apply_config_value("nes-hardware", "nes-ntsc")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_case_insensitive() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "NES-PAL")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);

        config
            .apply_config_value("nes-hardware", "NES-NTSC")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_file_hardware_playchoice10_forces_playchoice_expansion() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "playchoice10")
            .unwrap();

        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
        assert_eq!(config.nes.expansion_port, ExpansionPort::Playchoice10);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_config_file_hardware_famicom_sets_mode_and_model() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .apply_config_value("nes-hardware", "famicom")
            .unwrap();
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert!(config.nes.hardware_mode_explicit);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_audio() {
        let mut config = Config::default();
        config.apply_config_value("audio", "false").unwrap();
        assert!(!config.frontend.audio_enabled);

        config.apply_config_value("audio", "true").unwrap();
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_vsync() {
        let mut config = Config::default();
        config.apply_config_value("vsync", "false").unwrap();
        assert!(!config.frontend.vsync_enabled);

        config.apply_config_value("vsync", "true").unwrap();
        assert!(config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_file_gamepads() {
        let mut config = Config::default();
        config.apply_config_value("gamepads", "false").unwrap();
        assert!(!config.frontend.gamepads_enabled);

        config.apply_config_value("gamepads", "true").unwrap();
        assert!(config.frontend.gamepads_enabled);
    }

    #[test]
    fn test_config_file_fullscreen() {
        let mut config = Config::default();
        config.apply_config_value("fullscreen", "true").unwrap();
        assert!(config.frontend.fullscreen);

        config.apply_config_value("fullscreen", "false").unwrap();
        assert!(!config.frontend.fullscreen);
    }

    #[test]
    fn test_config_file_display() {
        let mut config = Config::default();
        config.apply_config_value("display", "1").unwrap();
        assert_eq!(config.frontend.fullscreen_display, Some(1));

        config.apply_config_value("display", "0").unwrap();
        assert_eq!(config.frontend.fullscreen_display, Some(0));
    }

    #[test]
    fn test_config_file_display_negative_ignored() {
        let mut config = Config::default();
        config.apply_config_value("display", "-1").unwrap();
        assert_eq!(config.frontend.fullscreen_display, None);
    }

    #[test]
    fn test_config_file_debugger() {
        let mut config = Config::default();
        config.apply_config_value("debugger", "true").unwrap();
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_file_apu_channels() {
        let mut config = Config::default();
        config.apply_config_value("nes-pulse1", "false").unwrap();
        config.apply_config_value("nes-pulse2", "false").unwrap();
        config.apply_config_value("nes-triangle", "false").unwrap();
        config.apply_config_value("nes-noise", "false").unwrap();
        config.apply_config_value("nes-dmc", "false").unwrap();

        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(!config.nes.apu_channels.contains(ApuChannels::NOISE));
        assert!(!config.nes.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_file_window_height() {
        let mut config = Config::default();
        config.apply_config_value("window_height", "720").unwrap();
        assert_eq!(config.frontend.window_height, 720);
    }

    #[test]
    fn test_config_file_controller_ports() {
        let mut config = Config::default();
        let _ = config.apply_config_value("nes-controller_port1", "arkanoid");
        let _ = config.apply_config_value("nes-controller_port2", "joypad");

        assert_eq!(config.nes.controller_port1, ControllerType::Arkanoid);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
        assert!(config.nes.controller_port1_explicit);
        assert!(config.nes.controller_port2_explicit);
    }

    #[test]
    fn test_config_file_controller_port_invalid_value_errors() {
        let mut config = Config::default();
        let result = config.apply_config_value("nes-controller_port1", "unknown");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for 'nes_controller_port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_cli_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-controller-port1=joypad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
    }

    #[test]
    fn test_config_controller_port_invalid_cli_value_does_not_override_config_file_and_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-controller-port1=unknown".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for '--nes-controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_zapper_detection_size_from_file() {
        let config_content = "nes-zapper_detection_size=1\n";
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("neser.conf");
        std::fs::write(&config_path, config_content).unwrap();

        let mut config = Config::default();
        config.load_from_file(&config_path).unwrap();

        assert_eq!(config.nes.zapper_detection_size, 1);
    }

    #[test]
    fn test_config_file_trace_cpu() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "2").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 2);
    }

    #[test]
    fn test_config_file_trace_ppu() {
        let mut config = Config::default();
        config.apply_config_value("trace-ppu", "3").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 3);
    }

    #[test]
    fn test_config_file_trace_apu() {
        let mut config = Config::default();
        config.apply_config_value("trace-apu", "1").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.apu, 1);
    }

    #[test]
    fn test_config_file_trace_mapper() {
        let mut config = Config::default();
        config.apply_config_value("trace-mapper", "4").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 4);
    }

    #[test]
    fn test_config_file_trace_nestest() {
        let mut config = Config::default();
        config.apply_config_value("trace-nestest", "true").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert!(config.frontend.tracing.nestest);
    }

    #[test]
    fn test_config_file_trace_zero_does_not_enable() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "0").unwrap();
        assert!(!config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 0);
    }

    #[test]
    fn test_config_file_bool_formats() {
        let mut config = Config::default();

        // Test "yes"/"no"
        config.apply_config_value("audio", "no").unwrap();
        assert!(!config.frontend.audio_enabled);
        config.apply_config_value("audio", "yes").unwrap();
        assert!(config.frontend.audio_enabled);

        // Test "1"/"0"
        config.apply_config_value("audio", "0").unwrap();
        assert!(!config.frontend.audio_enabled);
        config.apply_config_value("audio", "1").unwrap();
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_unknown_key_ignored() {
        let mut config = Config::default();
        // Should not panic
        config
            .apply_config_value("unknown_key", "some_value")
            .unwrap();
        // Config should remain unchanged
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_file_load_from_string_content() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
# Test config file
nes-hardware=nes-pal
audio=false
fullscreen=true
display=2
nes-filter=crt
nes-pulse1=false
"#;

        // Create a temporary file with config content
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let mut config = Config::default();
        config.load_from_file(file.path()).unwrap();

        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, Some(2));
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
        );
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        // Other values should remain default
        assert!(config.frontend.vsync_enabled);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_file_accepts_dashes_as_underscores() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "window-height=600\ntrace-cpu=2\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let mut config = Config::default();
        config.load_from_file(file.path()).unwrap();

        assert_eq!(config.frontend.window_height, 600);
        assert_eq!(config.frontend.tracing.cpu, 2);
        assert!(config.frontend.tracing.enabled);
    }

    #[test]
    fn test_config_args_override_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create config file that sets PAL and disables audio
        let content = r#"
    nes-hardware=nes-pal
audio=false
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Start with default config
        let mut config = Config::default();

        // Load from config file
        config.load_from_file(file.path()).unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);

        // Apply args - no args means config file values should remain
        let args = vec!["neser".to_string()];
        config.apply_args(&args).unwrap();

        // Config file values should persist since args don't override them
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_hardware_flag_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-hardware=nes-pal\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-hardware".to_string(),
            "nes-ntsc".to_string(),
        ];

        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_two_arkanoid_controllers_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
nes-controller_port1=arkanoid
nes-controller_port2=arkanoid
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];

        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_nonexistent_silently_ignored() {
        let mut config = Config::default();
        config
            .load_from_file(Path::new("/nonexistent/path/neser.conf"))
            .unwrap();
        // Should not panic, config should remain default
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_flag_loads_specified_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-hardware=nes-pal\naudio=false\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_flag_invalid_file_errors() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "/nonexistent/path/config.conf".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("/nonexistent/path/config.conf"));
    }

    #[test]
    fn test_config_flag_missing_value_errors() {
        let args = vec!["neser".to_string(), "--config".to_string()];
        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_flag_overrides_default_locations() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a config file with --config that sets PAL
        let content = "nes-hardware=nes-pal\n";
        let mut explicit_file = NamedTempFile::new().unwrap();
        explicit_file.write_all(content.as_bytes()).unwrap();

        // The --config file should be used
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            explicit_file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_parse_config_arg() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "my_config.conf".to_string(),
        ];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, Some("my_config.conf".to_string()));
    }

    #[test]
    fn test_parse_config_arg_not_present() {
        let args = vec!["neser".to_string()];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, None);
    }

    #[test]
    fn test_config_file_tv_system_key_is_ignored() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-tv_system", "pal")
            .expect("legacy key should be ignored gracefully");
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_file_load_state() {
        let mut config = Config::default();
        config.apply_config_value("load_state", "true").unwrap();
        assert!(config.frontend.load_state);
    }

    #[test]
    fn test_config_file_ram_init_mode_zero() {
        let mut config = Config::default();
        config.apply_config_value("ram_init_mode", "zero").unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_config_file_ram_init_mode_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "random")
            .unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded-random:42")
            .unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::SeededRandom(42));
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random_underscore() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded_random:12345")
            .unwrap();
        assert_eq!(
            config.frontend.ram_init_mode,
            RamInitMode::SeededRandom(12345)
        );
    }

    #[test]
    fn test_config_file_oam_dram_decay_enabled() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-oam_dram_decay", "true")
            .unwrap();
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_file_nes_hardware_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-hardware", "famicom")
            .unwrap();
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_nes_pulse1_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config.apply_config_value("nes-pulse1", "false").unwrap();
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_file_nes_expansion_port_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-expansion_port", "zapper")
            .unwrap();
        assert_eq!(config.nes.expansion_port, ExpansionPort::ZapperFamicom);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_config_file_nes_oam_dram_decay_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-oam_dram_decay", "true")
            .unwrap();
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_file_nes_horizontal_overscan_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-horizontal_overscan", "6")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 6);
    }

    #[test]
    fn test_config_file_nes_vs_dip_switches_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-vs_dip_switches", "0xFF")
            .unwrap();
        assert_eq!(config.nes.vs_dip_switches, 0xFF);
    }

    #[test]
    fn test_config_file_nes_palette_valid() {
        let mut config = Config::default();
        config.apply_config_value("nes-palette", "smooth").unwrap();
        assert_eq!(config.nes.palette, NesPalette::Smooth);
    }

    #[test]
    fn test_config_file_nes_palette_is_case_insensitive() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-palette", "Composite-Direct")
            .unwrap();
        assert_eq!(config.nes.palette, NesPalette::CompositeDirect);
    }

    #[test]
    fn test_config_file_nes_palette_invalid_keeps_default() {
        let mut config = Config::default();
        config.nes.palette = NesPalette::Classic;
        // Invalid value should not error from the config-file path; it keeps the current value.
        config.apply_config_value("nes-palette", "bogus").unwrap();
        assert_eq!(config.nes.palette, NesPalette::Classic);
    }

    #[test]
    fn test_config_file_nes_palette_mesen() {
        let mut config = Config::default();
        config.apply_config_value("nes-palette", "Mesen").unwrap();
        assert_eq!(config.nes.palette, NesPalette::Mesen);
    }

    #[test]
    fn test_config_file_nes_palette_warning_names_every_preset() {
        assert_eq!(
            super::invalid_nes_palette_warning("bogus", NesPalette::Default),
            "Warning: invalid value 'bogus' for 'nes-palette'; keeping default ('default'). \
             Valid values: default, nesdev, smooth, classic, composite-direct, mesen"
        );
    }
}
