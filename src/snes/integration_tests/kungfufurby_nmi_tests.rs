//! KungFuFurby's NMI test ROM collection (issue #2883), sourced from a 2016
//! forum find of byuu's "SNES TEST IMAGE" test suite (no formal license,
//! recorded as `unknown`; `test_irq.smc`/`test_irq4200.smc` are
//! byte-identical to the tukuyomi-bsnes-tests mirror, `nmi.smc` is
//! byte-identical to the jonasquinn-test-roms mirror -- see
//! `roms/snes/automated_tests/manifest.json`).
//!
//! Each ROM renders a solid backdrop color once its self-check completes:
//! blue for PASS, red/maroon for FAIL. Verified against Mesen2 headless
//! captures (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`), which show blue for all three ROMs
//! at frame 600 (nmi.smc transitions blue between frames ~450-600;
//! test_nmi.smc between ~30-60; demo_nmitest.smc is stable blue from
//! frame 5).
//!
//! **Golden convention (#3092).** `test_nmi_passes` below asserts the
//! Mesen2-correct blue PASS screen, not NESER's current output, so it FAILs
//! under `cargo test --include-ignored` until #3093 lands -- the designed
//! state, not a regression. See `kungfufurby_irq_tests`' module doc for the
//! rationale; the IRQ family is tracked on the same issue because both show
//! the same shape (the `demo_*` ROM passes, every other ROM renders the
//! identical maroon FAIL fill).

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
    /// this fix -- see `test_nmi_passes` below and #3049's follow-up).
    #[test]
    fn nmi_passes() {
        run_rom_screen_crc("nmi.smc", 600, 0x8695_BBB0);
    }

    /// #3093: NESER's self-check FAILs where Mesen2 PASSes (blue). NESER
    /// settles on a flat maroon `(82, 0, 0)` fill from frame 61 onward, so
    /// frame 120 samples a stable screen.
    ///
    /// `nmi_passes`' #3049 per-cycle dispatch fix does NOT change this ROM's
    /// outcome (identical CRC before/after) -- this self-checking ROM's
    /// divergence is a different residual gap. A spike extending the
    /// PHA/PHX/PHY internal-cycle-before-the-push fix to the full push/pull
    /// family (PLA/PLX/PLY/PLP/PLB/PLD/PHP/PHB/PHD/PHK) was tried and
    /// disproven -- it did not change this ROM's CRC either, so the pull-side
    /// mirror of the PHA fix (needed to close `nmi.smc`'s own remaining
    /// 5-line bus-trace residual, see `nmi_passes`) is NOT this ROM's root
    /// cause. Root cause not yet identified; needs fresh investigation.
    ///
    /// Until #3092 this test asserted `0x8662_6F50`, which matched neither a
    /// PASS nor NESER's own output -- that literal is a flat `(66, 0, 0)`
    /// fill, the settled screen of `test_hdmatiming.smc`, not of this ROM.
    /// Being `#[ignore]`d, the mismatch went unnoticed from #2883 until the
    /// #3092 audit.
    #[test]
    #[ignore = "self-check FAILs (maroon) where Mesen2 PASSes (blue); asserts the correct PASS golden so FAILs under --include-ignored until #3093"]
    fn test_nmi_passes() {
        run_rom_screen_crc("test_nmi.smc", 120, 0x8695_BBB0);
    }
}
