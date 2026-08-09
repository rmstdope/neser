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
//! **Golden convention (#3092).** All three goldens below assert the
//! Mesen2-correct blue PASS screen. See `kungfufurby_irq_tests`' module doc
//! for the rationale.

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
    /// #3120: row 1's first latch (`want $014E, got $014F`) was the H position
    /// exposed to software via OPHCT/`$213C`, which `Ppu::latch_counters` read
    /// straight off the render-dot counter instead of applying the correction
    /// Mesen2's `SnesPpu::GetCycle()` applies across the paired long-dot
    /// compensation region (dots 323/324 and 327/328). Fixed in
    /// `Ppu::readable_h_position` (`src/snes/ppu/timing.rs`); all 8 compared
    /// rows now match, including the H=1104 trigger and `$2137` latch checks
    /// in rows 7-8.
    #[test]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdmatiming.smc", 600, 0x8695_BBB0);
    }

    /// The four "HDMA during DMA" measurements `test_hdmatiming` records but never checks.
    ///
    /// The ROM runs 12 sub-tests and stores each as four words in cartridge SRAM at
    /// `$700000 + (n-1) * 8`, but its own verdict loop stops at `cpx.w #64` -- eight rows.
    /// Rows 9-12 are the ones its source labels "HDMA during DMA" (vendored
    /// `jonasquinn-test-roms/test_hdma/test_hdmatiming.asm:345-522`; the KungFuFurby copy run
    /// here is byte-identical, md5 `900bfa374d61d91bd76aafc453bd24ad`). They are identical to
    /// each other except for 0/1/2/3 `nop`s inserted before the `$420C`/`$420B` pair, so they
    /// sweep the CPU sync phase across the transfer -- the same alignment-sweep idea rows 1-2
    /// use, which pad with `db $42,$00` instead.
    ///
    /// Each row fires a 512-byte general-purpose transfer on channel 0 and an HDMA on
    /// channel 1 in the same breath (`sta $420c` then `sta $420b`), so the per-scanline HDMA
    /// trigger falls **inside** the general-purpose burst. NESER dropped such triggers
    /// entirely -- `dma_tick` never polled them -- so all four rows were wrong before #3127
    /// (row 9 read `[88, 144, 3, 0]`). The 512-byte channel also crosses Mesen2's 8-bit byte
    /// counter, so these rows exercise the end-pad alignment wrap at the same time.
    ///
    /// **The expectations below are Mesen2's, not byuu's**, and that is a deliberate choice
    /// made on measurement. byuu did write values for these rows into the in-ROM `compdata`
    /// table (asm:565-570), but neither reference emulator produces them, and he excluded them
    /// from the ROM's own comparison. Measured first latches:
    ///
    /// ```text
    ///                          row 9   row 10  row 11  row 12
    ///   byuu's compdata           94       96     101      105
    ///   ares (byuu's emulator)    91       95     100      103
    ///   Mesen2                    95       99     104      107
    ///   NESER (this fix)          95       99     104      107
    /// ```
    ///
    /// ares reproduces `compdata` exactly on rows 1-8, so the method is sound and the
    /// disagreement is real: all three emulators differ from the table, and from each other.
    /// Mesen2 sits a **uniform 4 dots** above ares on every row and on both latches, which
    /// says the two references model the nested envelope's length differently rather than
    /// either being phase-dependent. Asserting `compdata` would pin a value nothing achieves;
    /// asserting Mesen2's pins the reference this emulator has chosen to match (#3000, and
    /// the #3127 navigator decision to bit-match Mesen2's DMA counter). The 4-dot
    /// Mesen2/ares gap is recorded as an open question rather than silently resolved here.
    ///
    /// This is still a genuine oracle and not a photograph. It was red before the fix, and it
    /// is the **only** vector in the suite that observes HDMA-during-GPDMA at all: making
    /// `SnesSystemBus::take_due_hdma` return `None` (i.e. dropping the nesting entirely) turns
    /// this test red and leaves every other SNES test green, `hdmaen_latch_test` included.
    /// Reverting the end-pad divisor to a fixed 8 also turns it red.
    #[test]
    fn test_hdmatiming_hdma_during_dma_rows_match_mesen2() {
        use crate::platform::app_context::AppContext;
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        let path = Path::new(ROOT).join("test_hdmatiming.smc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let mut snes = Snes::new(AppContext::new_with_config(
            crate::platform::config::Config::default(),
        ));
        snes.load_rom(&rom, "test_hdmatiming.smc").unwrap();
        let mut frames = 0u32;
        while frames < 600 {
            snes.run_tick();
            if snes.is_ready_to_render() {
                frames += 1;
                snes.clear_ready_to_render();
            }
        }
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0) as u16;
        let word = |a: u32| rd(a) | (rd(a + 1) << 8);
        let row = |n: u32| {
            let base = 0x70_0000 + (n - 1) * 8;
            [word(base), word(base + 2), word(base + 4), word(base + 6)]
        };

        // All four rows are compared together so a failure shows the whole shape: a uniform
        // offset across the rows means the nested envelope's length, while one row moving
        // alone means the sub-test's own CPU sync phase (the pattern #3120 turned on).
        assert_eq!(
            [row(9), row(10), row(11), row(12)],
            // Mesen2 headless, `--snes.RamPowerOnState=AllZeros`, SRAM read at frame 600.
            [
                [0x005F, 0x0098, 0x0003, 0x0000],
                [0x0063, 0x009B, 0x0003, 0x0000],
                [0x0068, 0x00A0, 0x0003, 0x0000],
                [0x006B, 0x00A4, 0x0003, 0x0000],
            ],
            "rows 9-12 vs Mesen2 (see the doc comment: byuu's compdata and ares both differ)"
        );
    }
}
