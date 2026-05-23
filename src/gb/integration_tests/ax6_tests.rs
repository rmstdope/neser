//! Integration tests for ax6's split `rtc3test` MBC3 RTC test ROMs.
//!
//! The ROMs are vendored from GBEmulatorShootout under
//! `roms/gb/automated_tests/rtc3test/`. Baseline screenshots are captured to
//! `target/rtc3test-captures/` when `NESER_CAPTURE_SCREEN=1` is set.

use std::path::Path;

use super::helpers::{
    load_cgb_rom_with_model, load_gb_rom_with_model, run_one_frame, save_screen_png,
};
use crate::gb::bus::GbBus;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

const BOOT_FRAMES: u32 = 600;
const STABLE_FRAMES: u32 = 30;

#[derive(Clone, Copy, Debug)]
enum HardwareMode {
    Dmg,
    Cgb,
}

#[derive(Clone, Copy, Debug)]
struct Rtc3Rom {
    capture_name: &'static str,
    path: &'static str,
    min_result_frames: u32,
    max_result_frames: u32,
    expected_dmg_crc: u32,
    expected_cgb_crc: u32,
}

const BASIC_ROM: Rtc3Rom = Rtc3Rom {
    capture_name: "rtc3test_1",
    path: "roms/gb/automated_tests/rtc3test/rtc3test-1.gb",
    min_result_frames: 900,
    max_result_frames: 1_500,
    expected_dmg_crc: 0x4173_05B5,
    expected_cgb_crc: 0x26F6_0650,
};

const RANGE_ROM: Rtc3Rom = Rtc3Rom {
    capture_name: "rtc3test_2",
    path: "roms/gb/automated_tests/rtc3test/rtc3test-2.gb",
    min_result_frames: 300,
    max_result_frames: 900,
    expected_dmg_crc: 0,
    expected_cgb_crc: 0,
};

const SUB_SECOND_ROM: Rtc3Rom = Rtc3Rom {
    capture_name: "rtc3test_3",
    path: "roms/gb/automated_tests/rtc3test/rtc3test-3.gb",
    min_result_frames: 1_200,
    max_result_frames: 1_800,
    expected_dmg_crc: 0,
    expected_cgb_crc: 0,
};

#[test]
fn rtc3test_1_matches_reviewed_crc_on_dmg() {
    assert_rom_crc(BASIC_ROM, HardwareMode::Dmg);
}

#[test]
fn rtc3test_1_matches_reviewed_crc_on_cgb() {
    assert_rom_crc(BASIC_ROM, HardwareMode::Cgb);
}

#[test]
#[ignore = "visual FAIL in rtc3test Range DMG result screen (#2618)"]
fn rtc3test_2_matches_reviewed_crc_on_dmg() {
    assert_rom_crc(RANGE_ROM, HardwareMode::Dmg);
}

#[test]
#[ignore = "visual FAIL in rtc3test Range CGB result screen (#2618)"]
fn rtc3test_2_matches_reviewed_crc_on_cgb() {
    assert_rom_crc(RANGE_ROM, HardwareMode::Cgb);
}

#[test]
#[ignore = "visual FAIL in rtc3test Sub-second DMG result screen (#2619)"]
fn rtc3test_3_matches_reviewed_crc_on_dmg() {
    assert_rom_crc(SUB_SECOND_ROM, HardwareMode::Dmg);
}

#[test]
#[ignore = "visual FAIL in rtc3test Sub-second CGB result screen (#2619)"]
fn rtc3test_3_matches_reviewed_crc_on_cgb() {
    assert_rom_crc(SUB_SECOND_ROM, HardwareMode::Cgb);
}

fn assert_rom_crc(rom: Rtc3Rom, hardware: HardwareMode) {
    assert_rom_crc_with_capture(rom, hardware, should_capture_screen());
}

fn assert_rom_crc_with_capture(rom: Rtc3Rom, hardware: HardwareMode, capture_screen: bool) {
    let crc = run_rom_and_crc(rom, hardware, capture_screen);
    let expected_crc = expected_crc(rom, hardware);
    assert_eq!(
        crc, expected_crc,
        "{} {hardware:?} CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}",
        rom.capture_name
    );
}

fn expected_crc(rom: Rtc3Rom, hardware: HardwareMode) -> u32 {
    match hardware {
        HardwareMode::Dmg => rom.expected_dmg_crc,
        HardwareMode::Cgb => rom.expected_cgb_crc,
    }
}

fn should_capture_screen() -> bool {
    std::env::var_os("NESER_CAPTURE_SCREEN").is_some()
}

fn run_rom_and_crc(rom: Rtc3Rom, hardware: HardwareMode, capture_screen: bool) -> u32 {
    match hardware {
        HardwareMode::Dmg => {
            let mut gb = load_gb_rom_with_model(rom.path, DmgModel::DmgB);
            run_to_result_screen(&mut gb, rom, hardware);
            let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
            if capture_screen {
                capture_screen_result(&gb, rom.capture_name, hardware, crc);
            }
            crc
        }
        HardwareMode::Cgb => {
            let mut gb = load_cgb_rom_with_model(rom.path, CgbModel::CgbE);
            run_to_result_screen(&mut gb, rom, hardware);
            let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
            if capture_screen {
                capture_screen_result(&gb, rom.capture_name, hardware, crc);
            }
            crc
        }
    }
}

fn run_to_result_screen<B: GbBus>(gb: &mut Gb<B>, rom: Rtc3Rom, hardware: HardwareMode) {
    for _ in 0..BOOT_FRAMES {
        run_one_frame(gb);
    }

    let mut last_crc = gb.cpu.bus.ppu().screen_buffer().crc32();
    let mut stable_frames = 0;

    for frame in 0..rom.max_result_frames {
        run_one_frame(gb);
        let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
        if frame < rom.min_result_frames {
            last_crc = crc;
            stable_frames = 0;
            continue;
        }

        if crc == last_crc {
            stable_frames += 1;
            if stable_frames >= STABLE_FRAMES {
                return;
            }
        } else {
            last_crc = crc;
            stable_frames = 0;
        }
    }

    panic!(
        "{} did not reach a stable {hardware:?} result screen within {} frames",
        rom.capture_name, rom.max_result_frames
    );
}

fn capture_screen_result<B: GbBus>(
    gb: &Gb<B>,
    capture_name: &str,
    hardware: HardwareMode,
    crc: u32,
) {
    let dir = Path::new("target/rtc3test-captures");
    std::fs::create_dir_all(dir).expect("create rtc3test capture directory");
    let path = dir.join(format!("{capture_name}_{hardware:?}.png").to_ascii_lowercase());
    save_screen_png(gb, path.to_str().expect("valid rtc3test capture path"));
    println!("[rtc3test] {capture_name} {hardware:?}: CRC={crc:#010X}, PNG saved to {path:?}");
}
