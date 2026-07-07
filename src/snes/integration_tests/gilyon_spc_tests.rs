use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const GILYON_TESTS_SPC_ROOT: &str = "roms/snes/automated_tests/gilyon_tests/spctest";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spctest_passes_all_1368_tests() {
        // Reaches "Success" at test 0557 (hex), the last of 1368 SPC-700 tests.
        let path = Path::new(GILYON_TESTS_SPC_ROOT).join("spctest.sfc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            "spctest.sfc",
            RunConfig::new(4_000_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 2000,
                expected_crc: 0x87CD_986B,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "spctest.sfc: expected screen-CRC PASS at frame 2000, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }
}
