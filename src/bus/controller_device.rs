use crate::bus::bus::BusDevice;
use crate::input::Controller;
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
        // NES-001 open bus: only bits 5-7 are unconnected (open bus).
        // Bits 0-4 are driven by the controller I/O register:
        //   bit 0 = serial data, bits 1-2 = grounded, bits 3-4 = controller port.
        Some((open_bus & 0xE0) | controller_state)
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
    use crate::input::{Button, ControllerInput};

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

    fn create_test_controller_device() -> ControllerDevice {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        ControllerDevice::new(controller1, controller2)
    }

    #[test]
    fn test_dummy_read_uses_no_clock() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let reads_check = reads.clone();
        let dummy_reads_check = dummy_reads.clone();
        let controller1 = Rc::new(RefCell::new(Box::new(TestController::new(
            reads.clone(),
            dummy_reads.clone(),
        )) as Box<dyn Controller>));
        let controller2 = Rc::new(RefCell::new(
            Box::new(TestController::new(reads, dummy_reads)) as Box<dyn Controller>,
        ));

        let mut device = ControllerDevice::new(controller1, controller2);

        device.read(0x4016, 0xFF, true);

        assert_eq!(*reads_check.borrow(), 0);
        assert_eq!(*dummy_reads_check.borrow(), 1);
    }

    /// On NES-001, only bits 5-7 of $4016/$4017 are open bus.
    /// Bits 0-4 are driven by the controller I/O register.
    /// With open_bus = $BF and controller returning 0,
    /// the result should be $A0 (bits 5,7 from open bus).
    #[test]
    fn test_gamepad_open_bus_only_on_bits_5_to_7() {
        let mut device = create_test_controller_device();

        let result = device.read(0x4016, 0xBF, false).unwrap();
        assert_eq!(
            result, 0xA0,
            "Expected $A0 (only bits 5-7 from open bus), got ${:02X}",
            result
        );
    }

    /// With open_bus = $40, only bits 5-7 should reflect open bus.
    /// Controller returns 0, so result should be $40.
    #[test]
    fn test_gamepad_open_bus_with_40() {
        let mut device = create_test_controller_device();

        let result = device.read(0x4016, 0x40, false).unwrap();
        assert_eq!(
            result, 0x40,
            "Expected $40 (bits 5-7 from open bus $40), got ${:02X}",
            result
        );
    }
}
