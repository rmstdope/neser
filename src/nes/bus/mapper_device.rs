use crate::debugging::log_info;
use crate::nes::bus::bus::BusDevice;
use crate::nes::cartridge::Cartridge;
use crate::nes::ppu;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct MapperDevice {
    cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
    ppu: Rc<RefCell<ppu::Ppu>>,
}

impl MapperDevice {
    pub(crate) fn new(
        cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
        ppu: Rc<RefCell<ppu::Ppu>>,
    ) -> Self {
        Self { cartridge, ppu }
    }
}

impl BusDevice for MapperDevice {
    fn read(&mut self, addr: u16, open_bus: u8, _is_dummy_read: bool) -> Option<u8> {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return match addr {
                0x4020..=0x5FFF => Some(open_bus),
                0x6000..=0x7FFF => {
                    log_info(format!(
                        "Warning: Read from PRG-RAM {:04X} without cartridge, returning 0",
                        addr
                    ));
                    Some(0)
                }
                0x8000..=0xFFFF => panic!("No cartridge mapped, cannot read from {:04X}", addr),
                _ => None,
            };
        };

        Some(
            cartridge
                .borrow()
                .mapper()
                .read_prg_open_bus(addr, open_bus),
        )
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            match addr {
                0x4020..=0x5FFF => log_info(format!(
                    "Warning: Write to mapper expansion area {:04X} without cartridge, ignored",
                    addr
                )),
                0x6000..=0x7FFF => log_info(format!(
                    "Warning: Write to PRG-RAM {:04X} without cartridge, ignored",
                    addr
                )),
                0x8000..=0xFFFF => log_info(format!(
                    "Warning: Write to PRG ROM area {:04X} without cartridge, ignored",
                    addr
                )),
                _ => {}
            }
            return true;
        };

        let old_mirroring = cartridge.borrow().mapper().get_mirroring();
        cartridge.borrow_mut().mapper_mut().write_prg(addr, value);
        let new_mirroring = cartridge.borrow().mapper().get_mirroring();
        if new_mirroring != old_mirroring {
            self.ppu.borrow_mut().set_mirroring(new_mirroring);
        }

        true
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4020..=0xFFFF
    }
}
