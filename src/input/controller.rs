use crate::console::{ArkanoidState, JoypadState, ZapperState};
use crate::input::Button;

/// Unified controller state for save-state support.
#[derive(Debug, Clone)]
pub enum ControllerState {
    Joypad(JoypadState),
    Paddle(ArkanoidState),
    Zapper(ZapperState),
}

/// Controller type for a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerType {
    Joypad,
    Arkanoid,
    Zapper,
}

// TODO remove the "paddle" variant
impl ControllerType {
    /// Parse a controller type from a string configuration value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "joypad" => Some(Self::Joypad),
            "arkanoid" | "paddle" => Some(Self::Arkanoid),
            "zapper" => Some(Self::Zapper),
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
    // Mouse needed to provide input.
    Mouse,
}

/// Return the input type required for a given controller type.
pub fn controller_input_type(controller_type: ControllerType) -> ControllerInput {
    match controller_type {
        ControllerType::Joypad => ControllerInput::Gamepad,
        ControllerType::Arkanoid => ControllerInput::Mouse,
        ControllerType::Zapper => ControllerInput::Mouse,
    }
}

/// Trait for NES controller devices (Joypad, Arkanoid controller, etc.).
pub trait Controller {
    /// Write to strobe register ($4016).
    fn write_strobe(&mut self, value: u8);

    /// Read controller state, advancing the shift register.
    fn read(&mut self) -> u8;

    /// Read controller state without advancing the shift register.
    fn read_no_clock(&self) -> u8;

    /// Capture controller state for save-state.
    fn capture_state(&self) -> ControllerState;

    /// Restore controller state from save-state.
    fn restore_state(&mut self, state: &ControllerState);

    /// Create a new default controller instance.
    fn new_boxed() -> Box<dyn Controller>
    where
        Self: Sized;

    /// Set button state (for Joypad controllers).
    /// Returns true if the operation was successful, false if not supported.
    fn set_button(&mut self, button: Button, pressed: bool) -> bool;

    /// Set mouse X position for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_x_position(&mut self, position: u8) -> bool;

    /// Set mouse Y position for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_y_position(&mut self, position: u8) -> bool;

    /// Set mouse left button state for mouse-emulated controllers.
    /// Returns true if the operation was successful, false if not supported.
    fn set_mouse_left_button(&mut self, pressed: bool) -> bool;

    /// Update light detection state based on screen buffer (for light gun controllers).
    /// Returns true if the operation was performed, false if not supported.
    fn update_light_detection(&mut self, _screen_buffer: &crate::ppu::ScreenBuffer) -> bool {
        false
    }

    // Get the type of input this controller needs.
    fn input_type(&self) -> ControllerInput;
}
