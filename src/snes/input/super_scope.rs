use super::{SnesButton, SnesController, SnesControllerState};

#[derive(Debug, Clone, Default)]
pub struct SuperScopeController {
    x: i16,
    y: i16,
    trigger_pressed: bool,
    cursor_pressed: bool,
    turbo_pressed: bool,
    pause_pressed: bool,
    turbo_enabled: bool,
    turbo_lock: bool,
    trigger_output: bool,
    pause_output: bool,
    offscreen: bool,
    trigger_lock: bool,
    pause_lock: bool,
    latched: bool,
    counter: u8,
}

impl SuperScopeController {
    pub fn new() -> Self {
        Self {
            x: 128,
            y: 120,
            ..Self::default()
        }
    }

    fn update_input_state(&mut self) {
        self.offscreen = self.x < 0 || self.y < 0 || self.x >= 256 || self.y >= 224;
    }

    fn current_bit(&self) -> bool {
        match self.counter {
            0 => {
                if self.offscreen {
                    false
                } else {
                    self.trigger_output
                }
            }
            1 => self.cursor_pressed,
            2 => self.turbo_enabled,
            3 => self.pause_output,
            4 | 5 => false,
            6 => self.offscreen,
            7 => false,
            _ => true,
        }
    }
}

impl SnesController for SuperScopeController {
    fn write_strobe(&mut self, high: bool) {
        self.latched = high;
        if high {
            self.counter = 0;
        }
    }

    fn read(&mut self) -> (bool, bool) {
        if self.counter == 0 {
            if self.turbo_pressed && !self.turbo_lock {
                self.turbo_enabled = !self.turbo_enabled;
            }
            self.turbo_lock = self.turbo_pressed;

            if self.trigger_pressed {
                self.trigger_output = self.turbo_enabled || !self.trigger_lock;
                self.trigger_lock = true;
            } else {
                self.trigger_output = false;
                self.trigger_lock = false;
            }

            if self.pause_pressed {
                self.pause_output = !self.pause_lock;
                self.pause_lock = true;
            } else {
                self.pause_output = false;
                self.pause_lock = false;
            }

            self.update_input_state();
        }

        let bit = self.current_bit();
        if !self.latched && self.counter < u8::MAX {
            self.counter = self.counter.saturating_add(1);
        }
        (bit, false)
    }

    fn set_button(&mut self, _button: SnesButton, _pressed: bool) -> bool {
        false
    }

    fn set_superscope_position(&mut self, x: i16, y: i16) -> bool {
        self.x = x;
        self.y = y;
        self.update_input_state();
        true
    }

    fn set_superscope_trigger(&mut self, pressed: bool) -> bool {
        self.trigger_pressed = pressed;
        true
    }

    fn set_superscope_cursor(&mut self, pressed: bool) -> bool {
        self.cursor_pressed = pressed;
        true
    }

    fn set_superscope_turbo(&mut self, pressed: bool) -> bool {
        self.turbo_pressed = pressed;
        true
    }

    fn set_superscope_pause(&mut self, pressed: bool) -> bool {
        self.pause_pressed = pressed;
        true
    }

    fn is_superscope(&self) -> bool {
        true
    }

    fn capture_state(&self) -> SnesControllerState {
        SnesControllerState {
            superscope_x: self.x,
            superscope_y: self.y,
            superscope_trigger: self.trigger_pressed,
            superscope_cursor: self.cursor_pressed,
            superscope_turbo: self.turbo_pressed,
            superscope_pause: self.pause_pressed,
            superscope_offscreen: self.offscreen,
            superscope_turbo_enabled: self.turbo_enabled,
            superscope_turbo_lock: self.turbo_lock,
            superscope_trigger_output: self.trigger_output,
            superscope_pause_output: self.pause_output,
            superscope_trigger_lock: self.trigger_lock,
            superscope_pause_lock: self.pause_lock,
            superscope_latched: self.latched,
            shift: self.counter,
            strobe: self.latched,
            ..Default::default()
        }
    }

    fn restore_state(&mut self, state: &SnesControllerState) {
        self.x = state.superscope_x;
        self.y = state.superscope_y;
        self.trigger_pressed = state.superscope_trigger;
        self.cursor_pressed = state.superscope_cursor;
        self.turbo_pressed = state.superscope_turbo;
        self.pause_pressed = state.superscope_pause;
        self.offscreen = state.superscope_offscreen;
        self.turbo_enabled = state.superscope_turbo_enabled;
        self.turbo_lock = state.superscope_turbo_lock;
        self.trigger_output = state.superscope_trigger_output;
        self.pause_output = state.superscope_pause_output;
        self.trigger_lock = state.superscope_trigger_lock;
        self.pause_lock = state.superscope_pause_lock;
        self.latched = state.superscope_latched;
        self.counter = state.shift;
        self.latched = state.strobe;
    }
}

#[cfg(test)]
mod tests {
    use super::SuperScopeController;
    use crate::snes::input::SnesController;

    fn latch(scope: &mut SuperScopeController) {
        scope.write_strobe(true);
        scope.write_strobe(false);
    }

    #[test]
    fn serial_sequence_matches_the_documented_field_order() {
        let mut scope = SuperScopeController::new();
        scope.set_superscope_position(128, 120);
        scope.set_superscope_trigger(true);
        scope.set_superscope_cursor(true);
        scope.set_superscope_turbo(false);
        scope.set_superscope_pause(true);
        latch(&mut scope);

        let mut bits = Vec::new();
        for _ in 0..9 {
            bits.push(scope.read().0);
        }

        assert_eq!(
            bits,
            [true, true, false, true, false, false, false, false, true]
        );
    }
}
