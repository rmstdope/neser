//! Mapper 205 - MMC3-based multicart (BMC-JC-016-2)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_205>

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper205 {
    pub(crate) inner: MMC3Mapper,
    selected_block: u8,
}

impl Mapper205 {
    const MAPPER_NUMBER: u8 = 205;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_1K_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        Self {
            inner: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            selected_block: 0,
        }
    }

    fn adjust_prg_bank(&self, bank: usize) -> usize {
        let block = (self.selected_block & 0x03) as usize;
        let and_mask = if block <= 1 { 0x1F } else { 0x0F };
        (bank & and_mask) | (block << 4)
    }

    fn adjust_chr_1k_bank(&self, bank: usize) -> usize {
        let block = self.selected_block & 0x03;
        let mut adjusted = bank;
        if block >= 2 {
            adjusted &= 0x7F;
            adjusted |= 0x100;
        }
        if block == 1 || block == 3 {
            adjusted |= 0x80;
        }
        adjusted
    }
}

impl Mapper for Mapper205 {
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let raw_bank = self.inner.mapped_prg_bank(addr);
        let final_bank = self.adjust_prg_bank(raw_bank);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.inner.read_prg_at_bank(final_bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.selected_block = value & 0x03;
        } else {
            self.inner.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw_bank = self.inner.mapped_chr_1k_bank(addr);
        let final_bank = self.adjust_chr_1k_bank(raw_bank);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.inner.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw_bank = self.inner.mapped_chr_1k_bank(addr);
        let final_bank = self.adjust_chr_1k_bank(raw_bank);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.inner.write_chr_1k_at(final_bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.push(self.selected_block);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&selected_block, mmc3_data)) = data.split_last() {
            self.selected_block = selected_block & 0x03;
            self.inner.restore_registers(mmc3_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 64;
    const CHR_1K_BANKS: usize = 512;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            205,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 205 should be implemented")
    }

    #[test]
    fn mapper_205_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            205,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok(), "Mapper 205 must be registered in factory");
    }

    #[test]
    fn power_on_block_zero_keeps_first_128k_window() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xE000), 31);
    }

    #[test]
    fn block_one_applies_prg_and_chr_or_bits() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);

        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x1000), 5);

        mapper.write_prg(0x6000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 19);
        assert_eq!(mapper.read_chr(0x1000), 133);
    }

    #[test]
    fn block_two_masks_prg_and_sets_chr_a8() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 0x1F);
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 47);

        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 0x80);
        assert_eq!(mapper.read_chr(0x1000), 0);
    }

    #[test]
    fn reads_from_6000_return_zero() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x6000), 0);
    }

    #[test]
    fn snapshot_and_restore_preserve_selected_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0xE000), mapper.read_prg(0xE000));
        assert_eq!(restored.read_chr(0x1000), mapper.read_chr(0x1000));
    }
}
