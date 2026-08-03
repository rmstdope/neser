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
//! KungFuFurby ROMs (md5 `acec8b53...` for the former); the first now passes,
//! and the second carries the same single residual as its KungFuFurby twin.
//!
//! **Golden convention (#3092).** The remaining `#[ignore]`d *self-check* ROM
//! (`test_hdmatiming_passes`) asserts the Mesen2-correct blue PASS screen, not
//! NESER's current output, so it FAILs under `cargo test --include-ignored`
//! until #3120 lands -- the designed state, not a regression. See
//! `kungfufurby_irq_tests`' module doc for the rationale.
//! `test_dmatiming_matches_mesen2` is deliberately NOT on this convention: it
//! is a pixel-diff comparison against Mesen2's own render, not a PASS/FAIL
//! self-check, so a blue backdrop is not its correct result and it keeps
//! recording NESER's current diverging CRC.
//!
//! **Comparing this collection against Mesen2 needs a matched WRAM power-on
//! state.** `test_dmatiming/demo.smc` displays uninitialised WRAM, and Mesen2's
//! SNES default is `RamState::Random` while NESER zero-fills, so a capture
//! taken without `--snes.RamPowerOnState=AllZeros` differs from NESER *and from
//! itself* run to run. #3063's original "~0.93% divergence" for that ROM was
//! that artifact: two Random Mesen2 captures differ from each other by 1.06%,
//! more than either differs from NESER.

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

    /// Passes since #3111: the ROM checks that DMA between WRAM and the WMDATA
    /// port `$2180` moves no data in either direction, and NESER used to perform
    /// the copy. Before #3116 this rendered maroon `(82, 0, 0)`, which was
    /// post-`stp` garbage rather than the ROM's verdict; between the two fixes
    /// it rendered byuu's real `fail()` red `(255, 0, 0)`.
    ///
    /// `test_dmavalid_sub_checks_match_byuu_expectations` breaks the verdict
    /// down when this goes red again.
    #[test]
    fn test_dmavalid_passes() {
        run_rom_screen_crc("test_dmavalid_v01/test_dmavalid.smc", 600, 0x8695_BBB0);
    }

    /// Verdict path for `test_dmavalid_passes`: a screen CRC only says
    /// blue-or-red, so this asserts the evidence the ROM itself caches in
    /// cartridge SRAM and then compares. Expectations are transcribed from the
    /// vendored `test_dmavalid.asm` (its own comment blocks at the end of test 2
    /// and test 3), not from NESER's output.
    ///
    /// Test 2 is WRAM -> `$2180`, test 3 is `$2180` -> WRAM. Both must leave
    /// WMADD unmoved and both must still step the channel registers and consume
    /// the transfer's time.
    #[test]
    fn test_dmavalid_sub_checks_match_byuu_expectations() {
        use crate::platform::app_context::AppContext;
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        let path = Path::new(ROOT).join("test_dmavalid_v01/test_dmavalid.smc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let mut snes = Snes::new(AppContext::new_with_config(
            crate::platform::config::Config::default(),
        ));
        snes.load_rom(&rom, "test_dmavalid.smc").unwrap();
        let mut frames = 0u32;
        while frames < 60 {
            snes.run_tick();
            if snes.is_ready_to_render() {
                frames += 1;
                snes.clear_ready_to_render();
            }
        }
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0);

        // Test 2 -- WRAM -> $2180 (asm lines 80-96).
        assert_eq!(rd(0x70_0000), 0x3F, "$2180 not incremented (test 2)");
        assert_eq!(rd(0x70_0001), 0x55, "WMADD did not advance $400 (test 2)");
        assert_eq!(rd(0x70_0002), 0x55, "no DMA write occurred (test 2)");
        assert_eq!(
            (rd(0x70_0003), rd(0x70_0004), rd(0x70_0005)),
            (0x00, 0x14, 0x7E),
            "$43x2 still incremented (test 2)"
        );
        assert_eq!(
            (rd(0x70_0006), rd(0x70_0007)),
            (0x00, 0x00),
            "$43x5 still decremented (test 2)"
        );
        assert!(
            rd(0x70_000A) >= 0x33,
            "the refused transfer still consumed its time (test 2): vtime {:#04X} < 0x33",
            rd(0x70_000A)
        );

        // Test 3 -- $2180 -> WRAM (asm lines 141-158). Here the A-bus write DOES
        // happen; byuu only requires the destination to stop being its seed.
        assert_eq!(rd(0x70_0010), 0x3F, "$2180 not incremented (test 3)");
        assert_eq!(rd(0x70_0011), 0x55, "WMADD did not advance $400 (test 3)");
        assert_ne!(
            rd(0x70_0012),
            0xAA,
            "the DMA write did occur, with a value that is not the seed (test 3)"
        );
        assert_eq!(
            (rd(0x70_0013), rd(0x70_0014), rd(0x70_0015)),
            (0x00, 0x14, 0x7E),
            "$43x2 still incremented (test 3)"
        );
        assert_eq!(
            (rd(0x70_0016), rd(0x70_0017)),
            (0x00, 0x00),
            "$43x5 still decremented (test 3)"
        );
        assert!(
            rd(0x70_001A) >= 0x33,
            "the refused transfer still consumed its time (test 3): vtime {:#04X} < 0x33",
            rd(0x70_001A)
        );
    }

    /// The CPU clears $420C mid-frame. Passes since #3116: the maroon screen
    /// this ROM used to render was not a FAIL verdict at all but post-`stp`
    /// garbage, so the mid-frame HDMA-disable behaviour #3063 suspected here was
    /// never actually wrong.
    #[test]
    fn test_hdmadisable_passes() {
        run_rom_screen_crc("test_hdmadisable/test_hdmadisable.smc", 600, 0x8695_BBB0);
    }

    /// NESER diverges from Mesen2 by 36 px (0.063%) at frame 600, once both
    /// emulators start from the same WRAM.
    ///
    /// #3063 recorded this as "~0.93% (532 px)". That figure was an artifact:
    /// the ROM's "Full" and "Diff" rows display never-written WRAM, and Mesen2
    /// defaults to `RamState::Random`, so its capture is not reproducible even
    /// against itself (two Random runs differ by 1.06%, i.e. more than either
    /// differs from NESER). Re-measured with
    /// `--snes.RamPowerOnState=AllZeros`, the divergence is 36 px confined to
    /// the "Base" and "Diff" rows -- the `$213C`/`$213D` latch this ROM exists
    /// to measure. `test_dmatiming_latches_hv_after_gpdma` isolates it as one
    /// dot (4 master clocks) and carries the full diagnosis; #3127.
    ///
    /// Deliberately excluded from #3092's aspirational-golden convention (this
    /// is not an oversight): the ROM renders a picture rather than a PASS/FAIL
    /// backdrop, so there is no blue screen to assert. The CRC below stays
    /// NESER's own current render; the correct value would be Mesen2's exact
    /// framebuffer hash, which is a different kind of golden -- and one that
    /// would silently encode NESER's WRAM zero-fill (#3128).
    #[test]
    #[ignore = "36 px divergence from Mesen2 (4-clock GPDMA end pad); records NESER's current render, not a PASS golden; pending #3127"]
    fn test_dmatiming_matches_mesen2() {
        run_rom_screen_crc("test_dmatiming/demo.smc", 600, 0x97B7_5364);
    }

    /// The actual DMA-timing oracle in `test_dmatiming/demo.smc`, independent of
    /// the screen: after triggering two 6-byte GPDMA channels with a single
    /// `$420B` write, the ROM latches `$2137` and stores the masked `$213C`
    /// (H) and `$213D` (V) counters to `$7EC000` and `$7EC002` ("Base").
    ///
    /// Ground truth from a Mesen2 headless run of the identical ROM reading the
    /// same two WRAM words: `$7EC000 = 0x0023`, `$7EC002 = 0x0001`. Those are
    /// stable across `--snes.RamPowerOnState=AllZeros` and two separate
    /// `Random` runs, so they are a genuine measurement rather than a readback
    /// of uninitialised memory (unlike the ROM's "Full"/"Diff" rows, which read
    /// `$7EC004`/`$7EC006` -- never written, because the ROM's NMI handler is a
    /// bare `RTI`).
    ///
    /// NESER latches `(0x0024, 0x0001)`: one dot -- 4 master clocks -- late.
    /// Ignored under #3092's aspirational-golden convention, asserting Mesen2's
    /// value so it turns green when the cause lands. Tracked by #3127.
    ///
    /// The cause is located but deliberately NOT fixed here: `start_dma`'s
    /// `sync_end_pad(counter, 8)` rounds the general-purpose end pad to a fixed
    /// 8 where Mesen2 rounds to `cpuSpeed`. Passing `cpu_speed` makes this ROM
    /// match exactly -- and moves `peterlemon_ppu_advanced_tests::
    /// mosaic_mode5_sized` from 0 px to 12484 px (5.44%) against a Mesen2
    /// capture replayed with the identical input script. Both measurements are
    /// fresh, not inherited from #3067's write-up. So the fixed 8 is not simply
    /// wrong, and the real difference is more likely the DMA/HDMA re-entrancy
    /// gap `start_dma`'s comment describes: Mesen2 runs an HDMA firing mid-GPDMA
    /// nested, folding its clocks into the same counter the outer `SyncEndDma`
    /// rounds, which NESER cannot do while `self.dma` is `mem::take`n.
    #[test]
    #[ignore = "4-master-clock GPDMA end-pad divergence; asserts Mesen2's correct value so FAILs under --include-ignored until #3127"]
    fn test_dmatiming_latches_hv_after_gpdma() {
        use crate::platform::app_context::AppContext;
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        let path = Path::new(ROOT).join("test_dmatiming/demo.smc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let mut snes = Snes::new(AppContext::new_with_config(
            crate::platform::config::Config::default(),
        ));
        snes.load_rom(&rom, "demo.smc").unwrap();
        let mut frames = 0u32;
        while frames < 60 {
            snes.run_tick();
            if snes.is_ready_to_render() {
                frames += 1;
                snes.clear_ready_to_render();
            }
        }
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0) as u16;
        let word = |a: u32| rd(a) | (rd(a + 1) << 8);
        assert_eq!(
            (word(0x7E_C000), word(0x7E_C002)),
            (0x0023, 0x0001),
            "Base H/V latched after the two 6-byte GPDMA channels"
        );
    }

    /// Byte-identical byuu mirror of KungFuFurby test_hdmasync: sampled at frame
    /// 1100 rather than 600 because BOTH emulators render solid black until
    /// ~frame 1027 -- see `kungfufurby_hdma_tests::test_hdmasync_passes` for the
    /// full transition timeline and why #3116 turned this green.
    #[test]
    fn test_hdmasync_passes() {
        run_rom_screen_crc("test_hdma/test_hdmasync.smc", 1100, 0x8695_BBB0);
    }

    /// #3062 (byte-identical byuu mirror of KungFuFurby test_hdmatiming): down
    /// to a single differing value, row 1's first latch. See
    /// `kungfufurby_hdma_tests::test_hdmatiming_passes` for the measurement.
    #[test]
    #[ignore = "one residual value (row 1's first latch, 4 master clocks); asserts the correct PASS golden so FAILs under --include-ignored until #3120"]
    fn test_hdmatiming_passes() {
        run_rom_screen_crc("test_hdma/test_hdmatiming.smc", 600, 0x8695_BBB0);
    }
}
