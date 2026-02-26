//! Mapper 7 - AxROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing/board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::{BankSwitch, BankedRom, ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::trace_mapper;

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
    chr_memory: ChrMemory,
    prg_ram: Option<PrgRam>,
    prg_bank: BankSwitch,
    mirroring_bit: bool, // Bit 4 from bank select register
    bus_conflicts: bool,
}

impl AxROMMapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let submapper = if ctx.submapper == 0 && ctx.crc32 == 0x41D3_2FD7 {
            2
        } else {
            ctx.submapper
        };
        let prg_ram_banks_8k = ctx.prg_ram_banks_8k;
        let prg_rom = ctx.prg_rom;
        Self::new_with_submapper_and_prg_ram_banks(prg_rom, submapper, prg_ram_banks_8k)
    }

    pub fn new_with_submapper_and_prg_ram_banks(
        prg_rom: Vec<u8>,
        submapper: u8,
        prg_ram_banks_8k: u8,
    ) -> Self {
        let normalized_prg_rom = Self::normalize_prg_rom(prg_rom);

        // AxROM uses CHR-RAM, ignores chr_rom and initial mirroring (controlled by register)
        let prg_bank = BankSwitch::from_rom(&normalized_prg_rom, PRG_BANK_SIZE_32K);
        let bus_conflicts = submapper == 2;
        let prg_ram = if prg_ram_banks_8k == 0 {
            None
        } else {
            Some(PrgRam::new(
                prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        };

        Self {
            prg_rom: BankedRom::new(normalized_prg_rom, PRG_BANK_SIZE_32K),
            chr_memory: ChrMemory::new_ram(8192),
            prg_ram,
            prg_bank,
            mirroring_bit: false, // Default to lower nametable
            bus_conflicts,
        }
    }

    fn normalize_prg_rom(mut prg_rom: Vec<u8>) -> Vec<u8> {
        if prg_rom.len() == 0x4000 {
            let mirrored_half = prg_rom.clone();
            prg_rom.extend_from_slice(&mirrored_half);
        }
        prg_rom
    }
}

impl Mapper for AxROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG ROM at $8000-$FFFF (32KB switchable bank)
        if let Some(prg_ram) = &self.prg_ram
            && let Some(value) = prg_ram.try_read(addr)
        {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank.current(), 0x8000, addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if let Some(prg_ram) = &mut self.prg_ram
            && prg_ram.try_write(addr, value)
        {
            return;
        }

        // Register at $8000-$FFFF
        // Bits 0-2: PRG bank select
        // Bit 4: One-screen mirroring (0 = lower, 1 = upper)
        if (0x8000..=0xFFFF).contains(&addr) {
            let register_value = if self.bus_conflicts {
                value & self.read_prg(addr)
            } else {
                value
            };

            self.prg_bank.set(register_value & 0x07);
            self.mirroring_bit = (register_value & 0x10) != 0;

            trace_mapper!(1;
                "AxROM write ${:04X}: raw=${:02X} effective=${:02X} bank={} mirroring={} conflicts={}",
                addr,
                value,
                register_value,
                register_value & 0x07,
                if self.mirroring_bit { "upper" } else { "lower" },
                self.bus_conflicts
            );
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        // Bit 4 determines one-screen mirroring mode
        // 0 = lower nametable (single-screen A)
        // 1 = upper nametable (single-screen B)
        if self.mirroring_bit {
            NametableLayout::SingleScreenUpper
        } else {
            NametableLayout::SingleScreenLower
        }
    }

    fn mapper_number(&self) -> u8 {
        7
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
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
            max_prg_ram_kb: if self.prg_ram.is_some() { 8 } else { 0 },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    fn create_axrom_mapper(prg_rom: Vec<u8>, mirroring: NametableLayout) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(7, prg_rom, vec![], mirroring))
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

        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

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

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

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
    fn test_axrom_16kb_prg_is_mirrored_to_32kb_window() {
        let mut prg_rom = vec![0; 16 * 1024];
        for (index, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (index & 0xFF) as u8;
        }

        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        assert_eq!(mapper.read_prg(0x8000), 0x00);
        assert_eq!(mapper.read_prg(0xBFFF), 0xFF);
        assert_eq!(mapper.read_prg(0xC000), 0x00);
        assert_eq!(mapper.read_prg(0xFFFF), 0xFF);
    }

    #[test]
    fn test_axrom_chr_ram() {
        // AxROM uses 8KB CHR-RAM (no CHR ROM)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x8000, 0x00); // Bits: 0000 0000
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Write with bit 4 = 0 but other bits set
        mapper.write_prg(0x8000, 0x07); // Bits: 0000 0111
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_upper() {
        // Bit 4 = 1 selects upper nametable (single-screen B)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x8000, 0x10); // Bits: 0001 0000
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
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

        let mut mapper = create_axrom_mapper(prg_rom.clone(), NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 0x07); // select bank 7
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        let registers = mapper.registers_snapshot();
        let chr_ram = mapper.chr_ram_snapshot();

        let mut restored = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);
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

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

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

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write to different addresses in PRG ROM space
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 11);

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 12);
    }

    #[test]
    fn test_axrom_has_no_prg_ram_when_disabled() {
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0),
        )
        .expect("Failed to create AxROM mapper without PRG-RAM");

        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x5A),
            0x5A,
            "AxROM should return open bus in $6000-$7FFF"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, 0xC3),
            0xC3,
            "AxROM should return open bus in $6000-$7FFF"
        );
        assert_eq!(mapper.wram_size(), 0, "AxROM should report no WRAM");
    }

    #[test]
    fn test_axrom_uses_prg_ram_when_present() {
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        )
        .expect("Failed to create AxROM mapper with PRG-RAM");

        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
        assert_eq!(mapper.wram_size(), 8 * 1024);
    }

    #[test]
    fn test_axrom_open_bus() {
        let prg_rom = vec![0; 128 * 1024];
        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xEE), 0xEE);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xFF), 0xFF);
    }

    #[test]
    fn test_axrom_submapper_2_applies_bus_conflicts_to_bank_select() {
        let mut prg_rom = vec![0; 64 * 1024];

        for byte in &mut prg_rom[0..32 * 1024] {
            *byte = 0x00;
        }
        for byte in &mut prg_rom[32 * 1024..64 * 1024] {
            *byte = 0x01;
        }

        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(2),
        )
        .expect("Failed to create AxROM mapper with submapper 2");

        mapper.write_prg(0x8000, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            0x00,
            "submapper 2 should apply bus conflicts and keep bank 0 selected"
        );
    }

    #[test]
    fn test_axrom_submapper_1_disables_bus_conflicts() {
        let mut prg_rom = vec![0; 64 * 1024];

        for byte in &mut prg_rom[0..32 * 1024] {
            *byte = 0x00;
        }
        for byte in &mut prg_rom[32 * 1024..64 * 1024] {
            *byte = 0x01;
        }

        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        )
        .expect("Failed to create AxROM mapper with submapper 1");

        mapper.write_prg(0x8000, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            0x01,
            "submapper 1 should not apply bus conflicts and should select bank 1"
        );
    }
}
