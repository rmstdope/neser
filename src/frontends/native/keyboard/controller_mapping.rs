//! Keyboard-to-controller button mapping for the native frontend.
//!
//! Maps winit `KeyCode`s to NES/SNES/Power Pad/GB/GBA button presses and
//! releases. `handle_controller_key` routes NES/SNES/Power Pad keys to the
//! configured ports; the `*_key_to_button_id` functions map keys for the
//! single-joypad consoles (GB, GBA, SNES).

use crate::nes::input::{Button, PowerPadButton, SnesButton};
use crate::platform::emulator::Console;
use winit::keyboard::KeyCode;

/// Maps a key code to a Game Boy button ID (0=A,1=B,2=Select,3=Start,4=Up,5=Down,6=Left,7=Right).
///
/// Uses the same physical-position layout as the NES P1 keys so that
/// players feel at home: WASD for D-pad, T=A, R=B, 4=Select, 5=Start.
/// Arrow keys are also mapped to the D-pad for convenience.
pub(super) fn gameboy_key_to_button_id(key_code: KeyCode) -> Option<u8> {
    use Button::{A, B, Down, Left, Right, Select, Start, Up};
    match key_code {
        KeyCode::KeyT => Some(A as u8),
        KeyCode::KeyR => Some(B as u8),
        KeyCode::Digit4 => Some(Select as u8),
        KeyCode::Digit5 => Some(Start as u8),
        KeyCode::KeyW | KeyCode::ArrowUp => Some(Up as u8),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Down as u8),
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Left as u8),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Right as u8),
        _ => None,
    }
}

/// Maps a key code to a GBA button ID.
///
/// Extends the Game Boy/NES-style keyboard layout with Q=L and E=R,
/// matching the native NES/SNES shoulder-button positions.
pub(super) fn gba_key_to_button_id(key_code: KeyCode) -> Option<u8> {
    match key_code {
        KeyCode::KeyQ => Some(8), // L
        KeyCode::KeyE => Some(9), // R
        _ => gameboy_key_to_button_id(key_code),
    }
}

/// Maps a key code to a SNES button ID.
///
/// Extends the Game Boy/NES-style keyboard layout with the SNES face and
/// shoulder buttons. Button IDs follow the platform convention plus the
/// SNES-only `X`/`Y` (see [`crate::snes::input::button_from_id`]):
/// `0=A, 1=B, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right, 8=L, 9=R,
/// 10=X, 11=Y`.
pub(super) fn snes_key_to_button_id(key_code: KeyCode) -> Option<u8> {
    match key_code {
        KeyCode::KeyQ => Some(8),  // L
        KeyCode::KeyE => Some(9),  // R
        KeyCode::KeyY => Some(10), // X
        KeyCode::KeyG => Some(11), // Y
        _ => gameboy_key_to_button_id(key_code),
    }
}

// ── Controller key mapping ────────────────────────────────────────────────────

/// Maps a [`KeyCode`] to NES/SNES/Power Pad button presses or releases.
///
/// `ports` is the set of NES ports keyboard input should be routed to,
/// determined by [`keyboard_target_ports`].  P1 keys (WASD etc.) are sent to the
/// first port in `ports` (`ports.first()`); P2-specific keys (IJKL etc.) are sent
/// to the second port in `ports` (`ports.get(1)`), if present.
pub(super) fn handle_controller_key(
    console: &mut Console,
    key_code: KeyCode,
    pressed: bool,
    ports: &[u8],
) {
    let Some(nes) = console.as_nes_mut() else {
        return;
    };
    match key_code {
        // ── Player 1: 1/2/3 → Power Pad buttons ──────────────────────────
        KeyCode::Digit1 => pp_p1(nes, PowerPadButton::One, pressed, ports),
        KeyCode::Digit2 => pp_p1(nes, PowerPadButton::Two, pressed, ports),
        KeyCode::Digit3 => pp_p1(nes, PowerPadButton::Three, pressed, ports),

        // ── Player 1: QWEASD (D-pad / SNES L/R / Power Pad) ──────────────
        KeyCode::KeyQ => pp_or_snes_p1(nes, PowerPadButton::Four, SnesButton::L, pressed, ports),
        KeyCode::KeyW => pp_or_btn_or_snes_p1(
            nes,
            PowerPadButton::Five,
            Button::Up,
            SnesButton::Up,
            pressed,
            ports,
        ),
        KeyCode::KeyE => pp_or_snes_p1(nes, PowerPadButton::Six, SnesButton::R, pressed, ports),
        KeyCode::KeyA => pp_or_btn_or_snes_p1(
            nes,
            PowerPadButton::Seven,
            Button::Left,
            SnesButton::Left,
            pressed,
            ports,
        ),
        KeyCode::KeyS => pp_or_btn_or_snes_p1(
            nes,
            PowerPadButton::Eight,
            Button::Down,
            SnesButton::Down,
            pressed,
            ports,
        ),
        KeyCode::KeyD => pp_or_btn_or_snes_p1(
            nes,
            PowerPadButton::Nine,
            Button::Right,
            SnesButton::Right,
            pressed,
            ports,
        ),

        // ── Player 1: ZXC → Power Pad ────────────────────────────────────
        KeyCode::KeyZ => pp_p1(nes, PowerPadButton::Ten, pressed, ports),
        KeyCode::KeyX => pp_p1(nes, PowerPadButton::Eleven, pressed, ports),
        KeyCode::KeyC => pp_p1(nes, PowerPadButton::Twelve, pressed, ports),

        // ── Player 1: T/R = A/B (joypad or SNES Y/X) ─────────────────────
        KeyCode::KeyT => btn_or_snes_p1(nes, Button::A, SnesButton::Y, pressed, ports),
        KeyCode::KeyR => btn_or_snes_p1(nes, Button::B, SnesButton::X, pressed, ports),

        // ── Player 1: F/G = SNES B/A only ────────────────────────────────
        KeyCode::KeyF => snes_p1(nes, SnesButton::B, pressed, ports),
        KeyCode::KeyG => snes_p1(nes, SnesButton::A, pressed, ports),

        // ── Player 1: 4/5 = Select/Start ─────────────────────────────────
        KeyCode::Digit4 => btn_or_snes_p1(nes, Button::Select, SnesButton::Select, pressed, ports),
        KeyCode::Digit5 => btn_or_snes_p1(nes, Button::Start, SnesButton::Start, pressed, ports),

        // ── Player 2: 7/8 → Power Pad; 9 = PP3/Select; 0 = Start ─────────
        KeyCode::Digit7 => pp_p2(nes, PowerPadButton::One, pressed, ports),
        KeyCode::Digit8 => pp_p2(nes, PowerPadButton::Two, pressed, ports),
        KeyCode::Digit9 => pp_or_btn_p2(nes, PowerPadButton::Three, Button::Select, pressed, ports),
        KeyCode::Digit0 => btn_p2(nes, Button::Start, pressed, ports),

        // ── Player 2: UIOJKL M,. = D-pad / Power Pad ─────────────────────
        KeyCode::KeyU => pp_p2(nes, PowerPadButton::Four, pressed, ports),
        KeyCode::KeyI => pp_or_btn_p2(nes, PowerPadButton::Five, Button::Up, pressed, ports),
        KeyCode::KeyO => pp_or_btn_p2(nes, PowerPadButton::Six, Button::A, pressed, ports),
        KeyCode::KeyJ => pp_or_btn_p2(nes, PowerPadButton::Seven, Button::Left, pressed, ports),
        KeyCode::KeyK => pp_or_btn_p2(nes, PowerPadButton::Eight, Button::Down, pressed, ports),
        KeyCode::KeyL => pp_or_btn_p2(nes, PowerPadButton::Nine, Button::Right, pressed, ports),
        KeyCode::KeyM => pp_p2(nes, PowerPadButton::Ten, pressed, ports),
        KeyCode::Comma => pp_p2(nes, PowerPadButton::Eleven, pressed, ports),
        KeyCode::Period => pp_p2(nes, PowerPadButton::Twelve, pressed, ports),
        KeyCode::KeyP => btn_p2(nes, Button::B, pressed, ports),

        // ── VS System: coin insert / service button ──────────────────────
        KeyCode::Digit6 => nes.set_vs_coin_insert(0, pressed),
        KeyCode::Minus => nes.set_vs_service_button(pressed),

        _ => {}
    }
}

// ── Player-1 button helpers (route to the primary keyboard port) ───────────────────────────────────────

fn pp_p1(nes: &mut crate::nes::console::Nes, pp: PowerPadButton, pressed: bool, ports: &[u8]) {
    if let Some(&port) = ports.first() {
        nes.set_power_pad_button(port, pp, pressed);
    }
}

fn snes_p1(nes: &mut crate::nes::console::Nes, snes: SnesButton, pressed: bool, ports: &[u8]) {
    if let Some(&port) = ports.first() {
        nes.set_snes_button(port, snes, pressed);
    }
}

fn btn_or_snes_p1(
    nes: &mut crate::nes::console::Nes,
    btn: Button,
    snes: SnesButton,
    pressed: bool,
    ports: &[u8],
) {
    if let Some(&port) = ports.first()
        && !nes.set_snes_button(port, snes, pressed)
    {
        nes.set_button(port, btn, pressed);
    }
}

fn pp_or_snes_p1(
    nes: &mut crate::nes::console::Nes,
    pp: PowerPadButton,
    snes: SnesButton,
    pressed: bool,
    ports: &[u8],
) {
    if let Some(&port) = ports.first()
        && !nes.set_power_pad_button(port, pp, pressed)
    {
        nes.set_snes_button(port, snes, pressed);
    }
}

fn pp_or_btn_or_snes_p1(
    nes: &mut crate::nes::console::Nes,
    pp: PowerPadButton,
    btn: Button,
    snes: SnesButton,
    pressed: bool,
    ports: &[u8],
) {
    if let Some(&port) = ports.first()
        && !nes.set_power_pad_button(port, pp, pressed)
        && !nes.set_snes_button(port, snes, pressed)
    {
        nes.set_button(port, btn, pressed);
    }
}

// ── Player-2-only button helpers ─────────────────────────────────────────────

fn btn_p2(nes: &mut crate::nes::console::Nes, btn: Button, pressed: bool, ports: &[u8]) {
    if let Some(&port) = ports.get(1) {
        nes.set_button(port, btn, pressed);
    }
}

fn pp_p2(nes: &mut crate::nes::console::Nes, pp: PowerPadButton, pressed: bool, ports: &[u8]) {
    if let Some(&port) = ports.get(1) {
        nes.set_power_pad_button(port, pp, pressed);
    }
}

fn pp_or_btn_p2(
    nes: &mut crate::nes::console::Nes,
    pp: PowerPadButton,
    btn: Button,
    pressed: bool,
    ports: &[u8],
) {
    if let Some(&port) = ports.get(1)
        && !nes.set_power_pad_button(port, pp, pressed)
    {
        nes.set_button(port, btn, pressed);
    }
}

#[cfg(test)]
mod tests {

    use crate::frontends::native::app_state::NativeAppState;
    use crate::frontends::native::keyboard::test_support::*;
    use crate::frontends::native::keyboard::{handle_key_pressed, handle_key_released};

    use winit::keyboard::KeyCode;

    #[test]
    fn snes_key_mapping_covers_face_and_shoulder_buttons() {
        // Base GB-style keys still map.
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyT), Some(0)); // A
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyR), Some(1)); // B
        assert_eq!(super::snes_key_to_button_id(KeyCode::Digit4), Some(2)); // Select
        assert_eq!(super::snes_key_to_button_id(KeyCode::Digit5), Some(3)); // Start
        // SNES additions.
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyQ), Some(8)); // L
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyE), Some(9)); // R
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyY), Some(10)); // X
        assert_eq!(super::snes_key_to_button_id(KeyCode::KeyG), Some(11)); // Y
        assert_eq!(super::snes_key_to_button_id(KeyCode::F1), None);
    }

    // ── Player 1 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p1_w_sets_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should set Up on P1"
        );
    }

    #[test]
    fn test_p1_a_sets_left() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyA, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_LEFT,
            0,
            "A should set Left on P1"
        );
    }

    #[test]
    fn test_p1_s_sets_down() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyS, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_DOWN,
            0,
            "S should set Down on P1"
        );
    }

    #[test]
    fn test_p1_d_sets_right() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_RIGHT,
            0,
            "D should set Right on P1"
        );
    }

    #[test]
    fn test_p1_t_sets_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_A,
            0,
            "T should set A on P1"
        );
    }

    #[test]
    fn test_p1_r_sets_b() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_B,
            0,
            "R should set B on P1"
        );
    }

    #[test]
    fn test_p1_num4_sets_select() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit4, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_SELECT,
            0,
            "4 should set Select on P1"
        );
    }

    #[test]
    fn test_p1_num5_sets_start() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit5, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_START,
            0,
            "5 should set Start on P1"
        );
    }

    // ── Player 1 — key release clears button ──────────────────────────────────

    #[test]
    fn test_p1_w_released_clears_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(console.get_joypad_button_states(1) & BIT_UP, 0);
        handle_key_released(&mut console, KeyCode::KeyW, 0, false);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "Releasing W should clear Up"
        );
    }

    #[test]
    fn test_p1_t_released_clears_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        handle_key_released(&mut console, KeyCode::KeyT, 0, false);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_A,
            0,
            "Releasing T should clear A"
        );
    }

    // ── Player 2 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p2_i_sets_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I should set Up on P2"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "I should not affect P1"
        );
    }

    #[test]
    fn test_p2_j_sets_left() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyJ, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_LEFT,
            0,
            "J should set Left on P2"
        );
    }

    #[test]
    fn test_p2_k_sets_down() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyK, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_DOWN,
            0,
            "K should set Down on P2"
        );
    }

    #[test]
    fn test_p2_l_sets_right() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyL, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_RIGHT,
            0,
            "L should set Right on P2"
        );
    }

    #[test]
    fn test_p2_o_sets_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyO, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_A,
            0,
            "O should set A on P2"
        );
    }

    #[test]
    fn test_p2_p_sets_b() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyP, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_B,
            0,
            "P should set B on P2"
        );
    }

    #[test]
    fn test_p2_num9_sets_select() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit9, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_SELECT,
            0,
            "9 should set Select on P2"
        );
    }

    #[test]
    fn test_p2_num0_sets_start() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit0, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_START,
            0,
            "0 should set Start on P2"
        );
    }

    // ── P1 keys target port 1 only (not port 2) when no gamepad ─────────────

    #[test]
    fn test_w_targets_port1_only_when_no_gamepad() {
        let mut console = make_nes_console();
        let mut state = make_state(); // gamepad_count = 0
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should set Up on P1"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT set Up on P2 (port 2 has dedicated IJKL keys)"
        );
    }

    #[test]
    fn test_s_targets_port1_only_when_no_gamepad() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyS, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_DOWN,
            0,
            "S should set Down on P1"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_DOWN,
            0,
            "S should NOT set Down on P2"
        );
    }

    // ── Player 2 key release ──────────────────────────────────────────────────

    #[test]
    fn test_p2_i_released_clears_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        handle_key_released(&mut console, KeyCode::KeyI, 0, false);
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "Releasing I should clear Up on P2"
        );
    }

    // ── Gamepad-count-aware keyboard routing ──────────────────────────────────

    #[test]
    fn test_wasd_routes_to_port2_only_when_one_gamepad() {
        // Given: one gamepad connected (port 1 owned by gamepad)
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 1,
            ..NativeAppState::default()
        };
        // When: W (Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        // Then: port 2 gets Up; port 1 does NOT (gamepad owns port 1)
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should set port 2 Up when one gamepad is connected"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT set port 1 Up when one gamepad is connected"
        );
    }

    #[test]
    fn test_wasd_disabled_when_two_gamepads() {
        // Given: two gamepads connected (both ports owned by gamepads)
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 2,
            ..NativeAppState::default()
        };
        // When: W (Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        // Then: neither port gets input
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT set port 1 Up when two gamepads are connected"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT set port 2 Up when two gamepads are connected"
        );
    }

    #[test]
    fn test_ijkl_disabled_when_two_gamepads() {
        // Given: two gamepads connected
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 2,
            ..NativeAppState::default()
        };
        // When: I (P2 Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        // Then: port 2 gets no input
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I (P2 Up) should be disabled when two gamepads are connected"
        );
    }

    #[test]
    fn test_ijkl_disabled_when_one_gamepad() {
        // With 1 gamepad, port 1 is owned by the gamepad.  The keyboard player
        // on port 2 should use WASD (the P1 key set, which shifts to track
        // ports.first()).  The P2-specific IJKL keys should be disabled because
        // there is no dedicated keyboard "player 2" slot.
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 1,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I (P2 Up) should be disabled when one gamepad is connected; use WASD instead"
        );
    }

    #[test]
    fn test_help_overlay_port2_shows_wasd_not_ijkl_when_one_gamepad() {
        // When 1 gamepad is connected the keyboard player is on port 2 using
        // the WASD key set (P1 keys shift to ports.first() = port 2).
        // The IJKL keys do nothing, so the help text must NOT list them for
        // port 2 and MUST list WASD for port 2.
        let state = crate::frontends::native::app_state::NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            ..Default::default()
        };
        let nes = make_nes_console();
        let text = state
            .overlay_text(&nes, None)
            .expect("help overlay must be present");
        assert!(
            text.contains("W/A/S/D"),
            "help overlay must list W/A/S/D for port 2 with 1 gamepad; got:\n{text}"
        );
        assert!(
            !text.contains("I/J/K/L"),
            "help overlay must NOT list I/J/K/L when 1 gamepad connected; got:\n{text}"
        );
    }

    // ── Four Score: keyboard routes P1 keys to port 3 with 2 gamepads ────────

    #[test]
    fn test_wasd_routes_to_port3_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(3) & BIT_UP,
            0,
            "W should set Up on port 3 with four-score and 2 gamepads"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT affect port 1 (owned by gamepad)"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT affect port 2 (owned by gamepad)"
        );
    }

    #[test]
    fn test_ijkl_routes_to_port4_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(4) & BIT_UP,
            0,
            "I should set Up on port 4 with four-score and 2 gamepads"
        );
    }

    #[test]
    fn test_p2_start_routes_to_port4_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::Digit0, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(4) & BIT_START,
            0,
            "0 should set Start on port 4 with four-score and 2 gamepads"
        );
    }

    #[test]
    fn test_key_release_works_on_port3_with_four_score() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(console.get_joypad_button_states(3) & BIT_UP, 0);
        handle_key_released(&mut console, KeyCode::KeyW, 2, true);
        assert_eq!(
            console.get_joypad_button_states(3) & BIT_UP,
            0,
            "Releasing W should clear Up on port 3"
        );
    }
}
