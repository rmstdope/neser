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
/// determined by [`keyboard_target_ports`].  P1 keys (WASD etc.) are sent to the first
/// port in `ports` (ports.first()); P2-specific keys (IJKL etc.) are sent to port 2
/// only if 2 is in `ports`.
pub(super) fn handle_controller_key(
    console: &mut Console,
    key_code: KeyCode,
    pressed: bool,
    ports: &[u8],
) {
    let Console::Nes(nes) = console else {
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
