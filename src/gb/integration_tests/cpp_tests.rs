//! Integration tests for CasualPokePlayer GBEmulatorShootout test ROMs.
//!
//! The suite is vendored in `roms/gb/automated_tests/cpp/`. Active tests run
//! each scoped ROM for the Shootout runtime, and compare both DMG and CGB
//! outputs against the upstream reference PNGs. Set `NESER_CAPTURE_SCREEN=1` to
//! write screenshots to `target/cpp-captures/` while baselines are being
//! reviewed.

use std::path::Path;

use super::helpers::{
    decoded_png_rgb_crc, load_cgb_rom_with_model, load_gb_rom_with_model, run_frames_and_crc,
    save_screen_png,
};
use crate::gb::bus::GbBus;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

const CPP_DIR: &str = "roms/gb/automated_tests/cpp";
const RESULT_SCREEN_FRAMES: u32 = 300;
// This is intentionally not sourced from CGB boot ROM compatibility palettes:
// it normalizes CGB execution to the grayscale upstream DMG reference PNGs.
const UPSTREAM_REFERENCE_GREYSCALE_PALETTE: [u16; 4] = [0x7FFF, 0x56B5, 0x294A, 0x0000];

#[derive(Clone, Copy, Debug)]
enum HardwareMode {
    Dmg,
    Cgb,
}

#[derive(Clone, Copy, Debug)]
struct CppCase {
    capture_name: &'static str,
    rom_name: &'static str,
    reference_png_name: &'static str,
    hardware: HardwareMode,
    expected_screen_crc: u32,
    reference_png_crc: u32,
}

const RTC_INVALID_BANKS_DMG: CppCase = CppCase {
    capture_name: "rtc_invalid_banks_dmg",
    rom_name: "rtc-invalid-banks-test.gb",
    reference_png_name: "rtc-invalid-banks-test.png",
    hardware: HardwareMode::Dmg,
    expected_screen_crc: 0x51F5_D367,
    reference_png_crc: 0x51F5_D367,
};

const RTC_INVALID_BANKS_CGB: CppCase = CppCase {
    capture_name: "rtc_invalid_banks_cgb",
    rom_name: "rtc-invalid-banks-test.gb",
    reference_png_name: "rtc-invalid-banks-test.png",
    hardware: HardwareMode::Cgb,
    expected_screen_crc: 0x51F5_D367,
    reference_png_crc: 0x51F5_D367,
};

const LATCH_RTC_DMG: CppCase = CppCase {
    capture_name: "latch_rtc_dmg",
    rom_name: "latch-rtc-test.gb",
    reference_png_name: "latch-rtc-test.png",
    hardware: HardwareMode::Dmg,
    expected_screen_crc: 0xD4A8_D787,
    reference_png_crc: 0xD4A8_D787,
};

const LATCH_RTC_CGB: CppCase = CppCase {
    capture_name: "latch_rtc_cgb",
    rom_name: "latch-rtc-test.gb",
    reference_png_name: "latch-rtc-test.png",
    hardware: HardwareMode::Cgb,
    expected_screen_crc: 0xD4A8_D787,
    reference_png_crc: 0xD4A8_D787,
};

const RAMG_MBC3_DMG: CppCase = CppCase {
    capture_name: "ramg_mbc3_dmg",
    rom_name: "ramg-mbc3-test.gb",
    reference_png_name: "ramg-mbc3-test.png",
    hardware: HardwareMode::Dmg,
    expected_screen_crc: 0xCC80_B44B,
    reference_png_crc: 0xCC80_B44B,
};

const RAMG_MBC3_CGB: CppCase = CppCase {
    capture_name: "ramg_mbc3_cgb",
    rom_name: "ramg-mbc3-test.gb",
    reference_png_name: "ramg-mbc3-test.png",
    hardware: HardwareMode::Cgb,
    expected_screen_crc: 0xCC80_B44B,
    reference_png_crc: 0xCC80_B44B,
};

#[test]
fn rtc_invalid_banks_matches_shootout_reference_on_dmg() {
    assert_case_matches_reference_crc(RTC_INVALID_BANKS_DMG);
}

#[test]
fn rtc_invalid_banks_matches_shootout_reference_on_cgb() {
    assert_case_matches_reference_crc(RTC_INVALID_BANKS_CGB);
}

#[test]
fn latch_rtc_matches_shootout_reference_on_dmg() {
    assert_case_matches_reference_crc(LATCH_RTC_DMG);
}

#[test]
fn latch_rtc_matches_shootout_reference_on_cgb() {
    assert_case_matches_reference_crc(LATCH_RTC_CGB);
}

#[test]
fn ramg_mbc3_matches_shootout_reference_on_dmg() {
    assert_case_matches_reference_crc(RAMG_MBC3_DMG);
}

#[test]
fn ramg_mbc3_matches_shootout_reference_on_cgb() {
    assert_case_matches_reference_crc(RAMG_MBC3_CGB);
}

#[test]
fn rtc_invalid_banks_reference_png_matches_locked_crc() {
    assert_reference_png_matches_locked_crc(RTC_INVALID_BANKS_DMG);
}

#[test]
fn latch_rtc_reference_png_matches_locked_crc() {
    assert_reference_png_matches_locked_crc(LATCH_RTC_DMG);
}

#[test]
fn ramg_mbc3_reference_png_matches_locked_crc() {
    assert_reference_png_matches_locked_crc(RAMG_MBC3_DMG);
}

#[test]
#[ignore = "screenshot helper - run manually with NESER_CAPTURE_SCREEN=1 for baseline review"]
fn capture_cpp_screenshots() {
    for case in [
        RTC_INVALID_BANKS_DMG,
        RTC_INVALID_BANKS_CGB,
        LATCH_RTC_DMG,
        LATCH_RTC_CGB,
        RAMG_MBC3_DMG,
        RAMG_MBC3_CGB,
    ] {
        let crc = run_case_frames_and_crc(case, RESULT_SCREEN_FRAMES);
        println!(
            "[cpp] {} {:?} reference CRC={:#010X}",
            case.rom_name,
            case.hardware,
            decoded_png_rgb_crc(&reference_png_path(case.reference_png_name))
        );
        println!(
            "[cpp] {} {:?} frame {RESULT_SCREEN_FRAMES} CRC={crc:#010X}",
            case.rom_name, case.hardware
        );
    }
}

fn assert_case_matches_reference_crc(case: CppCase) {
    let crc = run_case_and_crc(case);
    assert_eq!(
        crc, case.expected_screen_crc,
        "{} {:?} frame {RESULT_SCREEN_FRAMES} CRC mismatch: got {crc:#010X}, expected {:#010X}",
        case.rom_name, case.hardware, case.expected_screen_crc
    );
}

fn assert_reference_png_matches_locked_crc(case: CppCase) {
    let crc = decoded_png_rgb_crc(&reference_png_path(case.reference_png_name));
    assert_eq!(
        crc, case.reference_png_crc,
        "{} reference PNG CRC mismatch: got {crc:#010X}, expected {:#010X}",
        case.reference_png_name, case.reference_png_crc
    );
}

fn run_case_and_crc(case: CppCase) -> u32 {
    run_case_frames_and_crc(case, RESULT_SCREEN_FRAMES)
}

fn run_case_frames_and_crc(case: CppCase, frames: u32) -> u32 {
    match case.hardware {
        HardwareMode::Dmg => {
            let mut gb = load_gb_rom_with_model(&rom_path(case.rom_name), DmgModel::DmgB);
            run_loaded_case_and_crc(&mut gb, case, frames)
        }
        HardwareMode::Cgb => {
            let mut gb = load_cgb_rom_with_model(&rom_path(case.rom_name), CgbModel::CgbE);
            // Normalize DMG-compat CGB colorization so extra CGB coverage can use
            // the same upstream PNG references as the official DMG Shootout rows.
            gb.cpu.bus.ppu.apply_dmg_compat_palettes(
                &UPSTREAM_REFERENCE_GREYSCALE_PALETTE,
                &UPSTREAM_REFERENCE_GREYSCALE_PALETTE,
                &UPSTREAM_REFERENCE_GREYSCALE_PALETTE,
            );
            run_loaded_case_and_crc(&mut gb, case, frames)
        }
    }
}

fn run_loaded_case_and_crc<B: GbBus>(gb: &mut Gb<B>, case: CppCase, frames: u32) -> u32 {
    let crc = run_frames_and_crc(gb, frames);
    capture_screen_if_requested(gb, case, frames, crc);
    crc
}

fn capture_screen_if_requested<B: GbBus>(gb: &Gb<B>, case: CppCase, frames: u32, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let dir = Path::new("target/cpp-captures");
    std::fs::create_dir_all(dir).expect("create cpp capture directory");
    let path = dir.join(format!("{}_frame_{frames}.png", case.capture_name));
    save_screen_png(gb, path.to_str().expect("valid cpp capture path"));
    println!(
        "[cpp] {} {:?} frame {frames}: CRC={crc:#010X}, PNG saved to {path:?}",
        case.rom_name, case.hardware
    );
}

fn rom_path(rom_name: &str) -> String {
    format!("{CPP_DIR}/{rom_name}")
}

fn reference_png_path(png_name: &str) -> std::path::PathBuf {
    Path::new(CPP_DIR).join(png_name)
}
