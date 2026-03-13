#[cfg(test)]
mod tests {
    /////////////////////////////////////
    // Input
    /////////////////////////////////////

    // TODO integrate PaddleTest3 ROM suite

    // TODO integrate ruder-0.03 ROM suite

    // TODO integrate spadtest-nes-0.01 ROM suite

    // TODO integrate vaus-test-0.02 ROM suite

    /////////////////////////////////////
    // Allpads harness smoke test
    /////////////////////////////////////

    use crate::input::Button;
    use crate::integration_tests::allpads_harness::tests::{
        ControllerConfig, run_allpads, script_enter_test, script_enter_test_and_press,
    };

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

    /// Helper: returns the OAM attribute byte for a given sprite index.
    fn oam_sprite_attr(oam: &[u8], sprite: usize) -> u8 {
        oam[sprite * 4 + 2]
    }

    /// Assert that exactly one sprite (the expected one) has attribute 0x01,
    /// and all other button sprites (0..8) have attribute 0x00.
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

    /// After probing, the controller display screen should identify a
    /// NES controller (shown as "NES DOGBONE" / "CONTROLLER") on port 1.
    #[test]
    fn allpads_joypad_probe_identifies_nes_controller() {
        let config = ControllerConfig::joypad_port1();
        let result = run_allpads(&config, &[], 300, 0);
        let cap = &result.captures[0];
        assert!(
            cap.nametable_text.contains("NES DOGBONE"),
            "Controller display should show 'NES DOGBONE', got:\n{}",
            cap.nametable_text
        );
        assert!(
            cap.nametable_text.contains("CONTROLLER"),
            "Controller display should show 'CONTROLLER', got:\n{}",
            cap.nametable_text
        );
    }

    /// Pressing A on the controller display enters the NES controller test screen
    /// and the A button's sprite indicator changes attribute to 0x01.
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

    /// Pressing Start produces an observable OAM sprite change.
    #[test]
    fn allpads_joypad_start_press_highlights_sprite() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test_and_press(Button::Start);
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert_only_sprite_highlighted(&cap.oam_data, 3);
    }

    /// Pressing Right produces an observable OAM sprite change.
    #[test]
    fn allpads_joypad_right_press_highlights_sprite() {
        let config = ControllerConfig::joypad_port1();
        let script = script_enter_test_and_press(Button::Right);
        let result = run_allpads(&config, &script, 420, 0);
        let cap = &result.captures[0];
        assert_only_sprite_highlighted(&cap.oam_data, 7);
    }

    /// The joypad scenario produces identical results across multiple runs
    /// (deterministic execution).
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
}
