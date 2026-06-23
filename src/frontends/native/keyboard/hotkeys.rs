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
/// Returns `Some(KeyOutcome::Continue)` for Escape and Space (which mutate
/// `app_state` / `audio`), `Some(KeyOutcome::CycleShader)` for F4, and
/// `None` for keys that need system-specific handling.
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
