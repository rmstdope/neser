use crate::bus::bus::BusDevice;
use crate::input::{Controller, ControllerState};
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct ControllerDevice {
    port1_controller: Rc<RefCell<Box<dyn Controller>>>,
    port2_controller: Rc<RefCell<Box<dyn Controller>>>,
}

impl ControllerDevice {
    pub(crate) fn new(
        port1_controller: Rc<RefCell<Box<dyn Controller>>>,
        port2_controller: Rc<RefCell<Box<dyn Controller>>>,
    ) -> Self {
        Self {
            port1_controller,
            port2_controller,
        }
    }
}

impl BusDevice for ControllerDevice {
    fn read(&mut self, addr: u16, open_bus: u8, clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        match addr {
            0x4016 => {
                let is_paddle = matches!(
                    self.port1_controller.borrow().capture_state(),
                    ControllerState::Paddle(_)
                );
                let controller_state = if clock_joypads {
                    self.port1_controller.borrow_mut().read()
                } else {
                    self.port1_controller.borrow().read_no_clock()
                };
                // Determine mask based on controller type.
                // Joypad uses bit 0 (mask 0xFE), Arkanoid controller uses bits 4-3 (mask 0xE7).
                let mask = if is_paddle { 0xE7 } else { 0xFE };
                Some((open_bus & mask) | controller_state)
            }
            0x4017 => {
                let is_paddle = matches!(
                    self.port2_controller.borrow().capture_state(),
                    ControllerState::Paddle(_)
                );
                let controller_state = if clock_joypads {
                    self.port2_controller.borrow_mut().read()
                } else {
                    self.port2_controller.borrow().read_no_clock()
                };
                // Determine mask based on controller type.
                // Joypad uses bit 0 (mask 0xFE), Arkanoid controller uses bits 4-3 (mask 0xE7).
                let mask = if is_paddle { 0xE7 } else { 0xFE };
                Some((open_bus & mask) | controller_state)
            }
            _ => None,
        }
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        if !self.address_range().contains(&addr) {
            return false;
        }

        match addr {
            0x4016 => {
                self.port1_controller.borrow_mut().write_strobe(value);
                self.port2_controller.borrow_mut().write_strobe(value);
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
