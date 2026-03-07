//! Mapper 346 - Kaiser KS7012
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/Mapper>
//! - Reference behavior: Mesen2 `Core/NES/Mappers/Kaiser/Kaiser7012.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE_KB: usize = 32;
const CHR_BANK_SIZE_KB: usize = 8;
const PRG_BANK_SIZE_BYTES: usize = PRG_BANK_SIZE_KB * 1024;
const CHR_BANK_SIZE_BYTES: usize = CHR_BANK_SIZE_KB * 1024;
const PRG_BANK_0: i16 = 0;
const PRG_BANK_1: i16 = 1;
const CHR_BANK_0: i16 = 0;
const WRITE_ADDR_SELECT_PRG_BANK_0: u16 = 0xE0A0;
const WRITE_ADDR_SELECT_PRG_BANK_1: u16 = 0xEE36;

/// Mapper 346 - Kaiser KS7012
pub struct Mapper346 {
    base: BaseMapper,
}

impl Mapper346 {
    fn selected_prg_bank(addr: u16) -> Option<i16> {
        match addr {
            WRITE_ADDR_SELECT_PRG_BANK_0 => Some(PRG_BANK_0),
            WRITE_ADDR_SELECT_PRG_BANK_1 => Some(PRG_BANK_1),
            _ => None,
        }
    }

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE_KB,
            chr_bank_size_kb: CHR_BANK_SIZE_KB,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        base.select_prg_page(0, PRG_BANK_1);
        base.select_chr_page(0, CHR_BANK_0);
        Self { base }
    }
}

impl Mapper for Mapper346 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if let Some(bank) = Self::selected_prg_bank(addr) {
            self.base.select_prg_page(0, bank);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.base.banking_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn test_context() -> MapperContext {
        MapperContext::new_for_test(
            346,
            banked_data(PRG_BANK_SIZE_BYTES, 2),
            banked_data(CHR_BANK_SIZE_BYTES, 2),
            NametableLayout::Horizontal,
        )
    }

    fn make_mapper() -> Mapper346 {
        Mapper346::new(test_context())
    }

    #[test]
    fn mapper_346_is_registered() {
        let mapper = create_mapper(test_context());
        assert!(mapper.is_ok(), "Mapper 346 must be registered in factory");
    }

    #[test]
    fn power_on_maps_prg_bank_1() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);
    }

    #[test]
    fn write_e0a0_selects_prg_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE0A0, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn write_ee36_selects_prg_bank_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE0A0, 0x00);
        mapper.write_prg(0xEE36, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);
    }

    #[test]
    fn write_other_addresses_do_not_change_prg_bank() {
        let mut mapper = make_mapper();
        let before = mapper.read_prg(0x8000);

        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0xE0A1, 0x00);
        mapper.write_prg(0xEE37, 0x00);

        assert_eq!(mapper.read_prg(0x8000), before);
        assert_eq!(mapper.read_prg(0xFFFF), before);
    }

    #[test]
    fn chr_is_fixed_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE0A0, 0x00);
        mapper.write_prg(0xEE36, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xE0A0, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore_preserves_prg_bank_selection() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE0A0, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);

        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 0);
        assert_eq!(restored.read_prg(0xFFFF), 0);
    }
}
