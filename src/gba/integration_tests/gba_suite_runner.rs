use crate::gba::Gba;
use crate::gba::bios::EMBEDDED_BIOS;
use crate::gba::cpu::bus::Bus;
use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use std::path::{Path, PathBuf};

const MAX_CYCLES: u64 = 480_000_000;
const IDLE_PROBE_STABLE_PC_THRESHOLD: u32 = 1;
// The suite ends in `idle: b idle` (ARM `b .`, opcode 0xEAFFFFFE).
const ARM_BRANCH_SELF_OPCODE: u32 = 0xEAFF_FFFE;
const THUMB_BRANCH_SELF_OPCODE: u16 = 0xE7FE;
// GBA nominal timing: 280_896 cycles per frame (~59.73 Hz).
pub(crate) const GBA_CYCLES_PER_FRAME: u64 = 280_896;
// Allow up to two frames while waiting for a fresh frame-ready edge after idle-loop detection.
const FRAME_SETTLE_MAX_CYCLES: u64 = GBA_CYCLES_PER_FRAME * 2;
pub const MGBA_MEMORY_PROPRIETARY_BIOS_ENV: &str = "NESER_GBA_PROPRIETARY_BIOS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    Arm,
    Thumb,
    Nes,
    Memory,
    SaveNone,
    SaveSram,
    SaveFlash64,
    SaveFlash128,
    PpuHello,
    PpuShades,
    PpuStripes,
    FuzzArmDataProcessing,
    FuzzArmAny,
    FuzzThumbDataProcessing,
    FuzzThumbAny,
    FuzzArmMixed,
    ArmWrestler,
    Mgba,
}

impl Suite {
    fn rom_path_str(self) -> &'static str {
        match self {
            Self::Arm => "roms/gba/automated_tests/gba-tests/arm/arm.gba",
            Self::Thumb => "roms/gba/automated_tests/gba-tests/thumb/thumb.gba",
            Self::Nes => "roms/gba/automated_tests/gba-tests/nes/nes.gba",
            Self::Memory => "roms/gba/automated_tests/gba-tests/memory/memory.gba",
            Self::SaveNone => "roms/gba/automated_tests/gba-tests/save/none.gba",
            Self::SaveSram => "roms/gba/automated_tests/gba-tests/save/sram.gba",
            Self::SaveFlash64 => "roms/gba/automated_tests/gba-tests/save/flash64.gba",
            Self::SaveFlash128 => "roms/gba/automated_tests/gba-tests/save/flash128.gba",
            Self::PpuHello => "roms/gba/automated_tests/gba-tests/ppu/hello.gba",
            Self::PpuShades => "roms/gba/automated_tests/gba-tests/ppu/shades.gba",
            Self::PpuStripes => "roms/gba/automated_tests/gba-tests/ppu/stripes.gba",
            Self::FuzzArmDataProcessing => {
                "roms/gba/automated_tests/FuzzARM/ARM_DataProcessing.gba"
            }
            Self::FuzzArmAny => "roms/gba/automated_tests/FuzzARM/ARM_Any.gba",
            Self::FuzzThumbDataProcessing => {
                "roms/gba/automated_tests/FuzzARM/THUMB_DataProcessing.gba"
            }
            Self::FuzzThumbAny => "roms/gba/automated_tests/FuzzARM/THUMB_Any.gba",
            Self::FuzzArmMixed => "roms/gba/automated_tests/FuzzARM/FuzzARM.gba",
            Self::ArmWrestler => "roms/gba/automated_tests/armwrestler/armwrestler-gba-fixed.gba",
            Self::Mgba => "roms/gba/automated_tests/mgba-emu-suite/suite.gba",
        }
    }

    fn rom_path(self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(self.rom_path_str())
    }

    fn result_register(self) -> Option<(usize, &'static str)> {
        match self {
            Self::Arm => Some((12, "r12")),
            Self::Thumb => Some((7, "r7")),
            Self::Nes => Some((12, "r12")),
            Self::Memory => Some((12, "r12")),
            Self::SaveNone => Some((12, "r12")),
            Self::SaveSram => Some((12, "r12")),
            Self::SaveFlash64 => Some((12, "r12")),
            Self::SaveFlash128 => Some((12, "r12")),
            Self::PpuHello => Some((12, "r12")),
            Self::PpuShades => Some((12, "r12")),
            Self::PpuStripes => Some((12, "r12")),
            Self::FuzzArmDataProcessing
            | Self::FuzzArmAny
            | Self::FuzzThumbDataProcessing
            | Self::FuzzThumbAny
            | Self::FuzzArmMixed
            | Self::ArmWrestler
            | Self::Mgba => None,
        }
    }

    fn capture_stem(self) -> &'static str {
        match self {
            Self::Arm => "arm",
            Self::Thumb => "thumb",
            Self::Nes => "nes",
            Self::Memory => "memory",
            Self::SaveNone => "save_none",
            Self::SaveSram => "save_sram",
            Self::SaveFlash64 => "save_flash64",
            Self::SaveFlash128 => "save_flash128",
            Self::PpuHello => "ppu_hello",
            Self::PpuShades => "ppu_shades",
            Self::PpuStripes => "ppu_stripes",
            Self::FuzzArmDataProcessing => "fuzzarm_data_processing",
            Self::FuzzArmAny => "fuzzarm_any",
            Self::FuzzThumbDataProcessing => "fuzzthumb_data_processing",
            Self::FuzzThumbAny => "fuzzthumb_any",
            Self::FuzzArmMixed => "fuzzarm_mixed",
            Self::ArmWrestler => "armwrestler",
            Self::Mgba => "mgba_suite",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        self.capture_stem()
    }

    pub(crate) fn is_fuzzarm(self) -> bool {
        matches!(
            self,
            Self::FuzzArmDataProcessing
                | Self::FuzzArmAny
                | Self::FuzzThumbDataProcessing
                | Self::FuzzThumbAny
                | Self::FuzzArmMixed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    IdleLoopDetected,
    ExceptionVectorTrap,
    CycleLimitReached,
    CartStopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteResult {
    pub passed: bool,
    pub failing_index: u32,
    pub cycles: u64,
    pub pc: u32,
    pub cpsr: u32,
    pub thumb: bool,
    pub opcode_at_pc: u32,
    pub framebuffer_crc32: u32,
    pub reg_name: Option<&'static str>,
    pub exit_reason: ExitReason,
    pub ewram_dump: Option<String>,
}

pub fn run_suite(suite: Suite) -> SuiteResult {
    let rom_path = suite.rom_path();
    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read suite ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    gba.bus_mut().load_bios(EMBEDDED_BIOS);
    gba.bus_mut().write8(0x03007FFC, 1); // Skip BIOS intro
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("gba-suite-rom"))
        .unwrap_or_else(|e| {
            panic!("failed to load suite ROM {}: {e}", rom_path.display());
        });

    let mut cycles = 0u64;
    let mut last_pc: Option<u32> = None;
    let mut stable_pc_count: u32 = 0;

    while cycles < MAX_CYCLES {
        let tick_cycles = gba.run_tick_for_tests() as u64;
        if tick_cycles == 0 {
            let pc = gba.cpu_pc();
            settle_framebuffer_for_result(&mut gba, ExitReason::CartStopped);
            let result = result_from_register(&mut gba, suite, cycles, pc, ExitReason::CartStopped);
            maybe_write_capture_png(&gba, suite, result.framebuffer_crc32);
            return result;
        }
        cycles += tick_cycles;

        let pc = gba.cpu_pc();
        if Some(pc) == last_pc {
            stable_pc_count = stable_pc_count.saturating_add(1);
        } else {
            stable_pc_count = 0;
            last_pc = Some(pc);
        }

        if stable_pc_count >= IDLE_PROBE_STABLE_PC_THRESHOLD {
            let is_idle = if gba.cpu_thumb() {
                let opcode = gba.bus_mut().peek16(pc);
                is_thumb_branch_to_self(opcode)
            } else {
                let opcode = gba.bus_mut().peek32(pc);
                is_arm_branch_to_self(opcode, pc)
            };

            if is_idle {
                let reason = if is_bios_exception_vector(pc) {
                    ExitReason::ExceptionVectorTrap
                } else {
                    ExitReason::IdleLoopDetected
                };
                settle_framebuffer_for_result(&mut gba, reason);
                let result = result_from_register(&mut gba, suite, cycles, pc, reason);
                maybe_write_capture_png(&gba, suite, result.framebuffer_crc32);
                return result;
            }
        }
    }

    let pc = gba.cpu_pc();
    settle_framebuffer_for_result(&mut gba, ExitReason::CycleLimitReached);
    let result = result_from_register(&mut gba, suite, cycles, pc, ExitReason::CycleLimitReached);
    maybe_write_capture_png(&gba, suite, result.framebuffer_crc32);
    result
}

fn result_from_register(
    gba: &mut Gba,
    suite: Suite,
    cycles: u64,
    pc: u32,
    exit_reason: ExitReason,
) -> SuiteResult {
    let (failing_index, reg_name) = match suite.result_register() {
        Some((reg_index, name)) => (gba.cpu_reg(reg_index), Some(name)),
        None => (0, None),
    };
    let cpsr = gba.cpu_cpsr();
    let thumb = gba.cpu_thumb();
    let opcode_at_pc = if thumb {
        gba.bus_mut().peek16(pc) as u32
    } else {
        gba.bus_mut().peek32(pc)
    };
    let framebuffer_crc32 = gba.screen_crc32();
    let passed = failing_index == 0 && exit_reason == ExitReason::IdleLoopDetected;

    // Always collect eWRAM diagnostics for FuzzARM suites. A FuzzARM
    // failure surfaces as a CRC mismatch (since result_register() is None,
    // `passed` is true whenever idle is detected). Including the dump
    // unconditionally ensures actionable diagnostics are always available.
    let ewram_dump = if suite.is_fuzzarm() {
        Some(dump_fuzzarm_ewram(gba))
    } else {
        None
    };

    SuiteResult {
        passed,
        failing_index,
        cycles,
        pc,
        cpsr,
        thumb,
        opcode_at_pc,
        framebuffer_crc32,
        reg_name,
        exit_reason,
        ewram_dump,
    }
}

fn settle_framebuffer_for_result(gba: &mut Gba, exit_reason: ExitReason) {
    if exit_reason != ExitReason::IdleLoopDetected {
        return;
    }

    // Wait for two frame-ready edges after idle detection:
    //
    // 1. The first completes whatever partial frame was in progress when the
    //    idle loop was detected. If VRAM changed mid-frame (e.g. "End of
    //    testing" was drawn after the PPU already rendered the text area),
    //    this frame's scanlines may contain stale pixels.
    //
    // 2. The second is a complete frame rendered entirely from the current
    //    (final) VRAM state, guaranteeing a clean capture.
    for _ in 0..2 {
        if gba.is_ready_to_render() {
            gba.clear_ready_to_render();
        }

        let mut settle_cycles = 0u64;
        while settle_cycles < FRAME_SETTLE_MAX_CYCLES {
            let tick_cycles = gba.run_tick_for_tests() as u64;
            if tick_cycles == 0 {
                return;
            }
            settle_cycles += tick_cycles;

            if gba.is_ready_to_render() {
                gba.clear_ready_to_render();
                break;
            }
        }
    }
}

fn maybe_write_capture_png(gba: &Gba, suite: Suite, framebuffer_crc32: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let path = capture_output_path(suite, framebuffer_crc32);
    let rgb = gba.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(&path, &rgb, Gba::SCREEN_WIDTH, Gba::SCREEN_HEIGHT);
    println!(
        "[gba-suite-capture] saved {} (crc=0x{:08X})",
        path.display(),
        framebuffer_crc32
    );
}

fn capture_output_path(suite: Suite, framebuffer_crc32: u32) -> PathBuf {
    let file_name = format!("{}_crc_{:08X}.png", suite.capture_stem(), framebuffer_crc32);
    PathBuf::from("target/gba_suite_checkpoints").join(file_name)
}

/// Parse FuzzARM eWRAM failure dump when a FuzzARM test fails.
///
/// FuzzARM dumps structured diagnostic data starting at eWRAM base (0x0200_0000):
/// - 1 word: 'AAAA' (ARM) or 'TTTT' (THUMB)
/// - 2 words: opcode description (12 ASCII chars padded with spaces)
/// - 1 word: padding
/// - 4 words: initial r0, r1, r2, CPSR
/// - 4 words: gotten r3, r4, 0, CPSR
/// - 4 words: expected r3, r4, 0, CPSR
fn dump_fuzzarm_ewram(gba: &mut Gba) -> String {
    const EWRAM_BASE: u32 = 0x0200_0000;

    let mode_word = gba.bus_mut().peek32(EWRAM_BASE);
    let mode = match &mode_word.to_le_bytes() {
        b"AAAA" => "ARM",
        b"TTTT" => "THUMB",
        _ => return format!("(no valid FuzzARM dump; mode word=0x{mode_word:08X})"),
    };

    // Read opcode description (2 words = 8 bytes) + 1 word padding = 12 chars total
    let op_word1 = gba.bus_mut().peek32(EWRAM_BASE + 4);
    let op_word2 = gba.bus_mut().peek32(EWRAM_BASE + 8);
    let op_word3 = gba.bus_mut().peek32(EWRAM_BASE + 12);
    let mut opcode_bytes = Vec::with_capacity(12);
    opcode_bytes.extend_from_slice(&op_word1.to_le_bytes());
    opcode_bytes.extend_from_slice(&op_word2.to_le_bytes());
    opcode_bytes.extend_from_slice(&op_word3.to_le_bytes());
    let opcode_str = String::from_utf8_lossy(&opcode_bytes).trim().to_string();

    // Initial values: r0, r1, r2, CPSR
    let init_r0 = gba.bus_mut().peek32(EWRAM_BASE + 16);
    let init_r1 = gba.bus_mut().peek32(EWRAM_BASE + 20);
    let init_r2 = gba.bus_mut().peek32(EWRAM_BASE + 24);
    let init_cpsr = gba.bus_mut().peek32(EWRAM_BASE + 28);

    // Gotten values: r3, r4, 0, CPSR
    let got_r3 = gba.bus_mut().peek32(EWRAM_BASE + 32);
    let got_r4 = gba.bus_mut().peek32(EWRAM_BASE + 36);
    let got_cpsr = gba.bus_mut().peek32(EWRAM_BASE + 44);

    // Expected values: r3, r4, 0, CPSR
    let exp_r3 = gba.bus_mut().peek32(EWRAM_BASE + 48);
    let exp_r4 = gba.bus_mut().peek32(EWRAM_BASE + 52);
    let exp_cpsr = gba.bus_mut().peek32(EWRAM_BASE + 60);

    format!(
        "FuzzARM {mode} failure: {opcode_str}\n\
         Initial: r0=0x{init_r0:08X} r1=0x{init_r1:08X} r2=0x{init_r2:08X} CPSR=0x{init_cpsr:08X}\n\
         Got:     r3=0x{got_r3:08X} r4=0x{got_r4:08X} CPSR=0x{got_cpsr:08X}\n\
         Expected:r3=0x{exp_r3:08X} r4=0x{exp_r4:08X} CPSR=0x{exp_cpsr:08X}"
    )
}

fn is_arm_branch_to_self(opcode: u32, pc: u32) -> bool {
    // Match unconditional ARM B (not BL) and compute branch target.
    if opcode >> 28 != 0xE {
        return false;
    }
    if (opcode & 0x0E00_0000) != 0x0A00_0000 {
        return false;
    }
    if (opcode & 0x0100_0000) != 0 {
        return false;
    }

    let imm24 = (opcode & 0x00FF_FFFF) as i32;
    let signed_imm24 = (imm24 << 8) >> 8;
    let offset = signed_imm24 << 2;
    let target = pc.wrapping_add(8).wrapping_add(offset as u32);
    target == pc
}

fn is_thumb_branch_to_self(opcode: u16) -> bool {
    // THUMB unconditional branch B: 1110_0<11-bit signed offset>
    // For b . (branch to self), PC = current + 4 + offset*2
    // offset = -2 halfwords → 11-bit two's complement = 0x7FE
    // Full opcode: 0xE7FE
    opcode == THUMB_BRANCH_SELF_OPCODE
}

fn is_bios_exception_vector(pc: u32) -> bool {
    const BIOS_EXCEPTION_VECTORS: [u32; 5] = [0x04, 0x0C, 0x10, 0x18, 0x1C];
    BIOS_EXCEPTION_VECTORS.contains(&pc)
}

// --- ArmWrestler-specific runner ---
//
// The armwrestler ROM presents a menu and requires button input to navigate
// through test pages. This runner injects button presses at frame boundaries
// to advance through all test pages, capturing the framebuffer CRC after each.
//
// Menu structure (6 groups):
//   Item 0: ARM ALU      → Test0, START chains: Test0→Test1→Test2→Test3→Test4→menu
//   Item 1: ARM LDR/STR  → Test2 (subset of above chain)
//   Item 2: ARM LDM/STM  → Test4 (subset of above chain)
//   Item 3: THUMB ALU    → _test0, START chains: _test0→_test1→_test2→menu
//   Item 4: THUMB LDR/STR→ _test1 (subset of above chain)
//   Item 5: THUMB LDM/STM→ _test2 (subset of above chain)
//
// We enter via ARM ALU (item 0) to cover all 5 ARM pages, return to menu,
// verify DOWN reaches all menu items, navigate back to THUMB ALU (item 3),
// then cover all 3 THUMB pages.

/// Total test pages validated.
pub const ARMWRESTLER_TEST_PAGE_COUNT: usize = 8;

/// Button bitmask for A (NES-convention bit 0).
const BTN_A: u8 = 0x01;
/// Button bitmask for B (NES-convention bit 1).
const BTN_B: u8 = 0x02;
/// Button bitmask for Start (NES-convention bit 3).
const BTN_START: u8 = 0x08;
/// Button bitmask for Up (NES-convention bit 4).
const BTN_UP: u8 = 0x10;
/// Button bitmask for Down (NES-convention bit 5).
const BTN_DOWN: u8 = 0x20;

/// Result of running the armwrestler ROM through all test pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmWrestlerResult {
    /// CRC32 of the framebuffer after each test page renders.
    /// Indices 0–4 are ARM pages; indices 5–7 are THUMB pages.
    pub page_crcs: Vec<u32>,
    /// Total cycles consumed.
    pub cycles: u64,
}

/// Run armwrestler-gba-fixed.gba, navigating through all ARM and THUMB test pages.
///
/// Returns one CRC per test page (5 ARM pages, then 3 THUMB pages).
pub fn run_armwrestler() -> ArmWrestlerResult {
    let rom_path = Suite::ArmWrestler.rom_path();

    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read armwrestler ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    gba.bus_mut().load_bios(EMBEDDED_BIOS);
    gba.bus_mut().write8(0x03007FFC, 1); // Skip BIOS intro
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("armwrestler"))
        .unwrap_or_else(|e| {
            panic!("failed to load armwrestler ROM {}: {e}", rom_path.display());
        });

    // Run frame-by-frame, injecting button presses to navigate tests.
    //
    // The ROM's edge-detection (XOR with previous state) requires a clean
    // release between successive presses.

    let mut cycles: u64 = 0;
    let mut page_crcs: Vec<u32> = Vec::new();

    // Boot: run frames until the menu is fully rendered and stable.
    // The ROM takes some frames to initialize; during this time the screen is black.
    // Wait for a non-black stable screen (same CRC for 5 consecutive frames, not the
    // blank-screen CRC).
    let mut menu_crc = 0u32;
    let mut stable = 0u32;
    for _ in 0..120 {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "armwrestler: halted during boot"
        );
        gba.clear_ready_to_render();
        let crc = gba.screen_crc32();
        if crc == menu_crc {
            stable += 1;
            if stable >= 5 {
                break;
            }
        } else {
            menu_crc = crc;
            stable = 1;
        }
    }
    // The menu CRC from boot might be an intermediate state (e.g., before cursor renders).
    // Run a few more frames to let the menu fully settle.
    for _ in 0..10 {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "armwrestler: halted during boot settle"
        );
        gba.clear_ready_to_render();
    }

    // Helper: press a button across two render edges (the runner starts at
    // VBlank, while the ROM samples input after its own VSync routine),
    // release, then allow several VBlanks for menu erasing/redrawing before
    // taking the framebuffer CRC.
    let press_and_wait = |gba: &mut Gba, cycles: &mut u64, button: u8, prev_crc: u32| -> u32 {
        for _attempt in 0..3 {
            gba.set_joypad_button_states(0, button);
            for _ in 0..2 {
                assert!(run_until_frame_ready(gba, cycles), "halted during press");
                gba.clear_ready_to_render();
            }
            gba.set_joypad_button_states(0, 0);
            for _ in 0..8 {
                assert!(
                    run_until_frame_ready(gba, cycles),
                    "halted waiting for redraw"
                );
                gba.clear_ready_to_render();
            }
            let crc = gba.screen_crc32();
            if crc != prev_crc {
                return crc;
            }
        }
        gba.screen_crc32()
    };

    // --- ARM test chain (pages 0–4): enter with START, advance with START ---

    // Armwrestler shows a title/splash that looks identical to the interactive menu.
    // Press START once to transition from title→menu (visual doesn't change), then
    // the next START enters the selected test.
    // Use a 1-frame hold here to avoid dismissing the title and entering the
    // first test with one long START press.
    gba.set_joypad_button_states(0, BTN_START);
    assert!(
        run_until_frame_ready(&mut gba, &mut cycles),
        "halted during title dismiss"
    );
    gba.clear_ready_to_render();
    gba.set_joypad_button_states(0, 0);
    // Wait for the menu to become interactive
    for _ in 0..10 {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "halted after title dismiss"
        );
        gba.clear_ready_to_render();
    }
    let mut current_crc = gba.screen_crc32();

    // Enter ARM ALU (menu item 0 is already selected)
    current_crc = press_and_wait(&mut gba, &mut cycles, BTN_START, current_crc);
    page_crcs.push(current_crc);
    maybe_write_armwrestler_png(&gba, 0, current_crc);

    for page_idx in 1..5 {
        current_crc = press_and_wait(&mut gba, &mut cycles, BTN_START, current_crc);
        page_crcs.push(current_crc);
        maybe_write_armwrestler_png(&gba, page_idx, current_crc);
    }

    // Return to the menu, verify DOWN can reach every menu item, navigate
    // back to THUMB ALU, then cover all 3 THUMB pages.
    current_crc = press_and_wait(&mut gba, &mut cycles, BTN_START, current_crc);
    assert_eq!(armwrestler_cursor(&mut gba), 0);
    for expected in 1..=5 {
        current_crc = press_and_wait(&mut gba, &mut cycles, BTN_DOWN, current_crc);
        assert_eq!(armwrestler_cursor(&mut gba), expected);
    }
    for expected in (3..=4).rev() {
        current_crc = press_and_wait(&mut gba, &mut cycles, BTN_UP, current_crc);
        assert_eq!(armwrestler_cursor(&mut gba), expected);
    }
    current_crc = press_and_wait(&mut gba, &mut cycles, BTN_START, current_crc);
    page_crcs.push(current_crc);
    maybe_write_armwrestler_png(&gba, 5, current_crc);

    for page_idx in 6..8 {
        current_crc = press_and_wait(&mut gba, &mut cycles, BTN_START, current_crc);
        page_crcs.push(current_crc);
        maybe_write_armwrestler_png(&gba, page_idx, current_crc);
    }

    ArmWrestlerResult { page_crcs, cycles }
}

fn armwrestler_cursor(gba: &mut Gba) -> u8 {
    (gba.bus_mut().peek32(0x0300_0010) & 0xFF) as u8
}

/// Advance emulation until the next VBlank (frame-ready edge).
///
/// Returns `true` if a fresh frame was produced, `false` if the CPU halted
/// (`tick == 0`) or the cycle budget was exhausted without a frame-ready edge.
fn run_until_frame_ready(gba: &mut Gba, cycles: &mut u64) -> bool {
    let max_cycle_budget = GBA_CYCLES_PER_FRAME * 2;
    let mut spent: u64 = 0;
    while spent < max_cycle_budget {
        let tick = gba.run_tick_for_tests() as u64;
        if tick == 0 {
            return false;
        }
        *cycles += tick;
        spent += tick;
        if gba.is_ready_to_render() {
            return true;
        }
    }
    false
}

// --- mgba-emu/suite runner ---
//
// The mgba-emu test suite is a single interactive ROM with 14 sub-suites
// navigated via menu (UP/DOWN to select, A to enter, B to go back).
// This runner boots the ROM, enters each sub-suite sequentially, waits
// for results to stabilise, captures the framebuffer CRC, and returns.
//
// Sub-suite order (menu index 0–13):
//   0: Memory, 1: I/O read, 2: Timing, 3: Timers, 4: Timer IRQ,
//   5: Shifter, 6: Carry, 7: Multiply long, 8: BIOS math, 9: DMA,
//   10: SIO read, 11: SIO timing, 12: Misc. edge cases, 13: Video

/// Total number of sub-suites in the mgba-emu test suite.
pub const MGBA_SUITE_COUNT: usize = 14;

/// CRC approval keys for each mgba sub-suite, in menu order.
pub const MGBA_SUITE_KEYS: [&str; MGBA_SUITE_COUNT] = [
    "mgba_memory",
    "mgba_io_read",
    "mgba_timing",
    "mgba_timers",
    "mgba_timer_irq",
    "mgba_shifter",
    "mgba_carry",
    "mgba_multiply_long",
    "mgba_bios_math",
    "mgba_dma",
    "mgba_sio_read",
    "mgba_sio_timing",
    "mgba_misc_edge",
    "mgba_video",
];

/// Result of running all mgba-emu sub-suites sequentially.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgbaSuiteResult {
    /// CRC32 of the framebuffer after each sub-suite completes (14 entries).
    pub suite_crcs: Vec<u32>,
    /// Total cycles consumed.
    pub cycles: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgbaMemoryFailure {
    pub test_name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgbaMemoryLog {
    pub raw_log: String,
    pub passed_count: Option<u32>,
    pub total_count: Option<u32>,
    pub failures: Vec<MgbaMemoryFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgbaMemoryDiagnosticResult {
    pub framebuffer_crc32: u32,
    pub passed_count: Option<u32>,
    pub total_count: Option<u32>,
    pub raw_log: String,
    pub failures: Vec<MgbaMemoryFailure>,
}

pub fn mgba_memory_diagnostic_from_sram(
    framebuffer_crc32: u32,
    sram: &[u8],
) -> MgbaMemoryDiagnosticResult {
    mgba_memory_diagnostic_from_log(framebuffer_crc32, parse_mgba_memory_sram_log(sram))
}

fn mgba_memory_diagnostic_from_log(
    framebuffer_crc32: u32,
    log: MgbaMemoryLog,
) -> MgbaMemoryDiagnosticResult {
    MgbaMemoryDiagnosticResult {
        framebuffer_crc32,
        passed_count: log.passed_count,
        total_count: log.total_count,
        raw_log: log.raw_log,
        failures: log.failures,
    }
}

fn mgba_memory_diagnostic_from_gba(gba: &Gba) -> MgbaMemoryDiagnosticResult {
    let log = parse_mgba_memory_sram_log(gba.bus().mgba_log_snapshot().as_bytes());
    mgba_memory_diagnostic_from_log(gba.screen_crc32(), log)
}

fn mgba_io_read_diagnostic_from_gba(gba: &Gba) -> MgbaMemoryDiagnosticResult {
    let log = parse_mgba_sub_suite_log(
        gba.bus().mgba_log_snapshot().as_bytes(),
        gba.bus().sram_snapshot(),
        "I/O read tests",
    );
    mgba_memory_diagnostic_from_log(gba.screen_crc32(), log)
}

pub fn run_mgba_memory_diagnostics() -> MgbaMemoryDiagnosticResult {
    let (gba, _rom) = boot_mgba_suite();
    run_mgba_memory_diagnostics_from_gba(gba)
}

pub fn run_mgba_io_read_diagnostics() -> MgbaMemoryDiagnosticResult {
    let (gba, _rom) = boot_mgba_suite();
    run_mgba_sub_suite_diagnostics_from_gba(gba, 1, "I/O read diagnostics")
}

pub fn run_mgba_io_read_diagnostics_after_bios_intro() -> MgbaMemoryDiagnosticResult {
    let (gba, _rom) = boot_mgba_suite_without_bios_intro_skip();
    run_mgba_sub_suite_diagnostics_from_gba_with_boot_frames(
        gba,
        1,
        "I/O read diagnostics after BIOS intro",
        300,
    )
}

fn run_mgba_memory_diagnostics_from_gba(gba: Gba) -> MgbaMemoryDiagnosticResult {
    run_mgba_sub_suite_diagnostics_from_gba(gba, 0, "mgba Memory diagnostics")
}

fn run_mgba_sub_suite_diagnostics_from_gba(
    gba: Gba,
    suite_index: usize,
    label: &str,
) -> MgbaMemoryDiagnosticResult {
    run_mgba_sub_suite_diagnostics_from_gba_with_boot_frames(gba, suite_index, label, 10)
}

fn run_mgba_sub_suite_diagnostics_from_gba_with_boot_frames(
    mut gba: Gba,
    suite_index: usize,
    label: &str,
    boot_frames: u32,
) -> MgbaMemoryDiagnosticResult {
    let mut cycles: u64 = 0;

    for _ in 0..boot_frames {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "{label}: timed out during initial boot"
        );
        gba.clear_ready_to_render();
    }

    for _ in 0..suite_index {
        press_button(&mut gba, &mut cycles, BTN_DOWN);
    }

    press_button(&mut gba, &mut cycles, BTN_A);

    const STABLE_FRAMES: u32 = 30;
    const MAX_SUITE_FRAMES: u32 = 2000;

    assert!(
        run_until_frame_ready(&mut gba, &mut cycles),
        "{label}: timed out getting initial frame"
    );
    gba.clear_ready_to_render();
    let initial_crc = gba.screen_crc32();

    let mut prev_crc: u32 = initial_crc;
    let mut stable_count: u32 = 1;
    let mut saw_change_from_initial = false;

    for frame in 1..MAX_SUITE_FRAMES {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "{label}: timed out at frame {frame}"
        );
        gba.clear_ready_to_render();

        let crc = gba.screen_crc32();
        if !saw_change_from_initial {
            if crc != initial_crc {
                saw_change_from_initial = true;
                prev_crc = crc;
                stable_count = 1;
            }
        } else if crc == prev_crc {
            stable_count += 1;
            if stable_count >= STABLE_FRAMES {
                break;
            }
        } else {
            prev_crc = crc;
            stable_count = 1;
        }
    }

    if suite_index == 1 {
        mgba_io_read_diagnostic_from_gba(&gba)
    } else {
        mgba_memory_diagnostic_from_gba(&gba)
    }
}

pub fn run_mgba_memory_diagnostics_with_bios_path(
    bios_path: Option<&Path>,
) -> Result<Option<MgbaMemoryDiagnosticResult>, String> {
    let Some(bios_path) = bios_path else {
        return Ok(None);
    };

    let bios = std::fs::read(bios_path)
        .map_err(|e| format!("failed to read GBA BIOS image {}: {e}", bios_path.display()))?;
    if bios.len() != crate::gba::bus::memory::BIOS_SIZE {
        return Err(format!(
            "GBA BIOS image {} has invalid size: expected {} bytes, found {}",
            bios_path.display(),
            crate::gba::bus::memory::BIOS_SIZE,
            bios.len()
        ));
    }

    let (gba, _rom) = boot_mgba_suite_with_bios(&bios);
    Ok(Some(run_mgba_memory_diagnostics_from_gba(gba)))
}

pub fn run_mgba_memory_diagnostics_with_proprietary_bios()
-> Result<Option<MgbaMemoryDiagnosticResult>, String> {
    let Some(path) = std::env::var_os(MGBA_MEMORY_PROPRIETARY_BIOS_ENV) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    run_mgba_memory_diagnostics_with_bios_path(Some(&path))
}

pub fn parse_mgba_memory_sram_log(bytes: &[u8]) -> MgbaMemoryLog {
    let raw_log = extract_mgba_log_text(bytes);
    let (passed_count, total_count) = parse_mgba_score(&raw_log);
    let failures = raw_log
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("fail"))
        .map(parse_mgba_failure_line)
        .collect();

    MgbaMemoryLog {
        raw_log,
        passed_count,
        total_count,
        failures,
    }
}

fn parse_mgba_sub_suite_log(
    score_bytes: &[u8],
    failure_bytes: &[u8],
    suite_name: &str,
) -> MgbaMemoryLog {
    let score_log = extract_mgba_debug_suite_log(score_bytes, suite_name);
    let failure_log = extract_ascii_segments(failure_bytes)
        .into_iter()
        .filter(|segment| segment.to_ascii_lowercase().contains("fail"))
        .collect::<Vec<_>>()
        .join("\n");
    let raw_log = [score_log.as_str(), failure_log.as_str()]
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let (passed_count, total_count) = parse_mgba_score(&score_log);
    let failures = failure_log
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("fail"))
        .map(parse_mgba_failure_line)
        .collect();

    MgbaMemoryLog {
        raw_log,
        passed_count,
        total_count,
        failures,
    }
}

fn extract_mgba_log_text(bytes: &[u8]) -> String {
    extract_ascii_segments(bytes)
        .into_iter()
        .find(|segment| segment.contains("Memory") && parse_mgba_score(segment).0.is_some())
        .unwrap_or_default()
}

fn extract_mgba_debug_suite_log(bytes: &[u8], suite_name: &str) -> String {
    let text = String::from_utf8_lossy(bytes);
    let Some(begin) = text.find(&format!("BEGIN: {suite_name}")) else {
        return String::new();
    };
    let suffix = &text[begin..];
    let Some(end) = suffix.find("END: ") else {
        return suffix.to_string();
    };
    let end_suffix = &suffix[end..];
    let score_end = end_suffix
        .find(|ch: char| {
            !(ch.is_ascii_digit()
                || ch == '/'
                || ch == 'E'
                || ch == 'N'
                || ch == 'D'
                || ch == ':'
                || ch == ' ')
        })
        .unwrap_or(end_suffix.len());
    suffix[..end + score_end].to_string()
}

fn extract_ascii_segments(bytes: &[u8]) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = Vec::new();

    for &byte in bytes {
        if byte.is_ascii_graphic() || matches!(byte, b' ' | b'\n' | b'\r' | b'\t') {
            current.push(byte);
        } else if !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }

    segments
        .into_iter()
        .map(|segment| {
            String::from_utf8_lossy(&segment)
                .trim_matches(['\r', '\n', '\t', ' '])
                .to_string()
        })
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn parse_mgba_score(raw_log: &str) -> (Option<u32>, Option<u32>) {
    for token in raw_log.split(|ch: char| !(ch.is_ascii_digit() || ch == '/')) {
        if let Some((passed, total)) = token.split_once('/') {
            let Ok(passed) = passed.parse::<u32>() else {
                continue;
            };
            let Ok(total) = total.parse::<u32>() else {
                continue;
            };
            return (Some(passed), Some(total));
        }
    }

    (None, None)
}

fn parse_mgba_failure_line(line: &str) -> MgbaMemoryFailure {
    if let Some((test_name, detail)) = line.split_once(':') {
        MgbaMemoryFailure {
            test_name: test_name.trim().to_string(),
            detail: detail.trim().to_string(),
        }
    } else {
        MgbaMemoryFailure {
            test_name: line.trim().to_string(),
            detail: String::new(),
        }
    }
}

/// Boot the mgba-emu test suite ROM with the embedded BIOS.
///
/// Returns a `Gba` instance ready to run, positioned at the cartridge
/// entrypoint with stack pointers initialised by the BIOS boot sequence.
pub fn boot_mgba_suite() -> (Gba, Vec<u8>) {
    boot_mgba_suite_with_bios(EMBEDDED_BIOS)
}

fn boot_mgba_suite_with_bios(bios: &[u8]) -> (Gba, Vec<u8>) {
    boot_mgba_suite_with_bios_intro_skip(bios, true)
}

fn boot_mgba_suite_without_bios_intro_skip() -> (Gba, Vec<u8>) {
    boot_mgba_suite_with_bios_intro_skip(EMBEDDED_BIOS, false)
}

fn boot_mgba_suite_with_bios_intro_skip(bios: &[u8], skip_intro: bool) -> (Gba, Vec<u8>) {
    let rom_path = Suite::Mgba.rom_path();
    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read mgba suite ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    gba.bus_mut().load_bios(bios);
    if skip_intro {
        gba.bus_mut().write8(0x03007FFC, 1); // Skip BIOS intro
    }
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("mgba-emu-suite"))
        .unwrap_or_else(|e| {
            panic!("failed to load mgba suite ROM {}: {e}", rom_path.display());
        });

    (gba, rom)
}

/// Run the mgba-emu test suite ROM through all 14 sub-suites.
///
/// Navigation protocol:
/// - Boot → 10 frames to render menu (cursor starts at index 0)
/// - For each sub-suite i (0..14):
///   - Press DOWN once to advance cursor (except suite 0)
///   - Press A to enter
///   - Wait for suite to finish: detect screen CRC change from initial state,
///     then wait for long stability (suite idle loop renders same frame)
///   - Capture CRC
///   - Press B to return to menu (retry if B doesn't register)
///   - Wait for menu to re-render
pub fn run_mgba_suite() -> MgbaSuiteResult {
    let (mut gba, _rom) = boot_mgba_suite();

    let mut cycles: u64 = 0;
    let mut suite_crcs: Vec<u32> = Vec::with_capacity(MGBA_SUITE_COUNT);

    // Boot: let the menu render.
    for _ in 0..10 {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "mgba suite: timed out during initial boot"
        );
        gba.clear_ready_to_render();
    }

    #[allow(clippy::needless_range_loop)]
    for suite_idx in 0..MGBA_SUITE_COUNT {
        // Navigate: cursor stays at previous position, press DOWN once.
        if suite_idx > 0 {
            press_button(&mut gba, &mut cycles, BTN_DOWN);
        }

        // Capture menu CRC before entering (for exit verification).
        let menu_crc = gba.screen_crc32();

        // Press A to enter the sub-suite.
        press_button(&mut gba, &mut cycles, BTN_A);

        // Wait for the sub-suite to finish running its tests.
        //
        // Strategy: capture the CRC of the first frame after entering (the
        // "initial" screen, often "Testing..."). Then wait for the screen to
        // CHANGE from that initial state (indicating tests are producing output).
        // Once a change is seen, wait for STABLE_FRAMES consecutive identical
        // frames — this means the suite has finished and entered its idle loop.
        // If the screen never changes, timeout at MAX_SUITE_FRAMES and capture
        // the initial screen (suite is stuck or produces no visible output).
        const STABLE_FRAMES: u32 = 30;
        const MAX_SUITE_FRAMES: u32 = 2000;

        // Get the initial frame CRC (the "entering suite" screen).
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "mgba suite '{}': timed out getting initial frame",
            MGBA_SUITE_KEYS[suite_idx]
        );
        gba.clear_ready_to_render();
        let initial_crc = gba.screen_crc32();

        let mut prev_crc: u32 = initial_crc;
        let mut stable_count: u32 = 1;
        let mut saw_change_from_initial = false;

        for frame in 1..MAX_SUITE_FRAMES {
            assert!(
                run_until_frame_ready(&mut gba, &mut cycles),
                "mgba suite '{}': timed out at frame {frame}",
                MGBA_SUITE_KEYS[suite_idx]
            );
            gba.clear_ready_to_render();

            let crc = gba.screen_crc32();

            if !saw_change_from_initial {
                // Still showing the initial screen — wait for it to change.
                if crc != initial_crc {
                    saw_change_from_initial = true;
                    prev_crc = crc;
                    stable_count = 1;
                }
            } else {
                // We've left the initial screen; now wait for stability.
                if crc == prev_crc {
                    stable_count += 1;
                    if stable_count >= STABLE_FRAMES {
                        break;
                    }
                } else {
                    prev_crc = crc;
                    stable_count = 1;
                }
            }
        }

        let crc = gba.screen_crc32();
        maybe_write_mgba_png(&gba, suite_idx, crc);
        suite_crcs.push(crc);

        // Press B to return to the menu. Retry if the suite hasn't finished
        // yet (B only works in the idle loop after all tests complete).
        // Verify return by checking if the screen matches the menu CRC.
        const MAX_B_RETRIES: u32 = 20;
        for attempt in 0..MAX_B_RETRIES {
            press_button(&mut gba, &mut cycles, BTN_B);

            // Give a few frames for the menu to re-render.
            for _ in 0..5 {
                assert!(
                    run_until_frame_ready(&mut gba, &mut cycles),
                    "mgba suite: timed out returning to menu after '{}' (attempt {attempt})",
                    MGBA_SUITE_KEYS[suite_idx]
                );
                gba.clear_ready_to_render();
            }

            // Check if we're back at the menu (screen matches the menu CRC
            // we captured before entering this suite).
            let current_crc = gba.screen_crc32();
            if current_crc == menu_crc {
                break;
            }
        }
    }

    MgbaSuiteResult { suite_crcs, cycles }
}

/// Press a button for 1 frame, then release for 1 frame (edge detection).
fn press_button(gba: &mut Gba, cycles: &mut u64, button: u8) {
    // Hold for 1 frame
    gba.set_joypad_button_states(1, button);
    assert!(
        run_until_frame_ready(gba, cycles),
        "mgba suite: timed out during button press (0x{button:02X})"
    );
    gba.clear_ready_to_render();

    // Release for 1 frame
    gba.set_joypad_button_states(1, 0);
    assert!(
        run_until_frame_ready(gba, cycles),
        "mgba suite: timed out during button release"
    );
    gba.clear_ready_to_render();
}

fn maybe_write_mgba_png(gba: &Gba, suite_index: usize, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let key = MGBA_SUITE_KEYS[suite_index];
    let file_name = format!("{key}_crc_{crc:08X}.png");
    let path = PathBuf::from("target/gba_suite_checkpoints").join(file_name);
    let rgb = gba.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(&path, &rgb, Gba::SCREEN_WIDTH, Gba::SCREEN_HEIGHT);
    println!(
        "[mgba-suite-capture] saved {} (suite={key}, crc=0x{crc:08X})",
        path.display()
    );
}

// --- Video sub-suite comparison runner ---

/// Button bitmask for Right (NES-convention bit 7).
const BTN_RIGHT: u8 = 0x80;

/// Names of the 7 video sub-tests in the mgba-emu/suite Video sub-menu.
pub const VIDEO_TEST_NAMES: [&str; 7] = [
    "Basic Mode 3",
    "Basic Mode 4",
    "Degenerate OBJ transforms",
    "Layer toggle",
    "Layer toggle 2",
    "OAM Update Delay",
    "Window offscreen reset",
];

/// Result of a single video test comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTestResult {
    /// CRC32 of the "actual" (rendered by emulator) framebuffer.
    pub actual_crc: u32,
    /// CRC32 of the "expected" (reference) framebuffer.
    pub expected_crc: u32,
    /// Whether the actual matches the expected.
    pub matches: bool,
}

/// Result of running the video comparison across all 7 tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgbaVideoResult {
    /// Per-test comparison results.
    pub tests: Vec<VideoTestResult>,
    /// Total cycles consumed.
    pub cycles: u64,
}

/// Run the mgba-emu/suite Video sub-suite, navigating into each of the 7
/// video tests and capturing both "actual" and "expected" framebuffers.
///
/// Navigation protocol:
/// - Boot → menu → navigate to Video (index 13) → A to enter video sub-menu
/// - For each video test i (0..7):
///   - Press DOWN to advance (except test 0)
///   - Press A to enter the test → shows "actual" rendering
///   - Wait for stability → capture actual CRC
///   - Press RIGHT → shows "expected" rendering
///   - Wait for stability → capture expected CRC
///   - Press B to return to video sub-menu
pub fn run_mgba_video_tests() -> MgbaVideoResult {
    let (mut gba, _rom) = boot_mgba_suite();
    let mut cycles: u64 = 0;

    // Boot: let the menu render.
    for _ in 0..10 {
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "video: timed out during initial boot"
        );
        gba.clear_ready_to_render();
    }

    // Navigate to Video (index 13) in the main menu.
    for _ in 0..13 {
        press_button(&mut gba, &mut cycles, BTN_DOWN);
    }

    // Press A to enter the Video sub-menu.
    press_button(&mut gba, &mut cycles, BTN_A);

    // Wait for the video sub-menu to render and stabilise.
    wait_for_stability(&mut gba, &mut cycles, "video sub-menu");

    let mut tests: Vec<VideoTestResult> = Vec::with_capacity(7);

    for (test_idx, test_name) in VIDEO_TEST_NAMES.iter().enumerate() {
        // Navigate to this test in the video sub-menu.
        if test_idx > 0 {
            press_button(&mut gba, &mut cycles, BTN_DOWN);
        }

        // Capture the menu CRC with cursor at this position (for B-return verification).
        let menu_crc = gba.screen_crc32();

        // Press A to enter the test — shows "actual" rendering.
        press_button(&mut gba, &mut cycles, BTN_A);

        // Wait for the actual rendering to stabilise.
        wait_for_stability(&mut gba, &mut cycles, test_name);
        let actual_crc = gba.screen_crc32();
        maybe_write_video_png(&gba, test_idx, "actual", actual_crc);

        // Press RIGHT to show the "expected" reference rendering.
        press_button(&mut gba, &mut cycles, BTN_RIGHT);

        // Wait for the expected rendering to stabilise.
        wait_for_stability(&mut gba, &mut cycles, test_name);
        let expected_crc = gba.screen_crc32();
        maybe_write_video_png(&gba, test_idx, "expected", expected_crc);

        tests.push(VideoTestResult {
            actual_crc,
            expected_crc,
            matches: actual_crc == expected_crc,
        });

        // Press B to return to the video sub-menu (with verification).
        const MAX_B_RETRIES: u32 = 20;
        let mut returned = false;
        for attempt in 0..MAX_B_RETRIES {
            press_button(&mut gba, &mut cycles, BTN_B);

            for _ in 0..5 {
                assert!(
                    run_until_frame_ready(&mut gba, &mut cycles),
                    "video: timed out returning to sub-menu after test {} (attempt {attempt})",
                    test_idx
                );
                gba.clear_ready_to_render();
            }

            if gba.screen_crc32() == menu_crc {
                returned = true;
                break;
            }
        }
        assert!(
            returned,
            "video: failed to return to sub-menu after test {test_idx} ({test_name}) after {MAX_B_RETRIES} retries"
        );
    }

    MgbaVideoResult { tests, cycles }
}

/// Wait for the screen to change from its initial state, then stabilise.
/// If the screen is already at its final state (no change observed), that is
/// acceptable — `press_button` already advanced frames before this is called.
/// Panics if a change was detected but the screen never stabilised.
fn wait_for_stability(gba: &mut Gba, cycles: &mut u64, label: &str) {
    const STABLE_FRAMES: u32 = 10;
    const MAX_FRAMES: u32 = 300;

    assert!(
        run_until_frame_ready(gba, cycles),
        "video '{label}': timed out getting initial frame"
    );
    gba.clear_ready_to_render();
    let initial_crc = gba.screen_crc32();

    let mut prev_crc = initial_crc;
    let mut stable_count: u32 = 1;
    let mut saw_change = false;

    for frame in 1..MAX_FRAMES {
        assert!(
            run_until_frame_ready(gba, cycles),
            "video '{label}': timed out at frame {frame}"
        );
        gba.clear_ready_to_render();
        let crc = gba.screen_crc32();

        if !saw_change {
            if crc != initial_crc {
                saw_change = true;
                prev_crc = crc;
                stable_count = 1;
            } else {
                // Screen unchanged — count towards stability from initial state.
                stable_count += 1;
                if stable_count >= STABLE_FRAMES {
                    return;
                }
            }
        } else if crc == prev_crc {
            stable_count += 1;
            if stable_count >= STABLE_FRAMES {
                return;
            }
        } else {
            prev_crc = crc;
            stable_count = 1;
        }
    }

    assert!(
        !saw_change,
        "video '{label}': screen changed but never stabilised within {MAX_FRAMES} frames"
    );
}

fn maybe_write_video_png(gba: &Gba, test_index: usize, view: &str, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let name = VIDEO_TEST_NAMES[test_index]
        .to_lowercase()
        .replace(' ', "_");
    let file_name = format!("video_{name}_{view}_crc_{crc:08X}.png");
    let path = PathBuf::from("target/gba_suite_checkpoints").join(file_name);
    let rgb = gba.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(&path, &rgb, Gba::SCREEN_WIDTH, Gba::SCREEN_HEIGHT);
    println!(
        "[video-capture] saved {} (test={}, view={view}, crc=0x{crc:08X})",
        path.display(),
        VIDEO_TEST_NAMES[test_index]
    );
}

fn maybe_write_armwrestler_png(gba: &Gba, page_index: usize, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let file_name = format!("armwrestler_page{page_index}_crc_{crc:08X}.png");
    let path = PathBuf::from("target/gba_suite_checkpoints").join(file_name);
    let rgb = gba.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(&path, &rgb, Gba::SCREEN_WIDTH, Gba::SCREEN_HEIGHT);
    println!(
        "[armwrestler-capture] saved {} (page={page_index}, crc=0x{crc:08X})",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::AppContext;

    #[test]
    fn arm_branch_to_self_detects_idle_opcode() {
        assert!(is_arm_branch_to_self(ARM_BRANCH_SELF_OPCODE, 0x0800_1000));
    }

    #[test]
    fn arm_branch_to_self_rejects_bl() {
        assert!(!is_arm_branch_to_self(0xEBFF_FFFE, 0x0800_1000));
    }

    #[test]
    fn thumb_branch_to_self_detects_idle_opcode() {
        assert!(is_thumb_branch_to_self(THUMB_BRANCH_SELF_OPCODE));
    }

    #[test]
    fn thumb_branch_to_self_rejects_forward_branch() {
        // B +2 (skip one instruction) in THUMB: 0xE000
        assert!(!is_thumb_branch_to_self(0xE000));
    }

    #[test]
    fn thumb_branch_to_self_rejects_conditional_branch() {
        // BEQ -4 is not unconditional, opcode starts with 0xD0xx
        assert!(!is_thumb_branch_to_self(0xD0FE));
    }

    #[test]
    fn non_idle_exit_is_not_counted_as_pass_even_if_index_is_zero() {
        let mut gba = Gba::new(AppContext::default());
        let result = result_from_register(
            &mut gba,
            Suite::Arm,
            12_345,
            0x0800_0000,
            ExitReason::CycleLimitReached,
        );
        assert!(!result.passed);
        assert_eq!(result.failing_index, 0);
    }

    #[test]
    fn exception_vector_branch_to_self_is_not_counted_as_pass() {
        assert!(is_bios_exception_vector(0x18));
        assert!(is_bios_exception_vector(0x04));
        assert!(!is_bios_exception_vector(0x08));

        let mut gba = Gba::new(AppContext::default());
        let result = result_from_register(
            &mut gba,
            Suite::Arm,
            42,
            0x18,
            ExitReason::ExceptionVectorTrap,
        );
        assert!(!result.passed);
    }

    #[test]
    fn capture_output_path_uses_expected_location_and_name() {
        let path = capture_output_path(Suite::PpuStripes, 0x8C90_CEE0);
        assert_eq!(
            path,
            PathBuf::from("target/gba_suite_checkpoints/ppu_stripes_crc_8C90CEE0.png")
        );
    }

    #[test]
    fn suite_capture_stem_is_stable() {
        assert_eq!(Suite::Arm.capture_stem(), "arm");
        assert_eq!(Suite::Thumb.capture_stem(), "thumb");
        assert_eq!(Suite::Nes.capture_stem(), "nes");
        assert_eq!(Suite::Memory.capture_stem(), "memory");
        assert_eq!(Suite::SaveNone.capture_stem(), "save_none");
        assert_eq!(Suite::SaveSram.capture_stem(), "save_sram");
        assert_eq!(Suite::SaveFlash64.capture_stem(), "save_flash64");
        assert_eq!(Suite::SaveFlash128.capture_stem(), "save_flash128");
        assert_eq!(Suite::PpuHello.capture_stem(), "ppu_hello");
        assert_eq!(Suite::PpuShades.capture_stem(), "ppu_shades");
        assert_eq!(Suite::PpuStripes.capture_stem(), "ppu_stripes");
        assert_eq!(
            Suite::FuzzArmDataProcessing.capture_stem(),
            "fuzzarm_data_processing"
        );
        assert_eq!(Suite::FuzzArmAny.capture_stem(), "fuzzarm_any");
        assert_eq!(
            Suite::FuzzThumbDataProcessing.capture_stem(),
            "fuzzthumb_data_processing"
        );
        assert_eq!(Suite::FuzzThumbAny.capture_stem(), "fuzzthumb_any");
        assert_eq!(Suite::FuzzArmMixed.capture_stem(), "fuzzarm_mixed");
        assert_eq!(Suite::ArmWrestler.capture_stem(), "armwrestler");
        assert_eq!(Suite::Mgba.capture_stem(), "mgba_suite");
    }

    #[test]
    fn mgba_memory_sram_log_parser_extracts_score_and_failures() {
        let bytes = b"Memory: 1379/1552\nCPU ROM OOB: FAIL expected=00000000 actual=FFFFFFFF\nDMA3 SRAM mirror: fail source mismatch\0\xFF\xFF";

        let log = parse_mgba_memory_sram_log(bytes);

        assert_eq!(log.passed_count, Some(1379));
        assert_eq!(log.total_count, Some(1552));
        assert_eq!(
            log.raw_log,
            "Memory: 1379/1552\nCPU ROM OOB: FAIL expected=00000000 actual=FFFFFFFF\nDMA3 SRAM mirror: fail source mismatch"
        );
        assert_eq!(
            log.failures,
            vec![
                MgbaMemoryFailure {
                    test_name: "CPU ROM OOB".to_string(),
                    detail: "FAIL expected=00000000 actual=FFFFFFFF".to_string(),
                },
                MgbaMemoryFailure {
                    test_name: "DMA3 SRAM mirror".to_string(),
                    detail: "fail source mismatch".to_string(),
                },
            ]
        );
    }

    #[test]
    fn mgba_sub_suite_log_parser_combines_debug_score_and_sram_failures() {
        let debug = b"BEGIN: I/O read testsPASS: BG0CNTFAIL: BG0HOFSEND: 129/130";
        let sram = b"Game Boy Advance Test Suite\n===\0BG0HOFS: Got 0xFFFF vs 0xDEAD: FAIL\0";

        let log = parse_mgba_sub_suite_log(debug, sram, "I/O read tests");

        assert_eq!(log.passed_count, Some(129));
        assert_eq!(log.total_count, Some(130));
        assert_eq!(
            log.failures,
            vec![MgbaMemoryFailure {
                test_name: "BG0HOFS".to_string(),
                detail: "Got 0xFFFF vs 0xDEAD: FAIL".to_string(),
            }]
        );
    }

    #[test]
    fn mgba_memory_diagnostic_from_sram_includes_crc_and_log() {
        let bytes = b"Memory: 1436/1552\nBIOS OOB: fail open bus\0";

        let result = mgba_memory_diagnostic_from_sram(0x2298_4983, bytes);

        assert_eq!(result.framebuffer_crc32, 0x2298_4983);
        assert_eq!(result.passed_count, Some(1436));
        assert_eq!(result.total_count, Some(1552));
        assert_eq!(result.raw_log, "Memory: 1436/1552\nBIOS OOB: fail open bus");
        assert_eq!(
            result.failures,
            vec![MgbaMemoryFailure {
                test_name: "BIOS OOB".to_string(),
                detail: "fail open bus".to_string(),
            }]
        );
    }

    #[test]
    fn mgba_memory_diagnostic_from_gba_uses_screen_crc_and_mgba_log() {
        let mut gba = Gba::new(AppContext::default());
        gba.bus_mut().write16(0x04FF_F780, 0xC0DE);
        for (offset, byte) in b"Memory: 1/2\nROM OOB: fail value\0".iter().enumerate() {
            gba.bus_mut().write8(0x04FF_F600 + offset as u32, *byte);
        }
        gba.bus_mut().write16(0x04FF_F700, 0x0100);
        let expected_crc = gba.screen_crc32();

        let result = mgba_memory_diagnostic_from_gba(&gba);

        assert_eq!(result.framebuffer_crc32, expected_crc);
        assert_eq!(result.passed_count, Some(1));
        assert_eq!(result.total_count, Some(2));
        assert_eq!(result.raw_log, "Memory: 1/2\nROM OOB: fail value");
    }
}
