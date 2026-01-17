use crate::bus::bus::BusDevice;
use crate::cartridge::Cartridge;
use crate::ppu;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct MapperDevice {
    cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
    ppu: Rc<RefCell<ppu::Ppu>>,
    open_bus: Rc<RefCell<u8>>,
}

impl MapperDevice {
    pub(crate) fn new(
        cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
        ppu: Rc<RefCell<ppu::Ppu>>,
        open_bus: Rc<RefCell<u8>>,
    ) -> Self {
        Self {
            cartridge,
            ppu,
            open_bus,
        }
    }
}

impl BusDevice for MapperDevice {
    fn read(&mut self, addr: u16, _clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        let open_bus = *self.open_bus.borrow();
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return match addr {
                0x5000..=0x5FFF => Some(open_bus),
                0x6000..=0x7FFF => {
                    eprintln!(
                        "Warning: Read from PRG-RAM {:04X} without cartridge, returning 0",
                        addr
                    );
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
        if !self.address_range().contains(&addr) {
            return false;
        }

        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            match addr {
                0x5000..=0x5FFF => eprintln!(
                    "Warning: Write to mapper expansion area {:04X} without cartridge, ignored",
                    addr
                ),
                0x6000..=0x7FFF => eprintln!(
                    "Warning: Write to PRG-RAM {:04X} without cartridge, ignored",
                    addr
                ),
                0x8000..=0xFFFF => eprintln!(
                    "Warning: Write to PRG ROM area {:04X} without cartridge, ignored",
                    addr
                ),
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
        0x5000..=0xFFFF
    }
}
