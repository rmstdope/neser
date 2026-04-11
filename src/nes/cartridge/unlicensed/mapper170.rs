//! Mapper 170 – Shiko Game Syu
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_170>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper170.h`
//!
//! ## Overview
//!
//! Used by *Shiko Game Syu* (式場遊), a Japanese unlicensed Famicom game.
//!
//! ## Memory Map
//!
//! * `CPU $8000–$FFFF`: 32 KiB PRG-ROM, fixed at bank 0
//! * `PPU $0000–$1FFF`: 8 KiB CHR-ROM/RAM, fixed at bank 0
//!
//! ## Registers
//!
//! The mapper exposes one read register and one write register, both accessed via
//! the CPU address bus with no mirroring mask (exact addresses only).
//!
//! ### Write register – `$6502`
//!
//! ```text
//! D~[VVVV VVVV]
//!    +-++++--- value stored as (value << 1) & 0x80 in internal reg
//! ```
//!
//! Writing to `$6502` stores `(value << 1) & 0x80` in the internal register.
//!
//! ### Read register – `$7777`
//!
//! ```text
//! Returns: reg | ((addr >> 8) & 0x7F)
//! ```
//!
//! Reading from `$7777` returns the internal register OR'd with the upper byte
//! of the address masked to 7 bits.
//!
//! ## Power-on / Reset State
//!
//! Internal register = 0. PRG and CHR banks fixed at 0.
//!
//! ## Known Limitations
//!
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 170;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 170 – Shiko Game Syu.
///
/// See the module-level documentation for hardware details.
pub struct Mapper170 {
    base: BaseMapper,
    /// Internal register written via `$6502` as `(value << 1) & 0x80`.
    reg: u8,
}

impl Mapper170 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self { base, reg: 0 };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper170 {
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
        // $6502 read: return reg | ((addr >> 8) & 0x7F)
        if addr == 0x7777 {
            return self.reg | ((addr >> 8) as u8 & 0x7F);
        }
        self.base.read_prg_banked(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // $6502 write: reg = (value << 1) & 0x80
        if addr == 0x6502 {
            self.reg = (value << 1) & 0x80;
        }
        // All other writes are ignored (no PRG-RAM, no bank switching writes)
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&r) = data.first() {
            self.reg = r;
        }
    }

    fn reset(&mut self) {
        self.reg = 0;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const TEST_PRG_BANKS: usize = 1;
    const TEST_CHR_BANKS: usize = 1;

    fn make_mapper() -> Mapper170 {
        Mapper170::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_170_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 170 must be creatable via factory");
    }

    #[test]
    fn prg_is_fixed_at_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG must be fixed to bank 0");
    }

    #[test]
    fn chr_is_fixed_at_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be fixed to bank 0");
    }

    #[test]
    fn reg_is_zero_at_power_on() {
        let mapper = make_mapper();
        assert_eq!(mapper.reg, 0);
    }

    #[test]
    fn write_6502_stores_shifted_value() {
        let mut mapper = make_mapper();
        // Writing 0x01: (0x01 << 1) & 0x80 = 0x02 & 0x80 = 0x00
        mapper.write_prg(0x6502, 0x01);
        assert_eq!(mapper.reg, 0x00);

        // Writing 0xC0: (0xC0 << 1) & 0x80 = 0x80 & 0x80 = 0x80
        mapper.write_prg(0x6502, 0xC0);
        assert_eq!(mapper.reg, 0x80);
    }

    #[test]
    fn read_7777_returns_reg_or_addr_high_byte() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6502, 0xC0); // reg = 0x80
        // $7777: reg | ((0x7777 >> 8) & 0x7F) = 0x80 | (0x77 & 0x7F) = 0x80 | 0x77 = 0xF7
        let val = mapper.read_prg(0x7777);
        assert_eq!(val, 0xF7, "read $7777 must return reg | (0x77 & 0x7F)");
    }

    #[test]
    fn read_7777_with_zero_reg() {
        let mapper = make_mapper();
        // reg=0: 0x00 | (0x77 & 0x7F) = 0x77
        let val = mapper.read_prg(0x7777);
        assert_eq!(val, 0x77);
    }

    #[test]
    fn write_other_addresses_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF); // should not change reg
        assert_eq!(mapper.reg, 0x00);
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.reg, 0x00);
    }

    #[test]
    fn reset_clears_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6502, 0xC0);
        assert_eq!(mapper.reg, 0x80);
        mapper.reset();
        assert_eq!(mapper.reg, 0x00);
    }

    #[test]
    fn snapshot_restore_round_trips_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6502, 0xC0); // reg = 0x80
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.reg, 0x80);
    }
}
