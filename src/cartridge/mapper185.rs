//! Mapper 185 - CNROM with CHR-ROM enable gating (chip select)
//!
//! This is a CNROM variant where CHR-ROM reads are gated by a chip-select
//! mechanism. Only when the low 2 bits of the bank-select write match the
//! NES 2.0 submapper value are CHR-ROM reads enabled; otherwise CHR reads
//! return PPU open bus (0).
//!
//! See: <https://www.nesdev.org/wiki/CNROM#Mapper_185>

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::MapperContext;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 185 - CNROM variant with CHR-ROM chip select gating.
///
/// Specifications:
/// - PRG-ROM: 16 or 32KB fixed (no banking)
/// - PRG-RAM: None unless explicitly specified
/// - CHR-ROM: Up to 8KB (1 bank); reads gated by chip-select
/// - Mirroring: Fixed (from header)
/// - NES 2.0 submapper: encodes which low-2-bit value enables CHR reads
pub struct Mapper185 {
    base: BaseMapper,
    register: u8,
    chr_enabled: bool,
    chr_enable_mask: u8, // low 2 bits that must match to enable CHR
}

impl Mapper185 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        // Submapper encodes which low-2-bit value enables CHR-ROM.
        // Submapper 0 (unspecified) defaults to disabled (mask = 0xFF → never matches low 2 bits).
        let chr_enable_mask = if ctx.submapper > 0 {
            ctx.submapper & 0x03
        } else {
            0xFF // never matches normal writes
        };
        let base = BaseMapper::new(&ctx, capabilities);

        Self {
            base,
            register: 0,
            chr_enabled: false,
            chr_enable_mask,
        }
    }
}

impl Mapper for Mapper185 {
    fn base(&self) -> Option<&BaseMapper> {
        Some(&self.base)
    }

    fn base_mut(&mut self) -> Option<&mut BaseMapper> {
        Some(&mut self.base)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_rom_fixed(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.register = value;
            self.chr_enabled = (value & 0x03) == self.chr_enable_mask;
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if !self.chr_enabled {
            return 0; // PPU open bus when CHR disabled
        }
        self.base.read_chr(addr)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CHR-ROM is read-only
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register, u8::from(self.chr_enabled)]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.register = bank;
        }
        if let Some(&enabled) = data.get(1) {
            self.chr_enabled = enabled != 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
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
