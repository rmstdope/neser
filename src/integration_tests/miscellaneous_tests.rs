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
        // NES-001 open-bus emulation: bits 4-1 of $4016/$4017 are grounded, so allpads
        // identifies the console as NES-001 (not NES-101/dogbone). Controllers show as
        // "NES CONTROLLER" (no dogbone replacement).
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

    /// Test that the allpads title screen shows a known console model (NES-001, NES-101,
    /// or Family Computer), not "Unknown console". This verifies correct PPU latch open-bus
    /// behavior: bits 7-5 of $4016/$4017 are open bus, bits 4-1 are grounded/driven on NES-001.
    #[test]
    fn allpads_console_probe_identifies_known_console_model() {
        let config = ControllerConfig::joypad_port1();
        // Capture title screen during first 200 frames (COLDBOOTTIME = 240)
        let result = run_allpads(&config, &[], 100, 0);
        let cap = &result.captures[0];
        let known_models = ["NES-001", "NES-101", "Family Computer"];
        let has_known_model = known_models.iter().any(|m| cap.nametable_text.contains(m));
        assert!(
            has_known_model,
            "Console probe should identify a known model (NES-001, NES-101, or Family Computer), got:\n{}",
            cap.nametable_text
        );
        assert!(
            !cap.nametable_text.contains("Unknown console"),
            "Console probe should NOT show 'Unknown console', got:\n{}",
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
