use super::helpers::{
    load_cgb_rom, load_cgb_rom_with_model, load_gb_rom, load_gb_rom_with_model, run_frames_and_crc,
    run_one_frame, save_screen_png,
};
use crate::gb::bus::GbBus;
use crate::gb::model::{CgbModel, DmgModel};

const ACID_DIR: &str = "roms/gb/automated_tests/acid";
const WHICH_GB_DMG_FRAMES: u32 = 500;
const WHICH_GB_CGB_FRAMES: u32 = 100;

fn which_gb_path() -> String {
    format!("{ACID_DIR}/which.gb")
}

fn assert_which_gb_dmg_crc(model: DmgModel, label: &str, expected_crc: u32) {
    let mut gb = load_gb_rom_with_model(&which_gb_path(), model);
    let crc = run_frames_and_crc(&mut gb, WHICH_GB_DMG_FRAMES);

    assert_eq!(
        crc, expected_crc,
        "which.gb {label} frame CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}"
    );
}

fn assert_which_gb_cgb_crc(model: CgbModel, label: &str, expected_crc: u32) {
    let mut gb = load_cgb_rom_with_model(&which_gb_path(), model);
    let crc = run_frames_and_crc(&mut gb, WHICH_GB_CGB_FRAMES);

    assert_eq!(
        crc, expected_crc,
        "which.gb {label} frame CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}"
    );
}

fn save_which_gb_dmg_capture(model: DmgModel, label: &str, path: &str) {
    let mut gb = load_gb_rom_with_model(&which_gb_path(), model);
    for _ in 0..WHICH_GB_DMG_FRAMES {
        run_one_frame(&mut gb);
    }
    save_screen_png(&gb, path);
    println!(
        "{label} screenshot saved to {path}; CRC: {:#010X}",
        gb.cpu.bus.ppu().screen_buffer().crc32()
    );
}

fn save_which_gb_cgb_capture(model: CgbModel, label: &str, path: &str) {
    let mut gb = load_cgb_rom_with_model(&which_gb_path(), model);
    for _ in 0..WHICH_GB_CGB_FRAMES {
        run_one_frame(&mut gb);
    }
    save_screen_png(&gb, path);
    println!(
        "{label} screenshot saved to {path}; CRC: {:#010X}",
        gb.cpu.bus.ppu().screen_buffer().crc32()
    );
}

/// Validate that `dmg-acid2.gb` renders a frame matching the expected CRC.
///
/// Baseline CRC `0x52D18222` was visually confirmed against the published
/// dmg-acid2 DMG reference image (smiley face, correct OBJ priority, no moles visible).
#[test]
fn test_dmg_acid2_frame_matches_reference_crc() {
    let mut gb = load_gb_rom(&format!("{ACID_DIR}/dmg-acid2.gb"));

    // Run 500 frames to allow the scroll-animation boot ROM (~293 frames)
    // plus the ROM's test rendering to complete.
    let crc = run_frames_and_crc(&mut gb, 500);

    const EXPECTED_CRC: u32 = 0x52D1_8222;
    assert_eq!(
        crc, EXPECTED_CRC,
        "dmg-acid2 frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

/// Validate that `cgb-acid2.gbc` renders a frame matching the expected CRC.
///
/// Baseline CRC established after visual confirmation against the published
/// cgb-acid2 CGB reference image (color smiley face, correct CGB palette/priority).
/// Tracking issue: https://github.com/rmstdope/neser/issues/2099
#[test]
fn test_cgb_acid2_frame_matches_reference_crc() {
    let mut gb = load_cgb_rom(&format!("{ACID_DIR}/cgb-acid2.gbc"));

    // Run 200 frames to allow the ROM to render its test output.
    let crc = run_frames_and_crc(&mut gb, 200);
    const EXPECTED_CRC: u32 = 0x0E0B_C136;
    assert_eq!(
        crc, EXPECTED_CRC,
        "cgb-acid2 frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

/// Validate that `cgb-acid-hell.gbc` renders a frame matching the expected CRC.
///
/// Baseline CRC `0x1D55EC40` was captured from frame 200 and paired with the
/// saved `cgb-acid-hell-screenshot.png` output for manual reference inspection.
#[test]
fn test_cgb_acid_hell_frame_matches_reference_crc() {
    let mut gb = load_cgb_rom(&format!("{ACID_DIR}/cgb-acid-hell.gbc"));

    // Run 200 frames to allow the ROM to render its test output.
    let crc = run_frames_and_crc(&mut gb, 200);

    const EXPECTED_CRC: u32 = 0x1D55_EC40;
    assert_eq!(
        crc, EXPECTED_CRC,
        "cgb-acid-hell frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

/// Validate that GBEmulatorShootout `acid/which.gb` renders the expected DMG output.
///
/// Baseline CRC `0x72DF46F3` was captured from frame 500, which renders the
/// DMG-0-specific result.
#[test]
fn test_which_gb_frame_matches_reference_crc_on_dmg_0() {
    assert_which_gb_dmg_crc(DmgModel::Dmg0, "DMG-0", 0x72DF_46F3);
}

/// Validate that GBEmulatorShootout `acid/which.gb` renders the expected DMG output.
///
/// Baseline CRC `0x3BC7E8E8` was captured from frame 500, which renders:
/// "WHICH.GB V0.8", "SEEMS TO BE A...", "DMG-CPU A/B/C".
#[test]
fn test_which_gb_frame_matches_reference_crc_on_dmg_b() {
    assert_which_gb_dmg_crc(DmgModel::DmgB, "DMG-B", 0x3BC7_E8E8);
}

/// Validate that GBEmulatorShootout `acid/which.gb` renders the expected CGB output.
///
/// Baseline CRC `0xA4849AC3` was captured from frame 100, which renders the
/// CGB-0-specific result.
#[test]
fn test_which_gb_frame_matches_reference_crc_on_cgb_0() {
    assert_which_gb_cgb_crc(CgbModel::Cgb0, "CGB-0", 0xA484_9AC3);
}

/// Validate that GBEmulatorShootout `acid/which.gb` renders the expected CGB output.
///
/// Baseline CRC `0x930E6D5E` was captured from frame 100, which renders:
/// "WHICH.GB V0.8", "SEEMS TO BE A...", "CPU CGB C".
#[test]
fn test_which_gb_frame_matches_reference_crc_on_cgb_c() {
    assert_which_gb_cgb_crc(CgbModel::CgbC, "CGB-C", 0x930E_6D5E);
}

/// Validate that GBEmulatorShootout `acid/which.gb` renders the expected CGB output.
///
/// Baseline CRC `0x8DCFBF6E` was captured from frame 100, which renders:
/// "WHICH.GB V0.8", "SEEMS TO BE A...", "CPU CGB E".
#[test]
fn test_which_gb_frame_matches_reference_crc_on_cgb_e() {
    assert_which_gb_cgb_crc(CgbModel::CgbE, "CGB-E", 0x8DCF_BF6E);
}

/// Validate that GBEmulatorShootout `acid/which.gb` identifies CGB-D correctly.
///
/// Baseline CRC `0x07E4EFB1` was captured from frame 100, which renders:
/// "WHICH.GB V0.8", "SEEMS TO BE A...", "CPU CGB D".
#[test]
fn test_which_gb_frame_matches_reference_crc_on_cgb_d() {
    assert_which_gb_cgb_crc(CgbModel::CgbD, "CGB-D", 0x07E4_EFB1);
}

/// Screenshot helper: saves `dmg-acid2.gb` frame 500 to `dmg-acid2-screenshot.png`.
/// Run with `cargo test -- --ignored` for visual baseline inspection.
#[test]
#[ignore = "screenshot helper — run manually for visual baseline inspection"]
fn save_dmg_acid2_screenshot() {
    let mut gb = load_gb_rom(&format!("{ACID_DIR}/dmg-acid2.gb"));
    for _ in 0..500 {
        run_one_frame(&mut gb);
    }
    save_screen_png(&gb, "dmg-acid2-screenshot.png");
    println!("Screenshot saved to dmg-acid2-screenshot.png");
    println!("CRC: {:#010X}", gb.cpu.bus.ppu().screen_buffer().crc32());
}

/// Screenshot helper: saves `cgb-acid2.gbc` frame 200 to `cgb-acid2-screenshot.png`.
/// Run with `cargo test -- --ignored` for visual baseline inspection.
#[test]
#[ignore = "screenshot helper — run manually for visual baseline inspection"]
fn save_cgb_acid2_screenshot() {
    let mut gb = load_cgb_rom(&format!("{ACID_DIR}/cgb-acid2.gbc"));
    for _ in 0..200 {
        run_one_frame(&mut gb);
    }
    save_screen_png(&gb, "cgb-acid2-screenshot.png");
    println!("Screenshot saved to cgb-acid2-screenshot.png");
    println!("CRC: {:#010X}", gb.cpu.bus.ppu().screen_buffer().crc32());
}

/// Screenshot helper: saves `cgb-acid-hell.gbc` frame 200 to `cgb-acid-hell-screenshot.png`.
/// Run with `cargo test -- --ignored` to capture the visual baseline and CRC.
#[test]
#[ignore = "screenshot helper — run manually for visual baseline inspection"]
fn save_cgb_acid_hell_screenshot() {
    let mut gb = load_cgb_rom(&format!("{ACID_DIR}/cgb-acid-hell.gbc"));
    for _ in 0..200 {
        run_one_frame(&mut gb);
    }
    save_screen_png(&gb, "cgb-acid-hell-screenshot.png");
    println!("Screenshot saved to cgb-acid-hell-screenshot.png");
    println!("CRC: {:#010X}", gb.cpu.bus.ppu().screen_buffer().crc32());
}

/// Screenshot helper: saves `which.gb` DMG frame 500 and CGB frame 100 captures.
/// Run with `cargo test -- --ignored` to capture the visual baseline and CRCs.
#[test]
#[ignore = "screenshot helper - run manually for visual baseline inspection"]
fn save_which_gb_screenshots() {
    save_which_gb_dmg_capture(DmgModel::Dmg0, "DMG-0", "which-gb-dmg-0-screenshot.png");
    save_which_gb_dmg_capture(DmgModel::DmgB, "DMG-B", "which-gb-dmg-b-screenshot.png");
    save_which_gb_cgb_capture(CgbModel::Cgb0, "CGB-0", "which-gb-cgb-0-screenshot.png");
    save_which_gb_cgb_capture(CgbModel::CgbC, "CGB-C", "which-gb-cgb-c-screenshot.png");
    save_which_gb_cgb_capture(CgbModel::CgbD, "CGB-D", "which-gb-cgb-d-screenshot.png");
    save_which_gb_cgb_capture(CgbModel::CgbE, "CGB-E", "which-gb-cgb-e-screenshot.png");
}
