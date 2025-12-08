/// NES Controller Button
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
pub struct Joypad {
    strobe: bool,
    button_index: u8,
    button_states: u8, // Bitfield: [Right, Left, Down, Up, Start, Select, B, A]
}

impl Joypad {
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
    /// Advances to next button if not in strobe mode
    pub fn read(&mut self) -> u8 {
        // After 8 reads, always return 1
        if self.button_index >= 8 {
            return 1;
        }

        // Return current button state (bit 0)
        let response = (self.button_states >> self.button_index) & 0x01;

        // Advance to next button only if not in strobe mode
        if !self.strobe {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_joypad() {
        let joypad = Joypad::new();
        assert_eq!(joypad.strobe, false);
        assert_eq!(joypad.button_index, 0);
        assert_eq!(joypad.button_states, 0);
    }

    #[test]
    fn test_strobe_reset() {
        let mut joypad = Joypad::new();

        // Press A button
        joypad.set_button(Button::A, true);

        // Read first button (A)
        joypad.read();
        assert_eq!(joypad.button_index, 1);

        // Read second button (B)
        joypad.read();
        assert_eq!(joypad.button_index, 2);

        // Strobe high
        joypad.write_strobe(1);

        // Strobe low - should reset
        joypad.write_strobe(0);
        assert_eq!(joypad.button_index, 0);
    }

    #[test]
    fn test_sequential_button_reading() {
        let mut joypad = Joypad::new();

        // Press A, Start, and Right
        joypad.set_button(Button::A, true);
        joypad.set_button(Button::Start, true);
        joypad.set_button(Button::Right, true);

        // Read buttons in order: A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(joypad.read(), 1); // A pressed
        assert_eq!(joypad.read(), 0); // B not pressed
        assert_eq!(joypad.read(), 0); // Select not pressed
        assert_eq!(joypad.read(), 1); // Start pressed
        assert_eq!(joypad.read(), 0); // Up not pressed
        assert_eq!(joypad.read(), 0); // Down not pressed
        assert_eq!(joypad.read(), 0); // Left not pressed
        assert_eq!(joypad.read(), 1); // Right pressed
    }

    #[test]
    fn test_ninth_read_returns_one() {
        let mut joypad = Joypad::new();

        // Read all 8 buttons
        for _ in 0..8 {
            joypad.read();
        }

        // 9th and subsequent reads should return 1
        assert_eq!(joypad.read(), 1);
        assert_eq!(joypad.read(), 1);
        assert_eq!(joypad.read(), 1);
    }

    #[test]
    fn test_strobe_holds_same_button() {
        let mut joypad = Joypad::new();

        // Press B button
        joypad.set_button(Button::B, true);

        // Set strobe high
        joypad.write_strobe(1);

        // Reading while strobe=1 should keep returning same button
        assert_eq!(joypad.read(), 0); // A not pressed
        assert_eq!(joypad.button_index, 0); // Index shouldn't advance
        assert_eq!(joypad.read(), 0); // Still reading A
        assert_eq!(joypad.button_index, 0);

        // Release strobe
        joypad.write_strobe(0);

        // Now reading should advance
        assert_eq!(joypad.read(), 0); // A
        assert_eq!(joypad.button_index, 1);
        assert_eq!(joypad.read(), 1); // B
        assert_eq!(joypad.button_index, 2);
    }

    #[test]
    fn test_button_state_changes() {
        let mut joypad = Joypad::new();

        // Press A
        joypad.set_button(Button::A, true);
        assert_eq!(joypad.read(), 1);

        // Reset and press B instead
        joypad.write_strobe(1);
        joypad.write_strobe(0);
        joypad.set_button(Button::A, false);
        joypad.set_button(Button::B, true);

        assert_eq!(joypad.read(), 0); // A not pressed
        assert_eq!(joypad.read(), 1); // B pressed
    }

    #[test]
    fn test_all_buttons() {
        let mut joypad = Joypad::new();

        // Press all buttons
        joypad.set_button(Button::A, true);
        joypad.set_button(Button::B, true);
        joypad.set_button(Button::Select, true);
        joypad.set_button(Button::Start, true);
        joypad.set_button(Button::Up, true);
        joypad.set_button(Button::Down, true);
        joypad.set_button(Button::Left, true);
        joypad.set_button(Button::Right, true);

        // All 8 reads should return 1
        for _ in 0..8 {
            assert_eq!(joypad.read(), 1);
        }
    }
}
