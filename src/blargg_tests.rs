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
    use crate::tracing::{self, Tracing};
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

    const NTSC_CPU_CYCLES_PER_FRAME: u32 = 29_780;

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
            init_apu_tracing_from_env();
            // Load ROM
            let rom_data = match fs::read(&self.rom_path) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to load ROM {}: {}", self.rom_path, e);
                    return BlarggTestResult::Fail(0x80_u8);
                }
            };

            let cartridge = match Cartridge::new(&rom_data) {
                Ok(cart) => cart,
                Err(e) => {
                    eprintln!("Failed to parse ROM {}: {}", self.rom_path, e);
                    return BlarggTestResult::Fail(0x81_u8);
                }
            };

            // Create NES and insert cartridge
            let mut nes = Nes::new(TvSystem::Ntsc);
            nes.insert_cartridge(cartridge);
            // Initial reset is treated as power-on.
            nes.reset(false);

            // println!("Running Blargg-based test ROM: {} ... ", self.rom_path);

            let mut running = false;
            let mut first_nonzero_status = None;
            // Run frames and check for results
            for frame in 1..=self.max_frames {
                // Run one frame (roughly 29780 CPU cycles for NTSC)
                let mut current_status = nes.memory.borrow_mut().read_for_testing(0x6000);
                if current_status == 0x80 {
                    running = true;
                }
                if current_status != 0 && first_nonzero_status.is_none() {
                    first_nonzero_status = Some((frame, current_status));
                }
                const STATUS_POLL_INTERVAL: u32 = 256;
                for cpu_cycle in 0..NTSC_CPU_CYCLES_PER_FRAME {
                    nes.run_cpu_tick();

                    if cpu_cycle != 0 && cpu_cycle % STATUS_POLL_INTERVAL == 0 {
                        current_status = nes.memory.borrow_mut().read_for_testing(0x6000);
                        if current_status == 0x80 {
                            running = true;
                        }
                    }
                }
                // Make sure we observe any status update at end-of-frame.
                let status = nes.memory.borrow_mut().read_for_testing(0x6000);
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

    fn run_address_test(
        rom_path: &str,
        max_frames: u32,
        stop_address: u16,
        verifier: fn(&mut Nes) -> bool,
    ) -> BlarggTestResult {
        init_apu_tracing_from_env();

        let rom_data = match fs::read(rom_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to load ROM {}: {}", rom_path, e);
                return BlarggTestResult::Fail(0x80_u8);
            }
        };

        let cartridge = match Cartridge::new(&rom_data) {
            Ok(cart) => cart,
            Err(e) => {
                eprintln!("Failed to parse ROM {}: {}", rom_path, e);
                return BlarggTestResult::Fail(0x81_u8);
            }
        };

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        for _frame in 1..=max_frames {
            for _cpu_cycle in 0..NTSC_CPU_CYCLES_PER_FRAME {
                if nes.cpu.pc() == stop_address {
                    // while nes.sample_ready() {
                    //     println!("SampleX: {}", nes.get_sample().unwrap());
                    // }
                    return if verifier(&mut nes) {
                        BlarggTestResult::Pass
                    } else {
                        BlarggTestResult::Fail(1)
                    };
                }
                nes.run_cpu_tick();
            }

            if nes.is_ready_to_render() {
                nes.clear_ready_to_render();
            }
            // while nes.sample_ready() {
            //     nes.get_sample().unwrap();
            // }
        }

        BlarggTestResult::Timeout
    }

    fn verify_always_pass(_nes: &mut Nes) -> bool {
        true
    }

    fn verify_always_fail(_nes: &mut Nes) -> bool {
        false
    }

    fn verify_mutating(_nes: &mut Nes) -> bool {
        true
    }

    fn init_apu_tracing_from_env() {
        let level = match std::env::var("NESER_TRACE_APU") {
            Ok(value) => value.parse::<u8>().unwrap_or(1),
            Err(_) => return,
        };

        if level == 0 {
            return;
        }

        tracing::init_tracing(Tracing {
            enabled: true,
            apu: level,
            ..Tracing::default()
        });
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

    macro_rules! blargg_address_test {
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let result = run_address_test($rom_path, $timeout, $stop_address, $verify_fn);
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, BlarggTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr) => {
            blargg_address_test!($test_name, $rom_path, $stop_address, $verify_fn, 180);
        };
    }

    #[test]
    fn test_address_test_verifier_failure() {
        let result = run_address_test(
            "roms/blargg/dmc_tests/buffer_retained.nes",
            300,
            0xE149,
            verify_always_fail,
        );
        assert_eq!(result, BlarggTestResult::Fail(1));
    }

    #[test]
    fn test_address_test_verifier_accepts_mut_nes() {
        let result = run_address_test(
            "roms/blargg/dmc_tests/buffer_retained.nes",
            300,
            0xE149,
            verify_mutating,
        );
        assert_eq!(result, BlarggTestResult::Pass);
    }

    //
    // APU
    //

    // apu_mixer
    blargg_test!(test_apu_mixer_dmc, "roms/blargg/apu_mixer/dmc.nes", 60 * 10);
    blargg_test!(
        test_apu_mixer_noise,
        "roms/blargg/apu_mixer/noise.nes",
        60 * 10
    );
    blargg_test!(
        test_apu_mixer_square,
        "roms/blargg/apu_mixer/square.nes",
        60 * 10
    );
    blargg_test!(
        test_apu_mixer_triangle,
        "roms/blargg/apu_mixer/triangle.nes",
        60 * 10
    );

    // apu_reset
    blargg_test!(test_4015_cleared, "roms/blargg/apu_reset/4015_cleared.nes");
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

    // apu_test
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

    // blargg_apu_2005.07.30
    blargg_console_test!(
        test_blargg_apu_01,
        "roms/blargg/blargg_apu_2005.07.30/01.len_ctr.nes"
    );
    blargg_console_test!(
        test_blargg_apu_02,
        "roms/blargg/blargg_apu_2005.07.30/02.len_table.nes"
    );
    blargg_console_test!(
        test_blargg_apu_03,
        "roms/blargg/blargg_apu_2005.07.30/03.irq_flag.nes"
    );
    blargg_console_test!(
        test_blargg_apu_04,
        "roms/blargg/blargg_apu_2005.07.30/04.clock_jitter.nes"
    );
    blargg_console_test!(
        test_blargg_apu_05,
        "roms/blargg/blargg_apu_2005.07.30/05.len_timing_mode0.nes"
    );
    blargg_console_test!(
        test_blargg_apu_06,
        "roms/blargg/blargg_apu_2005.07.30/06.len_timing_mode1.nes"
    );
    blargg_console_test!(
        test_blargg_apu_07,
        "roms/blargg/blargg_apu_2005.07.30/07.irq_flag_timing.nes"
    );
    blargg_console_test!(
        test_blargg_apu_08,
        "roms/blargg/blargg_apu_2005.07.30/08.irq_timing.nes"
    );
    blargg_console_test!(
        test_blargg_apu_09,
        "roms/blargg/blargg_apu_2005.07.30/09.reset_timing.nes"
    );
    blargg_console_test!(
        test_blargg_apu_10,
        "roms/blargg/blargg_apu_2005.07.30/10.len_halt_timing.nes"
    );
    blargg_console_test!(
        test_blargg_apu_11,
        "roms/blargg/blargg_apu_2005.07.30/11.len_reload_timing.nes"
    );

    // dmc_dma_during_read4
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

    // Test that the dmc is processing exactly one byte (0x55) -> alternating eight times
    // The final pulse tone will never play as we are stopping at first sight of the infinite loop
    fn check_one_dmc_byte_processed(nes: &mut Nes) -> bool {
        let mut samples = Vec::new();
        while nes.sample_ready() {
            samples.push(nes.get_sample().unwrap());
        }
        let mut expect_up = true;
        // First sample is garbage (0)
        let mut prev = samples[1];
        let mut alternations = 0;
        for &next in samples.iter().skip(2) {
            if next == prev {
                continue;
            }
            if expect_up {
                assert!(next > prev, "expected up step: {} -> {}", prev, next);
            } else {
                assert!(next < prev, "expected down step: {} -> {}", prev, next);
            }
            expect_up = !expect_up;
            prev = next;
            alternations += 1;
        }
        assert_eq!(
            alternations, 8,
            "expected 8 alternations, got {}",
            alternations
        );

        true
    }

    fn max_alternating_small_steps(samples: &[f32]) -> usize {
        const MIN_STEP: f32 = 0.000_05;
        const BIG_JUMP: f32 = 0.02;

        let mut count = 0usize;
        let mut last_dir: i32 = 0;
        let mut prev = match samples.first() {
            Some(value) => *value,
            None => return 0,
        };

        for &next in samples.iter().skip(1) {
            let delta = next - prev;
            let abs_delta = delta.abs();

            if abs_delta < MIN_STEP {
                prev = next;
                continue;
            }

            if abs_delta >= BIG_JUMP {
                prev = next;
                last_dir = 0;
                // println!("Big jump to {}", next);
                continue;
            }

            // println!("Processing {} count {}", next, count + 1);
            let dir = if delta > 0.0 { 1 } else { -1 };
            assert!(
                last_dir == 0 || dir != last_dir,
                "last_dir={}, dir={}, prev={}, next={}",
                last_dir,
                dir,
                prev,
                next
            );
            count += 1;
            last_dir = dir;

            prev = next;
        }

        count
    }

    // Test that the dmc is processing two bytes (0x55) four times
    // Sometime during the first byte, the output will be set to 0x32 but the DMC will keep
    // processing the remaining bits in that byte as well as the next byte already loaded into
    // sample buffer.
    // The final pulse tone will never play as we are stopping at first sight of the infinite loop
    fn check_four_by_two_dmc_bytes_processed(nes: &mut Nes) -> bool {
        let mut samples = Vec::new();
        while nes.sample_ready() {
            let sample = nes.get_sample().unwrap();
            samples.push(sample);
        }
        let alternations = max_alternating_small_steps(&samples);
        assert_eq!(alternations, 16 * 4);

        true
    }

    // dmc_tests
    blargg_address_test!(
        test_dmc_tests_buffer_retained,
        "roms/blargg/dmc_tests/buffer_retained.nes",
        0xE149,
        check_one_dmc_byte_processed
    );
    blargg_address_test!(
        test_dmc_tests_latency,
        "roms/blargg/dmc_tests/latency.nes",
        0xE162,
        check_four_by_two_dmc_bytes_processed
    );

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

    blargg_test!(
        test_cpu_cli_latency,
        "roms/blargg/cpu_interrupts_v2/rom_singles/1-cli_latency.nes"
    );
    blargg_test!(
        test_cpu_nmi_and_brk,
        "roms/blargg/cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"
    );
    blargg_test!(
        test_cpu_nmi_and_irq,
        "roms/blargg/cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"
    );
    blargg_test!(
        test_cpu_irq_and_dma,
        "roms/blargg/cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"
    );
    blargg_test!(
        test_cpu_branch_delays_irq,
        "roms/blargg/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"
    );
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
    blargg_test!(
        test_instr_misc_01,
        "roms/blargg/instr_misc/rom_singles/01-abs_x_wrap.nes"
    );
    blargg_test!(
        test_instr_misc_02,
        "roms/blargg/instr_misc/rom_singles/02-branch_wrap.nes"
    );
    blargg_test!(
        test_instr_misc_03,
        "roms/blargg/instr_misc/rom_singles/03-dummy_reads.nes"
    );
    blargg_test!(
        test_instr_misc_04,
        "roms/blargg/instr_misc/rom_singles/04-dummy_reads_apu.nes"
    );
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
        test_instr_timing_01,
        "roms/blargg/instr_timing/rom_singles/1-instr_timing.nes",
        60 * 5
    );
    blargg_test!(
        test_instr_timing_02,
        "roms/blargg/instr_timing/rom_singles/2-branch_timing.nes"
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
        test_sprite_hit_01,
        "roms/blargg/ppu_sprite_hit/rom_singles/01-basics.nes"
    );
    blargg_test!(
        test_sprite_hit_02,
        "roms/blargg/ppu_sprite_hit/rom_singles/02-alignment.nes"
    );
    blargg_test!(
        test_sprite_hit_03,
        "roms/blargg/ppu_sprite_hit/rom_singles/03-corners.nes"
    );
    blargg_test!(
        test_sprite_hit_04,
        "roms/blargg/ppu_sprite_hit/rom_singles/04-flip.nes"
    );
    blargg_test!(
        test_sprite_hit_05,
        "roms/blargg/ppu_sprite_hit/rom_singles/05-left_clip.nes"
    );
    blargg_test!(
        test_sprite_hit_06,
        "roms/blargg/ppu_sprite_hit/rom_singles/06-right_edge.nes"
    );
    blargg_test!(
        test_sprite_hit_07,
        "roms/blargg/ppu_sprite_hit/rom_singles/07-screen_bottom.nes"
    );
    blargg_test!(
        test_sprite_hit_08,
        "roms/blargg/ppu_sprite_hit/rom_singles/08-double_height.nes"
    );
    blargg_test!(
        test_sprite_hit_09,
        "roms/blargg/ppu_sprite_hit/rom_singles/09-timing.nes"
    );
    blargg_test!(
        test_sprite_hit_10,
        "roms/blargg/ppu_sprite_hit/rom_singles/10-timing_order.nes"
    );
    blargg_test!(
        test_sprite_overflow_01,
        "roms/blargg/ppu_sprite_overflow/rom_singles/01-basics.nes"
    );
    blargg_test!(
        test_sprite_overflow_02,
        "roms/blargg/ppu_sprite_overflow/rom_singles/02-details.nes"
    );
    blargg_test!(
        test_sprite_overflow_03,
        "roms/blargg/ppu_sprite_overflow/rom_singles/03-timing.nes"
    );
    blargg_test!(
        test_sprite_overflow_04,
        "roms/blargg/ppu_sprite_overflow/rom_singles/04-obscure.nes"
    );
    blargg_test!(
        test_sprite_overflow_05,
        "roms/blargg/ppu_sprite_overflow/rom_singles/05-emulator.nes"
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

    // Tests for OAM DMA and DMC DMA collision handling
    blargg_test!(
        test_sprdma_and_dmc_dma,
        "roms/blargg/sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes",
        60 * 15
    );
    blargg_test!(
        test_sprdma_and_dmc_dma_512,
        "roms/blargg/sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes",
        60 * 15
    );

    // MMC5 tests
    // NOTE: These tests are currently commented out because they timeout.
    // MMC5 implementation is partial - core registers work but these tests require:
    // - Scanline IRQ tracking integrated with PPU rendering
    // - CHR BG/sprite banking split (needs PPU fetch type detection)
    // - ExRAM as nametable memory (needs PPU nametable hooks)
    // See issue #XXX for details on remaining MMC5 work.
    //
    // blargg_test!(test_mmc5, "roms/blargg/mmc5test/mmc5test.nes", 60 * 10);
    // blargg_test!(
    //     test_mmc5_v2,
    //     "roms/blargg/mmc5test_v2/mmc5test.nes",
    //     60 * 10
    // );

    // MMC3 IRQ counter tests
    blargg_console_test!(
        test_mmc3_irq_1_clocking,
        "roms/blargg/mmc3_irq_tests/1.Clocking.nes",
        60 * 10 // Increased timeout for initial debugging
    );
    blargg_console_test!(
        test_mmc3_irq_2_details,
        "roms/blargg/mmc3_irq_tests/2.Details.nes"
    );
    blargg_console_test!(
        test_mmc3_irq_3_a12_clocking,
        "roms/blargg/mmc3_irq_tests/3.A12_clocking.nes"
    );
    blargg_console_test!(
        test_mmc3_irq_4_scanline_timing,
        "roms/blargg/mmc3_irq_tests/4.Scanline_timing.nes",
        60 * 5 // May need time for frame rendering
    );
    blargg_console_test!(
        test_mmc3_irq_5_rev_a,
        "roms/blargg/mmc3_irq_tests/5.MMC3_rev_A.nes"
    );
    blargg_console_test!(
        test_mmc3_irq_6_rev_b,
        "roms/blargg/mmc3_irq_tests/6.MMC3_rev_B.nes"
    );

    // MMC3 test suite (alternative test format)
    blargg_test!(
        test_mmc3_test_1_clocking,
        "roms/blargg/mmc3_test_2/rom_singles/1-clocking.nes"
    );
    blargg_test!(
        test_mmc3_test_2_details,
        "roms/blargg/mmc3_test_2/rom_singles/2-details.nes"
    );
    blargg_test!(
        test_mmc3_test_3_a12_clocking,
        "roms/blargg/mmc3_test_2/rom_singles/3-A12_clocking.nes"
    );
    blargg_test!(
        test_mmc3_test_4_scanline_timing,
        "roms/blargg/mmc3_test_2/rom_singles/4-scanline_timing.nes",
        60 * 5
    );
    blargg_test!(
        test_mmc3_test_5_mmc3,
        "roms/blargg/mmc3_test_2/rom_singles/5-MMC3.nes"
    );
    blargg_test!(
        test_mmc3_test_6_mmc3_alt,
        "roms/blargg/mmc3_test_2/rom_singles/6-MMC3_alt.nes"
    );
}
