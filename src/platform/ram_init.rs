//! RAM initialization utilities for configurable power-on behavior.
//!
//! This module provides helper functions to initialize RAM with different patterns
//! based on the configured RAM initialization mode. It lives under `platform`
//! alongside [`RamInitMode`] because every emulated system uses it; `nes::console`
//! re-exports [`initialize_ram`] so the NES call sites keep their historical path.

use crate::platform::config::RamInitMode;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Initialize a RAM buffer based on the configured RAM initialization mode.
///
/// # Arguments
/// * `buffer` - Mutable slice to initialize
/// * `mode` - RAM initialization mode (zero, random, or seeded-random)
///
/// # Examples
/// ```
/// use neser::platform::config::RamInitMode;
/// use neser::platform::ram_init::initialize_ram;
///
/// let mut ram = vec![0u8; 2048];
/// initialize_ram(&mut ram, RamInitMode::Zero);
/// assert!(ram.iter().all(|&b| b == 0x00));
/// ```
pub fn initialize_ram(buffer: &mut [u8], mode: RamInitMode) {
    match mode {
        RamInitMode::Zero => {
            // Initialize all bytes to 0x00
            buffer.fill(0x00);
        }
        RamInitMode::Random => {
            // Initialize with pseudo-random values
            let mut rng = rand::rng();
            rng.fill(buffer);
        }
        RamInitMode::SeededRandom(seed) => {
            // Initialize with pseudo-random values from a fixed seed
            let mut rng = StdRng::seed_from_u64(seed);
            rng.fill(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_mode_initializes_to_zero() {
        let mut buffer = vec![0xFF; 1024];
        initialize_ram(&mut buffer, RamInitMode::Zero);
        assert!(buffer.iter().all(|&b| b == 0x00));
    }

    #[test]
    fn test_seeded_random_is_deterministic() {
        let mut buffer1 = vec![0; 1024];
        let mut buffer2 = vec![0; 1024];

        initialize_ram(&mut buffer1, RamInitMode::SeededRandom(42));
        initialize_ram(&mut buffer2, RamInitMode::SeededRandom(42));

        assert_eq!(
            buffer1, buffer2,
            "Same seed should produce identical results"
        );
    }

    #[test]
    fn test_seeded_random_different_seeds_produce_different_values() {
        let mut buffer1 = vec![0; 1024];
        let mut buffer2 = vec![0; 1024];

        initialize_ram(&mut buffer1, RamInitMode::SeededRandom(42));
        initialize_ram(&mut buffer2, RamInitMode::SeededRandom(43));

        assert_ne!(
            buffer1, buffer2,
            "Different seeds should produce different results"
        );
    }

    #[test]
    fn test_seeded_random_produces_nonzero_values() {
        let mut buffer = vec![0; 1024];
        initialize_ram(&mut buffer, RamInitMode::SeededRandom(42));

        // At least some bytes should be non-zero
        assert!(buffer.iter().any(|&b| b != 0x00));
    }

    #[test]
    fn test_random_mode_produces_nonzero_values() {
        let mut buffer = vec![0; 1024];
        initialize_ram(&mut buffer, RamInitMode::Random);

        // At least some bytes should be non-zero with high probability
        assert!(buffer.iter().any(|&b| b != 0x00));
    }
}
