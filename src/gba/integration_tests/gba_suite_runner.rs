use crate::gba::Gba;
use crate::gba::bus::memory::BIOS_SIZE;
use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use std::path::PathBuf;

const MAX_CYCLES: u64 = 120_000_000;
const IDLE_PROBE_STABLE_PC_THRESHOLD: u32 = 1;
// The suite ends in `idle: b idle` (ARM `b .`, opcode 0xEAFFFFFE).
const ARM_BRANCH_SELF_OPCODE: u32 = 0xEAFF_FFFE;
const BIOS_RESET_STUB_LDR_PC_PLUS_24: u32 = 0xE59F_F018;
const BIOS_SWI_STUB_MOVS_PC_LR: u32 = 0xE1B0_F00E;
const CART_ENTRYPOINT: u32 = 0x0800_0000;
const BIOS_RESET_LITERAL_ADDR: usize = 0x20;
const BIOS_EXCEPTION_VECTORS: [u32; 5] = [0x04, 0x0C, 0x10, 0x18, 0x1C];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    Arm,
    Thumb,
    Nes,
    Memory,
}

impl Suite {
    fn rom_path(self) -> PathBuf {
        let rel = match self {
            Self::Arm => "roms/gba/automated_tests/gba-tests/arm/arm.gba",
            Self::Thumb => "roms/gba/automated_tests/gba-tests/thumb/thumb.gba",
            Self::Nes => "roms/gba/automated_tests/gba-tests/nes/nes.gba",
            Self::Memory => "roms/gba/automated_tests/gba-tests/memory/memory.gba",
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
    }

    fn result_register(self) -> (usize, &'static str) {
        match self {
            Self::Arm => (12, "r12"),
            Self::Thumb => (7, "r7"),
            Self::Nes => (12, "r12"),
            Self::Memory => (12, "r12"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    IdleLoopDetected,
    ExceptionVectorTrap,
    CycleLimitReached,
    CartStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuiteResult {
    pub passed: bool,
    pub failing_index: u32,
    pub cycles: u64,
    pub pc: u32,
    pub cpsr: u32,
    pub thumb: bool,
    pub opcode_at_pc: u32,
    pub reg_name: &'static str,
    pub exit_reason: ExitReason,
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
            return result_from_register(&mut gba, suite, cycles, pc, ExitReason::CartStopped);
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
            let opcode = gba.bus_mut().peek32(pc);
            if is_arm_branch_to_self(opcode, pc) {
                let reason = if is_bios_exception_vector(pc) {
                    ExitReason::ExceptionVectorTrap
                } else {
                    ExitReason::IdleLoopDetected
                };
                return result_from_register(&mut gba, suite, cycles, pc, reason);
            }
        }
    }

    let pc = gba.cpu_pc();
    result_from_register(&mut gba, suite, cycles, pc, ExitReason::CycleLimitReached)
}

fn result_from_register(
    gba: &mut Gba,
    suite: Suite,
    cycles: u64,
    pc: u32,
    exit_reason: ExitReason,
) -> SuiteResult {
    let (reg_index, reg_name) = suite.result_register();
    let failing_index = gba.cpu_reg(reg_index);
    let cpsr = gba.cpu_cpsr();
    let thumb = gba.cpu_thumb();
    let opcode_at_pc = if thumb {
        gba.bus_mut().peek16(pc) as u32
    } else {
        gba.bus_mut().peek32(pc)
    };
    let passed = failing_index == 0 && exit_reason == ExitReason::IdleLoopDetected;

    SuiteResult {
        passed,
        failing_index,
        cycles,
        pc,
        cpsr,
        thumb,
        opcode_at_pc,
        reg_name,
        exit_reason,
    }
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
}
