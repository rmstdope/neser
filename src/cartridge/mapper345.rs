//! Mapper 345 - BMC-L6IN1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_345>
//! - Reference behavior: MAME `nes_bmc_l6in1_device` (`mmc3_clones.cpp`)

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 345 - BMC-L6IN1 multicart.
///
/// Hardware mode register at `$6000-$7FFF` (`m_reg`):
/// - bits 7:6: outer PRG base (selects one of four 128KB regions)
/// - bits 3:2: mode selector (`!= 0` => MMC3 mode, `== 0` => AxROM mode)
/// - bits 1:0: AxROM 32KB bank within selected outer region
/// - bit 5: one-screen mirroring enable
/// - bit 4: one-screen target (`0=lower`, `1=upper`) when bit 5 is set
pub struct Mapper345 {
    mmc3: MMC3Mapper,
    mode_reg: u8,
    mmc3_mirroring_horizontal: bool,
    power_on_mirroring_horizontal: bool,
}

impl Mapper345 {
    const PRG_8K_BANK_SIZE: usize = 0x2000;
    const PRG_8K_BANK_MASK: usize = Self::PRG_8K_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_1K_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_seed = ctx.chr_rom;
        let mirroring = ctx.mirroring;

        let mut mmc3 = MMC3Mapper::new_with_irq_mode(prg_rom, vec![], mirroring, false);

        if !chr_seed.is_empty() {
            let mut chr_ram = ChrMemory::new_ram(8 * 1024);
            chr_ram.load_snapshot(&chr_seed);
            mmc3.base.set_chr_memory(chr_ram);
        }

        let mut mapper = Self {
            mmc3,
            mode_reg: 0,
            mmc3_mirroring_horizontal: matches!(mirroring, NametableLayout::Horizontal),
            power_on_mirroring_horizontal: matches!(mirroring, NametableLayout::Horizontal),
        };
        mapper.apply_mirroring();
        mapper
    }

    fn in_mmc3_mode(&self) -> bool {
        (self.mode_reg & 0x0C) != 0
    }

    fn outer_prg_base_8k(&self) -> usize {
        ((self.mode_reg & 0xC0) >> 2) as usize
    }

    fn axrom_32k_bank(&self) -> usize {
        (self.outer_prg_base_8k() >> 2) | (self.mode_reg as usize & 0x03)
    }

    fn apply_mirroring(&mut self) {
        if (self.mode_reg & 0x20) != 0 {
            let mirroring = if (self.mode_reg & 0x10) != 0 {
                NametableLayout::SingleScreenUpper
            } else {
                NametableLayout::SingleScreenLower
            };
            self.mmc3.base.set_mirroring(mirroring);
        } else {
            self.mmc3
                .base
                .set_mirroring_hv(self.mmc3_mirroring_horizontal);
        }
    }

    fn mapped_prg_8k_bank(&self, addr: u16) -> usize {
        if self.in_mmc3_mode() {
            self.outer_prg_base_8k() | (self.mmc3.mapped_prg_bank(addr) & 0x0F)
        } else {
            let slot = ((addr as usize).saturating_sub(0x8000) >> 13) & 0x03;
            self.axrom_32k_bank() * 4 + slot
        }
    }

    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        self.mmc3.mapped_chr_1k_bank(addr) & 0x07
    }
}

impl Mapper for Mapper345 {
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
        345
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return self.mmc3.read_prg(addr);
        }

        let bank = self.mapped_prg_8k_bank(addr);
        let offset = (addr as usize) & Self::PRG_8K_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.mmc3.is_prg_ram_writable() {
                self.mode_reg = value;
                self.apply_mirroring();
            }
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
            if (addr & 0x6001) == 0x2000 {
                self.mmc3_mirroring_horizontal = (value & 0x01) != 0;
                self.apply_mirroring();
            }
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
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
        snapshot.push(self.mode_reg);
        snapshot.push(self.mmc3_mirroring_horizontal as u8);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        let mmc3_len = data.len() - 2;
        self.mmc3.restore_registers(&data[..mmc3_len]);
        self.mode_reg = data[mmc3_len];
        self.mmc3_mirroring_horizontal = data[mmc3_len + 1] != 0;
        self.apply_mirroring();
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.mode_reg = 0;
        self.mmc3_mirroring_horizontal = self.power_on_mirroring_horizontal;
        self.apply_mirroring();
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 64;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            345,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 345 should be implemented")
    }

    #[test]
    fn mapper_345_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            345,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 345 must be registered in the factory"
        );
    }

    #[test]
    fn power_on_uses_axrom_mode_bank_0() {
        let mapper = make_mapper();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), 3);
    }

    #[test]
    fn write_6000_selects_axrom_32k_bank_with_outer_bits() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 12);
        assert_eq!(mapper.read_prg(0xE000), 15);

        mapper.write_prg(0x6000, 0xC2);
        assert_eq!(mapper.read_prg(0x8000), 56);
        assert_eq!(mapper.read_prg(0xE000), 59);
    }

    #[test]
    fn mmc3_mode_uses_inner_banking_with_outer_base() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x0C);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3);

        mapper.write_prg(0x6000, 0x4C);
        assert_eq!(mapper.read_prg(0x8000), 19);
    }

    #[test]
    fn bit5_forces_one_screen_mirroring_and_bit4_selects_nt_page() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x6000, 0x20);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0x6000, 0x30);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn chr_is_8k_mmc3_banked_chr_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x03);
        mapper.write_chr(0x1000, 0xAA);

        mapper.write_prg(0x8001, 0x04);
        mapper.write_chr(0x1000, 0xBB);

        mapper.write_prg(0x8001, 0x03);
        assert_eq!(mapper.read_chr(0x1000), 0xAA);

        mapper.write_prg(0x8001, 0x04);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_mode_and_banking() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x4C);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x03);
        mapper.write_prg(0xA000, 0x01);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }
}
