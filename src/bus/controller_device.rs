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
    fn read(&mut self, addr: u16, open_bus: u8, clock_joypads: bool) -> Option<u8> {
        let index = (addr - 0x4016) as usize;
        if index >= self.controllers.len() {
            return None;
        }

        let is_mouse = self.controllers[index].borrow().input_type() == ControllerInput::Mouse;
        let controller_state = if clock_joypads {
            self.controllers[index].borrow_mut().read()
        } else {
            self.controllers[index].borrow().read_no_clock()
        };
        // Determine mask based on controller type.
        // Joypad uses bit 0 (mask 0xFE), Arkanoid and Zapper controller uses bits 4-3 (mask 0xE7).
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
