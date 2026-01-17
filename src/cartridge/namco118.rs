#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge::MirroringMode;
    use crate::cartridge::mapper::{create_mapper, Mapper};

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    #[test]
    fn namco118_prg_chr_banking_matches_mmc3_subset() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper(206, prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("Mapper 206 should be implemented");

        // PRG mode 0 (bit 6 clear): R6 @ $8000, R7 @ $A000, fixed second-last @ $C000, last @ $E000.
        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // Switch to PRG mode 1 (bit 6 set): fixed second-last @ $8000, R7 @ $A000, R6 @ $C000, fixed last @ $E000.
        mapper.write_prg(0x8000, 0b0100_0110); // select R6 with PRG mode 1
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR mode 0 (bit 7 clear): R0/1 are 2KB even-aligned, R2-5 are 1KB.
        mapper.write_prg(0x8000, 0b0000_0000); // select R0, CHR mode 0
        mapper.write_prg(0x8001, 4); // R0 maps banks 4+5 at $0000-$07FF
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 6); // R1 maps banks 6+7 at $0800-$0FFF

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);
    }

    #[test]
    fn namco118_mirroring_and_irq_registers_are_noops() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper(206, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Mapper 206 should be implemented");

        // Mirroring should stay hardwired to the cartridge header; writes to $A000 must not change it.
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // IRQ-related registers should have no effect; mapper never asserts IRQ.
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE000, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..3 {
            mapper.ppu_address_changed(0x1000);
            mapper.ppu_scanline(0, true);
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }
    }
}
