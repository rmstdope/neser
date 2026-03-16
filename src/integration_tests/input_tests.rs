#[cfg(test)]
mod tests {
    /////////////////////////////////////
    // Input
    /////////////////////////////////////

    // TODO integrate spadtest-nes-0.01 ROM suite

    // TODO integrate vaus-test-0.02 ROM suite

    use crate::input::{Button, ControllerType};
    use crate::integration_tests::romtest_harness::tests::{
        ControllerConfig, InputAction, RomTestResult, ScriptEntry, run_rom_with_script,
    };

    /////////////////////////////////////
    // ruder-0.03 Zapper test suite (#1607)
    /////////////////////////////////////

    const RUDER_ROM_PATH: &str = "roms/automated_tests/ruder-0.03/ruder.nes";

    fn run_ruder(
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
    ) -> RomTestResult {
        let config = ControllerConfig::zapper().to_config();
        run_rom_with_script(
            RUDER_ROM_PATH,
            &config,
            script,
            total_frames,
            capture_interval,
            |b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            },
        )
    }

    /// Build a script that navigates from the title screen to the menu
    /// and then enters a specific menu item by pressing Down `downs` times
    /// before pressing A.
    fn script_ruder_enter_menu_item(downs: u32) -> Vec<ScriptEntry> {
        let mut script = vec![
            // Wait for title screen, then press A to enter menu
            ScriptEntry {
                frame: 120,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 125,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: false,
                }],
            },
        ];
        // Navigate down in the menu
        let mut frame = 200;
        for _ in 0..downs {
            script.push(ScriptEntry {
                frame,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::Down,
                    pressed: true,
                }],
            });
            script.push(ScriptEntry {
                frame: frame + 5,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::Down,
                    pressed: false,
                }],
            });
            frame += 15;
        }
        // Press A to enter the selected test
        script.push(ScriptEntry {
            frame: frame + 10,
            actions: vec![InputAction::Button {
                port: 1,
                button: Button::A,
                pressed: true,
            }],
        });
        script.push(ScriptEntry {
            frame: frame + 15,
            actions: vec![InputAction::Button {
                port: 1,
                button: Button::A,
                pressed: false,
            }],
        });
        script
    }

    /// Parse a numeric value following `label` in ruder nametable text.
    /// Handles both "LABEL=NNN" format (Y tracking, X tracking) and
    /// "LABEL   NN" format (trigger test) by looking for `label` then
    /// skipping any trailing '=' and whitespace before the digits.
    ///
    /// To avoid ambiguous matches (e.g. searching for "Y" in "Y1="),
    /// this requires that the character immediately following `label`
    /// is either '=' or ASCII whitespace.
    fn parse_ruder_value(text: &str, label: &str) -> Option<u32> {
        for line in text.lines() {
            let mut search_start = 0;
            while let Some(rel_pos) = line[search_start..].find(label) {
                let pos = search_start + rel_pos;
                let after_label = &line[pos + label.len()..];
                // Require a token boundary: next char must be '=' or whitespace (if any).
                if let Some(next_char) = after_label.chars().next() {
                    if next_char != '=' && !next_char.is_ascii_whitespace() {
                        search_start = pos + label.len();
                        continue;
                    }
                }
                let after = after_label.strip_prefix('=').unwrap_or(after_label);
                let trimmed = after.trim_start();
                let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    return digits.parse().ok();
                }
                // No digits after this occurrence; continue searching later in the line.
                search_start = pos + label.len();
            }
        }
        None
    }

    /// Run a ruder tracking test: navigate to the specified menu item,
    /// position the zapper, and return the nametable text from a single
    /// end-of-run capture.
    fn run_ruder_tracking_test(
        menu_downs: u32,
        mouse_x: u8,
        mouse_y: u8,
        total_frames: u32,
    ) -> String {
        let mut script = script_ruder_enter_menu_item(menu_downs);
        script.push(ScriptEntry {
            frame: 300,
            actions: vec![InputAction::MouseX(mouse_x), InputAction::MouseY(mouse_y)],
        });
        let result = run_ruder(&script, total_frames, 0);
        result.captures[0].nametable_text.clone()
    }

    #[test]
    fn ruder_y_tracking_reports_correct_position() {
        // Y tracking is menu item 0 (no downs needed, default cursor position).
        // Test two different Zapper Y positions and verify the ROM reports
        // the correct Y and HT (height) values.

        // Position 1: MouseY=120 → expected Y=105, HT=3
        let text1 = run_ruder_tracking_test(0, 128, 120, 380);
        let y1 = parse_ruder_value(&text1, "Y").expect("Y value should be present");
        let ht1 = parse_ruder_value(&text1, "HT").expect("HT value should be present");
        assert_eq!(
            y1, 105,
            "Y tracking at MouseY=120: expected Y=105, got {y1}"
        );
        assert_eq!(ht1, 3, "Y tracking at MouseY=120: expected HT=3, got {ht1}");

        // Position 2: MouseY=80 → expected Y=65, HT=3
        let text2 = run_ruder_tracking_test(0, 128, 80, 380);
        let y2 = parse_ruder_value(&text2, "Y").expect("Y value should be present");
        let ht2 = parse_ruder_value(&text2, "HT").expect("HT value should be present");
        assert_eq!(y2, 65, "Y tracking at MouseY=80: expected Y=65, got {y2}");
        assert_eq!(ht2, 3, "Y tracking at MouseY=80: expected HT=3, got {ht2}");
    }

    #[test]
    fn ruder_x_tracking_reports_position() {
        // X tracking is menu item 2 (1 down in left column).
        // The xyon kernel estimates horizontal position in ~18-pixel bins (0–14).

        // Position 1: MouseX=128, MouseY=120 → expected X=8, Y=120
        let text1 = run_ruder_tracking_test(1, 128, 120, 400);
        let x1 = parse_ruder_value(&text1, "X").expect("X value should be present");
        let y1 = parse_ruder_value(&text1, "Y").expect("Y value should be present");
        assert_eq!(x1, 8, "X tracking at MouseX=128: expected X=8, got {x1}");
        assert_eq!(
            y1, 120,
            "X tracking at MouseY=120: expected Y=120, got {y1}"
        );

        // Position 2: MouseX=40, MouseY=120 → expected X=3, Y=120
        let text2 = run_ruder_tracking_test(1, 40, 120, 400);
        let x2 = parse_ruder_value(&text2, "X").expect("X value should be present");
        let y2 = parse_ruder_value(&text2, "Y").expect("Y value should be present");
        assert_eq!(x2, 3, "X tracking at MouseX=40: expected X=3, got {x2}");
        assert_eq!(
            y2, 120,
            "X tracking at MouseY=120: expected Y=120, got {y2}"
        );
    }

    #[test]
    fn ruder_trigger_test_counts_hold_time() {
        // Trigger test is menu item 6 (3 downs in left column)
        let mut script = script_ruder_enter_menu_item(3);
        // Start holding trigger at frame 300
        script.push(ScriptEntry {
            frame: 300,
            actions: vec![InputAction::MouseButton(true)],
        });
        // Capture every 10 frames to see counter progression
        let result = run_ruder(&script, 360, 10);

        // At frame 310 (10 frames into hold), HELD TIME should be 10
        let cap_310 = result
            .captures
            .iter()
            .find(|c| c.frame == 310)
            .expect("Expected capture at frame 310");
        let held_310 = parse_ruder_value(&cap_310.nametable_text, "HELD TIME")
            .expect("HELD TIME should be present in nametable text");
        assert_eq!(
            held_310, 10,
            "Trigger HELD TIME at frame 310 should be 10, got {held_310}"
        );

        // At frame 340 (40 frames into hold), HELD TIME should be 40
        let cap_340 = result
            .captures
            .iter()
            .find(|c| c.frame == 340)
            .expect("Expected capture at frame 340");
        let held_340 = parse_ruder_value(&cap_340.nametable_text, "HELD TIME")
            .expect("HELD TIME should be present in nametable text");
        assert_eq!(
            held_340, 40,
            "Trigger HELD TIME at frame 340 should be 40, got {held_340}"
        );

        // Verify monotonic increase
        assert!(
            held_340 > held_310,
            "HELD TIME should increase over time: 310={held_310}, 340={held_340}"
        );
    }

    /////////////////////////////////////
    // PaddleTest3 Arkanoid paddle (#1606)
    /////////////////////////////////////

    const PADDLETEST3_ROM_PATH: &str = "roms/manual_tests/PaddleTest3/PaddleTest.nes";

    fn run_paddletest3(
        controller_config: &ControllerConfig,
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
    ) -> RomTestResult {
        let config = controller_config.to_config();
        run_rom_with_script(
            PADDLETEST3_ROM_PATH,
            &config,
            script,
            total_frames,
            capture_interval,
            |b| {
                // PaddleTest3 nametable tiles: best-effort ASCII mapping
                let ascii = b.wrapping_add(0x20);
                if (0x20..=0x7E).contains(&ascii) {
                    ascii as char
                } else {
                    ' '
                }
            },
        )
    }

    /// Helper: extract sprite 0 fields from OAM data.
    /// OAM byte layout: [Y, tile, attributes, X] per sprite.
    fn sprite0(oam: &[u8]) -> (u8, u8, u8, u8) {
        (oam[0], oam[1], oam[2], oam[3])
    }

    #[test]
    fn paddletest3_no_controller_shows_not_connected() {
        // With a joypad on port 1 (no Arkanoid), the ROM reads $4016 bit 4
        // as all-zero → PaddleButtons = 0xFF → "no controller" branch.
        let config = ControllerConfig {
            port1: ControllerType::Joypad,
            port2: ControllerType::Joypad,
        };
        let result = run_paddletest3(&config, &[], 300, 0);
        let cap = &result.captures[0];
        let (y, _tile, _attr, _x) = sprite0(&cap.oam_data);

        // The ROM sets sprite Y to PaddleYWhenNotPluggedIn (0x37) when no
        // controller is detected — different from plugged-in Y (0x47).
        let plugged_in_y: u8 = 0x47;
        assert_ne!(
            y, plugged_in_y,
            "Without Arkanoid, sprite Y should NOT be the plugged-in value (0x{plugged_in_y:02X}), got 0x{y:02X}"
        );
    }

    #[test]
    fn paddletest3_position_tracking_moves_sprite() {
        // With Arkanoid on port 1, scripting different MouseX values should
        // produce measurably different sprite X positions.
        let config = ControllerConfig::arkanoid();
        let script = vec![
            ScriptEntry {
                frame: 360,
                actions: vec![InputAction::MouseX(40)],
            },
            ScriptEntry {
                frame: 440,
                actions: vec![InputAction::MouseX(200)],
            },
        ];

        let result = run_paddletest3(&config, &script, 500, 20);

        // Capture after MouseX(40) settled and after MouseX(200) settled
        let cap_low = result
            .captures
            .iter()
            .find(|c| c.frame == 380)
            .expect("Expected capture at frame 380");
        let cap_high = result
            .captures
            .iter()
            .find(|c| c.frame == 460)
            .expect("Expected capture at frame 460");

        let (_y_low, _tile_low, _attr_low, x_low) = sprite0(&cap_low.oam_data);
        let (_y_high, _tile_high, _attr_high, x_high) = sprite0(&cap_high.oam_data);

        assert_ne!(
            x_low, x_high,
            "Sprite X should differ between low and high paddle positions"
        );
        assert!(
            x_high > x_low,
            "Higher MouseX should produce larger sprite X: low=0x{x_low:02X}, high=0x{x_high:02X}"
        );
    }

    #[test]
    fn paddletest3_fire_button_changes_tile() {
        // The ROM shows tile 0x02 (green) when fire is pressed and
        // tile 0x01 (red) when not pressed.
        let config = ControllerConfig::arkanoid();
        let script = vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::MouseButton(true)],
            },
            ScriptEntry {
                frame: 380,
                actions: vec![InputAction::MouseButton(false)],
            },
        ];

        let result = run_paddletest3(&config, &script, 420, 20);

        let cap_fire = result
            .captures
            .iter()
            .find(|c| c.frame == 340)
            .expect("Expected capture at frame 340 (fire pressed)");
        let cap_no_fire = result
            .captures
            .iter()
            .find(|c| c.frame == 400)
            .expect("Expected capture at frame 400 (fire released)");

        let (_y_fire, tile_fire, _attr_fire, _x_fire) = sprite0(&cap_fire.oam_data);
        let (_y_no, tile_no, _attr_no, _x_no) = sprite0(&cap_no_fire.oam_data);

        assert_ne!(
            tile_fire, tile_no,
            "Sprite tile should change between fire pressed and released"
        );
    }
}
