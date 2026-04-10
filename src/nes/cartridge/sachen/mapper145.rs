//! Mapper 145 – Sachen SA-72007
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_145>
//!
//! Simple Sachen mapper with fixed 32 KiB PRG-ROM and 1-bit CHR banking.
//!
//! Memory map:
//! - CPU `$8000–$FFFF`: 32 KiB fixed PRG bank 0
//! - PPU `$0000–$1FFF`: 8 KiB switchable CHR bank (bit 7 of register)
//!
//! Register write decode (`$4100–$7FFF`, condition `(addr & 0x4100) == 0x4100`):
//! - Both bit 14 and bit 8 of the address must be set.
//! - Written value bit 7 selects the CHR bank: `CHR bank = (value >> 7) & 1`.
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

pub struct Mapper145 {
    base: BaseMapper,
}

impl Mapper145 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_chr_banking(8 * 1024);
        base.select_chr_page(0, 0);
        Self { base }
    }

    fn is_register_address(addr: u16) -> bool {
        (addr & 0x4100) == 0x4100
    }
}

impl Mapper for Mapper145 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_register_address(addr) {
            self.base.select_chr_page(0, ((value >> 7) & 1) as i16);
        }
    }

    fn reset(&mut self) {
        self.base.select_chr_page(0, 0);
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANK_32K: usize = 32 * 1024;
    const CHR_BANK_8K: usize = 8 * 1024;

    fn make_mapper145() -> Box<dyn crate::nes::cartridge::mapper::Mapper> {
        let prg = banked_data(PRG_BANK_32K, 1);
        let chr = banked_data(CHR_BANK_8K, 3);
        create_mapper(MapperContext::new_for_test(
            145,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 145 must be registered in factory")
    }

    #[test]
    fn mapper_145_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_32K, 1);
        let chr = banked_data(CHR_BANK_8K, 3);
        let result = create_mapper(MapperContext::new_for_test(
            145,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 145 must be creatable via factory");
    }

    #[test]
    fn power_on_selects_chr_bank_0() {
        let mut mapper = make_mapper145();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 on power-on");
    }

    #[test]
    fn value_bit7_set_selects_chr_bank_1() {
        let mut mapper = make_mapper145();
        mapper.write_prg(0x4100, 0x80);
        assert_eq!(mapper.read_chr(0x0000), 1, "bit 7 set → CHR bank 1");
    }

    #[test]
    fn value_bit7_clear_selects_chr_bank_0() {
        let mut mapper = make_mapper145();
        mapper.write_prg(0x4100, 0x80); // bank 1
        mapper.write_prg(0x4100, 0x00); // back to bank 0
        assert_eq!(mapper.read_chr(0x0000), 0, "bit 7 clear → CHR bank 0");
    }

    #[test]
    fn write_decode_requires_both_bit14_and_bit8() {
        let mut mapper = make_mapper145();
        // $4200 has bit 14 but not bit 8 → not decoded
        mapper.write_prg(0x4200, 0x80);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$4200 lacks bit 8, must be ignored"
        );

        // $5100 has bit 14 and bit 8 → decoded
        mapper.write_prg(0x5100, 0x80);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "$5100 has both bits, must decode"
        );
    }

    #[test]
    fn prg_is_fixed_at_bank_0() {
        let mut mapper = make_mapper145();
        mapper.write_prg(0x4100, 0x80); // change CHR, PRG must stay
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must remain fixed at bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "PRG upper end also fixed bank 0"
        );
    }

    #[test]
    fn reset_restores_chr_bank_0() {
        let mut mapper = make_mapper145();
        mapper.write_prg(0x4100, 0x80);
        assert_eq!(mapper.read_chr(0x0000), 1);
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
    }
}
