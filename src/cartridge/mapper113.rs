//! Mapper 113 - NINA-03/NINA-06 multicart mode
//!
//! Specifications:
//! - Mesen2: `Nina03_06` (`multicartMode=true`)
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Nina03_06.h>
//! - NesDev wiki: <https://www.nesdev.org/wiki/INES_Mapper_113>
//!
//! Register decode and bit layout follow the Mesen/NesDev behavior:
//! - Register write recognized when `(addr & 0xE100) == 0x4100`
//! - PRG (32 KiB): `(value >> 3) & 0x07`
//! - CHR (8 KiB): `(value & 0x07) | ((value >> 3) & 0x08)` (4-bit bank)
//! - Mirroring: `bit7` (1 = Vertical, 0 = Horizontal)
//!
//! Known limitations:
//! - No known gameplay-blocking limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const REGISTER_ADDRESS_MASK: u16 = 0xE100;
const REGISTER_ADDRESS_MATCH: u16 = 0x4100;

pub struct Mapper113 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper113 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, reg: 0 };
        mapper.apply_register(0);
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.reg = value;
        let prg_bank = ((value >> 3) & 0x07) as i16;
        let chr_bank = ((value & 0x07) | ((value >> 3) & 0x08)) as i16;
        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);

        let mirroring = if (value & 0x80) != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper113 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        113
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (addr & REGISTER_ADDRESS_MASK) == REGISTER_ADDRESS_MATCH {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }

    fn reset(&mut self) {
        self.apply_register(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 8;
    const CHR_BANKS_8K: usize = 16;

    fn make_mapper() -> Mapper113 {
        Mapper113::new(MapperContext::new_for_test(
            113,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_113_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            113,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "mapper 113 should be registered in the factory"
        );
    }

    #[test]
    fn write_4100_controls_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x4100, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.read_chr(0x0000), 15);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn write_address_decode_matches_4100_mask() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x40);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 8);

        mapper.write_prg(0x4200, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "writes outside (addr & 0xE100)==0x4100 should be ignored"
        );
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_last_written_value() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0xCF);
        let snapshot = mapper.registers_snapshot();

        mapper.write_prg(0x4100, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.restore_registers(&snapshot);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 15);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
