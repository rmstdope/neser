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

    /// Test verification method
    #[derive(Debug, PartialEq, Eq)]
    pub enum BlarggTestVerification {
        /// Verify using status byte at 0x6000
        StatusByte,
        /// Verify using console output
        Console,
        /// Verify by matching a CRC-32 printed to the console output.
        ///
        /// Some blargg ROMs only print their CRC (and do not print "Passed"/"Failed").
        ConsoleCrc(&'static [u32]),
    }

    /// Runner for OAM test ROMs
    pub struct BlarggTestRunner {
        rom_path: String,
        max_frames: u32,
        wait_reset: u32,
        verification: BlarggTestVerification,
    }

    impl BlarggTestRunner {
        /// Create a new test runner for $6000-based tests
        pub fn new(rom_path: &str, max_frames: u32, verification: BlarggTestVerification) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                wait_reset: 1,
                verification,
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
            // Initial reset is treated as power-on.
            nes.reset(false);

            // println!("Running Blargg-based test ROM: {} ... ", self.rom_path);

            let mut running = false;
            // Run frames and check for results
            for _frame in 1..=self.max_frames {
                // Run one frame (roughly 29780 CPU cycles for NTSC)
                let mut current_status = nes.memory.borrow().read_for_testing(0x6000);
                if current_status == 0x80 {
                    running = true;
                }
                const STATUS_POLL_INTERVAL: u32 = 256;
                for cpu_cycle in 0..29780 {
                    // println!("{}", nes.trace(&crate::tracing::Tracing::default()));
                    nes.run_cpu_tick();

                    if cpu_cycle != 0 && cpu_cycle % STATUS_POLL_INTERVAL == 0 {
                        current_status = nes.memory.borrow().read_for_testing(0x6000);
                        if current_status == 0x80 {
                            running = true;
                        }
                    }
                }
                // Make sure we observe any status update at end-of-frame.
                let status = nes.memory.borrow().read_for_testing(0x6000);
                if status == 0x80 {
                    running = true;
                }

                // Drain side channels once per frame to avoid unbounded growth.
                if nes.is_ready_to_render() {
                    nes.clear_ready_to_render();
                }
                while nes.sample_ready() {
                    nes.get_sample();
                }

                if self.verification == BlarggTestVerification::StatusByte && !running {
                    continue;
                }
                if self.verification == BlarggTestVerification::StatusByte {
                    if status == 0x00 {
                        // println!("Test passed!");
                        return BlarggTestResult::Pass;
                    } else if status > 0x00 && status < 0x80 {
                        let base_addr = nes.base_nametable_addr();
                        let mut text = nes.read_nametable_text(base_addr, 32 * 32);
                        text = text
                            .as_bytes()
                            .chunks(32)
                            .map(|chunk| String::from_utf8_lossy(chunk).trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join("\n");
                        println!("Test failed with status code: 0x{:02X}", status);
                        println!("Console output:\n{}", text);
                        return BlarggTestResult::Fail(status);
                    } else if status == 0x81 {
                        if self.wait_reset > 0 {
                            // println!(
                            //     "Test indicates reset, waiting {} frames...",
                            //     self.wait_reset
                            // );
                            self.wait_reset -= 1;
                        } else {
                            // println!("Test indicates reset, restarting NES...");
                            // Test requests a reset-button style reset.
                            nes.reset(true);
                            nes.memory.borrow_mut().write_for_testing(0x6000, 0x80);
                            self.wait_reset = 1;
                        }
                    } else if status == 0x80 {
                        // Still running
                        continue;
                    }
                } else if self.verification == BlarggTestVerification::Console {
                    let base_addr = nes.base_nametable_addr();
                    let mut text = nes.read_nametable_text(base_addr, 32 * 32);
                    text = text
                        .as_bytes()
                        .chunks(32)
                        .map(|chunk| String::from_utf8_lossy(chunk).trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    // Check if $0x test
                    let is_0x = text.len() == 3 && text.starts_with("$0");
                    if text.to_uppercase().contains("PASSED") || text == "$01" {
                        // println!("Test passed!");
                        return BlarggTestResult::Pass;
                    } else if text.to_uppercase().contains("FAILED")
                        || text.to_uppercase().contains("ERROR")
                        || is_0x
                    {
                        println!("Test failed!");
                        println!("Console output:\n{}", text);
                        return BlarggTestResult::Fail(1);
                    }
                } else if let BlarggTestVerification::ConsoleCrc(expected_crcs) = self.verification
                {
                    let base_addr = nes.base_nametable_addr();
                    let mut text = nes.read_nametable_text(base_addr, 32 * 32);
                    text = text
                        .as_bytes()
                        .chunks(32)
                        .map(|chunk| String::from_utf8_lossy(chunk).trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");

                    if let Some(crc) = parse_crc32_from_console_text(&text) {
                        if expected_crcs.contains(&crc) {
                            return BlarggTestResult::Pass;
                        }
                        println!("Test failed! Unexpected CRC 0x{:08X}", crc);
                        println!("Console output:\n{}", text);
                        return BlarggTestResult::Fail(1);
                    }
                }
            }

            // No result found within timeout
            BlarggTestResult::Timeout
        }
    }

    fn parse_crc32_from_console_text(text: &str) -> Option<u32> {
        // The test framework prints CRCs as 8 hex digits (uppercase) on their own line.
        // We accept either case and ignore any other tokens.
        for token in text.split_whitespace() {
            if token.len() != 8 {
                continue;
            }
            if !token.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            if let Ok(value) = u32::from_str_radix(token, 16) {
                return Some(value);
            }
        }
        None
    }

    /// Macro to generate $6000-based tests with custom timeout
    macro_rules! blargg_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner =
                    BlarggTestRunner::new($rom_path, $timeout, BlarggTestVerification::StatusByte);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            blargg_test!($test_name, $rom_path, 180);
        };
    }

    macro_rules! blargg_console_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner =
                    BlarggTestRunner::new($rom_path, $timeout, BlarggTestVerification::Console);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            blargg_console_test!($test_name, $rom_path, 180);
        };
    }

    macro_rules! blargg_console_crc_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = BlarggTestRunner::new(
                    $rom_path,
                    $timeout,
                    BlarggTestVerification::ConsoleCrc($expected),
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
    }

    // Branch timing tests
    blargg_console_test!(
        test_branch_timing,
        "roms/blargg/branch_timing_tests/1.Branch_Basics.nes"
    );
    blargg_console_test!(
        test_backward_branch,
        "roms/blargg/branch_timing_tests/2.Backward_Branch.nes"
    );
    blargg_console_test!(
        test_forward_branch,
        "roms/blargg/branch_timing_tests/3.Forward_Branch.nes"
    );
    blargg_console_test!(
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

    // DMC DMA "register conflicts" and related timing edge cases
    blargg_console_crc_test!(
        test_dmc_dma_during_read4_2007_read,
        "roms/blargg/dmc_dma_during_read4/dma_2007_read.nes",
        300,
        &[0x159A7A8F, 0x5E3DF9C4]
    );
    blargg_console_test!(
        test_dmc_dma_during_read4_2007_write,
        "roms/blargg/dmc_dma_during_read4/dma_2007_write.nes",
        300
    );
    blargg_console_test!(
        test_dmc_dma_during_read4_4016_read,
        "roms/blargg/dmc_dma_during_read4/dma_4016_read.nes",
        300
    );
    blargg_console_crc_test!(
        test_dmc_dma_during_read4_double_2007_read,
        "roms/blargg/dmc_dma_during_read4/double_2007_read.nes",
        300,
        &[0xF018C287, 0xD84F6815] //CRC1 - Mesen, loopyNES, etc., CRC2 - Nintendulator, FCEUX
    );
    blargg_console_test!(
        test_dmc_dma_during_read4_read_write_2007,
        "roms/blargg/dmc_dma_during_read4/read_write_2007.nes",
        300
    );
    blargg_test!(
        test_cpu_cli_latency,
        "roms/blargg/cpu_interrupts_v2/rom_singles/1-cli_latency.nes"
    );
    blargg_test!(
        test_cpu_nmi_and_brk,
        "roms/blargg/cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"
    );
    // #[ignore = "Not working"]
    // #[test]
    // fn test_cpu_nmi_and_irq() {}
    blargg_test!(
        test_cpu_nmi_and_irq,
        "roms/blargg/cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"
    );
    blargg_test!(
        test_cpu_irq_and_dma,
        "roms/blargg/cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"
    );
    #[ignore = "Not working"]
    #[test]
    fn test_cpu_branch_delays_irq() {}
    // blargg_test!(
    //     test_cpu_branch_delays_irq,
    //     "roms/blargg/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"
    // );
    blargg_test!(
        test_cpu_reset_registers,
        "roms/blargg/cpu_reset/registers.nes"
    );
    blargg_test!(
        test_cpu_reset_ram_after_reset,
        "roms/blargg/cpu_reset/ram_after_reset.nes"
    );
    blargg_console_test!(
        test_cpu_timing_test,
        "roms/blargg/cpu_timing_test6/cpu_timing_test.nes",
        20 * 60 // Can take up to 16 * 60 frames according to README
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
    blargg_test!(
        test_instr_timing,
        "roms/blargg/instr_timing/instr_timing.nes",
        30 * 60 // According to README, this test can take up to 25 seconds, so let's run it for 30*60 frames
    );
    blargg_console_test!(
        test_palette_ram,
        "roms/blargg/blargg_ppu_tests_2005.09.15b/palette_ram.nes"
    );
    // DISABLED since it matches against the palette values of Blargg's NES
    // blargg_console_test!(
    //     test_power_up_palette,
    //     "roms/blargg/blargg_ppu_tests_2005.09.15b/power_up_palette.nes"
    // );
    blargg_console_test!(
        test_sprite_ram,
        "roms/blargg/blargg_ppu_tests_2005.09.15b/sprite_ram.nes"
    );
    blargg_console_test!(
        test_vbl_clear_time,
        "roms/blargg/blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes"
    );
    blargg_console_test!(
        test_vram_access,
        "roms/blargg/blargg_ppu_tests_2005.09.15b/vram_access.nes"
    );
    blargg_test!(test_oam_read, "roms/blargg/oam_read/oam_read.nes");
    blargg_test!(
        test_oam_stress,
        "roms/blargg/oam_stress/oam_stress.nes",
        60 * 10
    );
    blargg_test!(
        test_ppu_open_bus,
        "roms/blargg/ppu_open_bus/ppu_open_bus.nes"
    );
    blargg_test!(
        test_ppu_read_buffer,
        "roms/blargg/ppu_read_buffer/test_ppu_read_buffer.nes",
        60 * 25 // Takes about 20 seconds according to readme
    );
    blargg_test!(
        test_sprite_hit,
        "roms/blargg/ppu_sprite_hit/ppu_sprite_hit.nes",
        60 * 5
    );
    blargg_test!(
        test_sprite_overflow,
        "roms/blargg/ppu_sprite_overflow/ppu_sprite_overflow.nes"
    );

    blargg_test!(test_4015_cleared, "roms/blargg/apu_reset/4015_cleared.nes");
    // #[ignore = "Not working"]
    // #[test]
    // fn test_4017_timing() { }
    blargg_test!(test_4017_timing, "roms/blargg/apu_reset/4017_timing.nes");
    blargg_test!(test_4017_written, "roms/blargg/apu_reset/4017_written.nes");
    blargg_test!(
        test_irq_flag_cleared,
        "roms/blargg/apu_reset/irq_flag_cleared.nes"
    );
    blargg_test!(
        test_len_ctrs_enabled,
        "roms/blargg/apu_reset/len_ctrs_enabled.nes"
    );
    blargg_test!(
        test_works_immediately,
        "roms/blargg/apu_reset/works_immediately.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_01,
        "roms/blargg/ppu_vbl_nmi/rom_singles/01-vbl_basics.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_02,
        "roms/blargg/ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_03,
        "roms/blargg/ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_04,
        "roms/blargg/ppu_vbl_nmi/rom_singles/04-nmi_control.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_05,
        "roms/blargg/ppu_vbl_nmi/rom_singles/05-nmi_timing.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_06,
        "roms/blargg/ppu_vbl_nmi/rom_singles/06-suppression.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_07,
        "roms/blargg/ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_08,
        "roms/blargg/ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_09,
        "roms/blargg/ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes"
    );
    blargg_test!(
        test_ppu_vbl_nmi_10,
        "roms/blargg/ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes"
    );
    blargg_test!(
        test_apu_test_1,
        "roms/blargg/apu_test/rom_singles/1-len_ctr.nes"
    );
    blargg_test!(
        test_apu_test_2,
        "roms/blargg/apu_test/rom_singles/2-len_table.nes"
    );
    blargg_test!(
        test_apu_test_3,
        "roms/blargg/apu_test/rom_singles/3-irq_flag.nes"
    );
    blargg_test!(
        test_apu_test_4,
        "roms/blargg/apu_test/rom_singles/4-jitter.nes"
    );
    blargg_test!(
        test_apu_test_5,
        "roms/blargg/apu_test/rom_singles/5-len_timing.nes"
    );
    blargg_test!(
        test_apu_test_6,
        "roms/blargg/apu_test/rom_singles/6-irq_flag_timing.nes"
    );
    blargg_test!(
        test_apu_test_7,
        "roms/blargg/apu_test/rom_singles/7-dmc_basics.nes"
    );
    blargg_test!(
        test_apu_test_8,
        "roms/blargg/apu_test/rom_singles/8-dmc_rates.nes"
    );

    blargg_test!(test_apu_mixer_dmc, "roms/blargg/apu_mixer/dmc.nes", 60 * 10);
    blargg_test!(
        test_apu_mixer_noise,
        "roms/blargg/apu_mixer/noise.nes",
        60 * 10
    );
    blargg_test!(
        test_apu_mixer_triangle,
        "roms/blargg/apu_mixer/triangle.nes",
        60 * 10
    );
    blargg_test!(
        test_apu_mixer_square,
        "roms/blargg/apu_mixer/square.nes",
        60 * 10
    );

    // blargg_test!(
    //     test_dmc_tests_buffer_retained,
    //     "roms/blargg/dmc_tests/buffer_retained.nes",
    //     60 * 10
    // );
    // blargg_test!(
    //     test_dmc_tests_latency,
    //     "roms/blargg/dmc_tests/latency.nes",
    //     60 * 10
    // );
    // blargg_test!(
    //     test_dmc_tests_status,
    //     "roms/blargg/dmc_tests/status.nes",
    //     60 * 10
    // );
    // blargg_test!(
    //     test_dmc_tests_status_irq,
    //     "roms/blargg/dmc_tests/status_irq.nes",
    //     60 * 10
    // );
}
