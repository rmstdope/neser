//! Mapper 342 - COOLGIRL multicart (minimal baseline)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_342>
//!
//! Known Limitations:
//! - This is a minimal baseline implementation that only supports fixed
//!   PRG/CHR mapping for mapper instantiation and basic execution.
//! - COOLGIRL mapper-code emulation modes, advanced banking, and flash behavior
//!   are not implemented yet.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 342;
const PRG_BANK_SIZE_BYTES: usize = 32 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const MAX_PRG_RAM_KB: usize = 32;

/// Mapper 342 - COOLGIRL multicart.
///
/// This initial implementation provides only fixed mapping behavior so that
/// Mapper 342 ROMs can instantiate.
pub struct Mapper342 {
    base: BaseMapper,
}

impl Mapper342 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: MAX_PRG_RAM_KB,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        Self { base }
    }
}

impl Mapper for Mapper342 {
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
        // Delegate potential PRG-RAM writes ($6000-$7FFF) to the base mapper.
        self.base.try_write_prg_ram(addr, value);
        // No mapper-specific register handling in baseline implementation.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn mapper_342_context(prg_bank_count: usize, chr_bank_count: usize) -> MapperContext {
        MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, prg_bank_count),
            banked_data(CHR_BANK_SIZE_BYTES, chr_bank_count),
            NametableLayout::Vertical,
        )
    }

    #[test]
    fn mapper_342_is_registered() {
        let result = create_mapper(mapper_342_context(4, 4));
        assert!(result.is_ok(), "Mapper 342 should be registered");
    }

    #[test]
    fn mapper_342_reports_mapper_number() {
        let mapper = Mapper342::new(mapper_342_context(2, 2));
        assert_eq!(mapper.mapper_number(), MAPPER_NUMBER);
    }
}
