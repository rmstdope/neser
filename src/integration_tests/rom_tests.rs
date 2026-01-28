#[cfg(test)]
mod tests {
    /// OAM test infrastructure for automated testing of test ROMs
    ///
    /// This module provides infrastructure to run OAM test ROMs (oam_read, oam_stress, oam3)
    /// and automatically detect PASS/FAIL status by reading results from PRG-RAM.
    ///
    /// There are four kinds of ROM test automation strategies supported.
    /// 1. RAM based results ($6000-$6003):
    /// - $6000 = 0x00: Test passed
    /// - $6000 = 0x01+: Test failed with error code
    /// - $6000 = 0x80: Test is still running
    /// - $6000 = 0x81: Test requests a reset
    /// 2. Console output based results:
    /// - The test ROM prints text to the nametable area. The test is considered passed if the text
    ///   contains "PASSED" and failed if it contains "FAILED" or "ERROR".
    /// 3. Console output based results with CRC-32 matching:
    /// - The test ROM prints text to the nametable area. The test is considered passed if the text
    ///   contains a CRC-32 value that matches one of the expected values.
    /// 4. Address-based tests:
    /// - The test ROM runs until a specific CPU address is reached, at which point a custom verifier
    ///   function is called to determine pass/fail.
    ///
    use crate::cartridge::Cartridge;
    use crate::console::{Nes, TvSystem};
    use crate::debugging::{Tracing, init_tracing};
    use std::fs;

    /// Result of running an test ROM
    #[derive(Debug, PartialEq, Eq)]
    pub enum RomTestResult {
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
    pub enum RomTestVerification {
        /// Verify using status byte at 0x6000
        StatusByte,
        /// Verify using console output
        Console,
        /// Verify by matching a CRC-32 printed to the console output.
        ConsoleCrc(&'static [u32]),
    }

    /// Runner for OAM test ROMs
    pub struct RomTestRunner {
        rom_path: String,
        max_frames: u32,
        wait_reset: u32,
        verification: RomTestVerification,
    }

    impl RomTestRunner {
        /// Create a new test runner for $6000-based tests
        pub fn new(rom_path: &str, max_frames: u32, verification: RomTestVerification) -> Self {
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
        pub fn run_test(&mut self) -> RomTestResult {
            init_apu_tracing_from_env();
            // Load ROM
            let rom_data = match fs::read(&self.rom_path) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to load ROM {}: {}", self.rom_path, e);
                    return RomTestResult::Fail(0x80_u8);
                }
            };

            let cartridge = match Cartridge::new(&rom_data) {
                Ok(cart) => cart,
                Err(e) => {
                    eprintln!("Failed to parse ROM {}: {}", self.rom_path, e);
                    return RomTestResult::Fail(0x81_u8);
                }
            };

            // Create NES and insert cartridge
            let mut nes = Nes::new(TvSystem::Ntsc);
            nes.insert_cartridge(cartridge);
            // Initial reset is treated as power-on.
            nes.reset(false);

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

                if self.verification == RomTestVerification::StatusByte && !running {
                    continue;
                }
                if self.verification == RomTestVerification::StatusByte {
                    if status == 0x00 {
                        // println!("Test passed!");
                        return RomTestResult::Pass;
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
                        return RomTestResult::Fail(status);
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
                } else if self.verification == RomTestVerification::Console {
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
                    if text.to_uppercase().contains("PASSED")
                        || text == "$01"
                        || text.to_uppercase().contains("ALL TESTS COMPLETE")
                    {
                        // println!("Test passed!");
                        return RomTestResult::Pass;
                    } else if text.to_uppercase().contains("FAILED")
                        || text.to_uppercase().contains("ERROR")
                        || is_0x
                    {
                        println!("Test failed!");
                        println!("Console output:\n{}", text);
                        return RomTestResult::Fail(1);
                    }
                } else if let RomTestVerification::ConsoleCrc(expected_crcs) = self.verification {
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
                            return RomTestResult::Pass;
                        }
                        println!("Test failed! Unexpected CRC 0x{:08X}", crc);
                        println!("Console output:\n{}", text);
                        return RomTestResult::Fail(1);
                    }
                }
            }

            // No result found within timeout
            RomTestResult::Timeout
        }
    }

    fn run_address_test(
        rom_path: &str,
        max_frames: u32,
        stop_address: u16,
        verifier: fn(&mut Nes) -> bool,
    ) -> RomTestResult {
        init_apu_tracing_from_env();

        let rom_data = match fs::read(rom_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to load ROM {}: {}", rom_path, e);
                return RomTestResult::Fail(0x80_u8);
            }
        };

        let cartridge = match Cartridge::new(&rom_data) {
            Ok(cart) => cart,
            Err(e) => {
                eprintln!("Failed to parse ROM {}: {}", rom_path, e);
                return RomTestResult::Fail(0x81_u8);
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
                        RomTestResult::Pass
                    } else {
                        RomTestResult::Fail(1)
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

        RomTestResult::Timeout
    }

    fn run_nes_for_frames(nes: &mut Nes, frames: u32) {
        if frames == 0 {
            return;
        }

        let max_ticks: u64 = 200_000_000;

        let mut frames_completed = 0u32;
        let mut ticks = 0u64;

        while frames_completed < frames {
            nes.run_cpu_tick();
            ticks += 1;
            if ticks > max_ticks {
                panic!(
                    "Timed out running {} frames (only reached {})",
                    frames, frames_completed
                );
            }

            while nes.sample_ready() {
                nes.get_sample();
            }

            if nes.is_ready_to_render() {
                frames_completed += 1;
                nes.clear_ready_to_render();
            }
        }
    }

    fn capture_scanline_rgb(nes: &Nes, y: u32) -> Vec<(u8, u8, u8)> {
        let screen_buffer = nes.get_screen_buffer();
        (0..TvSystem::Ntsc.screen_width())
            .map(|x| screen_buffer.get_pixel(x, y))
            .collect()
    }

    fn matches_white_run(
        line: &[(u8, u8, u8)],
        start_x: usize,
        end_x: usize,
        white: (u8, u8, u8),
        black: (u8, u8, u8),
    ) -> bool {
        if start_x > end_x || end_x >= line.len() {
            return false;
        }

        if start_x > 0 && line[start_x - 1] != black {
            return false;
        }

        if end_x + 1 < line.len() && line[end_x + 1] != black {
            return false;
        }

        line[start_x..=end_x].iter().all(|&pixel| pixel == white)
    }

    fn init_apu_tracing_from_env() {
        let level = match std::env::var("NESER_TRACE_APU") {
            Ok(value) => value.parse::<u8>().unwrap_or(1),
            Err(_) => return,
        };

        if level == 0 {
            return;
        }

        init_tracing(Tracing {
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

    // Macro to generate $6000-based tests with custom timeout
    macro_rules! setup_rom_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner =
                    RomTestRunner::new($rom_path, $timeout, RomTestVerification::StatusByte);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, RomTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            setup_rom_test!($test_name, $rom_path, 180);
        };
    }

    macro_rules! setup_rom_console_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner =
                    RomTestRunner::new($rom_path, $timeout, RomTestVerification::Console);
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, RomTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            setup_rom_console_test!($test_name, $rom_path, 180);
        };
    }

    macro_rules! setup_rom_console_crc_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    RomTestVerification::ConsoleCrc($expected),
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, RomTestResult::Pass, "{} should pass", rom_name);
            }
        };
    }

    macro_rules! setup_rom_address_test {
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let result = run_address_test($rom_path, $timeout, $stop_address, $verify_fn);
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(result, RomTestResult::Pass, "{} should pass", rom_name);
            }
        };
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr) => {
            setup_rom_address_test!($test_name, $rom_path, $stop_address, $verify_fn, 180);
        };
    }

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

    // Check that exaclty one IRQ has been fired from the DMC
    fn check_one_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu.borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 1, "expected 1 IRQ fired, got {}", irq_count);
        true
    }

    // Check that exaclty zero IRQ has been fired from the DMC
    fn check_zero_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu.borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 0, "expected 0 IRQ fired, got {}", irq_count);
        true
    }

    /////////////////////////////////////
    // APU
    /////////////////////////////////////

    // apu_mixer
    setup_rom_test!(
        test_apu_mixer_dmc,
        "roms/automated_tests/apu_mixer/dmc.nes",
        60 * 10
    );
    setup_rom_test!(
        test_apu_mixer_noise,
        "roms/automated_tests/apu_mixer/noise.nes",
        60 * 10
    );
    setup_rom_test!(
        test_apu_mixer_square,
        "roms/automated_tests/apu_mixer/square.nes",
        60 * 10
    );
    setup_rom_test!(
        test_apu_mixer_triangle,
        "roms/automated_tests/apu_mixer/triangle.nes",
        60 * 10
    );

    // apu_reset
    setup_rom_test!(
        test_4015_cleared,
        "roms/automated_tests/apu_reset/4015_cleared.nes"
    );
    setup_rom_test!(
        test_4017_timing,
        "roms/automated_tests/apu_reset/4017_timing.nes"
    );
    setup_rom_test!(
        test_4017_written,
        "roms/automated_tests/apu_reset/4017_written.nes"
    );
    setup_rom_test!(
        test_irq_flag_cleared,
        "roms/automated_tests/apu_reset/irq_flag_cleared.nes"
    );
    setup_rom_test!(
        test_len_ctrs_enabled,
        "roms/automated_tests/apu_reset/len_ctrs_enabled.nes"
    );
    setup_rom_test!(
        test_works_immediately,
        "roms/automated_tests/apu_reset/works_immediately.nes"
    );

    // apu_test
    setup_rom_test!(
        test_apu_test_1,
        "roms/automated_tests/apu_test/rom_singles/1-len_ctr.nes"
    );
    setup_rom_test!(
        test_apu_test_2,
        "roms/automated_tests/apu_test/rom_singles/2-len_table.nes"
    );
    setup_rom_test!(
        test_apu_test_3,
        "roms/automated_tests/apu_test/rom_singles/3-irq_flag.nes"
    );
    setup_rom_test!(
        test_apu_test_4,
        "roms/automated_tests/apu_test/rom_singles/4-jitter.nes"
    );
    setup_rom_test!(
        test_apu_test_5,
        "roms/automated_tests/apu_test/rom_singles/5-len_timing.nes"
    );
    setup_rom_test!(
        test_apu_test_6,
        "roms/automated_tests/apu_test/rom_singles/6-irq_flag_timing.nes"
    );
    setup_rom_test!(
        test_apu_test_7,
        "roms/automated_tests/apu_test/rom_singles/7-dmc_basics.nes"
    );
    setup_rom_test!(
        test_apu_test_8,
        "roms/automated_tests/apu_test/rom_singles/8-dmc_rates.nes"
    );

    // blargg_apu_2005.07.30
    setup_rom_console_test!(
        test_blargg_apu_01,
        "roms/automated_tests/blargg_apu_2005.07.30/01.len_ctr.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_02,
        "roms/automated_tests/blargg_apu_2005.07.30/02.len_table.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_03,
        "roms/automated_tests/blargg_apu_2005.07.30/03.irq_flag.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_04,
        "roms/automated_tests/blargg_apu_2005.07.30/04.clock_jitter.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_05,
        "roms/automated_tests/blargg_apu_2005.07.30/05.len_timing_mode0.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_06,
        "roms/automated_tests/blargg_apu_2005.07.30/06.len_timing_mode1.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_07,
        "roms/automated_tests/blargg_apu_2005.07.30/07.irq_flag_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_08,
        "roms/automated_tests/blargg_apu_2005.07.30/08.irq_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_09,
        "roms/automated_tests/blargg_apu_2005.07.30/09.reset_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_10,
        "roms/automated_tests/blargg_apu_2005.07.30/10.len_halt_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_11,
        "roms/automated_tests/blargg_apu_2005.07.30/11.len_reload_timing.nes"
    );

    // dmc_dma_during_read4
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_2007_read,
        "roms/automated_tests/dmc_dma_during_read4/dma_2007_read.nes",
        300,
        &[0x159A7A8F, 0x5E3DF9C4]
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_2007_write,
        "roms/automated_tests/dmc_dma_during_read4/dma_2007_write.nes",
        300
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_4016_read,
        "roms/automated_tests/dmc_dma_during_read4/dma_4016_read.nes",
        300
    );
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_double_2007_read,
        "roms/automated_tests/dmc_dma_during_read4/double_2007_read.nes",
        300,
        &[0xF018C287, 0xD84F6815] //CRC1 - Mesen, loopyNES, etc., CRC2 - Nintendulator, FCEUX
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_read_write_2007,
        "roms/automated_tests/dmc_dma_during_read4/read_write_2007.nes",
        300
    );

    // dmc_tests
    setup_rom_address_test!(
        test_dmc_tests_buffer_retained,
        "roms/automated_tests/dmc_tests/buffer_retained.nes",
        0xE149,
        check_one_dmc_byte_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_latency,
        "roms/automated_tests/dmc_tests/latency.nes",
        0xE162,
        check_four_by_two_dmc_bytes_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_status_irq,
        "roms/automated_tests/dmc_tests/status_irq.nes",
        0xE154,
        check_one_irq_fired
    );
    setup_rom_address_test!(
        test_dmc_tests_status,
        "roms/automated_tests/dmc_tests/status.nes",
        0xE14E,
        check_zero_irq_fired
    );

    // dpcmletterbox
    #[test]
    fn test_dpcmletterbox() {
        let rom_path = "roms/automated_tests/dpcmletterbox/dpcmletterbox.nes";
        let rom_data = fs::read(rom_path).expect("dpcmletterbox ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("dpcmletterbox ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        run_nes_for_frames(&mut nes, 60);

        let crc = nes.get_screen_buffer().crc32();
        // Golden CRC generated from a visually verified correct frame capture using --trace-ppu=1
        assert_eq!(crc, 0x2813_2E95, "unexpected frame CRC for dpcmletterbox");
    }

    // volume_tests
    // TODO Not automated yet. Requires audio output analysis

    /////////////////////////////////////
    // CPU
    /////////////////////////////////////

    // blargg_nes_cpu_test5
    setup_rom_console_test!(
        test_blargg_nes_cpu_test5_cpu,
        "roms/automated_tests/blargg_nes_cpu_test5/cpu.nes",
        60 * 20
    );

    setup_rom_console_test!(
        test_blargg_nes_cpu_test5_official,
        "roms/automated_tests/blargg_nes_cpu_test5/official.nes",
        60 * 20
    );

    // branch_timing_tests
    setup_rom_console_test!(
        test_branch_timing,
        "roms/automated_tests/branch_timing_tests/1.Branch_Basics.nes"
    );
    setup_rom_console_test!(
        test_backward_branch,
        "roms/automated_tests/branch_timing_tests/2.Backward_Branch.nes"
    );
    setup_rom_console_test!(
        test_forward_branch,
        "roms/automated_tests/branch_timing_tests/3.Forward_Branch.nes"
    );

    // cpu_dummy_reads
    setup_rom_console_test!(
        test_cpu_dummy_reads,
        "roms/automated_tests/cpu_dummy_reads/cpu_dummy_reads.nes"
    );

    // cpu_dummy_writes
    setup_rom_test!(
        test_cpu_dummy_writes_oam,
        "roms/automated_tests/cpu_dummy_writes/cpu_dummy_writes_oam.nes"
    );
    setup_rom_test!(
        test_cpu_dummy_writes_ppumem,
        "roms/automated_tests/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"
    );

    // cpu_exec_space
    setup_rom_test!(
        test_cpu_exec_space_ppuio,
        "roms/automated_tests/cpu_exec_space/test_cpu_exec_space_ppuio.nes"
    );
    setup_rom_test!(
        test_cpu_exec_space_apu,
        "roms/automated_tests/cpu_exec_space/test_cpu_exec_space_apu.nes"
    );

    // cpu_interrupts_v2
    setup_rom_test!(
        test_cpu_interrupts_v2_cpu_interrupts,
        "roms/automated_tests/cpu_interrupts_v2/cpu_interrupts.nes",
        60 * 20
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_cli_latency,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/1-cli_latency.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_brk,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_irq,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_irq_and_dma,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_branch_delays_irq,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"
    );

    // cpu_reset
    setup_rom_test!(
        test_cpu_reset_ram_after_reset,
        "roms/automated_tests/cpu_reset/ram_after_reset.nes"
    );
    setup_rom_test!(
        test_cpu_reset_reset_registers,
        "roms/automated_tests/cpu_reset/registers.nes"
    );

    // cpu_timing_test6
    setup_rom_console_test!(
        test_cpu_timing_test6,
        "roms/automated_tests/cpu_timing_test6/cpu_timing_test.nes",
        20 * 60 // Can take up to 16 * 60 frames according to README
    );

    // instr_misc
    setup_rom_test!(
        test_instr_misc,
        "roms/automated_tests/instr_misc/instr_misc.nes"
    );
    setup_rom_test!(
        test_instr_misc_01,
        "roms/automated_tests/instr_misc/rom_singles/01-abs_x_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_02,
        "roms/automated_tests/instr_misc/rom_singles/02-branch_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_03,
        "roms/automated_tests/instr_misc/rom_singles/03-dummy_reads.nes"
    );
    setup_rom_test!(
        test_instr_misc_04,
        "roms/automated_tests/instr_misc/rom_singles/04-dummy_reads_apu.nes"
    );

    // test_instr_v3
    setup_rom_test!(
        test_instr_v3_01_implied,
        "roms/automated_tests/instr_test-v3/rom_singles/01-implied.nes"
    );
    setup_rom_test!(
        test_instr_v3_02_immediate,
        "roms/automated_tests/instr_test-v3/rom_singles/02-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v3_03_zero_page,
        "roms/automated_tests/instr_test-v3/rom_singles/03-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v3_04_zp_xy,
        "roms/automated_tests/instr_test-v3/rom_singles/04-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_05_absolute,
        "roms/automated_tests/instr_test-v3/rom_singles/05-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v3_06_abs_xy,
        "roms/automated_tests/instr_test-v3/rom_singles/06-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_07_ind_x,
        "roms/automated_tests/instr_test-v3/rom_singles/07-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v3_08_ind_y,
        "roms/automated_tests/instr_test-v3/rom_singles/08-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v3_09_branches,
        "roms/automated_tests/instr_test-v3/rom_singles/09-branches.nes"
    );
    setup_rom_test!(
        test_instr_v3_10_stack,
        "roms/automated_tests/instr_test-v3/rom_singles/10-stack.nes"
    );
    setup_rom_test!(
        test_instr_v3_11_jmp_jsr,
        "roms/automated_tests/instr_test-v3/rom_singles/11-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v3_12_rts,
        "roms/automated_tests/instr_test-v3/rom_singles/12-rts.nes"
    );
    setup_rom_test!(
        test_instr_v3_13_rti,
        "roms/automated_tests/instr_test-v3/rom_singles/13-rti.nes"
    );
    setup_rom_test!(
        test_instr_v3_14_brk,
        "roms/automated_tests/instr_test-v3/rom_singles/14-brk.nes"
    );
    setup_rom_test!(
        test_instr_v3_15_special,
        "roms/automated_tests/instr_test-v3/rom_singles/15-special.nes"
    );
    setup_rom_test!(
        test_instr_v3_all_instrs,
        "roms/automated_tests/instr_test-v3/all_instrs.nes",
        60 * 30
    );
    setup_rom_test!(
        test_instr_v3_official_only,
        "roms/automated_tests/instr_test-v3/official_only.nes",
        60 * 30
    );

    // test_instr_v5
    setup_rom_test!(
        test_instr_v5_01_basics,
        "roms/automated_tests/instr_test-v5/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_instr_v5_02_implied,
        "roms/automated_tests/instr_test-v5/rom_singles/02-implied.nes"
    );
    setup_rom_test!(
        test_instr_v5_03_immediate,
        "roms/automated_tests/instr_test-v5/rom_singles/03-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v5_04_zero_page,
        "roms/automated_tests/instr_test-v5/rom_singles/04-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v5_05_zp_xy,
        "roms/automated_tests/instr_test-v5/rom_singles/05-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_06_absolute,
        "roms/automated_tests/instr_test-v5/rom_singles/06-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v5_07_abs_xy,
        "roms/automated_tests/instr_test-v5/rom_singles/07-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_08_ind_x,
        "roms/automated_tests/instr_test-v5/rom_singles/08-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v5_09_ind_y,
        "roms/automated_tests/instr_test-v5/rom_singles/09-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v5_10_branches,
        "roms/automated_tests/instr_test-v5/rom_singles/10-branches.nes"
    );
    setup_rom_test!(
        test_instr_v5_11_stack,
        "roms/automated_tests/instr_test-v5/rom_singles/11-stack.nes"
    );
    setup_rom_test!(
        test_instr_v5_12_jmp_jsr,
        "roms/automated_tests/instr_test-v5/rom_singles/12-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v5_13_rts,
        "roms/automated_tests/instr_test-v5/rom_singles/13-rts.nes"
    );
    setup_rom_test!(
        test_instr_v5_14_rti,
        "roms/automated_tests/instr_test-v5/rom_singles/14-rti.nes"
    );
    setup_rom_test!(
        test_instr_v5_15_brk,
        "roms/automated_tests/instr_test-v5/rom_singles/15-brk.nes"
    );
    setup_rom_test!(
        test_instr_v5_16_special,
        "roms/automated_tests/instr_test-v5/rom_singles/16-special.nes"
    );
    setup_rom_test!(
        test_instr_v5_all_instrs,
        "roms/automated_tests/instr_test-v5/all_instrs.nes",
        60 * 30
    );
    setup_rom_test!(
        test_instr_v5_official_only,
        "roms/automated_tests/instr_test-v5/official_only.nes",
        60 * 30
    );

    // instr_timing
    setup_rom_test!(
        test_instr_timing_01,
        "roms/automated_tests/instr_timing/rom_singles/1-instr_timing.nes",
        60 * 5
    );
    setup_rom_test!(
        test_instr_timing_02,
        "roms/automated_tests/instr_timing/rom_singles/2-branch_timing.nes"
    );
    setup_rom_test!(
        test_instr_timing,
        "roms/automated_tests/instr_timing/instr_timing.nes",
        60 * 20
    );

    // nestest
    // Move nestest from nes.rs here

    // test_instr_v3
    setup_rom_test!(
        test_nes_instr_01_implied,
        "roms/automated_tests/nes_instr_test/rom_singles/01-implied.nes"
    );
    setup_rom_test!(
        test_nes_instr_02_immediate,
        "roms/automated_tests/nes_instr_test/rom_singles/02-immediate.nes"
    );
    setup_rom_test!(
        test_nes_instr_03_zero_page,
        "roms/automated_tests/nes_instr_test/rom_singles/03-zero_page.nes"
    );
    setup_rom_test!(
        test_nes_instr_04_zp_xy,
        "roms/automated_tests/nes_instr_test/rom_singles/04-zp_xy.nes"
    );
    setup_rom_test!(
        test_nes_instr_05_absolute,
        "roms/automated_tests/nes_instr_test/rom_singles/05-absolute.nes"
    );
    setup_rom_test!(
        test_nes_instr_06_abs_xy,
        "roms/automated_tests/nes_instr_test/rom_singles/06-abs_xy.nes"
    );
    setup_rom_test!(
        test_nes_instr_07_ind_x,
        "roms/automated_tests/nes_instr_test/rom_singles/07-ind_x.nes"
    );
    setup_rom_test!(
        test_nes_instr_08_ind_y,
        "roms/automated_tests/nes_instr_test/rom_singles/08-ind_y.nes"
    );
    setup_rom_test!(
        test_nes_instr_09_branches,
        "roms/automated_tests/nes_instr_test/rom_singles/09-branches.nes"
    );
    setup_rom_test!(
        test_nes_instr_10_stack,
        "roms/automated_tests/nes_instr_test/rom_singles/10-stack.nes"
    );
    setup_rom_test!(
        test_nes_instr_11_special,
        "roms/automated_tests/nes_instr_test/rom_singles/11-special.nes"
    );

    /////////////////////////////////////
    // Mapper
    /////////////////////////////////////

    // exram
    #[test]
    fn test_mmc5_exram_crc_sequence() {
        let rom_path = "roms/automated_tests/exram/mmc5exram.nes";
        let rom_data = fs::read(rom_path).expect("mmc5exram ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("mmc5exram ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let expected_crcs = [
            0x90428465, 0x4E2BA407, 0x01ECA2E8, 0x138E5FE2, 0xC7C91CC3, 0xEFBFD0D1, 0xD57CD303,
        ];
        for (index, expected_crc) in expected_crcs.iter().enumerate() {
            run_nes_for_frames(&mut nes, 60);
            let crc = nes.get_screen_buffer().crc32();
            assert_eq!(
                crc,
                *expected_crc,
                "unexpected frame CRC at checkpoint {} for mmc5exram",
                index + 1
            );
        }
    }

    // mmc3_irq_tests
    setup_rom_console_test!(
        test_mmc3_irq_tests_1_clocking,
        "roms/automated_tests/mmc3_irq_tests/1.Clocking.nes",
        60 * 10
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_2_details,
        "roms/automated_tests/mmc3_irq_tests/2.Details.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_3_a12_clocking,
        "roms/automated_tests/mmc3_irq_tests/3.A12_clocking.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_4_scanline_timing,
        "roms/automated_tests/mmc3_irq_tests/4.Scanline_timing.nes",
        60 * 5
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_5_rev_a,
        "roms/automated_tests/mmc3_irq_tests/5.MMC3_rev_A.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_6_rev_b,
        "roms/automated_tests/mmc3_irq_tests/6.MMC3_rev_B.nes"
    );

    // mmc3_test
    setup_rom_test!(
        test_mmc3_test_1_clocking,
        "roms/automated_tests/mmc3_test_2/rom_singles/1-clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_details,
        "roms/automated_tests/mmc3_test_2/rom_singles/2-details.nes"
    );
    setup_rom_test!(
        test_mmc3_test_3_a12_clocking,
        "roms/automated_tests/mmc3_test_2/rom_singles/3-A12_clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_4_scanline_timing,
        "roms/automated_tests/mmc3_test_2/rom_singles/4-scanline_timing.nes",
        60 * 5
    );
    setup_rom_test!(
        test_mmc3_test_5_mmc3,
        "roms/automated_tests/mmc3_test_2/rom_singles/5-MMC3.nes"
    );
    setup_rom_test!(
        test_mmc3_test_6_mmc3_alt,
        "roms/automated_tests/mmc3_test_2/rom_singles/6-MMC3_alt.nes"
    );

    /////////////////////////////////////
    // PPU
    /////////////////////////////////////

    // blargg_ppu_tests_2005.09.15b
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_palette_ram,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/palette_ram.nes"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_power_up_palette,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/power_up_palette.nes"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_sprite_ram,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/sprite_ram.nes"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_vbl_clear_time,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_vram_access,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/vram_access.nes"
    );

    // nmi_sync
    #[test]
    fn test_nmi_sync_demo_ntsc() {
        let rom_path = "roms/automated_tests/nmi_sync/demo_ntsc.nes";
        let rom_data = fs::read(rom_path).expect("demo_ntsc ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("demo_ntsc ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        const WARMUP_FRAMES: u32 = 25;
        run_nes_for_frames(&mut nes, WARMUP_FRAMES);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_a = capture_scanline_rgb(&nes, 121);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_b = capture_scanline_rgb(&nes, 121);

        let white = Nes::lookup_system_palette(0x30);
        let black = Nes::lookup_system_palette(0x0D);

        let a_80 = matches_white_run(&line_frame_a, 80, 103, white, black);
        let a_81 = matches_white_run(&line_frame_a, 81, 103, white, black);
        let b_80 = matches_white_run(&line_frame_b, 80, 103, white, black);
        let b_81 = matches_white_run(&line_frame_b, 81, 103, white, black);

        assert!(
            (a_80 && b_81) || (a_81 && b_80),
            "expected scanline 124 to alternate between white runs at x=80..103 and x=81..103, but got {:?} and {:?}",
            line_frame_a,
            line_frame_b
        );
    }

    // oam_read
    setup_rom_test!(test_oam_read, "roms/automated_tests/oam_read/oam_read.nes");

    // oam_stress
    setup_rom_test!(
        test_oam_stress,
        "roms/automated_tests/oam_stress/oam_stress.nes",
        60 * 10
    );

    // ppu_open_bus
    setup_rom_test!(
        test_ppu_open_bus,
        "roms/automated_tests/ppu_open_bus/ppu_open_bus.nes"
    );

    // ppu_read_buffer
    setup_rom_test!(
        test_ppu_read_buffer,
        "roms/automated_tests/ppu_read_buffer/test_ppu_read_buffer.nes",
        60 * 25 // Takes about 20 seconds according to readme
    );

    // ppu_sprite_hit
    setup_rom_test!(
        test_sprite_hit_01,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_sprite_hit_02,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/02-alignment.nes"
    );
    setup_rom_test!(
        test_sprite_hit_03,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/03-corners.nes"
    );
    setup_rom_test!(
        test_sprite_hit_04,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/04-flip.nes"
    );
    setup_rom_test!(
        test_sprite_hit_05,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/05-left_clip.nes"
    );
    setup_rom_test!(
        test_sprite_hit_06,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/06-right_edge.nes"
    );
    setup_rom_test!(
        test_sprite_hit_07,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/07-screen_bottom.nes"
    );
    setup_rom_test!(
        test_sprite_hit_08,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/08-double_height.nes"
    );
    setup_rom_test!(
        test_sprite_hit_09,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/09-timing.nes"
    );
    setup_rom_test!(
        test_sprite_hit_10,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/10-timing_order.nes"
    );

    // ppu_sprite_overflow
    setup_rom_test!(
        test_sprite_overflow_01,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_02,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/02-details.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_03,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/03-timing.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_04,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/04-obscure.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_05,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/05-emulator.nes"
    );

    // ppu_vbl_nmi
    setup_rom_test!(
        test_ppu_vbl_nmi_01,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/01-vbl_basics.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_02,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_03,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_04,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/04-nmi_control.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_05,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/05-nmi_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_06,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/06-suppression.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_07,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_08,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_09,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_10,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes"
    );

    // TODO scanline ROM suite is not working yet
    // TODO There are glitches in the scrolltest ROM suite that need investigation

    // sprdma_and_dmc_dma
    setup_rom_test!(
        test_sprdma_and_dmc_dma,
        "roms/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes",
        60 * 15
    );
    setup_rom_test!(
        test_sprdma_and_dmc_dma_512,
        "roms/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes",
        60 * 15
    );

    // sprite_hit_tests_2005
    // These are included even though ppu_sprite_hit_tests are included
    // as e.g. 09.timing-basics.nes found an issue that was not found earlier.
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_01_basics,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/01.basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_02_alignment,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/02.alignment.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_03_corners,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/03.corners.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_04_flip,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/04.flip.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_05_left_clip,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/05.left_clip.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_06_right_edge,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/06.right_edge.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_07_screen_bottom,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/07.screen_bottom.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_08_double_height,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/08.double_height.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_09_timing_basics,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/09.timing_basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_10_timing_order,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/10.timing_order.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_11_edge_timing,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/11.edge_timing.nes"
    );

    // sprite_overflow_tests
    setup_rom_console_test!(
        test_sprite_overflow_tests_1_basics,
        "roms/automated_tests/sprite_overflow_tests/1.Basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_2_details,
        "roms/automated_tests/sprite_overflow_tests/2.Details.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_3_timing,
        "roms/automated_tests/sprite_overflow_tests/3.Timing.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_4_obscure,
        "roms/automated_tests/sprite_overflow_tests/4.Obscure.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_5_emulator,
        "roms/automated_tests/sprite_overflow_tests/5.Emulator.nes"
    );

    // TODO tvpassfail/tv ROM suite not automated yet
    // We will need to capture the screen post filtering (need NTSC filtering turned on)
    // For the first test, we should be able to find nearest color of each 8x8 tile and
    // parse PASS
    // For the second test, we should be able to count the height and width of the left
    // rectangle (filtering off) and match

    // vbl_nmi_timing
    setup_rom_console_test!(
        test_vbl_nmi_timing_frame_basics,
        "roms/automated_tests/vbl_nmi_timing/1.frame_basics.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_vbl_timing,
        "roms/automated_tests/vbl_nmi_timing/2.vbl_timing.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_even_odd_frames,
        "roms/automated_tests/vbl_nmi_timing/3.even_odd_frames.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_vbl_clear_timing,
        "roms/automated_tests/vbl_nmi_timing/4.vbl_clear_timing.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_suppression,
        "roms/automated_tests/vbl_nmi_timing/5.nmi_suppression.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_disable,
        "roms/automated_tests/vbl_nmi_timing/6.nmi_disable.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_timing,
        "roms/automated_tests/vbl_nmi_timing/7.nmi_timing.nes"
    );

    /////////////////////////////////////
    // Miscellaneous
    /////////////////////////////////////

    // TODO integrate PaddleTest3 ROM suite
    // TODO integrate read_joy3 ROM suite
    // TODO integrate full_palette ROM suite
    // TODO integrate full_nes_palette ROM suite
}
