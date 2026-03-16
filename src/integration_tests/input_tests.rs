#[cfg(test)]
mod tests {
    /////////////////////////////////////
    // Input
    /////////////////////////////////////

    // TODO integrate ruder-0.03 ROM suite

    // TODO integrate spadtest-nes-0.01 ROM suite

    // TODO integrate vaus-test-0.02 ROM suite

    /////////////////////////////////////
    // PaddleTest3 Arkanoid paddle (#1606)
    /////////////////////////////////////

    use crate::input::ControllerType;
    use crate::integration_tests::romtest_harness::tests::{
        ControllerConfig, InputAction, RomTestResult, ScriptEntry, run_rom_with_script,
    };

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
