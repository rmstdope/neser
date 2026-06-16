use crate::snes::bus::SnesBus;
use crate::snes::cartridge::Cartridge;
use std::cell::Cell;

const WRAM_SIZE: usize = 128 * 1024;

/// SNES system bus (placeholder).
///
/// Real memory map/MMIO behavior is implemented incrementally in issue #2744.
pub struct SnesSystemBus {
    _cartridge: Cartridge,
    wram: Vec<u8>,
    mdr: Cell<u8>,
    ticks: Cell<u64>,
}

impl SnesSystemBus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            _cartridge: cartridge,
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
}

impl SnesBus for SnesSystemBus {
    fn read(&self, addr: u32) -> u8 {
        if let Some(index) = Self::decode_wram_index(addr) {
            let value = self.wram[index];
            self.mdr.set(value);
            value
        } else {
            self.mdr.get()
        }
    }

    fn write(&mut self, addr: u32, value: u8) {
        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index] = value;
        }
    }

    fn tick(&mut self) {
        self.ticks.set(self.ticks.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lorom_test_cart() -> Cartridge {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0xD5] = 0x20;
        rom[base + 0xD6] = 0x00;
        rom[base + 0xD7] = 0x07;
        rom[base + 0xD8] = 0x00;
        rom[base + 0xDC] = 0x34;
        rom[base + 0xDD] = 0x12;
        rom[base + 0xDE] = 0xCB;
        rom[base + 0xDF] = 0xED;
        Cartridge::from_bytes(&rom).expect("valid test cartridge")
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
}
