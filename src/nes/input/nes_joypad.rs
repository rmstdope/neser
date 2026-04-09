/// NES standard joypad
///
/// Emulates the NES standard controller which has 8 buttons: A, B, Select, Start, Up, Down, Left, Right.
/// The controller state is read via a shift register that outputs button states
/// sequentially when clocked. Writing to the strobe register resets the read position.
use super::ControllerInput;
use serde::{Deserialize, Serialize};

/// Joypad controller state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JoypadState {
    pub strobe: bool,
    pub button_index: u8,
    pub button_states: u8,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Button {
    A = 0,
    B = 1,
    Select = 2,
    Start = 3,
    Up = 4,
    Down = 5,
    Left = 6,
    Right = 7,
}

/// NES Standard Controller (Joypad)
///
/// The controller uses a shift register that returns button states sequentially.
/// Writing to the strobe register (bit 0) resets the read position.
pub struct NesJoypad {
    strobe: bool,
    button_index: u8,
    button_states: u8, // Bitfield: [Right, Left, Down, Up, Start, Select, B, A]
}

impl Default for NesJoypad {
    fn default() -> Self {
        Self::new()
    }
}

impl NesJoypad {
    pub fn new() -> Self {
        Self {
            strobe: false,
            button_index: 0,
            button_states: 0,
        }
    }

    /// Write to strobe register ($4016)
    /// When strobe transitions from 1 to 0, the button index is reset
    pub fn write_strobe(&mut self, value: u8) {
        let new_strobe = value & 0x01 != 0;

        // If going from strobe=1 to strobe=0, reset the button index
        if self.strobe && !new_strobe {
            self.button_index = 0;
        }

        self.strobe = new_strobe;
    }

    /// Read from controller register ($4016/$4017)
    /// Returns the current button state in bit 0
    /// Advances to next button if not in strobe mode and not a dummy read
    pub fn read(&mut self, is_dummy_read: bool) -> u8 {
        // After 8 reads, always return 1
        if self.button_index >= 8 {
            return 1;
        }

        // Apply SOCD (Simultaneous Opposite Cardinal Direction) cleaning
        // If both left+right or up+down are pressed, cancel both out
        let mut cleaned_states = self.button_states;

        // Check for left+right both pressed
        let left_mask = 1 << (Button::Left as u8);
        let right_mask = 1 << (Button::Right as u8);
        if (cleaned_states & left_mask) != 0 && (cleaned_states & right_mask) != 0 {
            // Cancel both
            cleaned_states &= !(left_mask | right_mask);
        }

        // Check for up+down both pressed
        let up_mask = 1 << (Button::Up as u8);
        let down_mask = 1 << (Button::Down as u8);
        if (cleaned_states & up_mask) != 0 && (cleaned_states & down_mask) != 0 {
            // Cancel both
            cleaned_states &= !(up_mask | down_mask);
        }

        // Return current button state (bit 0)
        let response = (cleaned_states >> self.button_index) & 0x01;

        // Advance to next button only if not in strobe mode and not a dummy read
        if !self.strobe && !is_dummy_read {
            self.button_index += 1;
        }

        response
    }

    /// Set the state of a button
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        let bit = button as u8;
        if pressed {
            self.button_states |= 1 << bit;
        } else {
            self.button_states &= !(1 << bit);
        }
    }

    /// Capture current joypad state for save-state.
    pub fn capture_state(&self) -> JoypadState {
        JoypadState {
            strobe: self.strobe,
            button_index: self.button_index,
            button_states: self.button_states,
        }
    }

    /// Restore joypad state from a save-state.
    pub fn restore_state(&mut self, state: &JoypadState) {
        self.strobe = state.strobe;
        self.button_index = state.button_index;
        self.button_states = state.button_states;
    }
}

impl crate::nes::input::Controller for NesJoypad {
    fn write_strobe(&mut self, value: u8) {
        self.write_strobe(value)
    }

    fn read(&mut self, is_dummy_read: bool) -> u8 {
        self.read(is_dummy_read)
    }

    fn capture_state(&self) -> crate::nes::input::ControllerState {
        crate::nes::input::ControllerState::Joypad(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::nes::input::ControllerState) {
        if let crate::nes::input::ControllerState::Joypad(joypad_state) = state {
            self.restore_state(joypad_state);
        }
    }

    fn set_button(&mut self, button: crate::nes::input::Button, pressed: bool) -> bool {
        self.set_button(button, pressed);
        true
    }

    fn set_mouse_x_position(&mut self, _position: u8) -> bool {
        false // Not supported for Joypad
    }

    fn set_mouse_y_position(&mut self, _position: u8) -> bool {
        false // Not supported for Joypad
    }

    fn set_mouse_left_button(&mut self, _pressed: bool) -> bool {
        false // Not supported for Joypad
    }

    fn input_type(&self) -> ControllerInput {
        crate::nes::input::controller_input_type(crate::nes::input::ControllerType::Joypad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_joypad() {
        let joypad = NesJoypad::new();
        assert!(!joypad.strobe);
        assert_eq!(joypad.button_index, 0);
        assert_eq!(joypad.button_states, 0);
    }

    #[test]
    fn test_strobe_reset() {
        let mut joypad = NesJoypad::new();

        // Press A button
        joypad.set_button(Button::A, true);

        // Read first button (A)
        joypad.read(false);
        assert_eq!(joypad.button_index, 1);

        // Read second button (B)
        joypad.read(false);
        assert_eq!(joypad.button_index, 2);

        // Strobe high
        joypad.write_strobe(1);

        // Strobe low - should reset
        joypad.write_strobe(0);
        assert_eq!(joypad.button_index, 0);
    }

    #[test]
    fn test_sequential_button_reading() {
        let mut joypad = NesJoypad::new();

        // Press A, Start, and Right
        joypad.set_button(Button::A, true);
        joypad.set_button(Button::Start, true);
        joypad.set_button(Button::Right, true);

        // Read buttons in order: A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(joypad.read(false), 1); // A pressed
        assert_eq!(joypad.read(false), 0); // B not pressed
        assert_eq!(joypad.read(false), 0); // Select not pressed
        assert_eq!(joypad.read(false), 1); // Start pressed
        assert_eq!(joypad.read(false), 0); // Up not pressed
        assert_eq!(joypad.read(false), 0); // Down not pressed
        assert_eq!(joypad.read(false), 0); // Left not pressed
        assert_eq!(joypad.read(false), 1); // Right pressed
    }

    #[test]
    fn test_ninth_read_returns_one() {
        let mut joypad = NesJoypad::new();

        // Read all 8 buttons
        for _ in 0..8 {
            joypad.read(false);
        }

        // 9th and subsequent reads should return 1
        assert_eq!(joypad.read(false), 1);
        assert_eq!(joypad.read(false), 1);
        assert_eq!(joypad.read(false), 1);
    }

    #[test]
    fn test_strobe_holds_same_button() {
        let mut joypad = NesJoypad::new();

        // Press B button
        joypad.set_button(Button::B, true);

        // Set strobe high
        joypad.write_strobe(1);

        // Reading while strobe=1 should keep returning same button
        assert_eq!(joypad.read(false), 0); // A not pressed
        assert_eq!(joypad.button_index, 0); // Index shouldn't advance
        assert_eq!(joypad.read(false), 0); // Still reading A
        assert_eq!(joypad.button_index, 0);

        // Release strobe
        joypad.write_strobe(0);

        // Now reading should advance
        assert_eq!(joypad.read(false), 0); // A
        assert_eq!(joypad.button_index, 1);
        assert_eq!(joypad.read(false), 1); // B
        assert_eq!(joypad.button_index, 2);
    }

    #[test]
    fn test_button_state_changes() {
        let mut joypad = NesJoypad::new();

        // Press A
        joypad.set_button(Button::A, true);
        assert_eq!(joypad.read(false), 1);

        // Reset and press B instead
        joypad.write_strobe(1);
        joypad.write_strobe(0);
        joypad.set_button(Button::A, false);
        joypad.set_button(Button::B, true);

        assert_eq!(joypad.read(false), 0); // A not pressed
        assert_eq!(joypad.read(false), 1); // B pressed
    }

    #[test]
    fn test_all_buttons() {
        let mut joypad = NesJoypad::new();

        // Press all buttons
        joypad.set_button(Button::A, true);
        joypad.set_button(Button::B, true);
        joypad.set_button(Button::Select, true);
        joypad.set_button(Button::Start, true);
        joypad.set_button(Button::Up, true);
        joypad.set_button(Button::Down, true);
        joypad.set_button(Button::Left, true);
        joypad.set_button(Button::Right, true);

        // With SOCD cleaning, opposite directions cancel out
        // Order: A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(joypad.read(false), 1); // A
        assert_eq!(joypad.read(false), 1); // B
        assert_eq!(joypad.read(false), 1); // Select
        assert_eq!(joypad.read(false), 1); // Start
        assert_eq!(joypad.read(false), 0); // Up - cancelled by Down
        assert_eq!(joypad.read(false), 0); // Down - cancelled by Up
        assert_eq!(joypad.read(false), 0); // Left - cancelled by Right
        assert_eq!(joypad.read(false), 0); // Right - cancelled by Left
    }

    #[test]
    fn test_dummy_read_does_not_advance_button_index() {
        let mut joypad = NesJoypad::new();
        joypad.set_button(Button::A, true);

        // First normal read advances index to 1.
        assert_eq!(joypad.read(false), 1);

        // Dummy read should not advance; should still read B (bit 1) which is not pressed.
        assert_eq!(joypad.read(true), 0);

        // Next normal read should now advance to index 2 (Select), still not pressed.
        assert_eq!(joypad.read(false), 0);
    }

    #[test]
    fn test_left_right_simultaneous_cancels_both() {
        let mut joypad = NesJoypad::new();

        // Press both left and right
        joypad.set_button(Button::Left, true);
        joypad.set_button(Button::Right, true);

        // Read through all buttons
        // Order: A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(joypad.read(false), 0); // A
        assert_eq!(joypad.read(false), 0); // B
        assert_eq!(joypad.read(false), 0); // Select
        assert_eq!(joypad.read(false), 0); // Start
        assert_eq!(joypad.read(false), 0); // Up
        assert_eq!(joypad.read(false), 0); // Down
        assert_eq!(joypad.read(false), 0); // Left - should be cancelled
        assert_eq!(joypad.read(false), 0); // Right - should be cancelled
    }

    #[test]
    fn test_up_down_simultaneous_cancels_both() {
        let mut joypad = NesJoypad::new();

        // Press both up and down
        joypad.set_button(Button::Up, true);
        joypad.set_button(Button::Down, true);

        // Read through all buttons
        // Order: A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(joypad.read(false), 0); // A
        assert_eq!(joypad.read(false), 0); // B
        assert_eq!(joypad.read(false), 0); // Select
        assert_eq!(joypad.read(false), 0); // Start
        assert_eq!(joypad.read(false), 0); // Up - should be cancelled
        assert_eq!(joypad.read(false), 0); // Down - should be cancelled
        assert_eq!(joypad.read(false), 0); // Left
        assert_eq!(joypad.read(false), 0); // Right
    }

    #[test]
    fn test_left_only_works() {
        let mut joypad = NesJoypad::new();

        // Press only left
        joypad.set_button(Button::Left, true);

        // Skip to left button (buttons 0-5)
        for _ in 0..6 {
            joypad.read(false);
        }

        // Left should be pressed
        assert_eq!(joypad.read(false), 1); // Left
        assert_eq!(joypad.read(false), 0); // Right
    }

    #[test]
    fn test_right_only_works() {
        let mut joypad = NesJoypad::new();

        // Press only right
        joypad.set_button(Button::Right, true);

        // Skip to left button (buttons 0-5)
        for _ in 0..6 {
            joypad.read(false);
        }

        // Right should be pressed
        assert_eq!(joypad.read(false), 0); // Left
        assert_eq!(joypad.read(false), 1); // Right
    }

    #[test]
    fn test_up_only_works() {
        let mut joypad = NesJoypad::new();

        // Press only up
        joypad.set_button(Button::Up, true);

        // Skip to up button (buttons 0-3)
        for _ in 0..4 {
            joypad.read(false);
        }

        // Up should be pressed
        assert_eq!(joypad.read(false), 1); // Up
        assert_eq!(joypad.read(false), 0); // Down
    }

    #[test]
    fn test_down_only_works() {
        let mut joypad = NesJoypad::new();

        // Press only down
        joypad.set_button(Button::Down, true);

        // Skip to up button (buttons 0-3)
        for _ in 0..4 {
            joypad.read(false);
        }

        // Down should be pressed
        assert_eq!(joypad.read(false), 0); // Up
        assert_eq!(joypad.read(false), 1); // Down
    }

    #[test]
    fn test_socd_with_other_buttons() {
        let mut joypad = NesJoypad::new();

        // Press A, left, and right (opposite directions)
        joypad.set_button(Button::A, true);
        joypad.set_button(Button::Left, true);
        joypad.set_button(Button::Right, true);

        // Read through all buttons
        assert_eq!(joypad.read(false), 1); // A - should work
        assert_eq!(joypad.read(false), 0); // B
        assert_eq!(joypad.read(false), 0); // Select
        assert_eq!(joypad.read(false), 0); // Start
        assert_eq!(joypad.read(false), 0); // Up
        assert_eq!(joypad.read(false), 0); // Down
        assert_eq!(joypad.read(false), 0); // Left - cancelled
        assert_eq!(joypad.read(false), 0); // Right - cancelled
    }

    #[test]
    fn test_socd_in_strobe_mode() {
        let mut joypad = NesJoypad::new();

        // Press opposite horizontal directions simultaneously
        joypad.set_button(Button::Left, true);
        joypad.set_button(Button::Right, true);

        // Advance reads to reach the Left button (index 6).
        // Buttons order: A(0), B(1), Select(2), Start(3), Up(4), Down(5), Left(6), Right(7)
        for _ in 0..6 {
            joypad.read(false);
        }

        // Enable strobe mode and repeatedly read the same button index (Left).
        // SOCD cleaning should still apply, so Left should read as 0.
        assert_eq!(joypad.read(true), 0); // Left under strobe - cancelled
        assert_eq!(joypad.read(true), 0); // Left under strobe - still cancelled
        assert_eq!(joypad.read(true), 0); // Left under strobe - still cancelled

        // Disable strobe and continue reading; the next button is Right.
        // SOCD cleaning should also cancel Right.
        assert_eq!(joypad.read(false), 0); // Right - cancelled
    }
}
