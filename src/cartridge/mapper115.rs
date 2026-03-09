//! Mapper 115 - KS202 / MMC3 variant
//!
//! Specifications:
//! - Mesen reference: `MMC3_115`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper115 {
    mmc3: MMC3Mapper,
    prg_reg: u8,
    chr_reg: u8,
    protection_reg: u8,
}

impl Mapper115 {
    const MAPPER_NUMBER: u8 = 115;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            prg_reg: 0,
            chr_reg: 0,
            protection_reg: 0,
        }
    }

    fn prg_slot(addr: u16) -> usize {
        ((addr as usize).saturating_sub(0x8000) >> 13) & 0x03
    }

    fn mapped_prg_bank(&self, addr: u16) -> usize {
        if (self.prg_reg & 0x80) == 0 {
            return self.mmc3.mapped_prg_bank(addr);
        }

        let low = (self.prg_reg & 0x0F) as usize;
        let slot = Self::prg_slot(addr);
        if (self.prg_reg & 0x20) != 0 {
            ((low >> 1) << 2) | slot
        } else {
            (low << 1) | (slot & 0x01)
        }
    }

    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        self.mmc3.mapped_chr_1k_bank(addr) | ((self.chr_reg as usize) << 8)
    }
}

impl Mapper for Mapper115 {
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

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x5FFF => self.protection_reg,
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.mapped_prg_bank(addr);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5000..=0x5FFF => self.protection_reg,
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x4100..=0x7FFF).contains(&addr) {
            if addr == 0x5080 {
                self.protection_reg = value;
            } else if (addr & 0x01) != 0 {
                self.chr_reg = value & 0x01;
            } else {
                self.prg_reg = value;
            }
        } else if (0x8000..=0xFFFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
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
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.prg_reg);
        snap.push(self.chr_reg);
        snap.push(self.protection_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            let (mmc3_data, tail) = data.split_at(data.len() - 3);
            self.prg_reg = tail[0];
            self.chr_reg = tail[1] & 0x01;
            self.protection_reg = tail[2];
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.prg_reg = 0;
        self.chr_reg = 0;
        self.protection_reg = 0;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
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

    const PRG_BANKS_8K: usize = 64;
    const CHR_BANKS_1K: usize = 320;

    fn chr_data_with_high_bank_marker() -> Vec<u8> {
        let mut chr = vec![0u8; CHR_BANKS_1K * 1024];
        for bank in 0..CHR_BANKS_1K {
            let base = bank * 1024;
            chr[base] = bank as u8;
            chr[base + 1] = ((bank >> 8) & 0x01) as u8;
        }
        chr
    }

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            115,
            banked_data(8 * 1024, PRG_BANKS_8K),
            chr_data_with_high_bank_marker(),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 115 should be implemented")
    }

    #[test]
    fn mapper_115_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            115,
            banked_data(8 * 1024, PRG_BANKS_8K),
            chr_data_with_high_bank_marker(),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 115 must be registered in the mapper factory"
        );
    }

    #[test]
    fn outer_prg_reg_bit7_clear_uses_inner_mmc3_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);

        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "With bit7 clear, mapper 115 PRG should follow MMC3 R6 mapping"
        );
    }

    #[test]
    fn outer_prg_reg_16k_mode_mirrors_low_and_high_halves() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x83);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 7);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn outer_prg_reg_32k_mode_selects_contiguous_four_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0xAB);

        assert_eq!(mapper.read_prg(0x8000), 20);
        assert_eq!(mapper.read_prg(0xA000), 21);
        assert_eq!(mapper.read_prg(0xC000), 22);
        assert_eq!(mapper.read_prg(0xE000), 23);
    }

    #[test]
    fn chr_outer_bit_extends_mmc3_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x1001), 0);

        mapper.write_prg(0x4101, 0x01);
        assert_eq!(mapper.read_chr(0x1001), 1);
    }

    #[test]
    fn mirroring_control_is_delegated_to_mmc3() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn irq_signalling_is_delegated_to_mmc3() {
        let mut mapper = make_mapper();
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

        assert!(mapper.irq_pending(), "Mapper 115 should raise MMC3 IRQs");
    }

    #[test]
    fn registers_snapshot_roundtrip_preserves_outer_regs() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0xAB);
        mapper.write_prg(0x4101, 0x01);
        mapper.write_prg(0x5080, 0x5A);
        mapper.write_prg(0xA000, 1);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 20);
        assert_eq!(restored.read_prg(0xA000), 21);
        assert_eq!(restored.read_prg(0xC000), 22);
        assert_eq!(restored.read_prg(0xE000), 23);
        assert_eq!(restored.read_chr(0x1001), 1);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x5000), 0x5A);
    }
}
