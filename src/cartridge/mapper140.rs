//! Mapper 140 - Jaleco JF-11/JF-14
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::common::{BankSwitch, BankedRom, ChrMemory};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const PRG_BANK_SIZE: usize = 0x8000; // 32KB
const CHR_BANK_SIZE: usize = 0x2000; // 8KB

/// Mapper 140 - Jaleco JF-11/JF-14
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_140>
/// - GNROM-like register format: `[..PP CCCC]`
/// - Register write port: `$6000-$7FFF`
/// - PRG: 32KB bank at `$8000-$FFFF` selected by bits 4-5
/// - CHR: 8KB bank at `$0000-$1FFF` selected by bits 0-3
/// - No PRG-RAM (registers occupy `$6000-$7FFF`)
/// - Mirroring: fixed from iNES header
pub struct Mapper140 {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    prg_bank: BankSwitch,
    chr_bank: BankSwitch,
    register: u8,
}

impl Mapper140 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;

        Self {
            prg_bank: BankSwitch::from_rom(&prg_rom, PRG_BANK_SIZE),
            chr_bank: BankSwitch::from_rom(&chr_rom, CHR_BANK_SIZE),
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring: ctx.mirroring,
            register: 0,
        }
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.prg_bank.set((value >> 4) & 0b11);
        self.chr_bank.set(value & 0b1111);
    }
}

impl Mapper for Mapper140 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank.current(), 0x8000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.apply_register(value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let index = self.chr_bank.offset(CHR_BANK_SIZE) + (addr as usize & 0x1FFF);
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let index = self.chr_bank.offset(CHR_BANK_SIZE) + (addr as usize & 0x1FFF);
        self.chr_memory.write_at_index(index, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        140
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_wram_snapshot(&mut self, _data: &[u8]) {}

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper140(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            140,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn test_mapper140_register_at_6000_7fff_selects_prg_and_chr() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 16);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x6000, 0x12); // PRG=1, CHR=2
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.write_prg(0x7FFF, 0x3F); // PRG=3, CHR=15
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_chr(0x0000), 15);
    }

    #[test]
    fn test_mapper140_ignores_writes_outside_6000_7fff() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x8000, 0x31);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn test_mapper140_has_no_prg_ram() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0);
        assert_eq!(mapper.wram_size(), 0);
        assert!(mapper.wram_snapshot().is_empty());
    }
}
