use crate::bus::bus::BusDevice;
use crate::input::{Controller, ControllerInput};
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct ControllerDevice {
    controllers: [Rc<RefCell<Box<dyn Controller>>>; 2],
}

impl ControllerDevice {
    pub(crate) fn new(
        port1_controller: Rc<RefCell<Box<dyn Controller>>>,
        port2_controller: Rc<RefCell<Box<dyn Controller>>>,
    ) -> Self {
        Self {
            controllers: [port1_controller, port2_controller],
        }
    }
}

impl BusDevice for ControllerDevice {
    fn read(&mut self, addr: u16, open_bus: u8, is_dummy_read: bool) -> Option<u8> {
        let index = (addr - 0x4016) as usize;

        let controller_state = self.controllers[index].borrow_mut().read(is_dummy_read);
        // Determine mask based on controller type.
        // Joypad uses bit 0 (mask 0xFE), Arkanoid and Zapper controller uses bits 4-3 (mask 0xE7).
        let is_mouse = self.controllers[index].borrow().input_type() == ControllerInput::Mouse;
        let mask = if is_mouse { 0xE7 } else { 0xFE };
        Some((open_bus & mask) | controller_state)
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        match addr {
            0x4016 => {
                self.controllers[0].borrow_mut().write_strobe(value);
                self.controllers[1].borrow_mut().write_strobe(value);
                true
            }
            0x4017 => false,
            _ => false,
        }
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4016..=0x4017
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Button;

    struct TestController {
        reads: Rc<RefCell<u32>>,
        dummy_reads: Rc<RefCell<u32>>,
    }

    impl TestController {
        fn new(reads: Rc<RefCell<u32>>, dummy_reads: Rc<RefCell<u32>>) -> Self {
            Self { reads, dummy_reads }
        }
    }

    impl Controller for TestController {
        fn write_strobe(&mut self, _value: u8) {}

        fn read(&mut self, is_dummy_read: bool) -> u8 {
            if is_dummy_read {
                *self.dummy_reads.borrow_mut() += 1;
            } else {
                *self.reads.borrow_mut() += 1;
            }
            0
        }

        fn capture_state(&self) -> crate::input::ControllerState {
            crate::input::ControllerState::Joypad(crate::input::JoypadState {
                strobe: false,
                button_index: 0,
                button_states: 0,
            })
        }

        fn restore_state(&mut self, _state: &crate::input::ControllerState) {}

        fn set_button(&mut self, _button: Button, _pressed: bool) -> bool {
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
            ControllerInput::Gamepad
        }
    }

    #[test]
    fn test_dummy_read_uses_no_clock() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller = Rc::new(RefCell::new(Box::new(TestController::new(
            reads.clone(),
            dummy_reads.clone(),
        )) as Box<dyn Controller>));

        let mut device = ControllerDevice::new(controller.clone(), controller);

        device.read(0x4016, 0xFF, true);

        assert_eq!(*reads.borrow(), 0);
        assert_eq!(*dummy_reads.borrow(), 1);
    }
}
