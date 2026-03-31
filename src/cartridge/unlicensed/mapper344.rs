//! Mapper 344 - BMC-GN-26 (MMC3-based multicart)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_344>
//!
//! Known Limitations:
//! - Solder-pad test behavior on `S=1` (possible PRG-ROM disable on some boards)
//!   is not emulated.
//! - The common dump PRG 128KB bank reorder quirk (`0,3,1,2`) is not auto-corrected.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::mmc3::MMC3Mapper;

pub struct Mapper344 {
    mmc3: MMC3Mapper,
    outer_reg: u8,
}

impl Mapper344 {
    const PRG_RAM_START: u16 = 0x6000;
    const PRG_RAM_END: u16 = 0x7FFF;
    const PRG_ROM_START: u16 = 0x8000;
    const PRG_ROM_END: u16 = 0xFFFF;

    const OUTER_REG_PRG_BANK_MASK: u8 = 0x03;
    const OUTER_REG_NROM_MODE_BIT: u8 = 0x08;
    const OUTER_REG_CHR_A17_SOURCE_BIT: u8 = 0x04;
    const PRG_INNER_BANK_MASK: usize = 0x0F;
    const OUTER_PRG_BANK_SHIFT: usize = 4;
    const NROM_BASE_BANK_MASK: usize = 0x0E;
    const CPU_A13_SHIFT: usize = 13;
    const CPU_A13_MASK: usize = 0x01;
    const PRG_BANK_OFFSET_MASK: usize = 0x1FFF;
    const CHR_BANK_OFFSET_MASK: usize = 0x03FF;
    const CHR_INNER_BANK_LOW_BITS_MASK: usize = 0x7F;
    const CHR_INNER_BANK_A17_MASK: usize = 0x80;
    const CHR_A17_SHIFT: usize = 7;
    const CHR_A18_SHIFT: usize = 8;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            outer_reg: 0,
        }
    }

    fn decode_outer_register_write(addr: u16) -> u8 {
        (addr & 0x0F) as u8
    }

    fn outer_prg_bank_base(&self) -> usize {
        ((self.outer_reg & Self::OUTER_REG_PRG_BANK_MASK) as usize) << Self::OUTER_PRG_BANK_SHIFT
    }

    fn is_nrom_prg_mode_enabled(&self) -> bool {
        (self.outer_reg & Self::OUTER_REG_NROM_MODE_BIT) != 0
    }

    fn prg_bank_offset(addr: u16) -> usize {
        (addr as usize) & Self::PRG_BANK_OFFSET_MASK
    }

    fn cpu_a13_bank_bit(addr: u16) -> usize {
        ((addr as usize) >> Self::CPU_A13_SHIFT) & Self::CPU_A13_MASK
    }

    fn with_outer_prg_bank(&self, inner_bank: usize) -> usize {
        self.outer_prg_bank_base() | inner_bank
    }

    fn outer_chr_a18_bank_bit(&self) -> usize {
        ((self.outer_reg as usize) >> 1) & 0x01
    }

    fn outer_chr_a17_bank_bit(&self) -> usize {
        (self.outer_reg as usize) & 0x01
    }

    fn is_chr_a17_sourced_from_outer_a_bit(&self) -> bool {
        (self.outer_reg & Self::OUTER_REG_CHR_A17_SOURCE_BIT) != 0
    }

    fn nrom_base_inner_bank(&self) -> usize {
        let bank_8000 = self.mmc3.raw_prg_8k_page_number(Self::PRG_ROM_START) as usize;
        let bank_c000 = self.mmc3.raw_prg_8k_page_number(0xC000) as usize;
        let register_6 = if bank_8000 == 0xFE || bank_8000 == 0xFF {
            bank_c000
        } else {
            bank_8000
        };
        register_6 & Self::NROM_BASE_BANK_MASK
    }

    fn is_prg_rom_addr(addr: u16) -> bool {
        (Self::PRG_ROM_START..=Self::PRG_ROM_END).contains(&addr)
    }

    fn is_outer_register_write(addr: u16) -> bool {
        (Self::PRG_RAM_START..=Self::PRG_RAM_END).contains(&addr)
    }

    fn read_prg_mmc3_mode(&self, addr: u16) -> u8 {
        let inner_bank = self.mmc3.mapped_prg_bank(addr) & Self::PRG_INNER_BANK_MASK;
        let bank = self.with_outer_prg_bank(inner_bank);
        let offset = Self::prg_bank_offset(addr);
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_nrom_mode(&self, addr: u16) -> u8 {
        let inner_bank = self.nrom_base_inner_bank() + Self::cpu_a13_bank_bit(addr);
        let bank = self.with_outer_prg_bank(inner_bank);
        let offset = Self::prg_bank_offset(addr);
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn mapped_chr_bank_1k(&self, addr: u16) -> usize {
        let raw_bank = self.mmc3.raw_chr_1k_bank(addr);
        let a18 = self.outer_chr_a18_bank_bit() << Self::CHR_A18_SHIFT;
        let a17 = if self.is_chr_a17_sourced_from_outer_a_bit() {
            self.outer_chr_a17_bank_bit() << Self::CHR_A17_SHIFT
        } else {
            raw_bank & Self::CHR_INNER_BANK_A17_MASK
        };
        let low_bits = raw_bank & Self::CHR_INNER_BANK_LOW_BITS_MASK;

        a18 | a17 | low_bits
    }

    fn chr_bank_offset(addr: u16) -> usize {
        (addr as usize) & Self::CHR_BANK_OFFSET_MASK
    }

    fn read_chr_mapped(&self, addr: u16) -> u8 {
        let bank = self.mapped_chr_bank_1k(addr);
        let offset = Self::chr_bank_offset(addr);
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr_mapped(&mut self, addr: u16, value: u8) {
        let bank = self.mapped_chr_bank_1k(addr);
        let offset = Self::chr_bank_offset(addr);
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn write_outer_register_if_enabled(&mut self, addr: u16) {
        if self.mmc3.is_prg_ram_writable() {
            self.outer_reg = Self::decode_outer_register_write(addr);
        }
    }
}

impl Mapper for Mapper344 {
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
        344
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !Self::is_prg_rom_addr(addr) {
            return self.mmc3.read_prg(addr);
        }

        if self.is_nrom_prg_mode_enabled() {
            self.read_prg_nrom_mode(addr)
        } else {
            self.read_prg_mmc3_mode(addr)
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_outer_register_write(addr) {
            self.write_outer_register_if_enabled(addr);
            self.mmc3.write_prg(addr, value);
            return;
        }

        self.mmc3.write_prg(addr, value);
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if !Self::is_prg_rom_addr(addr) {
            return self.mmc3.read_prg_open_bus(addr, open_bus);
        }

        if self.is_nrom_prg_mode_enabled() {
            self.read_prg_nrom_mode(addr)
        } else {
            self.read_prg_mmc3_mode(addr)
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.read_chr_mapped(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.write_chr_mapped(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = self.mmc3.registers_snapshot();
        snapshot.push(self.outer_reg);
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
        let Some((&outer, mmc3_data)) = data.split_last() else {
            return;
        };

        self.mmc3.restore_registers(mmc3_data);
        self.outer_reg = outer;
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer_reg = 0;
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.mmc3.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 64;
    const CHR_BANKS_1K: usize = 512;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            344,
            banked_data(8 * 1024, PRG_BANKS_8K),
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 344 should be implemented")
    }

    fn make_mapper_with_banked_chr() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            344,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 344 should be implemented")
    }

    #[test]
    fn mapper_344_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            344,
            banked_data(8 * 1024, PRG_BANKS_8K),
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        ));

        assert!(result.is_ok(), "Mapper 344 should be registered in factory");
    }

    #[test]
    fn outer_bank_register_extends_prg_bank_when_wram_is_enabled() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x01);
        let before_outer = mapper.read_prg(0x8000);

        mapper.write_prg(0x6002, 0x00);
        let after_outer = mapper.read_prg(0x8000);

        assert_ne!(
            before_outer, after_outer,
            "Outer register BA bits should change PRG A18..A17 selection in MMC3 mode"
        );
    }

    #[test]
    fn nrom_mode_uses_register_6_bits_1_3_for_full_32k_window() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x8000, 0x46);
        mapper.write_prg(0x8001, 0x06);

        mapper.write_prg(0x6008, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 0x06);
        assert_eq!(mapper.read_prg(0xA000), 0x07);
        assert_eq!(mapper.read_prg(0xC000), 0x06);
        assert_eq!(mapper.read_prg(0xE000), 0x07);
    }

    #[test]
    fn outer_register_write_address_also_writes_prg_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6002, 0x5A);

        assert_eq!(mapper.read_prg(0x6002), 0x5A);
    }

    #[test]
    fn read_prg_open_bus_delegates_wram_window_to_mmc3_behavior() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA001, 0x00);

        assert_eq!(mapper.read_prg_open_bus(0x6000, 0xAA), 0xAA);
    }

    #[test]
    fn chr_a17_mode_bit_switches_source_between_mmc3_and_outer_a_bit() {
        let mut mapper = make_mapper_with_banked_chr();

        mapper.write_prg(0xA001, 0x80);

        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x80);

        mapper.write_prg(0x6002, 0x00);
        let chr_m0 = mapper.read_chr(0x1000);

        mapper.write_prg(0x6006, 0x00);
        let chr_m1 = mapper.read_chr(0x1000);

        assert_ne!(
            chr_m0, chr_m1,
            "M bit should switch CHR A17 source from MMC3 A17 to outer register A bit"
        );
    }
}
