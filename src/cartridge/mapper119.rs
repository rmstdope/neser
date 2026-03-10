//! Mapper 119 - TQROM (MMC3 with mixed CHR-ROM/CHR-RAM banking)

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper119 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; 8 * 1024],
}

impl Mapper119 {
    const MAPPER_NUMBER: u16 = 119;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const CHR_RAM_SELECT_BIT: usize = 0x40;
    const CHR_ROM_BANK_MASK: usize = !Self::CHR_RAM_SELECT_BIT;
    const CHR_RAM_BANK_MASK: usize = 0x07;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_ram: [0; 8 * 1024],
        }
    }
}

impl Mapper for Mapper119 {
    fn base(&self) -> &BaseMapper {
        &self.inner.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
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

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if (raw_bank & Self::CHR_RAM_SELECT_BIT) != 0 {
            let ram_bank = raw_bank & Self::CHR_RAM_BANK_MASK;
            self.chr_ram[ram_bank * Self::CHR_1K_BANK_SIZE + offset]
        } else {
            let rom_bank = raw_bank & Self::CHR_ROM_BANK_MASK;
            self.inner.read_chr_1k_at(rom_bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        if (raw_bank & Self::CHR_RAM_SELECT_BIT) != 0 {
            let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
            let ram_bank = raw_bank & Self::CHR_RAM_BANK_MASK;
            self.chr_ram[ram_bank * Self::CHR_1K_BANK_SIZE + offset] = value;
        }
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.extend_from_slice(&self.chr_ram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const CHR_RAM_SIZE: usize = 8 * 1024;
        if data.len() >= CHR_RAM_SIZE {
            let (mmc3_data, chr_ram_data) = data.split_at(data.len() - CHR_RAM_SIZE);
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(chr_ram_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper119;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 6;
    const CHR_ROM_1K_BANKS: usize = 13;

    fn make_mapper_direct() -> Mapper119 {
        Mapper119::new(MapperContext::new_for_test(
            119,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_119_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            119,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 119 must be registered in the factory");
    }

    #[test]
    fn chr_rom_bank_selected_when_bit6_clear() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x1000), 5);
    }

    #[test]
    fn chr_ram_bank_selected_when_bit6_set() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 0x40 | 3);
        mapper.write_chr(0x1000, 0xAB);
        assert_eq!(mapper.read_chr(0x1000), 0xAB);
    }

    #[test]
    fn bank_type_switching_preserves_rom_and_ram_views() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0b0000_0010);

        mapper.write_prg(0x8001, 0x40 | 3);
        mapper.write_chr(0x1000, 0x6E);

        mapper.write_prg(0x8001, 3);
        assert_eq!(mapper.read_chr(0x1000), 3);

        mapper.write_prg(0x8001, 0x40 | 3);
        assert_eq!(mapper.read_chr(0x1000), 0x6E);
    }

    #[test]
    fn mmc3_irq_works_through_delegation() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending());
    }

    #[test]
    fn mmc3_mirroring_works_through_delegation() {
        let mut mapper = make_mapper_direct();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }
}
