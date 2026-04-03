//! Mapper 220 — FCEUX Debug Mapper
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_220>
//!
//! iNES Mapper 220 is used by FCEUX as a debugging mapper assignment and should
//! not be assigned to ROMs for regular usage.  No physical cartridge hardware
//! corresponds to this mapper number.  The implementation mirrors the simplest
//! possible fixed-bank layout (NROM-like): 32 KiB PRG-ROM fixed at $8000–$FFFF
//! and up to 8 KiB CHR fixed at $0000–$1FFF, with mirroring taken from the
//! iNES header.
//!
//! Known Limitations:
//! - No banking registers; writes to the PRG address space are silently ignored.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::Mapper;

const MAPPER_NUMBER: u16 = 220;

pub struct Mapper220 {
    base: BaseMapper,
}

impl Mapper220 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mut base = BaseMapper::new(&ctx, Default::default());
        base.set_mirroring(ctx.mirroring);
        Self { base }
    }
}

impl Mapper for Mapper220 {
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper(mirroring: NametableLayout) -> Mapper220 {
        Mapper220::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, 2),
            banked_data(8 * 1024, 1),
            mirroring,
        ))
    }

    #[test]
    fn mapper_220_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, 2),
            banked_data(8 * 1024, 1),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 220 should be registered in factory");
    }

    #[test]
    fn reports_correct_mapper_number() {
        let mapper = make_mapper(NametableLayout::Horizontal);
        assert_eq!(mapper.mapper_number(), MAPPER_NUMBER);
    }

    #[test]
    fn fixed_prg_layout_lower_upper_banks() {
        let mapper = make_mapper(NametableLayout::Vertical);
        // Bank 0 at $8000
        assert_eq!(mapper.read_prg(0x8000), 0);
        // Bank 1 (last 16 KiB) at $C000
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn writes_do_not_change_prg_banking() {
        let mut mapper = make_mapper(NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn fixed_chr_layout() {
        let mut mapper = make_mapper(NametableLayout::Vertical);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn uses_header_mirroring() {
        let vertical = make_mapper(NametableLayout::Vertical);
        assert_eq!(vertical.get_mirroring(), NametableLayout::Vertical);

        let horizontal = make_mapper(NametableLayout::Horizontal);
        assert_eq!(horizontal.get_mirroring(), NametableLayout::Horizontal);

        let single_lower = make_mapper(NametableLayout::SingleScreenLower);
        assert_eq!(
            single_lower.get_mirroring(),
            NametableLayout::SingleScreenLower
        );

        let single_upper = make_mapper(NametableLayout::SingleScreenUpper);
        assert_eq!(
            single_upper.get_mirroring(),
            NametableLayout::SingleScreenUpper
        );
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
