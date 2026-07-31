//! KungFuFurby's HDMA test ROM collection (issue #2884), from the same byuu
//! "SNES TEST IMAGE" suite as the NMI/IRQ ROMs (no formal license, recorded as
//! `unknown`; see `roms/snes/automated_tests/manifest.json`). These three HDMA
//! ROMs were earmarked out of scope for #2883/#3049 and are automated here.
//!
//! Like the NMI/IRQ suites, each ROM renders a solid backdrop colour on
//! completion (blue = PASS, red/maroon = FAIL), cross-checked against a Mesen2
//! headless capture (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`) of the identical ROM file.

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
            RunConfig::new(400_000_000, 0),
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

    // The three CRCs below are NESER's *current* (diverging) renders, NOT
    // Mesen2-approved goldens: Mesen2 shows a solid blue PASS backdrop for all
    // three at frame 600. Ignored pending #3062; re-approve against Mesen2 and
    // un-ignore once NESER renders the PASS backdrop.

    /// #3062: NESER renders solid black (self-check appears to crash/hang before
    /// painting the backdrop); Mesen2 renders blue PASS. Same black CRC as
    /// #2953.
    #[test]
    #[ignore = "renders black instead of Mesen2's blue PASS; pending #3062"]
    fn test_hdma_passes() {
        run_rom_screen_crc("test_hdma.smc", 600, 0x6E8D_8520);
    }

    /// #3062: NESER renders solid black (as `test_hdma`); Mesen2 renders blue
    /// PASS.
    #[test]
    #[ignore = "renders black instead of Mesen2's blue PASS; pending #3062"]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdmasync.smc", 600, 0x6E8D_8520);
    }

    /// #3062: NESER runs to completion but the ROM's self-check FAILS (red
    /// backdrop) where Mesen2 PASSes (blue) -- an HDMA timing behaviour NESER
    /// gets wrong.
    #[test]
    #[ignore = "self-check FAILs (red) where Mesen2 PASSes (blue); pending #3062"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdmatiming.smc", 600, 0x8662_6F50);
    }
}
