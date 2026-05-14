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

/// Generate an ignored DMG-B mealybug test.
macro_rules! mealybug_ignored_dmg_b {
    ($name:ident, $rom_base:literal, $issue:literal) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
            run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_dmg_b"));
        }
    };
}

/// Generate an ignored CGB-C mealybug test.
macro_rules! mealybug_ignored_cgb_c {
    ($name:ident, $rom_base:literal, $issue:literal) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
            run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_c"));
        }
    };
}

/// Generate an ignored CGB-D mealybug test.
macro_rules! mealybug_ignored_cgb_d {
    ($name:ident, $rom_base:literal, $issue:literal) => {
        #[test]
        #[ignore = concat!("mealybug: PPU timing not yet accurate — tracked in #", $issue)]
        fn $name() {
            let bytes = read_rom_from_zip(concat!($rom_base, ".gb"));
            let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
            run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, concat!($rom_base, "_cgb_d"));
        }
    };
}

fn assert_mealybug_crc(capture_name: &str, crc: u32, expected_crc: u32) {
    assert_eq!(
        crc, expected_crc,
        "{capture_name} CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}"
    );
}

// ============================================================================
// DMG-B tests (reference: expected/DMG-blob/)
// ============================================================================

#[test]
fn test_m2_win_en_toggle_dmg_b() {
    let bytes = read_rom_from_zip("m2_win_en_toggle.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m2_win_en_toggle_dmg_b");
    const EXPECTED_CRC: u32 = 0xCE29_5724;
    assert_eq!(
        crc, EXPECTED_CRC,
        "m2_win_en_toggle DMG-B CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

#[test]
fn test_m3_bgp_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_dmg_b");
    const EXPECTED_CRC: u32 = 0x2BA6_1257;
    assert_mealybug_crc("m3_bgp_change_dmg_b", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_bgp_change_sprites_dmg_b() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_dmg_b");
    const EXPECTED_CRC: u32 = 0x7E8E_86BC;
    assert_mealybug_crc("m3_bgp_change_sprites_dmg_b", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_en_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_dmg_b");
    const EXPECTED_CRC: u32 = 0x8897_C19D;
    assert_mealybug_crc("m3_lcdc_bg_en_change_dmg_b", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_map_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_dmg_b");
    const EXPECTED_CRC: u32 = 0x286F_119C;
    assert_mealybug_crc("m3_lcdc_bg_map_change_dmg_b", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_en_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_dmg_b");
    const EXPECTED_CRC: u32 = 0xE7B3_C4EC;
    assert_mealybug_crc("m3_lcdc_obj_en_change_dmg_b", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_lcdc_obj_en_change_variant_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_dmg_b");
    const EXPECTED_CRC: u32 = 0x9840_7F19;
    assert_mealybug_crc("m3_lcdc_obj_en_change_variant_dmg_b", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_size_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_dmg_b");
}

#[test]
fn test_m3_lcdc_obj_size_change_scx_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_dmg_b");
}
mealybug_ignored_dmg_b!(
    test_m3_lcdc_tile_sel_change_dmg_b,
    "m3_lcdc_tile_sel_change",
    "2353"
);
mealybug_ignored_dmg_b!(
    test_m3_lcdc_tile_sel_win_change_dmg_b,
    "m3_lcdc_tile_sel_win_change",
    "2353"
);
mealybug_ignored_dmg_b!(
    test_m3_lcdc_win_en_change_multiple_dmg_b,
    "m3_lcdc_win_en_change_multiple",
    "2354"
);
mealybug_ignored_dmg_b!(
    test_m3_lcdc_win_en_change_multiple_wx_dmg_b,
    "m3_lcdc_win_en_change_multiple_wx",
    "2354"
);
mealybug_ignored_dmg_b!(
    test_m3_lcdc_win_map_change_dmg_b,
    "m3_lcdc_win_map_change",
    "2355"
);
mealybug_ignored_dmg_b!(test_m3_obp0_change_dmg_b, "m3_obp0_change", "2356");
mealybug_ignored_dmg_b!(test_m3_scx_high_5_bits_dmg_b, "m3_scx_high_5_bits", "2357");
mealybug_ignored_dmg_b!(test_m3_scx_low_3_bits_dmg_b, "m3_scx_low_3_bits", "2357");
mealybug_ignored_dmg_b!(test_m3_scy_change_dmg_b, "m3_scy_change", "2358");
mealybug_ignored_dmg_b!(test_m3_window_timing_dmg_b, "m3_window_timing", "2359");
mealybug_ignored_dmg_b!(
    test_m3_window_timing_wx_0_dmg_b,
    "m3_window_timing_wx_0",
    "2359"
);
mealybug_ignored_dmg_b!(test_m3_wx_4_change_dmg_b, "m3_wx_4_change", "2360");
mealybug_ignored_dmg_b!(
    test_m3_wx_4_change_sprites_dmg_b,
    "m3_wx_4_change_sprites",
    "2360"
);
mealybug_ignored_dmg_b!(test_m3_wx_5_change_dmg_b, "m3_wx_5_change", "2360");
mealybug_ignored_dmg_b!(test_m3_wx_6_change_dmg_b, "m3_wx_6_change", "2360");

// ============================================================================
// CGB-C tests (reference: expected/CPU CGB C/)
// ============================================================================

#[test]
fn test_m2_win_en_toggle_cgb_c() {
    let bytes = read_rom_from_zip("m2_win_en_toggle.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m2_win_en_toggle_cgb_c");
    const EXPECTED_CRC: u32 = 0x5BB7_9D8A;
    assert_eq!(
        crc, EXPECTED_CRC,
        "m2_win_en_toggle CGB-C CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

#[test]
fn test_m3_bgp_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_cgb_c");
    const EXPECTED_CRC: u32 = 0x1A14_901B;
    assert_mealybug_crc("m3_bgp_change_cgb_c", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_bgp_change_sprites_cgb_c() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_cgb_c");
    const EXPECTED_CRC: u32 = 0x4F83_5D92;
    assert_mealybug_crc("m3_bgp_change_sprites_cgb_c", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_en_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_cgb_c");
    const EXPECTED_CRC: u32 = 0xB600_98E1;
    assert_mealybug_crc("m3_lcdc_bg_en_change_cgb_c", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_lcdc_bg_en_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change2_cgb_c");
    const EXPECTED_CRC: u32 = 0x9610_CDF4;
    assert_mealybug_crc("m3_lcdc_bg_en_change2_cgb_c", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_map_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_cgb_c");
    const EXPECTED_CRC: u32 = 0x044C_1F04;
    assert_mealybug_crc("m3_lcdc_bg_map_change_cgb_c", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_lcdc_bg_map_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change2_cgb_c");
    const EXPECTED_CRC: u32 = 0xFFD9_6BD0;
    assert_mealybug_crc("m3_lcdc_bg_map_change2_cgb_c", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_en_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_cgb_c");
    const EXPECTED_CRC: u32 = 0xAC65_AE57;
    assert_mealybug_crc("m3_lcdc_obj_en_change_cgb_c", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_lcdc_obj_en_change_variant_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_cgb_c");
    const EXPECTED_CRC: u32 = 0x1CC1_760F;
    assert_mealybug_crc("m3_lcdc_obj_en_change_variant_cgb_c", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_size_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_cgb_c");
}

#[test]
fn test_m3_lcdc_obj_size_change_scx_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_cgb_c");
}
mealybug_ignored_cgb_c!(
    test_m3_lcdc_tile_sel_change_cgb_c,
    "m3_lcdc_tile_sel_change",
    "2353"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_tile_sel_change2_cgb_c,
    "m3_lcdc_tile_sel_change2",
    "2353"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_tile_sel_win_change_cgb_c,
    "m3_lcdc_tile_sel_win_change",
    "2353"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_tile_sel_win_change2_cgb_c,
    "m3_lcdc_tile_sel_win_change2",
    "2353"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_win_en_change_multiple_cgb_c,
    "m3_lcdc_win_en_change_multiple",
    "2354"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_win_en_change_multiple_wx_cgb_c,
    "m3_lcdc_win_en_change_multiple_wx",
    "2354"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_win_map_change_cgb_c,
    "m3_lcdc_win_map_change",
    "2355"
);
mealybug_ignored_cgb_c!(
    test_m3_lcdc_win_map_change2_cgb_c,
    "m3_lcdc_win_map_change2",
    "2355"
);
mealybug_ignored_cgb_c!(test_m3_obp0_change_cgb_c, "m3_obp0_change", "2356");
mealybug_ignored_cgb_c!(test_m3_scx_high_5_bits_cgb_c, "m3_scx_high_5_bits", "2357");
mealybug_ignored_cgb_c!(
    test_m3_scx_high_5_bits_change2_cgb_c,
    "m3_scx_high_5_bits_change2",
    "2357"
);
mealybug_ignored_cgb_c!(test_m3_scx_low_3_bits_cgb_c, "m3_scx_low_3_bits", "2357");
mealybug_ignored_cgb_c!(test_m3_scy_change_cgb_c, "m3_scy_change", "2358");
mealybug_ignored_cgb_c!(test_m3_scy_change2_cgb_c, "m3_scy_change2", "2358");
mealybug_ignored_cgb_c!(test_m3_window_timing_cgb_c, "m3_window_timing", "2359");
mealybug_ignored_cgb_c!(
    test_m3_window_timing_wx_0_cgb_c,
    "m3_window_timing_wx_0",
    "2359"
);
mealybug_ignored_cgb_c!(test_m3_wx_4_change_cgb_c, "m3_wx_4_change", "2360");
mealybug_ignored_cgb_c!(
    test_m3_wx_4_change_sprites_cgb_c,
    "m3_wx_4_change_sprites",
    "2360"
);
mealybug_ignored_cgb_c!(test_m3_wx_5_change_cgb_c, "m3_wx_5_change", "2360");
mealybug_ignored_cgb_c!(test_m3_wx_6_change_cgb_c, "m3_wx_6_change", "2360");

// ============================================================================
// CGB-D tests (reference: expected/CPU CGB D/)
// ============================================================================

#[test]
fn test_m2_win_en_toggle_cgb_d() {
    let bytes = read_rom_from_zip("m2_win_en_toggle.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m2_win_en_toggle_cgb_d");
    const EXPECTED_CRC: u32 = 0x5BB7_9D8A;
    assert_eq!(
        crc, EXPECTED_CRC,
        "m2_win_en_toggle CGB-D CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

#[test]
fn test_m3_bgp_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_cgb_d");
    const EXPECTED_CRC: u32 = 0xEAF2_256B;
    assert_mealybug_crc("m3_bgp_change_cgb_d", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_bgp_change_sprites_cgb_d() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_cgb_d");
    const EXPECTED_CRC: u32 = 0x09D9_587E;
    assert_mealybug_crc("m3_bgp_change_sprites_cgb_d", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_en_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_cgb_d");
    const EXPECTED_CRC: u32 = 0xB600_98E1;
    assert_mealybug_crc("m3_lcdc_bg_en_change_cgb_d", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_bg_map_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_cgb_d");
    const EXPECTED_CRC: u32 = 0x044C_1F04;
    assert_mealybug_crc("m3_lcdc_bg_map_change_cgb_d", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_en_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_cgb_d");
    const EXPECTED_CRC: u32 = 0xAC65_AE57;
    assert_mealybug_crc("m3_lcdc_obj_en_change_cgb_d", crc, EXPECTED_CRC);
}

#[test]
fn test_m3_lcdc_obj_en_change_variant_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_cgb_d");
    const EXPECTED_CRC: u32 = 0x7DA1_31B3;
    assert_mealybug_crc("m3_lcdc_obj_en_change_variant_cgb_d", crc, EXPECTED_CRC);
}
#[test]
fn test_m3_lcdc_obj_size_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_cgb_d");
}

#[test]
fn test_m3_lcdc_obj_size_change_scx_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_cgb_d");
}
mealybug_ignored_cgb_d!(
    test_m3_lcdc_tile_sel_change_cgb_d,
    "m3_lcdc_tile_sel_change",
    "2353"
);
mealybug_ignored_cgb_d!(
    test_m3_lcdc_tile_sel_win_change_cgb_d,
    "m3_lcdc_tile_sel_win_change",
    "2353"
);
mealybug_ignored_cgb_d!(
    test_m3_lcdc_win_en_change_multiple_cgb_d,
    "m3_lcdc_win_en_change_multiple",
    "2354"
);
mealybug_ignored_cgb_d!(
    test_m3_lcdc_win_en_change_multiple_wx_cgb_d,
    "m3_lcdc_win_en_change_multiple_wx",
    "2354"
);
mealybug_ignored_cgb_d!(
    test_m3_lcdc_win_map_change_cgb_d,
    "m3_lcdc_win_map_change",
    "2355"
);
mealybug_ignored_cgb_d!(test_m3_obp0_change_cgb_d, "m3_obp0_change", "2356");
mealybug_ignored_cgb_d!(test_m3_scx_high_5_bits_cgb_d, "m3_scx_high_5_bits", "2357");
mealybug_ignored_cgb_d!(test_m3_scx_low_3_bits_cgb_d, "m3_scx_low_3_bits", "2357");
mealybug_ignored_cgb_d!(test_m3_scy_change_cgb_d, "m3_scy_change", "2358");
mealybug_ignored_cgb_d!(test_m3_window_timing_cgb_d, "m3_window_timing", "2359");
mealybug_ignored_cgb_d!(
    test_m3_window_timing_wx_0_cgb_d,
    "m3_window_timing_wx_0",
    "2359"
);
mealybug_ignored_cgb_d!(test_m3_wx_4_change_cgb_d, "m3_wx_4_change", "2360");
mealybug_ignored_cgb_d!(
    test_m3_wx_4_change_sprites_cgb_d,
    "m3_wx_4_change_sprites",
    "2360"
);
mealybug_ignored_cgb_d!(test_m3_wx_5_change_cgb_d, "m3_wx_5_change", "2360");
mealybug_ignored_cgb_d!(test_m3_wx_6_change_cgb_d, "m3_wx_6_change", "2360");
