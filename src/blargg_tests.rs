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
        wait_reset: u32,
    }

    impl BlarggTestRunner {
        /// Create a new test runner for $6000-based tests
        pub fn new(rom_path: &str, max_frames: u32) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                wait_reset: 1,
            }
        }

        /// Run the test ROM and return the result
        ///
        /// The test ROM is executed for up to `max_frames` frames.
        ///
        /// Checks for either $6000 status byte or console output:
        /// - Results are checked by reading $6000 in PRG-RAM:
        ///   - 0x00 = Pass
        ///   - 0x01+ = Fail with error code
        /// - For console-based tests:
        ///   - Reads nametable text looking for "PASSED" or "FAILED"
        ///
        /// Returns `Timeout` if no result is found within max_frames.
        pub fn run_test(&mut self) -> BlarggTestResult {
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

            // println!("Running Blargg-based test ROM: {} ... ", self.rom_path);

            // Run frames and check for results
            for _frame in 1..=self.max_frames {
                // Run one frame (roughly 29780 CPU cycles for NTSC)
                for _ in 0..29780 {
                    nes.run_cpu_tick();
                }

                // Try to read $6000 status byte
                let status = nes.memory.borrow().read(0x6000);
                // Try to read nametable text from the currently displayed nametable
                // Get base nametable address from PPUCTRL and add offset to skip first row
                let base_addr = nes.base_nametable_addr();
                let text = nes.read_nametable_text(base_addr, 32 * 32);
                if (status > 0x00 && status < 0x80)
                    || text.to_uppercase().contains("FAILED")
                    || text.to_uppercase().contains("ERROR")
                {
                    // Try to extract error code from "FAILED: #N" pattern
                    println!("Test failed!");
                    println!("Console output:");
                    // Split into 32-character rows (nametable width)
                    for line in text.as_bytes().chunks(32) {
                        let line_str = String::from_utf8_lossy(line);
                        let trimmed = line_str.trim_end();
                        if trimmed != "" {
                            println!("{}", trimmed);
                        }
                    }
                    return BlarggTestResult::Fail(1);
                } else if status == 0x00 {
                    // || text.to_uppercase().contains("PASSED") {
                    println!("Test passed!");
                    // for line in text.as_bytes().chunks(32) {
                    //     let line_str = String::from_utf8_lossy(line);
                    //     let trimmed = line_str.trim_end();
                    //     if trimmed != "" {
                    //         println!("{}", trimmed);
                    //     }
                    // }
                    return BlarggTestResult::Pass;
                } else if status == 0x81 {
                    // && !text.to_uppercase().contains("DISAPPEARS") {
                    if self.wait_reset > 0 {
                        // println!("Test indicates reset, waiting {} frames...", self.wait_reset);
                        self.wait_reset -= 1;
                    } else {
                        // println!("Test indicates reset, restarting NES...");
                        nes.reset();
                        self.wait_reset = 1;
                    }
                }
            }

            // No result found within timeout
            BlarggTestResult::Timeout
        }
    }

    /// Macro to generate $6000-based tests with custom timeout
    macro_rules! blargg_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = BlarggTestRunner::new($rom_path, $timeout);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            blargg_test!($test_name, $rom_path, 180);
        };
    }

    // Branch timing tests
    blargg_test!(
        test_branch_timing,
        "roms/blargg/branch_timing_tests/1.Branch_Basics.nes"
    );
    blargg_test!(
        test_backward_branch,
        "roms/blargg/branch_timing_tests/2.Backward_Branch.nes"
    );
    blargg_test!(
        test_forward_branch,
        "roms/blargg/branch_timing_tests/3.Forward_Branch.nes"
    );
    blargg_test!(
        test_cpu_dummy_reads,
        "roms/blargg/cpu_dummy_reads/cpu_dummy_reads.nes"
    );
    blargg_test!(
        test_cpu_dummy_writes_oam,
        "roms/blargg/cpu_dummy_writes/cpu_dummy_writes_oam.nes"
    );
    blargg_test!(
        test_cpu_dummy_writes_ppumem,
        "roms/blargg/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"
    );
    blargg_test!(
        test_cpu_exec_space_ppuio,
        "roms/blargg/cpu_exec_space/test_cpu_exec_space_ppuio.nes"
    );
    blargg_test!(
        test_cpu_exec_space_apu,
        "roms/blargg/cpu_exec_space/test_cpu_exec_space_apu.nes"
    );
    // blargg_test!(
    //     test_cpu_interrupts,
    //     "roms/blargg/cpu_interrupts_v2/cpu_interrupts.nes"
    // );
    blargg_test!(
        test_cpu_reset_registers,
        "roms/blargg/cpu_reset/registers.nes"
    );
    blargg_test!(
        test_cpu_reset_ram_after_reset,
        "roms/blargg/cpu_reset/ram_after_reset.nes"
    );
    blargg_test!(
        test_cpu_timing_test,
        "roms/blargg/cpu_timing_test6/cpu_timing_test.nes"
    );
    blargg_test!(test_instr_misc, "roms/blargg/instr_misc/instr_misc.nes");
    blargg_test!(
        test_instr_01_basics,
        "roms/blargg/instr_test-v5/rom_singles/01-basics.nes"
    );
    blargg_test!(
        test_instr_02_implied,
        "roms/blargg/instr_test-v5/rom_singles/02-implied.nes"
    );
    blargg_test!(
        test_instr_03_immediate,
        "roms/blargg/instr_test-v5/rom_singles/03-immediate.nes"
    );
    blargg_test!(
        test_instr_04_zero_page,
        "roms/blargg/instr_test-v5/rom_singles/04-zero_page.nes"
    );
    blargg_test!(
        test_instr_05_zp_xy,
        "roms/blargg/instr_test-v5/rom_singles/05-zp_xy.nes"
    );
    blargg_test!(
        test_instr_06_absolute,
        "roms/blargg/instr_test-v5/rom_singles/06-absolute.nes"
    );
    blargg_test!(
        test_instr_07_abs_xy,
        "roms/blargg/instr_test-v5/rom_singles/07-abs_xy.nes"
    );
    blargg_test!(
        test_instr_08_ind_x,
        "roms/blargg/instr_test-v5/rom_singles/08-ind_x.nes"
    );
    blargg_test!(
        test_instr_09_ind_y,
        "roms/blargg/instr_test-v5/rom_singles/09-ind_y.nes"
    );
    blargg_test!(
        test_instr_10_branches,
        "roms/blargg/instr_test-v5/rom_singles/10-branches.nes"
    );
    blargg_test!(
        test_instr_11_stack,
        "roms/blargg/instr_test-v5/rom_singles/11-stack.nes"
    );
    blargg_test!(
        test_instr_12_jmp_jsr,
        "roms/blargg/instr_test-v5/rom_singles/12-jmp_jsr.nes"
    );
    blargg_test!(
        test_instr_13_rts,
        "roms/blargg/instr_test-v5/rom_singles/13-rts.nes"
    );
    blargg_test!(
        test_instr_14_rti,
        "roms/blargg/instr_test-v5/rom_singles/14-rti.nes"
    );
    blargg_test!(
        test_instr_15_brk,
        "roms/blargg/instr_test-v5/rom_singles/15-brk.nes"
    );
    blargg_test!(
        test_instr_16_special,
        "roms/blargg/instr_test-v5/rom_singles/16-special.nes"
    );
    // OAM and APU tests
    blargg_test!(test_oam_read, "roms/oam_read.nes");

    // blargg_test!(test_oam_stress, "roms/oam_stress.nes", 600);
    // blargg_test!(test_cpu, "roms/cpu.nes");

    blargg_test!(test_4015_cleared, "roms/blargg/4015_cleared.nes");

    #[test]
    fn test_4017_timing() {
        let mut runner = BlarggTestRunner::new("roms/blargg/4017_timing.nes", 180);
        let result = runner.run_test();
        assert_eq!(
            result,
            BlarggTestResult::Pass,
            "4017_timing.nes should pass"
        );
    }

    blargg_test!(test_4017_written, "roms/blargg/4017_written.nes");
    blargg_test!(test_irq_flag_cleared, "roms/blargg/irq_flag_cleared.nes");
    blargg_test!(test_len_ctrs_enabled, "roms/blargg/len_ctrs_enabled.nes");
    blargg_test!(test_works_immediately, "roms/blargg/works_immediately.nes");
    blargg_test!(test_1_len_ctr, "roms/blargg/1-len_ctr.nes");
    blargg_test!(test_2_len_table, "roms/blargg/2-len_table.nes");
    blargg_test!(test_3_irq_flags, "roms/blargg/3-irq_flag.nes");
    blargg_test!(test_4_jitter, "roms/blargg/4-jitter.nes");
}
