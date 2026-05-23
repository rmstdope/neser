//! Integration tests for the [mealybug tearoom test suite](https://github.com/mattcurrie/mealybug-tearoom-tests).
//!
//! Each test runs a mealybug ROM until the `LD B,B` (0x40) software breakpoint fires, then
//! compares the screen buffer CRC against a visually confirmed baseline.
//!
//! Tests are run per hardware model where reference images exist:
//! - `dmg_b`  — DMG-B model (reference: `expected/DMG-blob/`)
//! - `cgb_c`  — CGB-C model (reference: `expected/CPU CGB C/`)
//! - `cgb_d`  — CGB-D model (reference: `expected/CPU CGB D/`)
//!
//! ## Baseline capture workflow
//!
//! Set `NESER_CAPTURE_SCREEN=1` before running to save PNGs to
//! `target/mealybug-captures/` alongside the CRC printed to stdout:
//!
//! ```bash
//! NESER_CAPTURE_SCREEN=1 cargo test --no-default-features --lib \
//!     gb::integration_tests::mealybug_tests -- --include-ignored --nocapture
//! ```
//!
//! Compare each PNG to the reference image:
//!
//! ```bash
//! compare -metric AE \
//!     target/mealybug-captures/m3_bgp_change_dmg_b.png \
//!     roms/gb/automated_tests/mealybug-tearoom/expected/DMG-blob/m3_bgp_change.png \
//!     /dev/null
//! ```

use super::helpers::{load_cgb_rom_from_bytes, load_gb_rom_from_bytes, run_to_breakpoint_and_crc};
use crate::gb::model::{CgbModel, DmgModel};

const ROM_DIR: &str = "roms/gb/automated_tests/mealybug-tearoom/roms";

const CYCLE_LIMIT: u64 = 10_000_000;

/// Read a mealybug ROM file by base name and return its bytes.
fn read_rom(rom_name: &str) -> Vec<u8> {
    let path = format!("{ROM_DIR}/{rom_name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

// ============================================================================
// Test-generating macros
// ============================================================================

/// Generate an active DMG-B mealybug test with a CRC assertion.
macro_rules! mealybug_dmg_b {
    ($name:ident, $rom_base:literal, $expected_crc:expr) => {
        #[test]
        fn $name() {
            let bytes = read_rom(concat!($rom_base, ".gb"));
            let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_dmg_b"));
            assert_mealybug_crc(concat!($rom_base, "_dmg_b"), crc, $expected_crc);
        }
    };
}

/// Generate an active CGB-C mealybug test with a CRC assertion.
macro_rules! mealybug_cgb_c {
    ($name:ident, $rom_base:literal, $expected_crc:expr) => {
        #[test]
        fn $name() {
            let bytes = read_rom(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_c"));
            assert_mealybug_crc(concat!($rom_base, "_cgb_c"), crc, $expected_crc);
        }
    };
}

/// Generate an active CGB-D mealybug test with a CRC assertion.
macro_rules! mealybug_cgb_d {
    ($name:ident, $rom_base:literal, $expected_crc:expr) => {
        #[test]
        fn $name() {
            let bytes = read_rom(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_d"));
            assert_mealybug_crc(concat!($rom_base, "_cgb_d"), crc, $expected_crc);
        }
    };
}

fn assert_mealybug_crc(capture_name: &str, crc: u32, expected_crc: u32) {
    assert_eq!(
        crc, expected_crc,
        "{capture_name} CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use super::super::helpers::decoded_png_rgb_crc;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum MealybugModel {
        DmgB,
        CgbC,
        CgbD,
    }

    impl MealybugModel {
        const fn suffix(self) -> &'static str {
            match self {
                Self::DmgB => "dmg_b",
                Self::CgbC => "cgb_c",
                Self::CgbD => "cgb_d",
            }
        }

        const fn expected_dir(self) -> &'static str {
            match self {
                Self::DmgB => "DMG-blob",
                Self::CgbC => "CPU CGB C",
                Self::CgbD => "CPU CGB D",
            }
        }
    }

    #[derive(Debug)]
    struct MealybugCase {
        model: MealybugModel,
        rom_base: String,
        expected_crc: u32,
    }

    fn parse_mealybug_cases(source: &str) -> Vec<MealybugCase> {
        let specs = [
            ("mealybug_dmg_b", MealybugModel::DmgB),
            ("mealybug_cgb_c", MealybugModel::CgbC),
            ("mealybug_cgb_d", MealybugModel::CgbD),
        ];
        let mut cases = Vec::new();

        for (macro_name, model) in specs {
            let needle = format!("{macro_name}!(");
            let mut search_from = 0;

            while let Some(relative_start) = source[search_from..].find(&needle) {
                let invocation_start = search_from + relative_start;
                let args_start = invocation_start + needle.len();
                let close_paren = find_matching_paren(source, args_start - 1);
                let args = split_macro_args(&source[args_start..close_paren]);
                assert_eq!(
                    args.len(),
                    3,
                    "{macro_name} invocation should have 3 arguments: {args:?}"
                );

                cases.push(MealybugCase {
                    model,
                    rom_base: unquote(&args[1]),
                    expected_crc: parse_crc(&args[2]),
                });

                search_from = close_paren + 1;
            }
        }

        cases
    }

    fn find_matching_paren(source: &str, open_paren: usize) -> usize {
        let mut depth = 0_u32;
        let mut in_string = false;

        for (offset, ch) in source[open_paren..].char_indices() {
            match ch {
                '"' => in_string = !in_string,
                '(' if !in_string => depth += 1,
                ')' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return open_paren + offset;
                    }
                }
                _ => {}
            }
        }

        panic!("macro invocation should close its parentheses");
    }

    fn split_macro_args(args: &str) -> Vec<String> {
        let mut split = Vec::new();
        let mut current = String::new();
        let mut in_string = false;

        for ch in args.chars() {
            match ch {
                '"' => {
                    in_string = !in_string;
                    current.push(ch);
                }
                ',' if !in_string => {
                    split.push(current.trim().to_owned());
                    current.clear();
                }
                _ => current.push(ch),
            }
        }

        if !current.trim().is_empty() {
            split.push(current.trim().to_owned());
        }

        split
    }

    fn unquote(value: &str) -> String {
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or_else(|| panic!("{value} should be a string literal"))
            .to_owned()
    }

    fn parse_crc(value: &str) -> u32 {
        let hex = value
            .strip_prefix("0x")
            .unwrap_or_else(|| panic!("{value} should be a hex CRC literal"))
            .replace('_', "");
        u32::from_str_radix(&hex, 16)
            .unwrap_or_else(|err| panic!("{value} should parse as a CRC-32 literal: {err}"))
    }

    fn expected_png_path(case: &MealybugCase) -> PathBuf {
        Path::new("roms/gb/automated_tests/mealybug-tearoom/expected")
            .join(case.model.expected_dir())
            .join(format!("{}.png", case.rom_base))
    }

    fn expected_png_count(model: MealybugModel) -> usize {
        std::fs::read_dir(
            Path::new("roms/gb/automated_tests/mealybug-tearoom/expected")
                .join(model.expected_dir()),
        )
        .unwrap_or_else(|err| panic!("read expected PNG directory for {:?}: {err}", model))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .count()
    }

    #[test]
    fn expected_crc_constants_match_reference_pngs() {
        let source = include_str!("mealybug_tests.rs");
        let cases = parse_mealybug_cases(source);
        assert_eq!(
            cases.len(),
            71,
            "all scoped Mealybug tests should be audited"
        );

        for (model, expected_count, placeholder_png_count) in [
            (MealybugModel::DmgB, 24, 0),
            // 4 placeholder PNGs remain in the submodule for CGB-C/D (m3_lcdc_win_en_change_multiple_wx,
            // m3_wx_4_change, m3_wx_5_change, m3_wx_6_change). These are GIMP-created images that do not
            // represent real hardware captures; the corresponding tests are commented out. See #2598.
            (MealybugModel::CgbC, 27, 4),
            (MealybugModel::CgbD, 20, 4),
        ] {
            let case_count = cases.iter().filter(|case| case.model == model).count();
            assert_eq!(
                case_count,
                expected_count,
                "{} test count should match #2427 coverage",
                model.suffix()
            );
            assert_eq!(
                expected_png_count(model),
                expected_count + placeholder_png_count,
                "{} expected PNG count should match test coverage plus {} known placeholder PNGs (#2598)",
                model.suffix(),
                placeholder_png_count
            );
        }

        let mut paths = HashSet::new();
        let mut mismatches = Vec::new();

        for case in &cases {
            let path = expected_png_path(case);
            assert!(
                paths.insert(path.clone()),
                "expected PNG should only be audited once: {}",
                path.display()
            );

            let png_crc = decoded_png_rgb_crc(&path);
            if case.expected_crc != png_crc {
                mismatches.push(format!(
                    "{}_{}: code={:#010X}, png={:#010X}",
                    case.rom_base,
                    case.model.suffix(),
                    case.expected_crc,
                    png_crc
                ));
            }
        }

        assert!(
            mismatches.is_empty(),
            "Mealybug expected PNG CRC mismatch(es):\n{}",
            mismatches.join("\n")
        );
    }
}

// ============================================================================
// DMG-B tests (reference: expected/DMG-blob/)
// ============================================================================

mealybug_dmg_b!(test_m2_win_en_toggle_dmg_b, "m2_win_en_toggle", 0xCE29_5724);
mealybug_dmg_b!(test_m3_bgp_change_dmg_b, "m3_bgp_change", 0x2BA6_1257);
mealybug_dmg_b!(
    test_m3_bgp_change_sprites_dmg_b,
    "m3_bgp_change_sprites",
    0x7E8E_86BC
);
mealybug_dmg_b!(
    test_m3_lcdc_bg_en_change_dmg_b,
    "m3_lcdc_bg_en_change",
    0x8897_C19D
);
mealybug_dmg_b!(
    test_m3_lcdc_bg_map_change_dmg_b,
    "m3_lcdc_bg_map_change",
    0x286F_119C
);
mealybug_dmg_b!(
    test_m3_lcdc_obj_en_change_dmg_b,
    "m3_lcdc_obj_en_change",
    0xE7B3_C4EC
);
mealybug_dmg_b!(
    test_m3_lcdc_obj_en_change_variant_dmg_b,
    "m3_lcdc_obj_en_change_variant",
    0x9840_7F19
);
mealybug_dmg_b!(
    test_m3_lcdc_obj_size_change_dmg_b,
    "m3_lcdc_obj_size_change",
    0xB198_14D0
);
mealybug_dmg_b!(
    test_m3_lcdc_obj_size_change_scx_dmg_b,
    "m3_lcdc_obj_size_change_scx",
    0x7564_DEC9
);
mealybug_dmg_b!(
    test_m3_lcdc_tile_sel_change_dmg_b,
    "m3_lcdc_tile_sel_change",
    0x2CFB_252D
);
mealybug_dmg_b!(
    test_m3_lcdc_tile_sel_win_change_dmg_b,
    "m3_lcdc_tile_sel_win_change",
    0x12DD_F759
);
mealybug_dmg_b!(
    test_m3_lcdc_win_en_change_multiple_dmg_b,
    "m3_lcdc_win_en_change_multiple",
    0xD1B2_30C6
);
mealybug_dmg_b!(
    test_m3_lcdc_win_en_change_multiple_wx_dmg_b,
    "m3_lcdc_win_en_change_multiple_wx",
    0xF538_4C09
);
mealybug_dmg_b!(
    test_m3_lcdc_win_map_change_dmg_b,
    "m3_lcdc_win_map_change",
    0x6066_383D
);
mealybug_dmg_b!(test_m3_obp0_change_dmg_b, "m3_obp0_change", 0xC7E0_7D30);
mealybug_dmg_b!(
    test_m3_scx_high_5_bits_dmg_b,
    "m3_scx_high_5_bits",
    0x76B4_CBF2
);
mealybug_dmg_b!(
    test_m3_scx_low_3_bits_dmg_b,
    "m3_scx_low_3_bits",
    0xD49D_F057
);
mealybug_dmg_b!(test_m3_scy_change_dmg_b, "m3_scy_change", 0x8179_BF2F);
mealybug_dmg_b!(test_m3_window_timing_dmg_b, "m3_window_timing", 0x92B6_5C2A);
mealybug_dmg_b!(
    test_m3_window_timing_wx_0_dmg_b,
    "m3_window_timing_wx_0",
    0x68EF_35FF
);
mealybug_dmg_b!(test_m3_wx_4_change_dmg_b, "m3_wx_4_change", 0xCC43_C685);
mealybug_dmg_b!(
    test_m3_wx_4_change_sprites_dmg_b,
    "m3_wx_4_change_sprites",
    0x9929_A33F
);
mealybug_dmg_b!(test_m3_wx_5_change_dmg_b, "m3_wx_5_change", 0xC4E8_2D09);
mealybug_dmg_b!(test_m3_wx_6_change_dmg_b, "m3_wx_6_change", 0x271A_96AF);

// ============================================================================
// CGB-C tests (reference: expected/CPU CGB C/)
// ============================================================================

mealybug_cgb_c!(test_m2_win_en_toggle_cgb_c, "m2_win_en_toggle", 0x5BB7_9D8A);
mealybug_cgb_c!(test_m3_bgp_change_cgb_c, "m3_bgp_change", 0x1A14_901B);
mealybug_cgb_c!(
    test_m3_bgp_change_sprites_cgb_c,
    "m3_bgp_change_sprites",
    0x4F83_5D92
);
mealybug_cgb_c!(
    test_m3_lcdc_bg_en_change_cgb_c,
    "m3_lcdc_bg_en_change",
    0xB600_98E1
);
mealybug_cgb_c!(
    test_m3_lcdc_bg_en_change2_cgb_c,
    "m3_lcdc_bg_en_change2",
    0x9610_CDF4
);
mealybug_cgb_c!(
    test_m3_lcdc_bg_map_change_cgb_c,
    "m3_lcdc_bg_map_change",
    0x044C_1F04
);
mealybug_cgb_c!(
    test_m3_lcdc_bg_map_change2_cgb_c,
    "m3_lcdc_bg_map_change2",
    0xFFD9_6BD0
);
mealybug_cgb_c!(
    test_m3_lcdc_obj_en_change_cgb_c,
    "m3_lcdc_obj_en_change",
    0xAC65_AE57
);
mealybug_cgb_c!(
    test_m3_lcdc_obj_en_change_variant_cgb_c,
    "m3_lcdc_obj_en_change_variant",
    0x1CC1_760F
);
mealybug_cgb_c!(
    test_m3_lcdc_obj_size_change_cgb_c,
    "m3_lcdc_obj_size_change",
    0xE7AD_A38D
);
mealybug_cgb_c!(
    test_m3_lcdc_obj_size_change_scx_cgb_c,
    "m3_lcdc_obj_size_change_scx",
    0x19B3_AC60
);
mealybug_cgb_c!(
    test_m3_lcdc_tile_sel_change_cgb_c,
    "m3_lcdc_tile_sel_change",
    0x1542_042D
);
mealybug_cgb_c!(
    test_m3_lcdc_tile_sel_change2_cgb_c,
    "m3_lcdc_tile_sel_change2",
    0x607B_6469
);
mealybug_cgb_c!(
    test_m3_lcdc_tile_sel_win_change_cgb_c,
    "m3_lcdc_tile_sel_win_change",
    0xCA7A_715D
);
mealybug_cgb_c!(
    test_m3_lcdc_tile_sel_win_change2_cgb_c,
    "m3_lcdc_tile_sel_win_change2",
    0x81DC_4AC9
);
mealybug_cgb_c!(
    test_m3_lcdc_win_en_change_multiple_cgb_c,
    "m3_lcdc_win_en_change_multiple",
    0xC001_01D8
);
// test_m3_lcdc_win_en_change_multiple_wx_cgb_c — commented out: no valid reference
// PNG exists for this ROM on CGB-C. The file in expected/CPU CGB C/ is a GIMP-created
// placeholder (identical to m3_wx_4_change, m3_wx_5_change, m3_wx_6_change) and
// does not represent a real hardware capture. The test cannot be verified until a
// proper reference screenshot is provided. See issue #2598.
mealybug_cgb_c!(
    test_m3_lcdc_win_map_change_cgb_c,
    "m3_lcdc_win_map_change",
    0x3E2C_073C
);
mealybug_cgb_c!(
    test_m3_lcdc_win_map_change2_cgb_c,
    "m3_lcdc_win_map_change2",
    0x0A03_88F2
);
mealybug_cgb_c!(test_m3_obp0_change_cgb_c, "m3_obp0_change", 0x7484_BAF1);
mealybug_cgb_c!(
    test_m3_scx_high_5_bits_cgb_c,
    "m3_scx_high_5_bits",
    0x3C71_CF1F
);
mealybug_cgb_c!(
    test_m3_scx_high_5_bits_change2_cgb_c,
    "m3_scx_high_5_bits_change2",
    0x582C_90F1
);
mealybug_cgb_c!(
    test_m3_scx_low_3_bits_cgb_c,
    "m3_scx_low_3_bits",
    0xD49D_F057
);
mealybug_cgb_c!(test_m3_scy_change_cgb_c, "m3_scy_change", 0xEEAF_63B5);
mealybug_cgb_c!(test_m3_scy_change2_cgb_c, "m3_scy_change2", 0x6D57_9852);
mealybug_cgb_c!(test_m3_window_timing_cgb_c, "m3_window_timing", 0x0BE0_3D45);
mealybug_cgb_c!(
    test_m3_window_timing_wx_0_cgb_c,
    "m3_window_timing_wx_0",
    0x1C33_F2FF
);
// test_m3_wx_4_change_cgb_c — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_c above. See issue #2598.
mealybug_cgb_c!(
    test_m3_wx_4_change_sprites_cgb_c,
    "m3_wx_4_change_sprites",
    0x2F7D_8812
);
// test_m3_wx_5_change_cgb_c — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_c above. See issue #2598.

// test_m3_wx_6_change_cgb_c — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_c above. See issue #2598.

// ============================================================================
// CGB-D tests (reference: expected/CPU CGB D/)
// ============================================================================

mealybug_cgb_d!(test_m2_win_en_toggle_cgb_d, "m2_win_en_toggle", 0x5BB7_9D8A);
mealybug_cgb_d!(test_m3_bgp_change_cgb_d, "m3_bgp_change", 0xEAF2_256B);
mealybug_cgb_d!(
    test_m3_bgp_change_sprites_cgb_d,
    "m3_bgp_change_sprites",
    0x09D9_587E
);
mealybug_cgb_d!(
    test_m3_lcdc_bg_en_change_cgb_d,
    "m3_lcdc_bg_en_change",
    0xB600_98E1
);
mealybug_cgb_d!(
    test_m3_lcdc_bg_map_change_cgb_d,
    "m3_lcdc_bg_map_change",
    0x044C_1F04
);
mealybug_cgb_d!(
    test_m3_lcdc_obj_en_change_cgb_d,
    "m3_lcdc_obj_en_change",
    0xAC65_AE57
);
mealybug_cgb_d!(
    test_m3_lcdc_obj_en_change_variant_cgb_d,
    "m3_lcdc_obj_en_change_variant",
    0x7DA1_31B3
);
mealybug_cgb_d!(
    test_m3_lcdc_obj_size_change_cgb_d,
    "m3_lcdc_obj_size_change",
    0xE7AD_A38D
);
mealybug_cgb_d!(
    test_m3_lcdc_obj_size_change_scx_cgb_d,
    "m3_lcdc_obj_size_change_scx",
    0x19B3_AC60
);
mealybug_cgb_d!(
    test_m3_lcdc_tile_sel_change_cgb_d,
    "m3_lcdc_tile_sel_change",
    0x1542_042D
);
mealybug_cgb_d!(
    test_m3_lcdc_tile_sel_win_change_cgb_d,
    "m3_lcdc_tile_sel_win_change",
    0xCA7A_715D
);
mealybug_cgb_d!(
    test_m3_lcdc_win_en_change_multiple_cgb_d,
    "m3_lcdc_win_en_change_multiple",
    0xC001_01D8
);
// test_m3_lcdc_win_en_change_multiple_wx_cgb_d — commented out: no valid reference
// PNG exists for this ROM on CGB-D. The file in expected/CPU CGB D/ is a GIMP-created
// placeholder (identical to m3_wx_4_change, m3_wx_5_change, m3_wx_6_change) and
// does not represent a real hardware capture. The test cannot be verified until a
// proper reference screenshot is provided. See issue #2598.
mealybug_cgb_d!(
    test_m3_lcdc_win_map_change_cgb_d,
    "m3_lcdc_win_map_change",
    0x3E2C_073C
);
// NOTE: m3_lcdc_win_map_change2 has no reference image in expected/CPU CGB D/,
// so no CGB-D test is added here (CGB-C only).
mealybug_cgb_d!(test_m3_obp0_change_cgb_d, "m3_obp0_change", 0xF2A5_FCD4);
mealybug_cgb_d!(
    test_m3_scx_high_5_bits_cgb_d,
    "m3_scx_high_5_bits",
    0x3C71_CF1F
);
mealybug_cgb_d!(
    test_m3_scx_low_3_bits_cgb_d,
    "m3_scx_low_3_bits",
    0xD49D_F057
);
mealybug_cgb_d!(test_m3_scy_change_cgb_d, "m3_scy_change", 0x7A71_4C6D);
mealybug_cgb_d!(test_m3_window_timing_cgb_d, "m3_window_timing", 0x92B6_5C2A);
mealybug_cgb_d!(
    test_m3_window_timing_wx_0_cgb_d,
    "m3_window_timing_wx_0",
    0x68EF_35FF
);
// test_m3_wx_4_change_cgb_d — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_d above. See issue #2598.
mealybug_cgb_d!(
    test_m3_wx_4_change_sprites_cgb_d,
    "m3_wx_4_change_sprites",
    0x2F7D_8812
);
// test_m3_wx_5_change_cgb_d — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_d above. See issue #2598.

// test_m3_wx_6_change_cgb_d — commented out: no valid reference PNG. Same reason
// as test_m3_lcdc_win_en_change_multiple_wx_cgb_d above. See issue #2598.
