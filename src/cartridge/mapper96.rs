//! Mapper 096 - Oeka Kids
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_096>

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_PAGE_MASK: u8 = 0x03;
const CHR_BANKS_PER_PAGE_64K: i16 = 8;

pub struct Mapper96 {
    base: BaseMapper,
    chr_page: u8,
}

impl Mapper96 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        Self { base, chr_page: 0 }
    }

    fn update_chr_bank(&mut self) {
        self.base
            .select_chr_page(0, (self.chr_page as i16) * CHR_BANKS_PER_PAGE_64K);
    }
}

impl Mapper for Mapper96 {
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
        self.chr_page = value & CHR_PAGE_MASK;
        self.update_chr_bank();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_page]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&chr_page) = data.first() {
            self.chr_page = chr_page & CHR_PAGE_MASK;
            self.update_chr_bank();
        }
    }

    fn reset(&mut self) {
        self.chr_page = 0;
        self.update_chr_bank();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 3;
    const CHR_BANKS_64K: usize = 2;

    fn make_mapper_direct() -> Mapper96 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(64 * 1024, CHR_BANKS_64K);
        Mapper96::new(MapperContext::new_for_test(
            96,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_96_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(64 * 1024, CHR_BANKS_64K);
        let result = create_mapper(MapperContext::new_for_test(
            96,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 96 must be registered in the factory"
        );
    }

    #[test]
    fn prg_reads_through_single_32k_bank() {
        let mapper = make_mapper_direct();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn chr_page_switches_via_write_bits_1_0() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0, "write 0 selects CHR page 0");
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 1, "write 1 selects CHR page 1");
    }

    #[test]
    fn chr_page_selection_uses_low_2_bits_with_rom_size_wrapping() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "value 0x02 wraps to CHR page 0 with 2 total pages"
        );
    }

    #[test]
    fn registers_snapshot_restore_and_reset_roundtrip_chr_page() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 0x01);
        let snapshot = mapper.registers_snapshot();

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0, "state changed before restore");

        mapper.restore_registers(&snapshot);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "restored snapshot returns CHR page 1 mapping"
        );

        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 0, "reset returns to CHR page 0");
    }
}
