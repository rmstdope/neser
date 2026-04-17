#[cfg(test)]
mod tests {
    use crate::nes::console::Config;
    use crate::nes::input::{Button, SnesButton};
    use crate::nes::integration_tests::romtest_harness::tests::{
        ControllerConfig, InputAction, RomTestResult, ScriptEntry, run_rom_with_script,
    };
    use crate::setup_rom_console_test;

    const ALLPADS_ROM_PATH: &str = "roms/nes/automated_tests/allpads-r9/allpads218.nes";

    /// Run `allpads.nes` with the given controller configuration and frame script.
    fn run_allpads(
        controller_config: &ControllerConfig,
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
    ) -> RomTestResult {
        let config = controller_config.to_config();
        run_allpads_with_config(&config, script, total_frames, capture_interval)
    }

    /// Run `allpads.nes` with a fully specified config and frame script.
    fn run_allpads_with_config(
        config: &Config,
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
    ) -> RomTestResult {
        run_rom_with_script(
            ALLPADS_ROM_PATH,
            config,
            script,
            total_frames,
            capture_interval,
            |b| {
                // allpads218.nes tiles are ASCII - 0x20 (tile 0x21 = 'A', tile 0x10 = '0')
                let ascii = b.wrapping_add(0x20);
                if (0x20..=0x7E).contains(&ascii) {
                    ascii as char
                } else {
                    ' '
                }
            },
        )
    }

    /// Build a script that enters the controller test (A press+release at frames
    /// 300/305) then presses the given button at frame 400.
    fn script_enter_test_and_press(button: Button) -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: false,
                }],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::Button {
                    port: 1,
                    button,
                    pressed: true,
                }],
            },
        ]
    }

    /// Build a script that enters the controller test (A press+release at frames
    /// 300/305) without pressing any additional button.
    fn script_enter_test() -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: false,
                }],
            },
        ]
    }

    /////////////////////////////////////
    // Allpads harness smoke test
    /////////////////////////////////////

    #[test]
    fn allpads_harness_smoke_test() {
        let config = ControllerConfig::joypad_port1();
        let result = run_allpads(&config, &[], 60, 0);
        assert_eq!(
            result.captures.len(),
            1,
            "Should capture nametable at final frame"
        );
        assert!(
            !result.captures[0].nametable_text.is_empty(),
            "Nametable text should not be empty after 60 frames"
        );
    }

    /////////////////////////////////////
    // Allpads Joypad scenario (#1555)
    /////////////////////////////////////

    fn oam_sprite_attr(oam: &[u8], sprite: usize) -> u8 {
        oam[sprite * 4 + 2]
    }

    fn assert_only_sprite_highlighted(oam: &[u8], expected_sprite: usize) {
        for sprite in 0..8 {
            let attr = oam_sprite_attr(oam, sprite);
            if sprite == expected_sprite {
                assert_eq!(
                    attr, 0x01,
                    "Sprite {} should be highlighted (attr 0x01), got 0x{:02X}",
                    sprite, attr
                );
            } else {
                assert_eq!(
                    attr, 0x00,
                    "Sprite {} should NOT be highlighted (attr 0x00), got 0x{:02X}",
                    sprite, attr
                );
            }
        }
    }

    fn parse_metric_value(nametable_text: &str, label: &str) -> Option<u32> {
        fn parse_token_number(token: &str) -> Option<u32> {
            if let Ok(value) = token.parse::<u32>() {
                return Some(value);
            }
            let digits: String = token
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u32>().ok()
            }
        }

        let lines: Vec<&str> = nametable_text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains(label) {
                continue;
            }

            // allpads may render metric values on the same line as the label or
            // on one of the immediately following lines (e.g. "PULLTIME" then
            // "Y 19").
            for candidate in lines.iter().skip(idx).take(3) {
                for token in candidate.split_whitespace().rev() {
                    if let Some(value) = parse_token_number(token) {
                        return Some(value);
                    }
                }
            }
        }
        None
    }

    fn parse_signed_numbers(line: &str) -> Vec<i32> {
        let mut result = Vec::new();
        let mut current = String::new();

        for ch in line.chars() {
            if ch.is_ascii_digit() || ((ch == '-' || ch == '+') && current.is_empty()) {
                current.push(ch);
            } else if !current.is_empty() {
                if let Ok(value) = current.parse::<i32>() {
                    result.push(value);
                }
                current.clear();
            }
        }

        if !current.is_empty()
            && let Ok(value) = current.parse::<i32>()
        {
            result.push(value);
        }

        result
    }

    fn parse_metric_signed_value(nametable_text: &str, label: &str) -> Option<i32> {
        let lines: Vec<&str> = nametable_text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains(label) {
                continue;
            }

            for candidate in lines.iter().skip(idx).take(3) {
                let numbers = parse_signed_numbers(candidate);
                if let Some(value) = numbers.first() {
                    return Some(*value);
                }
            }
        }
        None
    }

    fn parse_metric_signed_pair(nametable_text: &str, label: &str) -> Option<(i32, i32)> {
        let lines: Vec<&str> = nametable_text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains(label) {
                continue;
            }

            for candidate in lines.iter().skip(idx).take(3) {
                let numbers = parse_signed_numbers(candidate);
                if numbers.len() >= 2 {
                    return Some((numbers[0], numbers[1]));
                }
            }
        }
        None
    }

    #[test]
    fn allpads_joypad_probe_identifies_nes_controller() {
        let config = ControllerConfig::joypad_port1();
        let result = run_allpads(&config, &[], 300, 0);
        let cap = &result.captures[0];
        // NES-001 controller detection: bits 1-2 are grounded (driven to 0),
        // so allpads classifies it as "NES CONTROLLER" (not DOGBONE which is NES-101).
        assert!(
            cap.nametable_text.contains("NES"),
            "Controller display should show 'NES', got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("CONTROLLER"),
            "Controller display should show 'CONTROLLER', got:\n{}",
            cap.nametable_text
        );
    }

    /// Issue #1567: allpads console probe should identify NES-001 via open bus
    /// behavior. The probe computes min3F16 XOR min4016 and matches against
    /// known signatures. On NES-001, only bits 5-7 of $4016 are open bus,
    /// so the XOR signature is $E0, mapping to "NES-001".
    #[test]
    fn allpads_probe_identifies_console_model() {
        let config = ControllerConfig::joypad_port1();
        // Capture at frame 120 (title/probe screen) where model name is displayed
        let result = run_allpads(&config, &[], 120, 0);
        let cap = &result.captures[0];
        assert!(
            !cap.nametable_text.contains("Unknown console"),
            "Console probe should NOT show 'Unknown console', got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("NES-001"),
            "Console probe should identify as 'NES-001', got:\n{}",
            cap.nametable_text
        );
    }

    /// Issue #1601: allpads console probe should identify HVC-001 (Famicom)
    /// when hardware mode is set to Famicom. On Famicom, bits 3-4 of $4016
    /// are open bus (not grounded), so allpads detects HVC-001.
    #[test]
    fn allpads_probe_identifies_famicom_model() {
        use crate::nes::console::{Config, HardwareMode, RamInitMode};
        use crate::platform::config::FrontendConfig;
        let mut config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        config.nes.hardware_mode = HardwareMode::Famicom;
        config.nes.hardware_mode_explicit = true;
        config.nes.controller_port1_explicit = true;
        config.nes.controller_port2_explicit = true;
        let result = run_allpads_with_config(&config, &[], 120, 0);
        let cap = &result.captures[0];
        assert!(
            !cap.nametable_text.contains("Unknown console"),
            "Console probe should NOT show 'Unknown console', got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("FAMILY COMPUTER"),
            "Console probe should identify as 'FAMILY COMPUTER' (Famicom), got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_joypad_a_press_enters_test_and_highlights() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test_and_press(Button::A);
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert!(
            cap.nametable_text.contains("NES CONTROLLER"),
            "Should show NES controller test screen, got:\n{}",
            cap.nametable_text
        );
        assert_only_sprite_highlighted(&cap.oam_data, 0);
    }

    #[test]
    fn allpads_joypad_start_press_highlights_sprite() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test_and_press(Button::Start);
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert_only_sprite_highlighted(&cap.oam_data, 3);
    }

    #[test]
    fn allpads_joypad_right_press_highlights_sprite() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test_and_press(Button::Right);
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert_only_sprite_highlighted(&cap.oam_data, 7);
    }

    #[test]
    fn allpads_joypad_scenario_is_deterministic() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test();
        let result1 = run_allpads(&config, &script, 350, 0);
        let result2 = run_allpads(&config, &script, 350, 0);
        assert_eq!(
            result1.captures[0].nametable_raw, result2.captures[0].nametable_raw,
            "Nametable should be identical across runs"
        );
        assert_eq!(
            result1.captures[0].oam_data, result2.captures[0].oam_data,
            "OAM data should be identical across runs"
        );
    }

    /////////////////////////////////////
    // Allpads SNES adapter scenario (#1577)
    /////////////////////////////////////

    fn script_enter_snes_test_and_press(button: Button) -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: false,
                }],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::Button {
                    port: 1,
                    button,
                    pressed: true,
                }],
            },
        ]
    }

    fn script_enter_snes_test_and_press_snes(button: SnesButton) -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: false,
                }],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::SnesButton {
                    port: 1,
                    button,
                    pressed: true,
                }],
            },
        ]
    }

    fn snes_button_attrs(oam: &[u8]) -> Vec<u8> {
        (0..12).map(|sprite| oam_sprite_attr(oam, sprite)).collect()
    }

    fn highlighted_snes_button_indices(attrs: &[u8]) -> Vec<usize> {
        attrs
            .iter()
            .enumerate()
            .filter_map(|(idx, attr)| if *attr == 0x01 { Some(idx) } else { None })
            .collect()
    }

    #[test]
    fn allpads_snes_adapter_probe_identifies_super_nes_controller() {
        let config = ControllerConfig::snes_controller_port1();
        let result = run_allpads(&config, &[], 300, 0);
        let cap = &result.captures[0];
        let text = cap.nametable_text.to_ascii_uppercase();

        assert!(
            text.contains("SUPER NES"),
            "Probe screen should show 'Super NES', got:\n{}",
            cap.nametable_text
        );
        assert!(
            text.contains("CONTROLLER"),
            "Probe screen should show 'Controller', got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_snes_adapter_b_press_enters_test_screen() {
        let config = ControllerConfig::snes_controller_port1();
        let script = script_enter_snes_test_and_press(Button::B);
        let result = run_allpads(&config, &script, 430, 0);
        let cap = &result.captures[0];
        let text = cap.nametable_text.to_ascii_uppercase();

        assert!(
            text.contains("SUPER NES CONTROLLER"),
            "SNES test screen should show title, got:\n{}",
            cap.nametable_text
        );
        assert!(
            text.contains("RESET: EXIT"),
            "SNES test screen should be active after pressing B, got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_snes_adapter_scripted_input_changes_button_highlight() {
        let config = ControllerConfig::snes_controller_port1();

        let baseline_script = vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::B,
                    pressed: false,
                }],
            },
        ];
        let baseline = run_allpads(&config, &baseline_script, 430, 0);

        let active_script = script_enter_snes_test_and_press(Button::Right);
        let active = run_allpads(&config, &active_script, 430, 0);

        let baseline_attrs = snes_button_attrs(&baseline.captures[0].oam_data);
        let active_attrs = snes_button_attrs(&active.captures[0].oam_data);

        assert_ne!(
            baseline_attrs, active_attrs,
            "Scripted SNES input should change highlighted button sprite attrs"
        );
        assert!(
            active_attrs.contains(&0x01),
            "At least one SNES button sprite should be highlighted after input, attrs={:?}",
            active_attrs
        );
    }

    #[test]
    fn allpads_snes_adapter_each_button_is_one_hot_highlighted() {
        let config = ControllerConfig::snes_controller_port1();

        let baseline = run_allpads(
            &config,
            &script_enter_snes_test_and_press(Button::B),
            395,
            0,
        );
        let baseline_attrs = snes_button_attrs(&baseline.captures[0].oam_data);
        let baseline_highlighted = highlighted_snes_button_indices(&baseline_attrs);
        assert!(
            baseline_highlighted.is_empty(),
            "No SNES buttons should be highlighted without held input, got {:?} (attrs={:?})",
            baseline_highlighted,
            baseline_attrs
        );

        let cases: &[(SnesButton, usize, &str)] = &[
            (SnesButton::B, 0, "B"),
            (SnesButton::Y, 1, "Y"),
            (SnesButton::Select, 2, "Select"),
            (SnesButton::Start, 3, "Start"),
            (SnesButton::Up, 4, "Up"),
            (SnesButton::Down, 5, "Down"),
            (SnesButton::Left, 6, "Left"),
            (SnesButton::Right, 7, "Right"),
            (SnesButton::A, 8, "A"),
            (SnesButton::X, 9, "X"),
            (SnesButton::L, 10, "L"),
            (SnesButton::R, 11, "R"),
        ];

        for (button, expected_index, label) in cases {
            let script = script_enter_snes_test_and_press_snes(*button);
            let result = run_allpads(&config, &script, 430, 0);
            let attrs = snes_button_attrs(&result.captures[0].oam_data);
            let highlighted = highlighted_snes_button_indices(&attrs);

            assert_eq!(
                highlighted,
                vec![*expected_index],
                "Pressing {} should highlight only index {}. highlighted={:?}, attrs={:?}",
                label,
                expected_index,
                highlighted,
                attrs
            );
        }
    }

    #[test]
    fn allpads_snes_adapter_scenario_is_deterministic() {
        let config = ControllerConfig::snes_controller_port1();
        let script = script_enter_snes_test_and_press(Button::Right);
        let result1 = run_allpads(&config, &script, 430, 0);
        let result2 = run_allpads(&config, &script, 430, 0);

        assert_eq!(
            result1.captures[0].nametable_raw, result2.captures[0].nametable_raw,
            "Nametable should be identical across repeated SNES adapter runs"
        );
        assert_eq!(
            result1.captures[0].oam_data, result2.captures[0].oam_data,
            "OAM data should be identical across repeated SNES adapter runs"
        );
    }

    /////////////////////////////////////
    // Allpads SNES mouse adapter scenario (#1578)
    /////////////////////////////////////

    #[test]
    fn allpads_snes_mouse_adapter_probe_identifies_super_nes_mouse() {
        let config = ControllerConfig::snes_mouse_port1();
        let script = vec![
            ScriptEntry {
                frame: 1,
                actions: vec![InputAction::MouseX(96), InputAction::MouseY(96)],
            },
            ScriptEntry {
                frame: 2,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 6,
                actions: vec![InputAction::MouseButton(false)],
            },
        ];

        let result = run_allpads(&config, &script, 340, 0);
        let cap = &result.captures[0];
        let text = cap.nametable_text.to_ascii_uppercase();

        assert!(
            text.contains("SUPER NES") && text.contains("MOUSE"),
            "Probe screen should identify Super NES Mouse via adapter, got:\n{}",
            cap.nametable_text
        );
    }

    fn script_enter_snes_mouse_test() -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 1,
                actions: vec![InputAction::MouseX(96), InputAction::MouseY(96)],
            },
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::MouseButton(false)],
            },
        ]
    }

    #[test]
    fn allpads_snes_mouse_adapter_left_click_enters_mouse_test_screen() {
        let config = ControllerConfig::snes_mouse_port1();
        let script = script_enter_snes_mouse_test();
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        let text = cap.nametable_text.to_ascii_uppercase();

        assert!(
            text.contains("MOUSE ON") && text.contains("POSITION"),
            "Mouse test screen should be active after left click, got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_snes_mouse_adapter_right_click_changes_button_state() {
        let config = ControllerConfig::snes_mouse_port1();

        let baseline_script = script_enter_snes_mouse_test();
        let baseline = run_allpads(&config, &baseline_script, 430, 0);

        let mut right_click_script = script_enter_snes_mouse_test();
        right_click_script.push(ScriptEntry {
            frame: 400,
            actions: vec![InputAction::MouseRightButton(true)],
        });
        let active = run_allpads(&config, &right_click_script, 430, 0);

        assert_ne!(
            baseline.captures[0].oam_data, active.captures[0].oam_data,
            "Right mouse button should affect SNES mouse test-screen state"
        );
    }

    #[test]
    fn allpads_snes_mouse_adapter_scripted_movement_updates_position_metrics() {
        let config = ControllerConfig::snes_mouse_port1();
        let mut script = script_enter_snes_mouse_test();
        script.push(ScriptEntry {
            frame: 360,
            actions: vec![InputAction::MouseX(128), InputAction::MouseY(100)],
        });
        script.push(ScriptEntry {
            frame: 420,
            actions: vec![InputAction::MouseX(40), InputAction::MouseY(140)],
        });

        let result = run_allpads(&config, &script, 460, 20);
        let cap_before_move = result
            .captures
            .iter()
            .find(|capture| capture.frame == 380)
            .expect("Expected capture at frame 380");
        let cap_after_move = result
            .captures
            .iter()
            .find(|capture| capture.frame == 440)
            .expect("Expected capture at frame 440");

        let pos_before = parse_metric_signed_pair(&cap_before_move.nametable_text, "POSITION")
            .unwrap_or_else(|| {
                panic!(
                    "Expected POSITION metric before movement, got:\n{}",
                    cap_before_move.nametable_text
                )
            });
        let pos_after = parse_metric_signed_pair(&cap_after_move.nametable_text, "POSITION")
            .unwrap_or_else(|| {
                panic!(
                    "Expected POSITION metric after movement, got:\n{}",
                    cap_after_move.nametable_text
                )
            });

        assert_ne!(
            pos_before, pos_after,
            "Scripted SNES mouse movement should update POSITION (before={:?}, after={:?})",
            pos_before, pos_after
        );
    }

    #[test]
    fn allpads_snes_mouse_adapter_y_sweep_reaches_wide_range() {
        let config = ControllerConfig::snes_mouse_port1();
        let mut script = script_enter_snes_mouse_test();
        script.push(ScriptEntry {
            frame: 360,
            actions: vec![InputAction::MouseX(96), InputAction::MouseY(0)],
        });
        script.push(ScriptEntry {
            frame: 420,
            actions: vec![InputAction::MouseX(96), InputAction::MouseY(255)],
        });

        let result = run_allpads(&config, &script, 460, 20);
        let cap_before_move = result
            .captures
            .iter()
            .find(|capture| capture.frame == 380)
            .expect("Expected capture at frame 380");
        let cap_after_move = result
            .captures
            .iter()
            .find(|capture| capture.frame == 440)
            .expect("Expected capture at frame 440");

        let (x_before, y_before) =
            parse_metric_signed_pair(&cap_before_move.nametable_text, "POSITION").unwrap_or_else(
                || {
                    panic!(
                        "Expected POSITION metric before Y movement, got:\n{}",
                        cap_before_move.nametable_text
                    )
                },
            );
        let (x_after, y_after) =
            parse_metric_signed_pair(&cap_after_move.nametable_text, "POSITION").unwrap_or_else(
                || {
                    panic!(
                        "Expected POSITION metric after Y movement, got:\n{}",
                        cap_after_move.nametable_text
                    )
                },
            );

        assert_eq!(
            x_before, x_after,
            "X should remain stable during Y-only movement (before={}, after={})",
            x_before, x_after
        );
        let y_span = (y_after - y_before).abs();
        assert!(
            y_span >= 150,
            "Y sweep should span most of the screen (>=150), got {} (before={}, after={})",
            y_span,
            y_before,
            y_after
        );
    }

    #[test]
    fn allpads_snes_mouse_adapter_x_sweep_reaches_wide_range() {
        let config = ControllerConfig::snes_mouse_port1();
        let mut script = script_enter_snes_mouse_test();
        script.push(ScriptEntry {
            frame: 360,
            actions: vec![InputAction::MouseX(0), InputAction::MouseY(96)],
        });
        script.push(ScriptEntry {
            frame: 420,
            actions: vec![InputAction::MouseX(255), InputAction::MouseY(96)],
        });

        let result = run_allpads(&config, &script, 460, 20);
        let cap_left = result
            .captures
            .iter()
            .find(|capture| capture.frame == 380)
            .expect("Expected capture at frame 380");
        let cap_right = result
            .captures
            .iter()
            .find(|capture| capture.frame == 440)
            .expect("Expected capture at frame 440");

        let (x_left, y_left) = parse_metric_signed_pair(&cap_left.nametable_text, "POSITION")
            .unwrap_or_else(|| {
                panic!(
                    "Expected POSITION metric at left edge, got:\n{}",
                    cap_left.nametable_text
                )
            });
        let (x_right, y_right) = parse_metric_signed_pair(&cap_right.nametable_text, "POSITION")
            .unwrap_or_else(|| {
                panic!(
                    "Expected POSITION metric at right edge, got:\n{}",
                    cap_right.nametable_text
                )
            });

        assert_eq!(
            y_left, y_right,
            "Y should remain stable during X-only sweep (left={}, right={})",
            y_left, y_right
        );
        let x_span = (x_right - x_left).abs();
        assert!(
            x_span >= 220,
            "X sweep should span most of the screen (>=220), got {} (left={}, right={})",
            x_span,
            x_left,
            x_right
        );
    }

    #[test]
    fn allpads_snes_mouse_adapter_scenario_is_deterministic() {
        let config = ControllerConfig::snes_mouse_port1();
        let mut script = script_enter_snes_mouse_test();
        script.push(ScriptEntry {
            frame: 360,
            actions: vec![InputAction::MouseX(128), InputAction::MouseY(100)],
        });
        script.push(ScriptEntry {
            frame: 420,
            actions: vec![InputAction::MouseX(40), InputAction::MouseY(140)],
        });

        let result1 = run_allpads(&config, &script, 460, 0);
        let result2 = run_allpads(&config, &script, 460, 0);

        assert_eq!(
            result1.captures[0].nametable_raw, result2.captures[0].nametable_raw,
            "Nametable should be identical across repeated SNES mouse adapter runs"
        );
        assert_eq!(
            result1.captures[0].oam_data, result2.captures[0].oam_data,
            "OAM data should be identical across repeated SNES mouse adapter runs"
        );
    }

    /////////////////////////////////////
    // Allpads Zapper scenario (#1556)
    /////////////////////////////////////

    #[test]
    fn allpads_zapper_probe_identifies_zapper() {
        let config = ControllerConfig::zapper();
        let result = run_allpads(&config, &[], 300, 0);
        let cap = &result.captures[0];

        assert!(
            cap.nametable_text.contains("ZAPPER"),
            "Controller display should identify Zapper, got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_zapper_trigger_hold_updates_metrics_and_pulltime() {
        let config = ControllerConfig::zapper();
        let script = vec![
            ScriptEntry {
                frame: 360,
                actions: vec![InputAction::MouseX(80), InputAction::MouseY(96)],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 450,
                actions: vec![InputAction::MouseButton(false)],
            },
        ];

        let result = run_allpads(&config, &script, 460, 20);

        let during_hold_early = result
            .captures
            .iter()
            .find(|capture| capture.frame == 420)
            .expect("Expected capture at frame 420");
        let during_hold_late = result
            .captures
            .iter()
            .find(|capture| capture.frame == 440)
            .expect("Expected capture at frame 440");

        assert!(
            during_hold_early.nametable_text.contains("LIGHT Y"),
            "Zapper metrics should include 'LIGHT Y', got:\n{}",
            during_hold_early.nametable_text
        );
        assert!(
            during_hold_early.nametable_text.contains("HEIGHT"),
            "Zapper metrics should include 'HEIGHT', got:\n{}",
            during_hold_early.nametable_text
        );
        assert!(
            during_hold_early.nametable_text.contains("PULLTIME"),
            "Zapper metrics should include 'PULLTIME', got:\n{}",
            during_hold_early.nametable_text
        );

        let pull_time_early = parse_metric_value(&during_hold_early.nametable_text, "PULLTIME")
            .unwrap_or_else(|| {
                panic!(
                    "Expected PullTime metric at frame 420, got:\n{}",
                    during_hold_early.nametable_text
                )
            });
        let pull_time_late = parse_metric_value(&during_hold_late.nametable_text, "PULLTIME")
            .unwrap_or_else(|| {
                panic!(
                    "Expected PullTime metric at frame 440, got:\n{}",
                    during_hold_late.nametable_text
                )
            });

        assert!(
            pull_time_late > pull_time_early,
            "PullTime should increase while trigger is held (early={}, late={})",
            pull_time_early,
            pull_time_late
        );
    }

    #[test]
    fn allpads_zapper_y_sweep_reports_light_metrics() {
        let config = ControllerConfig::zapper();
        let sample_ys: [u8; 9] = [0, 1, 2, 118, 119, 120, 237, 238, 239];
        let mut script = vec![
            ScriptEntry {
                frame: 360,
                actions: vec![InputAction::MouseX(80), InputAction::MouseY(0)],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::MouseButton(true)],
            },
        ];

        let mut frame = 420;
        for y in sample_ys {
            script.push(ScriptEntry {
                frame,
                actions: vec![InputAction::MouseY(y)],
            });
            frame += 20;
        }

        let total_frames = frame + 20;
        let result = run_allpads(&config, &script, total_frames, 20);

        let mut sample_frame = 440;
        let mut observed: Vec<(u8, u32, u32)> = Vec::new();
        for y in sample_ys {
            let cap = result
                .captures
                .iter()
                .find(|capture| capture.frame == sample_frame)
                .unwrap_or_else(|| panic!("Expected capture at frame {}", sample_frame));
            let light_y = parse_metric_value(&cap.nametable_text, "LIGHT Y").unwrap_or_else(|| {
                panic!(
                    "Expected LIGHT Y metric at frame {}, got:\n{}",
                    sample_frame, cap.nametable_text
                )
            });
            let height = parse_metric_value(&cap.nametable_text, "HEIGHT").unwrap_or_else(|| {
                panic!(
                    "Expected HEIGHT metric at frame {}, got:\n{}",
                    sample_frame, cap.nametable_text
                )
            });
            println!("allpads y={} => LIGHT Y={}, HEIGHT={}", y, light_y, height);
            observed.push((y, light_y, height));
            sample_frame += 20;
        }

        for (y, light_y, height) in observed.iter().copied() {
            match y {
                0 | 1 | 2 | 237 | 238 | 239 => {
                    assert_eq!(
                        (light_y, height),
                        (192, 0),
                        "Top/bottom edge should be off-window sentinel at y={y}"
                    );
                }
                118..=120 => {
                    assert!(
                        light_y < 192,
                        "Mid-screen sample should detect light (LIGHT Y < 192) at y={y}, got {light_y}"
                    );
                    assert!(
                        (1..=6).contains(&height),
                        "Mid-screen detected light height should be short (1..=6) at y={y}, got {height}"
                    );
                }
                _ => unreachable!("Unexpected y sample"),
            }
        }
    }

    /////////////////////////////////////
    // Allpads Arkanoid scenario (#1557)
    /////////////////////////////////////

    #[test]
    fn allpads_arkanoid_probe_identifies_controller() {
        let config = ControllerConfig::arkanoid();
        let result = run_allpads(&config, &[], 300, 0);
        let cap = &result.captures[0];

        assert!(
            cap.nametable_text.contains("NES ARKANOID"),
            "Controller display should identify NES Arkanoid, got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("PRESS FIRE"),
            "Probe screen should prompt fire button, got:\n{}",
            cap.nametable_text
        );
    }

    #[test]
    fn allpads_arkanoid_movement_updates_displacement_range_velocity_and_acceleration() {
        let config = ControllerConfig::arkanoid();
        let script = vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::MouseButton(false)],
            },
            ScriptEntry {
                frame: 360,
                actions: vec![InputAction::MouseX(40)],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::MouseX(120)],
            },
            ScriptEntry {
                frame: 440,
                actions: vec![InputAction::MouseX(200)],
            },
            ScriptEntry {
                frame: 480,
                actions: vec![InputAction::MouseX(60)],
            },
        ];

        let result = run_allpads(&config, &script, 520, 20);

        let cap_a = result
            .captures
            .iter()
            .find(|capture| capture.frame == 420)
            .expect("Expected capture at frame 420");
        let cap_b = result
            .captures
            .iter()
            .find(|capture| capture.frame == 500)
            .expect("Expected capture at frame 500");

        for cap in [cap_a, cap_b] {
            assert!(
                cap.nametable_text.contains("DISPLACEMENT"),
                "Arkanoid screen should include Displacement, got:\n{}",
                cap.nametable_text
            );
            assert!(
                cap.nametable_text.contains("RANGE"),
                "Arkanoid screen should include Range, got:\n{}",
                cap.nametable_text
            );
            assert!(
                cap.nametable_text.contains("VELOCITY"),
                "Arkanoid screen should include Velocity, got:\n{}",
                cap.nametable_text
            );
            assert!(
                cap.nametable_text.contains("ACCELERATION"),
                "Arkanoid screen should include Acceleration, got:\n{}",
                cap.nametable_text
            );
        }

        let displacement_a = parse_metric_signed_value(&cap_a.nametable_text, "DISPLACEMENT")
            .expect("Expected Displacement value at frame 420");
        let displacement_b = parse_metric_signed_value(&cap_b.nametable_text, "DISPLACEMENT")
            .expect("Expected Displacement value at frame 500");

        let (range_min_a, range_max_a) = parse_metric_signed_pair(&cap_a.nametable_text, "RANGE")
            .expect("Expected Range min/max at frame 420");
        let (range_min_b, range_max_b) = parse_metric_signed_pair(&cap_b.nametable_text, "RANGE")
            .expect("Expected Range min/max at frame 500");

        parse_metric_signed_value(&cap_a.nametable_text, "VELOCITY")
            .expect("Expected Velocity value at frame 420");
        parse_metric_signed_value(&cap_b.nametable_text, "VELOCITY")
            .expect("Expected Velocity value at frame 500");
        parse_metric_signed_value(&cap_a.nametable_text, "ACCELERATION")
            .expect("Expected Acceleration value at frame 420");
        parse_metric_signed_value(&cap_b.nametable_text, "ACCELERATION")
            .expect("Expected Acceleration value at frame 500");

        assert_ne!(
            displacement_a, displacement_b,
            "Displacement should react to scripted movement"
        );
        assert!(
            range_max_a >= range_min_a && range_max_b >= range_min_b,
            "Range min/max ordering should remain valid (a: {}-{}, b: {}-{})",
            range_min_a,
            range_max_a,
            range_min_b,
            range_max_b
        );
        assert!(
            range_max_b - range_min_b >= range_max_a - range_min_a,
            "Range span should not shrink after wider sweep (a span {}, b span {})",
            range_max_a - range_min_a,
            range_max_b - range_min_b
        );
    }

    #[test]
    fn allpads_arkanoid_fire_button_produces_observable_response() {
        let config = ControllerConfig::arkanoid();
        let no_fire_result = run_allpads(&config, &[], 340, 0);
        let no_fire_cap = &no_fire_result.captures[0];
        assert!(
            no_fire_cap.nametable_text.contains("PRESS FIRE"),
            "Without fire input, probe screen should remain visible, got:\n{}",
            no_fire_cap.nametable_text
        );

        let script = vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::MouseButton(false)],
            },
            ScriptEntry {
                frame: 420,
                actions: vec![InputAction::MouseButton(false)],
            },
        ];

        let fire_result = run_allpads(&config, &script, 420, 0);
        let fire_cap = &fire_result.captures[0];

        assert!(
            fire_cap.nametable_text.contains("DISPLACEMENT"),
            "Fire input should enter Arkanoid paddle test screen, got:\n{}",
            fire_cap.nametable_text
        );
    }

    /////////////////////////////////////
    // Allpads Famicom controller scenario (#1574)
    /////////////////////////////////////

    /// Returns a Famicom-mode config with two explicitly configured joypads so
    /// that allpads can detect them as Famicom hardwired controllers (FC 1P / FC 2P).
    fn famicom_two_joypads_config() -> crate::nes::console::Config {
        use crate::nes::console::{Config, HardwareMode, RamInitMode};
        use crate::nes::input::ControllerType;
        use crate::platform::config::FrontendConfig;
        let mut config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        config.nes.hardware_mode = HardwareMode::Famicom;
        config.nes.hardware_mode_explicit = true;
        config.nes.controller_port1 = ControllerType::Joypad;
        config.nes.controller_port2 = ControllerType::Joypad;
        config.nes.controller_port1_explicit = true;
        config.nes.controller_port2_explicit = true;
        config
    }

    /// Issue #1574: allpads probe should identify both Famicom hardwired controllers
    /// when hardware mode is set to Famicom with two standard joypads. allpads
    /// recognises the open-bus signature (bits 3-4 floating) and two D0 controllers,
    /// then replaces them with TYPE_FC_1P ("Famicom Controller") and TYPE_FC_2P
    /// ("Famicom Mic Controller").
    #[test]
    fn allpads_famicom_probe_identifies_famicom_controllers() {
        let config = famicom_two_joypads_config();
        let result = run_allpads_with_config(&config, &[], 300, 0);
        let cap = &result.captures[0];
        assert!(
            cap.nametable_text.contains("FAMICOM"),
            "Probe should identify Famicom 1P controller, got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("MIC"),
            "Probe should identify Famicom Mic (2P) controller, got:\n{}",
            cap.nametable_text
        );
    }

    /// Issue #1574: pressing A on port 1 should select the Famicom 1P hardwired
    /// controller, which maps to the standard NES controller test.
    #[test]
    fn allpads_famicom_1p_a_press_enters_nes_controller_test() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::A);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert!(
            cap.nametable_text.contains("NES CONTROLLER"),
            "Pressing A on 1P should enter NES controller test, got:\n{}",
            cap.nametable_text
        );
        assert_only_sprite_highlighted(&cap.oam_data, 0);
    }

    // NES serial bit order: A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7
    // Each maps to sprite index 0-7 on the NES CONTROLLER test screen.

    #[test]
    fn allpads_famicom_1p_b_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::B);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 1);
    }

    #[test]
    fn allpads_famicom_1p_select_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Select);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 2);
    }

    #[test]
    fn allpads_famicom_1p_start_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Start);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 3);
    }

    #[test]
    fn allpads_famicom_1p_up_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Up);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 4);
    }

    #[test]
    fn allpads_famicom_1p_down_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Down);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 5);
    }

    #[test]
    fn allpads_famicom_1p_left_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Left);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 6);
    }

    #[test]
    fn allpads_famicom_1p_right_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_test_and_press(Button::Right);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 7);
    }

    /// Issue #1574: pressing A on port 2 should select the Famicom 2P hardwired
    /// controller, which maps to the Famicom Mic controller test.
    #[test]
    fn allpads_famicom_2p_a_press_enters_mic_controller_test() {
        let config = famicom_two_joypads_config();
        let script = vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 2,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 2,
                    button: Button::A,
                    pressed: false,
                }],
            },
        ];
        let result = run_allpads_with_config(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert!(
            cap.nametable_text.contains("FAMICOM MIC CONTROLLER"),
            "Pressing A on 2P should enter Famicom Mic controller test, got:\n{}",
            cap.nametable_text
        );
    }

    /// Builds a script that enters the FC 2P (Mic) test by pressing A on port 2
    /// at frame 300, then presses `button` on port 2 at frame 400.
    ///
    /// Note: pressing Select or Start on FC 2P causes allpads to switch to the
    /// NES controller test screen, so those buttons are intentionally excluded.
    fn script_enter_famicom_2p_test_and_press(button: Button) -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 2,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 2,
                    button: Button::A,
                    pressed: false,
                }],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::Button {
                    port: 2,
                    button,
                    pressed: true,
                }],
            },
        ]
    }

    // FC 2P (Mic) controller: NES serial bit order A=0, B=1, Up=4, Down=5, Left=6, Right=7.
    // Select (sprite 2) and Start (sprite 3) are hidden off-screen on FC 2P and pressing
    // them causes allpads to switch screens, so they are not tested here.

    #[test]
    fn allpads_famicom_2p_a_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::A);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 0);
    }

    #[test]
    fn allpads_famicom_2p_b_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::B);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 1);
    }

    #[test]
    fn allpads_famicom_2p_up_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::Up);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 4);
    }

    #[test]
    fn allpads_famicom_2p_down_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::Down);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 5);
    }

    #[test]
    fn allpads_famicom_2p_left_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::Left);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 6);
    }

    #[test]
    fn allpads_famicom_2p_right_press_highlighted() {
        let config = famicom_two_joypads_config();
        let script = script_enter_famicom_2p_test_and_press(Button::Right);
        let result = run_allpads_with_config(&config, &script, 420, 0);
        assert_only_sprite_highlighted(&result.captures[0].oam_data, 7);
    }

    /// Issue #1574: Famicom controller probe results should be fully deterministic.
    #[test]
    fn allpads_famicom_controller_scenario_is_deterministic() {
        let config = famicom_two_joypads_config();
        let result1 = run_allpads_with_config(&config, &[], 300, 0);
        let result2 = run_allpads_with_config(&config, &[], 300, 0);
        assert_eq!(
            result1.captures[0].nametable_raw, result2.captures[0].nametable_raw,
            "Famicom controller probe should be deterministic"
        );
        assert_eq!(
            result1.captures[0].oam_data, result2.captures[0].oam_data,
            "Famicom controller OAM should be deterministic"
        );
    }

    setup_rom_console_test!(
        test_read_joy3_count_errors,
        "roms/nes/automated_tests/read_joy3/count_errors.nes",
        "CONFLICTS: 0/1000-"
    );

    setup_rom_console_test!(
        test_read_joy3_count_errors_fast,
        "roms/nes/automated_tests/read_joy3/count_errors_fast.nes",
        "ERRORS: 0/1000"
    );

    setup_rom_console_test!(
        test_read_joy3_test_buttons,
        "roms/nes/automated_tests/read_joy3/test_buttons.nes"
    );

    setup_rom_console_test!(
        test_read_joy3_thorough_test,
        "roms/nes/automated_tests/read_joy3/thorough_test.nes"
    );
}
