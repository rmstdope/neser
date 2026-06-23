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
