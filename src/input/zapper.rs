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
        0
    }

    fn read_no_clock(&self) -> u8 {
        0
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

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.trigger = pressed;
        true
    }

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::Zapper)
    }
}
