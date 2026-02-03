/// NES Arkanoid paddle controller.
///
/// The paddle provides a serialized position value on bit 4 of $4016 reads and
/// uses bit 3 for the trigger/button. Position is latched on strobe and shifted
/// out LSB-first when strobe is low.
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
    pub fn new() -> Self {
        Self {
            strobe: false,
            shift_index: 0,
            position: 0,
            latched_position: 0,
            trigger: false,
        }
    }

    #[allow(dead_code)]
    pub fn set_position(&mut self, position: u8) {
        self.position = position;
    }

    #[allow(dead_code)]
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
    pub fn read(&mut self) -> u8 {
        let bit = if self.shift_index >= 8 {
            1
        } else {
            (self.latched_position >> self.shift_index) & 0x01
        };

        let response = (bit << 4) | ((self.trigger as u8) << 3);

        if !self.strobe {
            self.shift_index = self.shift_index.saturating_add(1);
        }

        response
    }

    /// Read paddle state without clocking the shift register.
    pub fn read_no_clock(&self) -> u8 {
        let bit = if self.shift_index >= 8 {
            1
        } else {
            (self.latched_position >> self.shift_index) & 0x01
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
        self.position = state.position;
        self.latched_position = state.latched_position;
        self.trigger = state.trigger;
    }
}

#[cfg(test)]
mod tests {
    use super::Paddle;

    #[test]
    fn test_paddle_serializes_position_lsb_first() {
        let mut paddle = Paddle::new();
        paddle.set_position(0xA5); // 0b1010_0101 (LSB-first: 1,0,1,0,0,1,0,1)

        paddle.write_strobe(1);
        paddle.write_strobe(0);

        let bits = [1, 0, 1, 0, 0, 1, 0, 1];
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
        paddle.set_position(0x02); // bit0 = 0

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
}
