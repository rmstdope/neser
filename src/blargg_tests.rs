#[cfg(test)]
mod tests {
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
        use_console_output: bool,
    }

    impl BlarggTestRunner {
        /// Create a new test runner for $6000-based tests
        pub fn new(rom_path: &str, max_frames: u32) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                use_console_output: false,
            }
        }

        /// Create a new test runner for console output-based tests (branch timing, etc.)
        pub fn new_console(rom_path: &str, max_frames: u32) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                use_console_output: true,
            }
        }

        /// Run the test ROM and return the result
        ///
        /// The test ROM is executed for up to `max_frames` frames.
        ///
        /// For $6000-based tests:
        /// - Results are checked by reading $6000 in PRG-RAM:
        ///   - 0x00 = Pass
        ///   - 0x01+ = Fail with error code
        ///
        /// For console-based tests:
        /// - Reads nametable text looking for "PASSED" or "FAILED"
        ///
        /// Returns `Timeout` if no result is found within max_frames.
        pub fn run_test(&self) -> BlarggTestResult {
            // Load ROM
            let rom_data = match fs::read(&self.rom_path) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to load ROM {}: {}", self.rom_path, e);
                    return BlarggTestResult::Fail(0x80 as u8);
                }
            };

            let cartridge = match Cartridge::new(&rom_data) {
                Ok(cart) => cart,
                Err(e) => {
                    eprintln!("Failed to parse ROM {}: {}", self.rom_path, e);
                    return BlarggTestResult::Fail(0x81 as u8);
                }
            };

            // Create NES and insert cartridge
            let mut nes = Nes::new(TvSystem::Ntsc);
            nes.insert_cartridge(cartridge);
            nes.reset();

            if self.use_console_output {
                println!("Running console-based test ROM: {} ... ", self.rom_path);
            } else {
                println!("Running $6000-based test ROM: {} ... ", self.rom_path);
            }

            // Run frames and check for results
            for frame in 1..=self.max_frames {
                // Run one frame (roughly 29780 CPU cycles for NTSC)
                for _ in 0..29780 {
                    nes.run_cpu_tick();
                }

                // Check every 60 frames (1 second intervals)
                if frame % 60 == 0 {
                    if self.use_console_output {
                        // Console-based test: read nametable text
                        // Console output starts at $2081 and spans multiple rows
                        let text = nes.read_nametable_text(0x2081, 160);

                        if text.to_uppercase().contains("PASSED") {
                            println!("Test passed (found 'PASSED' in console output)");
                            return BlarggTestResult::Pass;
                        } else if text.to_uppercase().contains("FAILED")
                            || text.to_uppercase().contains("ERROR")
                        {
                            // Try to extract error code from "FAILED: #N" pattern
                            println!("Test failed (found 'FAILED' or 'Error'in console output)");
                            println!("Console output: {}", text.trim());
                            return BlarggTestResult::Fail(1);
                        }
                    } else {
                        // $6000-based test
                        let status = nes.memory.borrow().read(0x6000);

                        // Check if test has completed
                        // Status byte 0x00 means "passed"
                        // Status byte 0x01-0x7F means "failed with error code"
                        // Status byte 0x80 means "running"
                        // Status byte 0x81 means "need reset"
                        if status == 0x00 {
                            return BlarggTestResult::Pass;
                        } else if status > 0x00 && status < 0x80 {
                            return BlarggTestResult::Fail(status);
                        } else if status == 0x81 {
                            nes.reset();
                        }
                    }
                }
            }

            // No result found within timeout
            BlarggTestResult::Timeout
        }
    }

    /// Macro to generate console-based tests for branch timing ROMs
    macro_rules! console_test {
        ($test_name:ident, $rom_path:expr) => {
            #[test]
            fn $test_name() {
                let runner = BlarggTestRunner::new_console($rom_path, 300);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
    }

    /// Macro to generate $6000-based tests with custom timeout
    macro_rules! prg_ram_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let runner = BlarggTestRunner::new($rom_path, $timeout);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            prg_ram_test!($test_name, $rom_path, 180);
        };
    }

    // Branch timing tests
    console_test!(
        test_branch_timing,
        "roms/blargg/branch_timing_tests/1.Branch_Basics.nes"
    );
    console_test!(
        test_backward_branch,
        "roms/blargg/branch_timing_tests/2.Backward_Branch.nes"
    );
    console_test!(
        test_forward_branch,
        "roms/blargg/branch_timing_tests/3.Forward_Branch.nes"
    );
    console_test!(
        test_cpu_dummy_reads,
        "roms/blargg/cpu_dummy_reads/cpu_dummy_reads.nes"
    );
    prg_ram_test!(
        test_cpu_dummy_writes_oam,
        "roms/blargg/cpu_dummy_writes/cpu_dummy_writes_oam.nes"
    );
    prg_ram_test!(
        test_cpu_dummy_writes_ppumem,
        "roms/blargg/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"
    );
    prg_ram_test!(
        test_cpu_exec_space_ppuio,
        "roms/blargg/cpu_exec_space/test_cpu_exec_space_ppuio.nes"
    );
    prg_ram_test!(
        test_cpu_exec_space_apu,
        "roms/blargg/cpu_exec_space/test_cpu_exec_space_apu.nes"
    );
    prg_ram_test!(
        test_cpu_interrupts,
        "roms/blargg/cpu_interrupts_v2/cpu_interrupts.nes"
    );

    // OAM and APU tests
    prg_ram_test!(test_oam_read, "roms/oam_read.nes");

    // prg_ram_test!(test_oam_stress, "roms/oam_stress.nes", 600);
    // prg_ram_test!(test_cpu, "roms/cpu.nes");

    prg_ram_test!(test_4015_cleared, "roms/blargg/4015_cleared.nes");

    #[test]
    #[ignore]
    fn test_4017_timing() {
        let runner = BlarggTestRunner::new("roms/blargg/4017_timing.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "4017_timing.nes should pass"
        );
    }

    prg_ram_test!(test_4017_written, "roms/blargg/4017_written.nes");
    prg_ram_test!(test_irq_flag_cleared, "roms/blargg/irq_flag_cleared.nes");
    prg_ram_test!(test_len_ctrs_enabled, "roms/blargg/len_ctrs_enabled.nes");
    prg_ram_test!(test_works_immediately, "roms/blargg/works_immediately.nes");
    prg_ram_test!(test_1_len_ctr, "roms/blargg/1-len_ctr.nes");
    prg_ram_test!(test_2_len_table, "roms/blargg/2-len_table.nes");
    prg_ram_test!(test_3_irq_flags, "roms/blargg/3-irq_flag.nes");
    prg_ram_test!(test_4_jitter, "roms/blargg/4-jitter.nes");
}
