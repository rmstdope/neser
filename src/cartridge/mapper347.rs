//! Mapper 347 - Kaiser UNL-KS7030 (FDS conversion)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_347>
//!
//! Known Limitations:
//! - FDS expansion audio output is not currently synthesized.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

pub struct Mapper347 {
    base: BaseMapper,
    reg_8000: u8,
    bank_7000: u8,
    bank_9000: u8,
    prg_ram: Vec<u8>,
}

impl Mapper347 {
    const PRG_BANK_SIZE_4K: usize = 0x1000;
    const FIRST_REGION_SIZE: usize = 64 * 1024;
    const SECOND_REGION_BASE: usize = 64 * 1024;
    const SECOND_REGION_SIZE: usize = 32 * 1024;
    const FIXED_REGION_BASE: usize = 96 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            has_irq: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 4,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_chr_banking(8 * 1024);
        base.select_chr_page(0, 0);
        Self {
            base,
            reg_8000: 0,
            bank_7000: 0,
            bank_9000: 0,
            prg_ram: vec![0; Self::PRG_RAM_SIZE],
        }
    }

    fn read_prg_rom_at(&self, index: usize) -> u8 {
        let prg = self.base.prg_rom();
        if prg.is_empty() {
            return 0;
        }
        prg[index % prg.len()]
    }

    fn bank_count_4k_in_region(&self, region_len: usize) -> usize {
        let available = self.base.prg_rom().len().min(region_len);
        (available / Self::PRG_BANK_SIZE_4K).max(1)
    }

    fn read_bank_9000_4k(&self, bank_offset: usize) -> u8 {
        let bank_count = self.bank_count_4k_in_region(Self::FIRST_REGION_SIZE);
        let bank = (self.bank_9000 as usize) % bank_count;
        self.read_prg_rom_at(bank * Self::PRG_BANK_SIZE_4K + bank_offset)
    }

    fn read_bank_8000_4k(&self, bank_offset: usize) -> u8 {
        let bank_count = self.bank_count_4k_in_region(Self::SECOND_REGION_SIZE);
        let bank = (self.bank_7000 as usize) % bank_count;
        let index = Self::SECOND_REGION_BASE + bank * Self::PRG_BANK_SIZE_4K + bank_offset;
        self.read_prg_rom_at(index)
    }

    fn read_fixed_32k_overlay(&self, addr: u16) -> u8 {
        let cpu_offset = (addr as usize) - 0x8000;
        self.read_prg_rom_at(Self::FIXED_REGION_BASE + cpu_offset)
    }

    fn prg_ram_index(addr: u16) -> Option<usize> {
        let index = match addr {
            0x6000..=0x6BFF => (addr - 0x6000) as usize,
            0xB800..=0xBFFF => 0x0C00 + (addr - 0xB800) as usize,
            0xCC00..=0xD7FF => 0x1400 + (addr - 0xCC00) as usize,
            _ => return None,
        };
        Some(index)
    }

    fn read_prg_ram_window(&self, addr: u16) -> Option<u8> {
        let ram_index = Self::prg_ram_index(addr)?;
        Some(self.prg_ram[ram_index])
    }

    fn write_prg_ram_window(&mut self, addr: u16, value: u8) -> bool {
        let Some(ram_index) = Self::prg_ram_index(addr) else {
            return false;
        };
        self.prg_ram[ram_index] = value;
        true
    }
}

impl Mapper for Mapper347 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.write_prg_ram_window(addr, value) {
            return;
        }

        match addr {
            0x8000..=0x8FFF => {
                self.reg_8000 = value;
                self.bank_7000 = value & 0x07;
                self.base.set_mirroring_hv((value & 0x08) != 0);
            }
            0x9000..=0x9FFF => {
                self.bank_9000 = value & 0x0F;
            }
            _ => {}
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.read_prg_ram_window(addr) {
            return value;
        }

        match addr {
            0x6C00..=0x6FFF => self.read_bank_9000_4k(0x0C00 + (addr - 0x6C00) as usize),
            0x7000..=0x7FFF => self.read_bank_8000_4k((addr - 0x7000) as usize),
            0x8000..=0xB7FF => self.read_fixed_32k_overlay(addr),
            0xC000..=0xCBFF => self.read_bank_9000_4k((addr - 0xC000) as usize),
            0xD800..=0xFFFF => self.read_fixed_32k_overlay(addr),
            _ => 0,
        }
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let copy_len = self.prg_ram.len().min(data.len());
        self.prg_ram[..copy_len].copy_from_slice(&data[..copy_len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg_8000, self.bank_9000]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.reg_8000 = data[0];
            self.bank_7000 = data[0] & 0x07;
            self.bank_9000 = data[1] & 0x0F;
        }
        self.base.set_mirroring_hv((self.reg_8000 & 0x08) != 0);
    }

    fn reset(&mut self) {
        self.reg_8000 = 0;
        self.bank_7000 = 0;
        self.bank_9000 = 0;
        self.base.set_mirroring_hv(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    const PRG_SIZE: usize = 128 * 1024;

    fn make_prg_rom() -> Vec<u8> {
        let mut rom = vec![0u8; PRG_SIZE];

        for bank in 0..16 {
            let start = bank * 0x1000;
            rom[start..start + 0x1000].fill(bank as u8);
        }

        let offset_8000_banks = 64 * 1024;
        for bank in 0..8 {
            let start = offset_8000_banks + bank * 0x1000;
            rom[start..start + 0x1000].fill(0x40 + bank as u8);
        }

        let fixed_offset = 96 * 1024;
        for chunk in 0..32 {
            let start = fixed_offset + chunk * 0x0400;
            rom[start..start + 0x0400].fill(0x80 + chunk as u8);
        }

        rom
    }

    fn make_mapper() -> Mapper347 {
        Mapper347::new(MapperContext::new_for_test(
            347,
            make_prg_rom(),
            vec![0xCC; 8 * 1024],
            NametableLayout::Vertical,
        ))
    }

    fn expected_fixed_region_byte(addr: u16) -> u8 {
        let cpu_offset = (addr - 0x8000) as usize;
        0x80 + (cpu_offset / 0x0400) as u8
    }

    #[test]
    fn mapper_347_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            347,
            make_prg_rom(),
            vec![0xCC; 8 * 1024],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 347 must be registered");
    }

    #[test]
    fn register_8000_selects_4k_bank_for_7000_window() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_prg(0x7000), 0x42);

        mapper.write_prg(0x8001, 0x07);
        assert_eq!(mapper.read_prg(0x7FFF), 0x47);
    }

    #[test]
    fn register_8000_bit3_controls_mirroring() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8000, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn register_9000_selects_4k_bank_split_across_6c00_and_c000_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x0A);

        assert_eq!(mapper.read_prg(0x6C00), 0x0A);
        assert_eq!(mapper.read_prg(0x6FFF), 0x0A);
        assert_eq!(mapper.read_prg(0xC000), 0x0A);
        assert_eq!(mapper.read_prg(0xCBFF), 0x0A);
    }

    #[test]
    fn fixed_prg_regions_follow_final_32k_layout() {
        let mapper = make_mapper();

        assert_eq!(mapper.read_prg(0x8000), expected_fixed_region_byte(0x8000));
        assert_eq!(mapper.read_prg(0xB7FF), expected_fixed_region_byte(0xB7FF));
        assert_eq!(mapper.read_prg(0xD800), expected_fixed_region_byte(0xD800));
        assert_eq!(mapper.read_prg(0xFFFF), expected_fixed_region_byte(0xFFFF));
    }

    #[test]
    fn split_prg_ram_windows_are_writable() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x6BFF, 0x22);
        mapper.write_prg(0xB800, 0x33);
        mapper.write_prg(0xBFFF, 0x44);
        mapper.write_prg(0xCC00, 0x55);
        mapper.write_prg(0xD7FF, 0x66);

        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0x6BFF), 0x22);
        assert_eq!(mapper.read_prg(0xB800), 0x33);
        assert_eq!(mapper.read_prg(0xBFFF), 0x44);
        assert_eq!(mapper.read_prg(0xCC00), 0x55);
        assert_eq!(mapper.read_prg(0xD7FF), 0x66);
    }

    #[test]
    fn chr_is_fixed_to_single_8k_page() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x07);
        mapper.write_prg(0x9000, 0x0F);

        assert_eq!(mapper.read_chr(0x0000), 0xCC);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn capabilities_report_no_irq_or_expansion_audio() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();

        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(!caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
    }
}
