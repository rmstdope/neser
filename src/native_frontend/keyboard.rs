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
        handle_cartridge_switch_key(key_code, app_state);
        return KeyOutcome::Continue;
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
        KeyCode::KeyO => {
            app_state.cart_switch.open = true;
            KeyOutcome::Continue
        }
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
        KeyCode::Escape => app_state.mouse_grabbed = false,
        KeyCode::Space => app_state.paused = !app_state.paused,
        KeyCode::KeyH => app_state.help_overlay_visible = !app_state.help_overlay_visible,
        KeyCode::F2 => adjust_volume(audio, 0.1),
        KeyCode::F3 => adjust_volume(audio, -0.1),
        KeyCode::F4 => return KeyOutcome::CycleShader,
        KeyCode::F5 => toggle_debugger(app_state),
        KeyCode::F6 => {
            // TODO(#1772): port save_state_to_disk from SDL2 frontend
            crate::debugging::log_info(
                "Save state: not yet implemented in native frontend".to_string(),
            );
        }
        KeyCode::F7 => {
            // TODO(#1772): port load_state_from_disk from SDL2 frontend
            crate::debugging::log_info(
                "Load state: not yet implemented in native frontend".to_string(),
            );
        }
        KeyCode::F10 => step_debugger(nes, app_state),
        KeyCode::F11 => step_debugger(nes, app_state),
        _ => handle_controller_key(nes, key_code, true),
    }

    KeyOutcome::Continue
}

fn toggle_debugger(app_state: &mut NativeAppState) {
    let open = !app_state.debugger_open;
    app_state.debugger_open = open;
    app_state.paused = open;
}

fn step_debugger(nes: &mut Nes, app_state: &mut NativeAppState) {
    enter_debugger_paused(app_state);
    nes.run_cpu_tick();
}

fn enter_debugger_paused(app_state: &mut NativeAppState) {
    app_state.paused = true;
    app_state.debugger_open = true;
}

fn adjust_volume(audio: Option<&dyn NesAudio>, delta: f32) {
    if let Some(audio) = audio {
        audio.set_volume(audio.get_volume() + delta);
    }
}

// ── Controller key mapping ────────────────────────────────────────────────────

/// Maps a [`KeyCode`] to NES/SNES/Power Pad button presses or releases for
/// both player ports simultaneously.  Shared between press and release paths.
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
    nes.set_power_pad_button(1, pp, pressed);
    nes.set_power_pad_button(2, pp, pressed);
}

fn snes_both(nes: &mut Nes, snes: SnesButton, pressed: bool) {
    nes.set_snes_button(1, snes, pressed);
    nes.set_snes_button(2, snes, pressed);
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

fn handle_cartridge_switch_key(key_code: KeyCode, app_state: &mut NativeAppState) {
    match key_code {
        KeyCode::Escape => app_state.cart_switch.close(),
        KeyCode::ArrowDown => {
            if app_state.cart_switch.selection + 1 < app_state.cart_switch.entries.len() {
                app_state.cart_switch.selection += 1;
            }
        }
        KeyCode::ArrowUp => {
            app_state.cart_switch.selection = app_state.cart_switch.selection.saturating_sub(1);
        }
        _ => {}
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
        NativeAppState::new()
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
    fn test_f5_opens_debugger_when_closed() {
        let mut nes = make_nes();
        let mut state = make_state();
        handle_key_pressed(&mut nes, KeyCode::F5, &mut state, None);
        assert!(state.debugger_open);
        assert!(state.paused, "Opening debugger should pause emulation");
    }

    #[test]
    fn test_f5_closes_debugger_when_open() {
        let mut nes = make_nes();
        let mut state = make_state();
        state.debugger_open = true;
        state.paused = true;
        handle_key_pressed(&mut nes, KeyCode::F5, &mut state, None);
        assert!(!state.debugger_open);
        assert!(!state.paused, "Closing debugger should resume emulation");
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
}
