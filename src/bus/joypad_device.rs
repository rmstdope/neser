use crate::bus::bus::BusDevice;
use crate::input::Joypad;
use std::ops::RangeInclusive;

pub(crate) struct JoypadDevice {
    joypad1: Joypad,
    joypad2: Joypad,
}

impl JoypadDevice {
    pub(crate) fn new() -> Self {
        Self {
            joypad1: Joypad::new(),
            joypad2: Joypad::new(),
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
                    self.joypad1.read()
                } else {
                    self.joypad1.read_no_clock()
                };
                Some((open_bus & 0xFE) | button_state)
            }
            0x4017 => {
                let button_state = if clock_joypads {
                    self.joypad2.read()
                } else {
                    self.joypad2.read_no_clock()
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
                self.joypad1.write_strobe(value);
                self.joypad2.write_strobe(value);
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
