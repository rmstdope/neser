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
const GBA_CYCLES_PER_FRAME: u64 = 280_896;
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
}

impl Suite {
    fn rom_path(self) -> PathBuf {
        let rel = match self {
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
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
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
            | Self::FuzzArmMixed => None,
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

    let ewram_dump = if !passed && suite.is_fuzzarm() {
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

    // Consume any stale frame-ready state and then wait for one full frame.
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
            return;
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
    }
}
