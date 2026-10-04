//! Which keyboard key drives which console input: the one table both shells read.
//!
//! The native shell looks every key up here (`frontends::native::keyboard`), and the web
//! shell reads the same rows through the wasm binding `key_binding_table`. A binding is
//! therefore one row, and a binding that only one shell has says so in its row
//! ([`Shells::DesktopOnly`] / [`Shells::WebOnly`]); the tests below pin the full list of
//! those, so a one-shell binding is always a decision someone wrote down.
//!
//! For one key on one console, rows apply in table order: a shell tries each row and stops
//! at the first one the plugged device accepts (a Power Pad, then an SNES pad on the NES
//! port, then the joypad; a Super Scope before the SNES pad's Select/Start). Which port a
//! [`Player`] is, and gamepad, Four Score, Zapper and Mouse routing, stay with each shell.

use crate::nes::input::{PowerPadButton, SnesButton as NesSnesButton};
use crate::platform::emulator::SystemType;

/// A key, named as the W3C `KeyboardEvent.code` (and winit's `KeyCode`) names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Minus,
    Comma,
    Period,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

impl Key {
    /// The lower-cased `KeyboardEvent.key` the web shell matches for this key on a US
    /// layout: the web reads the character typed, the desktop the physical key.
    pub const fn web_key(self) -> &'static str {
        match self {
            Key::Digit0 => "0",
            Key::Digit1 => "1",
            Key::Digit2 => "2",
            Key::Digit3 => "3",
            Key::Digit4 => "4",
            Key::Digit5 => "5",
            Key::Digit6 => "6",
            Key::Digit7 => "7",
            Key::Digit8 => "8",
            Key::Digit9 => "9",
            Key::KeyA => "a",
            Key::KeyB => "b",
            Key::KeyC => "c",
            Key::KeyD => "d",
            Key::KeyE => "e",
            Key::KeyF => "f",
            Key::KeyG => "g",
            Key::KeyI => "i",
            Key::KeyJ => "j",
            Key::KeyK => "k",
            Key::KeyL => "l",
            Key::KeyM => "m",
            Key::KeyO => "o",
            Key::KeyP => "p",
            Key::KeyQ => "q",
            Key::KeyR => "r",
            Key::KeyS => "s",
            Key::KeyT => "t",
            Key::KeyU => "u",
            Key::KeyV => "v",
            Key::KeyW => "w",
            Key::KeyX => "x",
            Key::KeyY => "y",
            Key::KeyZ => "z",
            Key::Minus => "-",
            Key::Comma => ",",
            Key::Period => ".",
            Key::ArrowUp => "arrowup",
            Key::ArrowDown => "arrowdown",
            Key::ArrowLeft => "arrowleft",
            Key::ArrowRight => "arrowright",
        }
    }
}

/// A joypad button of any console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadButton {
    A,
    B,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    L,
    R,
    X,
    Y,
}

impl PadButton {
    /// The platform button id every console's `button_from_id` and `Console::set_button`
    /// take: `0=A, 1=B, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right, 8=L, 9=R, 10=X, 11=Y`.
    pub const fn id(self) -> u8 {
        match self {
            PadButton::A => 0,
            PadButton::B => 1,
            PadButton::Select => 2,
            PadButton::Start => 3,
            PadButton::Up => 4,
            PadButton::Down => 5,
            PadButton::Left => 6,
            PadButton::Right => 7,
            PadButton::L => 8,
            PadButton::R => 9,
            PadButton::X => 10,
            PadButton::Y => 11,
        }
    }

    /// The button of an SNES controller in an NES port, whose discriminant is also the id
    /// the wasm binding `WasmNes::set_snes_button` takes.
    pub const fn on_nes_snes_pad(self) -> NesSnesButton {
        match self {
            PadButton::A => NesSnesButton::A,
            PadButton::B => NesSnesButton::B,
            PadButton::Select => NesSnesButton::Select,
            PadButton::Start => NesSnesButton::Start,
            PadButton::Up => NesSnesButton::Up,
            PadButton::Down => NesSnesButton::Down,
            PadButton::Left => NesSnesButton::Left,
            PadButton::Right => NesSnesButton::Right,
            PadButton::L => NesSnesButton::L,
            PadButton::R => NesSnesButton::R,
            PadButton::X => NesSnesButton::X,
            PadButton::Y => NesSnesButton::Y,
        }
    }
}

/// The keyboard player a key belongs to; each shell decides which port that player is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    One,
    Two,
}

/// What a key does to the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The joypad (NES, GB, GBA, SNES console).
    Pad(Player, PadButton),
    /// A Power Pad on the NES.
    PowerPad(Player, PowerPadButton),
    /// An SNES controller in an NES port.
    SnesPadOnNes(Player, PadButton),
    /// One Vs. System coin into slot 1, on the press; the core times the pulse.
    VsCoin,
    /// The Vs. System service button, held while the key is.
    VsService,
    /// A plugged Super Scope's Turbo switch, flipped on the press.
    SuperScopeTurbo,
    /// A plugged Super Scope's Pause button.
    SuperScopePause,
}

/// Which shells honour a binding. A one-shell binding is a difference somebody declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shells {
    Both,
    DesktopOnly,
    WebOnly,
}

/// One of the two shells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Desktop,
    Web,
}

impl Shells {
    /// Whether `shell` honours a binding declared for these shells.
    pub const fn include(self, shell: Shell) -> bool {
        matches!(
            (self, shell),
            (Shells::Both, _)
                | (Shells::DesktopOnly, Shell::Desktop)
                | (Shells::WebOnly, Shell::Web)
        )
    }
}

/// One key bound to one console input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub console: SystemType,
    pub key: Key,
    pub input: Input,
    pub shells: Shells,
}

/// Every keyboard binding of both shells.
pub const KEY_BINDINGS: &[KeyBinding] = &[
    // ── NES, player 1 ─────────────────────────────────────────────────────────────
    desktop(NES, Digit1, PowerPad(One, PP::One)),
    desktop(NES, Digit2, PowerPad(One, PP::Two)),
    desktop(NES, Digit3, PowerPad(One, PP::Three)),
    desktop(NES, KeyQ, PowerPad(One, PP::Four)),
    both(NES, KeyQ, SnesPadOnNes(One, L)),
    desktop(NES, KeyW, PowerPad(One, PP::Five)),
    both(NES, KeyW, SnesPadOnNes(One, Up)),
    both(NES, KeyW, Pad(One, Up)),
    desktop(NES, KeyE, PowerPad(One, PP::Six)),
    both(NES, KeyE, SnesPadOnNes(One, R)),
    desktop(NES, KeyA, PowerPad(One, PP::Seven)),
    both(NES, KeyA, SnesPadOnNes(One, Left)),
    both(NES, KeyA, Pad(One, Left)),
    desktop(NES, KeyS, PowerPad(One, PP::Eight)),
    both(NES, KeyS, SnesPadOnNes(One, Down)),
    both(NES, KeyS, Pad(One, Down)),
    desktop(NES, KeyD, PowerPad(One, PP::Nine)),
    both(NES, KeyD, SnesPadOnNes(One, Right)),
    both(NES, KeyD, Pad(One, Right)),
    desktop(NES, KeyZ, PowerPad(One, PP::Ten)),
    desktop(NES, KeyX, PowerPad(One, PP::Eleven)),
    desktop(NES, KeyC, PowerPad(One, PP::Twelve)),
    desktop(NES, KeyT, SnesPadOnNes(One, Y)),
    desktop(NES, KeyT, Pad(One, A)),
    web(NES, KeyT, SnesPadOnNes(One, A)),
    web(NES, KeyT, Pad(One, B)),
    desktop(NES, KeyR, SnesPadOnNes(One, X)),
    desktop(NES, KeyR, Pad(One, B)),
    web(NES, KeyR, SnesPadOnNes(One, B)),
    web(NES, KeyR, Pad(One, A)),
    desktop(NES, KeyF, SnesPadOnNes(One, B)),
    web(NES, KeyY, SnesPadOnNes(One, X)),
    desktop(NES, KeyG, SnesPadOnNes(One, A)),
    web(NES, KeyG, SnesPadOnNes(One, Y)),
    both(NES, Digit4, SnesPadOnNes(One, Select)),
    both(NES, Digit4, Pad(One, Select)),
    both(NES, Digit5, SnesPadOnNes(One, Start)),
    both(NES, Digit5, Pad(One, Start)),
    // ── NES, player 2 ─────────────────────────────────────────────────────────────
    desktop(NES, Digit7, PowerPad(Two, PP::One)),
    desktop(NES, Digit8, PowerPad(Two, PP::Two)),
    desktop(NES, Digit9, PowerPad(Two, PP::Three)),
    both(NES, Digit9, Pad(Two, Select)),
    both(NES, Digit0, Pad(Two, Start)),
    desktop(NES, KeyU, PowerPad(Two, PP::Four)),
    desktop(NES, KeyI, PowerPad(Two, PP::Five)),
    both(NES, KeyI, Pad(Two, Up)),
    desktop(NES, KeyO, PowerPad(Two, PP::Six)),
    both(NES, KeyO, Pad(Two, A)),
    desktop(NES, KeyJ, PowerPad(Two, PP::Seven)),
    both(NES, KeyJ, Pad(Two, Left)),
    desktop(NES, KeyK, PowerPad(Two, PP::Eight)),
    both(NES, KeyK, Pad(Two, Down)),
    desktop(NES, KeyL, PowerPad(Two, PP::Nine)),
    both(NES, KeyL, Pad(Two, Right)),
    desktop(NES, KeyM, PowerPad(Two, PP::Ten)),
    desktop(NES, Comma, PowerPad(Two, PP::Eleven)),
    desktop(NES, Period, PowerPad(Two, PP::Twelve)),
    both(NES, KeyP, Pad(Two, B)),
    // ── NES, Vs. System ───────────────────────────────────────────────────────────
    both(NES, Digit6, VsCoin),
    desktop(NES, Minus, VsService),
    // ── Game Boy ──────────────────────────────────────────────────────────────────
    desktop(GB, KeyT, Pad(One, A)),
    web(GB, KeyT, Pad(One, B)),
    desktop(GB, KeyR, Pad(One, B)),
    web(GB, KeyR, Pad(One, A)),
    both(GB, Digit4, Pad(One, Select)),
    both(GB, Digit5, Pad(One, Start)),
    both(GB, KeyW, Pad(One, Up)),
    desktop(GB, ArrowUp, Pad(One, Up)),
    both(GB, KeyS, Pad(One, Down)),
    desktop(GB, ArrowDown, Pad(One, Down)),
    both(GB, KeyA, Pad(One, Left)),
    desktop(GB, ArrowLeft, Pad(One, Left)),
    both(GB, KeyD, Pad(One, Right)),
    desktop(GB, ArrowRight, Pad(One, Right)),
    // ── Game Boy Advance ──────────────────────────────────────────────────────────
    desktop(GBA, KeyT, Pad(One, A)),
    web(GBA, KeyG, Pad(One, A)),
    desktop(GBA, KeyR, Pad(One, B)),
    web(GBA, KeyF, Pad(One, B)),
    desktop(GBA, KeyQ, Pad(One, L)),
    web(GBA, KeyV, Pad(One, L)),
    desktop(GBA, KeyE, Pad(One, R)),
    web(GBA, KeyB, Pad(One, R)),
    both(GBA, Digit4, Pad(One, Select)),
    both(GBA, Digit5, Pad(One, Start)),
    both(GBA, KeyW, Pad(One, Up)),
    desktop(GBA, ArrowUp, Pad(One, Up)),
    both(GBA, KeyS, Pad(One, Down)),
    desktop(GBA, ArrowDown, Pad(One, Down)),
    both(GBA, KeyA, Pad(One, Left)),
    desktop(GBA, ArrowLeft, Pad(One, Left)),
    both(GBA, KeyD, Pad(One, Right)),
    desktop(GBA, ArrowRight, Pad(One, Right)),
    // ── SNES, player 1 ────────────────────────────────────────────────────────────
    both(SNES, KeyT, Pad(One, A)),
    both(SNES, KeyR, Pad(One, B)),
    both(SNES, KeyY, Pad(One, X)),
    both(SNES, KeyG, Pad(One, Y)),
    both(SNES, KeyQ, Pad(One, L)),
    both(SNES, KeyE, Pad(One, R)),
    both(SNES, Digit4, SuperScopeTurbo),
    both(SNES, Digit4, Pad(One, Select)),
    both(SNES, Digit5, SuperScopePause),
    both(SNES, Digit5, Pad(One, Start)),
    both(SNES, KeyW, Pad(One, Up)),
    desktop(SNES, ArrowUp, Pad(One, Up)),
    both(SNES, KeyS, Pad(One, Down)),
    desktop(SNES, ArrowDown, Pad(One, Down)),
    both(SNES, KeyA, Pad(One, Left)),
    desktop(SNES, ArrowLeft, Pad(One, Left)),
    both(SNES, KeyD, Pad(One, Right)),
    desktop(SNES, ArrowRight, Pad(One, Right)),
    // ── SNES, player 2 ────────────────────────────────────────────────────────────
    web(SNES, KeyI, Pad(Two, Up)),
    web(SNES, KeyK, Pad(Two, Down)),
    web(SNES, KeyJ, Pad(Two, Left)),
    web(SNES, KeyL, Pad(Two, Right)),
    web(SNES, KeyP, Pad(Two, B)),
    web(SNES, KeyO, Pad(Two, A)),
    web(SNES, Digit9, Pad(Two, Select)),
    web(SNES, Digit0, Pad(Two, Start)),
];

use Input::{Pad, PowerPad, SnesPadOnNes, SuperScopePause, SuperScopeTurbo, VsCoin, VsService};
use Key::*;
use PadButton::{A, B, Down, L, Left, R, Right, Select, Start, Up, X, Y};
use Player::{One, Two};
use PowerPadButton as PP;
const NES: SystemType = SystemType::Nes;
const GB: SystemType = SystemType::GameBoy;
const GBA: SystemType = SystemType::Gba;
const SNES: SystemType = SystemType::Snes;

const fn row(console: SystemType, key: Key, input: Input, shells: Shells) -> KeyBinding {
    KeyBinding {
        console,
        key,
        input,
        shells,
    }
}

const fn both(console: SystemType, key: Key, input: Input) -> KeyBinding {
    row(console, key, input, Shells::Both)
}

const fn desktop(console: SystemType, key: Key, input: Input) -> KeyBinding {
    row(console, key, input, Shells::DesktopOnly)
}

const fn web(console: SystemType, key: Key, input: Input) -> KeyBinding {
    row(console, key, input, Shells::WebOnly)
}

/// The rows `shell` applies when `key` is pressed or released on `console`, in the order
/// it tries them.
pub fn bindings_for(
    shell: Shell,
    console: SystemType,
    key: Key,
) -> impl Iterator<Item = &'static KeyBinding> {
    KEY_BINDINGS
        .iter()
        .filter(move |b| b.console == console && b.key == key && b.shells.include(shell))
}

/// Every row `shell` honours, in table order.
pub fn bindings_of(shell: Shell) -> impl Iterator<Item = &'static KeyBinding> {
    KEY_BINDINGS.iter().filter(move |b| b.shells.include(shell))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn describe(b: &KeyBinding) -> String {
        format!("{:?} {:?} {:?}", b.console, b.key, b.input)
    }

    fn one_shell_rows(shells: Shells) -> Vec<String> {
        KEY_BINDINGS
            .iter()
            .filter(|b| b.shells == shells)
            .map(describe)
            .collect()
    }

    /// The parity test: every binding one shell has and the other lacks. Each line is a
    /// difference that existed when the table was made (nr-tlf) or that somebody declared
    /// since; adding a one-shell row means adding its line here on purpose. Whether to
    /// close any of them is a player-visible decision for the navigator.
    #[test]
    fn one_shell_bindings_are_exactly_the_declared_differences() {
        let desktop_only = [
            // The NES Power Pad has no keys on the web.
            "Nes Digit1 PowerPad(One, One)",
            "Nes Digit2 PowerPad(One, Two)",
            "Nes Digit3 PowerPad(One, Three)",
            "Nes KeyQ PowerPad(One, Four)",
            "Nes KeyW PowerPad(One, Five)",
            "Nes KeyE PowerPad(One, Six)",
            "Nes KeyA PowerPad(One, Seven)",
            "Nes KeyS PowerPad(One, Eight)",
            "Nes KeyD PowerPad(One, Nine)",
            "Nes KeyZ PowerPad(One, Ten)",
            "Nes KeyX PowerPad(One, Eleven)",
            "Nes KeyC PowerPad(One, Twelve)",
            // T and R are A and B on the desktop, B and A on the web (NES pad, SNES pad).
            "Nes KeyT SnesPadOnNes(One, Y)",
            "Nes KeyT Pad(One, A)",
            "Nes KeyR SnesPadOnNes(One, X)",
            "Nes KeyR Pad(One, B)",
            "Nes KeyF SnesPadOnNes(One, B)",
            "Nes KeyG SnesPadOnNes(One, A)",
            "Nes Digit7 PowerPad(Two, One)",
            "Nes Digit8 PowerPad(Two, Two)",
            "Nes Digit9 PowerPad(Two, Three)",
            "Nes KeyU PowerPad(Two, Four)",
            "Nes KeyI PowerPad(Two, Five)",
            "Nes KeyO PowerPad(Two, Six)",
            "Nes KeyJ PowerPad(Two, Seven)",
            "Nes KeyK PowerPad(Two, Eight)",
            "Nes KeyL PowerPad(Two, Nine)",
            "Nes KeyM PowerPad(Two, Ten)",
            "Nes Comma PowerPad(Two, Eleven)",
            "Nes Period PowerPad(Two, Twelve)",
            // The Vs. System service button is bound on the desktop only.
            "Nes Minus VsService",
            "GameBoy KeyT Pad(One, A)",
            "GameBoy KeyR Pad(One, B)",
            "GameBoy ArrowUp Pad(One, Up)",
            "GameBoy ArrowDown Pad(One, Down)",
            "GameBoy ArrowLeft Pad(One, Left)",
            "GameBoy ArrowRight Pad(One, Right)",
            "Gba KeyT Pad(One, A)",
            "Gba KeyR Pad(One, B)",
            "Gba KeyQ Pad(One, L)",
            "Gba KeyE Pad(One, R)",
            "Gba ArrowUp Pad(One, Up)",
            "Gba ArrowDown Pad(One, Down)",
            "Gba ArrowLeft Pad(One, Left)",
            "Gba ArrowRight Pad(One, Right)",
            "Snes ArrowUp Pad(One, Up)",
            "Snes ArrowDown Pad(One, Down)",
            "Snes ArrowLeft Pad(One, Left)",
            "Snes ArrowRight Pad(One, Right)",
        ];
        let web_only = [
            "Nes KeyT SnesPadOnNes(One, A)",
            "Nes KeyT Pad(One, B)",
            "Nes KeyR SnesPadOnNes(One, B)",
            "Nes KeyR Pad(One, A)",
            "Nes KeyY SnesPadOnNes(One, X)",
            "Nes KeyG SnesPadOnNes(One, Y)",
            "GameBoy KeyT Pad(One, B)",
            "GameBoy KeyR Pad(One, A)",
            "Gba KeyG Pad(One, A)",
            "Gba KeyF Pad(One, B)",
            "Gba KeyV Pad(One, L)",
            "Gba KeyB Pad(One, R)",
            // The SNES console has a keyboard player 2 on the web only.
            "Snes KeyI Pad(Two, Up)",
            "Snes KeyK Pad(Two, Down)",
            "Snes KeyJ Pad(Two, Left)",
            "Snes KeyL Pad(Two, Right)",
            "Snes KeyP Pad(Two, B)",
            "Snes KeyO Pad(Two, A)",
            "Snes Digit9 Pad(Two, Select)",
            "Snes Digit0 Pad(Two, Start)",
        ];
        assert_eq!(one_shell_rows(Shells::DesktopOnly), desktop_only);
        assert_eq!(one_shell_rows(Shells::WebOnly), web_only);
    }

    /// Every binding both shells share, pinned like the differences, so a shared row
    /// that changes meaning (or moves to one shell) turns this red.
    #[test]
    fn both_shells_share_exactly_these_bindings() {
        let shared = [
            "Nes KeyQ SnesPadOnNes(One, L)",
            "Nes KeyW SnesPadOnNes(One, Up)",
            "Nes KeyW Pad(One, Up)",
            "Nes KeyE SnesPadOnNes(One, R)",
            "Nes KeyA SnesPadOnNes(One, Left)",
            "Nes KeyA Pad(One, Left)",
            "Nes KeyS SnesPadOnNes(One, Down)",
            "Nes KeyS Pad(One, Down)",
            "Nes KeyD SnesPadOnNes(One, Right)",
            "Nes KeyD Pad(One, Right)",
            "Nes Digit4 SnesPadOnNes(One, Select)",
            "Nes Digit4 Pad(One, Select)",
            "Nes Digit5 SnesPadOnNes(One, Start)",
            "Nes Digit5 Pad(One, Start)",
            "Nes Digit9 Pad(Two, Select)",
            "Nes Digit0 Pad(Two, Start)",
            "Nes KeyI Pad(Two, Up)",
            "Nes KeyO Pad(Two, A)",
            "Nes KeyJ Pad(Two, Left)",
            "Nes KeyK Pad(Two, Down)",
            "Nes KeyL Pad(Two, Right)",
            "Nes KeyP Pad(Two, B)",
            "Nes Digit6 VsCoin",
            "GameBoy Digit4 Pad(One, Select)",
            "GameBoy Digit5 Pad(One, Start)",
            "GameBoy KeyW Pad(One, Up)",
            "GameBoy KeyS Pad(One, Down)",
            "GameBoy KeyA Pad(One, Left)",
            "GameBoy KeyD Pad(One, Right)",
            "Gba Digit4 Pad(One, Select)",
            "Gba Digit5 Pad(One, Start)",
            "Gba KeyW Pad(One, Up)",
            "Gba KeyS Pad(One, Down)",
            "Gba KeyA Pad(One, Left)",
            "Gba KeyD Pad(One, Right)",
            "Snes KeyT Pad(One, A)",
            "Snes KeyR Pad(One, B)",
            "Snes KeyY Pad(One, X)",
            "Snes KeyG Pad(One, Y)",
            "Snes KeyQ Pad(One, L)",
            "Snes KeyE Pad(One, R)",
            "Snes Digit4 SuperScopeTurbo",
            "Snes Digit4 Pad(One, Select)",
            "Snes Digit5 SuperScopePause",
            "Snes Digit5 Pad(One, Start)",
            "Snes KeyW Pad(One, Up)",
            "Snes KeyS Pad(One, Down)",
            "Snes KeyA Pad(One, Left)",
            "Snes KeyD Pad(One, Right)",
        ];
        assert_eq!(one_shell_rows(Shells::Both), shared);
    }

    /// A shell stops at the first row the device accepts, and a joypad, the Vs. coin and the
    /// Vs. service button always accept, so a row after one of those would never be
    /// reached; nor is the same row declared twice.
    #[test]
    fn every_row_a_shell_honours_is_reachable() {
        for shell in [Shell::Desktop, Shell::Web] {
            for b in bindings_of(shell) {
                let rows: Vec<_> = bindings_for(shell, b.console, b.key).collect();
                let always = |r: &&&KeyBinding| {
                    matches!(r.input, Input::Pad(..) | Input::VsCoin | Input::VsService)
                };
                if let Some(at) = rows.iter().position(|r| always(&r)) {
                    assert_eq!(
                        at,
                        rows.len() - 1,
                        "{shell:?}: a row of {:?} {:?} follows one that always accepts",
                        b.console,
                        b.key
                    );
                }
                let same = rows.iter().filter(|r| r.input == b.input).count();
                assert_eq!(same, 1, "{shell:?} {} is declared twice", describe(b));
            }
        }
    }

    #[test]
    fn pad_button_ids_are_each_consoles_button_ids() {
        use crate::nes::input::{Button, button_from_id as nes_button};
        use crate::snes::input::{SnesButton, button_from_id as snes_button};
        assert_eq!(nes_button(PadButton::A.id()), Some(Button::A));
        assert_eq!(nes_button(PadButton::B.id()), Some(Button::B));
        assert_eq!(nes_button(PadButton::Select.id()), Some(Button::Select));
        assert_eq!(nes_button(PadButton::Right.id()), Some(Button::Right));
        assert_eq!(snes_button(PadButton::L.id()), Some(SnesButton::L));
        assert_eq!(snes_button(PadButton::R.id()), Some(SnesButton::R));
        assert_eq!(snes_button(PadButton::X.id()), Some(SnesButton::X));
        assert_eq!(snes_button(PadButton::Y.id()), Some(SnesButton::Y));
    }

    #[test]
    fn web_keys_are_the_characters_a_us_layout_types() {
        assert_eq!(Key::KeyW.web_key(), "w");
        assert_eq!(Key::Digit6.web_key(), "6");
        assert_eq!(Key::Minus.web_key(), "-");
        assert_eq!(Key::Comma.web_key(), ",");
        assert_eq!(Key::ArrowUp.web_key(), "arrowup");
    }
}
