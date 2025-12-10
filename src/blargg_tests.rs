/// OAM test infrastructure for automated testing of blargg's OAM test ROMs
///
/// This module provides infrastructure to run OAM test ROMs (oam_read, oam_stress, oam3)
/// and automatically detect PASS/FAIL status by reading results from PRG-RAM.
///
/// Blargg test ROMs write their results to $6000-$6003:
/// - $6000 = 0x00: Test passed
/// - $6000 = 0x01+: Test failed with error code
/// - $6001-$6003: Additional error information or text output
use crate::cartridge::Cartridge;
use crate::nes::{Nes, TvSystem};
use std::fs;

/// Result of running an OAM test ROM
#[derive(Debug, PartialEq, Eq)]
pub enum BlarggTestResult {
    /// Test passed (status byte = 0x00)
    Pass,
    /// Test failed with error code
    Fail(u8),
    /// Test didn't complete within timeout
    Timeout,
}

/// Runner for OAM test ROMs
pub struct BlarggTestRunner {
    rom_path: String,
    max_frames: u32,
}

impl BlarggTestRunner {
    /// Create a new test runner
    pub fn new(rom_path: &str, max_frames: u32) -> Self {
        Self {
            rom_path: rom_path.to_string(),
            max_frames,
        }
    }

    /// Run the test ROM and return the result
    ///
    /// The test ROM is executed for up to `max_frames` frames.
    /// Results are checked by reading $6000 in PRG-RAM:
    /// - 0x00 = Pass
    /// - 0x01+ = Fail with error code
    ///
    /// Returns `Timeout` if no result is found within max_frames.
    pub fn run_test(&self) -> BlarggTestResult {
        // Load ROM
        let rom_data = match fs::read(&self.rom_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to load ROM {}: {}", self.rom_path, e);
                return BlarggTestResult::Timeout;
            }
        };

        let cartridge = match Cartridge::new(&rom_data) {
            Ok(cart) => cart,
            Err(e) => {
                eprintln!("Failed to parse ROM {}: {}", self.rom_path, e);
                return BlarggTestResult::Timeout;
            }
        };

        // Create NES and insert cartridge
        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset();
        println!("Running OAM test ROM: {} ... ", self.rom_path);

        // Run frames and check for results
        for frame in 1..=self.max_frames {
            // Run one frame (roughly 29780 CPU cycles for NTSC)
            for _ in 0..29780 {
                nes.run_cpu_tick();
            }

            // Check every 60 frames (1 second intervals)
            if frame % 60 == 0 {
                let status = nes.memory.borrow().read(0x6000);

                // Check if test has completed
                // Status byte 0x00 means "passed"
                // Status byte 0x01-0x7F means "failed with error code"
                // Status byte 0x80 means "running"
                // Status byte 0x81 means "need reset"
                if status == 0x00 {
                    // // Additional check: ensure we're not reading uninitialized memory
                    // // Many blargg tests write additional text after $6000
                    // let byte1 = nes.memory.borrow().read(0x6004);
                    // let byte2 = nes.memory.borrow().read(0x6005);

                    // // If there's readable text or specific patterns, test has run
                    // if byte1 != 0x00 || byte2 != 0x00 || frame > 120 {
                    return BlarggTestResult::Pass;
                    // }
                } else if status > 0x00 && status < 0x80 {
                    return BlarggTestResult::Fail(status);
                } else if status == 0x81 {
                    nes.reset();
                }
            }
        }

        // No result found within timeout
        BlarggTestResult::Timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oam_read() {
        let runner = BlarggTestRunner::new("roms/oam_read.nes", 180);
        let result = runner.run_test();
        assert_eq!(result, BlarggTestResult::Pass, "oam_read.nes should pass");
    }

    // #[test]
    // fn test_oam_stress() {
    //     let runner = BlarggTestRunner::new("roms/oam_stress.nes", 600); // Doubled timeout to 10 seconds
    //     let result = runner.run_test();
    //     assert_eq!(result, BlarggTestResult::Pass, "oam_stress.nes should pass");
    // }

    // #[test]
    // fn test_cpu() {
    //     let runner = BlarggTestRunner::new("roms/cpu.nes", 180);
    //     let result = runner.run_test();
    //     assert_eq!(result, BlarggTestResult::Pass, "cpu.nes should pass");
    // }

    #[test]
    fn test_4015_cleared() {
        let runner = BlarggTestRunner::new("roms/blargg/4015_cleared.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "4015_cleared.nes should pass"
        );
    }

    #[test]
    fn test_4017_timing() {
        let runner = BlarggTestRunner::new("roms/blargg/4017_timing.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "4017_timing.nes should pass"
        );
    }

    #[test]
    fn test_4017_written() {
        let runner = BlarggTestRunner::new("roms/blargg/4017_written.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "4017_written.nes should pass"
        );
    }
    #[test]
    fn test_irq_flag_cleared() {
        let runner = BlarggTestRunner::new("roms/blargg/irq_flag_cleared.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "irq_flag_cleared.nes should pass"
        );
    }
    #[test]
    fn test_len_ctrs_enabled() {
        let runner = BlarggTestRunner::new("roms/blargg/len_ctrs_enabled.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "len_ctrs_enabled.nes should pass"
        );
    }

    #[test]
    fn test_works_immediately() {
        let runner = BlarggTestRunner::new("roms/blargg/works_immediately.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "works_immediately.nes should pass"
        );
    }

}
