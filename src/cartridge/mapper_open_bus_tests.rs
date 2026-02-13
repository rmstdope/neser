//! Mapper-specific open-bus behavior tests
//!
//! Tests for mappers that have custom open-bus implementations:
//! - MMC1 (disabled WRAM)
//! - MMC3 (disabled PRG-RAM)
//! - MMC5 (expansion registers, ExRAM modes, disabled regions)

#[cfg(test)]
mod tests {
    use crate::cartridge::test_helpers::banked_data;
    use crate::cartridge::{Mapper, MapperContext, MirroringMode};

    /// Helper to create an MMC1 mapper
    fn create_mmc1(prg_size_kb: usize, chr_size_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(prg_size_kb * 1024, 1);
        let chr_rom = if chr_size_kb > 0 {
            banked_data(chr_size_kb * 1024, 1)
        } else {
            vec![]
        };
        let metadata = MapperContext::new(1, prg_rom, chr_rom, MirroringMode::Horizontal);
        crate::cartridge::mapper::create_mapper(metadata).expect("MMC1 should be supported")
    }

    /// Helper to create an MMC3 mapper
    fn create_mmc3(prg_size_kb: usize, chr_size_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(prg_size_kb * 1024, 1);
        let chr_rom = if chr_size_kb > 0 {
            banked_data(chr_size_kb * 1024, 1)
        } else {
            vec![]
        };
        let metadata = MapperContext::new(4, prg_rom, chr_rom, MirroringMode::Horizontal);
        crate::cartridge::mapper::create_mapper(metadata).expect("MMC3 should be supported")
    }

    /// Helper to create an MMC5 mapper
    fn create_mmc5(prg_size_kb: usize, chr_size_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(prg_size_kb * 1024, 1);
        let chr_rom = if chr_size_kb > 0 {
            banked_data(chr_size_kb * 1024, 1)
        } else {
            vec![]
        };
        let metadata = MapperContext::new(5, prg_rom, chr_rom, MirroringMode::Horizontal);
        crate::cartridge::mapper::create_mapper(metadata).expect("MMC5 should be supported")
    }

    /// Test MMC1 disabled WRAM returns open-bus
    ///
    /// MMC1B/C can disable WRAM via bit 4 of the PRG bank register ($E000-$FFFF).
    /// When disabled, reads from $6000-$7FFF should return open-bus.
    #[test]
    fn test_mmc1_disabled_wram_returns_open_bus() {
        let mut mapper = create_mmc1(256, 0);
        
        // First, enable WRAM and write some data
        // Write to $E000-$FFFF controls PRG banking and WRAM enable
        // We need to do 5 consecutive writes to load the shift register
        
        // Reset shift register
        mapper.write_prg(0x8000, 0x80);
        
        // Write pattern to enable WRAM: bit 4 = 0
        // We'll write 5 times with bit 0 = 0 each time to load 0x00 into register
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0x00);
        }
        
        // Write to WRAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7000, 0xBB);
        
        // Verify WRAM reads work when enabled
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7000), 0xBB);
        
        // Now disable WRAM by setting bit 4 = 1 in PRG bank register
        // Reset shift register
        mapper.write_prg(0x8000, 0x80);
        
        // Write pattern to disable WRAM: bit 4 = 1
        // We need to shift in 10000 binary = 0x10
        // Shift register is filled LSB first, so: bit0, bit1, bit2, bit3, bit4
        mapper.write_prg(0xE000, 0x00); // bit 0 = 0
        mapper.write_prg(0xE000, 0x00); // bit 1 = 0
        mapper.write_prg(0xE000, 0x00); // bit 2 = 0
        mapper.write_prg(0xE000, 0x00); // bit 3 = 0
        mapper.write_prg(0xE000, 0x01); // bit 4 = 1
        
        // read_prg should return 0 for backward compatibility
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x7000), 0x00);
        
        // read_prg_open_bus should return the open-bus value
        let open_bus = 0x42;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $6000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7000, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $7000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $7FFF"
        );
    }

    /// Test MMC1 enabled WRAM doesn't return open-bus
    #[test]
    fn test_mmc1_enabled_wram_returns_data() {
        let mut mapper = create_mmc1(256, 0);
        
        // Reset and ensure WRAM is enabled (bit 4 = 0)
        mapper.write_prg(0x8000, 0x80);
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0x00);
        }
        
        // Write to WRAM
        mapper.write_prg(0x6000, 0x55);
        
        let open_bus = 0xFF;
        let result = mapper.read_prg_open_bus(0x6000, open_bus);
        
        // Should return the written value, not open-bus
        assert_eq!(
            result, 0x55,
            "Enabled WRAM should return written data, not open-bus"
        );
    }

    /// Test MMC3 disabled PRG-RAM returns open-bus
    ///
    /// MMC3 can disable PRG-RAM via register $A001 bit 7.
    /// When disabled, reads from $6000-$7FFF should return open-bus.
    #[test]
    fn test_mmc3_disabled_prg_ram_returns_open_bus() {
        let mut mapper = create_mmc3(128, 128);
        
        // Enable PRG-RAM first (bit 7 = 1)
        mapper.write_prg(0xA001, 0b1000_0000);
        
        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xCC);
        mapper.write_prg(0x7FFF, 0xDD);
        
        // Verify reads work when enabled
        assert_eq!(mapper.read_prg(0x6000), 0xCC);
        assert_eq!(mapper.read_prg(0x7FFF), 0xDD);
        
        // Disable PRG-RAM (bit 7 = 0)
        mapper.write_prg(0xA001, 0b0000_0000);
        
        // read_prg should return 0 for backward compatibility
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        
        // read_prg_open_bus should return the open-bus value
        let open_bus = 0x77;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "Disabled PRG-RAM should return open-bus at $6000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus),
            open_bus,
            "Disabled PRG-RAM should return open-bus at $7FFF"
        );
    }

    /// Test MMC3 enabled PRG-RAM doesn't return open-bus
    #[test]
    fn test_mmc3_enabled_prg_ram_returns_data() {
        let mut mapper = create_mmc3(128, 128);
        
        // Enable PRG-RAM (bit 7 = 1)
        mapper.write_prg(0xA001, 0b1000_0000);
        
        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0x99);
        
        let open_bus = 0xFF;
        let result = mapper.read_prg_open_bus(0x6000, open_bus);
        
        // Should return the written value, not open-bus
        assert_eq!(
            result, 0x99,
            "Enabled PRG-RAM should return written data, not open-bus"
        );
    }

    /// Test MMC5 expansion registers don't return open-bus
    ///
    /// MMC5 has expansion registers at $5000-$5FFF that return actual data,
    /// not open-bus.
    #[test]
    fn test_mmc5_expansion_registers_return_data() {
        let mut mapper = create_mmc5(256, 8);
        
        // Write to hardware multiplier
        mapper.write_prg(0x5205, 3);
        mapper.write_prg(0x5206, 4);
        
        let open_bus = 0xAA;
        
        // $5205 should return low byte of 3 * 4 = 12 = 0x0C
        let result = mapper.read_prg_open_bus(0x5205, open_bus);
        assert_eq!(
            result, 12,
            "Multiplier result should be returned, not open-bus"
        );
        
        // $5206 should return high byte of 3 * 4 = 0
        let result = mapper.read_prg_open_bus(0x5206, open_bus);
        assert_eq!(
            result, 0,
            "Multiplier result should be returned, not open-bus"
        );
    }

    /// Test MMC5 ExRAM mode 0 returns open-bus during rendering
    ///
    /// In ExRAM mode 0, CPU reads from $5C00-$5FFF should return open-bus
    /// when rendering is enabled.
    #[test]
    fn test_mmc5_exram_mode_0_returns_open_bus_when_rendering() {
        let mut mapper = create_mmc5(256, 8);
        
        // Set ExRAM to mode 0 (extended attribute mode)
        mapper.write_prg(0x5104, 0x00);
        
        // Write some data to ExRAM
        mapper.write_prg(0x5C00, 0x42);
        
        // Enable rendering (write to PPUMASK via ppu_write_mask)
        mapper.ppu_write_mask(0b0001_1000); // Enable rendering (bits 3-4)
        
        let open_bus = 0xBB;
        let result = mapper.read_prg_open_bus(0x5C00, open_bus);
        
        // Should return open-bus when rendering is enabled
        assert_eq!(
            result, open_bus,
            "ExRAM mode 0 should return open-bus during rendering"
        );
    }

    /// Test MMC5 ExRAM mode 0 returns data when rendering disabled
    #[test]
    fn test_mmc5_exram_mode_0_returns_data_when_not_rendering() {
        let mut mapper = create_mmc5(256, 8);
        
        // Set ExRAM to mode 0
        mapper.write_prg(0x5104, 0x00);
        
        // Write some data to ExRAM
        mapper.write_prg(0x5C00, 0x5A);
        
        // Disable rendering
        mapper.ppu_write_mask(0b0000_0000);
        
        let open_bus = 0xBB;
        let result = mapper.read_prg_open_bus(0x5C00, open_bus);
        
        // Should return the written data
        assert_eq!(
            result, 0x5A,
            "ExRAM mode 0 should return data when rendering is disabled"
        );
    }

    /// Test MMC5 ExRAM mode 1 returns open-bus
    ///
    /// In ExRAM mode 1, CPU reads from $5C00-$5FFF should always return open-bus.
    #[test]
    fn test_mmc5_exram_mode_1_returns_open_bus() {
        let mut mapper = create_mmc5(256, 8);
        
        // Set ExRAM to mode 1 (nametable mode)
        mapper.write_prg(0x5104, 0x01);
        
        // Write some data to ExRAM
        mapper.write_prg(0x5C00, 0x42);
        
        let open_bus = 0xCC;
        
        // Should return open-bus regardless of rendering state
        mapper.ppu_write_mask(0b0000_0000);
        let result1 = mapper.read_prg_open_bus(0x5C00, open_bus);
        assert_eq!(
            result1, open_bus,
            "ExRAM mode 1 should return open-bus (rendering disabled)"
        );
        
        mapper.ppu_write_mask(0b0001_1000);
        let result2 = mapper.read_prg_open_bus(0x5C00, open_bus);
        assert_eq!(
            result2, open_bus,
            "ExRAM mode 1 should return open-bus (rendering enabled)"
        );
    }

    /// Test MMC5 ExRAM modes 2 and 3 return data
    ///
    /// In ExRAM mode 2, CPU reads/writes from $5C00-$5FFF work normally.
    /// In ExRAM mode 3, CPU reads work but writes are ignored (read-only).
    #[test]
    fn test_mmc5_exram_modes_2_and_3_return_data() {
        // Test mode 2 (read/write)
        {
            let mut mapper = create_mmc5(256, 8);
            
            // Set ExRAM to mode 2
            mapper.write_prg(0x5104, 2);
            
            // Write some data to ExRAM
            mapper.write_prg(0x5C00, 0x33);
            
            let open_bus = 0xDD;
            let result = mapper.read_prg_open_bus(0x5C00, open_bus);
            
            // Should return the written data, not open-bus
            assert_eq!(
                result, 0x33,
                "ExRAM mode 2 should return data, not open-bus"
            );
        }
        
        // Test mode 3 (read-only)
        {
            let mut mapper = create_mmc5(256, 8);
            
            // First write data in mode 2
            mapper.write_prg(0x5104, 2);
            mapper.write_prg(0x5C00, 0x44);
            
            // Switch to mode 3 (read-only)
            mapper.write_prg(0x5104, 3);
            
            let open_bus = 0xEE;
            let result = mapper.read_prg_open_bus(0x5C00, open_bus);
            
            // Should return the previously written data, not open-bus
            assert_eq!(
                result, 0x44,
                "ExRAM mode 3 should return data, not open-bus"
            );
            
            // Verify writes are ignored in mode 3
            mapper.write_prg(0x5C00, 0x99);
            let result2 = mapper.read_prg_open_bus(0x5C00, open_bus);
            assert_eq!(
                result2, 0x44,
                "ExRAM mode 3 should be read-only (writes ignored)"
            );
        }
    }

    /// Test that addresses below $5000 return open-bus for MMC5
    #[test]
    fn test_mmc5_below_5000_returns_open_bus() {
        let mapper = create_mmc5(256, 8);
        
        let open_bus = 0x88;
        
        // Test various addresses below $5000
        assert_eq!(
            mapper.read_prg_open_bus(0x0000, open_bus),
            open_bus,
            "MMC5 should return open-bus for $0000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x4000, open_bus),
            open_bus,
            "MMC5 should return open-bus for $4000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x4FFF, open_bus),
            open_bus,
            "MMC5 should return open-bus for $4FFF"
        );
    }

    /// Test consistency: multiple reads with same open-bus value return same result
    #[test]
    fn test_disabled_regions_consistency() {
        let mut mapper = create_mmc3(128, 128);
        
        // Disable PRG-RAM
        mapper.write_prg(0xA001, 0b0000_0000);
        
        let open_bus = 0x55;
        let addr = 0x6500;
        
        // Multiple reads should return consistent values
        let read1 = mapper.read_prg_open_bus(addr, open_bus);
        let read2 = mapper.read_prg_open_bus(addr, open_bus);
        let read3 = mapper.read_prg_open_bus(addr, open_bus);
        
        assert_eq!(read1, read2, "Consecutive reads should be consistent");
        assert_eq!(read2, read3, "Consecutive reads should be consistent");
        assert_eq!(read1, open_bus, "Should return open-bus value");
    }

    /// Test that different addresses with same open-bus return same value
    #[test]
    fn test_disabled_region_different_addresses() {
        let mut mapper = create_mmc3(128, 128);
        
        // Disable PRG-RAM
        mapper.write_prg(0xA001, 0b0000_0000);
        
        let open_bus = 0x66;
        
        // All addresses in disabled region should return same open-bus value
        assert_eq!(mapper.read_prg_open_bus(0x6000, open_bus), open_bus);
        assert_eq!(mapper.read_prg_open_bus(0x6800, open_bus), open_bus);
        assert_eq!(mapper.read_prg_open_bus(0x7000, open_bus), open_bus);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, open_bus), open_bus);
    }
}
