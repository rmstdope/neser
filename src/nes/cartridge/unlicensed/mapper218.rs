//! Mapper 218 - Magic Floor
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_218>
//! - Mesen reference: `MagicFloor218`
//!
//! Known Limitations:
//! - Four-screen mirroring metadata is coerced to single-screen-lower because
//!   mapper context does not preserve iNES flag 6 bit 0 alongside `FourScreen`.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::Mapper;

const MAPPER_NUMBER: u16 = 218;

pub struct Mapper218 {
    base: BaseMapper,
}

impl Mapper218 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mut base = BaseMapper::new(&ctx, Default::default());
        base.set_mirroring(Self::resolve_mirroring(ctx.mirroring));
        Self { base }
    }

    fn resolve_mirroring(mirroring: NametableLayout) -> NametableLayout {
        match mirroring {
            NametableLayout::FourScreen => NametableLayout::SingleScreenLower,
            other => other,
        }
    }
}

impl Mapper for Mapper218 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let _ = self.base.try_write_prg_ram(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_mapper(mirroring: NametableLayout) -> Mapper218 {
        Mapper218::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, 2),
            banked_data(8 * 1024, 1),
            mirroring,
        ))
    }

    #[test]
    fn mapper_218_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, 2),
            banked_data(8 * 1024, 1),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 218 should be registered in factory");
    }

    #[test]
    fn behaves_like_fixed_prg_chr_layout() {
        let mut mapper = make_mapper(NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.write_prg(0x8000, 0xFF);

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn uses_header_mirroring_for_standard_layouts() {
        let vertical = make_mapper(NametableLayout::Vertical);
        assert_eq!(vertical.get_mirroring(), NametableLayout::Vertical);

        let horizontal = make_mapper(NametableLayout::Horizontal);
        assert_eq!(horizontal.get_mirroring(), NametableLayout::Horizontal);

        let upper = make_mapper(NametableLayout::SingleScreenUpper);
        assert_eq!(upper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn four_screen_metadata_maps_to_single_screen_lower() {
        let mapper = make_mapper(NametableLayout::FourScreen);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn reports_no_irq_and_no_expansion_audio() {
        let mapper = make_mapper(NametableLayout::Vertical);
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(!caps.has_chr_banking);
        assert!(!caps.has_dynamic_mirroring);
    }
}
