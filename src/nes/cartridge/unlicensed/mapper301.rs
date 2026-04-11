//! Mapper 301 – BMC 8157
//!
//! Specifications:
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Bmc8157.h`
//!
//! ## Overview
//!
//! A multicart mapper that derives two 16 KiB PRG-ROM banks from the *address*
//! of the last write (data byte is ignored).  Mirroring is also address-driven.
//!
//! ## Memory Map
//!
//! * `CPU $8000–$BFFF`: 16 KiB PRG-ROM, slot 0 (address-derived)
//! * `CPU $C000–$FFFF`: 16 KiB PRG-ROM, slot 1 (address-derived)
//! * `PPU $0000–$1FFF`: 8 KiB CHR-ROM/RAM, fixed at bank 0
//!
//! ## Bank Selection
//!
//! Writing to any address in `$8000–$FFFF` latches the address and re-derives
//! both slots:
//!
//! ```text
//! innerPrg0  = (addr >> 2) & 0x07
//! innerPrg1  = ((addr >> 7) & 0x01) | ((addr >> 8) & 0x02)
//! outer128   = (addr >> 5) & 0x03
//! outer512   = (addr >> 8) & 0x01
//!
//! baseBank = if innerPrg1 == 0 { 0 }
//!            else if innerPrg1 == 1 { innerPrg0 }
//!            else { 7 }
//!
//! slot0 = (outer512 << 6) | (outer128 << 3) | innerPrg0
//! slot1 = (outer512 << 6) | (outer128 << 3) | baseBank
//! ```
//!
//! ## Mirroring
//!
//! Bit 1 of the latched address selects mirroring:
//!  * 0 → Vertical
//!  * 1 → Horizontal
//!
//! ## Power-on / Reset State
//!
//! Latched address = 0, both slots derived from address 0.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 301;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 301 – BMC 8157.
///
/// See the module-level documentation for hardware details.
pub struct Mapper301 {
    base: BaseMapper,
    last_addr: u16,
}

impl Mapper301 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            has_dynamic_mirroring: true,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self { base, last_addr: 0 };
        mapper.update_state();
        mapper
    }

    fn update_state(&mut self) {
        let addr = self.last_addr as u32;

        let inner_prg0 = ((addr >> 2) & 0x07) as i16;
        let inner_prg1 = (((addr >> 7) & 0x01) | ((addr >> 8) & 0x02)) as u8;
        let outer128 = ((addr >> 5) & 0x03) as i16;
        let outer512 = ((addr >> 8) & 0x01) as i16;

        let base_bank: i16 = match inner_prg1 {
            0 => 0,
            1 => inner_prg0,
            _ => 7,
        };

        self.base
            .select_prg_page(0, (outer512 << 6) | (outer128 << 3) | inner_prg0);
        self.base
            .select_prg_page(1, (outer512 << 6) | (outer128 << 3) | base_bank);
        self.base.select_chr_page(0, 0);

        let mirroring = if (addr & 0x02) != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        };
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper301 {
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

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.last_addr = addr;
            self.update_state();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            (self.last_addr & 0xFF) as u8,
            ((self.last_addr >> 8) & 0xFF) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.last_addr = (data[0] as u16) | ((data[1] as u16) << 8);
            self.update_state();
        }
    }

    fn reset(&mut self) {
        self.last_addr = 0;
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use 8 × 16KB banks = 128KB
    const PRG_BANKS: usize = 8;
    const CHR_BANKS: usize = 1;

    fn make_mapper() -> Mapper301 {
        Mapper301::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_301_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 301 must be creatable via factory");
    }

    #[test]
    fn power_on_both_slots_at_bank_0() {
        let mapper = make_mapper();
        // lastAddr=0: innerPrg0=0, innerPrg1=0→baseBank=0, outer128=0, outer512=0
        // slot0=0, slot1=0
        assert_eq!(mapper.read_prg(0x8000), 0, "slot0 must be bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "slot1 must be bank 0");
    }

    #[test]
    fn write_addr_0x8014_selects_expected_banks() {
        let mut mapper = make_mapper();
        // addr=0x8014: innerPrg0=(0x8014>>2)&7=(0x2005)&7=5;
        // innerPrg1=((0x8014>>7)&1)|((0x8014>>8)&2)=(0x100&1)|(0x80&2)=0|0=0 → baseBank=0
        // outer128=(0x8014>>5)&3=(0x400)&3=0; outer512=(0x8014>>8)&1=0x80&1=0
        // slot0=5, slot1=0
        mapper.write_prg(0x8014, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "slot0 should be bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "slot1 should be bank 0 (baseBank=0)"
        );
    }

    #[test]
    fn mirroring_from_addr_bit_1() {
        let mut mapper = make_mapper();
        // addr=0x8000: bit1=0 → Vertical
        mapper.write_prg(0x8000, 0);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "bit1=0 must be Vertical"
        );
        // addr=0x8002: bit1=1 → Horizontal
        mapper.write_prg(0x8002, 0);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "bit1=1 must be Horizontal"
        );
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8014, 0x00);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 0);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8014, 0x00);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.last_addr, 0x8014);
        assert_eq!(restored.read_prg(0x8000), 5);
    }
}
