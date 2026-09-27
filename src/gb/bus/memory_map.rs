//! The Game Boy memory map, written once for the DMG and CGB buses.
//!
//! [`MemoryMap`] routes every address both models decode the same way:
//!
//! - `$0000–$7FFF`, `$A000–$BFFF`: cartridge
//! - `$8000–$9FFF`: VRAM (through the PPU, so Mode 3 blocking applies)
//! - `$C000–$CFFF`: WRAM bank 0; `$D000–$DFFF`: the mapped WRAM bank
//! - `$E000–$FDFF`: echo of `$C000–$DDFF`
//! - `$FE00–$FE9F`: OAM (blocked while an OAM DMA holds it)
//! - `$FF00` P1, `$FF01` SB, `$FF02` SC read, `$FF04–$FF07` timer, `$FF0F` IF,
//!   `$FF10–$FF3F` APU, `$FF40–$FF4B` PPU registers and `$FF46` OAM DMA,
//!   `$FF80–$FFFE` HRAM, `$FFFF` IE
//! - anything left: reads `$FF`, writes are ignored
//!
//! A bus answers its own model's addresses first through [`MemoryMap::model_peek`],
//! [`MemoryMap::model_read`] and [`MemoryMap::model_write`]: the boot ROM overlay,
//! the `$FEA0–$FEFF` zone, the CGB registers and the other places the models differ.
//! Whatever a model does not claim falls through to the shared map.

use crate::gb::apu::Apu;
use crate::gb::cartridge::GbCartridge;
use crate::gb::input::joypad::Joypad;
use crate::gb::ppu::Ppu;
use crate::gb::timer::Timer;

/// Read-only view of the hardware both Game Boy buses route addresses to.
pub(super) struct MapView<'a> {
    pub cart: &'a dyn GbCartridge,
    pub ppu: &'a Ppu,
    /// `[bank at $C000, bank mapped at $D000]`.
    pub wram: [&'a [u8; 0x1000]; 2],
    pub hram: &'a [u8; 0x7F],
    pub timer: &'a Timer,
    pub apu: &'a Apu,
    pub joypad: &'a Joypad,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub sb: u8,
    pub sc: u8,
    pub dma_source: u8,
    pub dma_oam_blocked: bool,
}

/// Mutable view of the same hardware, for writes and side-effecting reads.
pub(super) struct MapViewMut<'a> {
    pub cart: &'a mut dyn GbCartridge,
    pub ppu: &'a mut Ppu,
    /// `[bank at $C000, bank mapped at $D000]`.
    pub wram: [&'a mut [u8; 0x1000]; 2],
    pub hram: &'a mut [u8; 0x7F],
    pub timer: &'a mut Timer,
    pub apu: &'a mut Apu,
    pub joypad: &'a mut Joypad,
    pub if_reg: &'a mut u8,
    pub ie_reg: &'a mut u8,
    pub sb: &'a mut u8,
    pub dma_oam_blocked: bool,
}

/// The shared Game Boy memory map, with a model's own addresses layered on.
pub(super) trait MemoryMap {
    /// Borrow the shared hardware for a side-effect-free access.
    fn view(&self) -> MapView<'_>;

    /// Borrow the shared hardware for a write or a side-effecting read.
    fn view_mut(&mut self) -> MapViewMut<'_>;

    /// Answer an address this model decodes itself, without side effects.
    /// `None` hands the address to the shared map.
    fn model_peek(&self, addr: u16) -> Option<u8>;

    /// Answer an address this model decodes itself, for a CPU read.
    fn model_read(&mut self, addr: u16) -> Option<u8> {
        self.model_peek(addr)
    }

    /// Handle a write to an address this model decodes itself.
    /// Returns `false` to hand the write to the shared map.
    fn model_write(&mut self, addr: u16, val: u8) -> bool;

    /// Start an OAM DMA transfer from `val << 8` (a `$FF46` write).
    fn start_oam_dma(&mut self, val: u8);

    /// Read `addr` without side effects (debugger reads).
    fn map_peek(&self, addr: u16) -> u8 {
        self.model_peek(addr)
            .unwrap_or_else(|| self.shared_peek(addr))
    }

    /// Read `addr` as the CPU does.
    fn map_read(&mut self, addr: u16) -> u8 {
        if let Some(value) = self.model_read(addr) {
            return value;
        }
        if let 0xFE00..=0xFE9F = addr {
            let v = self.view_mut();
            return if v.dma_oam_blocked {
                0xFF
            } else {
                v.ppu.read_oam(addr)
            };
        }
        self.shared_peek(addr)
    }

    /// The shared map's answer for `addr`, once the model has not claimed it.
    fn shared_peek(&self, addr: u16) -> u8 {
        let v = self.view();
        match addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => v.cart.read(addr),
            0x8000..=0x9FFF => v.ppu.read_vram(addr),
            0xC000..=0xCFFF => v.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => v.wram[1][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => v.wram[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => v.wram[1][(addr - 0xF000) as usize],
            // Direct OAM read: `Ppu::read_oam` would apply OAM corruption.
            0xFE00..=0xFE9F if v.dma_oam_blocked => 0xFF,
            0xFE00..=0xFE9F => v.ppu.oam[(addr - 0xFE00) as usize],
            0xFF00 => v.joypad.read(),
            0xFF01 => v.sb,
            0xFF02 => v.sc | 0x7E, // SC bits 6-1 read as 1
            0xFF04..=0xFF07 => v.timer.read(addr),
            0xFF0F => v.if_reg | 0xE0,
            0xFF10..=0xFF3F => v.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => v.ppu.read_register(addr),
            0xFF46 => v.dma_source,
            0xFF80..=0xFFFE => v.hram[(addr - 0xFF80) as usize],
            0xFFFF => v.ie_reg,
            _ => 0xFF,
        }
    }

    /// Write `val` to `addr` as the CPU does.
    fn map_write(&mut self, addr: u16, val: u8) {
        if self.model_write(addr, val) {
            return;
        }
        if addr == 0xFF46 {
            self.start_oam_dma(val);
            return;
        }
        let v = self.view_mut();
        match addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => v.cart.write(addr, val),
            0x8000..=0x9FFF => v.ppu.write_vram(addr, val),
            0xC000..=0xCFFF => v.wram[0][(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => v.wram[1][(addr - 0xD000) as usize] = val,
            0xE000..=0xEFFF => v.wram[0][(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => v.wram[1][(addr - 0xF000) as usize] = val,
            0xFE00..=0xFE9F if !v.dma_oam_blocked => v.ppu.write_oam(addr, val),
            0xFF00 => v.joypad.write(val),
            0xFF01 => *v.sb = val,
            0xFF04..=0xFF07 => {
                // A DIV write with the DIV-APU bit high clocks the frame sequencer.
                if v.timer.write(addr, val) {
                    v.apu.clock_div_apu();
                }
                // Raise a write-triggered TIMA overflow now, so service_interrupts()
                // sees it at the start of the next instruction.
                if v.timer.fire_write_overflow_if_pending() {
                    *v.if_reg |= 0x04;
                    v.timer.take_interrupt();
                }
            }
            0xFF0F => *v.if_reg = val & 0x1F,
            // NR52 takes the DIV-APU bit state for the power-on skip.
            0xFF26 => {
                let div_apu_high = v.timer.is_div_apu_bit_high();
                v.apu.write_nr52_with_div_state(val, div_apu_high);
            }
            0xFF10..=0xFF3F => v.apu.write_register(addr, val),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => {
                v.ppu.write_register(addr, val);
                // Flush an interrupt the write raised (e.g. LYC=LY on LCD re-enable)
                // so it is visible at the start of the next instruction.
                *v.if_reg |= v.ppu.take_pending_interrupts();
            }
            0xFF80..=0xFFFE => v.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => *v.ie_reg = val,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat 32 KB ROM + 8 KB RAM cartridge, so the map can be read back.
    struct FlatCart {
        rom: Vec<u8>,
        ram: [u8; 0x2000],
    }

    impl GbCartridge for FlatCart {
        fn read(&self, addr: u16) -> u8 {
            match addr {
                0x0000..=0x7FFF => self.rom[addr as usize],
                0xA000..=0xBFFF => self.ram[(addr - 0xA000) as usize],
                _ => 0xFF,
            }
        }

        fn write(&mut self, addr: u16, val: u8) {
            if let 0xA000..=0xBFFF = addr {
                self.ram[(addr - 0xA000) as usize] = val;
            }
        }
    }

    /// The smallest bus on the shared map: two WRAM banks, one claimed register.
    struct TestMap {
        cart: FlatCart,
        ppu: Ppu,
        wram: [[u8; 0x1000]; 2],
        hram: [u8; 0x7F],
        timer: Timer,
        apu: Apu,
        joypad: Joypad,
        if_reg: u8,
        ie_reg: u8,
        sb: u8,
        sc: u8,
        dma_source: u8,
        dma_oam_blocked: bool,
        /// The model's own register at `CLAIMED`.
        claimed: u8,
        dma_started_from: Option<u8>,
    }

    const CLAIMED: u16 = 0xFF4D;

    impl TestMap {
        fn new() -> Self {
            let mut rom = vec![0u8; 0x8000];
            rom[0x0150] = 0x5A;
            let mut ppu = Ppu::new();
            ppu.write_register(0xFF40, 0x00); // LCD off: VRAM and OAM open
            Self {
                cart: FlatCart {
                    rom,
                    ram: [0; 0x2000],
                },
                ppu,
                wram: [[0; 0x1000]; 2],
                hram: [0; 0x7F],
                timer: Timer::new(),
                apu: Apu::new(false),
                joypad: Joypad::new(),
                if_reg: 0,
                ie_reg: 0,
                sb: 0,
                sc: 0,
                dma_source: 0xFF,
                dma_oam_blocked: false,
                claimed: 0x42,
                dma_started_from: None,
            }
        }
    }

    impl MemoryMap for TestMap {
        fn view(&self) -> MapView<'_> {
            MapView {
                cart: &self.cart,
                ppu: &self.ppu,
                wram: [&self.wram[0], &self.wram[1]],
                hram: &self.hram,
                timer: &self.timer,
                apu: &self.apu,
                joypad: &self.joypad,
                if_reg: self.if_reg,
                ie_reg: self.ie_reg,
                sb: self.sb,
                sc: self.sc,
                dma_source: self.dma_source,
                dma_oam_blocked: self.dma_oam_blocked,
            }
        }

        fn view_mut(&mut self) -> MapViewMut<'_> {
            let [low, high] = &mut self.wram;
            MapViewMut {
                cart: &mut self.cart,
                ppu: &mut self.ppu,
                wram: [low, high],
                hram: &mut self.hram,
                timer: &mut self.timer,
                apu: &mut self.apu,
                joypad: &mut self.joypad,
                if_reg: &mut self.if_reg,
                ie_reg: &mut self.ie_reg,
                sb: &mut self.sb,
                dma_oam_blocked: self.dma_oam_blocked,
            }
        }

        fn model_peek(&self, addr: u16) -> Option<u8> {
            (addr == CLAIMED).then_some(self.claimed)
        }

        fn model_write(&mut self, addr: u16, val: u8) -> bool {
            if addr == CLAIMED {
                self.claimed = val;
            }
            addr == CLAIMED
        }

        fn start_oam_dma(&mut self, val: u8) {
            self.dma_started_from = Some(val);
            self.dma_source = val;
        }
    }

    #[test]
    fn cartridge_rom_and_ram_route_to_the_cartridge() {
        let mut map = TestMap::new();
        assert_eq!(map.map_read(0x0150), 0x5A);
        map.map_write(0xA123, 0x77);
        assert_eq!(map.map_read(0xA123), 0x77);
        assert_eq!(map.cart.ram[0x123], 0x77);
    }

    #[test]
    fn vram_routes_through_the_ppu() {
        let mut map = TestMap::new();
        map.map_write(0x8010, 0x3C);
        assert_eq!(map.ppu.vram[0x10], 0x3C);
        assert_eq!(map.map_read(0x8010), 0x3C);
    }

    #[test]
    fn wram_c000_is_bank_zero_and_d000_is_the_mapped_bank() {
        let mut map = TestMap::new();
        map.map_write(0xC001, 0x11);
        map.map_write(0xD001, 0x22);
        assert_eq!(map.wram[0][1], 0x11);
        assert_eq!(map.wram[1][1], 0x22);
    }

    #[test]
    fn echo_ram_mirrors_both_wram_halves_for_read_and_write() {
        let mut map = TestMap::new();
        map.map_write(0xC005, 0x11);
        map.map_write(0xDDFF, 0x22);
        assert_eq!(map.map_read(0xE005), 0x11);
        assert_eq!(map.map_read(0xFDFF), 0x22);
        map.map_write(0xE006, 0x33);
        map.map_write(0xF006, 0x44);
        assert_eq!(map.map_read(0xC006), 0x33);
        assert_eq!(map.map_read(0xD006), 0x44);
    }

    #[test]
    fn oam_reads_and_writes_while_no_dma_holds_it() {
        let mut map = TestMap::new();
        map.map_write(0xFE10, 0x9A);
        assert_eq!(map.ppu.oam[0x10], 0x9A);
        assert_eq!(map.map_read(0xFE10), 0x9A);
        assert_eq!(map.map_peek(0xFE10), 0x9A);
    }

    #[test]
    fn oam_reads_ff_and_ignores_writes_while_a_dma_holds_it() {
        let mut map = TestMap::new();
        map.ppu.oam[0x10] = 0x9A;
        map.dma_oam_blocked = true;
        assert_eq!(map.map_read(0xFE10), 0xFF);
        assert_eq!(map.map_peek(0xFE10), 0xFF);
        map.map_write(0xFE10, 0x01);
        assert_eq!(map.ppu.oam[0x10], 0x9A);
    }

    #[test]
    fn hram_and_ie_read_back_what_was_written() {
        let mut map = TestMap::new();
        map.map_write(0xFF80, 0x12);
        map.map_write(0xFFFE, 0x34);
        map.map_write(0xFFFF, 0x1F);
        assert_eq!(map.map_read(0xFF80), 0x12);
        assert_eq!(map.map_read(0xFFFE), 0x34);
        assert_eq!(map.map_read(0xFFFF), 0x1F);
    }

    #[test]
    fn if_keeps_five_bits_and_reads_the_upper_three_as_one() {
        let mut map = TestMap::new();
        map.map_write(0xFF0F, 0xFF);
        assert_eq!(map.if_reg, 0x1F);
        assert_eq!(map.map_read(0xFF0F), 0xFF);
        map.map_write(0xFF0F, 0x04);
        assert_eq!(map.map_read(0xFF0F), 0xE4);
    }

    #[test]
    fn sb_reads_back_and_sc_reads_its_unused_bits_as_one() {
        let mut map = TestMap::new();
        map.map_write(0xFF01, 0xA5);
        assert_eq!(map.map_read(0xFF01), 0xA5);
        map.sc = 0x81;
        assert_eq!(map.map_read(0xFF02), 0xFF);
        map.sc = 0x00;
        assert_eq!(map.map_read(0xFF02), 0x7E);
    }

    #[test]
    fn div_write_resets_the_divider() {
        let mut map = TestMap::new();
        for _ in 0..1000 {
            map.timer.tick(1);
        }
        assert_ne!(map.map_read(0xFF04), 0);
        map.map_write(0xFF04, 0x55);
        assert_eq!(map.map_read(0xFF04), 0);
    }

    #[test]
    fn apu_registers_route_to_the_apu() {
        let mut map = TestMap::new();
        map.map_write(0xFF26, 0x80);
        map.map_write(0xFF24, 0x77);
        assert_eq!(map.map_read(0xFF24), 0x77);
        map.map_write(0xFF30, 0xAB);
        assert_eq!(map.map_read(0xFF30), 0xAB);
    }

    #[test]
    fn ppu_registers_route_to_the_ppu() {
        let mut map = TestMap::new();
        map.map_write(0xFF42, 0x33);
        assert_eq!(map.map_read(0xFF42), 0x33);
        assert_eq!(map.map_peek(0xFF42), 0x33);
    }

    #[test]
    fn ff46_write_starts_oam_dma_and_reads_back_the_source() {
        let mut map = TestMap::new();
        map.map_write(0xFF46, 0xC1);
        assert_eq!(map.dma_started_from, Some(0xC1));
        assert_eq!(map.map_read(0xFF46), 0xC1);
    }

    #[test]
    fn p1_routes_to_the_joypad() {
        let mut map = TestMap::new();
        map.map_write(0xFF00, 0x20);
        assert_eq!(map.map_read(0xFF00), map.joypad.read());
        assert_eq!(map.map_read(0xFF00) & 0x30, 0x20);
    }

    #[test]
    fn unclaimed_io_reads_ff_and_ignores_writes() {
        let mut map = TestMap::new();
        for addr in [
            0xFF03, 0xFF08, 0xFF0E, 0xFF4C, 0xFF50, 0xFF70, 0xFF7F, 0xFEA0, 0xFEFF,
        ] {
            map.map_write(addr, 0x00);
            assert_eq!(map.map_read(addr), 0xFF, "${addr:04X}");
            assert_eq!(map.map_peek(addr), 0xFF, "${addr:04X}");
        }
    }

    #[test]
    fn a_model_claim_wins_over_the_shared_map() {
        let mut map = TestMap::new();
        assert_eq!(map.map_read(CLAIMED), 0x42);
        assert_eq!(map.map_peek(CLAIMED), 0x42);
        map.map_write(CLAIMED, 0x24);
        assert_eq!(map.claimed, 0x24);
    }

    #[test]
    fn peek_reads_oam_raw_while_the_cpu_is_locked_out_in_mode_2() {
        let mut map = TestMap::new();
        map.ppu.oam = std::array::from_fn(|i| i as u8);
        map.ppu.write_register(0xFF40, 0x91); // LCD on
        map.ppu.tick_dots(456 + 4); // line 1, OAM scan
        let oam_before = map.ppu.oam;
        assert_eq!(map.map_peek(0xFE10), 0x10);
        assert_eq!(map.ppu.oam, oam_before, "a peek must not corrupt OAM");
        assert_eq!(map.map_read(0xFE10), 0xFF);
    }

    #[test]
    fn peek_matches_read_across_the_map_with_the_lcd_off() {
        let mut map = TestMap::new();
        map.map_write(0xC000, 0x01);
        map.map_write(0xFF80, 0x02);
        for addr in [
            0x0150, 0x8000, 0xC000, 0xE000, 0xFE00, 0xFF0F, 0xFF40, 0xFF80,
        ] {
            let peeked = map.map_peek(addr);
            assert_eq!(map.map_read(addr), peeked, "${addr:04X}");
        }
    }
}
