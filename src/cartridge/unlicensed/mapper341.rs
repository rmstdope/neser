//! Mapper 341 - BMC-TJ-03
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_341>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 341;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const PRG_WRITE_START: u16 = 0x8000;
const BANK_SELECT_SHIFT: u16 = 8;
const BANK_SELECT_MASK: u16 = 0x07;
const MIRRORING_SELECT_BIT: u16 = 0x0002;

pub struct Mapper341 {
    base: BaseMapper,
    bank: u8,
}

impl Mapper341 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        base.set_mirroring(crate::cartridge::NametableLayout::Vertical);

        let mut mapper = Self { base, bank: 0 };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let bank_index = self.bank as i16;
        self.base.select_prg_page(0, bank_index);
        self.base.select_prg_page(1, bank_index);
        self.base.select_chr_page(0, bank_index);
    }

    fn bank_from_write_addr(addr: u16) -> u8 {
        ((addr >> BANK_SELECT_SHIFT) & BANK_SELECT_MASK) as u8
    }

    fn mirroring_hv_from_write_addr(addr: u16) -> bool {
        (addr & MIRRORING_SELECT_BIT) != 0
    }
}

impl Mapper for Mapper341 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if addr < PRG_WRITE_START {
            return;
        }

        self.bank = Self::bank_from_write_addr(addr);
        self.base
            .set_mirroring_hv(Self::mirroring_hv_from_write_addr(addr));
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.bank,
            (matches!(
                self.base.mirroring(),
                crate::cartridge::NametableLayout::Horizontal
            )) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }

        self.bank = data[0] & 0x07;
        self.base.set_mirroring_hv((data[1] & 0x01) != 0);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.base
            .set_mirroring(crate::cartridge::NametableLayout::Vertical);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper341 {
        Mapper341::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, 8),
            banked_data(CHR_BANK_SIZE_BYTES, 8),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_341_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, 8),
            banked_data(CHR_BANK_SIZE_BYTES, 8),
            NametableLayout::Vertical,
        ));

        assert!(result.is_ok(), "Mapper 341 should be registered in factory");
    }

    #[test]
    fn write_address_bits_10_to_8_select_prg_and_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8300, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_chr(0x0000), 3);

        mapper.write_prg(0x8700, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_chr(0x0000), 7);
    }

    #[test]
    fn write_address_bit_1_controls_mirroring() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn power_on_mirroring_is_vertical_even_if_header_is_horizontal() {
        let mapper = Mapper341::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, 8),
            banked_data(CHR_BANK_SIZE_BYTES, 8),
            NametableLayout::Horizontal,
        ));

        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on latch state should default to A1=0 (vertical mirroring)"
        );
    }

    #[test]
    fn capabilities_match_tj03_baseline() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();

        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
