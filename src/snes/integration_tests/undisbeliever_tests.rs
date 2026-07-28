//! Automates 26 of the 29 vendored undisbeliever/snes-test-roms hardware
//! ROMs (`roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/`)
//! that visually match Mesen2.
//!
//! Unlike blargg/gilyon ROMs, these do not print a PASS/FAIL text screen.
//! Nine of the 26 automated here demonstrate a real, documented
//! SNES hardware bug -- the "INIDISP D7 glitch" -- where writing `$2100`
//! shortly after the CPU/HDMA data bus held a byte with bit 7 set can
//! corrupt sprite rendering or briefly flip force-blank, on real 3-chip/
//! 1-chip consoles (see the NESdev post quoted in
//! <https://github.com/akatsuki105/snes-test-roms/tree/master/undisbeliever-inidisp>,
//! from Near/byuu, and its real-hardware reference photos). This is *not*
//! rare or a coin-flip for most of these ROMs -- the post documents fairly
//! reliable trigger conditions -- but it's an analog bus-residual effect
//! that **no known SNES emulator models**: checked Mesen2
//! (`Core/SNES/SnesPpu.cpp`), ares (`ares/sfc/ppu/io.cpp`, Near's own
//! current emulator), and Snes9x (`ppu.cpp`) -- all three write `$2100`
//! deterministically with no bus-residual modeling at all (see #2949). So
//! each golden in that glitch family is a **stability snapshot** cross-checked
//! against Mesen2 (using `--Video.VideoFilter=None --Video.AspectRatio=NoStretching`
//! for a comparable capture; since the BG vertical-scroll display-line fix
//! in #2945 the two emulators' captures align byte-for-byte with no row
//! offset): for the 2
//! ROMs the source confirms never glitch on real hardware
//! (`hdma_21ff_glitch_matches_mesen2`, `inidisp_hammer_0f0f_matches_mesen2`,
//! marked below) this genuinely *is* proof of hardware accuracy; for the
//! other 9 it only proves parity with a limitation NESER shares with every
//! checked reference emulator, not hardware accuracy -- see #2949 before
//! treating a mismatch here as a regression.
//!
//! The other 3 ROMs are deliberately left un-automated: cross-checking
//! against Mesen2 exposed real NESER divergences, tracked as follow-up bugs
//! rather than papered over with a golden that bakes in known-wrong
//! behavior (see README-SNES.md for the full breakdown):
//! - `hdma-2100-glitch-2ch-{0a,81}.sfc`, `hdma-21ff-2100-glitch.sfc` -- #2943

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const UNDISBELIEVER_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp";

#[cfg(test)]
mod tests {
    use super::*;

    /// All 26 ROMs here settle into their steady-state rendering well
    /// before frame 600 (matching the default budget used throughout
    /// blargg_apu_tests.rs / gilyon_*_tests.rs) and hold it indefinitely.
    ///
    /// Deliberately does not use `rom_runner::assert_rom_screen_crc`: its
    /// panic message says "expected screen-CRC PASS", which would be
    /// misleading here -- these ROMs have no PASS/FAIL concept, so a
    /// mismatch means the stability snapshot changed, not that a "test"
    /// failed.
    fn run_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(UNDISBELIEVER_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "undisbeliever_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {frames} no longer matches the \
             Mesen2-cross-checked stability-snapshot CRC (got 0x{:08X}); if this \
             is an intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! undisbeliever_rom_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_screen_crc($file, 600, $crc);
            }
        };
    }

    // Real hardware usually shows a sprite glitch here (~40% of runs per the
    // NESdev source, "may need a few console resets"). No known emulator
    // (Mesen2/ares/Snes9x) models it, so this golden matches that shared
    // limitation, not hardware accuracy. See #2949.
    undisbeliever_rom_test!(
        hdma_2100_glitch_matches_mesen2,
        "hdma-2100-glitch.sfc",
        0x3B89_56D6
    );

    // Glitches on real hardware via an FXPak firmware bug (a different cause
    // than the other ROMs here, per the NESdev source). Not modeled by any
    // checked emulator. See #2949.
    undisbeliever_rom_test!(
        hdma_21ff_2100_0f_glitch_matches_mesen2,
        "hdma-21ff-2100-0f-glitch.sfc",
        0x3B89_56D6
    );

    // Confirmed by the source to never glitch on real hardware -- this
    // golden genuinely is hardware-accurate, not just Mesen2-matching.
    // See #2949.
    undisbeliever_rom_test!(
        hdma_21ff_glitch_matches_mesen2,
        "hdma-21ff-glitch.sfc",
        0x3B89_56D6
    );

    // Real hardware reliably shows a sprite glitch here (`ldx.w #$0f80 ;
    // stx.w $20ff`, per the NESdev source). Not modeled by any checked
    // emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_d7_glitch_test_matches_mesen2,
        "inidisp_d7_glitch_test.sfc",
        0x3B89_56D6
    );

    // Real hardware reliably shows a brightness glitch here (`lda.b #$0f ;
    // sta.w $2100` with $21 left on the data bus, per the NESdev source).
    // Not modeled by any checked emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f_matches_mesen2,
        "inidisp_hammer_0f.sfc",
        0x3B89_56D6
    );

    // Real hardware reliably shows a brightness glitch here (`ldx.w #$0f00 ;
    // stx.w $20ff`, per the NESdev source). Not modeled by any checked
    // emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f00_matches_mesen2,
        "inidisp_hammer_0f00.sfc",
        0x3B89_56D6
    );

    // Confirmed by the source to never glitch on real hardware -- this
    // golden genuinely is hardware-accurate, not just Mesen2-matching.
    // See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f0f_matches_mesen2,
        "inidisp_hammer_0f0f.sfc",
        0x3B89_56D6
    );

    // Real hardware reliably shows the "inverse" glitch here (briefly
    // enabling the display for about a dot while in force-blank), per the
    // NESdev source. Not modeled by any checked emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f8f_matches_mesen2,
        "inidisp_hammer_0f8f.sfc",
        0x3B89_56D6
    );

    // Same inverse glitch as inidisp_hammer_0f8f.sfc at a faster hammer
    // rate; real hardware still glitches per the source photos. Not
    // modeled by any checked emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f8f_fast_matches_mesen2,
        "inidisp_hammer_0f8f_fast.sfc",
        0x3B89_56D6
    );

    // Real hardware reliably shows a sprite glitch here (`lda.b #$0f ;
    // sta.l $802100` with $80 left on the data bus, per the NESdev source).
    // Not modeled by any checked emulator. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_0f_long_matches_mesen2,
        "inidisp_hammer_0f_long.sfc",
        0x3B89_56D6
    );

    // The only exact byte-for-byte match against Mesen2 (both stay in
    // forced-blank almost the entire time, alternating INIDISP $8F/$0F).
    // The source's reference photos show real hardware still glitches here
    // too (not confirmed "does not glitch" like 0f0f/21ff-glitch above), so
    // -- like the others in this file -- this is a shared-limitation match,
    // not a hardware-accuracy claim. See #2949.
    undisbeliever_rom_test!(
        inidisp_hammer_8f0f_matches_mesen2,
        "inidisp_hammer_8f0f.sfc",
        0x6E8D_8520
    );

    // Fixed by the per-scanline INIDISP latch (was previously left un-automated
    // under #2944): the top of the frame is force-blanked (black), then the
    // display is enabled partway down, matching Mesen2 exactly.
    undisbeliever_rom_test!(
        inidisp_enable_display_mid_frame_matches_mesen2,
        "inidisp_enable_display_mid_frame.sfc",
        0xD3AE_551F
    );

    // Fixed by mid-scanline HDMA activation (#2943): ROMs write to HDMAEN mid-scanline
    // (via H-IRQ at dots 220-232) to enable HDMA channels. Channels activate on the
    // next scanline, producing horizontal striped patterns correctly.
    //
    // Pixel-exact vs Mesen2 since #3020's HDMA write scheduling (which resolved the
    // previously-documented #2967 rightmost-column delta for this vector).
    undisbeliever_rom_test!(
        hdmaen_latch_test_matches_mesen2,
        "hdmaen_latch_test.sfc",
        0x22C3_0946
    );

    // Same test as above with different timing. NOT pixel-exact: 6 whole scanlines
    // phase-flip vs Mesen2 (frame-to-frame timing drift class, #2971; write-clock
    // jitter follow-up #3042). Cross-checked NESER-current golden, re-approved
    // after #3020 (previously "one extra flickering line").
    undisbeliever_rom_test!(
        hdmaen_latch_test_2_matches_mesen2,
        "hdmaen_latch_test_2.sfc",
        0x67B7_6FBE
    );

    // Tests HDMA-driven INIDISP changes. Same known minor visual differences.
    undisbeliever_rom_test!(
        inidisp_brightness_delay_matches_mesen2,
        "inidisp_brightness_delay.sfc",
        0xA6F2_AED7
    );

    // Demonstrates forgetting to force-blank before uploading to the PPU (#2944):
    // VRAM writes outside VBlank/forced blank are dropped, OAM writes are
    // redirected into the high table, and CGRAM writes land at the renderer's
    // current palette fetch (entry 0 here -- the yellow), so the "clean" uploads
    // never land and the initial VRAM garbage fill stays on screen as the noisy
    // striped pattern. The frame-600 capture is pixel-identical to Mesen2's
    // (0.00% diff, byte-for-byte).
    undisbeliever_rom_test!(
        inidisp_forgot_to_force_blank_matches_mesen2,
        "inidisp_forgot_to_force_blank.sfc",
        0xBB04_7582
    );

    // The 10 scpu-a-dma-bug-* ROMs (issue #2945) share one harness
    // (`dma-test.inc`): an HTIME H-IRQ per visible scanline drives an MDMA
    // byte to WMDATA while HDMA writes INIDISP on the previous scanline,
    // rendering green squares with alternating dark/bright scanline
    // banding on pass; a crash traps into the break/COP handler
    // (flat half-brightness screen). They differ only in their appended
    // HDMA table. Rendering these correctly required the DMA-to-WMDATA
    // B-bus fix, the brightness formula fix (PR #2948), and the BG
    // vertical-scroll display-line fix (all #2945).
    undisbeliever_rom_test!(scpu_a_dma_bug_1, "scpu-a-dma-bug-1.sfc", 0x1E6F_71A7);
    undisbeliever_rom_test!(scpu_a_dma_bug_2, "scpu-a-dma-bug-2.sfc", 0x8FF8_6612);
    undisbeliever_rom_test!(scpu_a_dma_bug_3, "scpu-a-dma-bug-3.sfc", 0x8902_A5AE);
    undisbeliever_rom_test!(scpu_a_dma_bug_5, "scpu-a-dma-bug-5.sfc", 0x2B1E_9001);
    undisbeliever_rom_test!(scpu_a_dma_bug_ch0, "scpu-a-dma-bug-ch0.sfc", 0x8FF8_6612);
    undisbeliever_rom_test!(scpu_a_dma_bug_fix, "scpu-a-dma-bug-fix.sfc", 0x8FF8_6612);
    undisbeliever_rom_test!(scpu_a_dma_bug_fix2, "scpu-a-dma-bug-fix2.sfc", 0x8FF8_6612);
    undisbeliever_rom_test!(scpu_a_dma_bug_r2, "scpu-a-dma-bug-r2.sfc", 0x1E6F_71A7);
    undisbeliever_rom_test!(
        scpu_a_dma_bug_strange,
        "scpu-a-dma-bug-strange.sfc",
        0x1E6F_71A7
    );
    undisbeliever_rom_test!(
        scpu_a_dma_bug_two_regs,
        "scpu-a-dma-bug-two-regs.sfc",
        0x8FF8_6612
    );
}
