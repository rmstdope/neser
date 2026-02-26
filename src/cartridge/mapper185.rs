//! Mapper 185 - CNROM with CHR-ROM enable gating (chip select)
//!
//! This is a CNROM variant where CHR-ROM reads are gated by a chip-select
//! mechanism. Only when the low 2 bits of the bank-select write match the
//! NES 2.0 submapper value are CHR-ROM reads enabled; otherwise CHR reads
//! return PPU open bus (0).
//!
//! See: <https://www.nesdev.org/wiki/CNROM#Mapper_185>

use crate::cartridge::common::{BankSwitch, BankedRom, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::mapper::MapperContext;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 185 - CNROM variant with CHR-ROM chip select gating.
///
/// Specifications:
/// - PRG-ROM: 16 or 32KB fixed (no banking)
/// - PRG-RAM: None unless explicitly specified
/// - CHR-ROM: Up to 8KB (1 bank); reads gated by chip-select
/// - Mirroring: Fixed (from header)
/// - NES 2.0 submapper: encodes which low-2-bit value enables CHR reads
pub struct Mapper185 {
    prg_rom: Vec<u8>,
    prg_ram: Option<PrgRam>,
    chr_rom: BankedRom,
    chr_bank: BankSwitch,
    mirroring: NametableLayout,
    chr_enabled: bool,
    chr_enable_mask: u8, // low 2 bits that must match to enable CHR
}

impl Mapper185 {
    pub fn new(ctx: MapperContext) -> Self {
        let chr_bank_size = 8 * 1024;
        let chr_bank = BankSwitch::from_rom(&ctx.chr_rom, chr_bank_size);
        let prg_ram = if ctx.prg_ram_banks_8k > 0 && ctx.prg_ram_size_specified {
            Some(PrgRam::new(
                ctx.prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        } else {
            None
        };
        // Submapper encodes which low-2-bit value enables CHR-ROM.
        // Submapper 0 (unspecified) defaults to disabled (mask = 0xFF → never matches low 2 bits).
        let chr_enable_mask = if ctx.submapper > 0 {
            ctx.submapper & 0x03
        } else {
            0xFF // never matches normal writes
        };

        Self {
            prg_rom: ctx.prg_rom,
            prg_ram,
            chr_rom: BankedRom::new(ctx.chr_rom, chr_bank_size),
            chr_bank,
            mirroring: ctx.mirroring,
            chr_enabled: false,
            chr_enable_mask,
        }
    }
}

impl Mapper for Mapper185 {
    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(prg_ram) = &self.prg_ram
            && let Some(value) = prg_ram.try_read(addr)
        {
            return value;
        }
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize % self.prg_rom.len().max(1);
                self.prg_rom.get(offset).copied().unwrap_or(0)
            }
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
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank.set(value);
            self.chr_enabled = (value & 0x03) == self.chr_enable_mask;
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if !self.chr_enabled {
            return 0; // PPU open bus when CHR disabled
        }
        let offset = (addr & 0x1FFF) as usize;
        self.chr_rom.read(self.chr_bank.current(), offset)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CHR-ROM is read-only
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        185
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

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank.raw(), u8::from(self.chr_enabled)]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.chr_bank.set(bank);
        }
        if let Some(&enabled) = data.get(1) {
            self.chr_enabled = enabled != 0;
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
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

    fn chr_rom_with_banks(num_banks: usize) -> Vec<u8> {
        let mut chr = vec![0; num_banks * 8 * 1024];
        for bank in 0..num_banks {
            let start = bank * 8 * 1024;
            for byte in &mut chr[start..start + 8 * 1024] {
                *byte = (bank + 1) as u8 * 0x11; // bank 0 = 0x11, bank 1 = 0x22, etc.
            }
        }
        chr
    }

    #[test]
    fn test_mapper185_chr_disabled_until_correct_value_written() {
        // Before writing the enable value, CHR reads should return open bus (0).
        // Submapper 1 means low-2-bit value 0b01 enables CHR.
        let chr = chr_rom_with_banks(1);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                vec![0xFF; 32 * 1024],
                chr,
                NametableLayout::Horizontal,
            )
            .with_submapper(1)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        // Without any write, CHR should be disabled (returns 0)
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR reads must return 0 (open bus) when disabled"
        );
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    #[test]
    fn test_mapper185_chr_enabled_by_submapper_matching_write() {
        // Writing a value whose low 2 bits match the submapper enables CHR.
        // Submapper 1: low bits 0b01 (value 0x01, 0x05, 0x09 ...) enable CHR.
        let chr = chr_rom_with_banks(1);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                vec![0xFF; 32 * 1024],
                chr,
                NametableLayout::Horizontal,
            )
            .with_submapper(1)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        // Write enabling value (low 2 bits = submapper = 1)
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            0x11,
            "CHR must be readable after writing the enable value"
        );
    }

    #[test]
    fn test_mapper185_chr_disabled_by_non_matching_write() {
        // Writing a value whose low 2 bits do NOT match submapper disables CHR.
        let chr = chr_rom_with_banks(1);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                vec![0xFF; 32 * 1024],
                chr,
                NametableLayout::Horizontal,
            )
            .with_submapper(1)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        // Write with low bits 0b10 — does not match submapper 1
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR must return 0 when low bits don't match submapper"
        );
    }

    #[test]
    fn test_mapper185_submapper_2_uses_different_enable_mask() {
        // Submapper 2: low bits 0b10 enable CHR.
        let chr = chr_rom_with_banks(1);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                vec![0xFF; 32 * 1024],
                chr,
                NametableLayout::Horizontal,
            )
            .with_submapper(2)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        // Low bits 0b01 should NOT enable CHR for submapper 2
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "submapper 2: value 0x01 must not enable CHR"
        );

        // Low bits 0b10 should enable CHR for submapper 2
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            0x11,
            "submapper 2: value 0x02 must enable CHR"
        );
    }

    #[test]
    fn test_mapper185_prg_rom_fixed() {
        let mut prg_rom = vec![0; 32 * 1024];
        for (i, b) in prg_rom.iter_mut().enumerate() {
            *b = (i / 1024) as u8;
        }
        let mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                prg_rom,
                vec![0; 8 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(1)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 16);
        assert_eq!(mapper.read_prg(0xFFFF), 31);
    }

    #[test]
    fn test_mapper185_no_prg_ram_when_not_specified() {
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                185,
                vec![0xFF; 32 * 1024],
                vec![0; 8 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(1)
            .with_prg_ram_banks(0),
        )
        .expect("mapper 185 must be supported");

        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x55),
            0x55,
            "No PRG-RAM: must return open bus"
        );
        assert_eq!(mapper.wram_size(), 0);
    }
}
