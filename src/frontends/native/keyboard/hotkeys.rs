//! System, debugger, and cartridge-switch hotkey handling for the native frontend.
//!
//! Hotkeys that are not controller button presses: pause/volume/FPS/shader
//! (`handle_common_hotkey`), the NES Ctrl combos (`handle_ctrl_hotkey`), and the
//! cartridge-switch dialog input (`handle_cartridge_switch_key`).

use super::KeyOutcome;
use crate::frontends::native::app_state::NativeAppState;
use crate::platform::audio::EmulatorAudio;
use crate::platform::emulator::Console;
use winit::keyboard::KeyCode;

/// Attempts to handle a key press that is identical for all console types.
///
/// Handles Escape and Space (which mutate `app_state` / `audio`) and F2/F3
/// (volume adjust), returning `Some(KeyOutcome::Continue)`; returns
/// `Some(KeyOutcome::ToggleFps)` for F1 and `Some(KeyOutcome::CycleShader)` for
/// F4. Returns `None` for keys that need system-specific handling.
pub(super) fn handle_common_hotkey(
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> Option<KeyOutcome> {
    match key_code {
        KeyCode::Escape => {
            app_state.mouse_grabbed = false;
            app_state.mouse_released_by_escape = true;
            Some(KeyOutcome::Continue)
        }
        KeyCode::Space => {
            app_state.paused = !app_state.paused;
            if let Some(audio) = audio {
                if app_state.paused {
                    audio.pause();
                } else {
                    audio.resume();
                }
            }
            Some(KeyOutcome::Continue)
        }
        KeyCode::F1 => Some(KeyOutcome::ToggleFps),
        KeyCode::F4 => Some(KeyOutcome::CycleShader),
        KeyCode::F2 => {
            adjust_volume(audio, 0.1);
            Some(KeyOutcome::Continue)
        }
        KeyCode::F3 => {
            adjust_volume(audio, -0.1);
            Some(KeyOutcome::Continue)
        }
        _ => None,
    }
}

pub(super) fn handle_ctrl_hotkey(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
) -> KeyOutcome {
    match key_code {
        KeyCode::KeyQ => KeyOutcome::Quit,
        KeyCode::KeyR => {
            console.reset(!app_state.modifiers.shift_key());
            KeyOutcome::Continue
        }
        KeyCode::KeyO => KeyOutcome::OpenCartridgeSwitch,
        KeyCode::KeyF => {
            app_state.fullscreen = !app_state.fullscreen;
            KeyOutcome::Continue
        }
        _ => KeyOutcome::Continue,
    }
}

fn adjust_volume(audio: Option<&dyn EmulatorAudio>, delta: f32) {
    if let Some(audio) = audio {
        audio.set_volume(audio.get_volume() + delta);
    }
}

pub(super) fn handle_cartridge_switch_key(
    key_code: KeyCode,
    app_state: &mut NativeAppState,
) -> KeyOutcome {
    match key_code {
        KeyCode::Escape => {
            app_state.cart_switch.close();
            KeyOutcome::CloseCartridgeSwitch
        }
        KeyCode::ArrowDown => {
            app_state.cart_switch.move_selection_next();
            KeyOutcome::Continue
        }
        KeyCode::ArrowUp => {
            app_state.cart_switch.move_selection_prev();
            KeyOutcome::Continue
        }
        KeyCode::Backspace => {
            app_state.cart_switch.filter.pop();
            app_state.cart_switch.refresh_filtered();
            KeyOutcome::Continue
        }
        KeyCode::Enter | KeyCode::NumpadEnter => {
            if let Some(path) = app_state.cart_switch.selected_entry().map(str::to_string) {
                app_state.cart_switch.close();
                KeyOutcome::SwitchCartridge(path)
            } else {
                KeyOutcome::Continue
            }
        }
        _ => {
            if let Some(ch) = key_code_to_filter_char(key_code, app_state.modifiers.shift_key()) {
                app_state.cart_switch.filter.push(ch);
                app_state.cart_switch.refresh_filtered();
            }
            KeyOutcome::Continue
        }
    }
}

/// Maps a physical key code to its lowercase character for the filter field.
fn key_code_to_filter_char(key_code: KeyCode, shift: bool) -> Option<char> {
    match key_code {
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => Some('e'),
        KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'),
        KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'),
        KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'),
        KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'),
        KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'),
        KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'),
        KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'),
        KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'),
        KeyCode::KeyZ => Some('z'),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        KeyCode::Space => Some(' '),
        KeyCode::Minus => Some(if shift { '_' } else { '-' }),
        KeyCode::Period => Some('.'),
        KeyCode::Slash => Some('/'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontends::native::app_state::NativeAppState;
    use crate::frontends::native::keyboard::test_support::*;
    use crate::frontends::native::keyboard::{KeyOutcome, handle_key_pressed};

    use std::sync::atomic::Ordering;
    use winit::keyboard::{KeyCode, ModifiersState};

    // ── Hotkey tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_ctrl_q_returns_quit() {
        let mut console = make_nes_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None),
            KeyOutcome::Quit
        );
    }

    #[test]
    fn test_escape_returns_continue_when_mouse_not_grabbed() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.mouse_grabbed = false;
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_escape_releases_mouse_when_grabbed() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.mouse_grabbed = true;
        handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert!(!state.mouse_grabbed, "Escape should release mouse grab");
        assert!(
            state.mouse_released_by_escape,
            "Escape should set mouse_released_by_escape"
        );
    }

    #[test]
    fn test_space_toggles_pause_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Space, &mut state, None);
        assert!(state.paused, "Space should pause when unpaused");
    }

    #[test]
    fn test_space_toggles_pause_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = true;
        handle_key_pressed(&mut console, KeyCode::Space, &mut state, None);
        assert!(!state.paused, "Space should unpause when paused");
    }

    #[test]
    fn test_space_calls_audio_pause_when_pausing() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = false;
        let (audio, pause_called, _resume_called, _drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::Space,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            pause_called.load(Ordering::Relaxed),
            "Space (pause) should call audio.pause() to prevent ring buffer drain"
        );
    }

    #[test]
    fn test_space_calls_audio_resume_when_resuming() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = true;
        let (audio, _pause_called, resume_called, _drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::Space,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            resume_called.load(Ordering::Relaxed),
            "Space (resume) should call audio.resume() to restore audio after pause"
        );
    }

    #[test]
    fn test_h_toggles_help_overlay_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(state.help_overlay_visible);
    }

    #[test]
    fn test_h_toggles_help_overlay_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.help_overlay_visible = true;
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(!state.help_overlay_visible);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(state.fullscreen);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.fullscreen = true;
        with_ctrl(&mut state);
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen);
    }

    #[test]
    fn test_alt_f_does_not_toggle_fullscreen() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.modifiers = ModifiersState::ALT;
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen, "Alt+F should not toggle fullscreen");
    }

    #[test]
    fn test_f4_returns_cycle_shader() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F4, &mut state, None),
            KeyOutcome::CycleShader
        );
    }

    #[test]
    fn test_f2_increases_volume() {
        let mut console = make_nes_console();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut console, KeyCode::F2, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.6).abs() < 1e-5,
            "F2 should raise volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f3_decreases_volume() {
        let mut console = make_nes_console();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut console, KeyCode::F3, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.4).abs() < 1e-5,
            "F3 should lower volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f5_returns_toggle_debugger() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F5, &mut state, None),
            KeyOutcome::ToggleDebugger
        );
    }

    #[test]
    fn test_f8_returns_cycle_palette_for_nes() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F8, &mut state, None),
            KeyOutcome::CyclePalette
        );
    }

    #[test]
    fn test_f10_returns_step_over() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F10, &mut state, None),
            KeyOutcome::StepOver
        );
    }

    #[test]
    fn test_f11_returns_step_into() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F11, &mut state, None),
            KeyOutcome::StepInto
        );
    }

    #[test]
    fn test_ctrl_r_soft_resets() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_ctrl_shift_r_hard_resets() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_state();
        with_ctrl_shift(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    // ── Cartridge-switch dialog keyboard ──────────────────────────────────────

    fn make_cart_switch_state(entries: &[&str]) -> NativeAppState {
        let mut state = make_state();
        state.cart_switch.open = true;
        state.cart_switch.entries = entries.iter().map(|s| s.to_string()).collect();
        state
    }

    #[test]
    fn test_cart_switch_typing_adds_to_filter() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes"]);
        handle_key_pressed(&mut console, KeyCode::KeyA, &mut state, None);
        assert_eq!(state.cart_switch.filter, "a");
        handle_key_pressed(&mut console, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
    }

    #[test]
    fn test_cart_switch_backspace_removes_char() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        state.cart_switch.filter = "abc".to_string();
        let outcome = handle_key_pressed(&mut console, KeyCode::Backspace, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_backspace_on_empty_filter() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Backspace, &mut state, None);
        assert!(state.cart_switch.filter.is_empty());
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_enter_returns_switch_cartridge() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["game.nes", "other.nes"]);
        state.cart_switch.selection = 0;
        let outcome = handle_key_pressed(&mut console, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::SwitchCartridge("game.nes".to_string()));
        assert!(!state.cart_switch.open, "dialog should close after Enter");
    }

    #[test]
    fn test_cart_switch_enter_with_no_entries_returns_continue() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&[]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_escape_closes_dialog() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert!(!state.cart_switch.open);
    }

    #[test]
    fn test_cart_switch_arrow_down_wraps() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 1;
        handle_key_pressed(&mut console, KeyCode::ArrowDown, &mut state, None);
        assert_eq!(state.cart_switch.selection, 0, "should wrap to first");
    }

    #[test]
    fn test_cart_switch_arrow_up_wraps() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 0;
        handle_key_pressed(&mut console, KeyCode::ArrowUp, &mut state, None);
        assert_eq!(state.cart_switch.selection, 1, "should wrap to last");
    }

    #[test]
    fn test_cart_switch_typing_filters_entries() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes", "gamma.nes"]);
        // Type 'b' — only "beta.nes" matches (fuzzy on filename)
        handle_key_pressed(&mut console, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.visible_count(), 1);
        assert_eq!(state.cart_switch.selected_entry(), Some("beta.nes"));
    }

    #[test]
    fn test_cart_switch_keys_dont_trigger_controller() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_cart_switch_state(&["a.nes"]);
        // Pressing W while cart switch is open should NOT set Up on P1
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should not control NES when dialog is open"
        );
    }

    // ── Review fix: Escape returns CloseCartridgeSwitch ───────────────────────

    #[test]
    fn test_cart_switch_escape_returns_close_outcome() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert_eq!(
            outcome,
            KeyOutcome::CloseCartridgeSwitch,
            "Escape should return CloseCartridgeSwitch so event loop can restore pause"
        );
    }

    // ── Review fix: underscore via Shift+Minus ────────────────────────────────

    #[test]
    fn test_cart_switch_shift_minus_types_underscore() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a_b.nes"]);
        state.modifiers = ModifiersState::SHIFT;
        handle_key_pressed(&mut console, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "_",
            "Shift+Minus should type underscore"
        );
    }

    #[test]
    fn test_cart_switch_minus_without_shift_types_dash() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a-b.nes"]);
        handle_key_pressed(&mut console, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "-",
            "Minus without Shift should type dash"
        );
    }

    #[test]
    fn test_f7_drains_audio_buffer_after_state_load() {
        // When F7 (load state) is pressed, any pre-restore samples still buffered
        // in the audio ring buffer must be discarded silently so they do not
        // bleed into the post-restore playback.
        let mut console = make_nes_console();
        let mut state = make_state();
        let (audio, _pause_called, _resume_called, drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::F7,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            drain_buffer_called.load(Ordering::Relaxed),
            "F7 (load state) must call audio.drain_buffer() to discard stale samples"
        );
    }
}
