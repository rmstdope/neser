//! Mapper 122 - Sunsoft-1 (iNES 122 variant)
//!
//! Specifications:
//! - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_122>
//! - Reference behavior: FCEUmm `boards/122.c`

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_BANK_SIZE: usize = 4 * 1024;

pub struct Mapper122 {
    base: BaseMapper,
    chr_low_bank: u8,
    chr_high_bank: u8,
}

impl Mapper122 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 4,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            chr_low_bank: 0,
            chr_high_bank: 0,
        };
        mapper.sync();
        mapper
    }

    fn sync(&mut self) {
        self.base.select_chr_page(0, self.chr_low_bank as i16);
        self.base.select_chr_page(1, self.chr_high_bank as i16);
    }
}

impl Mapper for Mapper122 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        if (addr & 1) == 0 {
            self.chr_low_bank = value;
        } else {
            self.chr_high_bank = value;
        }
        self.sync();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_low_bank, self.chr_high_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.chr_low_bank = data[0];
            self.chr_high_bank = data[1];
            self.sync();
        }
    }

    fn reset(&mut self) {
        self.chr_low_bank = 0;
        self.chr_high_bank = 0;
        self.sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 3;
    const CHR_BANKS_4K: usize = 11;
    const PRG_BANK_SIZE_32K: usize = 32 * 1024;
    const TEST_PRG_PATTERN_BYTE: u8 = 0x5A;

    fn make_mapper(mirroring: NametableLayout) -> Mapper122 {
        Mapper122::new(MapperContext::new_for_test(
            122,
            banked_data(PRG_BANK_SIZE_32K, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_4K),
            mirroring,
        ))
    }

    #[test]
    fn mapper_122_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            122,
            banked_data(PRG_BANK_SIZE_32K, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_4K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "mapper 122 should be registered in factory");
    }

    #[test]
    fn prg_is_fixed_32k_bank_0() {
        let mut mapper = make_mapper(NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x07);
        mapper.write_prg(0x8001, 0x09);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn even_write_selects_chr_0000_bank_and_odd_write_selects_chr_1000_bank() {
        let mut mapper = make_mapper(NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 7);
        mapper.write_prg(0x8001, 9);

        assert_eq!(mapper.read_chr(0x0000), 7);
        assert_eq!(mapper.read_chr(0x1000), 9);
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = make_mapper(NametableLayout::Vertical);
        mapper.write_prg(0x8000, 1);
        mapper.write_prg(0x8001, 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn prg_16kb_rom_is_mirrored_without_panic() {
        let mut mapper = Mapper122::new(MapperContext::new_for_test(
            122,
            vec![TEST_PRG_PATTERN_BYTE; 16 * 1024],
            banked_data(CHR_BANK_SIZE, CHR_BANKS_4K),
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), TEST_PRG_PATTERN_BYTE);
        assert_eq!(mapper.read_prg(0xC000), TEST_PRG_PATTERN_BYTE);
    }
}
