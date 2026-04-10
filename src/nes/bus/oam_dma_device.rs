use crate::nes::bus::bus::BusDevice;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct OamDmaDevice {
    oam_dma_page: Rc<RefCell<Option<u8>>>,
    dma_triggered: Rc<RefCell<bool>>,
}

impl OamDmaDevice {
    pub(crate) fn new(
        oam_dma_page: Rc<RefCell<Option<u8>>>,
        dma_triggered: Rc<RefCell<bool>>,
    ) -> Self {
        Self {
            oam_dma_page,
            dma_triggered,
        }
    }
}

impl BusDevice for OamDmaDevice {
    fn read(&mut self, _addr: u16, open_bus: u8, _is_dummy_read: bool) -> Option<u8> {
        Some(open_bus)
    }

    fn write(&mut self, _addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        *self.oam_dma_page.borrow_mut() = Some(value);
        *self.dma_triggered.borrow_mut() = true;
        true
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4014..=0x4014
    }
}
