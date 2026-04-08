//! Mapper 041 - Caltron 6-in-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_041>
//!
//! Known Limitations:
//! - Bus-conflict nuances are not modeled beyond normal write-path behavior.

use crate::cartridge::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 041 - Caltron 6-in-1
pub struct Mapper41 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl Mapper41 {
    const OUTER_REGISTER_START: u16 = 0x6000;
    const OUTER_REGISTER_END: u16 = 0x67FF;
    const INNER_REGISTER_START: u16 = 0x8000;
    const INNER_REGISTER_END: u16 = 0xFFFF;

    const PRG_BANK_MASK: u16 = 0x0007;
    const CHR_OUTER_FROM_ADDR_MASK: u16 = 0x000C;
    const CHR_INNER_MASK: u8 = 0x03;
    const CHR_OUTER_MASK: u8 = 0x0C;
    const MIRROR_HORIZONTAL_ADDR_BIT: u16 = 0x0020;
    const INNER_ENABLE_PRG_THRESHOLD: u8 = 4;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_mapping();
        mapper
    }

    fn update_mapping(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper41 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (Self::OUTER_REGISTER_START..=Self::OUTER_REGISTER_END).contains(&addr) {
            self.prg_bank = (addr & Self::PRG_BANK_MASK) as u8;
            self.chr_bank = (self.chr_bank & Self::CHR_INNER_MASK)
                | (((addr >> 1) & Self::CHR_OUTER_FROM_ADDR_MASK) as u8);
            self.base
                .set_mirroring_hv((addr & Self::MIRROR_HORIZONTAL_ADDR_BIT) != 0);
            self.update_mapping();
            return;
        }

        if (Self::INNER_REGISTER_START..=Self::INNER_REGISTER_END).contains(&addr)
            && self.prg_bank >= Self::INNER_ENABLE_PRG_THRESHOLD
        {
            self.chr_bank = (self.chr_bank & Self::CHR_OUTER_MASK) | (value & Self::CHR_INNER_MASK);
            self.update_mapping();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirroring_h = matches!(
            self.base.mirroring(),
            crate::cartridge::NametableLayout::Horizontal
        );
        vec![self.prg_bank, self.chr_bank, mirroring_h as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            if data.len() >= 3 {
                self.base.set_mirroring_hv(data[2] != 0);
            }
            self.update_mapping();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.base
            .set_mirroring(crate::cartridge::NametableLayout::Vertical);
        self.update_mapping();
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper41;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 11;
    const CHR_BANKS_8K: usize = 13;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            41,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 41 should be implemented")
    }

    fn make_mapper_direct() -> Mapper41 {
        Mapper41::new(MapperContext::new_for_test(
            41,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_41_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            41,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 41 must be registered in the factory"
        );
    }

    #[test]
    fn outer_register_selects_prg_bank_from_address_bits_0_to_2() {
        let mut mapper = make_mapper_direct();

        mapper.write_prg(0x6005, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn outer_register_selects_chr_outer_bits_from_address() {
        let mut mapper = make_mapper_direct();

        // addr=0x6008 -> ((addr >> 1) & 0x0C) == 4, inner=0 => CHR bank 4
        mapper.write_prg(0x6008, 0x00);

        assert_eq!(mapper.read_chr(0x0000), 4);
    }

    #[test]
    fn inner_chr_register_only_updates_when_prg_bank_is_4_to_7() {
        let mut mapper = make_mapper_direct();

        mapper.write_prg(0x6001, 0x00);
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 0, "inner write must be ignored");

        mapper.write_prg(0x6004, 0x00);
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "inner write must apply");
    }

    #[test]
    fn outer_register_controls_mirroring_with_address_bit_5() {
        let mut mapper = make_mapper_direct();

        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x6020, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn reset_clears_outer_and_inner_register_effects() {
        let mut mapper = make_mapper_direct();

        mapper.write_prg(0x6027, 0x00);
        mapper.write_prg(0x8000, 0x03);
        assert_ne!(mapper.read_prg(0x8000), 0);
        assert_ne!(mapper.read_chr(0x0000), 0);

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn snapshot_and_restore_roundtrip_register_state() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6024, 0x00);
        mapper.write_prg(0x8000, 0x02);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper_direct();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }

    #[test]
    fn factory_mapper_exposes_same_behavior_surface() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6024, 0x00);
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_chr(0x0000), 1);
    }
}
