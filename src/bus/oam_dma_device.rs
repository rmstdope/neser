use crate::bus::bus::BusDevice;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct OamDmaDevice {
    oam_dma_page: Rc<RefCell<Option<u8>>>,
    dma_triggered: Rc<RefCell<bool>>,
    open_bus: Rc<RefCell<u8>>,
}

impl OamDmaDevice {
    pub(crate) fn new(
        oam_dma_page: Rc<RefCell<Option<u8>>>,
        dma_triggered: Rc<RefCell<bool>>,
        open_bus: Rc<RefCell<u8>>,
    ) -> Self {
        Self {
            oam_dma_page,
            dma_triggered,
            open_bus,
        }
    }
}

impl BusDevice for OamDmaDevice {
    fn read(&mut self, addr: u16, _clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        Some(*self.open_bus.borrow())
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        if !self.address_range().contains(&addr) {
            return false;
        }

        *self.oam_dma_page.borrow_mut() = Some(value);
        *self.dma_triggered.borrow_mut() = true;
        true
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4014..=0x4014
    }
}
