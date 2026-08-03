//! KungFuFurby's HDMA test ROM collection (issue #2884), from the same byuu
//! "SNES TEST IMAGE" suite as the NMI/IRQ ROMs (no formal license, recorded as
//! `unknown`; see `roms/snes/automated_tests/manifest.json`). These three HDMA
//! ROMs were earmarked out of scope for #2883/#3049 and are automated here.
//!
//! Like the NMI/IRQ suites, each ROM renders a solid backdrop colour on
//! completion (blue = PASS, red = FAIL), cross-checked against a Mesen2
//! headless capture (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`) of the identical ROM file. Both colours
//! are painted by byuu's `pass()`/`fail()` epilogues, which end in `stp`; until
//! #3116 NESER ran through that halt and every screen here was post-halt
//! garbage rather than the ROM's verdict.
//!
//! **Golden convention (#3092).** The two still-`#[ignore]`d tests assert the
//! Mesen2-correct blue PASS screen, not NESER's current output, so they FAIL
//! under `cargo test --include-ignored` until #3062 lands -- the designed
//! state, not a regression. See `kungfufurby_irq_tests`' module doc for the
//! rationale.

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

    /// Four sub-tests covering HDMA init semantics. Passes since #3062, which
    /// took two fixes, both found by reading the sub-test number the ROM leaves
    /// in SRAM byte 0 (`!test_number`; 0 = pass, 1-4 = the failing sub-test):
    ///
    /// - `$43xA` is the line counter itself, not an internal copy, so the
    ///   counter a ROM writes by hand before enabling `$420C` mid-frame is the
    ///   one that gets decremented.
    /// - `$213B` reads of a CGRAM high byte take bit 7 from PPU2 open bus.
    ///   Sub-test 4 HDMAs `$9ABC` into CGRAM and reads it back as `$BC, $9A`;
    ///   CGRAM only stores 15 bits, so without the open-bus bit it read `$1A`.
    ///
    /// Before #3116 it settled on solid black, because STP did not halt:
    /// `pass()`/`fail()` ran on into `get_dma_counter`, which starts a GPDMA
    /// and then `rts` on a stack it never set up.
    #[test]
    fn test_hdma_passes() {
        run_rom_screen_crc("test_hdma.smc", 600, 0x8695_BBB0);
    }

    /// This ROM is slow -- BOTH emulators render solid black until ~frame 1027,
    /// so the frame-600 sample this test used before #3092 could never see the
    /// divergence (it asserted "still black", which Mesen2 also produces).
    /// Sampled at frame 1100 instead, comfortably past the transition.
    ///
    /// Passes since #3116. The apparent HDMA divergence was the missing STP
    /// halt: the ROM did reach `pass()` and paint blue, then fell through the
    /// dead `stp` into `fail()` within the same scanline, so no frame ever
    /// showed the blue.
    #[test]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdmasync.smc", 1100, 0x8695_BBB0);
    }

    /// #3062: the ROM compares 8 rows of latched H positions and channel
    /// registers against an in-ROM `compdata` table (ROM offset `0x153D`) and
    /// leaves both in SRAM `$00..$3F`, so the failing row is readable from the
    /// emitted `.sav`.
    ///
    /// Down to **one** differing value. Rows 2-8 match exactly, including the
    /// H=1104 trigger and `$2137` latch checks in rows 7-8. Row 1 differs only
    /// in its first latch -- `want $014E, got $014F` -- while the second latch
    /// in the same row matches, so it is a 4-master-clock CPU arrival
    /// difference at one alignment, not an HDMA envelope error: `SyncStartDma`
    /// and `SyncEndDma` are already formula-identical to Mesen2's. That places
    /// it in the #3050 CPU-alignment family. Rows 9-12 also differ but the ROM
    /// records them without comparing them (`cpx.w #64` stops at row 8), so
    /// they do not affect the verdict -- they are the unmodelled
    /// HDMA-during-GPDMA nesting noted in `src/snes/bus/dma.rs`.
    #[test]
    #[ignore = "one residual value (row 1's first latch, 4 master clocks); asserts the correct PASS golden so FAILs under --include-ignored until #3062"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdmatiming.smc", 600, 0x8695_BBB0);
    }
}
