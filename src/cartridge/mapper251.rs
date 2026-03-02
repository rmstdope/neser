//! Mapper 251 - Alias of Mapper 45 (Nestopia assignment)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_251>
//! - Mapper behavior: same as mapper 45 outer-banking MMC3 multicart
//!
//! Notes:
//! - iNES mapper 251 is treated as mapper 45 compatibility behavior.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper45::Mapper45;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 251 implemented as a thin wrapper around Mapper 45 behavior.
pub struct Mapper251 {
    pub(crate) inner: Mapper45,
}

impl Mapper251 {
    const MAPPER_NUMBER: u8 = 251;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: Mapper45::new_internal(prg_rom, chr_rom, mirroring),
        }
    }
}

impl Mapper for Mapper251 {
    fn base(&self) -> &BaseMapper {
        &self.inner.mmc3.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.mmc3.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.inner.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.inner.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.inner.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.inner.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.inner.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.inner.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.inner.restore_registers(data);
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.inner.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper251(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            251, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_251() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
        assert_eq!(mapper.unwrap().mapper_number(), 251);
    }

    #[test]
    fn test_power_on_vectors_come_from_last_prg_bank_like_mapper45() {
        let mut prg_rom = vec![0u8; 8 * 1024 * 8];
        let chr_rom = banked_data(1024, 8);

        let bank0_base = 0usize;
        prg_rom[bank0_base + 0x1FFA] = 0x14;
        prg_rom[bank0_base + 0x1FFB] = 0x00;
        prg_rom[bank0_base + 0x1FFC] = 0x15;
        prg_rom[bank0_base + 0x1FFD] = 0x00;
        prg_rom[bank0_base + 0x1FFE] = 0x16;
        prg_rom[bank0_base + 0x1FFF] = 0x00;

        let last_bank_base = 7 * 8 * 1024;
        prg_rom[last_bank_base + 0x1FFA] = 0xCA;
        prg_rom[last_bank_base + 0x1FFB] = 0xFE;
        prg_rom[last_bank_base + 0x1FFC] = 0x05;
        prg_rom[last_bank_base + 0x1FFD] = 0xFE;
        prg_rom[last_bank_base + 0x1FFE] = 0x58;
        prg_rom[last_bank_base + 0x1FFF] = 0xE4;

        let mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.read_prg(0xFFFA), 0xCA);
        assert_eq!(mapper.read_prg(0xFFFB), 0xFE);
        assert_eq!(mapper.read_prg(0xFFFC), 0x05);
        assert_eq!(mapper.read_prg(0xFFFD), 0xFE);
        assert_eq!(mapper.read_prg(0xFFFE), 0x58);
        assert_eq!(mapper.read_prg(0xFFFF), 0xE4);
    }
}
