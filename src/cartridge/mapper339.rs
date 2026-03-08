//! Mapper 339 - BMC-K-3006 (MMC3+NROM multicart)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_339>
//!
//! Known Limitations:
//! - `P` solder-pad controlled PRG A1..A0 menu selection mode is not emulated.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 339;
const PRG_RAM_START: u16 = 0x6000;
const PRG_ROM_START: u16 = 0x8000;
const PRG_ROM_END: u16 = 0xFFFF;
const PRG_8K_OFFSET_MASK: usize = 0x1FFF;
const CHR_1K_OFFSET_MASK: usize = 0x03FF;
const OUTER_MODE_MMC3_BIT: u8 = 0x20;
const OUTER_CC_MASK: u8 = 0x18;
const OUTER_BBA_MASK: u8 = 0x07;
const OUTER_BB_MASK: u8 = 0x06;
const OUTER_A_BIT: u8 = 0x01;
const OUTER_REGISTER_MASK: u8 = 0xBF;

pub struct Mapper339 {
    mmc3: MMC3Mapper,
    outer_reg: u8,
    nrom_256_mode: bool,
    submapper: u8,
}

impl Mapper339 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            outer_reg: 0,
            nrom_256_mode: false,
            submapper: ctx.submapper,
        }
    }

    fn is_prg_rom_addr(addr: u16) -> bool {
        (PRG_ROM_START..=PRG_ROM_END).contains(&addr)
    }

    fn is_outer_register_addr(addr: u16) -> bool {
        (addr & 0xE000) == PRG_RAM_START
    }

    fn decode_outer_register(addr: u16) -> u8 {
        (addr as u8) & OUTER_REGISTER_MASK
    }

    fn is_mmc3_prg_mode(&self) -> bool {
        (self.outer_reg & OUTER_MODE_MMC3_BIT) != 0
    }

    fn outer_cc(&self) -> usize {
        ((self.outer_reg & OUTER_CC_MASK) >> 3) as usize
    }

    fn nrom_16k_bank(&self, addr: u16) -> usize {
        let cc_base = self.outer_cc() << 3;
        if self.nrom_256_mode {
            let bb = (self.outer_reg & OUTER_BB_MASK) as usize;
            let cpu_a14 = ((addr as usize) >> 14) & 0x01;
            cc_base | bb | cpu_a14
        } else {
            cc_base | ((self.outer_reg & OUTER_BBA_MASK) as usize)
        }
    }

    fn read_prg_16k_bank(&self, bank_16k: usize, addr: u16) -> u8 {
        let page_in_16k = ((addr as usize) >> 13) & 0x01;
        let bank_8k = bank_16k * 2 + page_in_16k;
        let offset = (addr as usize) & PRG_8K_OFFSET_MASK;
        self.mmc3.read_prg_at_bank(bank_8k, offset)
    }

    fn read_prg_nrom_mode(&self, addr: u16) -> u8 {
        let bank_16k = self.nrom_16k_bank(addr);
        self.read_prg_16k_bank(bank_16k, addr)
    }

    fn read_prg_mmc3_mode(&self, addr: u16) -> u8 {
        let inner = (self.mmc3.raw_prg_8k_page_number(addr) as usize) & 0x07;
        let bank = (self.outer_cc() << 3) | inner;
        let offset = (addr as usize) & PRG_8K_OFFSET_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        let inner = self.mmc3.raw_chr_1k_bank(addr) & 0x7F;
        (self.outer_cc() << 7) | inner
    }

    fn read_prg_rom_space(&self, addr: u16) -> u8 {
        if self.is_mmc3_prg_mode() {
            self.read_prg_mmc3_mode(addr)
        } else {
            self.read_prg_nrom_mode(addr)
        }
    }

    fn update_outer_register(&mut self, addr: u16) {
        self.outer_reg = Self::decode_outer_register(addr);
        self.nrom_256_mode = Self::decode_nrom_256_mode(self.submapper, addr);
    }

    fn decode_nrom_256_mode(submapper: u8, addr: u16) -> bool {
        match submapper {
            0 => (addr & 0x06) == 0x06,
            1 => (addr & 0x04) != 0,
            2 => (addr & 0x11) != 0,
            3 => (addr & 0x18) != 0,
            4 => (addr & 0x14) != 0,
            _ => false,
        }
    }
}

impl Mapper for Mapper339 {
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
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !Self::is_prg_rom_addr(addr) {
            return self.mmc3.read_prg(addr);
        }

        self.read_prg_rom_space(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_outer_register_addr(addr)
            && self.mmc3.is_prg_ram_writable()
            && !self.is_mmc3_prg_mode()
        {
            self.update_outer_register(addr);
        }

        self.mmc3.write_prg(addr, value);
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if !Self::is_prg_rom_addr(addr) {
            return self.mmc3.read_prg_open_bus(addr, open_bus);
        }

        self.read_prg_rom_space(addr)
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & CHR_1K_OFFSET_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & CHR_1K_OFFSET_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = self.mmc3.registers_snapshot();
        snapshot.push(self.outer_reg);
        snapshot.push(self.nrom_256_mode as u8);
        snapshot
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

    fn restore_registers(&mut self, data: &[u8]) {
        let Some((&nrom_256_mode, rest)) = data.split_last() else {
            return;
        };
        let Some((&outer_reg, mmc3_data)) = rest.split_last() else {
            return;
        };

        self.mmc3.restore_registers(mmc3_data);
        self.outer_reg = outer_reg & OUTER_REGISTER_MASK;
        self.nrom_256_mode = (nrom_256_mode & OUTER_A_BIT) != 0;
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer_reg = 0;
        self.nrom_256_mode = false;
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.mmc3.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 96;
    const CHR_BANKS_1K: usize = 512;

    fn make_mapper(submapper: u8) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(8 * 1024, PRG_BANKS_8K),
                banked_data(1024, CHR_BANKS_1K),
                NametableLayout::Vertical,
            )
            .with_submapper(submapper),
        )
        .expect("Mapper 339 should be created")
    }

    #[test]
    fn mapper_339_is_registered() {
        let mapper = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(8 * 1024, PRG_BANKS_8K),
                banked_data(1024, CHR_BANKS_1K),
                NametableLayout::Vertical,
            )
            .with_submapper(0),
        );
        assert!(mapper.is_ok(), "Mapper 339 should be registered in factory");
    }

    #[test]
    fn nrom_128_mode_uses_bba_16k_bank_and_mirrors_both_windows() {
        let mut mapper = make_mapper(0);

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6005, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xA000), 11);
        assert_eq!(mapper.read_prg(0xC000), 10);
        assert_eq!(mapper.read_prg(0xE000), 11);
    }

    #[test]
    fn submapper_0_selects_nrom_256_when_addr_and_06_equals_06() {
        let mut mapper = make_mapper(0);

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6006, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 12);
        assert_eq!(mapper.read_prg(0xC000), 14);
    }

    #[test]
    fn mmc3_prg_mode_uses_inner_banking_and_locks_outer_register() {
        let mut mapper = make_mapper(0);

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6008, 0x00);
        mapper.write_prg(0x6028, 0x00);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x01);

        assert_eq!(mapper.read_prg(0x8000), 9);

        mapper.write_prg(0x6018, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 9);
    }

    #[test]
    fn capabilities_report_irq_and_no_expansion_audio() {
        let mapper = make_mapper(0);
        let caps = mapper.capabilities();

        assert!(caps.has_irq);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert!(!caps.has_expansion_audio);
    }
}
