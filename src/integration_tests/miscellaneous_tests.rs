#[cfg(test)]
mod tests {
    use crate::input::Button;
    use crate::integration_tests::allpads_harness::tests::{
        ControllerConfig, run_allpads, script_enter_test, script_enter_test_and_press,
    };
    use crate::setup_rom_console_test;

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

    setup_rom_console_test!(
        test_read_joy3_count_errors,
        "roms/automated_tests/read_joy3/count_errors.nes",
        "CONFLICTS: 0/1000-"
    );

    setup_rom_console_test!(
        test_read_joy3_count_errors_fast,
        "roms/automated_tests/read_joy3/count_errors_fast.nes",
        "ERRORS: 0/1000"
    );

    setup_rom_console_test!(
        test_read_joy3_test_buttons,
        "roms/automated_tests/read_joy3/test_buttons.nes"
    );

    setup_rom_console_test!(
        test_read_joy3_thorough_test,
        "roms/automated_tests/read_joy3/thorough_test.nes"
    );
}
