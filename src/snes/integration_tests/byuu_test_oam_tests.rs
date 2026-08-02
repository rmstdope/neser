//! Automates byuu's interactive `test_oam.smc` OAM size test (vendored under
//! `roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/test_oam/`)
//! via `rom_runner` input scripting (issue #2879).
//!
//! The ROM shows a menu of eight counters (Base = OBSEL $2101 bits 5-7,
//! Size = the OAM high-table size bit, Char, VFlip, HFlip, $2105 BG mode,
//! raw $2133, unused) plus one sprite at (16, 39) whose tiles come from the
//! menu font, so each rendered tile displays its own tile number. Controls,
//! polled once per frame at level (no edge detection, so scripted presses
//! last exactly one frame; the menu skips polls while redrawing, hence the
//! 8-frame key period): Up/Down select a counter, Right/A/X add 1/10/100,
//! Select resets to min, Start applies the counters to the hardware
//! registers (after a `seek_frame` dot-alignment spin on $213F/$213C) and
//! redraws the result readouts (OPHCT/OPVCT/STA78 and R0-R7).
//!
//! Every golden below was approved via the #2878/#2879 baseline workflow:
//! the boot menu settles at frame 12 (probed to 1200), each combo's screen
//! is sampled 90 frames after the Start press, and every capture was
//! replayed in Mesen2 headless (same frame-stamped input schedule injected
//! through `emu.setInput` during `inputPolled`; `--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching`) and pixel-diffed:
//!
//! - The menu screen, all 16 OBSEL base x size-bit combos, all flip combos
//!   and all character-number variants match Mesen2 exactly at shift (0,0)
//!   and carry approved goldens.
//! - `setini1_*` (screen interlace, $2133=1): NESER now emits 512×448 by
//!   column-doubling each pixel (matching Mesen2's convention); both combos
//!   approved against Mesen2 headless replay at 0-pixel diff (#3001).
//! - `setini4_*` (239-line overscan, $2133=4): NESER now clips the 239-line
//!   frame to the Mesen2-compatible 224-line window (rows 7..231,
//!   Rust-exclusive); both combos approved against Mesen2 at 0-pixel diff
//!   (#3001).
//! - `setini2_*` (OBJ interlace, $2133=2): the sprite renders half-height
//!   with field-interleaved lines since issue #3000; both combos match
//!   Mesen2 exactly at shift (0,0) (re-approved via the same replay
//!   workflow, 0-pixel diffs) and carry approved goldens.

use super::rom_runner::{InputEvent, RunConfig, RunOracle, run_rom_with_oracle};
use crate::snes::input::SnesButton;
use std::fs;
use std::path::Path;

const TEST_OAM_ROM: &str =
    "roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/test_oam/test_oam.smc";

/// First frame at which the boot menu has fully settled (probed: stable
/// from frame 12 through 1200), with margin.
const MENU_READY_FRAME: u32 = 40;

/// Frames between successive scripted key presses. Each press is held for
/// exactly one frame: the menu acts on button level once per frame, so a
/// longer hold would repeat the action, while presses spaced more tightly
/// than the menu's redraw work get skipped (verified empirically: a
/// 4-frame period drops presses, 8 frames is reliable).
const KEY_PERIOD: u32 = 8;

/// Frames to run past the last scripted event before sampling the screen.
const SETTLE_MARGIN: u32 = 90;

/// One `test_oam` menu configuration: the counter values to dial in before
/// pressing Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OamCombo {
    base: u8,
    size: u8,
    chr: u8,
    vflip: u8,
    hflip: u8,
    mode: u8,
    setini: u8,
}

impl OamCombo {
    const fn default_menu() -> Self {
        Self {
            base: 0,
            size: 0,
            chr: 0,
            vflip: 0,
            hflip: 0,
            mode: 0,
            setini: 0,
        }
    }
}

/// Builds the frame-stamped input script that dials `combo` into the menu
/// and presses Start, and returns the script together with the frame at
/// which the resulting screen should be sampled.
fn build_combo_script(combo: OamCombo) -> (Vec<InputEvent>, u32) {
    let mut script = Vec::new();
    let mut frame = MENU_READY_FRAME;

    let mut tap = |button: SnesButton, frame: &mut u32| {
        script.push(InputEvent::button(*frame, button, true));
        script.push(InputEvent::button(*frame + 1, button, false));
        *frame += KEY_PERIOD;
    };

    // Counter values in menu order; the cursor starts on counter 0 and a
    // Down press moves to the next one.
    let values = [
        combo.base,
        combo.size,
        combo.chr,
        combo.vflip,
        combo.hflip,
        combo.mode,
        combo.setini,
    ];
    for (index, &value) in values.iter().enumerate() {
        for _ in 0..(value / 100) {
            tap(SnesButton::X, &mut frame);
        }
        for _ in 0..(value % 100 / 10) {
            tap(SnesButton::A, &mut frame);
        }
        for _ in 0..(value % 10) {
            tap(SnesButton::Right, &mut frame);
        }
        if index < values.len() - 1 {
            tap(SnesButton::Down, &mut frame);
        }
    }
    tap(SnesButton::Start, &mut frame);

    let sample_frame = frame + SETTLE_MARGIN;
    (script, sample_frame)
}

fn assert_test_oam_screen_crc(
    label: &str,
    script: &[InputEvent],
    sample_frame: u32,
    expected_crc: u32,
) {
    let rom = fs::read(Path::new(TEST_OAM_ROM))
        .unwrap_or_else(|err| panic!("failed to read ROM {TEST_OAM_ROM}: {err}"));
    let result = run_rom_with_oracle(
        &rom,
        &format!("test_oam-{label}.smc"),
        "byuu_test_oam_tests",
        RunConfig::new(2_000_000_000, 0).with_input_script(script),
        RunOracle::ScreenCrc {
            frames: sample_frame,
            expected_crc,
        },
    );
    assert_eq!(
        result.screen_crc32, expected_crc,
        "test_oam combo {label}: rendered screen at frame {sample_frame} no \
         longer matches the Mesen2-approved golden CRC (got 0x{:08X}); if \
         this is an intentional rendering change, re-approve the golden per \
         README-SNES.md",
        result.screen_crc32
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The untouched boot menu (no input), sampled well past its settle
    /// frame.
    #[test]
    fn menu_default() {
        assert_test_oam_screen_crc("menu-default", &[], 100, 0x46D4_DE32);
    }

    macro_rules! test_oam_combo {
        ($name:ident, $label:expr, { $($field:ident : $value:expr),* $(,)? }, $crc:expr) => {
            #[test]
            fn $name() {
                let combo = OamCombo {
                    $($field: $value,)*
                    ..OamCombo::default_menu()
                };
                let (script, sample_frame) = build_combo_script(combo);
                assert_test_oam_screen_crc($label, &script, sample_frame, $crc);
            }
        };
    }

    // All eight OBSEL base values (sprite size pairs) at both OAM size
    // bits; the sprite's tile glyphs display which tiles were fetched.
    test_oam_combo!(base0_small, "b0-s0", { base: 0, size: 0 }, 0xD90B_8CD1);
    test_oam_combo!(base0_large, "b0-s1", { base: 0, size: 1 }, 0xFD33_AAE9);
    test_oam_combo!(base1_small, "b1-s0", { base: 1, size: 0 }, 0x16CC_3C21);
    test_oam_combo!(base1_large, "b1-s1", { base: 1, size: 1 }, 0xD769_53FD);
    test_oam_combo!(base2_small, "b2-s0", { base: 2, size: 0 }, 0x5437_C894);
    test_oam_combo!(base2_large, "b2-s1", { base: 2, size: 1 }, 0xEA02_9269);
    test_oam_combo!(base3_small, "b3-s0", { base: 3, size: 0 }, 0x5284_143E);
    test_oam_combo!(base3_large, "b3-s1", { base: 3, size: 1 }, 0xD55A_A77A);
    test_oam_combo!(base4_small, "b4-s0", { base: 4, size: 0 }, 0x1D1D_4593);
    test_oam_combo!(base4_large, "b4-s1", { base: 4, size: 1 }, 0xE553_C3F6);
    test_oam_combo!(base5_small, "b5-s0", { base: 5, size: 0 }, 0xF532_5C9A);
    test_oam_combo!(base5_large, "b5-s1", { base: 5, size: 1 }, 0xE8E1_931B);
    test_oam_combo!(base6_small, "b6-s0", { base: 6, size: 0 }, 0x05E4_A3BC);
    test_oam_combo!(base6_large, "b6-s1", { base: 6, size: 1 }, 0x3922_7523);
    test_oam_combo!(base7_small, "b7-s0", { base: 7, size: 0 }, 0x2718_7AD0);
    test_oam_combo!(base7_large, "b7-s1", { base: 7, size: 1 }, 0xB7D0_29DE);

    // OAM attribute flips on the 32x32 sprite (base 3, large).
    test_oam_combo!(
        base3_large_hflip,
        "b3-s1-v0-h1",
        { base: 3, size: 1, hflip: 1 },
        0xAEC7_039E
    );
    test_oam_combo!(
        base3_large_vflip,
        "b3-s1-v1-h0",
        { base: 3, size: 1, vflip: 1 },
        0xCEBF_640B
    );
    test_oam_combo!(
        base3_large_vhflip,
        "b3-s1-v1-h1",
        { base: 3, size: 1, vflip: 1, hflip: 1 },
        0xEA6D_C5C0
    );

    // Character-number selection at both sizes (tile fetch offsets,
    // including the name-table wrap at 255).
    test_oam_combo!(char1_small, "c1-s0", { chr: 1, size: 0 }, 0x8DB8_D050);
    test_oam_combo!(char1_large, "c1-s1", { chr: 1, size: 1 }, 0xC69E_8FCC);
    test_oam_combo!(char16_small, "c16-s0", { chr: 16, size: 0 }, 0x3AE2_6077);
    test_oam_combo!(char16_large, "c16-s1", { chr: 16, size: 1 }, 0xF20F_422F);
    test_oam_combo!(char32_small, "c32-s0", { chr: 32, size: 0 }, 0x0E69_A388);
    test_oam_combo!(char32_large, "c32-s1", { chr: 32, size: 1 }, 0xDD3C_A9EB);
    test_oam_combo!(char255_small, "c255-s0", { chr: 255, size: 0 }, 0x90EB_9069);
    test_oam_combo!(char255_large, "c255-s1", { chr: 255, size: 1 }, 0x37C6_490A);

    // $2133 (SETINI) display modes: screen interlace and 239-line overscan.
    // NESER now emits Mesen2-compatible dimensions for both modes (#3001):
    // screen interlace → 512×448 (column-doubled), overscan → 256×224
    // (cropped to rows 7..231, Rust-exclusive).  All four combos carry
    // Mesen2-approved goldens verified at 0-pixel diff via testRunner replay.
    test_oam_combo!(setini1_small, "i1-s0", { setini: 1, size: 0 }, 0xF3AB_FC7C);
    test_oam_combo!(setini1_large, "i1-s1", { setini: 1, size: 1 }, 0xD678_64B6);
    test_oam_combo!(setini4_small, "i4-s0", { setini: 4, size: 0 }, 0xFD9E_A997);
    test_oam_combo!(setini4_large, "i4-s1", { setini: 4, size: 1 }, 0x2476_20AA);

    // OBJ interlace ($2133=2, issue #3000): the sprite renders half-height
    // with field-interleaved lines. Goldens re-approved against Mesen2
    // headless replay (0-pixel diffs at shift (0,0)).
    test_oam_combo!(setini2_small, "i2-s0", { setini: 2, size: 0 }, 0x7BE9_8B23);
    test_oam_combo!(setini2_large, "i2-s1", { setini: 2, size: 1 }, 0xFE8D_F854);
}
