use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const GILYON_TESTS_CPU_ROOT: &str = "roms/snes/automated_tests/gilyon_tests/cputest";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a vendored gilyon/snes-tests 65816 CPU test ROM and asserts that
    /// the screen rendered at `frames` matches the visually-approved golden
    /// CRC32. Both ROMs finish by looping forever on their final screen
    /// (either "Success" or a frozen "Failed" diagnostic screen), so any
    /// frame comfortably past completion works as the sampling point.
    fn run_cputest_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(GILYON_TESTS_CPU_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            RunConfig::new(4_000_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: expected screen-CRC PASS at frame {frames}, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    #[test]
    fn cputest_basic_passes_all_1107_tests() {
        // Reaches "Success" at test 0452 (hex), the last of 1107 basic tests.
        run_cputest_screen_crc("cputest-basic.sfc", 2000, 0xB4FA_650E);
    }

    #[test]
    fn cputest_full_passes_all_1610_tests() {
        // Reaches "Success" at test 0649 (hex), the last of 1610 full tests.
        run_cputest_screen_crc("cputest-full.sfc", 2000, 0xB7EB_715E);
    }
}
