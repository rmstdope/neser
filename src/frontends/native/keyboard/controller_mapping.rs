//! Keyboard-to-controller button mapping for the native frontend.
//!
//! Every binding lives in [`crate::platform::key_bindings`], the table the web shell reads
//! too; this module only turns a winit [`KeyCode`] into that table's [`Key`] and hands the
//! rows to the console. `handle_controller_key` routes NES keys to the configured ports;
//! [`pad_button_id`] gives the button of the single-joypad consoles (GB, GBA, SNES).

use crate::nes::input::button_from_id as nes_button;
use crate::platform::emulator::{Console, SystemType};
use crate::platform::key_bindings::{Input, Key, KeyBinding, Player, Shell, bindings_for};
use winit::keyboard::KeyCode;

/// The winit name of every key the table can bind: a name mapping, never a binding.
pub(super) const WINIT_KEYS: &[(KeyCode, Key)] = &[
    (KeyCode::Digit0, Key::Digit0),
    (KeyCode::Digit1, Key::Digit1),
    (KeyCode::Digit2, Key::Digit2),
    (KeyCode::Digit3, Key::Digit3),
    (KeyCode::Digit4, Key::Digit4),
    (KeyCode::Digit5, Key::Digit5),
    (KeyCode::Digit6, Key::Digit6),
    (KeyCode::Digit7, Key::Digit7),
    (KeyCode::Digit8, Key::Digit8),
    (KeyCode::Digit9, Key::Digit9),
    (KeyCode::KeyA, Key::KeyA),
    (KeyCode::KeyB, Key::KeyB),
    (KeyCode::KeyC, Key::KeyC),
    (KeyCode::KeyD, Key::KeyD),
    (KeyCode::KeyE, Key::KeyE),
    (KeyCode::KeyF, Key::KeyF),
    (KeyCode::KeyG, Key::KeyG),
    (KeyCode::KeyI, Key::KeyI),
    (KeyCode::KeyJ, Key::KeyJ),
    (KeyCode::KeyK, Key::KeyK),
    (KeyCode::KeyL, Key::KeyL),
    (KeyCode::KeyM, Key::KeyM),
    (KeyCode::KeyO, Key::KeyO),
    (KeyCode::KeyP, Key::KeyP),
    (KeyCode::KeyQ, Key::KeyQ),
    (KeyCode::KeyR, Key::KeyR),
    (KeyCode::KeyS, Key::KeyS),
    (KeyCode::KeyT, Key::KeyT),
    (KeyCode::KeyU, Key::KeyU),
    (KeyCode::KeyV, Key::KeyV),
    (KeyCode::KeyW, Key::KeyW),
    (KeyCode::KeyX, Key::KeyX),
    (KeyCode::KeyY, Key::KeyY),
    (KeyCode::KeyZ, Key::KeyZ),
    (KeyCode::Minus, Key::Minus),
    (KeyCode::Comma, Key::Comma),
    (KeyCode::Period, Key::Period),
    (KeyCode::ArrowUp, Key::ArrowUp),
    (KeyCode::ArrowDown, Key::ArrowDown),
    (KeyCode::ArrowLeft, Key::ArrowLeft),
    (KeyCode::ArrowRight, Key::ArrowRight),
];

/// The table's name for a winit key, if the table can bind it.
fn key_from_winit(key_code: KeyCode) -> Option<Key> {
    WINIT_KEYS
        .iter()
        .find(|&&(code, _)| code == key_code)
        .map(|&(_, key)| key)
}

/// The winit key for one of the table's keys.
#[cfg(test)]
pub(super) fn winit_key(key: Key) -> KeyCode {
    WINIT_KEYS
        .iter()
        .find(|&&(_, k)| k == key)
        .map(|&(code, _)| code)
        .expect("every Key has a winit KeyCode")
}

/// The desktop rows for `key_code` on `console`, in the order they are tried.
pub(super) fn desktop_rows(
    console: SystemType,
    key_code: KeyCode,
) -> impl Iterator<Item = &'static KeyBinding> {
    key_from_winit(key_code)
        .into_iter()
        .flat_map(move |key| bindings_for(Shell::Desktop, console, key))
}

/// The platform button id `key_code` presses on the single joypad of `console` (GB, GBA,
/// SNES), if any.
pub(super) fn pad_button_id(console: SystemType, key_code: KeyCode) -> Option<u8> {
    desktop_rows(console, key_code).find_map(|b| match b.input {
        Input::Pad(Player::One, button) => Some(button.id()),
        _ => None,
    })
}

// ── Controller key mapping ────────────────────────────────────────────────────

/// Applies the NES rows bound to a [`KeyCode`]: presses or releases its NES, SNES or
/// Power Pad buttons, or the Vs. System coin and service inputs.
///
/// `ports` is the set of NES ports keyboard input should be routed to,
/// determined by [`super::keyboard_target_ports`]. Player 1 keys (WASD etc.) are sent to the
/// first port in `ports`; player 2 keys (IJKL etc.) to the second, if present.
pub(super) fn handle_controller_key(
    console: &mut Console,
    key_code: KeyCode,
    pressed: bool,
    ports: &[u8],
) {
    apply_nes_rows(
        console,
        desktop_rows(SystemType::Nes, key_code),
        pressed,
        ports,
    );
}

/// Tries `rows` in order and stops at the first one the plugged device accepts.
pub(super) fn apply_nes_rows<'a>(
    console: &mut Console,
    rows: impl IntoIterator<Item = &'a KeyBinding>,
    pressed: bool,
    ports: &[u8],
) {
    let Some(nes) = console.as_nes_mut() else {
        return;
    };
    let port_of = |player| match player {
        Player::One => ports.first().copied(),
        Player::Two => ports.get(1).copied(),
    };
    for row in rows {
        let accepted = match row.input {
            Input::PowerPad(player, button) => {
                let Some(port) = port_of(player) else { return };
                nes.set_power_pad_button(port, button, pressed)
            }
            Input::SnesPadOnNes(player, button) => {
                let Some(port) = port_of(player) else { return };
                nes.set_snes_button(port, button.on_nes_snes_pad(), pressed)
            }
            Input::Pad(player, button) => {
                let Some(port) = port_of(player) else { return };
                if let Some(button) = nes_button(button.id()) {
                    nes.set_button(port, button, pressed);
                }
                true
            }
            // A press inserts one coin; the core times the coin pulse, so the release is moot.
            Input::VsCoin => {
                if pressed {
                    nes.insert_vs_coin(0);
                }
                true
            }
            Input::VsService => {
                nes.set_vs_service_button(pressed);
                true
            }
            Input::SuperScopeTurbo | Input::SuperScopePause => false,
        };
        if accepted {
            return;
        }
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
        let snes = |key| super::pad_button_id(crate::platform::emulator::SystemType::Snes, key);
        assert_eq!(snes(KeyCode::KeyT), Some(0)); // A
        assert_eq!(snes(KeyCode::KeyR), Some(1)); // B
        assert_eq!(snes(KeyCode::Digit4), Some(2)); // Select
        assert_eq!(snes(KeyCode::Digit5), Some(3)); // Start
        // SNES additions.
        assert_eq!(snes(KeyCode::KeyQ), Some(8)); // L
        assert_eq!(snes(KeyCode::KeyE), Some(9)); // R
        assert_eq!(snes(KeyCode::KeyY), Some(10)); // X
        assert_eq!(snes(KeyCode::KeyG), Some(11)); // Y
        assert_eq!(snes(KeyCode::F1), None);
    }

    // ── The shared key table (nr-tlf) ─────────────────────────────────────────

    use crate::nes::input::PowerPadButton;
    use crate::platform::emulator::SystemType;
    use crate::platform::key_bindings::{
        Input, Key, KeyBinding, PadButton, Player, Shell, Shells, bindings_of,
    };

    fn row(key: Key, input: Input) -> KeyBinding {
        KeyBinding {
            console: SystemType::Nes,
            key,
            input,
            shells: Shells::Both,
        }
    }

    /// The NES dispatch does what the rows say, whatever key they hang off: a Power Pad row
    /// a joypad rejects falls through to the next row.
    #[test]
    fn nes_keys_follow_the_rows_they_are_given() {
        let rows = [
            row(Key::KeyZ, Input::PowerPad(Player::Two, PowerPadButton::One)),
            row(Key::KeyZ, Input::Pad(Player::Two, PadButton::Start)),
        ];
        let mut console = make_nes_console();
        super::apply_nes_rows(&mut console, &rows, true, &[1, 2]);
        assert_ne!(console.get_joypad_button_states(2) & BIT_START, 0);
        assert_eq!(console.get_joypad_button_states(1), 0);
    }

    /// Every key the table binds is one the desktop can receive.
    #[test]
    fn every_table_key_has_a_winit_key() {
        for b in crate::platform::key_bindings::KEY_BINDINGS {
            assert!(
                super::WINIT_KEYS.iter().any(|&(_, key)| key == b.key),
                "{:?} has no winit KeyCode",
                b.key
            );
        }
    }

    /// Every desktop NES joypad row presses its button on its player's port.
    #[test]
    fn every_desktop_nes_pad_row_presses_its_button() {
        for b in bindings_of(Shell::Desktop).filter(|b| b.console == SystemType::Nes) {
            let Input::Pad(player, button) = b.input else {
                continue;
            };
            let mut console = make_nes_console();
            let mut state = make_state();
            handle_key_pressed(&mut console, super::winit_key(b.key), &mut state, None);
            let port = if player == Player::One { 1 } else { 2 };
            assert_ne!(
                console.get_joypad_button_states(port) & (1 << button.id()),
                0,
                "{b:?}"
            );
        }
    }

    // ── VS System coin key ────────────────────────────────────────────────────

    /// A VS System console with a NOP cartridge, so that frames run.
    fn make_vs_console() -> crate::platform::emulator::Console {
        use crate::nes::console::{Config, ExpansionPort, Nes, NesConfig};
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config {
                nes: NesConfig {
                    expansion_port: ExpansionPort::VsSystem,
                    ..Default::default()
                },
                ..Config::default()
            },
        ));
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        prg_rom[0x7FFD] = 0x80; // reset vector $8000
        nes.insert_cartridge(crate::nes::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        ));
        nes.reset(true);
        crate::platform::emulator::Console::Nes(Box::new(nes))
    }

    /// Runs `frames` frames and counts those in which $4016 reported coin slot 1 (bit 5).
    fn frames_with_coin_slot1(
        console: &mut crate::platform::emulator::Console,
        frames: usize,
    ) -> usize {
        let nes = console.as_nes_mut().expect("NES console");
        let mut with_coin = 0;
        for _ in 0..frames {
            if nes.bus().borrow_mut().read(0x4016, false) & 0x20 != 0 {
                with_coin += 1;
            }
            while !nes.is_ready_to_render() {
                nes.run_cpu_tick();
            }
            nes.clear_ready_to_render();
        }
        with_coin
    }

    /// nr-use: a person holds the coin key (6) for many frames, and Vs. Duck Hunt reads a
    /// coin line held ten frames or more as a jammed coin. Holding 6 must still insert
    /// exactly one coin: a four-frame pulse, as Mesen2 gives.
    #[test]
    fn test_holding_6_pulses_the_vs_coin_line_for_four_frames() {
        let mut console = make_vs_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit6, &mut state, None);
        assert_eq!(frames_with_coin_slot1(&mut console, 30), 4);
    }

    /// A tap of 6 released before the next frame still inserts a coin.
    #[test]
    fn test_tapping_6_pulses_the_vs_coin_line_for_four_frames() {
        let mut console = make_vs_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit6, &mut state, None);
        handle_key_released(&mut console, KeyCode::Digit6, 0, false);
        assert_eq!(frames_with_coin_slot1(&mut console, 30), 4);
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
