use crate::snes::bus::SnesBus;
use crate::snes::cartridge::Cartridge;
use crate::snes::cartridge::Mapping;
use std::cell::Cell;

const WRAM_SIZE: usize = 128 * 1024;

/// SNES system bus (placeholder).
///
/// Real memory map/MMIO behavior is implemented incrementally in issue #2744.
pub struct SnesSystemBus {
    _cartridge: Cartridge,
    mapping: Mapping,
    rom: Vec<u8>,
    sram: Vec<u8>,
    wram: Vec<u8>,
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
}

impl SnesBus for SnesSystemBus {
    fn read(&self, addr: u32) -> u8 {
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
}
