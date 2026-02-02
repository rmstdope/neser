#[cfg(test)]
pub(crate) mod tests {
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
    pub(crate) enum RomTestResult {
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
    pub(crate) enum RomTestVerification {
        /// Verify using status byte at 0x6000
        StatusByte,
        /// Verify using console output
        Console,
        /// Verify by matching a CRC-32 printed to the console output.
        ConsoleCrc(&'static [u32]),
    }

    /// Runner for OAM test ROMs
    pub(crate) struct RomTestRunner {
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
            init_tracing_from_env();
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

    pub(crate) fn run_address_test(
        rom_path: &str,
        max_frames: u32,
        stop_address: u16,
        verifier: fn(&mut Nes) -> bool,
    ) -> RomTestResult {
        init_tracing_from_env();

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

    pub fn run_nes_for_frames(nes: &mut Nes, frames: u32) {
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

    pub(crate) fn init_tracing_from_env() {
        let apu_level = match std::env::var("NESER_TRACE_APU") {
            Ok(value) => value.parse::<u8>().unwrap_or(1),
            Err(_) => return,
        };
        let cpu_level = match std::env::var("NESER_TRACE_CPU") {
            Ok(value) => value.parse::<u8>().unwrap_or(1),
            Err(_) => return,
        };

        if apu_level != 0 || cpu_level != 0 {
            init_tracing(Tracing {
                enabled: true,
                apu: apu_level,
                cpu: cpu_level,
                ..Tracing::default()
            });
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

    #[macro_export]
    macro_rules! setup_rom_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    $crate::integration_tests::rom_test_runner::tests::RomTestVerification::StatusByte,
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            setup_rom_test!($test_name, $rom_path, 60 * 30); // Wait for at least 30 s
        };
    }

    #[macro_export]
    macro_rules! setup_rom_console_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    $crate::integration_tests::rom_test_runner::tests::RomTestVerification::Console,
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
        ($test_name:ident, $rom_path:expr) => {
            setup_rom_console_test!($test_name, $rom_path, 60 * 30); // Wait for at least 30 s
        };
    }

    #[macro_export]
    macro_rules! setup_rom_console_crc_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    $crate::integration_tests::rom_test_runner::tests::RomTestVerification::ConsoleCrc(
                        $expected,
                    ),
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
        ($test_name:ident, $rom_path:expr, $expected:expr) => {
            setup_rom_console_crc_test!($test_name, $rom_path, 60 * 30, $expected); // Wait for at least 30 s
        };
    }

    #[macro_export]
    macro_rules! setup_rom_address_test {
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let result = $crate::integration_tests::rom_test_runner::tests::run_address_test(
                    $rom_path,
                    $timeout,
                    $stop_address,
                    $verify_fn,
                );
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr) => {
            setup_rom_address_test!($test_name, $rom_path, $stop_address, $verify_fn, 60 * 30); // Wait for at least 30 s
        };
    }
}
