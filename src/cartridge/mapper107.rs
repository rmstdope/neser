//! Mapper 107 - Magic Dragon
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_107>
//!
//! Register behavior:
//! - Writes to `$8000-$FFFF` update both PRG and CHR banking.
//! - PRG bank (32KB at `$8000-$FFFF`): `(value >> 2) & 0x0F`.
//! - CHR bank (8KB at `$0000-$1FFF`): `value & 0x03`.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 107;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper107 {
    base: BaseMapper,
    register: u8,
}

impl Mapper107 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self { base, register: 0 };
        mapper.apply_register(0);
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;

        let prg_bank = ((value >> 2) & 0x0F) as i16;
        let chr_bank = (value & 0x03) as i16;

        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);
    }
}

impl Mapper for Mapper107 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
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

    const PRG_BANKS_32K: usize = 19;
    const CHR_BANKS_8K: usize = 7;

    fn make_mapper() -> Mapper107 {
        Mapper107::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_107_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 107 must be creatable via factory");
    }

    #[test]
    fn write_value_selects_prg_bank_from_bits_5_2() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 1, "0x04 should select PRG bank 1");

        mapper.write_prg(0xFFFF, 0x0F);
        assert_eq!(mapper.read_prg(0x8000), 3, "0x0F should select PRG bank 3");
    }

    #[test]
    fn write_value_selects_chr_bank_from_bits_1_0() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 1, "0x05 should select CHR bank 1");

        mapper.write_prg(0x8000, 0x0F);
        assert_eq!(mapper.read_chr(0x0000), 3, "0x0F should select CHR bank 3");
    }

    #[test]
    fn write_updates_prg_and_chr_banks_together() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "0x00 -> PRG bank 0");
        assert_eq!(mapper.read_chr(0x0000), 0, "0x00 -> CHR bank 0");

        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 1, "0x05 -> PRG bank 1");
        assert_eq!(mapper.read_chr(0x0000), 1, "0x05 -> CHR bank 1");

        mapper.write_prg(0xFFFF, 0x0F);
        assert_eq!(mapper.read_prg(0x8000), 3, "0x0F -> PRG bank 3");
        assert_eq!(mapper.read_chr(0x0000), 3, "0x0F -> CHR bank 3");
    }
}
