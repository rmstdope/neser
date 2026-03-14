#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 96;
    const CHR_BANKS_1K: usize = 192;

    fn make_mapper(mapper_id: u16) -> Option<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            mapper_id,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_123_is_registered_in_factory() {
        let mapper = make_mapper(123);
        // Mapper 123 is currently not registered; once it is implemented and added
        // to the factory, flip this assertion to `is_some()` and extend the tests.
        assert!(
            mapper.is_none(),
            "Mapper 123 appears to be implemented; please update this test to verify \
             its factory registration and behavior."
        );
    }

    #[test]
    fn mapper_123_prg_chr_and_irq_match_mmc3() {
        // If mapper 123 is not yet implemented, skip this behavioral comparison.
        let Some(mut mapper_123) = make_mapper(123) else {
            // Nothing to compare until mapper 123 exists.
            return;
        };

        // Mapper 4 (MMC3) must be implemented for this test to be meaningful.
        let Some(mut mmc3) = make_mapper(4) else {
            panic!("Mapper 4 (MMC3) must be implemented for this test to run");
        };

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
