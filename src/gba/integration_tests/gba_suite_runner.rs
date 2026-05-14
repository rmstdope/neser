use crate::gba::Gba;
use crate::gba::bus::memory::BIOS_SIZE;
use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use std::path::PathBuf;

const MAX_CYCLES: u64 = 120_000_000;
const IDLE_PROBE_STABLE_PC_THRESHOLD: u32 = 1;
// The suite ends in `idle: b idle` (ARM `b .`, opcode 0xEAFFFFFE).
const ARM_BRANCH_SELF_OPCODE: u32 = 0xEAFF_FFFE;
const THUMB_BRANCH_SELF_OPCODE: u16 = 0xE7FE;
const BIOS_RESET_STUB_LDR_PC_PLUS_24: u32 = 0xE59F_F018;
const BIOS_SWI_STUB_MOVS_PC_LR: u32 = 0xE1B0_F00E;
const CART_ENTRYPOINT: u32 = 0x0800_0000;
const BIOS_RESET_LITERAL_ADDR: usize = 0x20;
const BIOS_EXCEPTION_VECTORS: [u32; 5] = [0x04, 0x0C, 0x10, 0x18, 0x1C];
// GBA nominal timing: 280_896 cycles per frame (~59.73 Hz).
pub(crate) const GBA_CYCLES_PER_FRAME: u64 = 280_896;
// Allow up to two frames while waiting for a fresh frame-ready edge after idle-loop detection.
const FRAME_SETTLE_MAX_CYCLES: u64 = GBA_CYCLES_PER_FRAME * 2;

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
    let mut bios = vec![0u8; BIOS_SIZE];
    bios[0..4].copy_from_slice(&BIOS_RESET_STUB_LDR_PC_PLUS_24.to_le_bytes());
    bios[BIOS_RESET_LITERAL_ADDR..BIOS_RESET_LITERAL_ADDR + 4]
        .copy_from_slice(&CART_ENTRYPOINT.to_le_bytes());
    bios[0x08..0x0C].copy_from_slice(&BIOS_SWI_STUB_MOVS_PC_LR.to_le_bytes());
    for &vector in &BIOS_EXCEPTION_VECTORS {
        let i = vector as usize;
        bios[i..i + 4].copy_from_slice(&ARM_BRANCH_SELF_OPCODE.to_le_bytes());
    }
    gba.bus_mut().load_bios(&bios);
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("gba-suite-rom"))
        .unwrap_or_else(|e| {
            panic!("failed to load suite ROM {}: {e}", rom_path.display());
        });
    gba.init_test_stack_pointers();

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
// navigate DOWN×3 to THUMB ALU (item 3), then cover all 3 THUMB pages.

/// Total test pages: 5 ARM (Test0–Test4) + 3 THUMB (_test0–_test2).
pub const ARMWRESTLER_TEST_PAGE_COUNT: usize = 8;

/// Button bitmask for A (NES-convention bit 0).
const BTN_A: u8 = 0x01;
/// Button bitmask for B (NES-convention bit 1).
const BTN_B: u8 = 0x02;
/// Button bitmask for Start (NES-convention bit 3).
const BTN_START: u8 = 0x08;
/// Button bitmask for Down (NES-convention bit 5).
const BTN_DOWN: u8 = 0x20;

/// Result of running the armwrestler ROM through all test pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmWrestlerResult {
    /// CRC32 of the framebuffer after each test page renders.
    /// Indices 0–4: ARM pages, indices 5–7: THUMB pages.
    pub page_crcs: Vec<u32>,
    /// Total cycles consumed.
    pub cycles: u64,
}

/// Run armwrestler-gba-fixed.gba, navigating through all ARM and THUMB test pages.
///
/// Returns one CRC per test page (8 total: 5 ARM + 3 THUMB).
pub fn run_armwrestler() -> ArmWrestlerResult {
    let rom_path = Suite::ArmWrestler.rom_path();

    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read armwrestler ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    let mut bios = vec![0u8; BIOS_SIZE];
    bios[0..4].copy_from_slice(&BIOS_RESET_STUB_LDR_PC_PLUS_24.to_le_bytes());
    bios[BIOS_RESET_LITERAL_ADDR..BIOS_RESET_LITERAL_ADDR + 4]
        .copy_from_slice(&CART_ENTRYPOINT.to_le_bytes());
    bios[0x08..0x0C].copy_from_slice(&BIOS_SWI_STUB_MOVS_PC_LR.to_le_bytes());
    for &vector in &BIOS_EXCEPTION_VECTORS {
        let i = vector as usize;
        bios[i..i + 4].copy_from_slice(&ARM_BRANCH_SELF_OPCODE.to_le_bytes());
    }
    gba.bus_mut().load_bios(&bios);
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("armwrestler"))
        .unwrap_or_else(|e| {
            panic!("failed to load armwrestler ROM {}: {e}", rom_path.display());
        });
    gba.init_test_stack_pointers();

    // Run frame-by-frame, injecting button presses to navigate tests.
    //
    // Button held for exactly 1 frame, released the next. The ROM's
    // edge-detection (XOR with previous state) requires a clean release
    // between successive presses.
    //
    // Frame schedule:
    //   ARM tests (enter from menu item 0, chain Test0–Test4):
    //     Frame 2: START (menu → Test0)
    //     Frame 4: capture[0], START (→ Test1)
    //     Frame 6: capture[1], START (→ Test2)
    //     Frame 8: capture[2], START (→ Test3)
    //     Frame 10: capture[3], START (→ Test4)
    //     Frame 12: capture[4], START (→ menu, TESTNUM wraps)
    //   Navigate to THUMB ALU (menu item 3 via DOWN×3):
    //     Frame 14: DOWN (CURSEL 0→1)
    //     Frame 16: DOWN (CURSEL 1→2)
    //     Frame 18: DOWN (CURSEL 2→3)
    //   THUMB tests (enter from menu item 3, chain _test0–_test2):
    //     Frame 20: START (menu → _tmbmain → _test0 init)
    //     Frame 22-23: wait (THUMB entry does extra VSync before rendering)
    //     Frame 24: capture[5], START (→ _test1)
    //     Frame 26: capture[6], START (→ _test2)
    //     Frame 28: capture[7] → done

    let mut frame_count: u32 = 0;
    let mut cycles: u64 = 0;
    let mut page_crcs: Vec<u32> = Vec::new();
    let mut button_state: u8 = 0;
    let max_frames: u32 = 35;

    // Advance to first VBlank (frame 0 is the initial partial frame)
    assert!(
        run_until_frame_ready(&mut gba, &mut cycles),
        "armwrestler: CPU halted or timed out before first frame"
    );
    gba.clear_ready_to_render();
    frame_count += 1;

    while frame_count < max_frames && page_crcs.len() < ARMWRESTLER_TEST_PAGE_COUNT {
        // Apply button state for this frame
        gba.set_joypad_button_states(0, button_state);

        // Run to next VBlank
        assert!(
            run_until_frame_ready(&mut gba, &mut cycles),
            "armwrestler: CPU halted or timed out at frame {frame_count}"
        );
        gba.clear_ready_to_render();
        frame_count += 1;

        match frame_count {
            // --- ARM test chain (pages 0–4) ---
            2 => button_state = BTN_START,
            3 => button_state = 0,
            4 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 0, crc);
                button_state = BTN_START;
            }
            5 => button_state = 0,
            6 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 1, crc);
                button_state = BTN_START;
            }
            7 => button_state = 0,
            8 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 2, crc);
                button_state = BTN_START;
            }
            9 => button_state = 0,
            10 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 3, crc);
                button_state = BTN_START;
            }
            11 => button_state = 0,
            12 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 4, crc);
                // Press START to return from Test4 to menu
                button_state = BTN_START;
            }
            13 => button_state = 0,
            // --- Navigate menu DOWN×3 to THUMB ALU (item 3) ---
            14 => button_state = BTN_DOWN,
            15 => button_state = 0,
            16 => button_state = BTN_DOWN,
            17 => button_state = 0,
            18 => button_state = BTN_DOWN,
            19 => button_state = 0,
            // --- Enter THUMB tests ---
            20 => button_state = BTN_START,
            21 => button_state = 0,
            // Frames 22-23: THUMB entry does an extra VSync before rendering _test0
            24 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 5, crc);
                button_state = BTN_START;
            }
            25 => button_state = 0,
            26 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 6, crc);
                button_state = BTN_START;
            }
            27 => button_state = 0,
            28 => {
                let crc = gba.screen_crc32();
                page_crcs.push(crc);
                maybe_write_armwrestler_png(&gba, 7, crc);
                button_state = 0;
            }
            _ => {}
        }
    }

    ArmWrestlerResult { page_crcs, cycles }
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

/// Build an enhanced stub BIOS for the mgba-emu test suite.
///
/// The mgba suite (via libgba) uses VBlankIntrWait (SWI 0x05) and relies on
/// the BIOS IRQ dispatcher at vector 0x18 to call the user-installed handler
/// at `[0x03FFFFFC]`. This stub provides:
///
/// - **Reset vector (0x00)**: jump to cartridge entrypoint 0x0800_0000.
/// - **SWI vector (0x08)**: `MOVS PC, LR` (fallback for non-HLE SWIs).
/// - **IRQ vector (0x18)**: ARM code that saves context, calls user handler
///   at `[0x03FFFFFC]`, restores context, and returns via `SUBS PC, LR, #4`.
/// - All other exception vectors: branch-to-self trap.
///
/// HLE SWI handling (VBlankIntrWait, Halt) is done in the CPU core when
/// `set_hle_swi(true)` is called — the BIOS SWI vector is only reached for
/// unrecognised SWI numbers.
pub fn build_mgba_stub_bios() -> Vec<u8> {
    let mut bios = vec![0u8; BIOS_SIZE];

    // Reset vector: LDR PC, [PC, #24] → loads literal at 0x20 = 0x0800_0000
    bios[0x00..0x04].copy_from_slice(&BIOS_RESET_STUB_LDR_PC_PLUS_24.to_le_bytes());
    bios[BIOS_RESET_LITERAL_ADDR..BIOS_RESET_LITERAL_ADDR + 4]
        .copy_from_slice(&CART_ENTRYPOINT.to_le_bytes());

    // SWI vector: MOVS PC, LR (fallback return for unhandled SWIs)
    bios[0x08..0x0C].copy_from_slice(&BIOS_SWI_STUB_MOVS_PC_LR.to_le_bytes());

    // Unused exception vectors: branch-to-self (trap)
    for &vector in &[0x04u32, 0x0C, 0x10, 0x1C] {
        let i = vector as usize;
        bios[i..i + 4].copy_from_slice(&ARM_BRANCH_SELF_OPCODE.to_le_bytes());
    }

    // IRQ vector (0x18): branch to IRQ handler body at 0x80.
    // ARM branch encoding: target = PC + 8 + (offset * 4)
    //   offset = (0x80 - (0x18 + 8)) / 4 = 0x60 / 4 = 24 = 0x18
    let irq_branch: u32 = 0xEA00_0018; // B 0x80
    bios[0x18..0x1C].copy_from_slice(&irq_branch.to_le_bytes());

    // IRQ handler body at 0x80:
    //   0x80: STMFD SP!, {r0-r3, r12, lr}   ; save caller-saved regs
    //   0x84: MOV r0, #0x04000000            ; I/O base
    //   0x88: ADD lr, pc, #0                 ; LR = 0x90 (return addr for user handler)
    //   0x8C: LDR pc, [r0, #-4]             ; jump to [0x03FFFFFC] (user handler)
    //   0x90: LDMFD SP!, {r0-r3, r12, lr}   ; restore regs
    //   0x94: SUBS pc, lr, #4               ; return from IRQ
    let irq_handler: [u32; 6] = [
        0xE92D_500F, // STMFD SP!, {r0-r3, r12, lr}
        0xE3A0_0301, // MOV r0, #0x04000000
        0xE28F_E000, // ADD lr, pc, #0
        0xE510_F004, // LDR pc, [r0, #-4]
        0xE8BD_500F, // LDMFD SP!, {r0-r3, r12, lr}
        0xE25E_F004, // SUBS pc, lr, #4
    ];
    for (i, &word) in irq_handler.iter().enumerate() {
        let addr = 0x80 + i * 4;
        bios[addr..addr + 4].copy_from_slice(&word.to_le_bytes());
    }

    bios
}

/// Boot the mgba-emu test suite ROM with the enhanced stub BIOS.
///
/// Returns a `Gba` instance ready to run, positioned at the cartridge
/// entrypoint with stack pointers initialised. HLE SWI is enabled so
/// VBlankIntrWait and Halt are handled in Rust.
pub fn boot_mgba_suite() -> (Gba, Vec<u8>) {
    let rom_path = Suite::Mgba.rom_path();
    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read mgba suite ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    let bios = build_mgba_stub_bios();
    gba.bus_mut().load_bios(&bios);
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("mgba-emu-suite"))
        .unwrap_or_else(|e| {
            panic!("failed to load mgba suite ROM {}: {e}", rom_path.display());
        });
    gba.init_test_stack_pointers();
    gba.set_hle_swi(true);

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
}
