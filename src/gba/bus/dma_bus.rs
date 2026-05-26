use super::dma;
use super::gba_bus::GbaBus;
use super::waitstates::WidthClass;
use crate::gba::cpu::bus::Bus;

impl dma::DmaBus for GbaBus {
    fn dma_read16(&mut self, addr: u32) -> u16 {
        if self.dma_latch_valid && (Self::is_bios_addr(addr) || Self::is_unused_internal_addr(addr))
        {
            return self.dma_latch_halfword(addr);
        }
        // Swap in the DMA latch so open-bus reads return the DMA's own
        // latched value rather than the CPU's last_bus_value.
        let saved = self.last_bus_value;
        self.last_bus_value = self.dma_latch;
        let val = <Self as Bus>::read16(self, addr);
        self.dma_latch = self.last_bus_value;
        self.dma_latch_valid = true;
        self.last_bus_value = saved;
        val
    }
    fn dma_write16(&mut self, addr: u32, value: u16) {
        let saved = self.last_bus_value;
        <Self as Bus>::write16(self, addr, value);
        self.last_bus_value = saved;
    }
    fn dma_read32(&mut self, addr: u32) -> u32 {
        if self.dma_latch_valid && (Self::is_bios_addr(addr) || Self::is_unused_internal_addr(addr))
        {
            return self.dma_latch;
        }
        let saved = self.last_bus_value;
        self.last_bus_value = self.dma_latch;
        let val = <Self as Bus>::read32(self, addr);
        self.dma_latch = self.last_bus_value;
        self.dma_latch_valid = true;
        self.last_bus_value = saved;
        val
    }
    fn dma_write32(&mut self, addr: u32, value: u32) {
        let saved = self.last_bus_value;
        <Self as Bus>::write32(self, addr, value);
        self.last_bus_value = saved;
    }
    fn dma_n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.n_cycles_width(addr, width)
    }
    fn dma_s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.s_cycles_width(addr, width)
    }
    fn dma_raise_irq(&mut self, sources: u16) {
        self.ic.raise(sources);
    }
}
