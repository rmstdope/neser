//! Automates the undisbeliever PPU window and INIDISP fade demo ROMs
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/`),
//! built from the vendored source mirror for issue #2880.
//!
//! Suite layout:
//!
//! - `window-mask-logic.sfc` (interactive): two HDMA window shapes feed
//!   the colour window; Right cycles the 4-bit mask-logic value (bits
//!   0-1 = OR/AND/XOR/XNOR, bit 2 = invert window 1, bit 3 = invert
//!   window 2), L / R toggle window 1 / window 2 enable. The 21 distinct
//!   visual states (16 logic x invert values with both windows on, each
//!   single-window state with and without its invert bit, and no
//!   windows) are dialled in via `rom_runner` input scripting.
//! - `window-shapes-single.sfc` (interactive): 14 hard-coded HDMA
//!   single-window shape tables. The ROM auto-advances every 120 frames
//!   until the first button press; a scripted A tap (read only by the
//!   initial any-button check, not by the Left/Right navigation loop)
//!   locks it on shape 0 without advancing, then Right taps select the
//!   shape. The A tap is scheduled twice (frames 40 and 48) so a press
//!   that lands before auto-joypad is live cannot be missed.
//! - `window-precalculated-single.sfc` / `-symmetrical.sfc`: self-running
//!   bouncing-window demos that animate every frame, sampled at frame
//!   120 (Mesen2 cross-checks need `--snes.disableFrameSkipping=true`).
//! - `inidisp_fadein_fadeout.sfc`: deterministic fade-in / hold /
//!   fade-to-force-blank image cycle (probed: brightness steps change
//!   every 4 frames from frame 12, full-brightness hold at frames
//!   68-135, black force-blank gap at 192-200, second image held at
//!   257-324, whole cycle repeats every 378 frames), sampled
//!   mid-plateau.
//!
//! Baseline results (#2878/#2879 workflow: `NESER_CAPTURE_SCREEN=1`
//! captures pixel-diffed against Mesen2 headless at identical frames,
//! `--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`, interactive schedules replayed
//! via Lua `emu.setInput`):
//!
//! **All 44 vectors are byte-for-byte (0 px) matches for fresh Mesen2
//! captures.** Only the 7 fade samples and the mask-logic "no windows"
//! state matched before #3011: every window-enabled vector rendered
//! inverted regions, most visibly on `window-precalculated-single.sfc`
//! where Mesen2 showed a white "!" on black and NESER the exact inverse.
//! Root cause was the W12SEL/W34SEL/WOBJSEL nibble decode -- NESER read
//! the enable and invert bits the wrong way round, so `%0010`
//! ("enabled, inside") and `%0011` ("enabled, outside") both came out
//! inverted, which is also why the OR/AND/XNOR states used to render one
//! identical image across all four invert combinations.
//!
//! Two methodology notes for re-approving these goldens:
//!
//! - The 14 shape vectors' scripted replay had never been validated
//!   against Mesen2 (no vector on that ROM passed before #3011), so each
//!   NESER capture was matched against ALL 14 Mesen2 shape captures to
//!   confirm the same shape index in both emulators. A misaligned index
//!   would compare two different pictures and look exactly like a
//!   rendering bug.
//! - `Mesen --testRunner` exits 0 printing nothing at all if another
//!   Mesen instance is running, so guard captures with
//!   `until ! pgrep -f "Mesen --testRunner"; do sleep 5; done`.

use super::rom_runner::{InputEvent, RunConfig, RunOracle, run_rom_with_oracle};
use crate::snes::input::SnesButton;
use std::fs;
use std::path::Path;

const WINDOW_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window";

/// First frame at which the interactive ROMs' boot screen has settled
/// and auto-joypad reads are live, with margin.
const MENU_READY_FRAME: u32 = 40;

/// Frames between successive scripted key presses (1-frame taps,
/// edge-triggered handlers; same period as the byuu test_oam suite).
const KEY_PERIOD: u32 = 8;

/// Frames to run past the last scripted event before sampling.
const SETTLE_MARGIN: u32 = 60;

/// Sample frame for the free-running bouncing-window demos.
const PRECALCULATED_SAMPLE_FRAME: u32 = 120;

fn run_window_screen_crc(
    file: &str,
    label: &str,
    script: &[InputEvent],
    sample_frame: u32,
    expected_crc: u32,
) {
    let path = Path::new(WINDOW_ROOT).join(file);
    let rom = fs::read(&path)
        .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
    let name = if label.is_empty() {
        file.to_string()
    } else {
        let stem = file.trim_end_matches(".sfc");
        format!("{stem}-{label}.sfc")
    };
    let result = run_rom_with_oracle(
        &rom,
        &name,
        "undisbeliever_ppu_window_tests",
        RunConfig::new(2_000_000_000, 0).with_input_script(script),
        RunOracle::ScreenCrc {
            frames: sample_frame,
            expected_crc,
        },
    );
    assert_eq!(
        result.screen_crc32, expected_crc,
        "{name}: rendered screen at frame {sample_frame} no longer matches \
         the Mesen2-approved golden CRC (got 0x{:08X}); if this is an \
         intentional rendering change, re-approve the golden per \
         README-SNES.md",
        result.screen_crc32
    );
}

/// One `window-mask-logic` state: how many Right taps to apply (the
/// 4-bit logic/invert value) and which windows stay enabled.
#[derive(Debug, Clone, Copy)]
struct MaskLogicCombo {
    right_taps: u8,
    win1: bool,
    win2: bool,
}

/// Builds the frame-stamped script dialling in `combo` and returns it
/// with the sample frame.
fn build_mask_logic_script(combo: MaskLogicCombo) -> (Vec<InputEvent>, u32) {
    let mut script = Vec::new();
    let mut frame = MENU_READY_FRAME;

    let mut tap = |button: SnesButton, frame: &mut u32| {
        script.push(InputEvent::button(*frame, button, true));
        script.push(InputEvent::button(*frame + 1, button, false));
        *frame += KEY_PERIOD;
    };

    if !combo.win1 {
        tap(SnesButton::L, &mut frame);
    }
    if !combo.win2 {
        tap(SnesButton::R, &mut frame);
    }
    for _ in 0..combo.right_taps {
        tap(SnesButton::Right, &mut frame);
    }

    let sample_frame = frame + SETTLE_MARGIN;
    (script, sample_frame)
}

/// Builds the script that locks `window-shapes-single` on shape 0 (two
/// A taps; only the first registers, the second is a startup guard) and
/// then advances to shape `index` with Right taps.
fn build_shape_script(index: u32) -> (Vec<InputEvent>, u32) {
    let mut script = Vec::new();
    let mut frame = MENU_READY_FRAME;

    let mut tap = |button: SnesButton, frame: &mut u32| {
        script.push(InputEvent::button(*frame, button, true));
        script.push(InputEvent::button(*frame + 1, button, false));
        *frame += KEY_PERIOD;
    };

    tap(SnesButton::A, &mut frame);
    tap(SnesButton::A, &mut frame);
    for _ in 0..index {
        tap(SnesButton::Right, &mut frame);
    }

    let sample_frame = frame + SETTLE_MARGIN;
    (script, sample_frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `window-mask-logic` state. Every one of these is a
    /// byte-for-byte (0 px) match for a fresh Mesen2 headless capture since
    /// #3011 fixed the window enable/invert decode -- before that they rendered
    /// inverted regions, and the four invert combinations of each operator
    /// collapsed onto one image because %0010 and %0011 both decoded as
    /// inverted. The 16 CRCs below being all distinct is the direct evidence
    /// that the invert bits now do something.
    macro_rules! mask_logic_test {
        ($name:ident, $label:expr, { $($field:ident : $value:expr),* $(,)? }, $crc:expr) => {
            #[test]
            fn $name() {
                let combo = MaskLogicCombo {
                    $($field: $value,)*
                    ..MaskLogicCombo { right_taps: 0, win1: true, win2: true }
                };
                let (script, sample_frame) = build_mask_logic_script(combo);
                run_window_screen_crc(
                    "window-mask-logic.sfc",
                    $label,
                    &script,
                    sample_frame,
                    $crc,
                );
            }
        };
    }

    // All 16 logic x invert values with both windows enabled. Value
    // layout: bits 0-1 = OR/AND/XOR/XNOR, bit 2 = invert window 1,
    // bit 3 = invert window 2.
    mask_logic_test!(logic_or, "or", { right_taps: 0 }, 0x782A_2854);
    mask_logic_test!(logic_and, "and", { right_taps: 1 }, 0x03EC_5846);
    mask_logic_test!(logic_xor, "xor", { right_taps: 2 }, 0x404A_C130);
    mask_logic_test!(logic_xnor, "xnor", { right_taps: 3 }, 0xC1B0_5B1A);
    mask_logic_test!(logic_or_inv1, "or-i1", { right_taps: 4 }, 0x24EA_F615);
    mask_logic_test!(logic_and_inv1, "and-i1", { right_taps: 5 }, 0x183C_9195);
    mask_logic_test!(logic_xor_inv1, "xor-i1", { right_taps: 6 }, 0xD92B_F4D3);
    mask_logic_test!(logic_xnor_inv1, "xnor-i1", { right_taps: 7 }, 0xC9D3_635B);
    mask_logic_test!(logic_or_inv2, "or-i2", { right_taps: 8 }, 0x048B_7332);
    mask_logic_test!(logic_and_inv2, "and-i2", { right_taps: 9 }, 0xBC07_A099);
    mask_logic_test!(logic_xor_inv2, "xor-i2", { right_taps: 10 }, 0x4082_8484);
    mask_logic_test!(logic_xnor_inv2, "xnor-i2", { right_taps: 11 }, 0xD420_A727);
    mask_logic_test!(logic_or_inv12, "or-i12", { right_taps: 12 }, 0x79D2_C6EE);
    mask_logic_test!(logic_and_inv12, "and-i12", { right_taps: 13 }, 0x146D_B980);
    mask_logic_test!(logic_xor_inv12, "xor-i12", { right_taps: 14 }, 0x5DB9_054C);
    mask_logic_test!(logic_xnor_inv12, "xnor-i12", { right_taps: 15 }, 0x5819_2B4D);

    // Single-window states (the logic operator is irrelevant with one
    // window, so only the matching invert bit is exercised).
    mask_logic_test!(win1_only, "w1", { win2: false }, 0xCCCB_A823);
    mask_logic_test!(win1_only_inv, "w1-i1", { right_taps: 4, win2: false }, 0x3B38_A29B);
    mask_logic_test!(win2_only, "w2", { win1: false }, 0x11BB_448E);
    mask_logic_test!(win2_only_inv, "w2-i2", { right_taps: 8, win1: false }, 0x44E8_C83A);

    /// With both windows disabled no masking applies. This was the only
    /// window-enabled-ROM vector that matched Mesen2 before #3011, which is
    /// what made the replay methodology trustworthy enough to build on.
    #[test]
    fn no_windows() {
        let combo = MaskLogicCombo {
            right_taps: 0,
            win1: false,
            win2: false,
        };
        let (script, sample_frame) = build_mask_logic_script(combo);
        run_window_screen_crc(
            "window-mask-logic.sfc",
            "none",
            &script,
            sample_frame,
            0x9A4B_2DF2,
        );
    }

    macro_rules! shape_test {
        ($name:ident, $label:expr, $index:expr, $crc:expr) => {
            #[test]
            fn $name() {
                let (script, sample_frame) = build_shape_script($index);
                run_window_screen_crc(
                    "window-shapes-single.sfc",
                    $label,
                    &script,
                    sample_frame,
                    $crc,
                );
            }
        };
    }

    // The 14 hard-coded HDMA shape tables, in ROM order; every one is a 0-px
    // match for a fresh Mesen2 capture since #3011. Before baselining these,
    // each NESER capture was checked against ALL 14 Mesen2 shape captures to
    // confirm its best match was the SAME index -- the scripted button replay
    // had never been validated on this ROM, and a misaligned shape index would
    // otherwise be indistinguishable from a rendering bug.
    shape_test!(shape_left_gt_right, "s00-left-gt-right", 0, 0x0061_921F);
    shape_test!(shape_rectangle, "s01-rectangle", 1, 0x6CF5_2227);
    shape_test!(shape_tall_rectangle, "s02-tall-rectangle", 2, 0x0D5D_F371);
    shape_test!(shape_trapezium_0, "s03-trapezium-0", 3, 0xAADE_376C);
    shape_test!(shape_trapezium_1, "s04-trapezium-1", 4, 0xDD42_50BE);
    shape_test!(shape_trapezium_2, "s05-trapezium-2", 5, 0xE06E_467C);
    shape_test!(shape_triangle_0, "s06-triangle-0", 6, 0xBC9A_4746);
    shape_test!(shape_triangle_1, "s07-triangle-1", 7, 0x07AB_2A54);
    shape_test!(shape_triangle_2, "s08-triangle-2", 8, 0x6975_8844);
    shape_test!(shape_triangle_3, "s09-triangle-3", 9, 0xFD9F_54D5);
    shape_test!(shape_triangle_4, "s10-triangle-4", 10, 0xF070_2B5C);
    shape_test!(shape_octagon, "s11-octagon", 11, 0x26E6_8A0E);
    shape_test!(shape_multiple, "s12-multiple", 12, 0x7974_BABB);
    shape_test!(shape_circle, "s13-circle", 13, 0xCB1A_007F);

    // Free-running bouncing-window demos, frame-exact by construction. These
    // were the clearest #3011 evidence: Mesen2 rendered a white "!" window
    // shape on black and NESER the exact inverse. Both are now 0-px matches.
    #[test]
    fn precalculated_single() {
        run_window_screen_crc(
            "window-precalculated-single.sfc",
            "",
            &[],
            PRECALCULATED_SAMPLE_FRAME,
            0x5148_A6F9,
        );
    }

    #[test]
    fn precalculated_symmetrical() {
        run_window_screen_crc(
            "window-precalculated-symmetrical.sfc",
            "",
            &[],
            PRECALCULATED_SAMPLE_FRAME,
            0xF279_4B0E,
        );
    }

    macro_rules! fade_test {
        ($name:ident, $label:expr, $frame:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_window_screen_crc("inidisp_fadein_fadeout.sfc", $label, &[], $frame, $crc);
            }
        };
    }

    // All sample frames sit mid-plateau (per-frame CRC probe: each
    // brightness step lasts 4 frames; plateau boundaries listed in the
    // module docs). All 7 match Mesen2 pixel-exactly.
    fade_test!(fade_in_low, "fade-in-low", 30, 0x5108_631A);
    fade_test!(fade_in_mid, "fade-in-mid", 46, 0xC69C_9D63);
    fade_test!(fade_in_high, "fade-in-high", 62, 0xC9A9_D3B1);
    fade_test!(hold_full, "hold-full", 100, 0xCAEF_FE8C);
    fade_test!(fade_out_mid, "fade-out-mid", 162, 0xECD4_AA00);
    fade_test!(forceblank_gap, "forceblank-gap", 196, 0x6E8D_8520);
    fade_test!(second_image_full, "second-image-full", 300, 0xFD19_0A1D);
}
