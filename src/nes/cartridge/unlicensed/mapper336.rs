//! Mapper 336 – BMC K-3046
//!
//! Specifications:
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/BmcK3046.h`
//!
//! ## Overview
//!
//! A simple multicart board with two switchable 16 KiB PRG-ROM slots and a
//! fixed 8 KiB CHR-ROM/RAM bank.
//!
//! ## Memory Map
//!
//! * `CPU $8000–$BFFF`: 16 KiB PRG-ROM, slot 0 (switchable)
//! * `CPU $C000–$FFFF`: 16 KiB PRG-ROM, slot 1 (switchable, defaults to last bank)
//! * `PPU $0000–$1FFF`: 8 KiB CHR-ROM/RAM, fixed at bank 0
//!
//! ## Registers
//!
//! ### Write register – any address in `$8000–$FFFF`
//!
//! ```text
//! D~[OOOO OIII]
//!    +++++     outer bank bits 3–5 of the 16 KiB block (bits [5:3] of value)
//!         +++  inner bank select within the block (bits [2:0] of value)
//! ```
//!
//! * `inner = value & 0x07`
//! * `outer = value & 0x38`
//! * Slot 0 (`$8000–$BFFF`) ← `outer | inner`
//! * Slot 1 (`$C000–$FFFF`) ← `outer | 7`
//!
//! ## Power-on / Reset State
//!
//! Slot 0 = bank 0; slot 1 = bank 7.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 336;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 336 – BMC K-3046 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper336 {
    base: BaseMapper,
}

impl Mapper336 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self { base };
        mapper.apply_banks(0);
        mapper
    }

    fn apply_banks(&mut self, value: u8) {
        let inner = (value & 0x07) as i16;
        let outer = (value & 0x38) as i16;
        self.base.select_prg_page(0, outer | inner);
        self.base.select_prg_page(1, outer | 7);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper336 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_banked(addr);
        }
        0
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_banks(value);
        }
    }

    fn reset(&mut self) {
        self.apply_banks(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 8;
    const CHR_BANKS: usize = 1;

    fn make_mapper() -> Mapper336 {
        Mapper336::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_336_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 336 must be creatable via factory");
    }

    #[test]
    fn power_on_slot0_is_bank_0_slot1_is_bank_7() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let mapper = Mapper336::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg.clone(),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        // Bank N starts with byte N (from banked_data pattern).
        assert_eq!(mapper.read_prg(0x8000), 0, "slot0 must be bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot1 must be bank 7 at power-on"
        );
    }

    #[test]
    fn write_selects_banks_correctly() {
        let mut mapper = make_mapper();
        // value = 0x05: inner=5, outer=0 → slot0=5, slot1=7
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "slot0 must be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 7, "slot1 must be bank 7");

        // value = 0x09: inner=1, outer=8(0x08) → slot0=9, slot1=15 (outer|7=8|7=15)
        // With 8 banks, bank 9 % 8 = 1, bank 15 % 8 = 7
        mapper.write_prg(0x9000, 0x09);
        // banked_data: bank N has first byte = N%256; with 8 banks, 9%8=1, 15%8=7
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "slot0 should wrap to bank 1 (9 mod 8)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "slot1 should wrap to bank 7 (15 mod 8)"
        );
    }

    #[test]
    fn write_from_any_prg_address_applies() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "write to $FFFF must set slot0 bank 3"
        );
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "reset must restore slot0 to bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "reset must restore slot1 to bank 7"
        );
    }

    #[test]
    fn chr_fixed_at_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be fixed to bank 0");
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR stays at bank 0 after PRG write"
        );
    }
}
