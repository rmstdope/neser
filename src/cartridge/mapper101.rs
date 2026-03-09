//! Mapper 101 – Jaleco JF-10

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper101 {
    base: BaseMapper,
    chr_bank: u8,
}

impl Mapper101 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, chr_bank: 0 };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper101 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x6000..=0x7FFF).contains(&addr) {
            return;
        }
        self.chr_bank = value & 0x0F;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(chr_bank) = data.first() {
            self.chr_bank = chr_bank & 0x0F;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
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

    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper101 {
        Mapper101::new(MapperContext::new_for_test(
            101,
            vec![0xAB; 32 * 1024],
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_101_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            101,
            vec![0xCD; 32 * 1024],
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 101 must be registered in the factory"
        );
    }

    #[test]
    fn chr_bank_select_uses_low_nibble_for_6000_to_7fff_writes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x02);
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.write_prg(0x7FFF, 0x04);
        assert_eq!(mapper.read_chr(0x0000), 4);
    }

    #[test]
    fn prg_reads_are_unaffected_by_chr_bank_writes() {
        let mut mapper = make_mapper();
        let before_8000 = mapper.read_prg(0x8000);
        let before_c000 = mapper.read_prg(0xC000);

        mapper.write_prg(0x6000, 0x0F);
        mapper.write_prg(0x7000, 0x03);
        mapper.write_prg(0x7FFF, 0x09);

        assert_eq!(mapper.read_prg(0x8000), before_8000);
        assert_eq!(mapper.read_prg(0xC000), before_c000);
    }

    #[test]
    fn snapshot_restore_preserves_selected_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03);

        let snapshot = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_chr(0x0000), 3);
    }
}
