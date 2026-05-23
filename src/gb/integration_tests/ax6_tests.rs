//! Integration tests for ax6's `rtc3test` MBC3 RTC test ROM.
//!
//! The v004 ROM is vendored at
//! `roms/gb/automated_tests/rtc3test/rtc3test.gb`.
//! Baseline screenshots are captured to `target/rtc3test-captures/` when
//! `NESER_CAPTURE_SCREEN=1` is set.

use std::path::Path;

use super::helpers::{load_gb_rom_with_model, run_one_frame, save_screen_png};
use crate::gb::GameBoy;
use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;
use crate::platform::app_context::AppContext;

const RTC3TEST_ROM: &str = "roms/gb/automated_tests/rtc3test/rtc3test.gb";
const RTC3TEST_V004_SIZE: u64 = 32 * 1024;
const BOOT_FRAMES: u32 = 600;
const PRESS_FRAMES: u32 = 10;
const RELEASE_FRAMES: u32 = 10;
const STABLE_FRAMES: u32 = 30;
const BUTTON_A: u8 = 0;
const BUTTON_DOWN: u8 = 5;
const P1_ACTION_SELECTED: u8 = 0x20;
const P1_DIRECTION_SELECTED: u8 = 0x10;
const BUTTON_SELECT_TIMEOUT_M_CYCLES: u64 = 1_000_000;

#[derive(Clone, Copy, Debug)]
struct Rtc3Suite {
    capture_name: &'static str,
    menu_down_presses: u8,
    min_result_frames: u32,
    max_result_frames: u32,
    expected_crc: u32,
}

const BASIC_SUITE: Rtc3Suite = Rtc3Suite {
    capture_name: "rtc3test_basic",
    menu_down_presses: 0,
    min_result_frames: 900,
    max_result_frames: 1_500,
    expected_crc: 0x750C_1951,
};

const RANGE_SUITE: Rtc3Suite = Rtc3Suite {
    capture_name: "rtc3test_range",
    menu_down_presses: 1,
    min_result_frames: 300,
    max_result_frames: 900,
    expected_crc: 0x866E_87C0,
};

const SUB_SECOND_SUITE: Rtc3Suite = Rtc3Suite {
    capture_name: "rtc3test_sub_second",
    menu_down_presses: 2,
    min_result_frames: 1_200,
    max_result_frames: 1_800,
    expected_crc: 0x1447_FA4A,
};

const CAPTURE_SUITES: &[Rtc3Suite] = &[BASIC_SUITE, RANGE_SUITE, SUB_SECOND_SUITE];

#[test]
fn rtc3test_v004_rom_is_vendored() {
    let metadata = std::fs::metadata(RTC3TEST_ROM)
        .unwrap_or_else(|err| panic!("rtc3test v004 ROM should exist at {RTC3TEST_ROM}: {err}"));

    assert_eq!(
        metadata.len(),
        RTC3TEST_V004_SIZE,
        "rtc3test v004 ROM should be the expected 32 KiB release asset"
    );
    assert!(
        Path::new(RTC3TEST_ROM).is_file(),
        "rtc3test v004 path should point to a file"
    );
}

#[test]
#[ignore = "capture helper - run manually with NESER_CAPTURE_SCREEN=1 for baseline review"]
fn capture_all_rtc3test_screenshots() {
    for suite in CAPTURE_SUITES {
        let gb = load_completed_suite(*suite);
        let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
        capture_screen_if_requested(&gb, suite.capture_name, crc);
    }
}

#[test]
fn rtc3test_basic_suite_matches_reviewed_crc() {
    assert_suite_crc(BASIC_SUITE);
}

#[test]
fn suite_assertions_capture_screenshots_when_enabled() {
    let capture_path = Path::new("target/rtc3test-captures/rtc3test_basic.png");
    let _ = std::fs::remove_file(capture_path);

    assert_suite_crc_with_capture(BASIC_SUITE, true);

    assert!(
        capture_path.is_file(),
        "suite assertion should write {capture_path:?} when NESER_CAPTURE_SCREEN is set"
    );
}

#[test]
fn rtc3test_cgb_auto_mode_reaches_visible_screen() {
    let rom = std::fs::read(RTC3TEST_ROM).expect("rtc3test ROM should be present");
    let mut gb = GameBoy::new(AppContext::new());
    gb.load_rom(&rom, RTC3TEST_ROM)
        .expect("rtc3test should load in GB auto mode");
    gb.reset(false);

    let mut cycles = 0u64;
    let max_cycles = u64::from(BOOT_FRAMES) * 35_112;
    while cycles < max_cycles {
        cycles += u64::from(gb.run_tick());
    }

    assert!(
        gb.screen_snapshot().iter().any(|channel| *channel != 0),
        "rtc3test is CGB-compatible and should not render as an all-black screen in native auto mode"
    );
}

#[test]
fn rtc3test_range_suite_matches_reviewed_crc() {
    assert_suite_crc(RANGE_SUITE);
}

#[test]
fn rtc3test_sub_second_suite_matches_reviewed_crc() {
    assert_suite_crc(SUB_SECOND_SUITE);
}

fn assert_suite_crc(suite: Rtc3Suite) {
    assert_suite_crc_with_capture(suite, should_capture_screen());
}

fn assert_suite_crc_with_capture(suite: Rtc3Suite, capture_screen: bool) {
    let gb = load_completed_suite(suite);
    let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
    if capture_screen {
        capture_screen_result(&gb, suite.capture_name, crc);
    }
    assert_eq!(
        crc, suite.expected_crc,
        "{} CRC mismatch: got {crc:#010X}, expected {:#010X}",
        suite.capture_name, suite.expected_crc
    );
}

fn should_capture_screen() -> bool {
    std::env::var_os("NESER_CAPTURE_SCREEN").is_some()
}

fn load_completed_suite(suite: Rtc3Suite) -> Gb<DmgBus> {
    let mut gb = load_gb_rom_with_model(RTC3TEST_ROM, DmgModel::DmgB);
    run_to_menu(&mut gb);
    select_suite(&mut gb, suite);
    wait_for_result_screen(&mut gb, suite);
    gb
}

fn run_to_menu(gb: &mut Gb<DmgBus>) {
    for _ in 0..BOOT_FRAMES {
        run_one_frame(gb);
    }
}

fn select_suite(gb: &mut Gb<DmgBus>, suite: Rtc3Suite) {
    for _ in 0..suite.menu_down_presses {
        press_button(gb, BUTTON_DOWN);
    }
    press_button(gb, BUTTON_A);
}

fn press_button(gb: &mut Gb<DmgBus>, button: u8) {
    wait_for_button_group_selection(gb, button);
    gb.cpu.bus.set_joypad_button(button, true);
    for _ in 0..PRESS_FRAMES {
        run_one_frame(gb);
    }
    gb.cpu.bus.set_joypad_button(button, false);
    for _ in 0..RELEASE_FRAMES {
        run_one_frame(gb);
    }
}

fn wait_for_button_group_selection(gb: &mut Gb<DmgBus>, button: u8) {
    let select_bit = if button <= 3 {
        P1_ACTION_SELECTED
    } else {
        P1_DIRECTION_SELECTED
    };
    let start_cycles = gb.cycles();

    while gb.cpu.bus.read(0xFF00) & select_bit != 0 {
        if gb.cycles().saturating_sub(start_cycles) >= BUTTON_SELECT_TIMEOUT_M_CYCLES {
            panic!("timed out waiting for joypad group selection before button {button}");
        }
        gb.step();
    }
}

fn wait_for_result_screen(gb: &mut Gb<DmgBus>, suite: Rtc3Suite) {
    let mut last_crc = gb.cpu.bus.ppu().screen_buffer().crc32();
    let mut stable_frames = 0;

    for frame in 0..suite.max_result_frames {
        run_one_frame(gb);
        let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
        if frame < suite.min_result_frames {
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
        "{} did not reach a stable result screen within {} frames",
        suite.capture_name, suite.max_result_frames
    );
}

fn capture_screen_if_requested(gb: &Gb<DmgBus>, capture_name: &str, crc: u32) {
    if should_capture_screen() {
        capture_screen_result(gb, capture_name, crc);
    }
}

fn capture_screen_result(gb: &Gb<DmgBus>, capture_name: &str, crc: u32) {
    let dir = Path::new("target/rtc3test-captures");
    std::fs::create_dir_all(dir).expect("create rtc3test capture directory");
    let path = dir.join(format!("{capture_name}.png"));
    save_screen_png(gb, path.to_str().expect("valid rtc3test capture path"));
    println!("[rtc3test] {capture_name}: CRC={crc:#010X}, PNG saved to {path:?}");
}
