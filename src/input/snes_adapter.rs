use super::ControllerInput;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
enum SnesAdapterMode {
    Controller,
    Mouse,
}

/// SNES adapter controller state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnesAdapterState {
    mode: SnesAdapterMode,
    pub strobe: bool,
    pub bit_index: u8,
    pub button_states: u16,
    pub mouse_left_button: bool,
    pub mouse_right_button: bool,
    pub mouse_speed: u8,
    pub mouse_x_position: u8,
    pub mouse_y_position: u8,
    pub mouse_last_reported_x: u8,
    pub mouse_last_reported_y: u8,
    pub mouse_packet: [u8; 4],
}

/// Super NES controller connected through a NES pin adapter.
///
/// Serial protocol:
/// - bits 0..=11: SNES buttons (B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R)
/// - bits 12..=15: always 1
/// - bits 16+: always 1
///
/// The adapter exposes serial data on D0 (bit 0), matching the standard
/// NES controller port pin adapter wiring.
pub struct SnesAdapter {
    mode: SnesAdapterMode,
    strobe: bool,
    bit_index: u8,
    button_states: u16,
    mouse_left_button: bool,
    mouse_right_button: bool,
    mouse_speed: u8,
    mouse_x_position: u8,
    mouse_y_position: u8,
    mouse_last_reported_x: u8,
    mouse_last_reported_y: u8,
    mouse_packet: [u8; 4],
}

impl Default for SnesAdapter {
    fn default() -> Self {
        Self::new_controller()
    }
}

impl SnesAdapter {
    pub fn new() -> Self {
        Self::new_controller()
    }

    pub fn new_controller() -> Self {
        Self::new_with_mode(SnesAdapterMode::Controller)
    }

    pub fn new_mouse() -> Self {
        Self::new_with_mode(SnesAdapterMode::Mouse)
    }

    fn new_with_mode(mode: SnesAdapterMode) -> Self {
        Self {
            mode,
            strobe: false,
            bit_index: 0,
            button_states: 0,
            mouse_left_button: false,
            mouse_right_button: false,
            mouse_speed: 0,
            mouse_x_position: 0,
            mouse_y_position: 0,
            mouse_last_reported_x: 0,
            mouse_last_reported_y: 0,
            mouse_packet: [0; 4],
        }
    }

    fn is_mouse_mode(&self) -> bool {
        matches!(self.mode, SnesAdapterMode::Mouse)
    }

    pub fn write_strobe(&mut self, value: u8) {
        let new_strobe = (value & 0x01) != 0;

        if self.strobe && !new_strobe {
            self.bit_index = 0;
            if self.is_mouse_mode() {
                self.mouse_packet = self.build_mouse_packet();
            }
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

    fn button_bit_for_snes_button(button: crate::input::SnesButton) -> u8 {
        match button {
            crate::input::SnesButton::B => 0,
            crate::input::SnesButton::Y => 1,
            crate::input::SnesButton::Select => 2,
            crate::input::SnesButton::Start => 3,
            crate::input::SnesButton::Up => 4,
            crate::input::SnesButton::Down => 5,
            crate::input::SnesButton::Left => 6,
            crate::input::SnesButton::Right => 7,
            crate::input::SnesButton::A => 8,
            crate::input::SnesButton::X => 9,
            crate::input::SnesButton::L => 10,
            crate::input::SnesButton::R => 11,
        }
    }

    fn current_serial_bit_controller(&self) -> u8 {
        match self.bit_index {
            0..=11 => ((self.button_states >> self.bit_index) & 0x01) as u8,
            12..=15 => 0,
            _ => 1,
        }
    }

    fn current_serial_bit_mouse(&self) -> u8 {
        if self.bit_index < 32 {
            let byte = self.mouse_packet[(self.bit_index / 8) as usize];
            (byte >> (7 - (self.bit_index % 8))) & 0x01
        } else {
            1
        }
    }

    fn current_serial_bit(&self) -> u8 {
        if self.is_mouse_mode() {
            self.current_serial_bit_mouse()
        } else {
            self.current_serial_bit_controller()
        }
    }

    fn can_use_mouse_mode(&self) -> bool {
        self.is_mouse_mode()
    }

    fn enable_mouse_mode(&mut self) -> bool {
        self.can_use_mouse_mode()
    }

    fn to_signed_magnitude_delta(delta: i16) -> u8 {
        if delta < 0 {
            (delta.unsigned_abs() as u8) | 0x80
        } else {
            delta as u8
        }
    }

    fn build_mouse_packet(&mut self) -> [u8; 4] {
        let raw_dx = self.mouse_x_position as i16 - self.mouse_last_reported_x as i16;
        let raw_dy = self.mouse_y_position as i16 - self.mouse_last_reported_y as i16;

        let reported_dx = raw_dx.clamp(-127, 127);
        let reported_dy = raw_dy.clamp(-127, 127);

        self.mouse_last_reported_x =
            (self.mouse_last_reported_x as i16 + reported_dx).clamp(0, 255) as u8;
        self.mouse_last_reported_y =
            (self.mouse_last_reported_y as i16 + reported_dy).clamp(0, 255) as u8;

        let dx = Self::to_signed_magnitude_delta(reported_dx);
        let dy = Self::to_signed_magnitude_delta(reported_dy);

        let speed_bits = (self.mouse_speed & 0x03) << 4;
        let right_button_bit = if self.mouse_right_button { 0x80 } else { 0x00 };
        let left_button_bit = if self.mouse_left_button { 0x40 } else { 0x00 };

        [
            0x00,
            right_button_bit | left_button_bit | speed_bits | 0x01,
            dy,
            dx,
        ]
    }

    pub fn read(&mut self, is_dummy_read: bool) -> u8 {
        let bit = self.current_serial_bit();

        if self.is_mouse_mode() && self.strobe && !is_dummy_read {
            self.mouse_speed ^= 0x01;
        } else if !self.strobe && !is_dummy_read {
            self.bit_index = self.bit_index.saturating_add(1);
        }

        if bit != 0 { 0x01 } else { 0x00 }
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

    pub fn set_snes_button(&mut self, button: crate::input::SnesButton, pressed: bool) {
        let bit = Self::button_bit_for_snes_button(button);
        if pressed {
            self.button_states |= 1u16 << bit;
        } else {
            self.button_states &= !(1u16 << bit);
        }
    }

    pub fn capture_state(&self) -> SnesAdapterState {
        SnesAdapterState {
            mode: self.mode,
            strobe: self.strobe,
            bit_index: self.bit_index,
            button_states: self.button_states,
            mouse_left_button: self.mouse_left_button,
            mouse_right_button: self.mouse_right_button,
            mouse_speed: self.mouse_speed,
            mouse_x_position: self.mouse_x_position,
            mouse_y_position: self.mouse_y_position,
            mouse_last_reported_x: self.mouse_last_reported_x,
            mouse_last_reported_y: self.mouse_last_reported_y,
            mouse_packet: self.mouse_packet,
        }
    }

    pub fn restore_state(&mut self, state: &SnesAdapterState) {
        self.mode = state.mode;
        self.strobe = state.strobe;
        self.bit_index = state.bit_index;
        self.button_states = state.button_states;
        self.mouse_left_button = state.mouse_left_button;
        self.mouse_right_button = state.mouse_right_button;
        self.mouse_speed = state.mouse_speed;
        self.mouse_x_position = state.mouse_x_position;
        self.mouse_y_position = state.mouse_y_position;
        self.mouse_last_reported_x = state.mouse_last_reported_x;
        self.mouse_last_reported_y = state.mouse_last_reported_y;
        self.mouse_packet = state.mouse_packet;
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
        if self.is_mouse_mode() {
            return false;
        }
        self.set_button(button, pressed);
        true
    }

    fn set_snes_button(&mut self, button: crate::input::SnesButton, pressed: bool) -> bool {
        if self.is_mouse_mode() {
            return false;
        }
        self.set_snes_button(button, pressed);
        true
    }

    fn set_mouse_x_position(&mut self, position: u8) -> bool {
        if !self.enable_mouse_mode() {
            return false;
        }
        self.mouse_x_position = position;
        true
    }

    fn set_mouse_y_position(&mut self, position: u8) -> bool {
        if !self.enable_mouse_mode() {
            return false;
        }
        self.mouse_y_position = position;
        true
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        if !self.enable_mouse_mode() {
            return false;
        }
        self.mouse_left_button = pressed;
        true
    }

    fn add_mouse_delta(&mut self, dx: i16, dy: i16) -> bool {
        if !self.enable_mouse_mode() {
            return false;
        }

        self.mouse_x_position = (self.mouse_x_position as i16 + dx).clamp(0, 255) as u8;
        self.mouse_y_position = (self.mouse_y_position as i16 + dy).clamp(0, 255) as u8;
        true
    }

    fn set_mouse_right_button(&mut self, pressed: bool) -> bool {
        if !self.enable_mouse_mode() {
            return false;
        }
        self.mouse_right_button = pressed;
        true
    }

    fn is_snes_mouse(&self) -> bool {
        self.is_mouse_mode()
    }

    fn input_type(&self) -> ControllerInput {
        if self.is_mouse_mode() {
            ControllerInput::Mouse
        } else {
            ControllerInput::Gamepad
        }
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
            bits.push(adapter.read(false) & 0x01);
        }

        assert_eq!(&bits[0..12], &[0; 12]);
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

        let first = adapter.read(false) & 0x01;
        for _ in 0..6 {
            adapter.read(false);
        }
        let eighth = adapter.read(false) & 0x01;

        assert_eq!(first, 1);
        assert_eq!(eighth, 1);
    }

    #[test]
    fn snes_adapter_supports_y_x_l_r_bits() {
        let mut adapter = SnesAdapter::new_controller();
        adapter.set_snes_button(crate::input::SnesButton::Y, true);
        adapter.set_snes_button(crate::input::SnesButton::X, true);
        adapter.set_snes_button(crate::input::SnesButton::L, true);
        adapter.set_snes_button(crate::input::SnesButton::R, true);

        assert_eq!(adapter.button_states & (1 << 1), 1 << 1);
        assert_eq!(adapter.button_states & (1 << 9), 1 << 9);
        assert_eq!(adapter.button_states & (1 << 10), 1 << 10);
        assert_eq!(adapter.button_states & (1 << 11), 1 << 11);
    }

    #[test]
    fn snes_mouse_large_move_is_reported_over_multiple_packets() {
        let mut adapter = SnesAdapter::new_mouse();
        adapter.mouse_x_position = 255;
        adapter.mouse_y_position = 255;

        adapter.write_strobe(1);
        adapter.write_strobe(0);
        assert_eq!(adapter.mouse_packet[2], 127);
        assert_eq!(adapter.mouse_packet[3], 127);

        adapter.write_strobe(1);
        adapter.write_strobe(0);
        assert_eq!(adapter.mouse_packet[2], 127);
        assert_eq!(adapter.mouse_packet[3], 127);

        adapter.write_strobe(1);
        adapter.write_strobe(0);
        assert_eq!(adapter.mouse_packet[2], 1);
        assert_eq!(adapter.mouse_packet[3], 1);

        adapter.write_strobe(1);
        adapter.write_strobe(0);
        assert_eq!(adapter.mouse_packet[2], 0);
        assert_eq!(adapter.mouse_packet[3], 0);
    }

    #[test]
    fn snes_mouse_packet_sets_right_button_bit() {
        let mut adapter = SnesAdapter::new_mouse();
        adapter.mouse_right_button = true;
        adapter.write_strobe(1);
        adapter.write_strobe(0);

        assert_eq!(adapter.mouse_packet[1] & 0x80, 0x80);
    }

    #[test]
    fn snes_adapter_returns_serial_data_on_d0() {
        let mut adapter = SnesAdapter::new();
        adapter.set_snes_button(crate::input::SnesButton::B, true);

        adapter.write_strobe(1);
        adapter.write_strobe(0);

        // First serial bit = B button — should appear on D0 (bit 0)
        let value = adapter.read(false);
        assert_eq!(
            value & 0x01,
            0x01,
            "SNES adapter serial data must appear on D0 (bit 0), got 0x{:02X}",
            value
        );
    }
}
