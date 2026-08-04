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
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::{Config, HardwareModel, Nes, RamInitMode};
    use crate::nes::input::Button;
    use crate::platform::config::FrontendConfig;
    use crate::platform::debugging::{Tracing, init_tracing};
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

    /// Test verification method
    #[derive(Debug, PartialEq, Eq)]
    pub(crate) enum RomTestVerification {
        /// Verify using status byte at 0x6000
        StatusByte,
        /// Verify using console output
        Console { pass_string: String },
        /// Verify by matching a CRC-32 printed to the console output.
        ConsoleCrc(&'static [u32]),
    }

    fn test_default_config() -> Config {
        Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Runner for OAM test ROMs
    pub(crate) struct RomTestRunner {
        rom_path: String,
        max_frames: u32,
        wait_reset: u32,
        verification: RomTestVerification,
        tv_system_override: Option<crate::nes::console::TimingMode>,
        ram_init_mode_override: Option<crate::nes::console::RamInitMode>,
    }

    impl RomTestRunner {
        fn read_console_text(nes: &mut Nes) -> String {
            let base_addr = nes.base_nametable_addr();
            let text = nes.read_nametable_text(base_addr, 32 * 32);
            text.as_bytes()
                .chunks(32)
                .map(|chunk| String::from_utf8_lossy(chunk).trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }

        /// Create a new test runner for $6000-based tests
        pub fn new(rom_path: &str, max_frames: u32, verification: RomTestVerification) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                wait_reset: 1,
                verification,
                tv_system_override: None,
                ram_init_mode_override: None,
            }
        }

        /// Create a new test runner with explicit TV system override
        pub fn new_with_tv_system(
            rom_path: &str,
            max_frames: u32,
            verification: RomTestVerification,
            tv_system: crate::nes::console::TimingMode,
        ) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                wait_reset: 1,
                verification,
                tv_system_override: Some(tv_system),
                ram_init_mode_override: None,
            }
        }

        /// Create a new test runner with explicit RAM init mode override
        pub fn new_with_ram_init_mode(
            rom_path: &str,
            max_frames: u32,
            verification: RomTestVerification,
            ram_init_mode: crate::nes::console::RamInitMode,
        ) -> Self {
            Self {
                rom_path: rom_path.to_string(),
                max_frames,
                wait_reset: 1,
                verification,
                tv_system_override: None,
                ram_init_mode_override: Some(ram_init_mode),
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

            let cartridge = match Cartridge::load_from_file(&rom_data, &self.rom_path, None) {
                Ok(cart) => cart,
                Err(e) => {
                    eprintln!("Failed to parse ROM {}: {}", self.rom_path, e);
                    return RomTestResult::Fail(0x81_u8);
                }
            };

            // Create NES with configuration based on cartridge's TV system
            let mut config = test_default_config();

            // Use override if provided, otherwise auto-detect from ROM header
            if let Some(timing_mode_override) = self.tv_system_override {
                config.nes.hardware_model = HardwareModel::from_timing_mode(timing_mode_override);
            } else {
                config.nes.hardware_model =
                    HardwareModel::from_timing_mode(cartridge.rom_timing_mode());
            }

            // Use RAM init mode override if provided
            if let Some(ram_init_mode) = self.ram_init_mode_override {
                config.frontend.ram_init_mode = ram_init_mode;
            }

            let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
                config,
            ));
            nes.insert_cartridge(cartridge);
            // Initial reset is treated as power-on.
            nes.reset(false);

            // CPU cycles per frame depends on TV system
            let cpu_cycles_per_frame = match nes.app_context().borrow().config().nes.hardware_model
            {
                HardwareModel::NesNtsc => 29_780u32,
                HardwareModel::NesPal => 33_247u32,
                HardwareModel::Dendy => 35_464u32, // 312 scanlines * 341 dots / 3 PPU:CPU
            };

            let mut running = false;
            let mut first_nonzero_status = None;
            let mut last_prompt: Option<String> = None;
            let mut pressed_for_prompt = false;
            let mut pending_release: Option<Button> = None;
            let mut release_after_frames: u8 = 0;
            // Run frames and check for results
            for frame in 1..=self.max_frames {
                // Run one frame (roughly 29780 CPU cycles for NTSC, 33247 for PAL)
                let mut current_status = nes.bus().borrow_mut().read_for_testing(0x6000);
                if current_status == 0x80 {
                    running = true;
                }
                if current_status != 0 && first_nonzero_status.is_none() {
                    first_nonzero_status = Some((frame, current_status));
                }
                const STATUS_POLL_INTERVAL: u32 = 256;
                for cpu_cycle in 0..cpu_cycles_per_frame {
                    nes.run_cpu_tick();

                    if cpu_cycle != 0 && cpu_cycle % STATUS_POLL_INTERVAL == 0 {
                        current_status = nes.bus().borrow_mut().read_for_testing(0x6000);
                        if current_status == 0x80 {
                            running = true;
                        }
                    }
                }
                // Make sure we observe any status update at end-of-frame.
                let status = nes.bus().borrow_mut().read_for_testing(0x6000);
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
                        return RomTestResult::Pass;
                    } else if status > 0x00 && status < 0x80 {
                        let text = Self::read_console_text(&mut nes);
                        let uppercase_text = text.to_uppercase();

                        // Some mapper-verification ROMs temporarily reuse $6000-$600F for mapper
                        // registers during the test. In those cases, only treat a non-zero status
                        // byte as terminal if the console has also reached a failure state.
                        if uppercase_text.ends_with("PASSED") {
                            return RomTestResult::Pass;
                        }
                        if uppercase_text.contains("FAILED")
                            || uppercase_text.contains("ERROR")
                            || (text.starts_with("0x") && text.chars().nth(2) != Some('0'))
                        {
                            println!("Test failed with status code: 0x{:02X}", status);
                            println!("Console output:\n{}", text);
                            return RomTestResult::Fail(status);
                        }

                        continue;
                    } else if status == 0x81 {
                        if self.wait_reset > 0 {
                            self.wait_reset -= 1;
                        } else {
                            // Test requests a reset-button style reset.
                            nes.reset(true);
                            nes.bus().borrow_mut().write_for_testing(0x6000, 0x80);
                            self.wait_reset = 1;
                        }
                    } else if status == 0x80 {
                        // Still running
                        continue;
                    }
                } else if let RomTestVerification::Console { pass_string } = &self.verification {
                    let text = Self::read_console_text(&mut nes);
                    let uppercase_text = text.to_uppercase();

                    // Check for FAIL first — a FAIL anywhere trumps a PASS at the end.
                    // Use "FAIL" (not "FAILED") so ROMs that print "FAIL" without "ED"
                    // (e.g. Mapper 31 test ROMs) are also caught.
                    // NOTE: "ERROR" is checked AFTER the pass condition because some
                    // pass strings legitimately contain "ERROR" (e.g. "ERRORS: 0/1000").
                    if uppercase_text.contains("FAIL") {
                        println!("Test failed!");
                        println!("Console output:\n{}", text);
                        return RomTestResult::Fail(1);
                    } else if uppercase_text.ends_with(pass_string) {
                        return RomTestResult::Pass;
                    } else if uppercase_text.contains("ERROR")
                        || (text.starts_with("0x") && text.chars().nth(2) != Some('0'))
                    {
                        println!("Test failed!");
                        println!("Console output:\n{}", text);
                        return RomTestResult::Fail(1);
                    } else if let Some(last_line) = text.lines().last().map(|line| line.trim()) {
                        let prompt_button = match last_line {
                            "A" => Some(Button::A),
                            "B" => Some(Button::B),
                            "Select" => Some(Button::Select),
                            "Start" => Some(Button::Start),
                            "Up" => Some(Button::Up),
                            "Down" => Some(Button::Down),
                            "Left" => Some(Button::Left),
                            "Right" => Some(Button::Right),
                            _ => None,
                        };

                        let prompt_changed = last_prompt.as_deref() != Some(last_line);
                        if prompt_changed {
                            last_prompt = Some(last_line.to_string());
                            pressed_for_prompt = false;
                        }

                        if let Some(button) = prompt_button
                            && !pressed_for_prompt
                            && pending_release.is_none()
                        {
                            nes.set_button(1, button, true);
                            pending_release = Some(button);
                            release_after_frames = 2;
                            pressed_for_prompt = true;
                        }
                    }
                } else if let RomTestVerification::ConsoleCrc(expected_crcs) = self.verification {
                    let text = Self::read_console_text(&mut nes);

                    if let Some(crc) = parse_crc32_from_console_text(&text) {
                        if expected_crcs.contains(&crc) {
                            return RomTestResult::Pass;
                        }
                        println!("Test failed! Unexpected CRC 0x{:08X}", crc);
                        println!("Console output:\n{}", text);
                        return RomTestResult::Fail(1);
                    }
                }

                if let Some(button) = pending_release
                    && release_after_frames > 0
                {
                    release_after_frames -= 1;
                    if release_after_frames == 0 {
                        nes.set_button(1, button, false);
                        pending_release = None;
                    }
                }
            }

            // No result found within timeout
            let text = Self::read_console_text(&mut nes);
            println!("Test Timed out with output:\n{}", text);
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

        let cartridge = match Cartridge::load_from_file(&rom_data, rom_path, None) {
            Ok(cart) => cart,
            Err(e) => {
                eprintln!("Failed to parse ROM {}: {}", rom_path, e);
                return RomTestResult::Fail(0x81_u8);
            }
        };

        // Create NES with configuration based on cartridge's TV system
        let mut config = test_default_config();
        config.nes.hardware_model = HardwareModel::from_timing_mode(cartridge.rom_timing_mode());

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // CPU cycles per frame depends on TV system
        let cpu_cycles_per_frame = match nes.app_context().borrow().config().nes.hardware_model {
            HardwareModel::NesNtsc => 29_780u32,
            HardwareModel::NesPal => 33_247u32,
            HardwareModel::Dendy => 35_464u32, // 312 scanlines * 341 dots / 3 PPU:CPU
        };

        for _frame in 1..=max_frames {
            for _cpu_cycle in 0..cpu_cycles_per_frame {
                if nes.cpu_ref().pc() == stop_address {
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

    pub(crate) fn write_checkpoint_png(
        path: &std::path::Path,
        rgb: &[u8],
        width: u32,
        height: u32,
    ) {
        crate::platform::png_utils::write_rgb_png(path, rgb, width, height)
            .expect("capture PNG should be written");
    }

    #[macro_export]
    macro_rules! setup_rom_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::nes::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestVerification::StatusByte,
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
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
        ($test_name:ident, $rom_path:expr) => {
            setup_rom_console_test!($test_name, $rom_path, "PASSED");
        };
        ($test_name:ident, $rom_path:expr, $pass_string:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::nes::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    60 * 30,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestVerification::Console {
                        pass_string: $pass_string.to_string(),
                    },
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
        ($test_name:ident, $rom_path:expr, $pass_string:expr, $tv_system:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::nes::integration_tests::rom_test_runner::tests::RomTestRunner::new_with_tv_system(
                    $rom_path,
                    60 * 30,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestVerification::Console {
                        pass_string: $pass_string.to_string(),
                    },
                    $tv_system,
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
    }

    #[macro_export]
    macro_rules! setup_rom_console_test_with_ram_init {
        ($test_name:ident, $rom_path:expr, $pass_string:expr, $ram_init_mode:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::nes::integration_tests::rom_test_runner::tests::RomTestRunner::new_with_ram_init_mode(
                    $rom_path,
                    60 * 30,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestVerification::Console {
                        pass_string: $pass_string.to_string(),
                    },
                    $ram_init_mode,
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
                    "{} should pass",
                    rom_name
                );
            }
        };
    }

    #[macro_export]
    macro_rules! setup_rom_console_crc_test {
        ($test_name:ident, $rom_path:expr, $timeout:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let mut runner = $crate::nes::integration_tests::rom_test_runner::tests::RomTestRunner::new(
                    $rom_path,
                    $timeout,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestVerification::ConsoleCrc(
                        $expected,
                    ),
                );
                let result = runner.run_test();
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
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
    macro_rules! setup_rom_crc_test {
        ($test_name:ident, $rom_path:expr, $checkpoints:expr) => {
            #[test]
            fn $test_name() {
                let rom_data = std::fs::read($rom_path).expect("ROM should load");
                let cartridge = $crate::nes::cartridge::Cartridge::load_from_file(
                    &rom_data,
                    $rom_path,
                    None,
                )
                .expect("ROM should parse");

                let mut config = $crate::nes::console::Config {
                    frontend: $crate::platform::config::FrontendConfig {
                        ram_init_mode: $crate::nes::console::RamInitMode::Zero,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                config.nes.hardware_model =
                    $crate::nes::console::HardwareModel::from_timing_mode(cartridge.rom_timing_mode());

                let mut nes = $crate::nes::console::Nes::new(
                    $crate::platform::app_context::AppContext::new_with_config(config),
                );
                nes.insert_cartridge(cartridge);
                nes.reset(false);

                let checkpoints = $checkpoints;
                let capture_screen = std::env::var_os("NESER_CAPTURE_SCREEN").is_some();
                let capture_dir =
                    std::path::PathBuf::from("target/crc_checkpoints").join(stringify!($test_name));

                let mut previous_frame = 0u32;
                let mut actual: Vec<(u32, u32)> = Vec::with_capacity(checkpoints.len());

                for (frame, _expected_crc) in checkpoints.iter().copied() {
                    assert!(
                        frame >= previous_frame,
                        "checkpoint frames must be in non-decreasing order"
                    );

                    let delta = frame - previous_frame;
                    if delta > 0 {
                        $crate::nes::integration_tests::rom_test_runner::tests::run_nes_for_frames(
                            &mut nes, delta,
                        );
                    }

                    let screen = nes.get_screen_buffer();
                    let crc = screen.crc32();
                    actual.push((frame, crc));

                    if capture_screen {
                        let rgb = screen.snapshot();
                        let file_name = format!("f{:05}_crc_{:08X}.png", frame, crc);
                        let path = capture_dir.join(file_name);
                        $crate::nes::integration_tests::rom_test_runner::tests::write_checkpoint_png(
                            &path, &rgb, 256, 240,
                        );
                    }

                    previous_frame = frame;
                }

                let expected: Vec<(u32, u32)> = checkpoints.iter().copied().collect();
                assert_eq!(
                    actual, expected,
                    "CRC checkpoints mismatch for {}",
                    $rom_path
                );

                if capture_screen {
                    println!(
                        "[crc-checkpoint] generated checkpoint artifacts in {}",
                        capture_dir.display()
                    );
                }
            }
        };
    }

    /// CRC-based ROM test macro with scripted button input.
    ///
    /// `$inputs` is a slice of `(frame, Button, pressed)` tuples specifying when to
    /// press/release buttons. `$checkpoints` is the usual `[(frame, expected_crc)]` list.
    /// Both use the same frame timeline. Input events at frame N are applied before the
    /// emulator steps frame N, and CRC checkpoints at frame N are captured after stepping.
    /// Both must be in non-decreasing frame order.
    ///
    /// # Example
    /// ```ignore
    /// setup_rom_crc_test_with_input!(
    ///     test_my_rom,
    ///     "roms/nes/automated_tests/my_rom.nes",
    ///     [
    ///         (10, Button::Start, true),
    ///         (12, Button::Start, false),
    ///     ],
    ///     [(600, 0xDEADBEEF)]
    /// );
    /// ```
    #[macro_export]
    macro_rules! setup_rom_crc_test_with_input {
        ($test_name:ident, $rom_path:expr, $inputs:expr, $checkpoints:expr) => {
            #[test]
            fn $test_name() {
                let rom_data = std::fs::read($rom_path).expect("ROM should load");
                let cartridge = $crate::nes::cartridge::Cartridge::load_from_file(
                    &rom_data,
                    $rom_path,
                    None,
                )
                .expect("ROM should parse");

                let mut config = $crate::nes::console::Config {
                    frontend: $crate::platform::config::FrontendConfig {
                        ram_init_mode: $crate::nes::console::RamInitMode::Zero,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                config.nes.hardware_model =
                    $crate::nes::console::HardwareModel::from_timing_mode(cartridge.rom_timing_mode());

                let mut nes = $crate::nes::console::Nes::new(
                    $crate::platform::app_context::AppContext::new_with_config(config),
                );
                nes.insert_cartridge(cartridge);
                nes.reset(false);

                let inputs: &[(u32, $crate::nes::input::Button, bool)] = &$inputs;
                let checkpoints: &[(u32, u32)] = &$checkpoints;

                // Validate non-decreasing frame order for both inputs and checkpoints
                for w in inputs.windows(2) {
                    assert!(
                        w[0].0 <= w[1].0,
                        "input frames must be in non-decreasing order"
                    );
                }
                for w in checkpoints.windows(2) {
                    assert!(
                        w[0].0 <= w[1].0,
                        "checkpoint frames must be in non-decreasing order"
                    );
                }

                let capture_screen = std::env::var_os("NESER_CAPTURE_SCREEN").is_some();
                let capture_dir =
                    std::path::PathBuf::from("target/crc_checkpoints").join(stringify!($test_name));

                // Collect all event frames (inputs + checkpoints) in order
                let max_frame = inputs
                    .iter()
                    .map(|(f, _, _)| *f)
                    .chain(checkpoints.iter().map(|(f, _)| *f))
                    .max()
                    .unwrap_or(0);

                let mut input_idx = 0;
                let mut checkpoint_idx = 0;
                let mut actual: Vec<(u32, u32)> = Vec::with_capacity(checkpoints.len());

                for frame in 0..=max_frame {
                    // Apply any input events scheduled for this frame (before stepping)
                    while input_idx < inputs.len() && inputs[input_idx].0 == frame {
                        let (_, button, pressed) = inputs[input_idx];
                        nes.set_button(1, button, pressed);
                        input_idx += 1;
                    }

                    // Advance emulator by one frame
                    $crate::nes::integration_tests::rom_test_runner::tests::run_nes_for_frames(
                        &mut nes, 1,
                    );

                    // Check any CRC checkpoints scheduled for this frame
                    while checkpoint_idx < checkpoints.len()
                        && checkpoints[checkpoint_idx].0 == frame
                    {
                        let screen = nes.get_screen_buffer();
                        let crc = screen.crc32();
                        actual.push((frame, crc));

                        if capture_screen {
                            let rgb = screen.snapshot();
                            let file_name = format!("f{:05}_crc_{:08X}.png", frame, crc);
                            let path = capture_dir.join(file_name);
                            $crate::nes::integration_tests::rom_test_runner::tests::write_checkpoint_png(
                                &path, &rgb, 256, 240,
                            );
                        }
                        checkpoint_idx += 1;
                    }
                }

                let expected: Vec<(u32, u32)> = checkpoints.iter().copied().collect();
                assert_eq!(
                    actual, expected,
                    "CRC checkpoints mismatch for {}",
                    $rom_path
                );

                if capture_screen {
                    println!(
                        "[crc-checkpoint] generated checkpoint artifacts in {}",
                        capture_dir.display()
                    );
                }
            }
        };
    }

    #[macro_export]
    macro_rules! setup_rom_address_test {
        ($test_name:ident, $rom_path:expr, $stop_address:expr, $verify_fn:expr, $timeout:expr) => {
            #[test]
            fn $test_name() {
                let result =
                    $crate::nes::integration_tests::rom_test_runner::tests::run_address_test(
                        $rom_path,
                        $timeout,
                        $stop_address,
                        $verify_fn,
                    );
                let rom_name = $rom_path.split('/').last().unwrap();
                assert_eq!(
                    result,
                    $crate::nes::integration_tests::rom_test_runner::tests::RomTestResult::Pass,
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
