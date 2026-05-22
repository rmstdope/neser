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
//! Compare each PNG to the reference image in the submodule:
//!
//! ```bash
//! compare -metric AE \
//!     target/mealybug-captures/m3_bgp_change_dmg_b.png \
//!     roms/gb/automated_tests/mealybug-tearoom-tests/expected/DMG-blob/m3_bgp_change.png \
//!     /dev/null
//! ```

use std::io::Read;
use std::sync::OnceLock;

use super::helpers::{load_cgb_rom_from_bytes, load_gb_rom_from_bytes, run_to_breakpoint_and_crc};
use crate::gb::model::{CgbModel, DmgModel};

const ZIP_PATH: &str = "roms/gb/automated_tests/mealybug-tearoom-tests/mealybug-tearoom-tests.zip";

const CYCLE_LIMIT: u64 = 10_000_000;

/// Return the raw bytes of the mealybug ZIP archive, reading the file only once.
fn zip_bytes() -> &'static [u8] {
    static ZIP: OnceLock<Vec<u8>> = OnceLock::new();
    ZIP.get_or_init(|| {
        std::fs::read(ZIP_PATH).unwrap_or_else(|e| panic!("failed to read {ZIP_PATH}: {e}"))
    })
}

/// Extract a ROM by filename from the mealybug zip archive and return its bytes.
fn read_rom_from_zip(rom_name: &str) -> Vec<u8> {
    let cursor = std::io::Cursor::new(zip_bytes());
    let mut archive =
        zip::ZipArchive::new(cursor).expect("mealybug zip should be a valid ZIP archive");
    let mut entry = archive
        .by_name(rom_name)
        .unwrap_or_else(|_| panic!("{rom_name} not found in mealybug zip"));
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .expect("read ROM bytes from zip");
    bytes
}

// ============================================================================
// Test-generating macros
// ============================================================================

/// Generate an active DMG-B mealybug test with a CRC assertion.
macro_rules! mealybug_dmg_b {
    ($name:ident, $rom_base:literal, $expected_crc:expr) => {
        #[test]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
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
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
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
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_d"));
            assert_mealybug_crc(concat!($rom_base, "_cgb_d"), crc, $expected_crc);
        }
    };
}

/// Generate an ignored DMG-B mealybug test.
#[allow(unused_macros)]
macro_rules! mealybug_ignored_dmg_b {
    ($name:ident, $rom_base:literal, $issue:literal, $expected_crc:expr) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_dmg_b"));
            assert_mealybug_crc(concat!($rom_base, "_dmg_b"), crc, $expected_crc);
        }
    };
}

/// Generate an ignored CGB-C mealybug test.
macro_rules! mealybug_ignored_cgb_c {
    ($name:ident, $rom_base:literal, $issue:literal, $expected_crc:expr) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
            let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_c"));
            assert_mealybug_crc(concat!($rom_base, "_cgb_c"), crc, $expected_crc);
        }
    };
}

/// Generate an ignored CGB-D mealybug test.
macro_rules! mealybug_ignored_cgb_d {
    ($name:ident, $rom_base:literal, $issue:literal, $expected_crc:expr) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
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

    use crate::gb::ppu::screen_buffer::ScreenBuffer;

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

    fn macro_definition<'a>(source: &'a str, macro_name: &str) -> &'a str {
        let marker = format!("macro_rules! {macro_name}");
        let start = source
            .find(&marker)
            .unwrap_or_else(|| panic!("{macro_name} macro definition should exist"));
        let open_brace = source[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{macro_name} macro definition should open with a brace"));

        let mut depth = 0_u32;
        for (offset, ch) in source[start + open_brace..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = start + open_brace + offset;
                        return &source[start..=end];
                    }
                }
                _ => {}
            }
        }

        panic!("{macro_name} macro definition should close its braces");
    }

    fn parse_mealybug_cases(source: &str) -> Vec<MealybugCase> {
        let specs = [
            ("mealybug_dmg_b", MealybugModel::DmgB, false),
            ("mealybug_cgb_c", MealybugModel::CgbC, false),
            ("mealybug_cgb_d", MealybugModel::CgbD, false),
            ("mealybug_ignored_dmg_b", MealybugModel::DmgB, true),
            ("mealybug_ignored_cgb_c", MealybugModel::CgbC, true),
            ("mealybug_ignored_cgb_d", MealybugModel::CgbD, true),
        ];
        let mut cases = Vec::new();

        for (macro_name, model, ignored) in specs {
            let needle = format!("{macro_name}!(");
            let mut search_from = 0;

            while let Some(relative_start) = source[search_from..].find(&needle) {
                let invocation_start = search_from + relative_start;
                let args_start = invocation_start + needle.len();
                let close_paren = find_matching_paren(source, args_start - 1);
                let args = split_macro_args(&source[args_start..close_paren]);
                let expected_arg_count = if ignored { 4 } else { 3 };
                assert_eq!(
                    args.len(),
                    expected_arg_count,
                    "{macro_name} invocation should have {expected_arg_count} arguments: {args:?}"
                );

                cases.push(MealybugCase {
                    model,
                    rom_base: unquote(&args[1]),
                    expected_crc: parse_crc(&args[expected_arg_count - 1]),
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
        Path::new("roms/gb/automated_tests/mealybug-tearoom-tests/expected")
            .join(case.model.expected_dir())
            .join(format!("{}.png", case.rom_base))
    }

    fn expected_png_count(model: MealybugModel) -> usize {
        std::fs::read_dir(
            Path::new("roms/gb/automated_tests/mealybug-tearoom-tests/expected")
                .join(model.expected_dir()),
        )
        .unwrap_or_else(|err| panic!("read expected PNG directory for {:?}: {err}", model))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .count()
    }

    fn decoded_png_rgb_crc(path: &Path) -> u32 {
        let file = std::fs::File::open(path)
            .unwrap_or_else(|err| panic!("open expected PNG {}: {err}", path.display()));
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);

        let mut reader = decoder
            .read_info()
            .unwrap_or_else(|err| panic!("read expected PNG info {}: {err}", path.display()));
        let mut raw = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut raw)
            .unwrap_or_else(|err| panic!("decode expected PNG {}: {err}", path.display()));
        let raw = &raw[..info.buffer_size()];

        assert_eq!(
            (info.width, info.height),
            (ScreenBuffer::WIDTH, ScreenBuffer::HEIGHT),
            "{} should have Game Boy screen dimensions",
            path.display()
        );

        let rgb = match info.color_type {
            png::ColorType::Rgb => raw.to_vec(),
            png::ColorType::Rgba => raw
                .chunks_exact(4)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect(),
            png::ColorType::Grayscale => raw.iter().flat_map(|value| [*value; 3]).collect(),
            png::ColorType::GrayscaleAlpha => raw
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0]; 3])
                .collect(),
            png::ColorType::Indexed => {
                panic!("{} should be expanded from indexed to RGB", path.display())
            }
        };

        assert_eq!(
            rgb.len(),
            (ScreenBuffer::WIDTH * ScreenBuffer::HEIGHT * 3) as usize,
            "{} should decode to RGB8 screen-buffer bytes",
            path.display()
        );

        crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(&rgb)
    }

    #[test]
    fn ignored_mealybug_macros_require_expected_crc_assertions() {
        let source = include_str!("mealybug_tests.rs");

        for macro_name in [
            "mealybug_ignored_dmg_b",
            "mealybug_ignored_cgb_c",
            "mealybug_ignored_cgb_d",
        ] {
            let definition = macro_definition(source, macro_name);
            assert!(
                definition.contains("$expected_crc:expr"),
                "{macro_name} should require an expected CRC argument"
            );
            assert!(
                definition.contains("assert_mealybug_crc("),
                "{macro_name} should assert the breakpoint CRC"
            );
        }
    }

    #[test]
    fn expected_crc_constants_match_reference_pngs() {
        let source = include_str!("mealybug_tests.rs");
        let cases = parse_mealybug_cases(source);
        assert_eq!(
            cases.len(),
            79,
            "all scoped Mealybug tests should be audited"
        );

        for (model, expected_count) in [
            (MealybugModel::DmgB, 24),
            (MealybugModel::CgbC, 31),
            (MealybugModel::CgbD, 24),
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
                expected_count,
                "{} expected PNG count should match #2427 coverage",
                model.suffix()
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
// The CGB non-sprite WX/LCDC-WX reference PNGs for #2598 are not native emulator
// captures: they are 171-colour GIMP images and the same image is reused by
// multiple ROMs.
mealybug_ignored_cgb_c!(
    test_m3_lcdc_win_en_change_multiple_wx_cgb_c,
    "m3_lcdc_win_en_change_multiple_wx",
    "2598",
    0x6581_49F1
);
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
// Same invalid-reference rationale as the CGB-C non-sprite LCDC-WX case above.
mealybug_ignored_cgb_c!(
    test_m3_wx_4_change_cgb_c,
    "m3_wx_4_change",
    "2598",
    0x6581_49F1
);
mealybug_cgb_c!(
    test_m3_wx_4_change_sprites_cgb_c,
    "m3_wx_4_change_sprites",
    0x2F7D_8812
);
mealybug_ignored_cgb_c!(
    test_m3_wx_5_change_cgb_c,
    "m3_wx_5_change",
    "2598",
    0x6581_49F1
);
mealybug_ignored_cgb_c!(
    test_m3_wx_6_change_cgb_c,
    "m3_wx_6_change",
    "2598",
    0x6581_49F1
);

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
// Same invalid-reference rationale as the CGB-C non-sprite LCDC-WX case above.
mealybug_ignored_cgb_d!(
    test_m3_lcdc_win_en_change_multiple_wx_cgb_d,
    "m3_lcdc_win_en_change_multiple_wx",
    "2598",
    0x6581_49F1
);
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
// Same invalid-reference rationale as the CGB-C non-sprite LCDC-WX case above.
mealybug_ignored_cgb_d!(
    test_m3_wx_4_change_cgb_d,
    "m3_wx_4_change",
    "2598",
    0x6581_49F1
);
mealybug_cgb_d!(
    test_m3_wx_4_change_sprites_cgb_d,
    "m3_wx_4_change_sprites",
    0x2F7D_8812
);
mealybug_ignored_cgb_d!(
    test_m3_wx_5_change_cgb_d,
    "m3_wx_5_change",
    "2598",
    0x6581_49F1
);
mealybug_ignored_cgb_d!(
    test_m3_wx_6_change_cgb_d,
    "m3_wx_6_change",
    "2598",
    0x6581_49F1
);
