use super::ControllerInput;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PowerPadState {
    pub strobe: bool,
    pub bit_index: u8,
    pub button_states: u16,
}

pub struct PowerPad {
    strobe: bool,
    bit_index: u8,
    button_states: u16,
}

impl Default for PowerPad {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerPad {
    const D3_ORDER: [crate::nes::input::PowerPadButton; 8] = [
        crate::nes::input::PowerPadButton::Two,
        crate::nes::input::PowerPadButton::One,
        crate::nes::input::PowerPadButton::Five,
        crate::nes::input::PowerPadButton::Nine,
        crate::nes::input::PowerPadButton::Six,
        crate::nes::input::PowerPadButton::Ten,
        crate::nes::input::PowerPadButton::Eleven,
        crate::nes::input::PowerPadButton::Seven,
    ];

    const D4_ORDER: [Option<crate::nes::input::PowerPadButton>; 8] = [
        Some(crate::nes::input::PowerPadButton::Four),
        Some(crate::nes::input::PowerPadButton::Three),
        Some(crate::nes::input::PowerPadButton::Twelve),
        Some(crate::nes::input::PowerPadButton::Eight),
        None,
        None,
        None,
        None,
    ];

    pub fn new() -> Self {
        Self {
            strobe: false,
            bit_index: 0,
            button_states: 0,
        }
    }

    pub fn write_strobe(&mut self, value: u8) {
        let new_strobe = value & 0x01 != 0;
        if self.strobe && !new_strobe {
            self.bit_index = 0;
        }
        self.strobe = new_strobe;
    }

    pub fn read(&mut self, is_dummy_read: bool) -> u8 {
        let index = self.bit_index as usize;
        let d3 = if index < 8 {
            self.button_is_pressed(Self::D3_ORDER[index])
        } else {
            true
        };
        let d4 = if index < 8 {
            match Self::D4_ORDER[index] {
                Some(button) => self.button_is_pressed(button),
                None => true,
            }
        } else {
            true
        };

        if !self.strobe && !is_dummy_read {
            self.bit_index = self.bit_index.saturating_add(1);
        }

        ((d3 as u8) << 3) | ((d4 as u8) << 4)
    }

    pub fn set_button(&mut self, button: crate::nes::input::PowerPadButton, pressed: bool) {
        let bit = button as u8;
        if pressed {
            self.button_states |= 1 << bit;
        } else {
            self.button_states &= !(1 << bit);
        }
    }

    fn button_is_pressed(&self, button: crate::nes::input::PowerPadButton) -> bool {
        (self.button_states & (1 << (button as u8))) != 0
    }

    pub fn capture_state(&self) -> PowerPadState {
        PowerPadState {
            strobe: self.strobe,
            bit_index: self.bit_index,
            button_states: self.button_states,
        }
    }

    pub fn restore_state(&mut self, state: &PowerPadState) {
        self.strobe = state.strobe;
        self.bit_index = state.bit_index;
        // Physical button states at save time are meaningless after restore.
        self.button_states = 0;
    }
}

impl crate::nes::input::Controller for PowerPad {
    fn write_strobe(&mut self, value: u8) {
        self.write_strobe(value);
    }

    fn read(&mut self, is_dummy_read: bool) -> u8 {
        self.read(is_dummy_read)
    }

    fn capture_state(&self) -> crate::nes::input::ControllerState {
        crate::nes::input::ControllerState::PowerPad(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::nes::input::ControllerState) {
        if let crate::nes::input::ControllerState::PowerPad(power_pad_state) = state {
            self.restore_state(power_pad_state);
        }
    }

    fn set_button(&mut self, _button: crate::nes::input::Button, _pressed: bool) -> bool {
        false
    }

    fn set_power_pad_button(
        &mut self,
        button: crate::nes::input::PowerPadButton,
        pressed: bool,
    ) -> bool {
        self.set_button(button, pressed);
        true
    }

    fn set_mouse_x_position(&mut self, _position: u8) -> bool {
        false
    }

    fn set_mouse_y_position(&mut self, _position: u8) -> bool {
        false
    }

    fn set_mouse_left_button(&mut self, _pressed: bool) -> bool {
        false
    }

    fn input_type(&self) -> ControllerInput {
        crate::nes::input::controller_input_type(crate::nes::input::ControllerType::PowerPad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::input::PowerPadButton;

    #[test]
    fn test_power_pad_shifts_buttons_in_protocol_order() {
        let mut power_pad = PowerPad::new();
        power_pad.set_button(PowerPadButton::One, true);
        power_pad.set_button(PowerPadButton::Four, true);
        power_pad.set_button(PowerPadButton::Twelve, true);

        power_pad.write_strobe(1);
        power_pad.write_strobe(0);

        let expected = [
            0x10, // D3=2 (off), D4=4 (on)
            0x08, // D3=1 (on), D4=3 (off)
            0x10, // D3=5 (off), D4=12 (on)
            0x00, // D3=9 (off), D4=8 (off)
            0x10, // D3=6 (off), D4=hardwired high
            0x10, // D3=10 (off), D4=hardwired high
            0x10, // D3=11 (off), D4=hardwired high
            0x10, // D3=7 (off), D4=hardwired high
        ];

        for expected_value in expected {
            assert_eq!(power_pad.read(false) & 0x18, expected_value);
        }
    }

    #[test]
    fn test_power_pad_capture_restore_state_roundtrip() {
        let mut power_pad = PowerPad::new();
        power_pad.set_button(PowerPadButton::Two, true);
        power_pad.set_button(PowerPadButton::Eight, true);
        power_pad.write_strobe(1);
        power_pad.write_strobe(0);
        power_pad.read(false);
        power_pad.read(false);

        let state = power_pad.capture_state();
        let mut restored = PowerPad::new();
        restored.restore_state(&state);
        // button_states is intentionally cleared on restore (physical input state).
        assert_eq!(restored.capture_state().button_states, 0);
        assert_eq!(restored.capture_state().bit_index, state.bit_index);
        assert_eq!(restored.capture_state().strobe, state.strobe);
    }

    #[test]
    fn test_power_pad_does_not_support_standard_joypad_buttons() {
        let mut power_pad = PowerPad::new();
        assert!(!crate::nes::input::Controller::set_button(
            &mut power_pad,
            crate::nes::input::Button::A,
            true
        ));
    }
}
