use crate::gba::Gba;
use crate::gba::bus::memory::BIOS_SIZE;
use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use std::path::PathBuf;

const MAX_CYCLES: u64 = 120_000_000;
// The suite ends in `idle: b idle`; 1024 repeated PCs is a conservative
// threshold to recognize that terminal loop without waiting for MAX_CYCLES.
const IDLE_PC_STABLE_THRESHOLD: u32 = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    Arm,
    Thumb,
}

impl Suite {
    fn rom_path(self) -> PathBuf {
        let rel = match self {
            Self::Arm => "roms/gba/automated_tests/gba-tests/arm/arm.gba",
            Self::Thumb => "roms/gba/automated_tests/gba-tests/thumb/thumb.gba",
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
    }

    fn result_register(self) -> (usize, &'static str) {
        match self {
            Self::Arm => (12, "r12"),
            Self::Thumb => (7, "r7"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    IdleLoopDetected,
    CycleLimitReached,
    CartStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuiteResult {
    pub passed: bool,
    pub failing_index: u32,
    pub cycles: u64,
    pub pc: u32,
    pub reg_name: &'static str,
    pub exit_reason: ExitReason,
}

pub fn run_suite(suite: Suite) -> SuiteResult {
    let rom_path = suite.rom_path();
    let rom = std::fs::read(&rom_path).unwrap_or_else(|e| {
        panic!("failed to read suite ROM {}: {e}", rom_path.display());
    });

    let mut gba = Gba::new(AppContext::default());
    let bios = vec![0u8; BIOS_SIZE];
    gba.bus_mut().load_bios(&bios);
    gba.load_rom(&rom, rom_path.to_str().unwrap_or("gba-suite-rom"))
        .unwrap_or_else(|e| {
            panic!("failed to load suite ROM {}: {e}", rom_path.display());
        });

    let mut cycles = 0u64;
    let mut stable_pc_count = 0u32;
    let mut last_pc: Option<u32> = None;

    while cycles < MAX_CYCLES {
        let tick_cycles = gba.run_tick_for_tests() as u64;
        if tick_cycles == 0 {
            let pc = gba.cpu_pc();
            return result_from_register(&gba, suite, cycles, pc, ExitReason::CartStopped);
        }
        cycles += tick_cycles;

        let pc = gba.cpu_pc();
        if Some(pc) == last_pc {
            stable_pc_count += 1;
        } else {
            stable_pc_count = 1;
            last_pc = Some(pc);
        }

        if stable_pc_count >= IDLE_PC_STABLE_THRESHOLD {
            return result_from_register(&gba, suite, cycles, pc, ExitReason::IdleLoopDetected);
        }
    }

    let pc = gba.cpu_pc();
    result_from_register(&gba, suite, cycles, pc, ExitReason::CycleLimitReached)
}

fn result_from_register(
    gba: &Gba,
    suite: Suite,
    cycles: u64,
    pc: u32,
    exit_reason: ExitReason,
) -> SuiteResult {
    let (reg_index, reg_name) = suite.result_register();
    let failing_index = gba.cpu_reg(reg_index);

    SuiteResult {
        passed: failing_index == 0,
        failing_index,
        cycles,
        pc,
        reg_name,
        exit_reason,
    }
}
