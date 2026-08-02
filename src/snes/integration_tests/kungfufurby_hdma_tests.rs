//! KungFuFurby's HDMA test ROM collection (issue #2884), from the same byuu
//! "SNES TEST IMAGE" suite as the NMI/IRQ ROMs (no formal license, recorded as
//! `unknown`; see `roms/snes/automated_tests/manifest.json`). These three HDMA
//! ROMs were earmarked out of scope for #2883/#3049 and are automated here.
//!
//! Like the NMI/IRQ suites, each ROM renders a solid backdrop colour on
//! completion (blue = PASS, red/maroon = FAIL), cross-checked against a Mesen2
//! headless capture (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`) of the identical ROM file.
//!
//! **Golden convention (#3092).** All three tests assert the Mesen2-correct
//! blue PASS screen, not NESER's current output, so they FAIL under
//! `cargo test --include-ignored` until #3062 lands -- the designed state, not
//! a regression. See `kungfufurby_irq_tests`' module doc for the rationale.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs";

#[cfg(test)]
mod tests {
    use super::*;

    fn run_rom_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "kungfufurby_hdma_tests",
            // Headroom for `test_hdmasync`'s frame-1100 sample point, which
            // needs ~393M master clocks (1100 x 1364 x 262). The previous
            // 400M cap cleared that by only ~19 frames; overshooting it would
            // exit on TickLimit and report a confusing budget failure instead
            // of the golden mismatch the test is actually about.
            RunConfig::new(600_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    // All three goldens below are the Mesen2-approved blue PASS screen,
    // verified by a fresh headless capture at each test's own sample frame.
    // Un-ignore each one once NESER renders the PASS backdrop (#3062).

    /// #3062: NESER paints a brief red flash at frames 12-13 and then settles
    /// on solid black from frame 14 onward; Mesen2 renders blue PASS. Same
    /// black CRC as #2953.
    #[test]
    #[ignore = "settles on black instead of Mesen2's blue PASS; asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdma_passes() {
        run_rom_screen_crc("test_hdma.smc", 600, 0x8695_BBB0);
    }

    /// #3062: this ROM is slow -- BOTH emulators render solid black until
    /// ~frame 1027, so the frame-600 sample this test used before #3092 could
    /// never see the divergence (it asserted "still black", which Mesen2 also
    /// produces). Sampled at frame 1100 instead, comfortably past the
    /// transition: Mesen2 is stable blue PASS from frame 1029, while NESER
    /// paints red FAIL at 1029 and settles on `0x7F21_BBD7` from frame 1031.
    #[test]
    #[ignore = "settles on a non-PASS screen where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdmasync.smc", 1100, 0x8695_BBB0);
    }

    /// #3062: NESER runs to completion but the ROM's self-check FAILS (a flat
    /// `(66, 0, 0)` fill, settled from frame 30) where Mesen2 PASSes (blue) --
    /// an HDMA timing behaviour NESER gets wrong.
    #[test]
    #[ignore = "self-check FAILs (maroon) where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdmatiming.smc", 600, 0x8695_BBB0);
    }
}
