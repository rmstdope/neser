use super::gba_suite_runner::{
    ARMWRESTLER_TEST_PAGE_COUNT, MGBA_SUITE_COUNT, MGBA_SUITE_KEYS, Suite, VIDEO_TEST_NAMES,
    boot_mgba_suite, run_armwrestler, run_mgba_io_read_diagnostics,
    run_mgba_io_read_diagnostics_after_bios_intro, run_mgba_memory_diagnostics,
    run_mgba_memory_diagnostics_with_bios_path, run_mgba_suite, run_mgba_timing_diagnostics,
    run_mgba_video_tests, run_suite,
};
use crate::gba::bios::EMBEDDED_BIOS;
use crate::gba::integration_tests::gba_suite_runner::GBA_CYCLES_PER_FRAME;
use crate::platform::emulator::Emulator;
use std::collections::HashMap;
use std::io::Write;

const APPROVALS_FILE: &str = "src/gba/integration_tests/gba_suite_crc_approvals.txt";
const APPROVALS_RAW: &str = include_str!("gba_suite_crc_approvals.txt");

fn parse_hex_crc(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))?;
    if digits.len() != 8 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

fn load_approved_crcs() -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for (line_no, raw_line) in APPROVALS_RAW.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (suite, crc_text) = line.split_once('=').unwrap_or_else(|| {
            panic!(
                "invalid entry in {APPROVALS_FILE}:{}: expected <suite>=0x<CRC32>",
                line_no + 1
            )
        });
        let suite = suite.trim();
        let crc_text = crc_text.trim();
        let crc = parse_hex_crc(crc_text).unwrap_or_else(|| {
            panic!(
                "invalid CRC in {APPROVALS_FILE}:{}: expected 8 hex digits with 0x prefix, got '{}': {}",
                line_no + 1,
                crc_text,
                raw_line
            )
        });

        let previous = map.insert(suite.to_string(), crc);
        assert!(
            previous.is_none(),
            "duplicate suite entry '{}' in {}:{}",
            suite,
            APPROVALS_FILE,
            line_no + 1
        );
    }
    map
}

fn approved_crc_for_suite(suite: Suite) -> u32 {
    approved_crc_for_suite_key(suite.label())
}

fn approved_crc_for_suite_key(key: &str) -> u32 {
    let approvals = load_approved_crcs();
    *approvals.get(key).unwrap_or_else(|| {
        panic!(
            "missing approved CRC for suite '{}' in {}. Generate captures with NESER_CAPTURE_SCREEN=1 and add {}=0x........ after visual approval.",
            key, APPROVALS_FILE, key
        )
    })
}

fn assert_suite_passes_with_crc(suite: Suite) {
    let suite_label = suite.label();
    let expected_crc32 = approved_crc_for_suite(suite);
    let result = run_suite(suite);
    let ewram_info = result
        .ewram_dump
        .as_deref()
        .map(|d| format!("\n{d}"))
        .unwrap_or_default();
    assert!(
        result.passed,
        "{suite_label} suite failed: index={} reg={} pc=0x{:08X} cpsr=0x{:08X} thumb={} opcode=0x{:08X} fb_crc=0x{:08X} cycles={} exit={:?}{ewram_info}",
        result.failing_index,
        result.reg_name.unwrap_or("n/a"),
        result.pc,
        result.cpsr,
        result.thumb,
        result.opcode_at_pc,
        result.framebuffer_crc32,
        result.cycles,
        result.exit_reason
    );
    assert_eq!(
        result.framebuffer_crc32, expected_crc32,
        "{suite_label} suite framebuffer CRC mismatch: expected=0x{expected_crc32:08X} actual=0x{:08X}{ewram_info}",
        result.framebuffer_crc32
    );
}

#[test]
fn gba_suite_arm_rom_passes() {
    assert_suite_passes_with_crc(Suite::Arm);
}

#[test]
fn gba_suite_thumb_rom_passes() {
    assert_suite_passes_with_crc(Suite::Thumb);
}

#[test]
fn gba_suite_nes_rom_passes() {
    assert_suite_passes_with_crc(Suite::Nes);
}

#[test]
fn gba_suite_memory_rom_passes() {
    assert_suite_passes_with_crc(Suite::Memory);
}

#[test]
fn gba_suite_save_none_rom_passes() {
    assert_suite_passes_with_crc(Suite::SaveNone);
}

#[test]
fn gba_suite_save_sram_rom_passes() {
    assert_suite_passes_with_crc(Suite::SaveSram);
}

#[test]
fn gba_suite_save_flash64_rom_passes() {
    assert_suite_passes_with_crc(Suite::SaveFlash64);
}

#[test]
fn gba_suite_save_flash128_rom_passes() {
    assert_suite_passes_with_crc(Suite::SaveFlash128);
}

#[test]
fn gba_suite_ppu_hello_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuHello);
}

#[test]
fn gba_suite_ppu_shades_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuShades);
}

#[test]
fn gba_suite_ppu_stripes_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuStripes);
}

#[test]
fn gba_fuzzarm_data_processing_passes() {
    assert_suite_passes_with_crc(Suite::FuzzArmDataProcessing);
}

#[test]
fn gba_fuzzarm_any_passes() {
    assert_suite_passes_with_crc(Suite::FuzzArmAny);
}

#[test]
fn gba_fuzzthumb_data_processing_passes() {
    assert_suite_passes_with_crc(Suite::FuzzThumbDataProcessing);
}

#[test]
fn gba_fuzzthumb_any_passes() {
    assert_suite_passes_with_crc(Suite::FuzzThumbAny);
}

#[test]
fn gba_fuzzarm_mixed_passes() {
    assert_suite_passes_with_crc(Suite::FuzzArmMixed);
}

#[test]
fn gba_suite_armwrestler_passes() {
    let approvals = load_approved_crcs();
    let result = run_armwrestler();
    assert_eq!(
        result.page_crcs.len(),
        ARMWRESTLER_TEST_PAGE_COUNT,
        "expected {} test page CRCs but got {}",
        ARMWRESTLER_TEST_PAGE_COUNT,
        result.page_crcs.len()
    );
    for (i, &crc) in result.page_crcs.iter().enumerate() {
        let key = format!("armwrestler_page{i}");
        let expected = approvals.get(&key).unwrap_or_else(|| {
            panic!(
                "missing approved CRC for '{}' in {}. Run with NESER_CAPTURE_SCREEN=1 and add {}=0x{:08X}",
                key, APPROVALS_FILE, key, crc
            )
        });
        assert_eq!(
            crc, *expected,
            "armwrestler page {i} CRC mismatch: expected=0x{expected:08X} actual=0x{crc:08X}"
        );
    }
}

#[test]
fn gba_mgba_suite_passes() {
    let approvals = load_approved_crcs();
    let result = run_mgba_suite();
    assert_eq!(
        result.suite_crcs.len(),
        MGBA_SUITE_COUNT,
        "expected {} sub-suite CRCs but got {}",
        MGBA_SUITE_COUNT,
        result.suite_crcs.len()
    );
    let mut mismatches = Vec::new();
    for (i, &crc) in result.suite_crcs.iter().enumerate() {
        let key = MGBA_SUITE_KEYS[i];
        match approvals.get(key) {
            Some(&expected) if crc == expected => {}
            Some(&expected) => mismatches.push(format!(
                "{key}: expected=0x{expected:08X} actual=0x{crc:08X}"
            )),
            None => mismatches.push(format!("{key}: missing approval actual=0x{crc:08X}")),
        }
    }
    assert!(
        mismatches.is_empty(),
        "mGBA suite approval mismatches:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn gba_mgba_memory_diagnostics_reports_mgba_log() {
    let result = run_mgba_memory_diagnostics();

    assert_eq!(
        result.total_count,
        Some(1552),
        "raw mGBA Memory log: {:?}",
        result.raw_log
    );
    assert!(
        result.passed_count.is_some(),
        "mGBA Memory diagnostics should include a parsed pass count"
    );
    assert_eq!(
        result.passed_count, result.total_count,
        "mGBA Memory diagnostics should pass every test with the embedded BIOS.\nraw log:\n{}",
        result.raw_log
    );
    assert!(
        result.failures.is_empty(),
        "mGBA Memory diagnostics reported failures with the embedded BIOS: {:?}\nraw log:\n{}",
        result.failures,
        result.raw_log
    );
    assert!(
        result.raw_log.contains("Memory"),
        "mGBA Memory diagnostics should include the mGBA log, got: {:?}",
        result.raw_log
    );
    assert_eq!(
        result.framebuffer_crc32,
        approved_crc_for_suite_key("mgba_memory")
    );
}

#[test]
fn gba_mgba_io_read_diagnostics_passes_every_register_read() {
    let result = run_mgba_io_read_diagnostics();

    assert_eq!(
        result.total_count,
        Some(130),
        "raw mGBA I/O read log: {:?}",
        result.raw_log
    );
    assert!(
        result.passed_count.is_some(),
        "mGBA I/O read diagnostics should include a parsed pass count"
    );
    assert_eq!(
        result.passed_count, result.total_count,
        "mGBA I/O read diagnostics should pass every register read with the embedded BIOS.\nfailures: {:?}\nraw log:\n{}",
        result.failures, result.raw_log
    );
    assert!(
        result.failures.is_empty(),
        "mGBA I/O read diagnostics reported failures with the embedded BIOS: {:?}\nraw log:\n{}",
        result.failures,
        result.raw_log
    );
}

#[test]
fn gba_mgba_io_read_diagnostics_passes_after_bios_intro() {
    let result = run_mgba_io_read_diagnostics_after_bios_intro();

    assert_eq!(
        result.total_count,
        Some(130),
        "raw mGBA I/O read log after BIOS intro: {:?}",
        result.raw_log
    );
    assert_eq!(
        result.passed_count, result.total_count,
        "mGBA I/O read diagnostics should pass after the normal BIOS intro.\nfailures: {:?}\nraw log:\n{}",
        result.failures, result.raw_log
    );
    assert!(
        result.failures.is_empty(),
        "mGBA I/O read diagnostics reported failures after the normal BIOS intro: {:?}\nraw log:\n{}",
        result.failures,
        result.raw_log
    );
}

#[test]
fn gba_mgba_timing_diagnostics_passes_every_timing_case() {
    let result = run_mgba_timing_diagnostics();

    assert_eq!(
        result.total_count,
        Some(2020),
        "raw mGBA Timing log: {:?}",
        result.raw_log
    );
    assert!(
        result.passed_count.is_some(),
        "mGBA Timing diagnostics should include a parsed pass count"
    );
    assert_eq!(
        result.passed_count, result.total_count,
        "mGBA Timing diagnostics should pass every timing case with the embedded BIOS.\nfailures: {:?}\nraw log:\n{}",
        result.failures, result.raw_log
    );
    assert!(
        result.failures.is_empty(),
        "mGBA Timing diagnostics reported failures with the embedded BIOS: {:?}\nraw log:\n{}",
        result.failures,
        result.raw_log
    );
}

#[test]
fn gba_mgba_timing_diagnostics_passes_c_loop_prefetch_cases() {
    let result = run_mgba_timing_diagnostics();
    let c_loop_start = result
        .raw_log
        .find("Timing test: C loop")
        .expect("mGBA Timing log should include the C-loop test section");
    let c_loop_suffix = &result.raw_log[c_loop_start..];
    let c_loop_end = c_loop_suffix
        .find("Timing test: BIOS Division")
        .unwrap_or(c_loop_suffix.len());
    let c_loop_log = &c_loop_suffix[..c_loop_end];

    assert!(
        !c_loop_log.contains("FAIL:"),
        "mGBA Timing C-loop prefetch cases should pass.\nC-loop log:\n{}",
        c_loop_log
    );
}

#[test]
fn gba_mgba_timing_diagnostics_passes_trivial_internal_dma_start_cases() {
    let result = run_mgba_timing_diagnostics();
    let failing_cases = [
        "FAIL: Trivial DMA (16) ARM/IWRAM",
        "FAIL: Trivial DMA (16) Thumb/IWRAM",
        "FAIL: Trivial DMA (32) ARM/IWRAM",
        "FAIL: Trivial DMA (32) Thumb/IWRAM",
    ];

    for failing_case in failing_cases {
        assert!(
            !result.raw_log.contains(failing_case),
            "mGBA Timing internal DMA start case should pass: {failing_case}\nraw log:\n{}",
            result.raw_log
        );
    }
}

#[test]
fn gba_mgba_timing_diagnostics_passes_rom_dma_prefetch_cases() {
    let result = run_mgba_timing_diagnostics();
    let failing_prefixes = [
        "FAIL: Trivial DMA (16/ROM)",
        "FAIL: Trivial DMA (16/to ROM)",
        "FAIL: Trivial DMA (16/ROM to ROM)",
        "FAIL: Trivial DMA (32/from ROM)",
        "FAIL: Trivial DMA (32/to ROM)",
        "FAIL: Trivial DMA (32/ROM to ROM)",
        "FAIL: Short DMA (16/from ROM)",
        "FAIL: Short DMA (16/to ROM)",
        "FAIL: Short DMA (16/ROM to ROM)",
        "FAIL: Short DMA (32/from ROM)",
        "FAIL: Short DMA (32/to ROM)",
        "FAIL: Short DMA (32/ROM to ROM)",
    ];

    for failing_prefix in failing_prefixes {
        assert!(
            !result.raw_log.contains(failing_prefix),
            "mGBA Timing ROM-DMA prefetch case should pass: {failing_prefix}\nraw log:\n{}",
            result.raw_log
        );
    }
}

#[test]
fn gba_mgba_timing_diagnostics_passes_bios_division_cases() {
    let result = run_mgba_timing_diagnostics();
    let failing_prefixes = ["FAIL: BIOS Division ", "FAIL: BIOS Division 2 "];

    for failing_prefix in failing_prefixes {
        assert!(
            !result.raw_log.contains(failing_prefix),
            "mGBA Timing BIOS division case should pass: {failing_prefix}\nraw log:\n{}",
            result.raw_log
        );
    }
}

#[test]
fn gba_mgba_timing_diagnostics_passes_bios_sqrt_cases() {
    let result = run_mgba_timing_diagnostics();
    let failing_prefixes = [
        "FAIL: BIOS Sqrt ",
        "FAIL: BIOS Sqrt 2 ",
        "FAIL: BIOS Sqrt 3 ",
    ];

    for failing_prefix in failing_prefixes {
        assert!(
            !result.raw_log.contains(failing_prefix),
            "mGBA Timing BIOS sqrt case should pass: {failing_prefix}\nraw log:\n{}",
            result.raw_log
        );
    }
}

#[test]
fn gba_mgba_memory_proprietary_diagnostics_skip_without_bios_path() {
    let result = run_mgba_memory_diagnostics_with_bios_path(None).unwrap();

    assert!(result.is_none());
}

#[test]
fn gba_mgba_memory_proprietary_diagnostics_run_with_bios_path() {
    let mut bios_file = tempfile::NamedTempFile::new().unwrap();
    bios_file.write_all(EMBEDDED_BIOS).unwrap();

    let result = run_mgba_memory_diagnostics_with_bios_path(Some(bios_file.path())).unwrap();
    let result = result.expect("configured BIOS path should run diagnostics");

    assert_eq!(
        result.framebuffer_crc32,
        approved_crc_for_suite_key("mgba_memory")
    );
    assert_eq!(
        result.total_count,
        Some(1552),
        "raw mGBA Memory log: {:?}",
        result.raw_log
    );
}

#[test]
fn approvals_manifest_parses() {
    let approvals = load_approved_crcs();
    assert_eq!(approvals.get("arm"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("thumb"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("nes"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("memory"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("save_none"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("save_sram"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("save_flash64"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("save_flash128"), Some(&0x12FD_AE0B));
    assert_eq!(approvals.get("ppu_hello"), Some(&0x52F9_B8A4));
    assert_eq!(approvals.get("ppu_shades"), Some(&0x57EB_C4ED));
    assert_eq!(approvals.get("ppu_stripes"), Some(&0xFBAB_D04A));
    assert_eq!(approvals.get("fuzzarm_data_processing"), Some(&0x9A78_A7EB));
    assert_eq!(approvals.get("fuzzarm_any"), Some(&0x9A78_A7EB));
    assert_eq!(
        approvals.get("fuzzthumb_data_processing"),
        Some(&0x9A78_A7EB)
    );
    assert_eq!(approvals.get("fuzzthumb_any"), Some(&0x9A78_A7EB));
    assert_eq!(approvals.get("fuzzarm_mixed"), Some(&0x9A78_A7EB));
    assert_eq!(approvals.get("armwrestler_page0"), Some(&0xAA08_1A57));
    assert_eq!(approvals.get("armwrestler_page1"), Some(&0x7725_D68D));
    assert_eq!(approvals.get("armwrestler_page2"), Some(&0xF9C3_8336));
    assert_eq!(approvals.get("armwrestler_page3"), Some(&0x76CE_D72B));
    assert_eq!(approvals.get("armwrestler_page4"), Some(&0x6795_E0F8));
    assert_eq!(approvals.get("armwrestler_page5"), Some(&0xE215_C2B0));
    assert_eq!(approvals.get("armwrestler_page6"), Some(&0xA522_34A7));
    assert_eq!(approvals.get("armwrestler_page7"), Some(&0x562A_5C65));

    // mgba-emu/suite keys
    assert_eq!(approvals.get("mgba_memory"), Some(&0x61F6_5500));
    assert_eq!(approvals.get("mgba_io_read"), Some(&0x7D32_0909));
    assert_eq!(approvals.get("mgba_timing"), Some(&0xDEEF_A167));
    assert_eq!(approvals.get("mgba_timers"), Some(&0xCFAB_2DCC));
    assert_eq!(approvals.get("mgba_timer_irq"), Some(&0xD1FF_FC47));
    assert_eq!(approvals.get("mgba_shifter"), Some(&0x8B4A_12AA));
    assert_eq!(approvals.get("mgba_carry"), Some(&0xFD9E_45E6));
    assert_eq!(approvals.get("mgba_multiply_long"), Some(&0x6996_55AB));
    assert_eq!(approvals.get("mgba_bios_math"), Some(&0xA4E2_450F));
    assert_eq!(approvals.get("mgba_dma"), Some(&0x9090_0973));
    assert_eq!(approvals.get("mgba_sio_read"), Some(&0xF5D9_8687));
    assert_eq!(approvals.get("mgba_sio_timing"), Some(&0xD95A_CB03));
    assert_eq!(approvals.get("mgba_misc_edge"), Some(&0x36EC_DBF9));
    assert_eq!(approvals.get("mgba_video"), Some(&0xAB7E_C249));
}

/// Verify that the mgba-emu test suite ROM boots to its main menu and
/// responds to button input (DOWN moves the menu cursor).
///
/// The ROM's main loop relies on VBlankIntrWait (SWI 0x05) and the BIOS IRQ
/// dispatcher to pace itself and process input each frame. Without a working
/// enhanced BIOS, the CPU will either:
/// - Get trapped at the IRQ vector (0x18) when the first VBlank fires, or
/// - Race through the loop without proper frame-sync, ignoring input.
///
/// This test boots the ROM, waits for the menu, presses DOWN, and asserts
/// the screen changes (proving the ROM processed the input).
#[test]
fn mgba_suite_boots_to_menu() {
    let (mut gba, _rom) = boot_mgba_suite();

    let max_frames = 60;
    let mut cycles: u64 = 0;

    // Run 10 frames to let the menu render and stabilise.
    for _ in 0..10 {
        advance_one_frame(&mut gba, &mut cycles);
    }
    let menu_crc = gba.screen_crc32();

    // Press DOWN and hold for multiple frames.
    gba.set_joypad_button_states(1, 0x20); // DOWN = bit 5

    // Run frames with DOWN held until the screen changes.
    let mut crc_changed = false;
    for _ in 0..max_frames {
        advance_one_frame(&mut gba, &mut cycles);
        if gba.screen_crc32() != menu_crc {
            crc_changed = true;
            break;
        }
    }
    gba.set_joypad_button_states(1, 0x00);

    assert!(
        crc_changed,
        "mgba suite menu did not respond to DOWN button after {max_frames} frames ({cycles} cycles). \
         The BIOS IRQ dispatch or VBlankIntrWait may not be working correctly, \
         so the ROM cannot process input. CPU PC=0x{:08X}",
        gba.cpu_pc()
    );
}

fn advance_one_frame(gba: &mut crate::gba::Gba, cycles: &mut u64) {
    let budget = GBA_CYCLES_PER_FRAME * 2;
    let mut spent: u64 = 0;
    while spent < budget {
        let tick = gba.run_tick_for_tests() as u64;
        assert!(tick > 0, "CPU halted at PC=0x{:08X}", gba.cpu_pc());
        *cycles += tick;
        spent += tick;
        if gba.is_ready_to_render() {
            gba.clear_ready_to_render();
            return;
        }
    }
    panic!("no frame produced within cycle budget");
}

/// Run the mgba-emu/suite Video sub-tests and compare actual vs expected
/// framebuffers. Reports which of the 7 video tests produce correct output.
///
/// Set `NESER_CAPTURE_SCREEN=1` to save PNG screenshots for visual inspection.
#[test]
#[ignore] // Investigation test — run manually to check video correctness.
fn gba_mgba_video_comparison() {
    let result = run_mgba_video_tests();
    assert_eq!(
        result.tests.len(),
        7,
        "expected 7 video test results, got {}",
        result.tests.len()
    );
    println!("\n=== mgba-emu/suite Video Comparison Results ===\n");
    println!("| # | Test                        | Actual CRC | Expected CRC | Match |");
    println!("|---|-----------------------------|-----------:|-------------:|:-----:|");

    let pass_count = result.tests.iter().filter(|t| t.matches).count();
    for (i, test) in result.tests.iter().enumerate() {
        let status = if test.matches { "✓" } else { "✗" };
        println!(
            "| {} | {:<27} | 0x{:08X} | 0x{:08X}   | {}    |",
            i + 1,
            VIDEO_TEST_NAMES[i],
            test.actual_crc,
            test.expected_crc,
            status
        );
    }
    println!("\nTotal: {pass_count}/7 tests match");
    println!("Total cycles: {}", result.cycles);

    // Assert known-passing tests to catch regressions.
    let expected_passing = [0, 1, 2, 6]; // Basic Mode 3, Basic Mode 4, Degenerate OBJ transforms, Window offscreen reset
    for &idx in &expected_passing {
        assert!(
            result.tests[idx].matches,
            "regression: '{}' (test {}) should match but actual=0x{:08X} expected=0x{:08X}",
            VIDEO_TEST_NAMES[idx],
            idx + 1,
            result.tests[idx].actual_crc,
            result.tests[idx].expected_crc,
        );
    }
}
