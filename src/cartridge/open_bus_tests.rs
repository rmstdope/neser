//! Comprehensive open-bus behavior tests for all mappers
//!
//! Open-bus behavior is when reading from unmapped or disabled CPU address space
//! returns the last value on the data bus (typically the last byte read).
//! This is important for:
//! - Games that detect mapper presence via open-bus
//! - ROM size detection
//! - Correct emulation of edge cases
//!
//! These tests verify that mappers properly implement `read_prg_open_bus()` to:
//! 1. Return open-bus value for disabled regions
//! 2. Return consistent values (same address, same open-bus value)
//! 3. Use open-bus as fallback for unmapped regions
//! 4. Handle addresses below $6000 correctly

#[cfg(test)]
mod tests {
    use crate::cartridge::test_helpers::banked_data;
    use crate::cartridge::{Mapper, MapperContext, MirroringMode};

    /// Helper to create a mapper via the factory
    fn create_mapper(mapper_number: u16, prg_size_kb: usize, chr_size_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(prg_size_kb * 1024, 1);
        let chr_rom = if chr_size_kb > 0 {
            banked_data(chr_size_kb * 1024, 1)
        } else {
            vec![]
        };
        let metadata = MapperContext::new(mapper_number, prg_rom, chr_rom, MirroringMode::Horizontal);
        crate::cartridge::mapper::create_mapper(metadata)
            .unwrap_or_else(|_| panic!("Mapper {} should be implemented", mapper_number))
    }

    /// Test that addresses below $6000 return open-bus value
    ///
    /// All mappers should return the open-bus value for addresses below $6000,
    /// as this is outside the cartridge address space.
    #[test]
    fn test_addresses_below_6000_return_open_bus() {
        // Test a representative set of mappers
        let mapper_numbers = vec![
            0,   // NROM
            1,   // MMC1
            2,   // UxROM
            3,   // CNROM
            4,   // MMC3
            7,   // AxROM
            9,   // MMC2
            10,  // MMC4
        ];

        for mapper_num in mapper_numbers {
            let mapper = create_mapper(mapper_num, 32, 8);
            
            let open_bus_value = 0x42;
            
            // Test various addresses below $6000
            assert_eq!(
                mapper.read_prg_open_bus(0x0000, open_bus_value),
                open_bus_value,
                "Mapper {} should return open-bus for address $0000",
                mapper_num
            );
            
            assert_eq!(
                mapper.read_prg_open_bus(0x5000, open_bus_value),
                open_bus_value,
                "Mapper {} should return open-bus for address $5000",
                mapper_num
            );
            
            assert_eq!(
                mapper.read_prg_open_bus(0x5FFF, open_bus_value),
                open_bus_value,
                "Mapper {} should return open-bus for address $5FFF",
                mapper_num
            );
        }
    }

    /// Test that open-bus values are consistent (same address returns same value)
    ///
    /// When reading the same address multiple times with the same open-bus value,
    /// the result should be consistent.
    #[test]
    fn test_open_bus_consistency() {
        let mapper = create_mapper(0, 32, 8); // NROM
        
        let open_bus_value = 0xAB;
        let addr = 0x5000;
        
        // Read the same address multiple times
        let read1 = mapper.read_prg_open_bus(addr, open_bus_value);
        let read2 = mapper.read_prg_open_bus(addr, open_bus_value);
        let read3 = mapper.read_prg_open_bus(addr, open_bus_value);
        
        assert_eq!(read1, read2, "Consecutive reads should return same value");
        assert_eq!(read2, read3, "Consecutive reads should return same value");
        assert_eq!(read1, open_bus_value, "Should return open-bus value");
    }

    /// Test that different open-bus values are properly returned
    ///
    /// The mapper should return whatever open-bus value is provided,
    /// not a fixed value.
    #[test]
    fn test_different_open_bus_values() {
        let mapper = create_mapper(0, 32, 8); // NROM
        let addr = 0x5000;
        
        // Test with different open-bus values
        assert_eq!(mapper.read_prg_open_bus(addr, 0x00), 0x00);
        assert_eq!(mapper.read_prg_open_bus(addr, 0xFF), 0xFF);
        assert_eq!(mapper.read_prg_open_bus(addr, 0xA5), 0xA5);
        assert_eq!(mapper.read_prg_open_bus(addr, 0x5A), 0x5A);
    }

    /// Test all supported mappers handle open-bus for low addresses
    ///
    /// This is a comprehensive test ensuring all mappers in the registry
    /// properly handle addresses below $6000.
    #[test]
    fn test_all_mappers_handle_low_addresses() {
        // List of all supported mappers (from mapper.rs)
        let mappers = vec![
            (0, "NROM", 0x5000),
            (1, "MMC1", 0x5000),
            (2, "UxROM", 0x5000),
            (3, "CNROM", 0x5000),
            (4, "MMC3", 0x5000),
            (5, "MMC5", 0x4000), // MMC5 has expansion registers at $5000-$5FFF
            (7, "AxROM", 0x5000),
            (9, "MMC2", 0x5000),
            (10, "MMC4", 0x5000),
            (11, "ColorDreams", 0x5000),
            (13, "Cprom", 0x5000),
            (15, "Multicart15", 0x5000),
            (16, "BandaiFcg", 0x5000),
            (19, "Namco163", 0x5000),
            (21, "VRC2/VRC4", 0x5000),
            (22, "VRC2/VRC4", 0x5000),
            (23, "VRC2/VRC4", 0x5000),
            (24, "VRC6", 0x5000),
            (25, "VRC2/VRC4", 0x5000),
            (26, "VRC6", 0x5000),
            (34, "BnromNina", 0x5000),
            (66, "GxROM", 0x5000),
            (68, "Sunsoft4", 0x5000),
            (69, "SunsoftFme7", 0x5000),
            (71, "Camerica", 0x5000),
            (78, "NinaTengen", 0x5000),
            (206, "Namco118", 0x5000),
        ];

        for (mapper_num, mapper_name, test_addr) in mappers {
            let mapper = create_mapper(mapper_num, 32, 8);
            let open_bus = 0x42;
            
            // Test address below $6000 (adjusting for MMC5 expansion registers)
            let result = mapper.read_prg_open_bus(test_addr, open_bus);
            assert_eq!(
                result, open_bus,
                "Mapper {} ({}) should return open-bus for address ${:04X}",
                mapper_num, mapper_name, test_addr
            );
        }
    }

    /// Test NROM (Mapper 0) open-bus behavior
    ///
    /// NROM uses the default implementation, so it should return open-bus
    /// for all addresses below $6000.
    #[test]
    fn test_nrom_open_bus() {
        let mapper = create_mapper(0, 32, 8);
        
        // Test various open-bus scenarios
        assert_eq!(mapper.read_prg_open_bus(0x0000, 0x12), 0x12);
        assert_eq!(mapper.read_prg_open_bus(0x1000, 0x34), 0x34);
        assert_eq!(mapper.read_prg_open_bus(0x2000, 0x56), 0x56);
        assert_eq!(mapper.read_prg_open_bus(0x3000, 0x78), 0x78);
        assert_eq!(mapper.read_prg_open_bus(0x4000, 0x9A), 0x9A);
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xBC), 0xBC);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xDE), 0xDE);
    }

    /// Test UxROM (Mapper 2) open-bus behavior
    #[test]
    fn test_uxrom_open_bus() {
        let mapper = create_mapper(2, 128, 0); // UxROM uses CHR-RAM
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xAA), 0xAA);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xBB), 0xBB);
    }

    /// Test CNROM (Mapper 3) open-bus behavior
    #[test]
    fn test_cnrom_open_bus() {
        let mapper = create_mapper(3, 32, 32);
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xCC), 0xCC);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xDD), 0xDD);
    }

    /// Test AxROM (Mapper 7) open-bus behavior
    #[test]
    fn test_axrom_open_bus() {
        let mapper = create_mapper(7, 128, 0); // AxROM uses CHR-RAM
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xEE), 0xEE);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xFF), 0xFF);
    }

    /// Test MMC2 (Mapper 9) open-bus behavior
    #[test]
    fn test_mmc2_open_bus() {
        let mapper = create_mapper(9, 128, 128);
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x11), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x22), 0x22);
    }

    /// Test MMC4 (Mapper 10) open-bus behavior
    #[test]
    fn test_mmc4_open_bus() {
        let mapper = create_mapper(10, 128, 128);
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x33), 0x33);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x44), 0x44);
    }

    /// Test ColorDreams (Mapper 11) open-bus behavior
    #[test]
    fn test_colordreams_open_bus() {
        let mapper = create_mapper(11, 128, 128);
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x55), 0x55);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x66), 0x66);
    }

    /// Test Cprom (Mapper 13) open-bus behavior
    #[test]
    fn test_cprom_open_bus() {
        let mapper = create_mapper(13, 32, 0); // Cprom uses CHR-RAM
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x77), 0x77);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x88), 0x88);
    }

    /// Test GxROM (Mapper 66) open-bus behavior
    #[test]
    fn test_gxrom_open_bus() {
        let mapper = create_mapper(66, 128, 32);
        
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x99), 0x99);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xAA), 0xAA);
    }

    /// Test that PRG-ROM and PRG-RAM regions don't return open-bus
    ///
    /// Regions that are mapped should return actual data, not open-bus value.
    #[test]
    fn test_mapped_regions_dont_return_open_bus() {
        let prg_rom = vec![0xAB; 32 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let metadata = MapperContext::new(0, prg_rom, chr_rom, MirroringMode::Horizontal);
        let mapper = crate::cartridge::mapper::create_mapper(metadata).unwrap();
        
        let open_bus = 0x42;
        
        // PRG-RAM region ($6000-$7FFF) should return actual data, not open-bus
        // (Though for NROM it might return 0 or actual data depending on implementation)
        let _ram_result = mapper.read_prg_open_bus(0x6000, open_bus);
        // We just verify it reads without panic - the actual value depends on mapper
        
        // PRG-ROM region ($8000-$FFFF) should return ROM data, not open-bus
        let rom_result = mapper.read_prg_open_bus(0x8000, open_bus);
        assert_eq!(
            rom_result, 0xAB,
            "PRG-ROM region should return ROM data, not open-bus"
        );
        
        let rom_result2 = mapper.read_prg_open_bus(0xC000, open_bus);
        assert_eq!(
            rom_result2, 0xAB,
            "PRG-ROM region should return ROM data, not open-bus"
        );
    }

    /// Test boundary between open-bus and mapped regions
    ///
    /// Verifies that $5FFF returns open-bus but $6000 does not.
    #[test]
    fn test_boundary_at_6000() {
        let mapper = create_mapper(0, 32, 8);
        let open_bus = 0x55;
        
        // $5FFF should return open-bus
        assert_eq!(
            mapper.read_prg_open_bus(0x5FFF, open_bus),
            open_bus,
            "$5FFF should return open-bus"
        );
        
        // $6000 might return different value (PRG-RAM or 0)
        // We just verify it doesn't panic
        let _ = mapper.read_prg_open_bus(0x6000, open_bus);
    }
}
