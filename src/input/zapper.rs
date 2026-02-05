use super::ControllerInput;
use crate::console::ZapperState;
use crate::input::Button;

/// NES Zapper controller.
///
/// Minimal implementation for save-state support and mouse-driven trigger.
pub struct Zapper {
    x: u8,
    y: u8,
    trigger: bool,
    light: bool,
}

impl Default for Zapper {
    fn default() -> Self {
        Self::new()
    }
}

impl Zapper {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            trigger: false,
            light: false,
        }
    }

    pub fn new_boxed() -> Box<dyn crate::input::Controller> {
        Box::new(Self::new())
    }

    pub fn capture_state(&self) -> ZapperState {
        ZapperState {
            x: self.x,
            y: self.y,
            trigger: self.trigger,
            light: self.light,
        }
    }

    pub fn restore_state(&mut self, state: &ZapperState) {
        self.x = state.x;
        self.y = state.y;
        self.trigger = state.trigger;
        self.light = state.light;
    }
}

impl crate::input::Controller for Zapper {
    fn write_strobe(&mut self, _value: u8) {}

    fn read(&mut self) -> u8 {
        self.read_no_clock()
    }

    fn read_no_clock(&self) -> u8 {
        let trigger_bit = (self.trigger as u8) << 3;
        let light_bit = if self.light { 0 } else { 1 << 4 };
        trigger_bit | light_bit
    }

    fn capture_state(&self) -> crate::input::ControllerState {
        crate::input::ControllerState::Zapper(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::input::ControllerState) {
        if let crate::input::ControllerState::Zapper(zapper_state) = state {
            self.restore_state(zapper_state);
        }
    }

    fn new_boxed() -> Box<dyn crate::input::Controller>
    where
        Self: Sized,
    {
        Self::new_boxed()
    }

    fn set_button(&mut self, _button: Button, _pressed: bool) -> bool {
        false
    }

    fn set_mouse_x_position(&mut self, position: u8) -> bool {
        self.x = position;
        true
    }

    fn set_mouse_y_position(&mut self, position: u8) -> bool {
        self.y = position;
        true
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.trigger = pressed;
        true
    }

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::Zapper)
    }
}

#[cfg(test)]
mod tests {
    use super::Zapper;
    use crate::input::Controller;

    #[test]
    fn test_zapper_trigger_and_light_bits() {
        let mut zapper = Zapper::new();

        zapper.set_mouse_left_button(true);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 1);
        assert_eq!((value >> 4) & 0x01, 1);

        zapper.set_mouse_left_button(false);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 0);
    }

    #[test]
    fn test_zapper_light_bit_clears_on_light() {
        let mut zapper = Zapper::new();
        zapper.restore_state(&crate::console::ZapperState {
            x: 0,
            y: 0,
            trigger: false,
            light: true,
        });

        let value = zapper.read_no_clock();
        assert_eq!((value >> 4) & 0x01, 0);
    }

    #[test]
    fn test_zapper_capture_restore_roundtrip() {
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(0x22);
        zapper.set_mouse_y_position(0x77);
        zapper.set_mouse_left_button(true);

        let state = zapper.capture_state();

        let mut restored = Zapper::new();
        restored.restore_state(&state);

        let restored_state = restored.capture_state();
        assert_eq!(restored_state.x, 0x22);
        assert_eq!(restored_state.y, 0x77);
        assert!(restored_state.trigger);
    }
}
