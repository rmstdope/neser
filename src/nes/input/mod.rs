mod arkanoid_controller;
mod controller;
pub mod mouse_mapping;
mod nes_joypad;
mod power_pad;
mod snes_adapter;
mod zapper;

pub use arkanoid_controller::ArkanoidController;
pub use arkanoid_controller::ArkanoidState;
pub use controller::{
    Controller, ControllerInput, ControllerState, ControllerType, PowerPadButton, SnesButton,
    controller_input_type,
};
pub use nes_joypad::JoypadState;
pub use nes_joypad::{Button, NesJoypad};
pub use power_pad::{PowerPad, PowerPadState};
pub use snes_adapter::{SnesAdapter, SnesAdapterState};
pub use zapper::Zapper;
pub use zapper::ZapperState;

/// Convert a numeric button ID to a [`Button`] variant.
///
/// Button IDs match the [`Button`] discriminant values:
/// A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7.
pub fn button_from_id(id: u8) -> Option<Button> {
    match id {
        0 => Some(Button::A),
        1 => Some(Button::B),
        2 => Some(Button::Select),
        3 => Some(Button::Start),
        4 => Some(Button::Up),
        5 => Some(Button::Down),
        6 => Some(Button::Left),
        7 => Some(Button::Right),
        _ => None,
    }
}
