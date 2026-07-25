//! Automates the NESER-authored PPU colour-math, window and brightness
//! test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/neser-colormath-tests/`),
//! written for issue #2880 against undisbeliever's bass framework
//! (sources live in the same directory).
//!
//! All ROMs render one static Mode 1 quadrant scene: BG1 (main screen)
//! draws 8 vertical colour bars over the left 192px, BG2 (sub screen)
//! draws 8 horizontal bars over the top 192px, giving 64 (main, sub)
//! colour-math crossings plus backdrop-main, transparent-sub
//! (fixed-colour fallback) and backdrop x transparent regions. Bar
//! colours discriminate clamp-at-31 (16+16), floor-at-0 (8-16) and
//! halve-after-add ((15+1)/2 = 8). The scene settles at frame 8
//! (per-frame CRC probe to 700) and is sampled at frame 66 like the
//! neser-obj suite.
//!
//! Baseline results (#2878/#2879 workflow: `NESER_CAPTURE_SCREEN=1`
//! captures pixel-diffed against Mesen2 headless at identical frames):
//!
//! - `cm-add-clamp`, `cm-sub-floor`, `cm-fixed-add` and
//!   `cm-fixed-sub-half` match Mesen2 pixel-exactly (approved goldens),
//!   covering add/clamp, subtract/floor and both fixed-colour math
//!   paths.
//! - `cm-add-half`, `cm-sub-half`, `cm-obj-palettes` and
//!   `cm-sub-backdrop` diverge ONLY in the transparent-sub fallback
//!   regions (pixel-count-exact: 7424 = fallback area minus
//!   halve-invariant black bars; 38912 = the enlarged fallback area of
//!   the centre-only scene): NESER fails to suppress halving for the
//!   fixed-colour fallback. `#[ignore]`d pending issue #3012. The 64
//!   sub-opaque crossings and the OBJ palette 4-7 rule match Mesen2.
//! - `cm-window-clip` and `win-layer-masks` diverge (NESER window
//!   masking regions are inverted). `#[ignore]`d pending issue #3011.
//! - `brightness-steps.sfc` steps INIDISP through all 16 brightness
//!   levels from the NMI frame counter (probed plateaus: level N shown
//!   for frames 64N+8 through 64N+71, N >= 1; level 0 from frame 1;
//!   full-brightness hold to frame 1159; force-blank from frame 1160),
//!   sampled mid-plateau at frame 64N+32. All 17 samples match Mesen2
//!   pixel-exactly (approved goldens). Level 0 and force-blank both
//!   render all-black, so those two goldens share a CRC by design.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const COLORMATH_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-colormath-tests";

/// Static scenes settle at frame 8 and are sampled with the same
/// margin as the neser-obj suite.
const SAMPLE_FRAME: u32 = 66;

fn run_colormath_screen_crc(file: &str, label: &str, sample_frame: u32, expected_crc: u32) {
    let path = Path::new(COLORMATH_ROOT).join(file);
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
        "neser_color_math_tests",
        RunConfig::new(2_000_000_000, 0),
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! colormath_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_colormath_screen_crc($file, "", SAMPLE_FRAME, $crc);
            }
        };
    }

    colormath_test!(add_clamp, "cm-add-clamp.sfc", 0x419D_2073);
    colormath_test!(sub_floor, "cm-sub-floor.sfc", 0x1ED3_2531);
    colormath_test!(fixed_add, "cm-fixed-add.sfc", 0x0B4F_9F53);
    colormath_test!(fixed_sub_half, "cm-fixed-sub-half.sfc", 0xEFFE_5313);

    /// NESER halves the fixed-colour fallback for transparent
    /// sub-screen pixels (hardware and Mesen2 suppress halving there),
    /// so these record NESER's current CRCs until #3012 is fixed. The
    /// sub-opaque crossings (and the OBJ palette 4-7 rule in
    /// `cm-obj-palettes`) already match Mesen2.
    macro_rules! colormath_test_ignored_3012 {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            #[ignore = "NESER does not suppress halving for the transparent-sub fixed-colour fallback; pending #3012"]
            fn $name() {
                run_colormath_screen_crc($file, "", SAMPLE_FRAME, $crc);
            }
        };
    }

    colormath_test_ignored_3012!(add_half, "cm-add-half.sfc", 0x0929_94A1);
    colormath_test_ignored_3012!(sub_half, "cm-sub-half.sfc", 0x685C_2396);
    colormath_test_ignored_3012!(obj_palettes, "cm-obj-palettes.sfc", 0xF5EE_CF42);
    colormath_test_ignored_3012!(sub_backdrop, "cm-sub-backdrop.sfc", 0xCAFF_1B44);

    /// NESER renders inverted window masking regions, so these record
    /// NESER's current CRCs until #3011 is fixed.
    macro_rules! colormath_test_ignored_3011 {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            #[ignore = "NESER window masking regions are inverted vs Mesen2; pending #3011"]
            fn $name() {
                run_colormath_screen_crc($file, "", SAMPLE_FRAME, $crc);
            }
        };
    }

    colormath_test_ignored_3011!(window_clip, "cm-window-clip.sfc", 0xDD03_4E93);
    colormath_test_ignored_3011!(layer_masks, "win-layer-masks.sfc", 0xF1B3_F2CD);

    macro_rules! brightness_test {
        ($name:ident, $label:expr, $sample_frame:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_colormath_screen_crc("brightness-steps.sfc", $label, $sample_frame, $crc);
            }
        };
    }

    // One mid-plateau sample per INIDISP brightness level (level N is
    // shown for frames 64N+8 through 64N+71; frame 64N+32 has at least
    // 24 frames of margin on both sides), plus the force-blank cut.
    brightness_test!(brightness_level_0, "l00", 32, 0x6E8D_8520);
    brightness_test!(brightness_level_1, "l01", 96, 0x0755_F8F8);
    brightness_test!(brightness_level_2, "l02", 64 * 2 + 32, 0xE0BF_8749);
    brightness_test!(brightness_level_3, "l03", 64 * 3 + 32, 0x8C14_E8EE);
    brightness_test!(brightness_level_4, "l04", 64 * 4 + 32, 0x808D_D75B);
    brightness_test!(brightness_level_5, "l05", 64 * 5 + 32, 0x0A3C_27FC);
    brightness_test!(brightness_level_6, "l06", 64 * 6 + 32, 0xA51D_1A04);
    brightness_test!(brightness_level_7, "l07", 64 * 7 + 32, 0xB707_E4FC);
    brightness_test!(brightness_level_8, "l08", 64 * 8 + 32, 0x7722_8A91);
    brightness_test!(brightness_level_9, "l09", 64 * 9 + 32, 0x9930_6105);
    brightness_test!(brightness_level_10, "l10", 64 * 10 + 32, 0x9CC8_1CA6);
    brightness_test!(brightness_level_11, "l11", 64 * 11 + 32, 0x9B79_2FD3);
    brightness_test!(brightness_level_12, "l12", 64 * 12 + 32, 0x383C_3B27);
    brightness_test!(brightness_level_13, "l13", 64 * 13 + 32, 0x8188_1AE6);
    brightness_test!(brightness_level_14, "l14", 64 * 14 + 32, 0x9AB3_879C);
    brightness_test!(brightness_level_15, "l15", 64 * 15 + 32, 0x5A5B_F43E);
    brightness_test!(brightness_force_blank, "fblank", 1184, 0x6E8D_8520);
}
