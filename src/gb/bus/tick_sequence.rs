//! The Game Boy M-cycle tick sequence, written once for the DMG and CGB buses.
//!
//! Each [`TickSequence::tick`] runs, in order:
//!
//! 1. PPU interrupts raised during the previous tick reach IF (the interrupt
//!    controller samples the PPU's lines one M-cycle late);
//! 2. per M-cycle: the timer (a TIMA overflow raises IF bit 2 at once), the
//!    APU's DIV-APU edges, the serial port clocked off the DIV counter (a
//!    finished transfer raises IF bit 3 at once), and one OAM DMA step;
//! 3. the PPU advances [`TickSequence::dots_per_m_cycle`] dots per M-cycle;
//! 4. the model's after-PPU work: the APU, the cartridge clock, and on the
//!    CGB its double-speed half rates and HDMA.
//!
//! A CPU write M-cycle that changes LCDC's tile-data bit in Mode 3 lands one
//! dot before the M-cycle ends; any other CPU access that the running OAM DMA
//! displaces (the model's rule) is dropped, or reads the DMA's byte.

use crate::gb::apu::Apu;
use crate::gb::bus::memory_map::MemoryMap;
use crate::gb::bus::oam_dma::OamDma;
use crate::gb::bus::serial::Serial;
use crate::gb::ppu::Ppu;
use crate::gb::ppu::timing::PpuMode;
use crate::gb::timer::Timer;

/// The hardware the tick sequence drives, borrowed from a bus.
pub(super) struct TickParts<'a> {
    pub ppu: &'a mut Ppu,
    pub timer: &'a mut Timer,
    pub apu: &'a mut Apu,
    pub if_reg: &'a mut u8,
    pub serial: &'a mut Serial,
    pub oam_dma: &'a mut OamDma,
}

/// The shared tick sequence, with a model's own timing layered on.
pub(super) trait TickSequence: MemoryMap {
    /// Borrow the hardware the sequence drives.
    fn tick_parts(&mut self) -> TickParts<'_>;

    /// The running OAM DMA transfer.
    fn oam_dma(&self) -> &OamDma;

    /// The byte OAM DMA reads at `addr` (the DMA controller's view of the bus).
    fn dma_read(&self, addr: u16) -> u8;

    /// Whether a CPU access to `addr` is displaced by the running OAM DMA.
    fn dma_conflicts_with(&self, addr: u16) -> bool;

    /// The model's work once the PPU has advanced: APU, cartridge clock, HDMA.
    fn tick_after_ppu(&mut self, m_cycles: u8);

    /// PPU dots per CPU M-cycle.
    fn dots_per_m_cycle(&self) -> u32 {
        4
    }

    /// Whether the serial port follows CGB-mode rules (the fast clock bit).
    fn serial_cgb_mode(&self) -> bool {
        false
    }

    /// Steps 1 and 2 of the sequence.
    fn tick_before_ppu(&mut self, m_cycles: u8) {
        let parts = self.tick_parts();
        *parts.if_reg |= parts.ppu.take_pending_interrupts();

        for _ in 0..m_cycles {
            let serial_cgb_mode = self.serial_cgb_mode();
            let parts = self.tick_parts();
            let counter_before = parts.timer.raw_counter();
            let (div_apu_falling, div_apu_rising) = parts.timer.tick(1);
            if parts.timer.interrupt_pending {
                *parts.if_reg |= 0x04;
                parts.timer.interrupt_pending = false;
            }
            // A rising edge fires the APU secondary event (envelope phantom ticks).
            for _ in 0..div_apu_rising {
                parts.apu.clock_div_apu_secondary();
            }
            // A falling edge steps the APU frame sequencer.
            for _ in 0..div_apu_falling {
                parts.apu.clock_div_apu();
            }
            if parts
                .serial
                .clock(counter_before, parts.timer.raw_counter(), serial_cgb_mode)
            {
                *parts.if_reg |= 0x08;
            }

            if let Some((index, src)) = self.tick_parts().oam_dma.step() {
                let byte = self.dma_read(src);
                self.tick_parts().ppu.oam[index] = byte;
            }
        }
    }

    /// Advance the bus by `m_cycles` CPU M-cycles.
    fn tick(&mut self, m_cycles: u8) {
        let dots = self.dots_per_m_cycle();
        self.tick_before_ppu(m_cycles);
        self.tick_parts().ppu.tick_dots(u32::from(m_cycles) * dots);
        self.tick_after_ppu(m_cycles);
    }

    /// Whether a CPU write is an LCDC tile-data change in Mode 3, which the
    /// PPU sees one dot before the write M-cycle ends.
    fn needs_mode3_lcdc_write_phase(&self, addr: u16, val: u8) -> bool {
        const LCDC_TILE_DATA: u8 = 0x10;

        let ppu = self.view().ppu;
        addr == 0xFF40
            && ppu.is_lcd_enabled()
            && ppu.mode() == PpuMode::PixelTransfer
            && ppu.read_register(0xFF40) & LCDC_TILE_DATA != val & LCDC_TILE_DATA
    }

    /// One CPU write M-cycle.
    fn cpu_write_m_cycle(&mut self, addr: u16, val: u8) {
        if self.needs_mode3_lcdc_write_phase(addr, val) {
            let dots = self.dots_per_m_cycle();
            self.tick_before_ppu(1);
            self.tick_parts().ppu.tick_dots(dots - 1);
            self.map_write(addr, val);
            self.tick_parts().ppu.tick_dots(1);
            self.tick_after_ppu(1);
        } else {
            self.tick(1);
            if !self.dma_conflicts_with(addr) {
                self.map_write(addr, val);
            }
        }
    }

    /// One CPU read (the M-cycle's tick is the caller's).
    fn cpu_read_m_cycle(&mut self, addr: u16) -> u8 {
        if self.dma_conflicts_with(addr) {
            self.dma_read(self.oam_dma().conflict_address())
        } else {
            self.map_read(addr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::memory_map::tests::TestMap;
    use crate::gb::bus::memory_map::{MapView, MapViewMut};
    use crate::gb::ppu::timing::PpuMode;

    /// A bus on the shared test map, recording what the model hooks see.
    struct TickBus {
        map: TestMap,
        oam_dma: OamDma,
        dots: u32,
        after_ppu: Vec<u8>,
    }

    impl TickBus {
        fn new(dots: u32) -> Self {
            Self {
                map: TestMap::new(),
                oam_dma: OamDma::default(),
                dots,
                after_ppu: Vec::new(),
            }
        }

        fn dma_byte(addr: u16) -> u8 {
            (addr as u8) ^ 0xA5
        }
    }

    impl MemoryMap for TickBus {
        fn view(&self) -> MapView<'_> {
            let mut view = self.map.view();
            view.dma_source = self.oam_dma.source();
            view.dma_oam_blocked = self.oam_dma.blocks_oam();
            view
        }

        fn view_mut(&mut self) -> MapViewMut<'_> {
            let mut view = self.map.view_mut();
            view.dma_oam_blocked = self.oam_dma.blocks_oam();
            view
        }

        fn model_peek(&self, addr: u16) -> Option<u8> {
            self.map.model_peek(addr)
        }

        fn model_write(&mut self, addr: u16, val: u8) -> bool {
            self.map.model_write(addr, val)
        }

        fn start_oam_dma(&mut self, val: u8) {
            self.oam_dma.start(val);
        }
    }

    impl TickSequence for TickBus {
        fn tick_parts(&mut self) -> TickParts<'_> {
            TickParts {
                ppu: &mut self.map.ppu,
                timer: &mut self.map.timer,
                apu: &mut self.map.apu,
                if_reg: &mut self.map.if_reg,
                serial: &mut self.map.serial,
                oam_dma: &mut self.oam_dma,
            }
        }

        fn oam_dma(&self) -> &OamDma {
            &self.oam_dma
        }

        fn dma_read(&self, addr: u16) -> u8 {
            Self::dma_byte(addr)
        }

        fn dma_conflicts_with(&self, addr: u16) -> bool {
            self.oam_dma.holds_bus() && matches!(addr, 0xC000..=0xDFFF)
        }

        fn tick_after_ppu(&mut self, m_cycles: u8) {
            self.after_ppu.push(m_cycles);
        }

        fn dots_per_m_cycle(&self) -> u32 {
            self.dots
        }

        fn serial_cgb_mode(&self) -> bool {
            self.map.cgb_mode
        }
    }

    #[test]
    fn a_timer_overflow_raises_if_bit_2_inside_the_tick() {
        let mut bus = TickBus::new(4);
        bus.map_write(0xFF07, 0x05); // timer on, one increment per 4 M-cycles
        bus.map_write(0xFF05, 0xFF);
        let mut m_cycles = 0;
        while bus.map.if_reg & 0x04 == 0 {
            bus.tick(1);
            m_cycles += 1;
            assert!(m_cycles <= 8, "TIMA overflow never reached IF");
        }
        assert_eq!(bus.map.if_reg & 0x04, 0x04);
    }

    #[test]
    fn ppu_interrupts_raised_before_a_tick_reach_if_at_its_start() {
        let mut bus = TickBus::new(4);
        bus.map_write(0xFF40, 0x91);
        bus.map.if_reg = 0;
        // Run the PPU alone into VBlank: its interrupt waits in the PPU.
        bus.map.ppu.tick_dots(145 * 456);
        assert_eq!(bus.map.if_reg & 0x01, 0);
        bus.tick(1);
        assert_eq!(bus.map.if_reg & 0x01, 0x01);
    }

    #[test]
    fn the_ppu_advances_dots_per_m_cycle_times_m_cycles() {
        for dots in [4, 2] {
            let mut bus = TickBus::new(dots);
            bus.map_write(0xFF40, 0x91);
            let start = bus.map.ppu.dot();
            bus.tick(3);
            assert_eq!(u32::from(bus.map.ppu.dot() - start), 3 * dots);
            assert_eq!(
                bus.after_ppu,
                vec![3],
                "the model's after-PPU work runs once"
            );
        }
    }

    /// Tick one M-cycle at a time until IF bit 3 rises; the M-cycle count.
    fn ticks_until_serial_interrupt(bus: &mut TickBus, limit: u32) -> Option<u32> {
        (1..=limit).find(|_| {
            bus.tick(1);
            bus.map.if_reg & 0x08 != 0
        })
    }

    #[test]
    fn a_finished_serial_transfer_raises_if_bit_3_inside_the_tick() {
        let mut bus = TickBus::new(4);
        bus.map_write(0xFF01, 0x41);
        bus.map_write(0xFF02, 0x81); // internal clock, 8192 Hz
        let done = ticks_until_serial_interrupt(&mut bus, 1024).expect("transfer completes");
        assert!(done > 900, "normal clock: done at {done}");
        assert_eq!(bus.map.serial.output(), &[0x41]);
    }

    #[test]
    fn the_models_serial_mode_decides_the_fast_clock() {
        let mut cgb = TickBus::new(4);
        cgb.map.cgb_mode = true;
        cgb.map_write(0xFF02, 0x83); // internal clock, fast bit
        assert!(ticks_until_serial_interrupt(&mut cgb, 34).is_some());

        let mut dmg = TickBus::new(4);
        dmg.map_write(0xFF02, 0x83);
        assert_eq!(ticks_until_serial_interrupt(&mut dmg, 64), None);
    }

    #[test]
    fn oam_dma_copies_through_the_models_dma_read() {
        let mut bus = TickBus::new(4);
        bus.map_write(0xFF46, 0xC1);
        bus.tick(162);
        assert!(!bus.oam_dma.blocks_oam());
        for index in 0..0xA0u16 {
            assert_eq!(
                bus.map.ppu.oam[usize::from(index)],
                TickBus::dma_byte(0xC100 + index)
            );
        }
    }

    #[test]
    fn a_conflicting_cpu_write_is_dropped_and_a_conflicting_read_sees_the_dma_byte() {
        let mut bus = TickBus::new(4);
        bus.map_write(0xFF46, 0xC1);
        bus.tick(3);
        assert!(bus.oam_dma.holds_bus());

        bus.cpu_write_m_cycle(0xC200, 0x77);
        assert_eq!(
            bus.map.wram[0][0x200], 0,
            "the conflicting write is dropped"
        );
        bus.cpu_write_m_cycle(0xFF80, 0x66);
        assert_eq!(bus.map.hram[0], 0x66, "a write off the DMA's bus lands");

        let expected = TickBus::dma_byte(bus.oam_dma.conflict_address());
        assert_eq!(bus.cpu_read_m_cycle(0xC200), expected);
    }

    #[test]
    fn a_mode3_tile_data_lcdc_write_lands_one_dot_before_the_m_cycle_ends() {
        for dots in [4, 2] {
            let mut bus = TickBus::new(dots);
            bus.map_write(0xFF40, 0x91);
            while bus.map.ppu.dot() < 84 {
                bus.tick(1);
            }
            assert_eq!(bus.map.ppu.mode(), PpuMode::PixelTransfer);
            let start = bus.map.ppu.dot();
            // Turning the LCD off stops the PPU where the write lands.
            bus.cpu_write_m_cycle(0xFF40, 0x01);
            assert_eq!(u32::from(bus.map.ppu.dot() - start), dots - 1);
            assert_eq!(bus.after_ppu.last(), Some(&1));
        }
    }
}
