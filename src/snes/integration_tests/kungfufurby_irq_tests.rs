//! KungFuFurby's IRQ test ROM collection (issue #2883), from the same
//! "SNES TEST IMAGE" (byuu) family as `kungfufurby_nmi_tests` (no formal
//! license, recorded as `unknown`; `test_irq.smc`/`test_irq4200.smc` are
//! byte-identical to the tukuyomi-bsnes-tests mirror -- see
//! `roms/snes/automated_tests/manifest.json`).
//!
//! Same pass/fail convention as the NMI suite: blue backdrop for PASS, red
//! for FAIL. Both are painted by byuu's `pass()`/`fail()` epilogues, which end
//! in `stp`. Verified against Mesen2 headless captures, which show blue for all
//! six ROMs by frame 600 (`irq.smc` needs longer, matching `demo_irqtest.smc`
//! transitioning well after frame 600 too).
//!
//! All six originally failed in NESER (investigated as issue #2883
//! increment 2, found to share `kungfufurby_nmi_tests`' interrupt-dispatch
//! root cause, tracked in #3049). #3049's per-CPU-cycle NMI and H/V-IRQ
//! dispatch fixes (see `src/snes/cpu/cpu.rs`) fixed `demo_irqtest.smc`,
//! pixel-verified against Mesen2 at frame 600. #3116 then fixed
//! `test_irq4209.smc` here and `test_nmi.smc` in the NMI suite -- neither had
//! been failing on IRQ behaviour at all: STP did not halt the CPU, so a PASS
//! run painted blue and immediately fell through the dead `stp` into `fail()`.
//! The `maroon` FAIL shades this doc used to record were post-halt garbage, not
//! ROM verdicts.
//!
//! #3144 then ported Mesen2's level+edge IRQ counter circuit (`ppu/irq.rs`),
//! fixing `test_irq4200.smc` -- the one ROM whose failure was circuit
//! semantics (enable writes re-evaluated against a continuous compare level).
//!
//! #3146 then fixed the CPU recognition boundary: the IRQ line is now sampled
//! at the START of each CPU cycle for dispatch (Mesen2 `PrevIrqSource`), where
//! NESER had sampled it at the end and so dispatched one instruction early.
//! That fixed `test_irq.smc` outright -- all ten sub-tests, not just the
//! dispatch-boundary sub-test 1 that was visible -- and also `irq.smc`, whose
//! verdict was opaque (no source exists) and which had been slated for a Mesen2
//! trace diff; it shared the root cause and went green with no change aimed at
//! it.
//!
//! `test_irqb.smc` went green in two halves under #3147. Its sub-test 4 --
//! the one the issue was filed against, latching OPHCT `$0C` where hardware
//! records `$10` -- turned out to be the same dispatch-boundary defect and was
//! already fixed by the cycle-start move above; the `$0C` is exactly what a
//! dispatch one instruction early produces there, confirmed by reverting the
//! move and watching the whole sub-test 4 block return to its historical
//! `0C 00 01 00 76 00 01 00 CC`. What remained was sub-test 5, and it was not
//! an IRQ defect at all: `jmp $217F` lands in the APU comm-port mirrors
//! (`$2144-$217F`), which NESER decoded as open bus, so the CPU ran a 6-cycle
//! `AND (dp,X)` where hardware runs the `CLC` the ROM's SPC preamble planted.
//! An ordinal-aligned bus-trace diff against Mesen2 matched for 197 cycles and
//! split on exactly that fetch.
//!
//! **Golden convention (#3092).** Every test here asserts the
//! *Mesen2-correct* blue PASS screen, not NESER's current output -- whether
//! it currently passes or is still `#[ignore]`d pending a tracked fix. An
//! ignored one therefore FAILs under `cargo test --include-ignored` until
//! that fix lands -- the designed state, not a regression -- and turns green
//! exactly when the underlying gap closes, at which point the `#[ignore]`
//! comes off and the same golden keeps guarding it. Recording NESER's own
//! diverging CRC instead (the convention these tests used before #3092)
//! inverts that signal: a real fix would turn them red and read as a
//! breakage.
//!
//! Every golden here was verified against a fresh Mesen2 headless capture
//! (`--testRunner`, flags per the `snes-hardware-research` skill) at the
//! same frame the test samples: each is a uniform `(0, 0, 255)` 256x224
//! fill hashing to `0x8695_BBB0`. That one literal recurs across every
//! blue-PASS golden in the SNES suite because a flat fill of a given
//! colour and size hashes identically whichever ROM produced it.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a KungFuFurby IRQ ROM to `frames` and asserts the rendered
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
            "kungfufurby_irq_tests",
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

    /// Was red under #3093 for a long time -- unaffected by #3049's per-cycle
    /// dispatch fixes and by #3144's IRQ counter circuit port (identical CRC
    /// before/after both). No source or disassembly exists for this ROM, so its
    /// verdict was opaque and #3093 planned a Mesen2 trace diff for it. That
    /// turned out to be unnecessary: it shares the recognition boundary
    /// `test_irq.smc` sub-test 1 measured, and went green with #3146's
    /// cycle-start dispatch sample without any change aimed at it.
    #[test]
    fn irq_passes() {
        run_rom_screen_crc("irq.smc", 1200, 0x8695_BBB0);
    }

    /// Fixed by #3146. NESER used to dispatch the V-IRQ one instruction early,
    /// so sub-test 1 captured `$EA` (nop) where hardware captures `$18` (clc):
    /// the IRQ line was sampled *after* each CPU cycle's clocks, so a line
    /// rising 2 clocks into `sec`'s final cycle was seen at that boundary
    /// instead of the next. Sampling at cycle start (Mesen2 `PrevIrqSource`)
    /// fixed all ten sub-tests at once. Read as maroon before #3116, which was
    /// post-`stp` garbage rather than the ROM's verdict.
    ///
    /// `test_irq_sram_verdict_reports_all_ten_sub_tests_pass` names the
    /// failing sub-test if this ever goes red again.
    #[test]
    fn test_irq_passes() {
        run_rom_screen_crc("test_irq.smc", 600, 0x8695_BBB0);
    }

    /// Fixed by #3144's port of Mesen2's level+edge IRQ counter circuit: every
    /// one of this ROM's 80 mid-scanline `$4200` enable writes (40 per HTIME
    /// setup) needs the write to be re-evaluated against a continuous compare
    /// level (V == VTIME for the whole matching line), which the old
    /// single-instant point compare could not express -- its pre-fix SRAM log
    /// was all `$FF` sentinels, not one IRQ fired. Read as maroon before #3116, which was post-`stp`
    /// garbage rather than the ROM's verdict.
    ///
    /// `test_irq4200_sram_log_matches_byuu_check_table` breaks the verdict
    /// down when this goes red again.
    #[test]
    fn test_irq4200_passes() {
        run_rom_screen_crc("test_irq4200.smc", 600, 0x8695_BBB0);
    }

    /// Passes since #3116. The maroon screen recorded here was never a FAIL
    /// verdict: STP did not halt, so `pass()` painted blue and then fell
    /// through the dead `stp` into `fail()` and on into the bytes beyond.
    #[test]
    fn test_irq4209_passes() {
        run_rom_screen_crc("test_irq4209.smc", 600, 0x8695_BBB0);
    }

    /// Green since #3147, which took two fixes landing in different places.
    /// Sub-test 4 (`jmp $000000` with the stack parked on the register file)
    /// was fixed by #3146's move of the IRQ dispatch sample to cycle start:
    /// with the old end-of-cycle sample the dispatch fired one instruction
    /// early, at the `jml` boundary rather than after the `BIT #$83` the CPU
    /// really runs out of low WRAM, and the interrupt sequence's `PCL` push
    /// into `$4201` latched OPHCT `$0C` instead of `$10`. Sub-test 5 was the
    /// APU port mirrors (`$2144-$217F`), fixed here.
    ///
    /// `test_irqb_sram_log_matches_byuu_expected_latches` names the failing
    /// sub-test and byte when this is red.
    #[test]
    fn test_irqb_passes() {
        run_rom_screen_crc("test_irqb.smc", 600, 0x8695_BBB0);
    }

    /// Fixed by #3049's per-cycle H/V-IRQ dispatch fix (`irq_line_shadow`,
    /// resampled once per CPU cycle instead of once per instruction).
    /// Verified pixel-exact against a Mesen2 capture at frame 600.
    #[test]
    fn demo_irqtest_passes() {
        run_rom_screen_crc("demo_irqtest.smc", 600, 0x8695_BBB0);
    }

    /// Boots `file` in a bare [`Snes`](crate::snes::console::Snes) and runs it
    /// for `frames` rendered frames so a test can inspect the verdict bytes the
    /// ROM caches in cartridge SRAM (bank `$70`). Same pattern as
    /// `jonasquinn_dma_tests::test_dmavalid_sub_checks_match_byuu_expectations`.
    fn run_rom_for_sram(file: &str, frames: u32) -> crate::snes::console::Snes {
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        let path = Path::new(ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
        snes.load_rom(&rom, file).unwrap();
        let mut rendered = 0u32;
        while rendered < frames {
            snes.run_tick();
            if snes.is_ready_to_render() {
                rendered += 1;
                snes.clear_ready_to_render();
            }
        }
        snes
    }

    /// Verdict path for `test_irq_passes`: a screen CRC only says blue-or-red,
    /// so this asserts the evidence the ROM itself keeps in SRAM. byuu's
    /// `!test_number` lives at `$700000`, is incremented as each of the ten
    /// sub-tests starts, and is zeroed only on the path into `pass()` -- so a
    /// non-zero value names the first failing sub-test. Sub-tests 1-4
    /// additionally store the wrongly captured opcode byte to `$700001`.
    /// Expectations transcribed from the vendored
    /// `jonasquinn-test-roms/snestest_082506/test_irq.asm`, not from any
    /// emulator's output.
    #[test]
    fn test_irq_sram_verdict_reports_all_ten_sub_tests_pass() {
        let snes = run_rom_for_sram("test_irq.smc", 120);
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0);
        let sub_test = rd(0x70_0000);
        let captured_opcode = rd(0x70_0001);
        assert_eq!(
            sub_test, 0x00,
            "test_irq.smc failed at sub-test {sub_test} \
             (captured opcode for sub-tests 1-4: ${captured_opcode:02X})"
        );
    }

    /// One 20-byte block of byuu's `check:` table in `test_irq4200.asm`,
    /// transcribed verbatim: for each of the ten `%checkirq(a,b,c,d)` rounds
    /// per HTIME setup, every $4200 enable write that fires an IRQ logs the
    /// interrupted A (= the enable value just written) through the `$2180`
    /// WRAM port, and each round ends with an `$FF` sentinel. Only the
    /// mid-scanline V-IRQ enables (`$20`) may fire on the VTIME=1 line;
    /// H-and-HV enables (`$10`/`$30`) never hit their H compare inside the
    /// enable window.
    const TEST_IRQ4200_CHECK_BLOCK: [u8; 20] = [
        0xFF, // (00,00,00,00): no IRQs
        0xFF, // (00,10,00,10): H enables do not fire mid-line
        0x20, 0x20, 0xFF, // (00,20,00,20): both V enables fire
        0xFF, // (00,30,00,30): HV needs the H compare too
        0xFF, // (10,10,10,10)
        0x20, 0x20, 0xFF, // (10,20,10,20)
        0xFF, // (10,30,10,30)
        0x20, 0x20, 0x20, 0x20, 0xFF, // (20,20,20,20): every re-enable re-fires
        0x20, 0x20, 0xFF, // (20,30,20,30)
        0xFF, // (30,30,30,30)
    ];

    /// Verdict path for `test_irq4200_passes`: the ROM copies its `$7F0000`
    /// IRQ-fire log to SRAM `$700000` before comparing the first `$28` bytes
    /// against its in-ROM `check:` table (first mismatch jumps to `fail()`).
    /// This asserts those same 40 bytes: [`TEST_IRQ4200_CHECK_BLOCK`] once for
    /// the HTIME=0 setup and once for HTIME=$152, VTIME=1 in both.
    /// Expectations transcribed from the vendored disassembly
    /// `jonasquinn-test-roms/blobs/disassembly/test_irq4200.asm`. Green since
    /// #3144's IRQ counter circuit port.
    #[test]
    fn test_irq4200_sram_log_matches_byuu_check_table() {
        let snes = run_rom_for_sram("test_irq4200.smc", 120);
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0);
        let actual: Vec<u8> = (0..0x28).map(|i| rd(0x70_0000 + i)).collect();
        let expected: Vec<u8> = TEST_IRQ4200_CHECK_BLOCK
            .iter()
            .chain(TEST_IRQ4200_CHECK_BLOCK.iter())
            .copied()
            .collect();
        assert_eq!(
            actual, expected,
            "IRQ-fire log mismatch (each byte is the $4200 enable value whose \
             write fired an IRQ, $FF = end-of-round sentinel); \
             actual={actual:02X?} expected={expected:02X?}"
        );
    }

    /// Verdict path for `test_irqb_passes`: the ROM's IRQ handler logs
    /// OPHCT/OPVCT latch reads (`$213C`/`$213D`, the value's low byte and then
    /// the second read's bit 0 -- bit 8 of the 9-bit counter) before and after
    /// re-latching via a `$4201` WRIO write and a `$2137` strobe, for five
    /// sub-tests that each take the IRQ while executing from a different
    /// address (`jmp $2137`/`$2136`/plain code/`$000000`/`$217F`). Both
    /// `pass()` and `fail()` copy the `$7F0000..$7F07FF` log to SRAM, so the
    /// bytes are readable either way; this asserts exactly the bytes each
    /// sub-test's own `check` block compares. Expectations transcribed from the
    /// vendored disassembly
    /// `jonasquinn-test-roms/blobs/disassembly/test_irqb.asm`.
    ///
    /// Two of the five sub-tests are not what their `jmp` target suggests.
    /// Sub-test 4's `jmp $000000` is *not* an open-bus fetch: the ROM has just
    /// stored its own `test4_check` long pointer to `$00/$01/$02`, so the CPU
    /// executes `BIT #$83` out of low WRAM, and the 16 master clocks that
    /// instruction costs are exactly what puts the interrupt sequence's `PCL`
    /// push into `$4201` at H=16 (#3146/#3147). Sub-test 5's `jmp $217F` is the
    /// genuinely open-bus-looking one, and it is not open bus either --
    /// `$2144-$217F` mirror the APU comm ports, which the ROM's
    /// `smp_return_0x18` preamble has primed to read `$18` = `CLC`. Its check
    /// offsets are shifted by two (`+0..+6, +10` where sub-tests 1-4 use
    /// `+0..+4, +8`) because two `$2180` WMDATA *reads* -- `CLC`'s
    /// interrupt-imminent dummy read and the interrupt sequence's own dummy
    /// re-read of PC, both landing on `$2180` -- advance WMADD past two
    /// never-written log slots before the handler writes anything.
    #[test]
    fn test_irqb_sram_log_matches_byuu_expected_latches() {
        let snes = run_rom_for_sram("test_irqb.smc", 120);
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0);
        // (sub-test, SRAM offset, expected byte), in the ROM's own check order.
        const CHECKS: [(u8, u32, u8); 32] = [
            (1, 0x00, 0x07),
            (1, 0x01, 0x00),
            (1, 0x02, 0x01),
            (1, 0x03, 0x00),
            (1, 0x04, 0x7A),
            (1, 0x08, 0xD0),
            (2, 0x0C, 0x06),
            (2, 0x0D, 0x00),
            (2, 0x0E, 0x00),
            (2, 0x0F, 0x00),
            (2, 0x10, 0x7A),
            (2, 0x14, 0xD0),
            (3, 0x18, 0x07),
            (3, 0x19, 0x00),
            (3, 0x1A, 0x01),
            (3, 0x1B, 0x00),
            (3, 0x1C, 0x7C),
            (3, 0x20, 0xD2),
            (4, 0x24, 0x10),
            (4, 0x25, 0x00),
            (4, 0x26, 0x01),
            (4, 0x27, 0x00),
            (4, 0x28, 0x7A),
            (4, 0x2C, 0xD0),
            (5, 0x30, 0x00),
            (5, 0x31, 0x00),
            (5, 0x32, 0x06),
            (5, 0x33, 0x00),
            (5, 0x34, 0x00),
            (5, 0x35, 0x00),
            (5, 0x36, 0x7A),
            (5, 0x3A, 0xCF),
        ];
        // Report every mismatch at once. A fail-fast loop hides the rest of the
        // log behind the first bad byte, which is how sub-test 4's later
        // latches went unread and sub-test 5 stayed unverified for as long as
        // it did. Note the ROM stops at ITS first mismatch too, so once a
        // sub-test diverges the later sub-tests never run and their slots stay
        // at the `$00` the ROM pre-filled -- read a run of `$00`s as "never
        // reached", not as agreement.
        let mismatches: Vec<String> = CHECKS
            .iter()
            .filter_map(|&(sub_test, offset, expected)| {
                let actual = rd(0x70_0000 + offset);
                (actual != expected).then(|| {
                    format!(
                        "  sub-test {sub_test}: $70{offset:04X} = ${actual:02X}, \
                         expected ${expected:02X}"
                    )
                })
            })
            .collect();
        assert!(
            mismatches.is_empty(),
            "test_irqb OPHCT/OPVCT latch log diverges from byuu's expectations \
             in {} of {} checked bytes:\n{}",
            mismatches.len(),
            CHECKS.len(),
            mismatches.join("\n")
        );
    }
}
