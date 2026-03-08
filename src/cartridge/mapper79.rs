//! Mapper 79 - NINA-03/NINA-06
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_079>
//!
//! Hardware register behavior used by this implementation (NINA-03/NINA-06):
//! - Register write decode: `$4100-$5FFF`, but only addresses where
//!   `(addr & 0xE100) == 0x4100` latch the register.
//! - PRG bank (32KB at `$8000-$FFFF`): `bit 3` (`(value >> 3) & 0x01`).
//! - CHR bank (8KB at `$0000-$1FFF`): `bits [2:0]` (`value & 0x07`).
//! - Other bits are ignored for mapper 79.
//! - Mirroring is fixed from the ROM header.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const REGISTER_ADDR_START: u16 = 0x4100;
const REGISTER_ADDR_END: u16 = 0x5FFF;
const REGISTER_ADDR_MASK: u16 = 0xE100;
const REGISTER_ADDR_MATCH: u16 = 0x4100;
const PRG_BANK_MASK: u8 = 0b0000_0001;
const PRG_BANK_SHIFT: u8 = 3;
const CHR_BANK_MASK: u8 = 0b0000_0111;

/// Mapper 79 - NINA-03/NINA-06
pub struct Mapper79 {
    base: BaseMapper,
    register: u8,
}

impl Mapper79 {
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
        self.base.select_prg_page(0, Self::prg_bank(value));
        self.base.select_chr_page(0, Self::chr_bank(value));
    }

    fn prg_bank(value: u8) -> i16 {
        ((value >> PRG_BANK_SHIFT) & PRG_BANK_MASK) as i16
    }

    fn chr_bank(value: u8) -> i16 {
        (value & CHR_BANK_MASK) as i16
    }

    fn is_register_address(addr: u16) -> bool {
        (REGISTER_ADDR_START..=REGISTER_ADDR_END).contains(&addr)
            && (addr & REGISTER_ADDR_MASK) == REGISTER_ADDR_MATCH
    }
}

impl Mapper for Mapper79 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_register_address(addr) {
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

    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 9;

    fn make_mapper(mirroring: NametableLayout) -> Mapper79 {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        Mapper79::new(MapperContext::new_for_test(79, prg, chr, mirroring))
    }

    #[test]
    fn mapper_79_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);

        let result = create_mapper(MapperContext::new_for_test(
            79,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));

        assert!(result.is_ok(), "Mapper 79 must be creatable via factory");
    }

    #[test]
    fn value_bit_3_selects_32k_prg_bank() {
        let mut mapper = make_mapper(NametableLayout::Horizontal);

        mapper.write_prg(0x4100, 0b0000_0000);
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x4100, 0b0000_1000);

        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "bit 3 from register should select 32KB PRG bank"
        );
    }

    #[test]
    fn value_bits_2_0_select_8k_chr_bank() {
        let mut mapper = make_mapper(NametableLayout::Horizontal);

        mapper.write_prg(0x4100, 0b0000_0101);

        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "bits [2:0] from register should select 8KB CHR bank"
        );
    }

    #[test]
    fn register_decode_uses_4100_mask_pattern() {
        let mut mapper = make_mapper(NametableLayout::Horizontal);

        mapper.write_prg(0x4100, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 1);

        mapper.write_prg(0x4200, 0b0000_0100);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "$4200 should not latch mapper 79 register"
        );

        mapper.write_prg(0x5100, 0b0000_0110);
        assert_eq!(
            mapper.read_chr(0x0000),
            6,
            "$5100 should latch mapper 79 register"
        );
    }

    #[test]
    fn writes_to_prg_rom_are_ignored() {
        let mut mapper = make_mapper(NametableLayout::Horizontal);

        mapper.write_prg(0x4100, 0b0000_1000);
        assert_eq!(mapper.read_prg(0x8000), 1);

        mapper.write_prg(0x8000, 0b0000_0101);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "writes in $8000-$FFFF must not change mapper 79 banks"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = make_mapper(NametableLayout::Vertical);

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x4100, 0b0010_0011);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
