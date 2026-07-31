//! Jonas Quinn's DMA/HDMA test ROM collection (issue #2884), vendored under
//! `roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/` (no LICENSE,
//! recorded as `unknown`; the folder is one manifest asset `snes-tests-jonasquinn`).
//!
//! Canonical DMA/HDMA ROMs from the collection (duplicates that recur under
//! `test_hdma/`, `test_mdrhdma/`, `blobs/` and `snestest_082506/` are treated as
//! mirror-provenance and not re-automated). Screen-CRC oracle at frame 600,
//! cross-checked against a Mesen2 headless capture
//! (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`) of the identical ROM file.
//!
//! Two ROMs render a pixel-exact (0-pixel diff) match for Mesen2 and are
//! committed goldens; the rest are `#[ignore]`d recording NESER's current
//! diverging CRC, pending fixes. `test_hdma/test_hdmasync.smc` and
//! `test_hdma/test_hdmatiming.smc` are byte-identical byuu-suite mirrors of the
//! KungFuFurby ROMs and share their divergence (#3062).

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
            RunConfig::new(400_000_000, 0),
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

    // Divergences below record NESER's current CRC, NOT a Mesen2 golden; Mesen2
    // renders the PASS state where NESER does not. Un-ignore + re-approve once
    // fixed.

    /// #3063: NESER's self-check FAILs (red) where Mesen2 PASSes (blue).
    #[test]
    #[ignore = "self-check FAILs (red) where Mesen2 PASSes (blue); pending #3063"]
    fn test_dmavalid_passes() {
        run_rom_screen_crc("test_dmavalid_v01/test_dmavalid.smc", 600, 0x0B56_4EEF);
    }

    /// #3063: NESER's self-check FAILs (red) where Mesen2 PASSes (blue). The
    /// CPU clears $420C mid-frame; NESER gets the mid-frame HDMA-disable
    /// behaviour wrong.
    #[test]
    #[ignore = "self-check FAILs (red) where Mesen2 PASSes (blue); pending #3063"]
    fn test_hdmadisable_passes() {
        run_rom_screen_crc("test_hdmadisable/test_hdmadisable.smc", 600, 0x0B56_4EEF);
    }

    /// #3063: NESER diverges from Mesen2 by ~0.93% (532 px) at frame 600 -- a
    /// small DMA-timing visual difference.
    #[test]
    #[ignore = "~0.93% pixel divergence from Mesen2; pending #3063"]
    fn test_dmatiming_matches_mesen2() {
        run_rom_screen_crc("test_dmatiming/demo.smc", 600, 0x97B7_5364);
    }

    /// #3062 (byte-identical byuu mirror of KungFuFurby test_hdmasync): NESER
    /// renders black where Mesen2 renders blue PASS.
    #[test]
    #[ignore = "renders black instead of Mesen2's blue PASS; pending #3062"]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdma/test_hdmasync.smc", 600, 0x6E8D_8520);
    }

    /// #3062 (byte-identical byuu mirror of KungFuFurby test_hdmatiming): NESER
    /// self-check FAILs (red) where Mesen2 PASSes (blue).
    #[test]
    #[ignore = "self-check FAILs (red) where Mesen2 PASSes (blue); pending #3062"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdma/test_hdmatiming.smc", 600, 0x8662_6F50);
    }
}
