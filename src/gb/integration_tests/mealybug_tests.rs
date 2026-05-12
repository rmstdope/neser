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
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_sprites_dmg_b() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2349"]
fn test_m3_lcdc_bg_en_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2350"]
fn test_m3_lcdc_bg_map_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_variant_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_scx_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_win_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_win_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_win_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_en_change_multiple_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_wx_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple_wx.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(
        &mut gb,
        CYCLE_LIMIT,
        "m3_lcdc_win_en_change_multiple_wx_dmg_b",
    );
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2355"]
fn test_m3_lcdc_win_map_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_lcdc_win_map_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_map_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2356"]
fn test_m3_obp0_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_obp0_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_obp0_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_high_5_bits_dmg_b() {
    let bytes = read_rom_from_zip("m3_scx_high_5_bits.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_high_5_bits_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_low_3_bits_dmg_b() {
    let bytes = read_rom_from_zip("m3_scx_low_3_bits.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_low_3_bits_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2358"]
fn test_m3_scy_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_scy_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scy_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_dmg_b() {
    let bytes = read_rom_from_zip("m3_window_timing.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_wx_0_dmg_b() {
    let bytes = read_rom_from_zip("m3_window_timing_wx_0.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_wx_0_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_wx_4_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_sprites_dmg_b() {
    let bytes = read_rom_from_zip("m3_wx_4_change_sprites.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_sprites_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_5_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_wx_5_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_5_change_dmg_b");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_6_change_dmg_b() {
    let bytes = read_rom_from_zip("m3_wx_6_change.gb");
    let mut gb = load_gb_rom_from_bytes(&bytes, DmgModel::DmgB);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_6_change_dmg_b");
}

// ============================================================================
// CGB-C tests (reference: expected/CPU CGB C/)
// ============================================================================

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2347"]
fn test_m2_win_en_toggle_cgb_c() {
    let bytes = read_rom_from_zip("m2_win_en_toggle.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m2_win_en_toggle_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_sprites_cgb_c() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2349"]
fn test_m3_lcdc_bg_en_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2349"]
fn test_m3_lcdc_bg_en_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2350"]
fn test_m3_lcdc_bg_map_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2350"]
fn test_m3_lcdc_bg_map_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_variant_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_scx_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_win_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_win_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_win_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_win_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_win_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_win_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_en_change_multiple_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_wx_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple_wx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(
        &mut gb,
        CYCLE_LIMIT,
        "m3_lcdc_win_en_change_multiple_wx_cgb_c",
    );
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2355"]
fn test_m3_lcdc_win_map_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_win_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_map_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2355"]
fn test_m3_lcdc_win_map_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_lcdc_win_map_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_map_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2356"]
fn test_m3_obp0_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_obp0_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_obp0_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_high_5_bits_cgb_c() {
    let bytes = read_rom_from_zip("m3_scx_high_5_bits.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_high_5_bits_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_high_5_bits_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_scx_high_5_bits_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_high_5_bits_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_low_3_bits_cgb_c() {
    let bytes = read_rom_from_zip("m3_scx_low_3_bits.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_low_3_bits_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2358"]
fn test_m3_scy_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_scy_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scy_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2358"]
fn test_m3_scy_change2_cgb_c() {
    let bytes = read_rom_from_zip("m3_scy_change2.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scy_change2_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_cgb_c() {
    let bytes = read_rom_from_zip("m3_window_timing.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_wx_0_cgb_c() {
    let bytes = read_rom_from_zip("m3_window_timing_wx_0.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_wx_0_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_wx_4_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_sprites_cgb_c() {
    let bytes = read_rom_from_zip("m3_wx_4_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_sprites_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_5_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_wx_5_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_5_change_cgb_c");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_6_change_cgb_c() {
    let bytes = read_rom_from_zip("m3_wx_6_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbC);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_6_change_cgb_c");
}

// ============================================================================
// CGB-D tests (reference: expected/CPU CGB D/)
// ============================================================================

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2347"]
fn test_m2_win_en_toggle_cgb_d() {
    let bytes = read_rom_from_zip("m2_win_en_toggle.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m2_win_en_toggle_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_bgp_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2348"]
fn test_m3_bgp_change_sprites_cgb_d() {
    let bytes = read_rom_from_zip("m3_bgp_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_bgp_change_sprites_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2349"]
fn test_m3_lcdc_bg_en_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_en_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2350"]
fn test_m3_lcdc_bg_map_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_bg_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_bg_map_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2351"]
fn test_m3_lcdc_obj_en_change_variant_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_en_change_variant.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_en_change_variant_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2352"]
fn test_m3_lcdc_obj_size_change_scx_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_obj_size_change_scx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_obj_size_change_scx_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2353"]
fn test_m3_lcdc_tile_sel_win_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_tile_sel_win_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_tile_sel_win_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc =
        run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_en_change_multiple_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2354"]
fn test_m3_lcdc_win_en_change_multiple_wx_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_win_en_change_multiple_wx.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(
        &mut gb,
        CYCLE_LIMIT,
        "m3_lcdc_win_en_change_multiple_wx_cgb_d",
    );
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2355"]
fn test_m3_lcdc_win_map_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_lcdc_win_map_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_lcdc_win_map_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2356"]
fn test_m3_obp0_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_obp0_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_obp0_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_high_5_bits_cgb_d() {
    let bytes = read_rom_from_zip("m3_scx_high_5_bits.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_high_5_bits_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2357"]
fn test_m3_scx_low_3_bits_cgb_d() {
    let bytes = read_rom_from_zip("m3_scx_low_3_bits.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scx_low_3_bits_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2358"]
fn test_m3_scy_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_scy_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_scy_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_cgb_d() {
    let bytes = read_rom_from_zip("m3_window_timing.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2359"]
fn test_m3_window_timing_wx_0_cgb_d() {
    let bytes = read_rom_from_zip("m3_window_timing_wx_0.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_window_timing_wx_0_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_wx_4_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_4_change_sprites_cgb_d() {
    let bytes = read_rom_from_zip("m3_wx_4_change_sprites.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_4_change_sprites_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_5_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_wx_5_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_5_change_cgb_d");
}

#[test]
#[ignore = "mealybug: PPU timing not yet accurate — tracked in #2360"]
fn test_m3_wx_6_change_cgb_d() {
    let bytes = read_rom_from_zip("m3_wx_6_change.gb");
    let mut gb = load_cgb_rom_from_bytes(&bytes, CgbModel::CgbD);
    let _crc = run_to_breakpoint_and_crc(&mut gb, CYCLE_LIMIT, "m3_wx_6_change_cgb_d");
}
