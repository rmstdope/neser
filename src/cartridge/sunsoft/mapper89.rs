//! Mapper 089 - Sunsoft-2
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_089>
//! - Mesen2 reference: `Sunsoft89.h`

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper89 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper89 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
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

        let prg_bank = ((value >> 4) & 0x07) as i16;
        let chr_bank = ((value & 0x07) | ((value & 0x80) >> 4)) as i16;
        let mirroring = if (value & 0x08) != 0 {
            NametableLayout::SingleScreenUpper
        } else {
            NametableLayout::SingleScreenLower
        };

        self.base.select_prg_page(0, prg_bank);
        self.base.select_prg_page(1, -1);
        self.base.select_chr_page(0, chr_bank);
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper89 {
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
        self.apply_register(value);
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

    const PRG_BANKS_16K: usize = 11;
    const CHR_BANKS_8K: usize = 13;

    fn make_mapper() -> Mapper89 {
        Mapper89::new(MapperContext::new_for_test(
            89,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_89_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            89,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 89 must be registered in the factory"
        );
    }

    #[test]
    fn write_selects_16k_prg_bank_at_8000_and_keeps_last_bank_fixed_at_c000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x20); // PRG bank = (0x20 >> 4) & 0x07 = 2

        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000-$BFFF should map selected PRG bank"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS_16K - 1) as u8,
            "$C000-$FFFF should stay fixed to last PRG bank"
        );
    }

    #[test]
    fn write_selects_8k_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x83); // CHR bank = (0x83 & 0x07) | ((0x83 & 0x80) >> 4) = 11

        assert_eq!(
            mapper.read_chr(0x0000),
            11,
            "CHR bank should switch at $0000"
        );
        assert_eq!(
            mapper.read_chr(0x1FFF),
            11,
            "CHR bank should switch at $1FFF"
        );
    }

    #[test]
    fn mirroring_bit_toggles_one_screen_layout() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "bit 3 clear should select one-screen lower"
        );

        mapper.write_prg(0x8000, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "bit 3 set should select one-screen upper"
        );
    }

    #[test]
    fn capabilities_match_mapper89_contract() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "mapper 89 must support CHR banking");
        assert!(
            caps.has_dynamic_mirroring,
            "mapper 89 must support dynamic mirroring"
        );
        assert!(!caps.has_irq, "mapper 89 should not have IRQ support");
    }
}
