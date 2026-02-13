//! Tests to verify mappers use BankedRom for bank operations.
//!
//! These tests ensure that:
//! 1. Mappers can be migrated to use BankedRom
//! 2. Bank switching behavior is preserved
//! 3. Edge cases (wrapping, bounds) work correctly

#[cfg(test)]
mod tests {
    use crate::cartridge::common::BankedRom;
    use crate::cartridge::test_helpers::banked_data;

    /// Test that BankedRom can replace manual bank offset calculations for ColorDreams
    #[test]
    fn test_colordreams_banked_rom_replacement() {
        const PRG_BANK_SIZE: usize = 32 * 1024;
        const CHR_BANK_SIZE: usize = 8 * 1024;

        // Create test ROM with distinct data per bank
        let prg_rom = banked_data(PRG_BANK_SIZE, 4);
        let chr_rom = banked_data(CHR_BANK_SIZE, 4);

        // Create BankedRom instances like the mapper would
        let prg_banked = BankedRom::new(prg_rom.clone(), PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom.clone(), CHR_BANK_SIZE);

        // Test reading from different banks
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(2, 0), 2);
        assert_eq!(prg_banked.read(3, 0), 3);

        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(1, 0), 1);
        assert_eq!(chr_banked.read(2, 0), 2);
        assert_eq!(chr_banked.read(3, 0), 3);

        // Test bank wrapping for PRG (4 banks)
        assert_eq!(prg_banked.read(4, 0), 0); // wraps to bank 0
        assert_eq!(prg_banked.read(5, 0), 1); // wraps to bank 1
        assert_eq!(prg_banked.read(7, 0), 3); // wraps to bank 3
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to bank 0
    }

    /// Test that BankedRom can replace manual bank offset calculations for BNROM
    #[test]
    fn test_bnrom_banked_rom_replacement() {
        const PRG_BANK_SIZE: usize = 0x8000; // 32KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 4);
        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);

        // Test basic bank reading
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(2, 0), 2);
        assert_eq!(prg_banked.read(3, 0), 3);

        // Test bank wrapping
        assert_eq!(prg_banked.read(4, 0), 0);
        assert_eq!(prg_banked.read(7, 0), 3);
    }

    /// Test that BankedRom can replace manual bank offset calculations for Nina-Tengen
    #[test]
    fn test_nina_tengen_banked_rom_replacement() {
        const PRG_BANK_SIZE: usize = 0x4000; // 16KB
        const CHR_BANK_SIZE: usize = 0x2000; // 8KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 8);
        let chr_rom = banked_data(CHR_BANK_SIZE, 16);

        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom, CHR_BANK_SIZE);

        // Test PRG bank reading
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(7, 0), 7);

        // Test CHR bank reading
        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(15, 0), 15);

        // Test last bank wrapping
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to 0
        assert_eq!(chr_banked.read(16, 0), 0); // wraps to 0
    }

    /// Test that BankedRom handles Bandai FCG's 16KB PRG banks
    #[test]
    fn test_bandai_fcg_banked_rom_replacement() {
        const PRG_BANK_SIZE: usize = 16 * 1024; // 16KB
        const CHR_BANK_SIZE: usize = 1024; // 1KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 16);
        let chr_rom = banked_data(CHR_BANK_SIZE, 128);

        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom, CHR_BANK_SIZE);

        // Test PRG banks
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(15, 0), 15);

        // Test CHR banks
        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(127, 0), 127);

        // Test wrapping
        assert_eq!(prg_banked.read(16, 0), 0);
        assert_eq!(chr_banked.read(128, 0), 0);
    }

    /// Test that BankedRom works with read_with_base for addressing
    #[test]
    fn test_banked_rom_with_cpu_addressing() {
        const PRG_BANK_SIZE: usize = 0x4000; // 16KB
        let rom = banked_data(PRG_BANK_SIZE, 8);
        let banked = BankedRom::new(rom, PRG_BANK_SIZE);

        // Test reading from $8000-$BFFF with bank 0
        assert_eq!(banked.read_with_base(0, 0x8000, 0x8000), 0);
        assert_eq!(banked.read_with_base(0, 0x8000, 0x8001), 0);
        assert_eq!(banked.read_with_base(0, 0x8000, 0xBFFF), 0);

        // Test reading from $8000-$BFFF with bank 3
        assert_eq!(banked.read_with_base(3, 0x8000, 0x8000), 3);
        assert_eq!(banked.read_with_base(3, 0x8000, 0x8001), 3);
    }

    /// Test handling of empty ROMs with BankedRom
    #[test]
    fn test_banked_rom_empty_rom() {
        const PRG_BANK_SIZE: usize = 0x4000;
        let empty_rom = vec![];
        let banked = BankedRom::new(empty_rom, PRG_BANK_SIZE);

        // Should handle gracefully
        assert_eq!(banked.num_banks(), 0);
        assert_eq!(banked.read(0, 0), 0);
        assert_eq!(banked.read(1, 0), 0);
    }

    /// Test that BankedRom handles out-of-bounds reads gracefully
    #[test]
    fn test_banked_rom_bounds_checking() {
        const BANK_SIZE: usize = 1024;
        let rom = banked_data(BANK_SIZE, 4);
        let banked = BankedRom::new(rom, BANK_SIZE);

        // Should read valid data within bank
        assert_eq!(banked.read(0, 0), 0);
        assert_eq!(banked.read(0, BANK_SIZE - 1), 0);
        assert_eq!(banked.read(3, BANK_SIZE - 1), 3);

        // Reading with offset beyond bank size still works (just reads from later in ROM)
        // Since we have 4 banks of 1024 bytes each = 4096 total
        // read(0, 2048) = index 2048 = start of bank 2, value = 2
        assert_eq!(banked.read(0, BANK_SIZE * 2), 2);

        // Reading way beyond total ROM should return 0
        assert_eq!(banked.read(0, 10000), 0);
        assert_eq!(banked.read(99, 10000), 0);
    }
}
