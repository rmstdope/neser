use crate::bus::bus::BusDevice;
use crate::cartridge::Cartridge;
use crate::ppu;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct PpuDevice {
    ppu: Rc<RefCell<ppu::Ppu>>,
    cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
}

impl PpuDevice {
    pub(crate) fn new(
        ppu: Rc<RefCell<ppu::Ppu>>,
        cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
    ) -> Self {
        Self { ppu, cartridge }
    }
}

impl BusDevice for PpuDevice {
    fn read(&mut self, addr: u16, _clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        let reg = addr & 0x2007;
        match reg {
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => Some(self.ppu.borrow().io_bus()),
            0x2002 => Some(self.ppu.borrow_mut().get_status()),
            0x2004 => Some(self.ppu.borrow_mut().read_oam_data()),
            0x2007 => Some(self.ppu.borrow_mut().read_data()),
            _ => None,
        }
    }

    fn write(&mut self, addr: u16, value: u8, is_dummy_write: bool) -> bool {
        if !self.address_range().contains(&addr) {
            return false;
        }

        let reg = addr & 0x2007;
        match reg {
            0x2000 => {
                self.ppu.borrow_mut().write_control(value);
                if let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() {
                    cartridge.borrow_mut().mapper_mut().ppu_write_ctrl(value);
                }
                true
            }
            0x2001 => {
                self.ppu.borrow_mut().write_mask(value);
                if let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() {
                    cartridge.borrow_mut().mapper_mut().ppu_write_mask(value);
                }
                true
            }
            0x2002 => {
                self.ppu.borrow_mut().set_io_bus(value);
                true
            }
            0x2003 => {
                self.ppu.borrow_mut().write_oam_address(value);
                true
            }
            0x2004 => {
                self.ppu.borrow_mut().write_oam_data(value);
                true
            }
            0x2005 => {
                self.ppu.borrow_mut().write_scroll(value, is_dummy_write);
                true
            }
            0x2006 => {
                self.ppu.borrow_mut().write_address(value, is_dummy_write);
                true
            }
            0x2007 => {
                self.ppu.borrow_mut().write_data(value);
                true
            }
            _ => false,
        }
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x2000..=0x3FFF
    }
}
