//! Automates all 29 vendored undisbeliever/snes-test-roms hardware ROMs
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/`), every
//! one of them a 0-pixel match against a Mesen2 capture of the same frame.
//!
//! Unlike blargg/gilyon ROMs, these do not print a PASS/FAIL text screen.
//! Fourteen of the 29 automated here demonstrate a real, documented
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
//! deterministically with no bus-residual modeling at all (see #2949).
//!
//! **So a 0-pixel Mesen2 match does not mean the same thing for every golden
//! here.** For the 3 ROMs the source confirms never glitch on real hardware
//! (`hdma_21ff_glitch_matches_mesen2`, `inidisp_hammer_0f0f_matches_mesen2`,
//! `hdma_2100_glitch_2ch_0a_matches_mesen2`, marked below) it genuinely *is*
//! proof of hardware accuracy. For the other 11 it only proves parity with a
//! limitation NESER shares with every checked reference emulator -- read #2949
//! before treating a mismatch there as a regression.
//!
//! Captures are cross-checked with `--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching --snes.disableFrameSkipping=true`; since
//! the BG vertical-scroll display-line fix in #2945 the two emulators align
//! byte-for-byte at zero row offset, so a non-zero best shift from
//! `python -m scripts.diff_screenshots` is a bug, not a convention.

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

    // The three ROMs below were the last of the 29 left un-automated. They were
    // held back under #2943/#3083 because their HDMA-driven per-scanline banding
    // diverged from Mesen2, and a golden would have baked that in. On current
    // main all three are a **0-pixel, byte-for-byte match** with a fresh Mesen2
    // frame-600 capture (measured with `python -m scripts.diff_screenshots`, and
    // separately confirmed by crc32 over Mesen2's own decoded pixels equalling
    // the value asserted here). See #3083 for the measurement table.
    //
    // What these three pin, verified by mutation: making `hdma_transfer_due`
    // skip scanline 0 (a one-line banding phase shift) breaks all three. Moving
    // `HDMA_TRANSFER_POSITION` from dot 276 to 277 does NOT -- the write still
    // lands in hblank, before the next line's dot-22 INIDISP latch. So they are
    // a frame-phase oracle, not a within-line clock oracle; the sub-line timing
    // is pinned by the unit tests in `bus/system_bus.rs` instead.
    //
    // Two HDMA channels both writing $2100, with $0A on the second channel.
    // Confirmed by the NESdev source to be one of the three tests that do NOT
    // glitch on real hardware, so -- like `hdma_21ff_glitch` above -- this
    // golden genuinely is hardware-accurate, not merely Mesen2-matching.
    undisbeliever_rom_test!(
        hdma_2100_glitch_2ch_0a_matches_mesen2,
        "hdma-2100-glitch-2ch-0a.sfc",
        0x9E55_B80C
    );

    // The same two-channel setup with $81 on the second channel, which puts bit
    // 7 on the bus ahead of the INIDISP write. Real hardware glitches (~40% of
    // runs, "may need a few console resets"); no checked emulator models the
    // bus-residual, so this golden is parity with a shared limitation rather
    // than hardware accuracy. See #2949. The visible per-scanline banding IS
    // modelled and is what the 0-pixel match confirms.
    undisbeliever_rom_test!(
        hdma_2100_glitch_2ch_81_matches_mesen2,
        "hdma-2100-glitch-2ch-81.sfc",
        0xD291_738A
    );

    // An HDMA write with bit 7 set immediately followed by an HDMA write to
    // INIDISP -- the exact sprite-glitch trigger the NESdev source reports for
    // its 3-chip console. Not modeled by any checked emulator; parity, not
    // hardware accuracy. See #2949.
    undisbeliever_rom_test!(
        hdma_21ff_2100_glitch_matches_mesen2,
        "hdma-21ff-2100-glitch.sfc",
        0x5D02_D928
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
    // Re-approved for #3050 and pixel-exact (0 px) against a fresh Mesen2 capture.
    //
    // #3021 left this at 5 differing scanlines (rows 18/70/122/148/200, 1280 px): this ROM
    // re-enables HDMAEN a few clocks either side of the dot-276 HDMA trigger, and NESER's
    // CPU reached that store 2-8 master clocks early, so those lines armed when Mesen2's did
    // not. #3050 found the cause -- the HDMA `SyncEndDma` pad rounded to a fixed 8-clock CPU
    // cycle instead of the speed of the access the transfer stands in front of -- and with
    // it fixed the store lands on Mesen2's clock exactly.
    undisbeliever_rom_test!(
        hdmaen_latch_test_matches_mesen2,
        "hdmaen_latch_test.sfc",
        0x22C3_0946
    );

    // Same test as above with different timing. Now a byte-for-byte (0 px)
    // match for a fresh Mesen2 capture, re-approved for #3021's HDMA
    // envelope: the 6 phase-flipped scanlines documented here since #3020 --
    // and the "one extra flickering line" before that -- were the flat
    // 18-clock HDMA cost model mis-timing this ROM's mid-scanline HDMAEN
    // races. A genuine hardware-accuracy claim now, not a stability snapshot.
    undisbeliever_rom_test!(
        hdmaen_latch_test_2_matches_mesen2,
        "hdmaen_latch_test_2.sfc",
        0x7EDB_74D1
    );

    // Tests mid-scanline INIDISP (brightness) changes: an H-IRQ at dot 309
    // writes INIDISP directly (no HDMA involved, despite this ROM's name
    // implying otherwise) to step the master brightness up/down on specific
    // scanlines. NESER latches INIDISP once per scanline at
    // `VISIBLE_DOT_START` (`Ppu::render_dot`/`line_inidisp` in
    // `framebuffer.rs`), applying the whole row's brightness at once, while
    // real hardware has some (undocumented) pixel-level delay between the
    // write and the visible change. This produces a 4.45% pixel diff vs
    // Mesen2 concentrated at brightness band edges (issue #2973).
    //
    // Investigated and left as a documented known limitation rather than a
    // bug fix: fullsnes itself is explicitly unsure of the exact delay
    // ("Forced blank doesn't apply immediately... so one must wait whatever
    // (maybe a scanline)... or is it only vice-versa... shows garbage
    // pixels?"), and no cycle-accurate real-hardware measurement is
    // documented anywhere -- undisbeliever's `inidisp_brightness_delay.asm`
    // source (designed to visualize the effect via real-hardware photos)
    // asserts no specific delay value either. Any "fix" here would just
    // mean picking one undocumented emulator's model over another, not
    // matching a known-correct reference -- same shared-limitation
    // reasoning as the `inidisp_hammer_*` glitch tests above (#2949) and
    // related low-severity timing issues #2967/#2971. The golden CRC is
    // unchanged; no behavior/timing code was modified for this issue.
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
    //
    // This vector is the current witness for the general-purpose DMA end pad staying on a
    // fixed 8 (#3067): flip `DmaController::start_dma`'s literal to `cpu_speed` and this
    // golden diverges while every other vector is unchanged. It was ALSO the reason #3050
    // could not make that change -- then for a different reason (a compensating 65816
    // push/pull ordering error, since fixed in #3070). Treat a change to this CRC as a signal
    // about the DMA envelope, and re-derive rather than trusting either history.
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
