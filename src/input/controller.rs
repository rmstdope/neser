use crate::console::{JoypadState, PaddleState};
use crate::input::Button;

/// Unified controller state for save-state support.
#[derive(Debug, Clone)]
pub enum ControllerState {
    Joypad(JoypadState),
    Paddle(PaddleState),
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

/// Trait for NES controller devices (Joypad, Paddle, etc.).
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

    /// Set paddle position (for Paddle controllers).
    /// Returns true if the operation was successful, false if not supported.
    fn set_paddle_position(&mut self, position: u8) -> bool;

    /// Set paddle trigger state (for Paddle controllers).
    /// Returns true if the operation was successful, false if not supported.
    fn set_paddle_trigger(&mut self, pressed: bool) -> bool;

    // Get the type of input this controller needs.
    #[allow(dead_code)]
    fn input_type(&self) -> ControllerInput;
}
