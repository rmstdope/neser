//! Mapper 118 - TxSROM (MMC3 variant with CHR-bit7 CIRAM selection)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_118>
//! - Base behavior: <https://www.nesdev.org/wiki/MMC3>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper118 {
    mmc3: MMC3Mapper,
    ciram: [u8; Self::CIRAM_SIZE],
}

impl Mapper118 {
    const MAPPER_NUMBER: u16 = 118;
    const CIRAM_SIZE: usize = 0x0800;
    const NAMETABLE_PAGE_SIZE: usize = 0x0400;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            ciram: [0; Self::CIRAM_SIZE],
        }
    }

    fn ciram_page_for_addr(&self, addr: u16) -> usize {
        let raw_chr_bank = self.mmc3.raw_chr_1k_bank(addr & 0x1FFF);
        (raw_chr_bank >> 7) & 0x01
    }
}

impl Mapper for Mapper118 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.mmc3.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.mmc3.write_prg(addr, value);
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.mmc3.read_prg_open_bus(addr, open_bus)
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }

        let page = self.ciram_page_for_addr(addr);
        let offset = (addr as usize) & (Self::NAMETABLE_PAGE_SIZE - 1);
        Some(self.ciram[page * Self::NAMETABLE_PAGE_SIZE + offset])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }

        let page = self.ciram_page_for_addr(addr);
        let offset = (addr as usize) & (Self::NAMETABLE_PAGE_SIZE - 1);
        self.ciram[page * Self::NAMETABLE_PAGE_SIZE + offset] = value;
        true
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = self.mmc3.registers_snapshot();
        snapshot.extend_from_slice(&self.ciram);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < Self::CIRAM_SIZE {
            return;
        }
        let (mmc3_data, ciram_data) = data.split_at(data.len() - Self::CIRAM_SIZE);
        self.mmc3.restore_registers(mmc3_data);
        self.ciram.copy_from_slice(ciram_data);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.mmc3.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::mapper::{create_mapper, Mapper, MapperContext};
    use crate::cartridge::test_helpers::banked_data;
    use crate::cartridge::NametableLayout;

    const PRG_BANKS_8K: usize = 64;
    const CHR_BANKS_1K: usize = 256;

    fn make_mapper(mapper_id: u16) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            mapper_id,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("mapper should be implemented")
    }

    #[test]
    fn mapper_118_is_registered_in_factory() {
        let _mapper = make_mapper(118);
    }

    #[test]
    fn mapper_118_prg_chr_and_irq_match_mmc3() {
        let mut txsrom = make_mapper(118);
        let mut mmc3 = make_mapper(4);

        for mapper in [&mut txsrom, &mut mmc3] {
            mapper.write_prg(0x8000, 0x06);
            mapper.write_prg(0x8001, 0x09);
            mapper.write_prg(0x8000, 0x07);
            mapper.write_prg(0x8001, 0x0B);
            mapper.write_prg(0x8000, 0x80);
            mapper.write_prg(0x8001, 0x14);
            mapper.write_prg(0x8000, 0x02);
            mapper.write_prg(0x8001, 0x21);
        }

        for addr in [0x8000, 0xA000, 0xC000, 0xE000] {
            assert_eq!(txsrom.read_prg(addr), mmc3.read_prg(addr));
        }
        for addr in [0x0000, 0x0400, 0x0800, 0x1000, 0x1400, 0x1C00] {
            assert_eq!(txsrom.read_chr(addr), mmc3.read_chr(addr));
        }

        for mapper in [&mut txsrom, &mut mmc3] {
            mapper.write_prg(0xC000, 0x01);
            mapper.write_prg(0xC001, 0x00);
            mapper.write_prg(0xE001, 0x00);
        }
        for _ in 0..2 {
            txsrom.ppu_address_changed(0x0FFF);
            mmc3.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                txsrom.cpu_cycle();
                mmc3.cpu_cycle();
            }
            txsrom.ppu_address_changed(0x1000);
            mmc3.ppu_address_changed(0x1000);
        }
        assert_eq!(txsrom.irq_pending(), mmc3.irq_pending());
    }

    #[test]
    fn mapper_118_mirroring_comes_from_chr_bank_bit7_not_a000() {
        let mut mapper = make_mapper(118);

        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        assert!(mapper.write_nametable(0x2000, 0x11));

        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x80);
        assert!(mapper.write_nametable(0x2000, 0xAA));
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));

        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));

        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x11));
    }

    #[test]
    fn mapper_118_snapshot_restore_preserves_bank_and_mirroring_state() {
        let mut mapper = make_mapper(118);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x19);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x80);
        assert!(mapper.write_nametable(0x2000, 0xCC));
        mapper.write_prg(0x8001, 0x00);
        assert!(mapper.write_nametable(0x2000, 0x33));
        mapper.write_prg(0x8001, 0x80);

        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper(118);
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.read_nametable(0x2000), Some(0xCC));
        restored.write_prg(0x8000, 0x00);
        restored.write_prg(0x8001, 0x00);
        assert_eq!(restored.read_nametable(0x2000), Some(0x33));
    }
}
