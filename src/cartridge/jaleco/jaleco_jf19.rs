//! Mapper 092 - Jaleco JF-19/JF-21
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_092>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 092 - Jaleco JF-19/JF-21
pub struct JalecoJf19Mapper {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl JalecoJf19Mapper {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let num_prg_banks = ctx.prg_rom.len() / (16 * 1024);
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as i16
        } else {
            0
        };
        base.select_prg_page(1, last_bank);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for JalecoJf19Mapper {
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
        if (value & 0x80) != 0 {
            self.prg_bank = value & 0x07;
            self.base.select_prg_page(0, self.prg_bank as i16);
        }
        if (value & 0x40) != 0 {
            self.chr_bank = value & 0x0F;
            self.base.select_chr_page(0, self.chr_bank as i16);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two counts help catch incorrect modulo-wrapping assumptions.
    const PRG_BANKS: usize = 11;
    const CHR_BANKS: usize = 13;

    fn make_test_mapper(mirroring: NametableLayout) -> JalecoJf19Mapper {
        JalecoJf19Mapper::new(MapperContext::new_for_test(
            92,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            mirroring,
        ))
    }

    #[test]
    fn mapper_92_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            92,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 92 must be registered in the factory"
        );
    }

    #[test]
    fn power_on_maps_bank_0_at_8000_and_last_bank_at_c000() {
        let mut mapper = make_test_mapper(NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), (PRG_BANKS - 1) as u8);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn level_sensitive_prg_and_chr_bank_switching_matches_spec_vectors() {
        let mut mapper = make_test_mapper(NametableLayout::Vertical);

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.write_prg(0x8000, 0x82);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.write_prg(0x8000, 0x43);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_chr(0x0000), 3);

        mapper.write_prg(0x8000, 0xC5);
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn prg_updates_while_bit7_stays_high() {
        let mut mapper = make_test_mapper(NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x82);
        mapper.write_prg(0x8000, 0x83);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Mapper 92 must be level-sensitive (not edge-triggered)"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = make_test_mapper(NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0xC5);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore_round_trips_registers() {
        let mut mapper = make_test_mapper(NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0xC5);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_test_mapper(NametableLayout::Vertical);
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_chr(0x0000), 5);
    }

    #[test]
    fn chr_ram_is_used_when_chr_rom_is_absent() {
        let mut mapper = JalecoJf19Mapper::new(MapperContext::new_for_test(
            92,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        mapper.write_chr(0x0123, 0xAB);
        assert_eq!(mapper.read_chr(0x0123), 0xAB);
    }
}
