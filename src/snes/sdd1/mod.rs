//! The S-DD1 data decompressor (Star Ocean, Street Fighter Alpha 2).
//!
//! fullsnes "SNES Cart S-DD1" gives the I/O ports, the `$C0-$FF` ROM banks (1 MiB each,
//! selected by `$4804-$4807`), the LoROM exception window at `$00:8000`, and the
//! decompression algorithm (see [`decompressor`]). What fullsnes leaves open follows Mesen2
//! (`Core/SNES/Coprocessors/SDD1/Sdd1.cpp`, `Sdd1Mmc.cpp`), as bsnes does too:
//!
//! - `$00-$3F/$80-$BF:$8000-$FFFF` is LoROM over the first 2 MiB; bit 7 of `$4805` folds
//!   `$20-$3F` onto `$00-$1F`, and bit 7 of `$4807` folds `$A0-$BF` onto `$80-$9F`.
//! - SRAM is at `$70-$73:$0000-$FFFF` and `$00-$3F/$80-$BF:$6000-$7FFF`.
//! - The chip snoops CPU writes of each DMA channel's A-bus address (`$43x2-$43x4`) and byte
//!   count (`$43x5-$43x6`). A read in `$C0-$FF` at a channel's start address, with that
//!   channel's bit set in both `$4800` and `$4801`, returns the next decompressed byte
//!   instead of ROM. The games use fixed-address DMA, so the address does not move. After
//!   the byte count (0 = 65536) the channel's `$4801` bit clears and the next match starts a
//!   new stream.

pub mod decompressor;

use decompressor::Sdd1Decompressor;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Everything the chip holds, saved in save states.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Sdd1State {
    /// `$4800`: channels whose DMA from ROM is decompressed (kept after a transfer).
    pub dma_enable: u8,
    /// `$4801`: channels armed for the next transfer (each bit clears when its transfer ends).
    pub dma_pending: u8,
    /// `$4804-$4807`: the 1 MiB ROM bank mapped at `$C0`, `$D0`, `$E0` and `$F0`.
    pub banks: [u8; 4],
    /// Each channel's A-bus address, snooped from `$43x2-$43x4`.
    pub dma_address: [u32; 8],
    /// Each channel's byte count, snooped from `$43x5-$43x6`, counted down as data is read.
    pub dma_length: [u16; 8],
    /// Whether the next matching read starts a new compressed stream.
    pub need_init: bool,
    pub decompressor: Sdd1Decompressor,
}

impl Default for Sdd1State {
    /// The state after /RES: banks 0-3 in order, nothing armed (Mesen2 `Sdd1::Reset`).
    fn default() -> Self {
        Self {
            dma_enable: 0,
            dma_pending: 0,
            banks: [0, 1, 2, 3],
            dma_address: [0; 8],
            dma_length: [0; 8],
            need_init: true,
            decompressor: Sdd1Decompressor::default(),
        }
    }
}

pub struct Sdd1 {
    rom: Rc<Vec<u8>>,
    state: Sdd1State,
}

impl Sdd1 {
    pub fn new(rom: Rc<Vec<u8>>) -> Self {
        Self {
            rom,
            state: Sdd1State::default(),
        }
    }

    /// /RES: back to the power-on banks with nothing armed.
    pub fn reset(&mut self) {
        self.state = Sdd1State::default();
    }

    /// `$4800-$4807` reads, for a system-bank offset. `None` is open bus: `$4802`/`$4803`
    /// and everything outside the chip's ports.
    pub fn read_register(&self, offset: u16) -> Option<u8> {
        match offset {
            0x4800 => Some(self.state.dma_enable),
            0x4801 => Some(self.state.dma_pending),
            0x4804..=0x4807 => Some(self.state.banks[usize::from(offset & 3)]),
            _ => None,
        }
    }

    /// `$4800-$4807` writes; `false` when `offset` is not one of the chip's ports.
    pub fn write_register(&mut self, offset: u16, value: u8) -> bool {
        match offset {
            0x4800 => self.state.dma_enable = value,
            0x4801 => self.state.dma_pending = value,
            // fullsnes: "Unknown ... set to 0000h by Star Ocean"; the chip takes the write.
            0x4802 | 0x4803 => {}
            0x4804..=0x4807 => self.state.banks[usize::from(offset & 3)] = value,
            _ => return false,
        }
        true
    }

    /// Watches a CPU write to the DMA registers (`$4300-$437F`) for each channel's source
    /// address and byte count. The DMA controller still receives the write.
    pub fn snoop_dma_register(&mut self, offset: u16, value: u8) {
        if !(0x4300..=0x437F).contains(&offset) {
            return;
        }
        let channel = usize::from((offset >> 4) & 7);
        let address = &mut self.state.dma_address[channel];
        let length = &mut self.state.dma_length[channel];
        let value = u32::from(value);
        match offset & 0x0F {
            0x2 => *address = (*address & 0xFF_FF00) | value,
            0x3 => *address = (*address & 0xFF_00FF) | (value << 8),
            0x4 => *address = (*address & 0x00_FFFF) | (value << 16),
            0x5 => *length = (*length & 0xFF00) | value as u16,
            0x6 => *length = (*length & 0x00FF) | ((value as u16) << 8),
            _ => {}
        }
    }

    /// ROM offset of `addr`, or `None` outside the chip's two ROM windows.
    pub fn rom_index(&self, addr: u32) -> Option<usize> {
        if self.rom.is_empty() {
            return None;
        }
        let addr = addr & 0xFF_FFFF;
        let bank = (addr >> 16) as u8;
        let offset = (addr & 0xFFFF) as usize;
        let index = if bank >= 0xC0 {
            let selected = usize::from(self.state.banks[usize::from((bank >> 4) & 3)] & 0x0F);
            (selected << 20) | (addr as usize & 0xF_FFFF)
        } else if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset >= 0x8000 {
            let fold_register = if bank & 0x80 != 0 { 3 } else { 1 };
            let bank_mask = if self.state.banks[fold_register] & 0x80 != 0 {
                0x1F
            } else {
                0x3F
            };
            usize::from(bank & bank_mask) * 0x8000 + (offset - 0x8000)
        } else {
            return None;
        };
        Some(index % self.rom.len())
    }

    /// A read the CPU or DMA makes of ROM: decompressed data when a DMA from ROM is being
    /// decompressed at this address, the ROM byte otherwise. `None` outside the ROM windows.
    pub fn read_rom(&mut self, addr: u32) -> Option<u8> {
        let addr = addr & 0xFF_FFFF;
        let active = self.state.dma_enable & self.state.dma_pending;
        if active != 0 && addr >= 0xC0_0000 {
            let channel = (0..8)
                .find(|&ch| active & (1 << ch) != 0 && self.state.dma_address[ch] == addr);
            if let Some(channel) = channel {
                return Some(self.next_decompressed_byte(channel, addr));
            }
        }
        self.peek_rom(addr)
    }

    fn next_decompressed_byte(&mut self, channel: usize, addr: u32) -> u8 {
        let mut decompressor = std::mem::take(&mut self.state.decompressor);
        let mut read = |source: u32| self.peek_rom(source).unwrap_or(0);
        if self.state.need_init {
            decompressor.init(addr, &mut read);
        }
        let byte = decompressor.next_byte(&mut read);
        self.state.decompressor = decompressor;
        self.state.need_init = false;
        let length = &mut self.state.dma_length[channel];
        *length = length.wrapping_sub(1);
        if *length == 0 {
            self.state.need_init = true;
            self.state.dma_pending &= !(1 << channel);
        }
        byte
    }

    /// The ROM byte at `addr` for a debugger; never advances a decompression.
    pub fn peek_rom(&self, addr: u32) -> Option<u8> {
        self.rom_index(addr).map(|index| self.rom[index])
    }

    /// Linear SRAM offset of `addr` (the caller wraps it by the SRAM size), or `None`.
    pub fn sram_offset(addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = (addr >> 16) as u8;
        let offset = (addr & 0xFFFF) as usize;
        match bank {
            0x70..=0x73 => Some(usize::from(bank - 0x70) * 0x1_0000 + offset),
            0x00..=0x3F | 0x80..=0xBF if (0x6000..0x8000).contains(&offset) => {
                Some(offset - 0x6000)
            }
            _ => None,
        }
    }

    pub fn capture_state(&self) -> Sdd1State {
        self.state.clone()
    }

    pub fn restore_state(&mut self, state: &Sdd1State) {
        self.state = state.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8 MiB whose every byte is distinct per 64 KiB page, so a wrong offset shows.
    fn chip_with_rom(len: usize) -> Sdd1 {
        let rom: Vec<u8> = (0..len).map(|i| (i ^ (i >> 16)) as u8).collect();
        Sdd1::new(Rc::new(rom))
    }

    #[test]
    fn banks_reset_to_0_1_2_3_and_map_c0_ff_in_1mib_units() {
        let mut chip = chip_with_rom(0x80_0000);
        assert_eq!(chip.rom_index(0xC1_2345), Some(0x01_2345));
        assert_eq!(chip.rom_index(0xD1_2345), Some(0x11_2345));
        assert_eq!(chip.rom_index(0xE0_0000), Some(0x20_0000));
        assert_eq!(chip.rom_index(0xFF_FFFF), Some(0x3F_FFFF));

        assert!(chip.write_register(0x4804, 0x05));
        assert!(chip.write_register(0x4807, 0x07));
        assert_eq!(chip.rom_index(0xC1_2345), Some(0x51_2345));
        assert_eq!(chip.rom_index(0xFF_FFFF), Some(0x7F_FFFF));

        chip.reset();
        assert_eq!(chip.rom_index(0xC1_2345), Some(0x01_2345));
    }

    #[test]
    fn lorom_window_covers_first_2mib_and_leaves_other_banks_unmapped() {
        let chip = chip_with_rom(0x40_0000);
        assert_eq!(chip.rom_index(0x00_8000), Some(0x00_0000));
        assert_eq!(chip.rom_index(0x01_8123), Some(0x00_8123));
        assert_eq!(chip.rom_index(0x3F_FFFF), Some(0x1F_FFFF));
        assert_eq!(chip.rom_index(0x80_8000), Some(0x00_0000));
        assert_eq!(chip.rom_index(0xBF_FFFF), Some(0x1F_FFFF));
        assert_eq!(chip.rom_index(0x00_7FFF), None);
        assert_eq!(chip.rom_index(0x40_8000), None);
        assert_eq!(chip.rom_index(0x7D_8000), None);
    }

    #[test]
    fn lorom_window_folds_20_3f_when_4805_bit7_set() {
        let mut chip = chip_with_rom(0x40_0000);
        chip.write_register(0x4805, 0x81);
        assert_eq!(chip.rom_index(0x20_8000), Some(0x00_0000));
        assert_eq!(chip.rom_index(0x3F_FFFF), Some(0x0F_FFFF));
        // $80-$BF answers to $4807, not $4805.
        assert_eq!(chip.rom_index(0xA0_8000), Some(0x10_0000));
    }

    #[test]
    fn lorom_window_folds_a0_bf_when_4807_bit7_set() {
        let mut chip = chip_with_rom(0x40_0000);
        chip.write_register(0x4807, 0x83);
        assert_eq!(chip.rom_index(0xA0_8000), Some(0x00_0000));
        assert_eq!(chip.rom_index(0x20_8000), Some(0x10_0000));
    }

    #[test]
    fn registers_read_back_and_4802_is_open_bus() {
        let mut chip = chip_with_rom(0x10_0000);
        for (port, value) in [(0x4800, 0x11), (0x4801, 0x22), (0x4804, 0x33), (0x4807, 0x44)] {
            assert!(chip.write_register(port, value));
            assert_eq!(chip.read_register(port), Some(value), "${port:04X}");
        }
        assert_eq!(chip.read_register(0x4805), Some(1));
        assert_eq!(chip.read_register(0x4802), None);
        assert_eq!(chip.read_register(0x4803), None);
        assert_eq!(chip.read_register(0x4808), None);
        assert!(!chip.write_register(0x4808, 0));
    }

    /// Arms channel `ch` for a `len`-byte transfer from `addr` as a game does: DMA registers
    /// first, then `$4800`/`$4801`.
    fn arm(chip: &mut Sdd1, ch: u16, addr: u32, len: u16) {
        let base = 0x4300 | (ch << 4);
        chip.snoop_dma_register(base + 2, addr as u8);
        chip.snoop_dma_register(base + 3, (addr >> 8) as u8);
        chip.snoop_dma_register(base + 4, (addr >> 16) as u8);
        chip.snoop_dma_register(base + 5, len as u8);
        chip.snoop_dma_register(base + 6, (len >> 8) as u8);
        chip.write_register(0x4800, 1 << ch);
        chip.write_register(0x4801, 1 << ch);
    }

    /// A compressed stream at `$C0:1000`: raw-byte mode, header `$CA` then zeros, which
    /// Mesen2's decoder turns into `51 04 51 04 ...` (unlike the ROM bytes themselves).
    fn chip_with_stream() -> Sdd1 {
        let mut rom = vec![0xA5u8; 0x10_0000];
        rom[0x1000..0x1010].fill(0x00);
        rom[0x1000] = 0xCA;
        Sdd1::new(Rc::new(rom))
    }

    #[test]
    fn matching_dma_read_returns_decompressed_bytes_and_clears_4801_bit() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 3, 0xC0_1000, 4);
        let data: Vec<_> = (0..4).map(|_| chip.read_rom(0xC0_1000)).collect();
        assert_eq!(data, [Some(0x51), Some(0x04), Some(0x51), Some(0x04)]);
        assert_eq!(chip.read_register(0x4801), Some(0x00));
        assert_eq!(chip.read_register(0x4800), Some(0x08), "$4800 is kept");
        // Transfer over: the address reads as ROM again.
        assert_eq!(chip.read_rom(0xC0_1000), Some(0xCA));
        assert_eq!(chip.read_rom(0xC0_2000), Some(0xA5));
    }

    #[test]
    fn a_new_transfer_restarts_the_stream() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 0, 0xC0_1000, 1);
        assert_eq!(chip.read_rom(0xC0_1000), Some(0x51));
        arm(&mut chip, 0, 0xC0_1000, 1);
        assert_eq!(chip.read_rom(0xC0_1000), Some(0x51));
    }

    #[test]
    fn read_without_both_enable_bits_returns_raw_rom() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 0, 0xC0_1010, 4);
        chip.write_register(0x4801, 0x00);
        assert_eq!(chip.read_rom(0xC0_1010), Some(0xA5));
        chip.write_register(0x4800, 0x00);
        chip.write_register(0x4801, 0x01);
        assert_eq!(chip.read_rom(0xC0_1010), Some(0xA5));
    }

    #[test]
    fn read_at_other_address_or_outside_c0_ff_returns_raw_rom() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 0, 0xC0_1010, 4);
        assert_eq!(chip.read_rom(0xC0_1011), Some(0xA5));
        // $00:9010 is the same ROM byte through the LoROM window: never decompressed.
        chip.snoop_dma_register(0x4302, 0x10);
        chip.snoop_dma_register(0x4303, 0x90);
        chip.snoop_dma_register(0x4304, 0x00);
        assert_eq!(chip.read_rom(0x00_9010), Some(0xA5));
        assert_eq!(chip.read_register(0x4801), Some(0x01));
    }

    #[test]
    fn zero_length_means_65536() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 1, 0xC0_1000, 0);
        for _ in 0..0xFFFF {
            chip.read_rom(0xC0_1000);
        }
        assert_eq!(chip.read_register(0x4801), Some(0x02));
        chip.read_rom(0xC0_1000);
        assert_eq!(chip.read_register(0x4801), Some(0x00));
    }

    #[test]
    fn peek_never_advances_the_decompressor() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 0, 0xC0_1010, 2);
        assert_eq!(chip.peek_rom(0xC0_1010), Some(0xA5));
        let before = chip.capture_state();
        chip.peek_rom(0xC0_1010);
        assert_eq!(chip.capture_state(), before);
    }

    #[test]
    fn state_round_trips_mid_transfer() {
        let mut chip = chip_with_stream();
        arm(&mut chip, 0, 0xC0_1000, 4);
        chip.read_rom(0xC0_1000);
        let saved = chip.capture_state();
        let mut other = chip_with_stream();
        other.restore_state(&saved);
        assert_eq!(other.capture_state(), saved);
        assert_eq!(other.read_rom(0xC0_1000), chip.read_rom(0xC0_1000));
    }

    #[test]
    fn sram_windows() {
        assert_eq!(Sdd1::sram_offset(0x70_0000), Some(0x0_0000));
        assert_eq!(Sdd1::sram_offset(0x73_FFFF), Some(0x3_FFFF));
        assert_eq!(Sdd1::sram_offset(0x00_6000), Some(0x0000));
        assert_eq!(Sdd1::sram_offset(0xBF_7FFF), Some(0x1FFF));
        assert_eq!(Sdd1::sram_offset(0x74_0000), None);
        assert_eq!(Sdd1::sram_offset(0x00_8000), None);
        assert_eq!(Sdd1::sram_offset(0x40_6000), None);
    }
}
