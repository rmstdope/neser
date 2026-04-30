//! GBA input subsystem.
//!
//! Exposes the [`Keypad`] which models the `KEYINPUT` (P1) and `KEYCNT`
//! registers along with key-interrupt (IRQ3) generation.
//!
//! Modeled per GBATek "GBA Keypad Input".
//!
//! <https://problemkaputt.de/gbatek.htm#gbakeypadinput>

pub mod keypad;

pub use keypad::{KEYCNT_COND_AND, KEYCNT_IRQ_ENABLE, KEYS_MASK, Keypad, REG_KEYCNT, REG_KEYINPUT};
