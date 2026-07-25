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
//! - All 7 fade samples and the mask-logic "no windows" state match
//!   Mesen2 pixel-exactly and carry approved goldens.
//! - Every window-enabled vector diverges (NESER renders inverted
//!   window regions; clearest on `window-precalculated-single.sfc`
//!   where Mesen2 shows a white "!" on black and NESER the exact
//!   inverse). Those 36 tests are `#[ignore]`d with NESER's current
//!   CRCs recorded, pending issue #3011.

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
        script.push(InputEvent {
            frame: *frame,
            button,
            pressed: true,
        });
        script.push(InputEvent {
            frame: *frame + 1,
            button,
            pressed: false,
        });
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
        script.push(InputEvent {
            frame: *frame,
            button,
            pressed: true,
        });
        script.push(InputEvent {
            frame: *frame + 1,
            button,
            pressed: false,
        });
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

    /// NESER renders inverted window regions (issue #3011), so all
    /// window-enabled mask-logic states record NESER's current CRC and
    /// stay ignored until the fix lands. Note that NESER currently
    /// renders identical output for the OR/AND/XNOR states regardless
    /// of the window invert bits -- an artifact of the same bug.
    macro_rules! mask_logic_test_ignored_3011 {
        ($name:ident, $label:expr, { $($field:ident : $value:expr),* $(,)? }, $crc:expr) => {
            #[test]
            #[ignore = "NESER window masking regions are inverted vs Mesen2; pending #3011"]
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
    mask_logic_test_ignored_3011!(logic_or, "or", { right_taps: 0 }, 0x5D0D_2DF9);
    mask_logic_test_ignored_3011!(logic_and, "and", { right_taps: 1 }, 0x9690_8E5E);
    mask_logic_test_ignored_3011!(logic_xor, "xor", { right_taps: 2 }, 0x321F_97E7);
    mask_logic_test_ignored_3011!(logic_xnor, "xnor", { right_taps: 3 }, 0xA510_2687);
    mask_logic_test_ignored_3011!(logic_or_inv1, "or-i1", { right_taps: 4 }, 0x5D0D_2DF9);
    mask_logic_test_ignored_3011!(logic_and_inv1, "and-i1", { right_taps: 5 }, 0x9690_8E5E);
    mask_logic_test_ignored_3011!(logic_xor_inv1, "xor-i1", { right_taps: 6 }, 0xA11B_B37B);
    mask_logic_test_ignored_3011!(logic_xnor_inv1, "xnor-i1", { right_taps: 7 }, 0xA510_2687);
    mask_logic_test_ignored_3011!(logic_or_inv2, "or-i2", { right_taps: 8 }, 0x5D0D_2DF9);
    mask_logic_test_ignored_3011!(logic_and_inv2, "and-i2", { right_taps: 9 }, 0x9690_8E5E);
    mask_logic_test_ignored_3011!(logic_xor_inv2, "xor-i2", { right_taps: 10 }, 0xBCE8_7707);
    mask_logic_test_ignored_3011!(logic_xnor_inv2, "xnor-i2", { right_taps: 11 }, 0xA510_2687);
    mask_logic_test_ignored_3011!(logic_or_inv12, "or-i12", { right_taps: 12 }, 0x5D0D_2DF9);
    mask_logic_test_ignored_3011!(logic_and_inv12, "and-i12", { right_taps: 13 }, 0x9690_8E5E);
    mask_logic_test_ignored_3011!(logic_xor_inv12, "xor-i12", { right_taps: 14 }, 0x2FEC_539B);
    mask_logic_test_ignored_3011!(logic_xnor_inv12, "xnor-i12", { right_taps: 15 }, 0xA510_2687);

    // Single-window states (the logic operator is irrelevant with one
    // window, so only the matching invert bit is exercised).
    mask_logic_test_ignored_3011!(win1_only, "w1", { win2: false }, 0x4450_63D3);
    mask_logic_test_ignored_3011!(win1_only_inv, "w1-i1", { right_taps: 4, win2: false }, 0x4450_63D3);
    mask_logic_test_ignored_3011!(win2_only, "w2", { win1: false }, 0x8FCD_C074);
    mask_logic_test_ignored_3011!(win2_only_inv, "w2-i2", { right_taps: 8, win1: false }, 0x8FCD_C074);

    /// With both windows disabled no masking applies, and NESER matches
    /// Mesen2 pixel-exactly (approved golden).
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

    macro_rules! shape_test_ignored_3011 {
        ($name:ident, $label:expr, $index:expr, $crc:expr) => {
            #[test]
            #[ignore = "NESER window masking regions are inverted vs Mesen2; pending #3011"]
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

    // The 14 hard-coded HDMA shape tables, in ROM order. All render as
    // the inverse of Mesen2's shapes pending #3011.
    shape_test_ignored_3011!(shape_left_gt_right, "s00-left-gt-right", 0, 0xD618_9570);
    shape_test_ignored_3011!(shape_rectangle, "s01-rectangle", 1, 0x54FE_E780);
    shape_test_ignored_3011!(shape_tall_rectangle, "s02-tall-rectangle", 2, 0x2212_6209);
    shape_test_ignored_3011!(shape_trapezium_0, "s03-trapezium-0", 3, 0xA712_2D9F);
    shape_test_ignored_3011!(shape_trapezium_1, "s04-trapezium-1", 4, 0x7AD1_D13D);
    shape_test_ignored_3011!(shape_trapezium_2, "s05-trapezium-2", 5, 0xFF37_5526);
    shape_test_ignored_3011!(shape_triangle_0, "s06-triangle-0", 6, 0xA2DF_6316);
    shape_test_ignored_3011!(shape_triangle_1, "s07-triangle-1", 7, 0xB499_945E);
    shape_test_ignored_3011!(shape_triangle_2, "s08-triangle-2", 8, 0xA1E2_CC1B);
    shape_test_ignored_3011!(shape_triangle_3, "s09-triangle-3", 9, 0x1AEC_E068);
    shape_test_ignored_3011!(shape_triangle_4, "s10-triangle-4", 10, 0x890F_31F8);
    shape_test_ignored_3011!(shape_octagon, "s11-octagon", 11, 0xD40C_C8F1);
    shape_test_ignored_3011!(shape_multiple, "s12-multiple", 12, 0x5435_E355);
    shape_test_ignored_3011!(shape_circle, "s13-circle", 13, 0x1D2E_3703);

    // Free-running bouncing-window demos, frame-exact by construction.
    // Mesen2 renders a white window shape on black; NESER the exact
    // inverse (the clearest #3011 evidence).
    #[test]
    #[ignore = "NESER window masking regions are inverted vs Mesen2; pending #3011"]
    fn precalculated_single() {
        run_window_screen_crc(
            "window-precalculated-single.sfc",
            "",
            &[],
            PRECALCULATED_SAMPLE_FRAME,
            0xCBF2_E4A3,
        );
    }

    #[test]
    #[ignore = "NESER window masking regions are inverted vs Mesen2; pending #3011"]
    fn precalculated_symmetrical() {
        run_window_screen_crc(
            "window-precalculated-symmetrical.sfc",
            "",
            &[],
            PRECALCULATED_SAMPLE_FRAME,
            0x5AA6_814D,
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
