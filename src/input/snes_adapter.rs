use super::ControllerInput;
use serde::{Deserialize, Serialize};

/// SNES adapter controller state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnesAdapterState {
    pub strobe: bool,
    pub bit_index: u8,
    pub button_states: u16,
}

/// Super NES controller connected through a NES pin adapter.
///
/// Serial protocol:
/// - bits 0..=11: SNES buttons (B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R)
/// - bits 12..=15: always 1
/// - bits 16+: always 1
///
/// The adapter exposes serial data on D1 (bit 1), which is what allpads-r9
/// probes for the SNES controller adapter path.
pub struct SnesAdapter {
    strobe: bool,
    bit_index: u8,
    button_states: u16,
    mouse_mode: bool,
    mouse_left_button: bool,
    mouse_speed: u8,
}

impl Default for SnesAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SnesAdapter {
    pub fn new() -> Self {
        Self {
            strobe: false,
            bit_index: 0,
            button_states: 0,
            mouse_mode: false,
            mouse_left_button: false,
            mouse_speed: 0,
        }
    }

    pub fn write_strobe(&mut self, value: u8) {
        let new_strobe = value & 0x01 != 0;
        if self.strobe && !new_strobe {
            self.bit_index = 0;
        }
        self.strobe = new_strobe;
    }

    fn button_bit_for_nes_button(button: crate::input::Button) -> Option<u8> {
        match button {
            crate::input::Button::B => Some(0),
            crate::input::Button::A => Some(1),
            crate::input::Button::Select => Some(2),
            crate::input::Button::Start => Some(3),
            crate::input::Button::Up => Some(4),
            crate::input::Button::Down => Some(5),
            crate::input::Button::Left => Some(6),
            crate::input::Button::Right => Some(7),
        }
    }

    fn current_serial_bit_controller(&self) -> u8 {
        match self.bit_index {
            0..=7 => ((self.button_states >> self.bit_index) & 0x01) as u8,
            8..=11 => 1,
            12..=15 => 0,
            _ => 1,
        }
    }

    fn current_serial_bit_mouse(&self) -> u8 {
        match self.bit_index {
            9 => u8::from(self.mouse_left_button),
            11 => self.mouse_speed & 0x01,
            15 => 1,
            16..=31 => 1,
            _ => 0,
        }
    }

    fn current_serial_bit(&self) -> u8 {
        if self.mouse_mode {
            self.current_serial_bit_mouse()
        } else {
            self.current_serial_bit_controller()
        }
    }

    fn enable_mouse_mode(&mut self) {
        self.mouse_mode = true;
    }

    pub fn read(&mut self, is_dummy_read: bool) -> u8 {
        let bit = self.current_serial_bit();

        if self.mouse_mode && self.strobe && !is_dummy_read {
            self.mouse_speed ^= 0x01;
        } else if !self.strobe && !is_dummy_read {
            self.bit_index = self.bit_index.saturating_add(1);
        }

        if bit != 0 { 0x02 } else { 0x00 }
    }

    pub fn set_button(&mut self, button: crate::input::Button, pressed: bool) {
        if let Some(bit) = Self::button_bit_for_nes_button(button) {
            if pressed {
                self.button_states |= 1u16 << bit;
            } else {
                self.button_states &= !(1u16 << bit);
            }
        }
    }

    pub fn capture_state(&self) -> SnesAdapterState {
        SnesAdapterState {
            strobe: self.strobe,
            bit_index: self.bit_index,
            button_states: self.button_states,
        }
    }

    pub fn restore_state(&mut self, state: &SnesAdapterState) {
        self.strobe = state.strobe;
        self.bit_index = state.bit_index;
        self.button_states = state.button_states;
    }
}

impl crate::input::Controller for SnesAdapter {
    fn write_strobe(&mut self, value: u8) {
        self.write_strobe(value)
    }

    fn read(&mut self, is_dummy_read: bool) -> u8 {
        self.read(is_dummy_read)
    }

    fn capture_state(&self) -> crate::input::ControllerState {
        crate::input::ControllerState::SnesAdapter(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::input::ControllerState) {
        if let crate::input::ControllerState::SnesAdapter(snes_state) = state {
            self.restore_state(snes_state);
        }
    }

    fn set_button(&mut self, button: crate::input::Button, pressed: bool) -> bool {
        self.set_button(button, pressed);
        true
    }

    fn set_mouse_x_position(&mut self, _position: u8) -> bool {
        self.enable_mouse_mode();
        true
    }

    fn set_mouse_y_position(&mut self, _position: u8) -> bool {
        self.enable_mouse_mode();
        true
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.enable_mouse_mode();
        self.mouse_left_button = pressed;
        true
    }

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::SnesAdapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snes_adapter_serial_stream_defaults_to_expected_padding() {
        let mut adapter = SnesAdapter::new();

        adapter.write_strobe(1);
        adapter.write_strobe(0);

        let mut bits = Vec::new();
        for _ in 0..24 {
            bits.push((adapter.read(false) >> 1) & 0x01);
        }

        assert_eq!(&bits[0..8], &[0; 8]);
        assert_eq!(&bits[8..12], &[1, 1, 1, 1]);
        assert_eq!(&bits[12..16], &[0, 0, 0, 0]);
        assert_eq!(&bits[16..24], &[1; 8]);
    }

    #[test]
    fn snes_adapter_maps_b_to_first_bit_and_right_to_eighth_bit() {
        let mut adapter = SnesAdapter::new();
        adapter.set_button(crate::input::Button::B, true);
        adapter.set_button(crate::input::Button::Right, true);

        adapter.write_strobe(1);
        adapter.write_strobe(0);

        let first = (adapter.read(false) >> 1) & 0x01;
        for _ in 0..6 {
            adapter.read(false);
        }
        let eighth = (adapter.read(false) >> 1) & 0x01;

        assert_eq!(first, 1);
        assert_eq!(eighth, 1);
    }
}
