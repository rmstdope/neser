//! Jonas Quinn's DMA/HDMA test ROM collection (issue #2884), vendored under
//! `roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/` (no LICENSE,
//! recorded as `unknown`; the folder is one manifest asset `snes-tests-jonasquinn`).
//!
//! Canonical DMA/HDMA ROMs from the collection (duplicates that recur under
//! `test_hdma/`, `test_mdrhdma/`, `blobs/` and `snestest_082506/` are treated as
//! mirror-provenance and not re-automated). Screen-CRC oracle mostly at frame
//! 600, cross-checked against a Mesen2 headless capture
//! (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`) of the identical ROM file.
//!
//! Two ROMs render a pixel-exact (0-pixel diff) match for Mesen2 and are
//! committed goldens. `test_hdma/test_hdmasync.smc` and
//! `test_hdma/test_hdmatiming.smc` are byte-identical byuu-suite mirrors of the
//! KungFuFurby ROMs (md5 `acec8b53...` for the former) and share their
//! divergence (#3062).
//!
//! **Golden convention (#3092).** The four `#[ignore]`d *self-check* ROMs
//! assert the Mesen2-correct blue PASS screen, not NESER's current output, so
//! they FAIL under `cargo test --include-ignored` until #3062/#3063 land --
//! the designed state, not a regression. See `kungfufurby_irq_tests`' module
//! doc for the rationale. `test_dmatiming_matches_mesen2` is deliberately NOT
//! on this convention: it is a pixel-diff comparison against Mesen2's own
//! render, not a PASS/FAIL self-check, so a blue backdrop is not its correct
//! result and it keeps recording NESER's current diverging CRC.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a jonasquinn ROM to `frames` and asserts the screen matches
    /// `expected_crc`. To approve a golden: run with NESER_CAPTURE_SCREEN=1,
    /// pixel-diff the capture against a Mesen2 capture at the same frame, then
    /// record the CRC here.
    fn run_rom_screen_crc(subpath: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(ROOT).join(subpath);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let name = subpath.replace('/', "_");
        let result = run_rom_with_oracle(
            &rom,
            &name,
            "jonasquinn_dma_tests",
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
            "{subpath}: got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    /// PASS: NESER's frame-600 capture is a 0-pixel-diff match for Mesen2 (both
    /// render the ROM's blue PASS backdrop). MDR-during-HDMA behaviour.
    #[test]
    fn test_mdrhdma_matches_mesen2() {
        run_rom_screen_crc("test_mdrhdma2/test_mdrhdma.sfc", 600, 0x8695_BBB0);
    }

    /// PASS: NESER's frame-600 capture is a 0-pixel-diff match for Mesen2 (the
    /// mid-frame HDMA visual). Ships `image00x.bmp` reference frames.
    #[test]
    fn hdma_midframe_matches_mesen2() {
        run_rom_screen_crc("hdma_midframe/demo.smc", 600, 0xE90C_27F0);
    }

    // The self-check goldens below are the Mesen2-approved blue PASS screen,
    // verified by a fresh headless capture at each test's own sample frame.
    // Un-ignore each one once NESER renders the PASS backdrop.

    /// #3063: NESER's self-check FAILs (flat maroon) where Mesen2 PASSes (blue).
    #[test]
    #[ignore = "self-check FAILs (maroon) where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3063"]
    fn test_dmavalid_passes() {
        run_rom_screen_crc("test_dmavalid_v01/test_dmavalid.smc", 600, 0x8695_BBB0);
    }

    /// #3063: NESER's self-check FAILs (flat maroon) where Mesen2 PASSes (blue).
    /// The CPU clears $420C mid-frame; NESER gets the mid-frame HDMA-disable
    /// behaviour wrong.
    #[test]
    #[ignore = "self-check FAILs (maroon) where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3063"]
    fn test_hdmadisable_passes() {
        run_rom_screen_crc("test_hdmadisable/test_hdmadisable.smc", 600, 0x8695_BBB0);
    }

    /// #3063: NESER diverges from Mesen2 by ~0.93% (532 px) at frame 600 -- a
    /// small DMA-timing visual difference.
    ///
    /// Deliberately excluded from #3092's aspirational-golden convention (this
    /// is not an oversight): the ROM renders a picture rather than a PASS/FAIL
    /// backdrop, so there is no blue screen to assert. The CRC below stays
    /// NESER's own current render; the correct value would be Mesen2's exact
    /// framebuffer hash, which is a different kind of golden.
    #[test]
    #[ignore = "~0.93% pixel divergence from Mesen2; records NESER's current render, not a PASS golden; pending #3063"]
    fn test_dmatiming_matches_mesen2() {
        run_rom_screen_crc("test_dmatiming/demo.smc", 600, 0x97B7_5364);
    }

    /// #3062 (byte-identical byuu mirror of KungFuFurby test_hdmasync): sampled
    /// at frame 1100 rather than 600 because BOTH emulators render solid black
    /// until ~frame 1027 -- see `kungfufurby_hdma_tests::test_hdmasync_passes`
    /// for the full transition timeline.
    #[test]
    #[ignore = "settles on a non-PASS screen where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdma/test_hdmasync.smc", 1100, 0x8695_BBB0);
    }

    /// #3062 (byte-identical byuu mirror of KungFuFurby test_hdmatiming): NESER
    /// self-check FAILs (flat `(66, 0, 0)`) where Mesen2 PASSes (blue).
    #[test]
    #[ignore = "self-check FAILs (maroon) where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdma/test_hdmatiming.smc", 600, 0x8695_BBB0);
    }
}
