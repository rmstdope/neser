//! KungFuFurby's NMI test ROM collection (issue #2883), sourced from a 2016
//! forum find of byuu's "SNES TEST IMAGE" test suite (no formal license,
//! recorded as `unknown`; `test_irq.smc`/`test_irq4200.smc` are
//! byte-identical to the tukuyomi-bsnes-tests mirror, `nmi.smc` is
//! byte-identical to the jonasquinn-test-roms mirror -- see
//! `roms/snes/automated_tests/manifest.json`).
//!
//! Each ROM renders a solid backdrop color once its self-check completes:
//! blue for PASS, red for FAIL, both painted by byuu's `pass()`/`fail()`
//! epilogues, which end in `stp`. Verified against Mesen2 headless captures
//! (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`), which show blue for all three ROMs
//! at frame 600 (nmi.smc transitions blue between frames ~450-600;
//! test_nmi.smc between ~30-60; demo_nmitest.smc is stable blue from
//! frame 5).
//!
//! All three pass. `test_nmi_passes` was `#[ignore]`d under #3093 until #3116,
//! on the strength of a maroon screen that turned out not to be a FAIL verdict:
//! STP did not halt the CPU, so the ROM painted its blue PASS and then fell
//! through the dead `stp` into `fail()` and on into the following bytes.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a KungFuFurby NMI ROM to `frames` and asserts the rendered
    /// screen matches the Mesen2-approved PASS golden CRC32.
    ///
    /// To approve a new golden, run with NESER_CAPTURE_SCREEN=1 and diff the
    /// capture under target/snes_test_captures/ against a Mesen2 headless
    /// capture at the same frame *programmatically* (never by eye), then
    /// record the CRC here.
    fn run_rom_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "kungfufurby_nmi_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: expected screen-CRC PASS (blue) at frame {frames}, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    #[test]
    fn demo_nmitest_passes() {
        run_rom_screen_crc("demo_nmitest.smc", 600, 0x8695_BBB0);
    }

    /// Fixed by #3049: NMI dispatch is now checked per-CPU-cycle (an
    /// arm-this-cycle/latch-next-cycle counter, mirroring Mesen2's
    /// `NmiFlagCounter`/`DetectNmiSignalEdge`, hooked into `tick_read`/
    /// `tick_write`/`tick_internal_cycle`) instead of once per `step()` call,
    /// plus two related opcode-timing fixes surfaced while verifying this
    /// ROM's bus trace: the hardware-interrupt dispatch sequence's two wasted
    /// cycles now happen before the pushes (matching Mesen2's
    /// `ProcessInterrupt`), and PHA/PHX/PHY's internal cycle now ticks before
    /// the push (matching Mesen2's `PHA`/`PHX`/`PHY`). Verified pixel-exact
    /// against a Mesen2 capture at frame 600 (this ROM's NMI handler PHAs A
    /// as its very first instruction), and a bus-trace diff against Mesen2
    /// across the first 500k+ master clocks is byte-identical except for one
    /// pull sequence right before this ROM's RTI (`PLA` and friends have the
    /// mirror-image "internal cycle before the pull" issue, out of scope for
    /// this fix -- see #3049's follow-up; it is not what `test_nmi_passes`
    /// below was failing on, which #3116 settled).
    #[test]
    fn nmi_passes() {
        run_rom_screen_crc("nmi.smc", 600, 0x8695_BBB0);
    }

    /// Sampled at frame 120, comfortably past this ROM's ~frame 61 settle
    /// point, so the screen is stable when the CRC is taken.
    ///
    /// Passes since #3116. Two earlier attempts to find an NMI-side root cause
    /// here came up empty and are worth recording, because the reason they
    /// failed is that there was no NMI bug to find: neither `nmi_passes`' #3049
    /// per-cycle dispatch fix nor a spike extending the PHA/PHX/PHY
    /// internal-cycle-before-the-push fix to the full push/pull family changed
    /// this ROM's CRC. The maroon screen was post-`stp` garbage, not a verdict.
    ///
    /// Until #3092 this test asserted `0x8662_6F50`, which matched neither a
    /// PASS nor NESER's own output -- that literal is a flat `(66, 0, 0)`
    /// fill, the settled screen of `test_hdmatiming.smc`, not of this ROM.
    /// Being `#[ignore]`d, the mismatch went unnoticed from #2883 until the
    /// #3092 audit.
    #[test]
    fn test_nmi_passes() {
        run_rom_screen_crc("test_nmi.smc", 120, 0x8695_BBB0);
    }
}
