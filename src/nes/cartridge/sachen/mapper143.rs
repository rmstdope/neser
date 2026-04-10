//! Mapper 143 – Sachen SA-013 / SA-013A copy-protection
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_143>
//!
//! The board has no bank-switching; it exposes only a copy-protection
//! read window at CPU $4100–$5FFF that returns an address-derived value
//! used by the game to verify the board is genuine hardware.
//!
//! Memory map:
//! - CPU `$8000–$BFFF`: 16 KiB fixed PRG bank 0
//! - CPU `$C000–$FFFF`: 16 KiB fixed PRG bank 1
//! - PPU `$0000–$1FFF`: 8 KiB fixed CHR bank 0
//!
//! Copy-protection read ($4100–$5FFF):
//! - Returns `(!addr as u8 & 0x3F) | 0x40`
//! - Bit 7 is always 0; bit 6 is always 1; bits 5–0 are the bitwise NOT
//!   of the lower byte of the address.
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

pub struct Mapper143 {
    base: BaseMapper,
}

impl Mapper143 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let base = BaseMapper::new(&ctx, capabilities);
        Self { base }
    }

    fn is_protection_window(addr: u16) -> bool {
        (0x4100..=0x5FFF).contains(&addr)
    }

    fn copy_protection_response(addr: u16) -> u8 {
        (!(addr as u8) & 0x3F) | 0x40
    }
}

impl Mapper for Mapper143 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_protection_window(addr) {
            return Self::copy_protection_response(addr);
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // no register writes; copy-protection is read-only
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANK_16K: usize = 16 * 1024;
    const CHR_BANK_8K: usize = 8 * 1024;

    fn make_mapper143() -> Box<dyn crate::nes::cartridge::mapper::Mapper> {
        let prg = banked_data(PRG_BANK_16K, 2);
        let chr = banked_data(CHR_BANK_8K, 1);
        create_mapper(MapperContext::new_for_test(
            143,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 143 must be registered in factory")
    }

    #[test]
    fn mapper_143_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_16K, 2);
        let chr = banked_data(CHR_BANK_8K, 1);
        let result = create_mapper(MapperContext::new_for_test(
            143,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 143 must be creatable via factory");
    }

    #[test]
    fn protection_read_at_4100_returns_addr_not_low_byte_or_0x40() {
        let mut mapper = make_mapper143();
        // addr=$4100, low byte=0x00, ~0x00=0xFF, 0xFF&0x3F=0x3F, |0x40=0x7F
        let val = mapper.read_prg_open_bus(0x4100, 0x00);
        assert_eq!(val, 0x7F, "read at $4100 should return 0x7F");
    }

    #[test]
    fn protection_read_at_4101_returns_addr_not_low_byte_or_0x40() {
        let mut mapper = make_mapper143();
        // addr=$4101, low byte=0x01, ~0x01=0xFE, 0xFE&0x3F=0x3E, |0x40=0x7E
        let val = mapper.read_prg_open_bus(0x4101, 0x00);
        assert_eq!(val, 0x7E, "read at $4101 should return 0x7E");
    }

    #[test]
    fn protection_read_at_5fff_returns_addr_not_low_byte_or_0x40() {
        let mut mapper = make_mapper143();
        // addr=$5FFF, low byte=0xFF, ~0xFF=0x00, 0x00&0x3F=0x00, |0x40=0x40
        let val = mapper.read_prg_open_bus(0x5FFF, 0x00);
        assert_eq!(val, 0x40, "read at $5FFF should return 0x40");
    }

    #[test]
    fn prg_bank_0_is_fixed_at_8000() {
        let mapper = make_mapper143();
        // PRG bank 0 is filled with 0x00 (bank id 0 from banked_data)
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "bank 0 should be fixed at $8000"
        );
    }

    #[test]
    fn prg_bank_1_is_fixed_at_c000() {
        let mapper = make_mapper143();
        // PRG bank 1 is filled with 0x01 (bank id 1 from banked_data)
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "bank 1 should be fixed at $C000"
        );
    }

    #[test]
    fn chr_is_fixed_at_bank_0() {
        let mut mapper = make_mapper143();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR should be fixed at bank 0");
    }
}
