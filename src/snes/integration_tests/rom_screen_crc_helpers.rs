//! Shared utilities for ROM-based screen CRC32 validation tests.
//!
//! Provides helper functions for running ROMs that report pass/fail by rendering
//! a solid backdrop color: blue for PASS, red/maroon for FAIL. These helpers
//! are used by multiple test suites to validate screen output against
//! human-approved golden CRC32 values.

use super::rom_runner::{run_rom_with_oracle, RunConfig, RunExitReason, RunOracle};
use std::fs;
use std::path::Path;

/// Runs a ROM to the specified frame and asserts the rendered screen
/// matches the expected CRC32.
///
/// # Arguments
///
/// * `rom_path` - Full path to the ROM file
/// * `file_name` - Short file name for error messages
/// * `test_suite` - Test suite name for capture output directory
/// * `frames` - Number of frames to run until validation
/// * `expected_crc` - Expected CRC32 of the screen at the target frame
///
/// # Panics
///
/// Panics if the ROM cannot be read or if the screen CRC doesn't match
/// the expected value.
///
/// # Example
///
/// ```ignore
/// let path = Path::new("roms/test").join("example.smc");
/// assert_rom_screen_crc(&path, "example.smc", "example_tests", 600, 0x12345678);
/// ```
pub fn assert_rom_screen_crc(
    rom_path: &Path,
    file_name: &str,
    test_suite: &str,
    frames: u32,
    expected_crc: u32,
) {
    let rom = fs::read(rom_path)
        .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", rom_path.display()));

    let result = run_rom_with_oracle(
        &rom,
        file_name,
        test_suite,
        RunConfig::new(400_000_000, 0),
        RunOracle::ScreenCrc {
            frames,
            expected_crc,
        },
    );

    assert!(
        result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
        "{file_name}: expected screen-CRC PASS (blue) at frame {frames}, \
         got crc=0x{:08X} passed={} exit={:?}",
        result.screen_crc32,
        result.passed,
        result.exit_reason
    );
}
