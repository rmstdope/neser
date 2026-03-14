#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 96;
    const CHR_BANKS_1K: usize = 192;

    fn make_mapper(mapper_id: u16) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            mapper_id,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("mapper should be implemented")
    }

    #[test]
    fn mapper_123_is_registered_in_factory() {
        let _mapper = make_mapper(123);
    }

    #[test]
    fn mapper_123_prg_chr_and_irq_match_mmc3() {
        let mut mapper_123 = make_mapper(123);
        let mut mmc3 = make_mapper(4);

        for mapper in [&mut mapper_123, &mut mmc3] {
            mapper.write_prg(0x8000, 0x06);
            mapper.write_prg(0x8001, 0x09);
            mapper.write_prg(0x8000, 0x07);
            mapper.write_prg(0x8001, 0x0B);
            mapper.write_prg(0x8000, 0x80);
            mapper.write_prg(0x8001, 0x14);
            mapper.write_prg(0x8000, 0x02);
            mapper.write_prg(0x8001, 0x21);
        }

        for addr in [0x8000, 0xA000, 0xC000, 0xE000] {
            assert_eq!(mapper_123.read_prg(addr), mmc3.read_prg(addr));
        }
        for addr in [0x0000, 0x0400, 0x0800, 0x1000, 0x1400, 0x1C00] {
            assert_eq!(mapper_123.read_chr(addr), mmc3.read_chr(addr));
        }

        for mapper in [&mut mapper_123, &mut mmc3] {
            mapper.write_prg(0xC000, 0x01);
            mapper.write_prg(0xC001, 0x00);
            mapper.write_prg(0xE001, 0x00);
        }
        for _ in 0..2 {
            mapper_123.ppu_address_changed(0x0FFF);
            mmc3.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper_123.cpu_cycle();
                mmc3.cpu_cycle();
            }
            mapper_123.ppu_address_changed(0x1000);
            mmc3.ppu_address_changed(0x1000);
        }
        assert_eq!(mapper_123.irq_pending(), mmc3.irq_pending());
    }
}
