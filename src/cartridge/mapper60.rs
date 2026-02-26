//! Mapper 060 - Reset-based NROM-128 4-in-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_060>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 060 - Reset-based NROM-128 4-in-1
///
/// Hardware: Multicart PCB with reset counter.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_060>
/// - PRG-ROM: 64 KiB total (4 × 16 KiB, one per game)
/// - CHR: 32 KiB total (4 × 8 KiB, one per game)
/// - Mirroring: Fixed from header
///
/// Each of the 4 games is NROM-128:
/// - PRG: 16 KiB bank = game_select (same page at $8000 and $C000)
/// - CHR: 8 KiB bank = game_select
///
/// The game is selected by a 2-bit internal counter that increments on every
/// soft reset and wraps from 3 back to 0.
pub struct Mapper60 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    game_select: u8, // 2-bit counter, incremented on reset
}

impl Mapper60 {
    const MAPPER_NUMBER: u8 = 60;
    const PRG_BANK_SIZE: usize = 0x4000; // 16 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            game_select: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn chr_bank_count(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
    }
}

impl Mapper for Mapper60 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let count = self.prg_bank_count();
                if count == 0 {
                    return 0;
                }
                let bank = (self.game_select as usize) % count;
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_rom
                    .get(bank * Self::PRG_BANK_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // No writable registers
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let count = self.chr_bank_count();
        if count == 0 {
            return self.chr_memory.read(addr);
        }
        let bank = (self.game_select as usize) % count;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.chr_memory
            .read_at_index(bank * Self::CHR_BANK_SIZE + offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.game_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&v) = data.first() {
            self.game_select = v & 0x03;
        }
    }

    /// On reset, advance to the next game (2-bit counter).
    fn reset(&mut self) {
        self.game_select = (self.game_select + 1) & 0x03;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper60 {
        let prg = banked_data(16 * 1024, 4);
        let chr = banked_data(8 * 1024, 4);
        Mapper60::new(MapperContext::new_for_test(
            60,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_60_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            60,
            banked_data(16 * 1024, 4),
            banked_data(8 * 1024, 4),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 60 must be registered");
    }

    #[test]
    fn default_is_game_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn prg_mirrors_same_bank_at_8000_and_c000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            mapper.read_prg(0xC000),
            "NROM-128: $8000 and $C000 are the same bank"
        );
    }

    #[test]
    fn reset_advances_game_select() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.game_select, 0);
        mapper.reset();
        assert_eq!(mapper.game_select, 1);
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.reset();
        assert_eq!(mapper.game_select, 2);
        mapper.reset();
        assert_eq!(mapper.game_select, 3);
        mapper.reset();
        assert_eq!(mapper.game_select, 0, "counter wraps from 3 to 0");
    }

    #[test]
    fn chr_advances_with_game_select() {
        let mut mapper = make_mapper();
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 1);
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn registers_snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.reset();
        mapper.reset(); // game_select = 2
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), 2);
        assert_eq!(r.read_chr(0x0000), 2);
    }
}
