//! Per-console key-press dispatch for the native frontend.
//!
//! Routes a key press to the right console: NES uses `handle_unmodified_key`
//! (called from `handle_key_pressed`), while GB/GBA/SNES share
//! `handle_single_joypad_key_pressed`. These delegate hotkeys to `super::hotkeys`
//! and button mapping to `super::controller_mapping`.

use super::{KeyOutcome, controller_mapping, hotkeys, keyboard_target_ports};
use crate::frontends::native::app_state::NativeAppState;
use crate::platform::audio::EmulatorAudio;
use crate::platform::emulator::Console;
use winit::keyboard::KeyCode;

/// Handles a key-press event for a [`Console::GameBoy`].
///
/// Dispatches generic hotkeys (pause, fullscreen, Ctrl+Q, Ctrl+R, shader cycling,
/// debugger controls, save/load state) and maps standard button keys to Game Boy
/// buttons on port 0.
pub(super) fn handle_gameboy_key_pressed(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> KeyOutcome {
    handle_single_joypad_key_pressed(
        console,
        key_code,
        app_state,
        audio,
        controller_mapping::gameboy_key_to_button_id,
    )
}

pub(super) fn handle_gba_key_pressed(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> KeyOutcome {
    handle_single_joypad_key_pressed(
        console,
        key_code,
        app_state,
        audio,
        controller_mapping::gba_key_to_button_id,
    )
}

pub(super) fn handle_snes_key_pressed(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> KeyOutcome {
    handle_single_joypad_key_pressed(
        console,
        key_code,
        app_state,
        audio,
        controller_mapping::snes_key_to_button_id,
    )
}

fn handle_single_joypad_key_pressed(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
    key_to_button_id: fn(KeyCode) -> Option<u8>,
) -> KeyOutcome {
    // Generic hotkeys that work for any system.
    if app_state.modifiers.control_key() {
        return match key_code {
            KeyCode::KeyQ => KeyOutcome::Quit,
            KeyCode::KeyR => {
                console.reset(!app_state.modifiers.shift_key());
                KeyOutcome::Continue
            }
            KeyCode::KeyF => {
                app_state.fullscreen = !app_state.fullscreen;
                KeyOutcome::Continue
            }
            _ => KeyOutcome::Continue,
        };
    }

    if let Some(outcome) = hotkeys::handle_common_hotkey(key_code, app_state, audio) {
        return outcome;
    }

    match key_code {
        KeyCode::KeyH => app_state.help_overlay_visible = !app_state.help_overlay_visible,
        KeyCode::F5 => return KeyOutcome::ToggleDebugger,
        KeyCode::F6 => {
            crate::nes::console::save_state_io::save_state_to_disk(console);
        }
        KeyCode::F7 => {
            crate::nes::console::save_state_io::load_state_from_disk(console);
            if let Some(audio) = audio {
                audio.drain_buffer();
            }
        }
        KeyCode::F10 => return KeyOutcome::StepOver,
        KeyCode::F11 => return KeyOutcome::StepInto,
        _ => {
            if let Some(btn_id) = key_to_button_id(key_code) {
                console.set_button(0, btn_id, true);
            }
        }
    }

    KeyOutcome::Continue
}

pub(super) fn handle_unmodified_key(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> KeyOutcome {
    if let Some(outcome) = hotkeys::handle_common_hotkey(key_code, app_state, audio) {
        return outcome;
    }

    match key_code {
        KeyCode::KeyH => app_state.help_overlay_visible = !app_state.help_overlay_visible,
        KeyCode::F5 => return KeyOutcome::ToggleDebugger,
        KeyCode::F6 => {
            crate::nes::console::save_state_io::save_state_to_disk(console);
        }
        KeyCode::F7 => {
            crate::nes::console::save_state_io::load_state_from_disk(console);
            if let Some(audio) = audio {
                audio.drain_buffer();
            }
        }
        KeyCode::F8 => return KeyOutcome::CyclePalette,
        KeyCode::F10 => return KeyOutcome::StepOver,
        KeyCode::F11 => return KeyOutcome::StepInto,
        _ => {
            let ports =
                keyboard_target_ports(app_state.gamepad_count, app_state.four_score_enabled);
            controller_mapping::handle_controller_key(console, key_code, true, ports);
        }
    }
    KeyOutcome::Continue
}

#[cfg(test)]
mod tests {

    use crate::frontends::native::keyboard::test_support::*;
    use crate::frontends::native::keyboard::{KeyOutcome, handle_key_pressed, handle_key_released};
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;
    use crate::platform::emulator::Console;
    use winit::keyboard::KeyCode;

    // ── Game Boy keyboard dispatch ────────────────────────────────────────────

    /// Creates a minimal valid GB ROM for testing (ROM-only, 32 KB, correct checksum).
    fn minimal_gb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn make_gameboy_console() -> Console {
        let mut console = Console::new_gameboy(AppContext::new_with_config(Config::default()));
        console
            .load_rom(&minimal_gb_rom(), "test.gb")
            .expect("minimal GB ROM should load");
        console
    }

    #[test]
    fn gameboy_d_key_sets_right_button() {
        // Given a Game Boy console and default state (no gamepads)
        let mut console = make_gameboy_console();
        let mut state = make_state();
        // When the 'D' key (Right) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        // Then the Right button (bit 7) should be set on port 0
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "D key should set GB Right button"
        );
    }

    #[test]
    fn gameboy_w_key_sets_up_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_UP,
            0,
            "W key should set GB Up button"
        );
    }

    #[test]
    fn gameboy_t_key_sets_a_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_A,
            0,
            "T key should set GB A button"
        );
    }

    #[test]
    fn gameboy_f6_save_state_returns_continue() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F6, &mut state, None),
            KeyOutcome::Continue,
            "F6 should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_f7_load_state_returns_continue() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F7, &mut state, None),
            KeyOutcome::Continue,
            "F7 should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_ctrl_r_soft_resets() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue,
            "Ctrl+R should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_ctrl_shift_r_hard_resets() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        with_ctrl_shift(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue,
            "Ctrl+Shift+R should return Continue in GB mode"
        );
    }

    #[test]
    fn snes_f6_save_state_returns_continue() {
        let mut console = make_snes_console("snes-f6-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F6, &mut state, None),
            KeyOutcome::Continue,
            "F6 should return Continue in SNES mode"
        );
    }

    #[test]
    fn snes_f7_load_state_returns_continue() {
        let mut console = make_snes_console("snes-f7-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F7, &mut state, None),
            KeyOutcome::Continue,
            "F7 should return Continue in SNES mode"
        );
    }

    #[test]
    fn snes_f4_cycle_shader_returns_cycle_shader() {
        let mut console = make_snes_console("snes-f4-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F4, &mut state, None),
            KeyOutcome::CycleShader,
            "F4 should request shader cycling in SNES mode"
        );
    }

    #[test]
    fn gameboy_h_key_toggles_help_overlay_on() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(
            state.help_overlay_visible,
            "H key should toggle help overlay on in GB mode"
        );
    }

    #[test]
    fn gameboy_h_key_toggles_help_overlay_off() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        state.help_overlay_visible = true;
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(
            !state.help_overlay_visible,
            "H key should toggle help overlay off in GB mode"
        );
    }

    #[test]
    fn gameboy_d_key_released_clears_right_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        // Pressing D must set the button first; if not, the release test is meaningless.
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "D key must set GB Right button before testing release"
        );
        handle_key_released(&mut console, KeyCode::KeyD, 0, false);
        assert_eq!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "Releasing D should clear GB Right button"
        );
    }

    #[test]
    fn gba_keyboard_maps_all_ten_buttons() {
        let cases = [
            (KeyCode::KeyT, GBA_KEY_A, "T should press GBA A"),
            (KeyCode::KeyR, GBA_KEY_B, "R should press GBA B"),
            (KeyCode::Digit4, GBA_KEY_SELECT, "4 should press GBA Select"),
            (KeyCode::Digit5, GBA_KEY_START, "5 should press GBA Start"),
            (KeyCode::KeyW, GBA_KEY_UP, "W should press GBA Up"),
            (KeyCode::KeyS, GBA_KEY_DOWN, "S should press GBA Down"),
            (KeyCode::KeyA, GBA_KEY_LEFT, "A should press GBA Left"),
            (KeyCode::KeyD, GBA_KEY_RIGHT, "D should press GBA Right"),
            (KeyCode::KeyQ, GBA_KEY_L, "Q should press GBA L"),
            (KeyCode::KeyE, GBA_KEY_R, "E should press GBA R"),
            (KeyCode::ArrowUp, GBA_KEY_UP, "ArrowUp should press GBA Up"),
            (
                KeyCode::ArrowDown,
                GBA_KEY_DOWN,
                "ArrowDown should press GBA Down",
            ),
            (
                KeyCode::ArrowLeft,
                GBA_KEY_LEFT,
                "ArrowLeft should press GBA Left",
            ),
            (
                KeyCode::ArrowRight,
                GBA_KEY_RIGHT,
                "ArrowRight should press GBA Right",
            ),
        ];

        for (key, mask, message) in cases {
            let mut console = make_gba_console();
            let mut state = make_state();
            handle_key_pressed(&mut console, key, &mut state, None);
            assert_eq!(gba_keyinput(&console) & mask, 0, "{message}");
        }
    }

    #[test]
    fn gba_keyboard_releases_l_and_r_shoulders() {
        let mut console = make_gba_console();
        let mut state = make_state();

        handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None);
        handle_key_pressed(&mut console, KeyCode::KeyE, &mut state, None);

        assert_eq!(
            gba_keyinput(&console) & GBA_KEY_L,
            0,
            "Q should press GBA L"
        );
        assert_eq!(
            gba_keyinput(&console) & GBA_KEY_R,
            0,
            "E should press GBA R"
        );

        handle_key_released(&mut console, KeyCode::KeyQ, 0, false);
        handle_key_released(&mut console, KeyCode::KeyE, 0, false);

        assert_ne!(
            gba_keyinput(&console) & GBA_KEY_L,
            0,
            "releasing Q should clear GBA L"
        );
        assert_ne!(
            gba_keyinput(&console) & GBA_KEY_R,
            0,
            "releasing E should clear GBA R"
        );
    }

    #[test]
    fn gameboy_q_and_e_do_not_map_to_buttons() {
        let mut console = make_gameboy_console();
        let mut state = make_state();

        handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None);
        handle_key_pressed(&mut console, KeyCode::KeyE, &mut state, None);

        assert_eq!(
            console.get_joypad_button_states(0),
            0,
            "Game Boy keyboard mapping should remain eight-button only"
        );
    }
}
