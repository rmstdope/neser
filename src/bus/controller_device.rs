use crate::bus::bus::BusDevice;
use crate::input::{Controller, ControllerState};
use crate::ppu;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct ControllerDevice {
    port1_controller: Rc<RefCell<Box<dyn Controller>>>,
    port2_controller: Rc<RefCell<Box<dyn Controller>>>,
    ppu: Rc<RefCell<ppu::Ppu>>,
    zapper_light_radius: u8,
}

impl ControllerDevice {
    pub(crate) fn new(
        port1_controller: Rc<RefCell<Box<dyn Controller>>>,
        port2_controller: Rc<RefCell<Box<dyn Controller>>>,
        ppu: Rc<RefCell<ppu::Ppu>>,
        zapper_light_radius: u8,
    ) -> Self {
        Self {
            port1_controller,
            port2_controller,
            ppu,
            zapper_light_radius,
        }
    }
}

impl BusDevice for ControllerDevice {
    fn read(&mut self, addr: u16, open_bus: u8, clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        // Update PPU context for light gun controllers before reading
        {
            let ppu = self.ppu.borrow();
            let scanline = ppu.timing().scanline;
            let pixel = ppu.timing().pixel;
            let screen_buffer = ppu.screen_buffer();

            // Update both controllers (only light guns will use this)
            self.port1_controller.borrow_mut().set_ppu_context(
                scanline,
                pixel,
                screen_buffer,
                self.zapper_light_radius,
            );
            self.port2_controller.borrow_mut().set_ppu_context(
                scanline,
                pixel,
                screen_buffer,
                self.zapper_light_radius,
            );
        }

        match addr {
            0x4016 => {
                let is_mouse = matches!(
                    self.port1_controller.borrow().capture_state(),
                    ControllerState::Paddle(_) | ControllerState::Zapper(_)
                );
                let controller_state = if clock_joypads {
                    self.port1_controller.borrow_mut().read()
                } else {
                    self.port1_controller.borrow().read_no_clock()
                };
                // Determine mask based on controller type.
                // Joypad uses bit 0 (mask 0xFE), Arkanoid and Zapper controller uses bits 4-3 (mask 0xE7).
                let mask = if is_mouse { 0xE7 } else { 0xFE };
                Some((open_bus & mask) | controller_state)
            }
            0x4017 => {
                let is_mouse = matches!(
                    self.port2_controller.borrow().capture_state(),
                    ControllerState::Paddle(_) | ControllerState::Zapper(_)
                );
                let controller_state = if clock_joypads {
                    self.port2_controller.borrow_mut().read()
                } else {
                    self.port2_controller.borrow().read_no_clock()
                };
                // Determine mask based on controller type.
                // Joypad uses bit 0 (mask 0xFE), Arkanoid and Zapper controller uses bits 4-3 (mask 0xE7).
                let mask = if is_mouse { 0xE7 } else { 0xFE };
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
