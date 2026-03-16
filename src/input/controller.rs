use crate::input::Button;
use crate::input::arkanoid_controller::ArkanoidState;
use crate::input::nes_joypad::JoypadState;
use crate::input::power_pad::PowerPadState;
use crate::input::snes_adapter::SnesAdapterState;
use crate::input::zapper::ZapperState;

/// Unified controller state for save-state support.
#[derive(Debug, Clone)]
pub enum ControllerState {
    Joypad(JoypadState),
    SnesAdapter(SnesAdapterState),
    Paddle(ArkanoidState),
    Zapper(ZapperState),
    PowerPad(PowerPadState),
}

/// Controller type for a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerType {
    Joypad,
    SnesAdapter,
    SnesController,
    SnesMouse,
    Arkanoid,
    Zapper,
    PowerPad,
}

/// SNES-specific button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnesButton {
    B,
    Y,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    A,
    X,
    L,
    R,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPadButton {
    One = 0,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Eleven,
    Twelve,
}

impl ControllerType {
    /// Parse a controller type from a string configuration value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "joypad" => Some(Self::Joypad),
            "snes-controller" | "snes_controller" | "snescontroller" => Some(Self::SnesController),
            "snes-mouse" | "snes_mouse" | "snesmouse" => Some(Self::SnesMouse),
            "arkanoid" | "paddle" => Some(Self::Arkanoid),
            "zapper" => Some(Self::Zapper),
            "power-pad" | "power_pad" | "powerpad" => Some(Self::PowerPad),
            _ => None,
        }
    }
}

/// The type of input a controller need.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerInput {
    // Gamepad (or keyboard as fallback) needed to provide input.
    Gamepad,
    // Keyboard-only input.
    Keyboard,
    // Mouse needed to provide input.
    Mouse,
}

/// Return the input type required for a given controller type.
pub fn controller_input_type(controller_type: ControllerType) -> ControllerInput {
    match controller_type {
        ControllerType::Joypad => ControllerInput::Gamepad,
        ControllerType::SnesAdapter => ControllerInput::Gamepad,
        ControllerType::SnesController => ControllerInput::Gamepad,
        ControllerType::SnesMouse => ControllerInput::Mouse,
        ControllerType::Arkanoid => ControllerInput::Mouse,
        ControllerType::Zapper => ControllerInput::Mouse,
        ControllerType::PowerPad => ControllerInput::Keyboard,
    }
}

/// Trait for NES controller devices (Joypad, Arkanoid controller, etc.).
pub trait Controller {
    /// Write to strobe register ($4016).
    fn write_strobe(&mut self, value: u8);

    /// Read controller state, optionally treating the read as a dummy cycle.
    fn read(&mut self, is_dummy_read: bool) -> u8;

    /// Capture controller state for save-state.
    fn capture_state(&self) -> ControllerState;

    /// Restore controller state from save-state.
    fn restore_state(&mut self, state: &ControllerState);

    /// Set button state (for Joypad controllers).
    /// Returns true if the operation was successful, false if not supported.
    fn set_button(&mut self, button: Button, pressed: bool) -> bool;

    /// Set SNES button state (for SNES controller emulation).
    /// Returns true if the operation was successful, false if not supported.
    fn set_snes_button(&mut self, _button: SnesButton, _pressed: bool) -> bool {
        false
    }

    /// Set Power Pad button state.
    /// Returns true if the operation was successful, false if not supported.
    fn set_power_pad_button(&mut self, _button: PowerPadButton, _pressed: bool) -> bool {
        false
    }

    /// Set mouse X position for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_x_position(&mut self, position: u8) -> bool;

    /// Set mouse Y position for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_y_position(&mut self, position: u8) -> bool;

    /// Set mouse left button state for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_left_button(&mut self, pressed: bool) -> bool;

    /// Apply relative mouse delta for mouse-emulated controllers.
    ///
    /// Returns true if the controller consumed the delta.
    fn add_mouse_delta(&mut self, _dx: i16, _dy: i16) -> bool {
        false
    }

    /// Set mouse right button state for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_right_button(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Returns true when this controller is a Super NES mouse.
    fn is_snes_mouse(&self) -> bool {
        false
    }

    // Get the type of input this controller needs.
    fn input_type(&self) -> ControllerInput;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_type_parse_supports_explicit_snes_controller_and_snes_mouse() {
        assert_eq!(
            ControllerType::parse("snes-controller"),
            Some(ControllerType::SnesController)
        );
        assert_eq!(
            ControllerType::parse("snes-mouse"),
            Some(ControllerType::SnesMouse)
        );
        assert_eq!(ControllerType::parse("snes-adapter"), None);
        assert_eq!(ControllerType::parse("snes"), None);
    }

    #[test]
    fn controller_type_parse_supports_power_pad_aliases() {
        assert_eq!(
            ControllerType::parse("power-pad"),
            Some(ControllerType::PowerPad)
        );
        assert_eq!(
            ControllerType::parse("power_pad"),
            Some(ControllerType::PowerPad)
        );
        assert_eq!(
            ControllerType::parse("powerpad"),
            Some(ControllerType::PowerPad)
        );
    }

    #[test]
    fn controller_input_type_reports_expected_inputs_for_explicit_snes_types() {
        assert_eq!(
            controller_input_type(ControllerType::SnesController),
            ControllerInput::Gamepad
        );
        assert_eq!(
            controller_input_type(ControllerType::SnesMouse),
            ControllerInput::Mouse
        );
    }

    #[test]
    fn controller_input_type_reports_keyboard_for_power_pad() {
        assert_eq!(
            controller_input_type(ControllerType::PowerPad),
            ControllerInput::Keyboard
        );
    }
}
