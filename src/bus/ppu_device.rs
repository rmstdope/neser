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
    fn read(&mut self, addr: u16, _open_bus: u8, _is_dummy_read: bool) -> Option<u8> {
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
        let reg = addr & 0x2007;
        match reg {
            0x2000 => {
                self.ppu.borrow_mut().write_control(value);
                if addr == 0x2000
                    && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
                {
                    cartridge.borrow_mut().mapper_mut().ppu_write_ctrl(value);
                }
                true
            }
            0x2001 => {
                self.ppu.borrow_mut().write_mask(value);
                if addr == 0x2001
                    && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
                {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Cartridge, Mapper, MirroringMode};
    use crate::console::TimingMode;
    use std::cell::Cell;

    struct MaskTrackingMapper {
        mask_writes: Rc<Cell<usize>>,
    }

    impl Mapper for MaskTrackingMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn get_mirroring(&self) -> MirroringMode {
            MirroringMode::Horizontal
        }

        fn mapper_number(&self) -> u8 {
            0
        }

        fn wram_size(&self) -> usize {
            0
        }

        fn wram_snapshot(&self) -> Vec<u8> {
            Vec::new()
        }

        fn load_wram_snapshot(&mut self, _data: &[u8]) {}

        fn chr_ram_snapshot(&self) -> Vec<u8> {
            Vec::new()
        }

        fn restore_chr_ram(&mut self, _data: &[u8]) {}

        fn registers_snapshot(&self) -> Vec<u8> {
            Vec::new()
        }

        fn restore_registers(&mut self, _data: &[u8]) {}

        fn reset(&mut self) {}

        fn ppu_write_mask(&mut self, _value: u8) {
            self.mask_writes.set(self.mask_writes.get() + 1);
        }
    }

    #[test]
    fn test_ppu_mask_mirror_write_does_not_notify_mapper() {
        let mask_writes = Rc::new(Cell::new(0));
        let mapper = Box::new(MaskTrackingMapper {
            mask_writes: Rc::clone(&mask_writes),
        });
        let cartridge = Cartridge::from_mapper_for_test(mapper);
        let cartridge = Rc::new(RefCell::new(cartridge));
        let cartridge_slot = Rc::new(RefCell::new(Some(cartridge)));

        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let mut device = PpuDevice::new(Rc::clone(&ppu), Rc::clone(&cartridge_slot));

        // Write to a PPUMASK mirror ($2009) should not notify the mapper.
        device.write(0x2009, 0x18, false);
        assert_eq!(mask_writes.get(), 0);

        // Write to the fully decoded $2001 should notify the mapper.
        device.write(0x2001, 0x18, false);
        assert_eq!(mask_writes.get(), 1);
    }
}
