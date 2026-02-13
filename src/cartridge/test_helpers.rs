#![cfg(test)]

//! Common test utilities for mapper testing
//!
//! This module provides shared helper functions used across mapper test modules
//! to reduce code duplication and ensure consistent test patterns.

/// Create a ROM with distinct data in each bank for testing
///
/// Fills each bank with the bank index as the fill byte (lower 8 bits),
/// making it easy to verify correct bank selection by reading the data.
///
/// # Arguments
///
/// * `bank_size` - Size of each bank in bytes
/// * `num_banks` - Number of banks to create
///
/// # Returns
///
/// A vector containing `bank_size * num_banks` bytes, where each bank
/// is filled with its index (0, 1, 2, etc.)
///
/// # Examples
///
/// ```
/// # use neser::cartridge::test_helpers::banked_data;
/// // Create 4 banks of 8KB each
/// let rom = banked_data(8192, 4);
/// assert_eq!(rom.len(), 32768);
/// assert_eq!(rom[0], 0);      // First bank filled with 0
/// assert_eq!(rom[8192], 1);   // Second bank filled with 1
/// assert_eq!(rom[16384], 2);  // Third bank filled with 2
/// assert_eq!(rom[24576], 3);  // Fourth bank filled with 3
/// ```
pub fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
    let mut data = vec![0u8; bank_size * num_banks];
    for bank in 0..num_banks {
        let start = bank * bank_size;
        let end = start + bank_size;
        data[start..end].fill(bank as u8);
    }
    data
}

/// Create a ROM with distinct data in each bank using upper 8 bits of bank index
///
/// Similar to `banked_data`, but fills each bank with the upper 8 bits of the
/// bank index (bank >> 8). This is useful for testing mappers with large numbers
/// of banks (> 256) where the lower 8 bits would wrap around.
///
/// # Arguments
///
/// * `bank_size` - Size of each bank in bytes
/// * `num_banks` - Number of banks to create
///
/// # Returns
///
/// A vector containing `bank_size * num_banks` bytes, where each bank
/// is filled with the upper 8 bits of its index
///
/// # Examples
///
/// ```
/// # use neser::cartridge::test_helpers::banked_data_with_upper_marker;
/// // Create 512 banks - bank 256 will have marker 1, bank 257 marker 1, etc.
/// let rom = banked_data_with_upper_marker(1024, 512);
/// assert_eq!(rom[0], 0);           // Bank 0: 0 >> 8 = 0
/// assert_eq!(rom[256 * 1024], 1);  // Bank 256: 256 >> 8 = 1
/// assert_eq!(rom[257 * 1024], 1);  // Bank 257: 257 >> 8 = 1
/// ```
pub fn banked_data_with_upper_marker(bank_size: usize, num_banks: usize) -> Vec<u8> {
    let mut data = vec![0u8; bank_size * num_banks];
    for bank in 0..num_banks {
        let start = bank * bank_size;
        let end = start + bank_size;
        data[start..end].fill((bank >> 8) as u8);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banked_data_creates_distinct_banks() {
        let rom = banked_data(1024, 4);
        assert_eq!(rom.len(), 4096);
        
        // Verify each bank is filled with its index
        assert_eq!(rom[0], 0);
        assert_eq!(rom[1023], 0);
        assert_eq!(rom[1024], 1);
        assert_eq!(rom[2047], 1);
        assert_eq!(rom[2048], 2);
        assert_eq!(rom[3071], 2);
        assert_eq!(rom[3072], 3);
        assert_eq!(rom[4095], 3);
    }

    #[test]
    fn test_banked_data_single_bank() {
        let rom = banked_data(100, 1);
        assert_eq!(rom.len(), 100);
        assert!(rom.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_banked_data_empty() {
        let rom = banked_data(100, 0);
        assert_eq!(rom.len(), 0);
    }

    #[test]
    fn test_banked_data_with_upper_marker_basic() {
        let rom = banked_data_with_upper_marker(1024, 4);
        assert_eq!(rom.len(), 4096);
        
        // All banks 0-3 should have marker 0 (0 >> 8 = 0)
        assert!(rom.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_banked_data_with_upper_marker_large_banks() {
        // Test with bank numbers that have different upper bytes
        let rom = banked_data_with_upper_marker(1024, 512);
        assert_eq!(rom.len(), 512 * 1024);
        
        // Banks 0-255 should have marker 0
        assert_eq!(rom[0], 0);
        assert_eq!(rom[255 * 1024], 0);
        
        // Banks 256-511 should have marker 1
        assert_eq!(rom[256 * 1024], 1);
        assert_eq!(rom[511 * 1024], 1);
    }

    #[test]
    fn test_banked_data_with_upper_marker_boundary() {
        // Test bank 255 (0xFF) and 256 (0x100)
        let rom = banked_data_with_upper_marker(10, 260);
        
        // Bank 255: 255 >> 8 = 0
        assert_eq!(rom[255 * 10], 0);
        
        // Bank 256: 256 >> 8 = 1
        assert_eq!(rom[256 * 10], 1);
        
        // Bank 259: 259 >> 8 = 1
        assert_eq!(rom[259 * 10], 1);
    }
}
