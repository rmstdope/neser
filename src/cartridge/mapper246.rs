//! Mapper 246 - Fong Shen Bang
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_246>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::{BankedRom, ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 246 - Fong Shen Bang
///
/// Hardware: Register-based PRG and CHR bank switching.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_246>
/// - PRG-ROM: Up to 512KB (8KB banks)
/// - CHR-ROM: Up to 512KB (2KB banks)
/// - Mirroring: Fixed from header
///
/// Registers at $6000-$6007 (write only, within PRG-RAM range):
/// - $6000: PRG bank 0 ($8000-$9FFF, 8KB)
/// - $6001: PRG bank 1 ($A000-$BFFF, 8KB)
/// - $6002: PRG bank 2 ($C000-$DFFF, 8KB)
/// - $6003: PRG bank 3 ($E000-$FFFF, 8KB)
/// - $6004: CHR bank 0 ($0000-$07FF, 2KB)
/// - $6005: CHR bank 1 ($0800-$0FFF, 2KB)
/// - $6006: CHR bank 2 ($1000-$17FF, 2KB)
/// - $6007: CHR bank 3 ($1800-$1FFF, 2KB)
///
/// PRG-RAM at $6800-$7FFF (6KB usable, excluding register space)
pub struct Mapper246 {
    prg_rom: BankedRom,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    prg_banks: [u8; 4],
    chr_banks: [u8; 4],
}

impl Mapper246 {
    const MAPPER_NUMBER: u8 = 246;
    const PRG_BANK_SIZE: usize = 8 * 1024; // 8KB
    const CHR_BANK_SIZE: usize = 2 * 1024; // 2KB

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        let num_prg_banks = prg_rom.len() / Self::PRG_BANK_SIZE;
        // Default: last 4 banks of PRG for initial mapping
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as u8
        } else {
            0
        };
        Self {
            prg_rom: BankedRom::new(prg_rom, Self::PRG_BANK_SIZE),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            prg_banks: [0, 1, last_bank.saturating_sub(1), last_bank],
            chr_banks: [0, 0, 0, 0],
        }
    }

    fn resolve_prg_bank(&self, bank: u8) -> usize {
        let num_banks = self.prg_rom.num_banks();
        if num_banks == 0 {
            0
        } else {
            (bank as usize) % num_banks
        }
    }

    fn resolve_chr_bank(&self, bank: u8) -> usize {
        let total_size = self.chr_memory.size();
        let num_banks = total_size / Self::CHR_BANK_SIZE;
        if num_banks == 0 {
            0
        } else {
            (bank as usize) % num_banks
        }
    }
}

impl Mapper for Mapper246 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x67FF => {
                // Register space - reads return open bus (0)
                0
            }
            0x6800..=0x7FFF => {
                // PRG-RAM (offset by $800 to skip register space)
                self.prg_ram.try_read(addr).unwrap_or(0)
            }
            0x8000..=0x9FFF => {
                let bank = self.resolve_prg_bank(self.prg_banks[0]);
                let offset = (addr - 0x8000) as usize;
                self.prg_rom.read(bank, offset)
            }
            0xA000..=0xBFFF => {
                let bank = self.resolve_prg_bank(self.prg_banks[1]);
                let offset = (addr - 0xA000) as usize;
                self.prg_rom.read(bank, offset)
            }
            0xC000..=0xDFFF => {
                let bank = self.resolve_prg_bank(self.prg_banks[2]);
                let offset = (addr - 0xC000) as usize;
                self.prg_rom.read(bank, offset)
            }
            0xE000..=0xFFFF => {
                let bank = self.resolve_prg_bank(self.prg_banks[3]);
                let offset = (addr - 0xE000) as usize;
                self.prg_rom.read(bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000 => self.prg_banks[0] = value,
            0x6001 => self.prg_banks[1] = value,
            0x6002 => self.prg_banks[2] = value,
            0x6003 => self.prg_banks[3] = value,
            0x6004 => self.chr_banks[0] = value,
            0x6005 => self.chr_banks[1] = value,
            0x6006 => self.chr_banks[2] = value,
            0x6007 => self.chr_banks[3] = value,
            0x6800..=0x7FFF => {
                self.prg_ram.try_write(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = ((addr >> 11) & 0x03) as usize; // 2KB slots: 0-3
        let bank = self.resolve_chr_bank(self.chr_banks[slot]);
        let offset = (addr & 0x07FF) as usize;
        let index = bank * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.read_at_index(index)
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
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut regs = Vec::with_capacity(8);
        regs.extend_from_slice(&self.prg_banks);
        regs.extend_from_slice(&self.chr_banks);
        regs
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.prg_banks.copy_from_slice(&data[0..4]);
        }
        if data.len() >= 8 {
            self.chr_banks.copy_from_slice(&data[4..8]);
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.prg_ram.initialize(mode);
        self.chr_memory.initialize(mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper246(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(246, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_factory_creates_mapper_246() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 246 should be creatable via factory");
    }

    #[test]
    fn test_prg_bank_switching_8kb() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set all 4 PRG banks
        mapper.write_prg(0x6000, 5); // $8000-$9FFF = bank 5
        mapper.write_prg(0x6001, 10); // $A000-$BFFF = bank 10
        mapper.write_prg(0x6002, 3); // $C000-$DFFF = bank 3
        mapper.write_prg(0x6003, 7); // $E000-$FFFF = bank 7

        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xA000), 10);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_chr_bank_switching_2kb() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set 4 CHR banks (2KB each covering $0000-$1FFF)
        mapper.write_prg(0x6004, 3); // $0000-$07FF = bank 3
        mapper.write_prg(0x6005, 7); // $0800-$0FFF = bank 7
        mapper.write_prg(0x6006, 1); // $1000-$17FF = bank 1
        mapper.write_prg(0x6007, 12); // $1800-$1FFF = bank 12

        assert_eq!(mapper.read_chr(0x0000), 3);
        assert_eq!(mapper.read_chr(0x0800), 7);
        assert_eq!(mapper.read_chr(0x1000), 1);
        assert_eq!(mapper.read_chr(0x1800), 12);
    }

    #[test]
    fn test_prg_ram_at_6800_7fff() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // PRG-RAM should work at $6800-$7FFF
        mapper.write_prg(0x6800, 0x42);
        mapper.write_prg(0x7FFF, 0xAB);
        assert_eq!(mapper.read_prg(0x6800), 0x42);
        assert_eq!(mapper.read_prg(0x7FFF), 0xAB);
    }

    #[test]
    fn test_register_space_does_not_return_ram() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write to register space
        mapper.write_prg(0x6000, 0xFF);
        // Read from register space should return 0 (open bus), not the written value
        assert_eq!(mapper.read_prg(0x6000), 0);
    }

    #[test]
    fn test_mirroring_is_fixed() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Horizontal).unwrap();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_prg_bank_wrapping() {
        let prg_rom = banked_data(8 * 1024, 8); // 8 banks
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Bank 10 should wrap to bank 2 (10 % 8)
        mapper.write_prg(0x6000, 10);
        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper =
            create_mapper246(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 5);
        mapper.write_prg(0x6001, 10);
        mapper.write_prg(0x6002, 3);
        mapper.write_prg(0x6003, 7);
        mapper.write_prg(0x6004, 2);
        mapper.write_prg(0x6005, 8);
        mapper.write_prg(0x6006, 1);
        mapper.write_prg(0x6007, 14);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_prg(0xA000), 10);
        assert_eq!(restored.read_prg(0xC000), 3);
        assert_eq!(restored.read_prg(0xE000), 7);
        assert_eq!(restored.read_chr(0x0000), 2);
        assert_eq!(restored.read_chr(0x0800), 8);
        assert_eq!(restored.read_chr(0x1000), 1);
        assert_eq!(restored.read_chr(0x1800), 14);
    }
}
