//! Mapper 7 - AxROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing/board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{BankSwitch, BankedRom, ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE_32K: usize = 0x8000; // 32KB (for AxROM)

/// Mapper 7 - AxROM (AMROM, ANROM, AN1ROM, AOROM boards)
///
/// Hardware: Simple 32KB PRG banking with programmable one-screen mirroring
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/AxROM>
/// - Variants: <https://www.nesdev.org/wiki/AxROM#Variants>
/// - PRG-ROM: Up to 256KB (8 32KB banks)
/// - PRG-RAM: None (some bootleg boards have 8KB)
/// - CHR: 8KB CHR-RAM fixed (no CHR-ROM support)
/// - Mirroring: Programmable one-screen (selectable A or B nametable)
///
/// Common boards: NES-AMROM, NES-ANROM, NES-AOROM
///
/// Notes:
/// - Register at any write to $8000-$FFFF
/// - Bits 0-2: Select 32KB PRG bank
/// - Bit 4: One-screen mirroring (0 = lower/A, 1 = upper/B)
/// - Used in Battletoads, Marble Madness, Wizards & Warriors
pub struct AxROMMapper {
    prg_rom: BankedRom,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    prg_bank: BankSwitch,
    mirroring_bit: bool, // Bit 4 from bank select register
}

impl AxROMMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, _mirroring: MirroringMode) -> Self {
        // AxROM uses CHR-RAM, ignores chr_rom and initial mirroring (controlled by register)
        let prg_bank = BankSwitch::from_rom(&prg_rom, PRG_BANK_SIZE_32K);

        Self {
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE_32K),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new_ram(8192),
            prg_bank,
            mirroring_bit: false, // Default to lower nametable
        }
    }
}

impl Mapper for AxROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF (32KB switchable bank)
        match addr {
            0x8000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank.current(), 0x8000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // Register at $8000-$FFFF
        // Bits 0-2: PRG bank select
        // Bit 4: One-screen mirroring (0 = lower, 1 = upper)
        if (0x8000..=0xFFFF).contains(&addr) {
            self.prg_bank.set(value & 0x07);
            self.mirroring_bit = (value & 0x10) != 0;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> MirroringMode {
        // Bit 4 determines one-screen mirroring mode
        // 0 = lower nametable (single-screen A)
        // 1 = upper nametable (single-screen B)
        if self.mirroring_bit {
            MirroringMode::SingleScreenUpper
        } else {
            MirroringMode::SingleScreenLower
        }
    }

    fn mapper_number(&self) -> u8 {
        7
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
        // Store both bank and mirroring bit in a single byte
        let value = self.prg_bank.raw() | (if self.mirroring_bit { 0x10 } else { 0 });
        vec![value]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.prg_bank.set(value & 0x07);
            self.mirroring_bit = (value & 0x10) != 0;
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    fn create_axrom_mapper(prg_rom: Vec<u8>, mirroring: MirroringMode) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new(7, prg_rom, vec![], mirroring))
            .expect("Failed to create AxROM mapper")
    }

    #[test]
    fn test_axrom_256kb_prg_bank_switching() {
        // AxROM with 256KB (8 banks × 32KB)
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each 32KB bank with its bank number
        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Default bank should be 0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_axrom_bank_select_bits_0_2() {
        // Test that bits 0-2 select the bank (3-bit bank select = 8 banks max)
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write to $8000 with different bank values
        mapper.write_prg(0x8000, 0x00); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 0x01); // Bank 1
        assert_eq!(mapper.read_prg(0x8000), 101);

        mapper.write_prg(0x8000, 0x07); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 107);

        // Test that upper bits are ignored (only bits 0-2 matter for bank)
        mapper.write_prg(0x8000, 0xF2); // 0b11110010 -> bank 2
        assert_eq!(mapper.read_prg(0x8000), 102);
    }

    #[test]
    fn test_axrom_chr_ram() {
        // AxROM uses 8KB CHR-RAM (no CHR ROM)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        // Read back
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_lower() {
        // Bit 4 = 0 selects lower nametable (single-screen A)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x8000, 0x00); // Bits: 0000 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Write with bit 4 = 0 but other bits set
        mapper.write_prg(0x8000, 0x07); // Bits: 0000 0111
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_upper() {
        // Bit 4 = 1 selects upper nametable (single-screen B)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x8000, 0x10); // Bits: 0001 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
    }

    #[test]
    fn test_axrom_registers_and_chr_ram_snapshot_roundtrip() {
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 1) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom.clone(), MirroringMode::Horizontal);

        mapper.write_prg(0x8000, 0x07); // select bank 7
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        let registers = mapper.registers_snapshot();
        let chr_ram = mapper.chr_ram_snapshot();

        let mut restored = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);
        restored.restore_registers(&registers);
        restored.restore_chr_ram(&chr_ram);

        assert_eq!(restored.read_prg(0x8000), 8);
        assert_eq!(restored.read_chr(0x0000), 0x42);
        assert_eq!(restored.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_128kb_rom_4_banks() {
        // Test with 128KB ROM (4 banks × 32KB)
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Select each of the 4 banks
        for bank in 0..4 {
            mapper.write_prg(0x8000, bank as u8);
            assert_eq!(mapper.read_prg(0x8000), (bank + 50) as u8);
        }

        // Bank numbers wrap (bank 7 % 4 = 3)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 53); // Bank 3
    }

    #[test]
    fn test_axrom_register_write_any_address() {
        // Writes anywhere in $8000-$FFFF should change the bank
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write to different addresses in PRG ROM space
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 11);

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 12);
    }

    #[test]
    fn test_axrom_prg_ram_support() {
        // AxROM should support PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
    }

    #[test]
    fn test_axrom_open_bus() {
        let prg_rom = vec![0; 128 * 1024];
        let mapper = create_axrom_mapper(prg_rom, MirroringMode::Horizontal);

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xEE), 0xEE);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xFF), 0xFF);
    }
}
