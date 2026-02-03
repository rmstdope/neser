/// NES Arkanoid paddle controller.
///
/// The paddle provides a serialized position value on bit 4 of $4016 reads and
/// uses bit 3 for the trigger/button. Position is latched on strobe and shifted
/// out MSB-first (inverted) when strobe is low.
pub struct Paddle {
    strobe: bool,
    shift_index: u8,
    position: u8,
    latched_position: u8,
    trigger: bool,
}

impl Default for Paddle {
    fn default() -> Self {
        Self::new()
    }
}

impl Paddle {
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

    #[allow(dead_code)]
    pub fn set_position(&mut self, position: u8) {
        self.position = position.clamp(Self::MIN_POSITION, Self::MAX_POSITION);
    }

    #[allow(dead_code)]
    pub fn set_trigger(&mut self, pressed: bool) {
        println!("Paddle trigger set to {}", pressed);
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
    pub fn read(&mut self) -> u8 {
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

        if !self.strobe {
            self.shift_index = self.shift_index.saturating_add(1);
        }

        response
    }

    /// Read paddle state without clocking the shift register.
    pub fn read_no_clock(&self) -> u8 {
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

        (bit << 4) | ((self.trigger as u8) << 3)
    }

    /// Capture current paddle state for save-state.
    pub fn capture_state(&self) -> crate::console::PaddleState {
        crate::console::PaddleState {
            strobe: self.strobe,
            shift_index: self.shift_index,
            position: self.position,
            latched_position: self.latched_position,
            trigger: self.trigger,
            enabled: false,
        }
    }

    /// Restore paddle state from a save-state.
    pub fn restore_state(&mut self, state: &crate::console::PaddleState) {
        self.strobe = state.strobe;
        self.shift_index = state.shift_index;
        self.position = state.position.clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        self.latched_position = state
            .latched_position
            .clamp(Self::MIN_POSITION, Self::MAX_POSITION);
        self.trigger = state.trigger;
    }
}

#[cfg(test)]
mod tests {
    use super::Paddle;

    #[test]
    fn test_paddle_serializes_position_msb_first() {
        let mut paddle = Paddle::new();
        paddle.set_position(0x92); // 0b1001_0010 -> inverted 0b0110_1101

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        let bits = [0, 1, 1, 0, 1, 1, 0, 1];
        for expected in bits {
            let value = paddle.read();
            assert_eq!((value >> 4) & 0x01, expected);
        }

        let value = paddle.read();
        assert_eq!((value >> 4) & 0x01, 1);
    }

    #[test]
    fn test_paddle_strobe_holds_first_bit() {
        let mut paddle = Paddle::new();
        paddle.set_position(0x80); // inverted MSB = 0

        paddle.write_strobe(1);
        let first = paddle.read();
        let second = paddle.read();

        assert_eq!((first >> 4) & 0x01, 0);
        assert_eq!((second >> 4) & 0x01, 0);
    }

    #[test]
    fn test_paddle_trigger_bit() {
        let mut paddle = Paddle::new();
        paddle.set_position(0x00);

        paddle.write_strobe(1);
        paddle.set_trigger(true);
        let value = paddle.read();
        assert_eq!((value >> 3) & 0x01, 1);

        paddle.set_trigger(false);
        let value = paddle.read();
        assert_eq!((value >> 3) & 0x01, 0);
    }

    #[test]
    fn test_paddle_position_clamps_to_valid_range() {
        let mut paddle = Paddle::new();

        let read_position = |paddle: &mut Paddle| {
            let mut position = 0u8;
            for bit_index in (0..8).rev() {
                let value = paddle.read();
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
}
