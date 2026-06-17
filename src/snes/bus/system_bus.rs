use crate::snes::bus::SnesBus;
use crate::snes::bus::dma::{DmaABus, DmaController};
use crate::snes::cartridge::Cartridge;
use crate::snes::cartridge::Mapping;
use std::cell::Cell;

const WRAM_SIZE: usize = 128 * 1024;

/// SNES system bus.
///
/// This bus currently implements:
/// - WRAM direct and low-RAM mirror windows
/// - cartridge ROM mapping (LoROM/HiROM/ExHiROM)
/// - battery SRAM windows
/// - open-bus/MDR read semantics
/// - CPU/MMIO registers needed for early bring-up (`$2180-$2183`, `$4202-$4206`,
///   `$420D`, and `$4300-$437F` register latches)
pub struct SnesSystemBus {
    _cartridge: Cartridge,
    mapping: Mapping,
    rom: Vec<u8>,
    sram: Vec<u8>,
    wram: Vec<u8>,
    wmadd: Cell<u32>,
    wrmpya: u8,
    wrdiv: u16,
    rddiv: u16,
    rdmpy: u16,
    memsel: u8,
    dma: DmaController,
    mdr: Cell<u8>,
    ticks: Cell<u64>,
}

impl SnesSystemBus {
    pub fn new(cartridge: Cartridge) -> Self {
        let mapping = cartridge.mapping();
        let rom = cartridge.rom().to_vec();
        let sram = vec![0; cartridge.sram_size()];
        Self {
            _cartridge: cartridge,
            mapping,
            rom,
            sram,
            wram: vec![0; WRAM_SIZE],
            wmadd: Cell::new(0),
            wrmpya: 0,
            wrdiv: 0,
            rddiv: 0,
            rdmpy: 0,
            memsel: 0,
            dma: DmaController::new(),
            mdr: Cell::new(0),
            ticks: Cell::new(0),
        }
    }

    fn decode_wram_index(addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        if (0x7E..=0x7F).contains(&bank) {
            return Some((((bank as usize - 0x7E) << 16) | offset as usize) & (WRAM_SIZE - 1));
        }

        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset <= 0x1FFF {
            return Some(offset as usize);
        }

        None
    }

    fn decode_rom_index(&self, addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        match self.mapping {
            Mapping::LoRom => {
                if (bank <= 0x7D || bank >= 0x80) && offset >= 0x8000 {
                    let bank_index = (bank & 0x7F) as usize;
                    Some(bank_index * 0x8000 + (offset as usize - 0x8000))
                } else {
                    None
                }
            }
            Mapping::HiRom => {
                if (0xC0..=0xFF).contains(&bank) {
                    Some((bank as usize - 0xC0) * 0x10000 + offset as usize)
                } else if (0x40..=0x7D).contains(&bank) {
                    Some((bank as usize - 0x40) * 0x10000 + offset as usize)
                } else if (matches!(bank, 0x00..=0x3F | 0x80..=0xBF)) && offset >= 0x8000 {
                    Some((bank as usize & 0x3F) * 0x10000 + offset as usize)
                } else {
                    None
                }
            }
            Mapping::ExHiRom => {
                if (0xC0..=0xFF).contains(&bank) {
                    Some((bank as usize - 0xC0) * 0x10000 + offset as usize)
                } else if (0x40..=0x7D).contains(&bank) {
                    Some(0x400000 + (bank as usize - 0x40) * 0x10000 + offset as usize)
                } else if (matches!(bank, 0x00..=0x3F | 0x80..=0xBF)) && offset >= 0x8000 {
                    Some((bank as usize & 0x3F) * 0x10000 + offset as usize)
                } else {
                    None
                }
            }
        }
    }

    fn decode_sram_index(&self, addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        match self.mapping {
            Mapping::LoRom => {
                if (0x70..=0x7D).contains(&bank) && offset <= 0x7FFF {
                    Some((bank as usize - 0x70) * 0x8000 + offset as usize)
                } else {
                    None
                }
            }
            Mapping::HiRom => {
                if (matches!(bank, 0x20..=0x3F | 0xA0..=0xBF))
                    && (0x6000..=0x7FFF).contains(&offset)
                {
                    Some((bank as usize & 0x1F) * 0x2000 + (offset as usize - 0x6000))
                } else {
                    None
                }
            }
            Mapping::ExHiRom => {
                if (matches!(bank, 0x20..=0x3F | 0xA0..=0xBF))
                    && (0x6000..=0x7FFF).contains(&offset)
                {
                    Some((bank as usize & 0x1F) * 0x2000 + (offset as usize - 0x6000))
                } else {
                    None
                }
            }
        }
    }

    fn decode_system_offset(addr: u32) -> Option<u16> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) {
            Some(offset)
        } else {
            None
        }
    }

    fn is_system_bank(bank: u8) -> bool {
        matches!(bank, 0x00..=0x3F | 0x80..=0xBF)
    }

    fn is_dma_a_bus_mmio(addr: u32) -> bool {
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        Self::is_system_bank(bank)
            && (matches!(offset, 0x2100..=0x21FF | 0x4000..=0x41FF | 0x4200..=0x421F)
                || (0x4300..=0x437F).contains(&offset))
    }

    fn dma_read_a_bus_impl(&self, addr: u32, open_bus: u8) -> u8 {
        if Self::is_dma_a_bus_mmio(addr) {
            return open_bus;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index]
        } else if let Some(index) = self.decode_rom_index(addr) {
            self.rom.get(index).copied().unwrap_or(open_bus)
        } else if let Some(index) = self.decode_sram_index(addr) {
            if self.sram.is_empty() {
                open_bus
            } else {
                self.sram[index % self.sram.len()]
            }
        } else {
            open_bus
        }
    }

    fn dma_write_a_bus_impl(&mut self, addr: u32, value: u8) {
        if Self::is_dma_a_bus_mmio(addr) {
            return;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index] = value;
        } else if let Some(index) = self.decode_sram_index(addr) {
            let len = self.sram.len();
            if len != 0 {
                self.sram[index % len] = value;
            }
        }
    }

    fn start_dma_transfer(&mut self, mdmaen: u8) {
        let mut dma = std::mem::take(&mut self.dma);
        let (consumed_ticks, dma_open_bus) = dma.start_dma(mdmaen, self, self.mdr.get());

        self.ticks
            .set(self.ticks.get().wrapping_add(consumed_ticks));
        self.mdr.set(dma_open_bus);
        self.dma = dma;
    }

    fn read_mmio(&self, addr: u32) -> Option<u8> {
        let offset = Self::decode_system_offset(addr)?;
        let value = match offset {
            0x2180 => {
                let wmadd = self.wmadd.get() & 0x1_FFFF;
                let value = self.wram[(wmadd as usize) & (WRAM_SIZE - 1)];
                self.wmadd.set((wmadd + 1) & 0x1_FFFF);
                value
            }
            0x2181 => (self.wmadd.get() & 0xFF) as u8,
            0x2182 => ((self.wmadd.get() >> 8) & 0xFF) as u8,
            0x2183 => ((self.wmadd.get() >> 16) & 0x01) as u8,
            0x4214 => (self.rddiv & 0x00FF) as u8,
            0x4215 => (self.rddiv >> 8) as u8,
            0x4216 => (self.rdmpy & 0x00FF) as u8,
            0x4217 => (self.rdmpy >> 8) as u8,
            0x420D => self.memsel,
            0x4300..=0x437F => self.dma.read_register(offset)?,
            _ => return None,
        };
        Some(value)
    }

    fn write_mmio(&mut self, addr: u32, value: u8) -> bool {
        let Some(offset) = Self::decode_system_offset(addr) else {
            return false;
        };

        match offset {
            0x2180 => {
                let wmadd = self.wmadd.get() & 0x1_FFFF;
                let index = (wmadd as usize) & (WRAM_SIZE - 1);
                self.wram[index] = value;
                self.wmadd.set((wmadd + 1) & 0x1_FFFF);
                true
            }
            0x2181 => {
                let wmadd = self.wmadd.get();
                self.wmadd.set((wmadd & !0x0000_00FF) | value as u32);
                true
            }
            0x2182 => {
                let wmadd = self.wmadd.get();
                self.wmadd
                    .set((wmadd & !0x0000_FF00) | ((value as u32) << 8));
                true
            }
            0x2183 => {
                let wmadd = self.wmadd.get();
                self.wmadd
                    .set((wmadd & !0x0001_0000) | (((value & 0x01) as u32) << 16));
                true
            }
            0x4202 => {
                self.wrmpya = value;
                true
            }
            0x4203 => {
                self.rdmpy = (self.wrmpya as u16).wrapping_mul(value as u16);
                true
            }
            0x4204 => {
                self.wrdiv = (self.wrdiv & 0xFF00) | value as u16;
                true
            }
            0x4205 => {
                self.wrdiv = (self.wrdiv & 0x00FF) | ((value as u16) << 8);
                true
            }
            0x4206 => {
                let dividend = self.wrdiv;
                if value == 0 {
                    self.rddiv = 0xFFFF;
                    self.rdmpy = dividend;
                } else {
                    self.rddiv = dividend / value as u16;
                    self.rdmpy = dividend % value as u16;
                }
                true
            }
            0x420D => {
                self.memsel = value & 0x01;
                true
            }
            0x420B => {
                self.start_dma_transfer(value);
                true
            }
            0x4300..=0x437F => self.dma.write_register(offset, value),
            _ => false,
        }
    }
}

impl DmaABus for SnesSystemBus {
    fn dma_read_a_bus(&mut self, addr: u32, open_bus: u8) -> u8 {
        self.dma_read_a_bus_impl(addr, open_bus)
    }

    fn dma_write_a_bus(&mut self, addr: u32, value: u8) {
        self.dma_write_a_bus_impl(addr, value);
    }
}

impl SnesBus for SnesSystemBus {
    fn read(&self, addr: u32) -> u8 {
        if let Some(value) = self.read_mmio(addr) {
            self.mdr.set(value);
            return value;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            let value = self.wram[index];
            self.mdr.set(value);
            value
        } else if let Some(index) = self.decode_rom_index(addr) {
            if let Some(&value) = self.rom.get(index) {
                self.mdr.set(value);
                value
            } else {
                self.mdr.get()
            }
        } else if let Some(index) = self.decode_sram_index(addr) {
            if self.sram.is_empty() {
                self.mdr.get()
            } else if let Some(&value) = self.sram.get(index % self.sram.len()) {
                self.mdr.set(value);
                value
            } else {
                self.mdr.get()
            }
        } else {
            self.mdr.get()
        }
    }

    fn write(&mut self, addr: u32, value: u8) {
        if self.write_mmio(addr, value) {
            return;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index] = value;
        } else if let Some(index) = self.decode_sram_index(addr) {
            let len = self.sram.len();
            if len != 0 {
                let wrapped = index % len;
                if let Some(slot) = self.sram.get_mut(wrapped) {
                    *slot = value;
                }
            }
        }
    }

    fn tick(&mut self) {
        self.ticks.set(self.ticks.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_cart(
        rom: &mut [u8],
        header_base: usize,
        map_mode: u8,
        ram_size_field: u8,
    ) -> Cartridge {
        let base = header_base;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0xD5] = map_mode;
        rom[base + 0xD6] = 0x00;
        rom[base + 0xD7] = 0x07;
        rom[base + 0xD8] = ram_size_field;
        rom[base + 0xDC] = 0x34;
        rom[base + 0xDD] = 0x12;
        rom[base + 0xDE] = 0xCB;
        rom[base + 0xDF] = 0xED;
        Cartridge::from_bytes(rom).expect("valid test cartridge")
    }

    fn lorom_test_cart() -> Cartridge {
        let mut rom = vec![0u8; 0x10000];
        build_cart(&mut rom, 0x7FC0, 0x20, 0x00)
    }

    fn lorom_cart_with_sram() -> Cartridge {
        let mut rom = vec![0u8; 0x20000];
        build_cart(&mut rom, 0x7FC0, 0x20, 0x05)
    }

    fn write_dma_channel(
        bus: &mut SnesSystemBus,
        channel: u8,
        dmap: u8,
        bbad: u8,
        a_addr: u32,
        count: u16,
    ) {
        let base = 0x004300u32 + (channel as u32) * 0x10;
        bus.write(base, dmap);
        bus.write(base + 0x1, bbad);
        bus.write(base + 0x2, (a_addr & 0xFF) as u8);
        bus.write(base + 0x3, ((a_addr >> 8) & 0xFF) as u8);
        bus.write(base + 0x4, ((a_addr >> 16) & 0xFF) as u8);
        bus.write(base + 0x5, (count & 0xFF) as u8);
        bus.write(base + 0x6, (count >> 8) as u8);
    }

    #[test]
    fn wram_direct_region_round_trips() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0000, 0x5A);
        assert_eq!(bus.read(0x7E0000), 0x5A);
    }

    #[test]
    fn low_ram_mirror_region_maps_to_wram_base() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x000123, 0x3C);
        assert_eq!(bus.read(0x7E0123), 0x3C);
        assert_eq!(bus.read(0x800123), 0x3C);
    }

    #[test]
    fn unmapped_reads_return_mdr_from_last_successful_read() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0010, 0xA7);
        assert_eq!(bus.read(0x7E0010), 0xA7);
        assert_eq!(bus.read(0x002100), 0xA7);
    }

    #[test]
    fn lorom_reads_from_upper_32k_windows() {
        let mut rom = vec![0u8; 0x40000];
        rom[0x000000] = 0x11; // bank 00, addr 8000
        rom[0x008000] = 0x22; // bank 01, addr 8000
        let cart = build_cart(&mut rom, 0x7FC0, 0x20, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0x008000), 0x11);
        assert_eq!(bus.read(0x018000), 0x22);
    }

    #[test]
    fn hirom_reads_from_full_64k_windows() {
        let mut rom = vec![0u8; 0x30000];
        rom[0x000000] = 0x33; // C0:0000
        rom[0x010000] = 0x44; // C1:0000
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0xC00000), 0x33);
        assert_eq!(bus.read(0xC10000), 0x44);
    }

    #[test]
    fn exhirom_maps_c0_ff_to_lower_4mb_and_40_7d_to_upper_4mb() {
        let mut rom = vec![0u8; 0x800000];
        rom[0x000000] = 0x55; // C0:0000
        rom[0x400000] = 0x66; // 40:0000
        let cart = build_cart(&mut rom, 0x40FFC0, 0x35, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0xC00000), 0x55);
        assert_eq!(bus.read(0x400000), 0x66);
    }

    #[test]
    fn lorom_sram_window_round_trips() {
        let mut bus = SnesSystemBus::new(lorom_cart_with_sram());
        bus.write(0x700123, 0x7D);
        assert_eq!(bus.read(0x700123), 0x7D);
    }

    #[test]
    fn hirom_reads_from_40_7d_window() {
        let mut rom = vec![0u8; 0x20000];
        rom[0x000000] = 0x77; // 40:0000
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x00);
        let bus = SnesSystemBus::new(cart);
        assert_eq!(bus.read(0x400000), 0x77);
    }

    #[test]
    fn exhirom_reads_from_low_bank_upper_window() {
        let mut rom = vec![0u8; 0x800000];
        rom[0x008000] = 0x88; // 00:8000
        let cart = build_cart(&mut rom, 0x40FFC0, 0x35, 0x00);
        let bus = SnesSystemBus::new(cart);
        assert_eq!(bus.read(0x008000), 0x88);
    }

    #[test]
    fn hirom_sram_window_mirrors_to_a0_bf_banks() {
        let mut rom = vec![0u8; 0x40000];
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x05);
        let mut bus = SnesSystemBus::new(cart);
        bus.write(0x206123, 0x91);
        assert_eq!(bus.read(0xA06123), 0x91);
    }

    #[test]
    fn sram_window_wraps_for_small_sram_sizes() {
        let mut rom = vec![0u8; 0x20000];
        let cart = build_cart(&mut rom, 0x7FC0, 0x20, 0x01); // 2 KiB SRAM
        let mut bus = SnesSystemBus::new(cart);
        bus.write(0x700000, 0xA1);
        bus.write(0x700800, 0xB2); // +2 KiB -> wraps to same byte
        assert_eq!(bus.read(0x700000), 0xB2);
    }

    #[test]
    fn wram_ports_auto_increment_and_write_through() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        // WMADD = 0x000123
        bus.write(0x002181, 0x23);
        bus.write(0x002182, 0x01);
        bus.write(0x002183, 0x00);
        bus.write(0x002180, 0xAA); // write at 0x0123, increment
        bus.write(0x002180, 0xBB); // write at 0x0124

        assert_eq!(bus.read(0x7E0123), 0xAA);
        assert_eq!(bus.read(0x7E0124), 0xBB);
    }

    #[test]
    fn multiply_registers_update_rdmpy() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004202, 6);
        bus.write(0x004203, 7);

        assert_eq!(bus.read(0x004216), 42);
        assert_eq!(bus.read(0x004217), 0);
    }

    #[test]
    fn divide_registers_update_rddiv_and_remainder() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004204, 0x34);
        bus.write(0x004205, 0x12);
        bus.write(0x004206, 0x10);

        assert_eq!(bus.read(0x004214), 0x23);
        assert_eq!(bus.read(0x004215), 0x01);
        assert_eq!(bus.read(0x004216), 0x04);
        assert_eq!(bus.read(0x004217), 0x00);
    }

    #[test]
    fn memsel_register_reads_back_last_written_value() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x00420D, 0x01);
        assert_eq!(bus.read(0x00420D), 0x01);
        bus.write(0x80420D, 0x00);
        assert_eq!(bus.read(0x00420D), 0x00);
    }

    #[test]
    fn dma_register_file_latches_and_mirrors_across_system_banks() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004300, 0xAB);
        bus.write(0x00430A, 0x55);

        assert_eq!(bus.read(0x004300), 0xAB);
        assert_eq!(bus.read(0x804300), 0xAB);
        assert_eq!(bus.read(0x00430A), 0x55);
    }

    #[test]
    fn dma_channel_register_blocks_do_not_alias() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        for channel in 0u32..8 {
            for reg in 0u32..=0x0B {
                let addr = 0x004300 + channel * 0x10 + reg;
                let value = (channel as u8).wrapping_mul(0x10).wrapping_add(reg as u8);
                bus.write(addr, value);
            }
        }

        for channel in 0u32..8 {
            for reg in 0u32..=0x0B {
                let addr = 0x004300 + channel * 0x10 + reg;
                let expected = (channel as u8).wrapping_mul(0x10).wrapping_add(reg as u8);
                assert_eq!(bus.read(addr), expected);
            }
        }
    }

    #[test]
    fn wram_port_reads_auto_increment_address() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x02);
        bus.write(0x002183, 0x00);
        bus.write(0x002180, 0xC1);
        bus.write(0x002180, 0xD2);
        // Reset WMADD to the first byte and verify consecutive reads advance.
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x02);
        bus.write(0x002183, 0x00);
        assert_eq!(bus.read(0x002180), 0xC1);
        assert_eq!(bus.read(0x002180), 0xD2);
    }

    #[test]
    fn wrdiv_writes_do_not_change_rddiv_until_divide_trigger() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        assert_eq!(bus.read(0x004214), 0x00);
        assert_eq!(bus.read(0x004215), 0x00);
        bus.write(0x004204, 0x34);
        bus.write(0x004205, 0x12);
        assert_eq!(bus.read(0x004214), 0x00);
        assert_eq!(bus.read(0x004215), 0x00);
        bus.write(0x004206, 0x10);
        assert_eq!(bus.read(0x004214), 0x23);
        assert_eq!(bus.read(0x004215), 0x01);
    }

    #[test]
    fn mdmaen_runs_dma_synchronously_and_updates_channel_registers() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0100, 0x3A);
        write_dma_channel(&mut bus, 0, 0x00, 0x00, 0x7E0100, 1);
        bus.write(0x00420B, 0x01);

        // Read back from B-bus via reverse DMA to verify data landed at $2100.
        write_dma_channel(&mut bus, 0, 0x80, 0x00, 0x7E0200, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E0200), 0x3A);

        // DAS reaches zero and A1T advances by transfer byte count.
        assert_eq!(bus.read(0x004305), 0x00);
        assert_eq!(bus.read(0x004306), 0x00);
        assert_eq!(bus.read(0x004302), 0x01);
        assert_eq!(bus.read(0x004303), 0x02);
    }

    #[test]
    fn mdmaen_executes_channels_in_priority_order_and_accounts_cycles() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0100, 0x11);
        bus.write(0x7E0200, 0x22);
        write_dma_channel(&mut bus, 0, 0x00, 0x10, 0x7E0100, 1);
        write_dma_channel(&mut bus, 1, 0x00, 0x10, 0x7E0200, 1);

        let ticks_before = bus.ticks.get();
        bus.write(0x00420B, 0x03);
        let ticks_after = bus.ticks.get();

        // Channel 1 must run after channel 0, so final B-bus value is from channel 1.
        write_dma_channel(&mut bus, 0, 0x80, 0x10, 0x7E0300, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E0300), 0x22);

        // 8/byte + 8/channel + fixed 16 global transfer overhead.
        assert_eq!(ticks_after - ticks_before, 16 + 2 * 8 + 2 * 8);
    }

    #[test]
    fn dma_modes_5_6_7_alias_modes_1_2_3() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // mode 5 should alias mode 1 (p, p+1)
        bus.write(0x7E1000, 0xA1);
        bus.write(0x7E1001, 0xB2);
        write_dma_channel(&mut bus, 0, 0x05, 0x20, 0x7E1000, 2);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x81, 0x20, 0x7E1100, 2);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1100), 0xA1);
        assert_eq!(bus.read(0x7E1101), 0xB2);

        // mode 6 should alias mode 2 (p, p)
        bus.write(0x7E1200, 0xC3);
        bus.write(0x7E1201, 0xD4);
        write_dma_channel(&mut bus, 0, 0x06, 0x24, 0x7E1200, 2);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x82, 0x24, 0x7E1300, 2);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1300), 0xD4);
        assert_eq!(bus.read(0x7E1301), 0xD4);

        // mode 7 should alias mode 3 (p, p, p+1, p+1)
        bus.write(0x7E1400, 0x10);
        bus.write(0x7E1401, 0x20);
        bus.write(0x7E1402, 0x30);
        bus.write(0x7E1403, 0x40);
        write_dma_channel(&mut bus, 0, 0x07, 0x28, 0x7E1400, 4);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x83, 0x28, 0x7E1500, 4);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1500), 0x20);
        assert_eq!(bus.read(0x7E1501), 0x20);
        assert_eq!(bus.read(0x7E1502), 0x40);
        assert_eq!(bus.read(0x7E1503), 0x40);
    }

    #[test]
    fn dma_a_bus_mmio_regions_are_treated_as_open_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // Prime MDR with a value that differs from DMA register file contents.
        bus.write(0x7E0010, 0x9A);
        assert_eq!(bus.read(0x7E0010), 0x9A);
        bus.write(0x004300, 0x55);

        // A-bus source points to excluded MMIO space ($4300); DMA must read open bus (MDR=0x9A).
        write_dma_channel(&mut bus, 0, 0x00, 0x30, 0x004300, 1);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x80, 0x30, 0x7E1600, 1);
        bus.write(0x00420B, 0x01);

        assert_eq!(bus.read(0x7E1600), 0x9A);
    }

    #[test]
    fn dma_byte_count_zero_means_65536_bytes() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E1700, 0x6E);
        write_dma_channel(&mut bus, 0, 0x08, 0x38, 0x7E1700, 0x0000); // fixed A-bus step
        bus.write(0x00420B, 0x01);

        assert_eq!(bus.read(0x004305), 0x00);
        assert_eq!(bus.read(0x004306), 0x00);
        // Fixed addressing keeps A1T unchanged after a 65536-byte transfer.
        assert_eq!(bus.read(0x004302), 0x00);
        assert_eq!(bus.read(0x004303), 0x17);
    }

    #[test]
    fn dma_updates_mdr_with_last_transferred_byte() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E1800, 0x5C);
        write_dma_channel(&mut bus, 0, 0x00, 0x40, 0x7E1800, 1);
        bus.write(0x00420B, 0x01);

        // Unmapped read returns MDR; after DMA it should be the last transferred byte.
        assert_eq!(bus.read(0x002200), 0x5C);
    }
}
