mod joypad;
mod paddle;

pub use joypad::{Button, Joypad};
pub use paddle::Paddle;

use crate::console::{JoypadState, PaddleState};

/// Unified controller state for save-state support.
#[derive(Debug, Clone)]
pub enum ControllerState {
    Joypad(JoypadState),
    Paddle(PaddleState),
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
    fn new_boxed() -> Box<dyn Controller> where Self: Sized;
}
