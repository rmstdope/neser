//! Mapper 097 - Irem TAM-S1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_097>
//! - Fallback: MAME `nes_tam_s1_device` implementation in `irem.cpp`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::NametableLayout;

pub struct Mapper97 {
    base: BaseMapper,
    c000_bank: u8,
    initial_mirroring: NametableLayout,
}

impl Mapper97 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        // Irem TAM-S1: $8000-$BFFF fixed to last bank, $C000-$FFFF switchable (power-on bank 0).
        base.select_prg_page(0, -1);
        base.select_prg_page(1, 0);
        Self {
            base,
            c000_bank: 0,
            initial_mirroring: ctx.mirroring,
        }
    }

    fn update_c000_bank(&mut self) {
        self.base.select_prg_page(1, self.c000_bank as i16);
    }
}

impl Mapper for Mapper97 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x8000..=0xBFFF).contains(&addr) {
            return;
        }

        self.c000_bank = value & 0x3F;
        self.update_c000_bank();
        let mirroring = if (value & 0x80) != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
        self.base.set_mirroring(mirroring);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.c000_bank, self.base.mirroring().to_snapshot_byte()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.c000_bank = bank;
            self.update_c000_bank();
        }
        if let Some(&mirroring) = data.get(1) {
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(mirroring));
        }
    }

    fn reset(&mut self) {
        self.c000_bank = 0;
        self.update_c000_bank();
        self.base.set_mirroring(self.initial_mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 5;

    fn make_mapper() -> Mapper97 {
        Mapper97::new(MapperContext::new_for_test(
            97,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_97_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            97,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 97 should be registered in factory");
    }

    #[test]
    fn power_on_maps_last_bank_at_8000_and_bank0_at_c000() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), (PRG_BANKS - 1) as u8);
        assert_eq!(mapper.read_prg(0xC000), 0);
    }

    #[test]
    fn write_in_8000_bfff_selects_c000_bank_and_mirroring_from_bit7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x82);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xBFFF, 0x01);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_in_c000_ffff_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x83);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xC000, 0x00);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn prg_bank_select_uses_lower_6_bits() {
        let mut mapper = make_mapper();
        // 0x22 uses bit 5. With 5 PRG banks: 0x22 & 0x3F = 34, and 34 % 5 = 4.
        // The previous 0x0F mask would produce 0x02, and 2 % 5 = 2.
        mapper.write_prg(0x8000, 0x22);
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    #[test]
    fn registers_snapshot_restore_and_reset_roundtrip_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x82);
        let snapshot = mapper.registers_snapshot();

        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.restore_registers(&snapshot);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.reset();
        assert_eq!(mapper.read_prg(0xC000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }
}
