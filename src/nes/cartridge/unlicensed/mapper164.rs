//! Mapper 164 – Waixing PEC-9588
//!
//! Specifications:
//! - Primary source: NESdev wiki <https://www.nesdev.org/wiki/INES_Mapper_164>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Waixing/Waixing164.h`
//!
//! ## Overview
//!
//! Used by Waixing's *Final Fantasy V* and similar large-PRG unlicensed titles.
//! The mapper selects a single 32 KiB PRG-ROM bank via two nibble registers
//! in the `$5000–$5FFF` address window.
//!
//! ## Memory Map
//!
//! * `CPU $8000–$FFFF`: 32 KiB PRG-ROM, bank-switched
//! * `PPU $0000–$1FFF`: 8 KiB CHR-ROM/RAM, fixed at bank 0
//!
//! ## Registers
//!
//! Both registers live in `$5000–$5FFF` (CPU).  The active register is
//! determined by bits 12 and 8 of the address (`addr & 0x7300`):
//!
//! | `addr & 0x7300` | Effect |
//! |-----------------|--------|
//! | `0x5000`        | Low nibble: `prg = (prg & 0xF0) \| (value & 0x0F)` |
//! | `0x5100`        | High nibble: `prg = (prg & 0x0F) \| ((value & 0x0F) << 4)` |
//!
//! Other `$5xxx` addresses are ignored.
//!
//! ## Power-on / Reset State
//!
//! PRG bank = `0x0F` (per Mesen2).  CHR bank fixed at 0.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 164;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 164 – Waixing PEC-9588.
///
/// See the module-level documentation for hardware details.
pub struct Mapper164 {
    base: BaseMapper,
    prg_bank: u8,
}

impl Mapper164 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_bank: 0x0F,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper164 {
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
        match addr & 0x7300 {
            0x5000 => {
                self.prg_bank = (self.prg_bank & 0xF0) | (value & 0x0F);
                self.apply_banks();
            }
            0x5100 => {
                self.prg_bank = (self.prg_bank & 0x0F) | ((value & 0x0F) << 4);
                self.apply_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.prg_bank = bank;
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0x0F;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use 16 × 32KB banks = 512KB, large enough to exercise full nibble range.
    const PRG_BANKS: usize = 16;
    const CHR_BANKS: usize = 1;

    fn make_mapper() -> Mapper164 {
        Mapper164::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_164_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 164 must be creatable via factory");
    }

    #[test]
    fn power_on_prg_bank_is_0x0f() {
        let mapper = make_mapper();
        // banked_data: bank N has first byte = N%256; bank 0x0F = 15
        assert_eq!(
            mapper.read_prg(0x8000),
            15,
            "power-on PRG bank must be 0x0F"
        );
    }

    #[test]
    fn low_nibble_write_5000_sets_low_nibble() {
        let mut mapper = make_mapper();
        // prg_bank starts 0x0F; write low nibble 5 → 0x05
        mapper.write_prg(0x5000, 0x05);
        assert_eq!(mapper.prg_bank, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn high_nibble_write_5100_sets_high_nibble() {
        let mut mapper = make_mapper();
        // prg_bank starts 0x0F; write high nibble 3 → 0x3F
        mapper.write_prg(0x5100, 0x03);
        assert_eq!(mapper.prg_bank, 0x3F);
        // With 16 banks, 0x3F % 16 = 15
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn combined_nibble_writes_produce_expected_bank() {
        let mut mapper = make_mapper();
        // Write high nibble 2 then low nibble 3 → bank 0x23
        mapper.write_prg(0x5100, 0x02);
        mapper.write_prg(0x5000, 0x03);
        assert_eq!(mapper.prg_bank, 0x23);
    }

    #[test]
    fn addr_mask_ignores_other_5xxx_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5200, 0xAB); // addr & 0x7300 = 0x5200, not 0x5000 or 0x5100
        assert_eq!(mapper.prg_bank, 0x0F, "other $5xxx writes must be ignored");
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x03);
        mapper.write_prg(0x5100, 0x02);
        mapper.reset();
        assert_eq!(mapper.prg_bank, 0x0F, "reset must restore bank to 0x0F");
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5100, 0x02);
        mapper.write_prg(0x5000, 0x05);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.prg_bank, 0x25);
    }
}
