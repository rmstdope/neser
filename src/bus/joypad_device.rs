use crate::bus::bus::BusDevice;
use crate::input::{Button, Joypad};
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct JoypadDevice {
    joypad1: Rc<RefCell<Joypad>>,
    joypad2: Rc<RefCell<Joypad>>,
}

impl JoypadDevice {
    pub(crate) fn new(joypad1: Rc<RefCell<Joypad>>, joypad2: Rc<RefCell<Joypad>>) -> Self {
        Self { joypad1, joypad2 }
    }

    pub(crate) fn set_button(&mut self, controller: u8, button: Button, pressed: bool) {
        match controller {
            1 => self.joypad1.borrow_mut().set_button(button, pressed),
            2 => self.joypad2.borrow_mut().set_button(button, pressed),
            _ => {}
        }
    }
}

impl BusDevice for JoypadDevice {
    fn read(&mut self, addr: u16, open_bus: u8, clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        match addr {
            0x4016 => {
                let button_state = if clock_joypads {
                    self.joypad1.borrow_mut().read()
                } else {
                    self.joypad1.borrow().read_no_clock()
                };
                Some((open_bus & 0xFE) | button_state)
            }
            0x4017 => {
                let button_state = if clock_joypads {
                    self.joypad2.borrow_mut().read()
                } else {
                    self.joypad2.borrow().read_no_clock()
                };
                Some((open_bus & 0xFE) | button_state)
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
                self.joypad1.borrow_mut().write_strobe(value);
                self.joypad2.borrow_mut().write_strobe(value);
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
