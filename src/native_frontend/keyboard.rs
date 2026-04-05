//! Keyboard input and hotkey handling for the native frontend.
//!
//! This module maps winit [`KeyCode`]s to NES controller buttons, SNES
//! buttons, and Power Pad buttons, and dispatches system hotkeys such as
//! Ctrl+Q (quit), Ctrl+R (reset), Space (pause), and so on.

use crate::audio::NesAudio;
use crate::console::Nes;
use crate::input::{Button, PowerPadButton, SnesButton};
use crate::native_frontend::app_state::NativeAppState;
use winit::keyboard::KeyCode;

/// The result of processing a key-press event.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// The user requested the application to exit (e.g. Ctrl+Q).
    Quit,
    /// Normal processing; the event loop should continue.
    Continue,
    /// The shader preset should be cycled (F4).
    CycleShader,
    /// Toggle the debugger open/closed (F5).
    ToggleDebugger,
    /// Step over the current instruction (F10).
    StepOver,
    /// Step into the current instruction (F11).
    StepInto,
    /// The user selected a cartridge to load (Enter in cart-switch dialog).
    SwitchCartridge(String),
    /// The user requested the cartridge-switch dialog (Ctrl+O).
    OpenCartridgeSwitch,
    /// The user closed the cartridge-switch dialog (Escape).
    CloseCartridgeSwitch,
}

/// Handles a key-press event.
///
/// Updates `nes` and `app_state` as appropriate, and optionally adjusts
/// audio volume via `audio`.  Returns a [`KeyOutcome`] so the caller can act
/// on actions that require access to the GL wrapper (e.g. shader cycling,
/// fullscreen toggle).
pub fn handle_key_pressed(
    nes: &mut Nes,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn NesAudio>,
) -> KeyOutcome {
    if app_state.cart_switch.open {
        return handle_cartridge_switch_key(key_code, app_state);
    }

    if app_state.modifiers.control_key() {
        return handle_ctrl_hotkey(nes, key_code, app_state);
    }

    handle_unmodified_key(nes, key_code, app_state, audio)
}

/// Handles a key-release event.
///
/// Releases the NES / SNES / Power Pad button corresponding to the given key.
pub fn handle_key_released(nes: &mut Nes, key_code: KeyCode) {
    handle_controller_key(nes, key_code, false);
}

// ── Hotkey dispatch ───────────────────────────────────────────────────────────

fn handle_ctrl_hotkey(
    nes: &mut Nes,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
) -> KeyOutcome {
    match key_code {
        KeyCode::KeyQ => KeyOutcome::Quit,
        KeyCode::KeyR => {
            nes.reset(!app_state.modifiers.shift_key());
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

fn handle_unmodified_key(
    nes: &mut Nes,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn NesAudio>,
) -> KeyOutcome {
    match key_code {
        KeyCode::Escape => {
            app_state.mouse_grabbed = false;
            app_state.mouse_released_by_escape = true;
        }
        KeyCode::Space => app_state.paused = !app_state.paused,
        KeyCode::KeyH => app_state.help_overlay_visible = !app_state.help_overlay_visible,
        KeyCode::F2 => adjust_volume(audio, 0.1),
        KeyCode::F3 => adjust_volume(audio, -0.1),
        KeyCode::F4 => return KeyOutcome::CycleShader,
        KeyCode::F5 => return KeyOutcome::ToggleDebugger,
        KeyCode::F6 => {
            crate::console::save_state_io::save_state_to_disk(nes);
        }
        KeyCode::F7 => {
            crate::console::save_state_io::load_state_from_disk(nes);
        }
        KeyCode::F10 => return KeyOutcome::StepOver,
        KeyCode::F11 => return KeyOutcome::StepInto,
        _ => handle_controller_key(nes, key_code, true),
    }

    KeyOutcome::Continue
}

fn adjust_volume(audio: Option<&dyn NesAudio>, delta: f32) {
    if let Some(audio) = audio {
        audio.set_volume(audio.get_volume() + delta);
    }
}

// ── Controller key mapping ────────────────────────────────────────────────────

/// Maps a [`KeyCode`] to NES/SNES/Power Pad button presses or releases for
/// both player ports simultaneously.  Shared between press and release paths.
///
/// Note: Player 1 keys are intentionally applied to both ports so a single
/// keyboard can cover both players when no gamepad is connected.  Player
/// 2-specific keys (IJKL etc.) remain port-2 only.
fn handle_controller_key(nes: &mut Nes, key_code: KeyCode, pressed: bool) {
    match key_code {
        // ── Player 1: 1/2/3 → Power Pad buttons ──────────────────────────
        KeyCode::Digit1 => pp_both(nes, PowerPadButton::One, pressed),
        KeyCode::Digit2 => pp_both(nes, PowerPadButton::Two, pressed),
        KeyCode::Digit3 => pp_both(nes, PowerPadButton::Three, pressed),

        // ── Player 1: QWEASD (D-pad / SNES L/R / Power Pad) ──────────────
        KeyCode::KeyQ => pp_or_snes_both(nes, PowerPadButton::Four, SnesButton::L, pressed),
        KeyCode::KeyW => pp_or_btn_or_snes_both(
            nes,
            PowerPadButton::Five,
            Button::Up,
            SnesButton::Up,
            pressed,
        ),
        KeyCode::KeyE => pp_or_snes_both(nes, PowerPadButton::Six, SnesButton::R, pressed),
        KeyCode::KeyA => pp_or_btn_or_snes_both(
            nes,
            PowerPadButton::Seven,
            Button::Left,
            SnesButton::Left,
            pressed,
        ),
        KeyCode::KeyS => pp_or_btn_or_snes_both(
            nes,
            PowerPadButton::Eight,
            Button::Down,
            SnesButton::Down,
            pressed,
        ),
        KeyCode::KeyD => pp_or_btn_or_snes_both(
            nes,
            PowerPadButton::Nine,
            Button::Right,
            SnesButton::Right,
            pressed,
        ),

        // ── Player 1: ZXC → Power Pad ────────────────────────────────────
        KeyCode::KeyZ => pp_both(nes, PowerPadButton::Ten, pressed),
        KeyCode::KeyX => pp_both(nes, PowerPadButton::Eleven, pressed),
        KeyCode::KeyC => pp_both(nes, PowerPadButton::Twelve, pressed),

        // ── Player 1: R/T = A/B (joypad or SNES Y/X) ─────────────────────
        KeyCode::KeyR => btn_or_snes_both(nes, Button::A, SnesButton::Y, pressed),
        KeyCode::KeyT => btn_or_snes_both(nes, Button::B, SnesButton::X, pressed),

        // ── Player 1: F/G = SNES B/A only ────────────────────────────────
        KeyCode::KeyF => snes_both(nes, SnesButton::B, pressed),
        KeyCode::KeyG => snes_both(nes, SnesButton::A, pressed),

        // ── Player 1: 4/5 = Select/Start ─────────────────────────────────
        KeyCode::Digit4 => btn_or_snes_both(nes, Button::Select, SnesButton::Select, pressed),
        KeyCode::Digit5 => btn_or_snes_both(nes, Button::Start, SnesButton::Start, pressed),

        // ── Player 2: 7/8 → Power Pad; 9 = PP3/Select; 0 = Start ─────────
        KeyCode::Digit7 => pp_p2(nes, PowerPadButton::One, pressed),
        KeyCode::Digit8 => pp_p2(nes, PowerPadButton::Two, pressed),
        KeyCode::Digit9 => pp_or_btn_p2(nes, PowerPadButton::Three, Button::Select, pressed),
        KeyCode::Digit0 => nes.set_button(2, Button::Start, pressed),

        // ── Player 2: UIOJKL M,. = D-pad / Power Pad ─────────────────────
        KeyCode::KeyU => pp_p2(nes, PowerPadButton::Four, pressed),
        KeyCode::KeyI => pp_or_btn_p2(nes, PowerPadButton::Five, Button::Up, pressed),
        KeyCode::KeyO => pp_or_btn_p2(nes, PowerPadButton::Six, Button::A, pressed),
        KeyCode::KeyJ => pp_or_btn_p2(nes, PowerPadButton::Seven, Button::Left, pressed),
        KeyCode::KeyK => pp_or_btn_p2(nes, PowerPadButton::Eight, Button::Down, pressed),
        KeyCode::KeyL => pp_or_btn_p2(nes, PowerPadButton::Nine, Button::Right, pressed),
        KeyCode::KeyM => pp_p2(nes, PowerPadButton::Ten, pressed),
        KeyCode::Comma => pp_p2(nes, PowerPadButton::Eleven, pressed),
        KeyCode::Period => pp_p2(nes, PowerPadButton::Twelve, pressed),
        KeyCode::KeyP => nes.set_button(2, Button::B, pressed),

        _ => {}
    }
}

// ── Dual-port (P1 + P2) button helpers ───────────────────────────────────────

fn pp_both(nes: &mut Nes, pp: PowerPadButton, pressed: bool) {
    for port in [1, 2] {
        nes.set_power_pad_button(port, pp, pressed);
    }
}

fn snes_both(nes: &mut Nes, snes: SnesButton, pressed: bool) {
    for port in [1, 2] {
        nes.set_snes_button(port, snes, pressed);
    }
}

fn btn_or_snes_both(nes: &mut Nes, btn: Button, snes: SnesButton, pressed: bool) {
    for port in [1, 2] {
        if !nes.set_snes_button(port, snes, pressed) {
            nes.set_button(port, btn, pressed);
        }
    }
}

fn pp_or_snes_both(nes: &mut Nes, pp: PowerPadButton, snes: SnesButton, pressed: bool) {
    for port in [1, 2] {
        if !nes.set_power_pad_button(port, pp, pressed) {
            nes.set_snes_button(port, snes, pressed);
        }
    }
}

fn pp_or_btn_or_snes_both(
    nes: &mut Nes,
    pp: PowerPadButton,
    btn: Button,
    snes: SnesButton,
    pressed: bool,
) {
    for port in [1, 2] {
        if !nes.set_power_pad_button(port, pp, pressed) && !nes.set_snes_button(port, snes, pressed)
        {
            nes.set_button(port, btn, pressed);
        }
    }
}

// ── Player-2-only button helpers ─────────────────────────────────────────────

fn pp_p2(nes: &mut Nes, pp: PowerPadButton, pressed: bool) {
    nes.set_power_pad_button(2, pp, pressed);
}

fn pp_or_btn_p2(nes: &mut Nes, pp: PowerPadButton, btn: Button, pressed: bool) {
    if !nes.set_power_pad_button(2, pp, pressed) {
        nes.set_button(2, btn, pressed);
    }
}

// ── Cartridge-switch dialog ───────────────────────────────────────────────────

fn handle_cartridge_switch_key(key_code: KeyCode, app_state: &mut NativeAppState) -> KeyOutcome {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::console::Config;
    use winit::keyboard::ModifiersState;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn make_nes() -> Nes {
        Nes::new(AppContext::new_with_config(Config::default()))
    }

    fn make_nes_with_cartridge() -> Nes {
        let mut nes = make_nes();
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFA] = 0x00;
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x80;
        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes
    }

    fn make_state() -> NativeAppState {
        NativeAppState::default()
    }

    fn with_ctrl(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL;
    }

    fn with_ctrl_shift(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    }

    fn buttons(nes: &Nes, port: u8) -> u8 {
        nes.get_joypad_button_states(port)
    }

    const BIT_A: u8 = 1 << Button::A as u8;
    const BIT_B: u8 = 1 << Button::B as u8;
    const BIT_SELECT: u8 = 1 << Button::Select as u8;
    const BIT_START: u8 = 1 << Button::Start as u8;
    const BIT_UP: u8 = 1 << Button::Up as u8;
    const BIT_DOWN: u8 = 1 << Button::Down as u8;
    const BIT_LEFT: u8 = 1 << Button::Left as u8;
    const BIT_RIGHT: u8 = 1 << Button::Right as u8;

    // ── Mock audio ────────────────────────────────────────────────────────────

    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockAudio {
        volume: Arc<AtomicU32>,
    }

    impl MockAudio {
        fn new_with_volume(vol: f32) -> Self {
            Self {
                volume: Arc::new(AtomicU32::new(f32::to_bits(vol))),
            }
        }
    }

    impl NesAudio for MockAudio {
        fn queue_sample(&mut self, _sample: f32) {}
        fn resume(&self) {}
        fn pause(&self) {}
        fn set_volume(&self, volume: f32) {
            self.volume
                .store(f32::to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
        }
        fn get_volume(&self) -> f32 {
            f32::from_bits(self.volume.load(Ordering::Relaxed))
        }
        fn prime_startup(&mut self, _samples: usize) {}
        fn take_and_reset_stats(&self) -> (u64, u64, u64) {
            (0, 0, 0)
        }
        fn actual_sample_rate(&self) -> i32 {
            44100
        }
    }

    // ── Hotkey tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_ctrl_q_returns_quit() {
        let mut nes = make_nes();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::KeyQ, &mut state, None),
            KeyOutcome::Quit
        );
    }

    #[test]
    fn test_escape_returns_continue_when_mouse_not_grabbed() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.mouse_grabbed = false;
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::Escape, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_escape_releases_mouse_when_grabbed() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.mouse_grabbed = true;
        handle_key_pressed(&mut nes, KeyCode::Escape, &mut state, None);
        assert!(!state.mouse_grabbed, "Escape should release mouse grab");
        assert!(
            state.mouse_released_by_escape,
            "Escape should set mouse_released_by_escape"
        );
    }

    #[test]
    fn test_space_toggles_pause_on() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::Space, &mut state, None);
        assert!(state.paused, "Space should pause when unpaused");
    }

    #[test]
    fn test_space_toggles_pause_off() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.paused = true;
        handle_key_pressed(&mut nes, KeyCode::Space, &mut state, None);
        assert!(!state.paused, "Space should unpause when paused");
    }

    #[test]
    fn test_h_toggles_help_overlay_on() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyH, &mut state, None);
        assert!(state.help_overlay_visible);
    }

    #[test]
    fn test_h_toggles_help_overlay_off() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.help_overlay_visible = true;
        handle_key_pressed(&mut nes, KeyCode::KeyH, &mut state, None);
        assert!(!state.help_overlay_visible);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_on() {
        let mut nes = make_nes();
        let mut state = make_state();
        with_ctrl(&mut state);
        handle_key_pressed(&mut nes, KeyCode::KeyF, &mut state, None);
        assert!(state.fullscreen);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_off() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.fullscreen = true;
        with_ctrl(&mut state);
        handle_key_pressed(&mut nes, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen);
    }

    #[test]
    fn test_alt_f_does_not_toggle_fullscreen() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.modifiers = ModifiersState::ALT;
        handle_key_pressed(&mut nes, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen, "Alt+F should not toggle fullscreen");
    }

    #[test]
    fn test_f4_returns_cycle_shader() {
        let mut nes = make_nes();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::F4, &mut state, None),
            KeyOutcome::CycleShader
        );
    }

    #[test]
    fn test_f2_increases_volume() {
        let mut nes = make_nes();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut nes, KeyCode::F2, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.6).abs() < 1e-5,
            "F2 should raise volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f3_decreases_volume() {
        let mut nes = make_nes();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut nes, KeyCode::F3, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.4).abs() < 1e-5,
            "F3 should lower volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f5_returns_toggle_debugger() {
        let mut nes = make_nes();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::F5, &mut state, None),
            KeyOutcome::ToggleDebugger
        );
    }

    #[test]
    fn test_f10_returns_step_over() {
        let mut nes = make_nes();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::F10, &mut state, None),
            KeyOutcome::StepOver
        );
    }

    #[test]
    fn test_f11_returns_step_into() {
        let mut nes = make_nes();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::F11, &mut state, None),
            KeyOutcome::StepInto
        );
    }

    #[test]
    fn test_ctrl_r_soft_resets() {
        let mut nes = make_nes_with_cartridge();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_ctrl_shift_r_hard_resets() {
        let mut nes = make_nes_with_cartridge();
        let mut state = make_state();
        with_ctrl_shift(&mut state);
        assert_eq!(
            handle_key_pressed(&mut nes, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    // ── Player 1 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p1_w_sets_up() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyW, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_UP, 0, "W should set Up on P1");
    }

    #[test]
    fn test_p1_a_sets_left() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyA, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_LEFT, 0, "A should set Left on P1");
    }

    #[test]
    fn test_p1_s_sets_down() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyS, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_DOWN, 0, "S should set Down on P1");
    }

    #[test]
    fn test_p1_d_sets_right() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyD, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_RIGHT, 0, "D should set Right on P1");
    }

    #[test]
    fn test_p1_r_sets_a() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyR, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_A, 0, "R should set A on P1");
    }

    #[test]
    fn test_p1_t_sets_b() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyT, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_B, 0, "T should set B on P1");
    }

    #[test]
    fn test_p1_num4_sets_select() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::Digit4, &mut state, None);
        assert_ne!(
            buttons(&nes, 1) & BIT_SELECT,
            0,
            "4 should set Select on P1"
        );
    }

    #[test]
    fn test_p1_num5_sets_start() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::Digit5, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_START, 0, "5 should set Start on P1");
    }

    // ── Player 1 — key release clears button ──────────────────────────────────

    #[test]
    fn test_p1_w_released_clears_up() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyW, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_UP, 0);
        handle_key_released(&mut nes, KeyCode::KeyW);
        assert_eq!(buttons(&nes, 1) & BIT_UP, 0, "Releasing W should clear Up");
    }

    #[test]
    fn test_p1_r_released_clears_a() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyR, &mut state, None);
        handle_key_released(&mut nes, KeyCode::KeyR);
        assert_eq!(buttons(&nes, 1) & BIT_A, 0, "Releasing R should clear A");
    }

    // ── Player 2 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p2_i_sets_up() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyI, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_UP, 0, "I should set Up on P2");
        assert_eq!(buttons(&nes, 1) & BIT_UP, 0, "I should not affect P1");
    }

    #[test]
    fn test_p2_j_sets_left() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyJ, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_LEFT, 0, "J should set Left on P2");
    }

    #[test]
    fn test_p2_k_sets_down() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyK, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_DOWN, 0, "K should set Down on P2");
    }

    #[test]
    fn test_p2_l_sets_right() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyL, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_RIGHT, 0, "L should set Right on P2");
    }

    #[test]
    fn test_p2_o_sets_a() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyO, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_A, 0, "O should set A on P2");
    }

    #[test]
    fn test_p2_p_sets_b() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyP, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_B, 0, "P should set B on P2");
    }

    #[test]
    fn test_p2_num9_sets_select() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::Digit9, &mut state, None);
        assert_ne!(
            buttons(&nes, 2) & BIT_SELECT,
            0,
            "9 should set Select on P2"
        );
    }

    #[test]
    fn test_p2_num0_sets_start() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::Digit0, &mut state, None);
        assert_ne!(buttons(&nes, 2) & BIT_START, 0, "0 should set Start on P2");
    }

    // ── Shared keys target both ports ─────────────────────────────────────────

    #[test]
    fn test_w_targets_both_p1_and_p2() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyW, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_UP, 0, "W should set Up on P1");
        assert_ne!(buttons(&nes, 2) & BIT_UP, 0, "W should also set Up on P2");
    }

    #[test]
    fn test_s_targets_both_p1_and_p2() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyS, &mut state, None);
        assert_ne!(buttons(&nes, 1) & BIT_DOWN, 0);
        assert_ne!(buttons(&nes, 2) & BIT_DOWN, 0);
    }

    // ── Player 2 key release ──────────────────────────────────────────────────

    #[test]
    fn test_p2_i_released_clears_up() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::KeyI, &mut state, None);
        handle_key_released(&mut nes, KeyCode::KeyI);
        assert_eq!(
            buttons(&nes, 2) & BIT_UP,
            0,
            "Releasing I should clear Up on P2"
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
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes"]);
        handle_key_pressed(&mut nes, KeyCode::KeyA, &mut state, None);
        assert_eq!(state.cart_switch.filter, "a");
        handle_key_pressed(&mut nes, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
    }

    #[test]
    fn test_cart_switch_backspace_removes_char() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes"]);
        state.cart_switch.filter = "abc".to_string();
        let outcome = handle_key_pressed(&mut nes, KeyCode::Backspace, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_backspace_on_empty_filter() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut nes, KeyCode::Backspace, &mut state, None);
        assert!(state.cart_switch.filter.is_empty());
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_enter_returns_switch_cartridge() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["game.nes", "other.nes"]);
        state.cart_switch.selection = 0;
        let outcome = handle_key_pressed(&mut nes, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::SwitchCartridge("game.nes".to_string()));
        assert!(!state.cart_switch.open, "dialog should close after Enter");
    }

    #[test]
    fn test_cart_switch_enter_with_no_entries_returns_continue() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&[]);
        let outcome = handle_key_pressed(&mut nes, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_escape_closes_dialog() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes"]);
        handle_key_pressed(&mut nes, KeyCode::Escape, &mut state, None);
        assert!(!state.cart_switch.open);
    }

    #[test]
    fn test_cart_switch_arrow_down_wraps() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 1;
        handle_key_pressed(&mut nes, KeyCode::ArrowDown, &mut state, None);
        assert_eq!(state.cart_switch.selection, 0, "should wrap to first");
    }

    #[test]
    fn test_cart_switch_arrow_up_wraps() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 0;
        handle_key_pressed(&mut nes, KeyCode::ArrowUp, &mut state, None);
        assert_eq!(state.cart_switch.selection, 1, "should wrap to last");
    }

    #[test]
    fn test_cart_switch_typing_filters_entries() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes", "gamma.nes"]);
        // Type 'b' — only "beta.nes" matches (fuzzy on filename)
        handle_key_pressed(&mut nes, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.visible_count(), 1);
        assert_eq!(state.cart_switch.selected_entry(), Some("beta.nes"));
    }

    #[test]
    fn test_cart_switch_keys_dont_trigger_controller() {
        let mut nes = make_nes_with_cartridge();
        let mut state = make_cart_switch_state(&["a.nes"]);
        // Pressing W while cart switch is open should NOT set Up on P1
        handle_key_pressed(&mut nes, KeyCode::KeyW, &mut state, None);
        assert_eq!(
            buttons(&nes, 1) & BIT_UP,
            0,
            "W should not control NES when dialog is open"
        );
    }

    // ── Review fix: Escape returns CloseCartridgeSwitch ───────────────────────

    #[test]
    fn test_cart_switch_escape_returns_close_outcome() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut nes, KeyCode::Escape, &mut state, None);
        assert_eq!(
            outcome,
            KeyOutcome::CloseCartridgeSwitch,
            "Escape should return CloseCartridgeSwitch so event loop can restore pause"
        );
    }

    // ── Review fix: underscore via Shift+Minus ────────────────────────────────

    #[test]
    fn test_cart_switch_shift_minus_types_underscore() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a_b.nes"]);
        state.modifiers = ModifiersState::SHIFT;
        handle_key_pressed(&mut nes, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "_",
            "Shift+Minus should type underscore"
        );
    }

    #[test]
    fn test_cart_switch_minus_without_shift_types_dash() {
        let mut nes = make_nes();
        let mut state = make_cart_switch_state(&["a-b.nes"]);
        handle_key_pressed(&mut nes, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "-",
            "Minus without Shift should type dash"
        );
    }
}
