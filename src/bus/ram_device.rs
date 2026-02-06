use crate::bus::bus::BusDevice;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct RamDevice {
    cpu_ram: Rc<RefCell<Vec<u8>>>,
}

impl RamDevice {
    pub(crate) fn new(cpu_ram: Rc<RefCell<Vec<u8>>>) -> Self {
        Self { cpu_ram }
    }
}

impl BusDevice for RamDevice {
    fn read(&mut self, addr: u16, _open_bus: u8, _clock_joypads: bool) -> Option<u8> {
        Some(self.cpu_ram.borrow()[(addr & 0x07FF) as usize])
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        self.cpu_ram.borrow_mut()[(addr & 0x07FF) as usize] = value;
        true
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x0000..=0x1FFF
    }
}
