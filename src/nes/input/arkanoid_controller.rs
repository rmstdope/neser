/// NES Arkanoid controller.
///
/// The Arkanoid controller provides a serialized position value on bit 4 of $4016 reads and
/// uses bit 3 for the trigger/button. Position is latched on strobe and shifted
/// out MSB-first (inverted) when strobe is low.
use super::ControllerInput;
use serde::{Deserialize, Serialize};

/// Arkanoid controller state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArkanoidState {
    pub strobe: bool,
    pub shift_index: u8,
    pub position: u8,
    pub latched_position: u8,
    pub trigger: bool,
    pub enabled: bool,
}

pub struct ArkanoidController {
    strobe: bool,
    shift_index: u8,
    position: u8,
    latched_position: u8,
    trigger: bool,
}

impl Default for ArkanoidController {
    fn default() -> Self {
        Self::new()
    }
}

impl ArkanoidController {
    const MIN_POSITION: u8 = 0x62;
    const MAX_POSITION: u8 = 0xF2;
    pub fn new() -> Self {
        Self {
            strobe: false,
            shift_index: 0,
            position: Self::MIN_POSITION,
            latched_position: Self::MIN_POSITION,
            trigger: false,
        }
    }

    pub fn set_position(&mut self, position: u8) {
        self.position = position.clamp(Self::MIN_POSITION, Self::MAX_POSITION);
    }

    pub fn set_trigger(&mut self, pressed: bool) {
        self.trigger = pressed;
    }

    /// Write to strobe register ($4016).
    /// When strobe is high, the position is latched and the shift index is reset.
    pub fn write_strobe(&mut self, value: u8) {
        let new_strobe = value & 0x01 != 0;

        if new_strobe {
            self.latched_position = self.position;
            self.shift_index = 0;
        } else if self.strobe && !new_strobe {
            self.shift_index = 0;
        }

        self.strobe = new_strobe;
    }

    /// Read paddle state, optionally clocking the shift register.
    /// Bit 4 = position serial, bit 3 = trigger.
    pub fn read(&mut self, is_dummy_read: bool) -> u8 {
        let position = self
            .latched_position
            .clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        let inverted = position ^ 0xFF;
        let bit = if self.shift_index >= 8 {
            1
        } else {
            let bit_index = 7u8.saturating_sub(self.shift_index);
            (inverted >> bit_index) & 0x01
        };

        let response = (bit << 4) | ((self.trigger as u8) << 3);

        if !self.strobe && !is_dummy_read {
            self.shift_index = self.shift_index.saturating_add(1);
        }

        response
    }

    /// Read fire button state for Famicom expansion port ($4016 bit 1).
    pub fn read_expansion_trigger(&self) -> u8 {
        (self.trigger as u8) << 1
    }

    /// Read serial knob data for Famicom expansion port ($4017 bit 1).
    /// Advances the shift register on non-dummy reads when strobe is low.
    pub fn read_expansion_knob(&mut self, is_dummy_read: bool) -> u8 {
        let position = self
            .latched_position
            .clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        let inverted = position ^ 0xFF;
        let bit = if self.shift_index >= 8 {
            1
        } else {
            let bit_index = 7u8.saturating_sub(self.shift_index);
            (inverted >> bit_index) & 0x01
        };

        if !self.strobe && !is_dummy_read {
            self.shift_index = self.shift_index.saturating_add(1);
        }

        bit << 1
    }

    /// Capture current paddle state for save-state.
    pub fn capture_state(&self) -> ArkanoidState {
        ArkanoidState {
            strobe: self.strobe,
            shift_index: self.shift_index,
            position: self.position,
            latched_position: self.latched_position,
            trigger: self.trigger,
            enabled: false,
        }
    }

    /// Restore paddle state from a save-state.
    pub fn restore_state(&mut self, state: &ArkanoidState) {
        self.strobe = state.strobe;
        self.shift_index = state.shift_index;
        self.position = state.position.clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        self.latched_position = state
            .latched_position
            .clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        self.trigger = state.trigger;
    }
}

impl crate::nes::input::Controller for ArkanoidController {
    fn write_strobe(&mut self, value: u8) {
        self.write_strobe(value)
    }

    fn read(&mut self, is_dummy_read: bool) -> u8 {
        self.read(is_dummy_read)
    }

    fn capture_state(&self) -> crate::nes::input::ControllerState {
        crate::nes::input::ControllerState::Paddle(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::nes::input::ControllerState) {
        if let crate::nes::input::ControllerState::Paddle(paddle_state) = state {
            self.restore_state(paddle_state);
        }
    }

    fn set_button(&mut self, _button: crate::nes::input::Button, _pressed: bool) -> bool {
        false // Not supported for Paddle
    }

    fn set_mouse_x_position(&mut self, position: u8) -> bool {
        self.set_position(position);
        true
    }

    fn set_mouse_y_position(&mut self, _position: u8) -> bool {
        false
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.set_trigger(pressed);
        true
    }

    fn input_type(&self) -> ControllerInput {
        crate::nes::input::controller_input_type(crate::nes::input::ControllerType::Arkanoid)
    }
}

#[cfg(test)]
mod tests {
    use super::ArkanoidController;

    #[test]
    fn test_paddle_serializes_position_msb_first() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x92); // 0b1001_0010 -> inverted 0b0110_1101

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        let bits = [0, 1, 1, 0, 1, 1, 0, 1];
        for expected in bits {
            let value = paddle.read(false);
            assert_eq!((value >> 4) & 0x01, expected);
        }

        let value = paddle.read(false);
        assert_eq!((value >> 4) & 0x01, 1);
    }

    #[test]
    fn test_paddle_strobe_holds_first_bit() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x80); // inverted MSB = 0

        paddle.write_strobe(1);
        let first = paddle.read(false);
        let second = paddle.read(false);

        assert_eq!((first >> 4) & 0x01, 0);
        assert_eq!((second >> 4) & 0x01, 0);
    }

    #[test]
    fn test_paddle_trigger_bit() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x00);

        paddle.write_strobe(1);
        paddle.set_trigger(true);
        let value = paddle.read(false);
        assert_eq!((value >> 3) & 0x01, 1);

        paddle.set_trigger(false);
        let value = paddle.read(false);
        assert_eq!((value >> 3) & 0x01, 0);
    }

    #[test]
    fn test_paddle_position_clamps_to_valid_range() {
        let mut paddle = ArkanoidController::new();

        let read_position = |paddle: &mut ArkanoidController| {
            let mut position = 0u8;
            for bit_index in (0..8).rev() {
                let value = paddle.read(false);
                let bit = (value >> 4) & 0x01;
                position |= bit << bit_index;
            }
            position
        };

        paddle.set_position(0x20);
        paddle.write_strobe(1);
        paddle.write_strobe(0);
        let low = read_position(&mut paddle);
        assert_eq!(low, 0x9D);

        paddle.set_position(0xFF);
        paddle.write_strobe(1);
        paddle.write_strobe(0);
        let high = read_position(&mut paddle);
        assert_eq!(high, 0x0D);
    }

    #[test]
    fn test_expansion_trigger_returns_fire_on_bit1() {
        let mut paddle = ArkanoidController::new();

        // Trigger not pressed
        let value = paddle.read_expansion_trigger();
        assert_eq!(value, 0x00);

        // Trigger pressed
        paddle.set_trigger(true);
        let value = paddle.read_expansion_trigger();
        assert_eq!(value, 0x02);

        // Trigger released
        paddle.set_trigger(false);
        let value = paddle.read_expansion_trigger();
        assert_eq!(value, 0x00);
    }

    #[test]
    fn test_expansion_knob_returns_serial_data_on_bit1() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x92); // 0b1001_0010 -> inverted 0b0110_1101

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        // Expected bits (MSB first, inverted): 0, 1, 1, 0, 1, 1, 0, 1
        let expected_bits = [0, 1, 1, 0, 1, 1, 0, 1];
        for expected in expected_bits {
            let value = paddle.read_expansion_knob(false);
            assert_eq!(
                (value >> 1) & 0x01,
                expected,
                "Expected bit {} on bit 1, got value 0x{:02X}",
                expected,
                value
            );
        }
    }

    #[test]
    fn test_expansion_knob_advances_shift_register() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x92);

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        // Read all 8 bits
        for _ in 0..8 {
            paddle.read_expansion_knob(false);
        }

        // 9th+ reads should return 1 on bit 1
        let value = paddle.read_expansion_knob(false);
        assert_eq!(value & 0x02, 0x02);
    }

    #[test]
    fn test_expansion_knob_dummy_read_does_not_advance() {
        let mut paddle = ArkanoidController::new();
        paddle.set_position(0x92);

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        // Read first bit (non-dummy), advances shift index from 0 to 1
        let first = paddle.read_expansion_knob(false);
        // Dummy read at index 1 should NOT advance the shift index
        let dummy = paddle.read_expansion_knob(true);
        // Next non-dummy read should return the same bit as the dummy
        // (both read from index 1), proving the dummy did not consume a position
        let second = paddle.read_expansion_knob(false);

        // Dummy and second should match — both read from the same shift position
        assert_eq!(dummy, second);

        // Verify against a reference paddle that reads normally
        let mut expected_paddle = ArkanoidController::new();
        expected_paddle.set_position(0x92);
        expected_paddle.write_strobe(1);
        expected_paddle.write_strobe(0);
        let expected_first = expected_paddle.read_expansion_knob(false);
        let expected_second = expected_paddle.read_expansion_knob(false);
        assert_eq!(first, expected_first);
        assert_eq!(second, expected_second);
    }
}
