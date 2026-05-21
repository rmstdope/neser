//! ARM7TDMI CPU core wrapper.
//!
//! This struct ties the register file, the bus and the ARM/Thumb instruction
//! executors together. It implements the fetch / decode / execute loop, a
//! simple S/N cycle accounting model, exception entry vectors and IRQ/FIQ
//! interrupt dispatch.
//!
//! The implementation mirrors the structure of the existing NES (`Cpu` in
//! `src/nes/cpu/cpu.rs`) and Game Boy (`Sm83` in `src/gb/cpu/sm83.rs`) cores
//! to keep the module layout consistent across emulated systems.

// Many literals in tests below are deliberately written using bit fields that
// mirror the ARM/Thumb instruction encoding for readability.
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::identity_op)]

use super::arm;
use super::bus::Bus;
use super::registers::{CpuMode, FLAG_F, FLAG_I, FLAG_T, Registers};
use super::thumb;
use crate::gba::bus::WidthClass;

/// Address of each ARM exception vector in the BIOS region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionVector {
    Reset = 0x00,
    Undefined = 0x04,
    SoftwareInterrupt = 0x08,
    PrefetchAbort = 0x0C,
    DataAbort = 0x10,
    Irq = 0x18,
    Fiq = 0x1C,
}

/// ARM7TDMI CPU core.
pub struct Arm7tdmi {
    /// Register file.
    pub regs: Registers,
    /// Cumulative cycle counter.
    pub cycles: u64,
    /// Pending IRQ line (rising-edge triggered).
    irq_pending: bool,
    /// Pending FIQ line (rising-edge triggered).
    fiq_pending: bool,
    /// HALT-exit signal: set when (IE & IF) != 0 regardless of IME/CPSR.I.
    /// This unhalts the CPU without dispatching the IRQ handler.
    halt_exit_pending: bool,
    /// Whether the prefetch buffers contain valid instructions for the
    /// current PC/state.
    prefetch_valid: bool,
    /// Three-entry ARM prefetch buffer: [current, next, next-next].
    prefetch_arm: [u32; 3],
    /// Three-entry Thumb prefetch buffer: [current, next, next-next].
    prefetch_thumb: [u16; 3],
    /// CPU is halted (e.g. via SWI Halt/VBlankIntrWait). While halted the
    /// CPU consumes idle cycles until an enabled interrupt wakes it.
    halted: bool,
    /// Saved PC of an instruction whose data access caused a fault, waiting
    /// to be dispatched as a Data Abort at the start of the next `step`.
    /// Data Abort has ARM7TDMI exception priority 2 (higher than FIQ=3 and
    /// IRQ=4), so it must be dispatched before asynchronous interrupts.
    pending_data_abort_exec_pc: Option<u32>,
    /// Debug counter: number of times dispatch_irq has been called.
    #[cfg(test)]
    irq_dispatch_count: u64,
}

impl Default for Arm7tdmi {
    fn default() -> Self {
        Self::new()
    }
}

impl Arm7tdmi {
    /// Create a new CPU core. Enters Supervisor mode at the reset vector with
    /// IRQ and FIQ both masked, mirroring the real hardware reset behaviour.
    pub fn new() -> Self {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::Supervisor);
        regs.cpsr |= FLAG_I | FLAG_F;
        regs.set_thumb(false);
        regs.r[15] = ExceptionVector::Reset as u32;
        Self {
            regs,
            cycles: 0,
            irq_pending: false,
            fiq_pending: false,
            halt_exit_pending: false,
            prefetch_valid: false,
            prefetch_arm: [0; 3],
            prefetch_thumb: [0; 3],
            halted: false,
            pending_data_abort_exec_pc: None,
            #[cfg(test)]
            irq_dispatch_count: 0,
        }
    }

    fn refill_prefetch<B: Bus>(&mut self, bus: &mut B) {
        let pc = self.regs.r[15];
        if self.thumb() {
            self.prefetch_thumb[0] = bus.fetch16(pc);
            self.prefetch_thumb[1] = bus.fetch16(pc.wrapping_add(2));
            self.prefetch_thumb[2] = bus.fetch16(pc.wrapping_add(4));
        } else {
            self.prefetch_arm[0] = bus.fetch32(pc);
            self.prefetch_arm[1] = bus.fetch32(pc.wrapping_add(4));
            self.prefetch_arm[2] = bus.fetch32(pc.wrapping_add(8));
        }
        self.prefetch_valid = true;
    }

    /// Whether the CPU is currently executing in Thumb state.
    pub fn thumb(&self) -> bool {
        self.regs.thumb()
    }

    /// Raise an external IRQ. Will be dispatched on the next `step` if not
    /// masked by the I flag.
    pub fn raise_irq(&mut self) {
        self.irq_pending = true;
    }

    /// Clear a previously raised IRQ.
    pub fn clear_irq(&mut self) {
        self.irq_pending = false;
    }

    /// Raise an external FIQ. Will be dispatched on the next `step` if not
    /// masked by the F flag.
    pub fn raise_fiq(&mut self) {
        self.fiq_pending = true;
    }

    /// Clear a previously raised FIQ.
    pub fn clear_fiq(&mut self) {
        self.fiq_pending = false;
    }

    /// Put the CPU into halt state. While halted the CPU idles (consuming
    /// 1 cycle per step) until an enabled interrupt wakes it.
    pub fn halt(&mut self) {
        self.halted = true;
    }

    /// Signal that HALT should exit without dispatching an IRQ handler.
    /// Used when (IE & IF) != 0 but interrupt dispatch is masked by IME or
    /// CPSR.I.
    pub fn signal_halt_exit(&mut self) {
        self.halt_exit_pending = true;
    }

    /// Returns true if the CPU is currently in halt state.
    #[cfg(test)]
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Debug: how many times dispatch_irq has been called.
    #[cfg(test)]
    pub fn irq_dispatch_count(&self) -> u64 {
        self.irq_dispatch_count
    }

    /// Return the size of the instruction currently pointed to by R15.
    fn instr_size(&self) -> u32 {
        if self.thumb() { 2 } else { 4 }
    }

    /// Run a single instruction. Returns the number of cycles consumed.
    ///
    /// The PC value visible to instructions during execution is two
    /// instructions ahead of the address being executed, matching the
    /// ARM7TDMI three-stage pipeline.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        // Data Abort has ARM7TDMI exception priority 2, which is higher than
        // FIQ (3) and IRQ (4).  A fault detected at the end of the previous
        // step is saved in `pending_data_abort_exec_pc` and dispatched here,
        // before any asynchronous interrupt is serviced.
        if let Some(abort_pc) = self.pending_data_abort_exec_pc.take() {
            self.dispatch_data_abort(abort_pc);
            return 3;
        }

        // Service pending interrupts (FIQ has higher priority than IRQ).
        // An interrupt also wakes the CPU from halt state.
        if self.fiq_pending && !self.regs.f_flag() {
            self.halt_exit_pending = false;
            self.halted = false;
            self.dispatch_fiq();
            return 3;
        }
        if self.irq_pending && !self.regs.i_flag() {
            self.halt_exit_pending = false;
            self.halted = false;
            self.dispatch_irq();
            return 3;
        }
        // On real GBA hardware HALT exits when (IE & IF) != 0 regardless of
        // IME and the CPU I flag. If dispatch is masked, the CPU still exits
        // HALT so the running code can restore interrupt state and continue.
        if self.halt_exit_pending {
            self.halt_exit_pending = false;
            self.halted = false;
            // Fall through to normal instruction execution.
        }

        // While halted, idle for 1 cycle without executing instructions.
        if self.halted {
            self.cycles = self.cycles.wrapping_add(1);
            return 1;
        }

        if !self.prefetch_valid {
            self.refill_prefetch(bus);
        }

        let exec_pc = self.regs.r[15];

        // Check for prefetch abort: raised when the instruction fetch to the
        // current PC caused a bus fault.  Dispatch before executing the
        // instruction — it reaches the Execute stage already faulted.
        if bus.prefetch_abort_pending() {
            self.dispatch_prefetch_abort(exec_pc);
            return 3;
        }

        let cycles = if self.thumb() {
            // PC during Thumb execution should read as exec_pc + 4.
            self.regs.r[15] = exec_pc.wrapping_add(4);
            let raw = self.prefetch_thumb[0];
            let outcome = thumb::execute(&mut self.regs, bus, raw);
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else {
                self.regs.r[15] = exec_pc.wrapping_add(2);
                self.prefetch_thumb[0] = self.prefetch_thumb[1];
                self.prefetch_thumb[1] = self.prefetch_thumb[2];
                self.prefetch_thumb[2] = bus.fetch16(exec_pc.wrapping_add(6));
            }
            let code_addr = if outcome.branched {
                self.regs.r[15]
            } else {
                exec_pc
            };
            let code_width = if outcome.branched && !self.thumb() {
                WidthClass::Word
            } else {
                WidthClass::HalfwordOrByte
            };
            outcome.resolve_cycles(
                bus.s_cycles(code_addr, code_width),
                bus.n_cycles(code_addr, code_width),
                bus.s_cycles(outcome.data_addr, outcome.data_width),
                bus.n_cycles(outcome.data_addr, outcome.data_width),
            )
        } else {
            // PC during ARM execution should read as exec_pc + 8.
            self.regs.r[15] = exec_pc.wrapping_add(8);
            let raw = self.prefetch_arm[0];
            let outcome = arm::execute(&mut self.regs, bus, raw);
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else {
                self.regs.r[15] = exec_pc.wrapping_add(4);
                self.prefetch_arm[0] = self.prefetch_arm[1];
                self.prefetch_arm[1] = self.prefetch_arm[2];
                self.prefetch_arm[2] = bus.fetch32(exec_pc.wrapping_add(12));
            }
            let code_addr = if outcome.branched {
                self.regs.r[15]
            } else {
                exec_pc
            };
            let code_width = if outcome.branched && self.thumb() {
                WidthClass::HalfwordOrByte
            } else {
                WidthClass::Word
            };
            outcome.resolve_cycles(
                bus.s_cycles(code_addr, code_width),
                bus.n_cycles(code_addr, code_width),
                bus.s_cycles(outcome.data_addr, outcome.data_width),
                bus.n_cycles(outcome.data_addr, outcome.data_width),
            )
        };

        // Data Abort: raised when a load/store instruction caused a bus fault.
        // Rather than dispatching immediately (which would give it lower priority
        // than FIQ/IRQ on the *next* step), save the faulting PC and dispatch at
        // the start of the next step, where Data Abort (ARM7TDMI priority 2) is
        // checked before FIQ (3) and IRQ (4).
        if bus.data_abort_pending() {
            self.pending_data_abort_exec_pc = Some(exec_pc);
        }

        self.cycles = self.cycles.wrapping_add(cycles as u64);
        cycles
    }

    /// Dispatch a software interrupt: switch to Supervisor mode, save state
    /// and jump to the SWI vector.
    fn dispatch_swi(&mut self, instr_pc: u32) {
        let return_addr = instr_pc.wrapping_add(self.instr_size());
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Supervisor);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = return_addr;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::SoftwareInterrupt as u32;
        self.prefetch_valid = false;
    }

    /// Dispatch an undefined instruction exception: switch to Undefined mode,
    /// save state and jump to the undefined vector (0x04).
    fn dispatch_undefined(&mut self, instr_pc: u32) {
        let return_addr = instr_pc.wrapping_add(self.instr_size());
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Undefined);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = return_addr;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Undefined as u32;
        self.prefetch_valid = false;
    }

    /// Dispatch an IRQ: switch to IRQ mode, save state and jump to 0x18.
    fn dispatch_irq(&mut self) {
        #[cfg(test)]
        {
            self.irq_dispatch_count += 1;
        }
        // Return address per ARM7TDMI spec and GBATek:
        //   LR_irq = address_of_next_instruction + 4
        // In our pipeline model R15 holds the address of the next instruction
        // to execute, so LR_irq = R15 + 4 regardless of ARM/THUMB state.
        // (Return via `SUBS PC, LR, #4` restores PC to the interrupted
        // instruction.)
        let next_pc = self.regs.r[15].wrapping_add(4);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Irq);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = next_pc;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Irq as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
        // The pending line is treated as a latched edge: clear on dispatch so
        // unmasking I later does not spuriously re-enter the handler.
        self.irq_pending = false;
    }

    /// Dispatch an FIQ: switch to FIQ mode, mask both IRQ and FIQ, jump to 0x1C.
    fn dispatch_fiq(&mut self) {
        // Same return-address rule as IRQ: LR_fiq = R15 + 4 regardless of
        // ARM/THUMB state. Return via `SUBS PC, LR, #4`.
        let next_pc = self.regs.r[15].wrapping_add(4);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Fiq);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = next_pc;
        self.regs.cpsr |= FLAG_I | FLAG_F;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Fiq as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
        // Latched-edge semantics: clear pending on dispatch.
        self.fiq_pending = false;
    }

    /// Dispatch a Prefetch Abort: switch to Abort mode, save state, jump to 0x0C.
    ///
    /// Per ARM7TDMI spec (GBATek BASE+0Ch, priority 5):
    ///   LR_abt = exec_pc + 4, so `SUBS PC, LR, #4` retries the faulting
    ///   instruction.  I is set; F is unchanged.
    fn dispatch_prefetch_abort(&mut self, exec_pc: u32) {
        // LR_abt = exec_pc + 4 in both ARM and Thumb state.
        let lr = exec_pc.wrapping_add(4);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Abort);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = lr;
        self.regs.cpsr |= FLAG_I;
        // F flag is left unchanged per GBATek ("I=1, F=unchanged").
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::PrefetchAbort as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
    }

    /// Dispatch a Data Abort: switch to Abort mode, save state, jump to 0x10.
    ///
    /// Per ARM7TDMI spec (GBATek BASE+10h, priority 2):
    ///   LR_abt = exec_pc + 8, so `SUBS PC, LR, #8` retries the faulting
    ///   instruction.  I is set; F is unchanged.
    fn dispatch_data_abort(&mut self, exec_pc: u32) {
        // LR_abt = exec_pc + 8 in both ARM and Thumb state.
        let lr = exec_pc.wrapping_add(8);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Abort);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = lr;
        self.regs.cpsr |= FLAG_I;
        // F flag is left unchanged per GBATek ("I=1, F=unchanged").
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::DataAbort as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cpu::bus::RamBus;
    use crate::gba::cpu::registers::{FLAG_F, FLAG_Z};

    fn write_arm_word(bus: &mut RamBus, addr: u32, word: u32) {
        bus.write_word(addr, word);
    }

    fn assert_arm_undefined_dispatch(instr: u32) {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        write_arm_word(&mut bus, 0x100, instr);

        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Undefined);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(cpu.regs.r[14], 0x100 + 4);
        assert!(cpu.regs.i_flag(), "IRQ must be masked");
        assert_eq!(
            cpu.regs.cpsr & FLAG_F,
            cpsr_before & FLAG_F,
            "FIQ mask state must be preserved"
        );
        assert!(!cpu.regs.thumb(), "undefined exception enters ARM state");
        assert_eq!(cpu.regs.r[15], ExceptionVector::Undefined as u32);
    }

    #[test]
    fn arm_add_test_vector_executes() {
        // Test vector: R0=1, R1=2; ADD R2,R0,R1 -> R2=3, flags unchanged.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[0] = 1;
        cpu.regs.r[1] = 2;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x100);
        // ADD R2, R0, R1 (cond=AL, opcode=4, S=0, Rn=R0, Rd=R2, Rm=R1).
        // Encoding: cond(0xE) | 000 | opcode(0100) | S(0) | Rn(0) | Rd(2)
        //           | shift_imm(0) | shift_type(00) | 0 | Rm(1)
        let instr = (0xE_u32 << 28)
            | (0x4_u32 << 21)
            | (0_u32 << 20)
            | (0_u32 << 16)
            | (2_u32 << 12)
            | 1_u32;
        write_arm_word(&mut bus, 0x0, instr);

        cpu.step(&mut bus);
        assert_eq!(cpu.regs.r[2], 3);
        assert_eq!(cpu.regs.cpsr, cpsr_before);
        // PC advanced past the executed instruction.
        assert_eq!(cpu.regs.r[15], 0x4);
    }

    #[test]
    fn irq_dispatch_test_vector() {
        // Test vector: CPU in USR mode, IRQ raised
        // -> Mode → IRQ, SPSR_irq = old CPSR, LR_irq = PC+4, IRQ masked.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I; // unmask IRQ
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Irq);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(cpu.regs.r[14], 0x100 + 4); // ARM mode -> +4
        assert!(cpu.regs.i_flag(), "IRQ must be masked after dispatch");
        assert_eq!(cpu.regs.r[15], 0x18);
    }

    #[test]
    fn fiq_dispatch_test_vector() {
        // Test vector: CPU in USR mode, FIQ raised
        // -> Mode → FIQ, banked R8–R12 active, FIQ+IRQ masked.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        for i in 8..=12 {
            cpu.regs.r[i] = 0x1000 + i as u32; // USR R8..R12
        }
        cpu.regs.r[15] = 0x200;

        cpu.raise_fiq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Fiq);
        // Banked R8..R12 are now the FIQ bank (initial zeros).
        for i in 8..=12 {
            assert_eq!(cpu.regs.r[i], 0);
        }
        assert!(cpu.regs.f_flag(), "FIQ must be masked after dispatch");
        assert!(
            cpu.regs.i_flag(),
            "IRQ must also be masked after FIQ dispatch"
        );
        assert_eq!(cpu.regs.r[15], 0x1C);

        // Returning to USR restores the original USR R8..R12.
        cpu.regs.switch_mode(CpuMode::User);
        for i in 8..=12 {
            assert_eq!(cpu.regs.r[i], 0x1000 + i as u32);
        }
    }

    #[test]
    fn irq_does_not_dispatch_when_masked() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr |= FLAG_I; // mask IRQ
        cpu.regs.r[15] = 0x0;
        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        // Code: NOP-equivalent (MOV R0,R0)
        let instr = (0xE_u32 << 28) | (0xD_u32 << 21); // MOV R0,R0
        bus.write_word(0x0, instr);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::User);
    }

    #[test]
    fn dispatched_irq_pending_is_cleared() {
        // Latched-edge semantics: once dispatched, IRQ pending must clear so
        // that unmasking I after the handler doesn't immediately re-enter.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::Irq);

        // Simulate "return from handler": switch back to USR and unmask I.
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        // Bus must contain a valid ARM word so the next step can decode it.
        bus.write_word(0x0, (0xE_u32 << 28) | (0xD_u32 << 21)); // MOV R0,R0
        cpu.regs.r[15] = 0x0;
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "IRQ pending must be cleared on dispatch"
        );
    }

    #[test]
    fn dispatched_fiq_pending_is_cleared() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        cpu.regs.r[15] = 0x100;
        cpu.raise_fiq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::Fiq);

        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        bus.write_word(0x0, (0xE_u32 << 28) | (0xD_u32 << 21));
        cpu.regs.r[15] = 0x0;
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "FIQ pending must be cleared on dispatch"
        );
    }

    #[test]
    fn thumb_arm_switch_via_bx() {
        // Test vector: BX with bit 0 clear -> CPSR T cleared, ARM mode resumes.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[1] = 0x40; // bit 0 clear
        let mut bus = RamBus::new(0x200);
        // Thumb BX R1: 0100_0111_0_0001_000
        let bx = 0b0100_0111_0_0001_000u16;
        bus.write_halfword(0x0, bx);
        cpu.step(&mut bus);
        assert!(!cpu.thumb());
        assert_eq!(cpu.regs.r[15], 0x40);
    }

    #[test]
    fn conditional_skip_test_vector() {
        // Test vector: ADDNE R0,R0,#1 with Z set -> R0 unchanged.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr |= FLAG_Z;
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[0] = 5;
        let mut bus = RamBus::new(0x100);
        // ADDNE R0,R0,#1
        let instr = (0x1_u32 << 28) | (0b001 << 25) | (0x4_u32 << 21) | 1u32;
        bus.write_word(0x0, instr);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.r[0], 5);
        assert_eq!(cpu.regs.r[15], 0x4); // PC still advanced (prefetch).
    }

    #[test]
    fn boot_stub_smoke_test() {
        // Run a tiny boot sequence: ADD, BX-to-Thumb, MOV-imm-thumb, B.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        let mut bus = RamBus::new(0x200);

        // 0x00: MOV R0,#5      (ARM, immediate)
        let mov_imm = (0xE_u32 << 28) | (0b001 << 25) | (0xD_u32 << 21) | 5u32;
        bus.write_word(0x00, mov_imm);
        // 0x04: MOV R1,#0x11   ; target Thumb addr (0x10 | 1)
        let mov_imm2 = (0xE_u32 << 28) | (0b001 << 25) | (0xD_u32 << 21) | (1u32 << 12) | 0x11u32;
        bus.write_word(0x04, mov_imm2);
        // 0x08: BX R1
        let bx = 0xE12F_FF11u32;
        bus.write_word(0x08, bx);
        // 0x10 (Thumb): MOV R2,#42
        let thumb_mov = 0b00100_010_00101010u16;
        bus.write_halfword(0x10, thumb_mov);
        // 0x12: B #0  -> infinite loop (PC won't change in our test, we only step a few times)
        let thumb_b = 0b11100_111_1111_1110u16; // offset -2 -> targets self minus 2 effectively
        bus.write_halfword(0x12, thumb_b);

        // Execute up to a fixed number of cycles, ensuring the CPU never
        // enters an undefined state.
        for _ in 0..16 {
            cpu.step(&mut bus);
        }
        assert_eq!(cpu.regs.r[0], 5);
        assert_eq!(cpu.regs.r[2], 42);
        assert!(
            cpu.thumb(),
            "execution should have switched into Thumb state"
        );
    }

    #[test]
    fn arm_prefetch_keeps_two_instructions_after_self_modify() {
        // Mirrors gba-tests/nes t001: store over the next two ARM opcodes and
        // ensure they still execute from the prefetch queue.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        let mut bus = RamBus::new(0x100);

        bus.write_word(0x00, 0xE3A02000); // mov r2, #0
        bus.write_word(0x04, 0xE3A00014); // mov r0, #0x14 (.pipe1)
        bus.write_word(0x08, 0xE3A01018); // mov r1, #0x18 (.pipe2)
        bus.write_word(0x0C, 0xE5802000); // str r2, [r0]
        bus.write_word(0x10, 0xE5812000); // str r2, [r1]
        bus.write_word(0x14, 0xE3520000); // .pipe1: cmp r2, #0
        bus.write_word(0x18, 0x0A000000); // .pipe2: beq pass
        bus.write_word(0x1C, 0xE3A04001); // fail: mov r4, #1
        bus.write_word(0x20, 0xE3A04002); // pass: mov r4, #2

        for _ in 0..8 {
            cpu.step(&mut bus);
        }

        assert_eq!(cpu.regs.r[4], 2);
    }

    #[test]
    fn undefined_instruction_dispatch_arm() {
        // Test that executing a CLZ instruction (ARMv5TE) causes the CPU to
        // enter Undefined mode with correct state.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I; // unmask IRQ
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        // CLZ R0, R1: cond(0xE) 0001 0110 1111 0000 1111 0001 0001
        let clz: u32 = 0xE16F_0F11;
        write_arm_word(&mut bus, 0x100, clz);

        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Undefined,
            "CPU should be in Undefined mode"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_und should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 4,
            "LR_und should be address after undefined instr"
        );
        assert!(
            cpu.regs.i_flag(),
            "IRQ must be masked after undefined dispatch"
        );
        assert_eq!(
            cpu.regs.r[15],
            ExceptionVector::Undefined as u32,
            "PC should be at undefined vector (0x04)"
        );
    }

    #[test]
    fn undefined_instruction_dispatch_thumb() {
        // Test that executing Thumb BLX (ARMv5TE) causes the CPU to enter
        // Undefined mode with correct state.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        // Thumb BLX: 11101_00000000000 = 0xE800
        let blx_thumb: u16 = 0xE800;
        bus.write16(0x100, blx_thumb);
        bus.write16(0x102, 0); // padding for prefetch
        bus.write16(0x104, 0);

        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Undefined,
            "CPU should be in Undefined mode"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_und should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 2,
            "LR_und should be address after undefined Thumb instr"
        );
        assert!(
            cpu.regs.i_flag(),
            "IRQ must be masked after undefined dispatch"
        );
        assert!(!cpu.regs.thumb(), "T bit should be clear (ARM state)");
        assert_eq!(cpu.regs.r[15], ExceptionVector::Undefined as u32);
    }

    #[test]
    fn coprocessor_transfer_dispatches_undefined_exception() {
        // LDC p3, c2, [r1]: no GBA coprocessor acknowledges the transfer.
        let ldc = (0xE_u32 << 28)
            | (0b110 << 25)
            | (1 << 24)
            | (1 << 23)
            | (1 << 20)
            | (1 << 16)
            | (2 << 12)
            | (3 << 8);

        assert_arm_undefined_dispatch(ldc);
    }

    #[test]
    fn coprocessor_register_transfer_dispatches_undefined_exception() {
        // MRC p4, #1, r3, c2, c5: no GBA coprocessor acknowledges the transfer.
        let mrc = (0xE_u32 << 28)
            | (0b1110 << 24)
            | (1 << 21)
            | (1 << 20)
            | (2 << 16)
            | (3 << 12)
            | (4 << 8)
            | (1 << 4)
            | 5;

        assert_arm_undefined_dispatch(mrc);
    }

    #[test]
    fn swi_still_dispatches_to_supervisor_exception() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        write_arm_word(&mut bus, 0x100, 0xEF00_0000);

        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Supervisor);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(cpu.regs.r[14], 0x100 + 4);
        assert!(cpu.regs.i_flag(), "IRQ must be masked");
        assert_eq!(cpu.regs.cpsr & FLAG_F, cpsr_before & FLAG_F);
        assert!(!cpu.regs.thumb(), "SWI enters ARM state");
        assert_eq!(cpu.regs.r[15], ExceptionVector::SoftwareInterrupt as u32);
    }

    // -----------------------------------------------------------------------
    // step() cycle resolution via Bus timing
    // -----------------------------------------------------------------------

    /// A custom bus with configurable access costs, used to verify that
    /// step() resolves S/N/I counts through the bus timing methods.
    struct SlowBus {
        inner: RamBus,
        s_cost: u32,
        n_cost: u32,
    }

    impl SlowBus {
        fn new(s_cost: u32, n_cost: u32) -> Self {
            Self {
                inner: RamBus::new(0x1000),
                s_cost,
                n_cost,
            }
        }
    }

    impl Bus for SlowBus {
        fn read32(&mut self, addr: u32) -> u32 {
            self.inner.read32(addr)
        }
        fn read16(&mut self, addr: u32) -> u16 {
            self.inner.read16(addr)
        }
        fn read8(&mut self, addr: u32) -> u8 {
            self.inner.read8(addr)
        }
        fn write32(&mut self, addr: u32, value: u32) {
            self.inner.write32(addr, value);
        }
        fn write16(&mut self, addr: u32, value: u16) {
            self.inner.write16(addr, value);
        }
        fn write8(&mut self, addr: u32, value: u8) {
            self.inner.write8(addr, value);
        }
        fn n_cycles(&self, _addr: u32, _width: WidthClass) -> u32 {
            self.n_cost
        }
        fn s_cycles(&self, _addr: u32, _width: WidthClass) -> u32 {
            self.s_cost
        }
    }

    struct AddressTimingBus {
        inner: RamBus,
    }

    impl AddressTimingBus {
        fn new() -> Self {
            Self {
                inner: RamBus::new(0x1000),
            }
        }

        fn write_word(&mut self, addr: u32, word: u32) {
            self.inner.write_word(addr, word);
        }

        fn write_halfword(&mut self, addr: u32, halfword: u16) {
            self.inner.write_halfword(addr, halfword);
        }

        fn costs_for_addr(addr: u32) -> (u32, u32) {
            match addr >> 24 {
                0x01 => (5, 11), // branch target region
                0x02 => (3, 7),  // data region
                _ => (1, 2),     // executed instruction region
            }
        }
    }

    impl Bus for AddressTimingBus {
        fn read32(&mut self, addr: u32) -> u32 {
            self.inner.read32(addr)
        }

        fn read16(&mut self, addr: u32) -> u16 {
            self.inner.read16(addr)
        }

        fn read8(&mut self, addr: u32) -> u8 {
            self.inner.read8(addr)
        }

        fn write32(&mut self, addr: u32, value: u32) {
            self.inner.write32(addr, value);
        }

        fn write16(&mut self, addr: u32, value: u16) {
            self.inner.write16(addr, value);
        }

        fn write8(&mut self, addr: u32, value: u8) {
            self.inner.write8(addr, value);
        }

        fn n_cycles(&self, addr: u32, _width: WidthClass) -> u32 {
            let (_, n) = Self::costs_for_addr(addr);
            n
        }

        fn s_cycles(&self, addr: u32, _width: WidthClass) -> u32 {
            let (s, _) = Self::costs_for_addr(addr);
            s
        }
    }

    /// With a RamBus (all costs = 1), a MOV immediate (1S) should cost 1 cycle.
    /// With a SlowBus (S=3, N=5), the same MOV should cost 3 cycles.
    #[test]
    fn step_resolves_sni_via_bus_timing() {
        // MOV R0, #42 in ARM (data processing, 1S)
        let mov_instr: u32 = 0xE3A0_002A;

        // Fast bus: S=1, N=1 → total = 1*1 = 1
        let mut cpu = Arm7tdmi::new();
        let mut fast_bus = RamBus::new(0x1000);
        fast_bus.write_word(0x00, mov_instr);
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;
        let fast_cycles = cpu.step(&mut fast_bus);
        assert_eq!(fast_cycles, 1, "MOV with RamBus (S=1) should cost 1");

        // Slow bus: S=3, N=5 → total = 1*3 = 3
        let mut cpu = Arm7tdmi::new();
        let mut slow_bus = SlowBus::new(3, 5);
        slow_bus.inner.write_word(0x00, mov_instr);
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;
        let slow_cycles = cpu.step(&mut slow_bus);
        assert_eq!(
            slow_cycles, 3,
            "MOV (1S) with SlowBus (s_cycles=3) should resolve to 3 cycles"
        );
    }

    #[test]
    fn arm_ldm_pc_step_resolves_branch_cycles_at_loaded_pc() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0000_0000;
        cpu.regs.r[0] = 0x0200_0040;

        let mut bus = AddressTimingBus::new();
        // LDMIA R0, {PC}
        let ldmia_pc = (0xE_u32 << 28) | (0b100 << 25) | (1 << 23) | (1 << 20) | (1 << 15);
        bus.write_word(0x0000_0000, ldmia_pc);
        bus.write_word(0x0200_0040, 0x0100_0080);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cpu.regs.r[15], 0x0100_0080);
        assert_eq!(
            cycles, 29,
            "LDM PC branch refill should use target-region code timing: 2*5S + 1*11N + 1*7N(data) + 1I"
        );
    }

    #[test]
    fn thumb_pop_pc_step_resolves_branch_cycles_at_loaded_pc() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.r[15] = 0x0000_0000;
        cpu.regs.r[13] = 0x0200_0040;

        let mut bus = AddressTimingBus::new();
        // POP {PC}
        let pop_pc = 0b1011_1_10_1_0000_0000u16;
        bus.write_halfword(0x0000_0000, pop_pc);
        bus.write_word(0x0200_0040, 0x0100_0081);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cpu.regs.r[15], 0x0100_0080);
        assert_eq!(
            cycles, 29,
            "POP PC branch refill should use target-region code timing: 2*5S + 1*11N + 1*7N(data) + 1I"
        );
    }

    /// Bus that returns different costs for 16-bit vs 32-bit access.
    struct WidthAwareBus {
        inner: RamBus,
    }

    impl WidthAwareBus {
        fn new() -> Self {
            Self {
                inner: RamBus::new(0x1000),
            }
        }
    }

    impl Bus for WidthAwareBus {
        fn read32(&mut self, addr: u32) -> u32 {
            self.inner.read32(addr)
        }
        fn read16(&mut self, addr: u32) -> u16 {
            self.inner.read16(addr)
        }
        fn read8(&mut self, addr: u32) -> u8 {
            self.inner.read8(addr)
        }
        fn write32(&mut self, addr: u32, value: u32) {
            self.inner.write32(addr, value);
        }
        fn write16(&mut self, addr: u32, value: u16) {
            self.inner.write16(addr, value);
        }
        fn write8(&mut self, addr: u32, value: u8) {
            self.inner.write8(addr, value);
        }
        fn n_cycles(&self, _addr: u32, width: WidthClass) -> u32 {
            match width {
                WidthClass::HalfwordOrByte => 2,
                WidthClass::Word => 5,
            }
        }
        fn s_cycles(&self, _addr: u32, width: WidthClass) -> u32 {
            match width {
                WidthClass::HalfwordOrByte => 1,
                WidthClass::Word => 3,
            }
        }
    }

    // -----------------------------------------------------------------------
    // Prefetch Abort and Data Abort exception dispatch tests
    // -----------------------------------------------------------------------

    /// Test bus that can be configured to signal prefetch or data aborts.
    struct FaultBus {
        inner: RamBus,
        signal_prefetch_abort: bool,
        signal_data_abort: bool,
    }

    impl FaultBus {
        fn new() -> Self {
            Self {
                inner: RamBus::new(0x200),
                signal_prefetch_abort: false,
                signal_data_abort: false,
            }
        }
    }

    impl Bus for FaultBus {
        fn read32(&mut self, addr: u32) -> u32 {
            self.inner.read32(addr)
        }
        fn read16(&mut self, addr: u32) -> u16 {
            self.inner.read16(addr)
        }
        fn read8(&mut self, addr: u32) -> u8 {
            self.inner.read8(addr)
        }
        fn write32(&mut self, addr: u32, value: u32) {
            self.inner.write32(addr, value);
        }
        fn write16(&mut self, addr: u32, value: u16) {
            self.inner.write16(addr, value);
        }
        fn write8(&mut self, addr: u32, value: u8) {
            self.inner.write8(addr, value);
        }
        fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
            self.inner.n_cycles(addr, width)
        }
        fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
            self.inner.s_cycles(addr, width)
        }
        fn prefetch_abort_pending(&mut self) -> bool {
            let v = self.signal_prefetch_abort;
            self.signal_prefetch_abort = false;
            v
        }
        fn data_abort_pending(&mut self) -> bool {
            let v = self.signal_data_abort;
            self.signal_data_abort = false;
            v
        }
    }

    #[test]
    fn prefetch_abort_dispatch_arm() {
        // ARM mode: bus signals prefetch abort → CPU enters Abort mode.
        // LR_abt = exec_pc + 4; SUBS PC, LR, #4 retries the faulting instruction.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = FaultBus::new();
        bus.signal_prefetch_abort = true;
        // Prefill valid ARM NOPs so bus reads don't return garbage.
        bus.inner.write_word(0x100, 0xE320_F000); // NOP
        bus.inner.write_word(0x104, 0xE320_F000);
        bus.inner.write_word(0x108, 0xE320_F000);

        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Abort,
            "CPU should enter Abort mode on prefetch abort"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_abt should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 4,
            "LR_abt = exec_pc + 4 for prefetch abort"
        );
        assert!(cpu.regs.i_flag(), "IRQ must be masked after prefetch abort");
        assert!(!cpu.regs.thumb(), "T bit must be clear on exception entry");
        assert_eq!(
            cpu.regs.r[15],
            ExceptionVector::PrefetchAbort as u32,
            "PC should jump to prefetch abort vector (0x0C)"
        );
    }

    #[test]
    fn prefetch_abort_dispatch_thumb() {
        // Thumb mode: bus signals prefetch abort → same LR_abt = exec_pc + 4.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = FaultBus::new();
        bus.signal_prefetch_abort = true;
        // Prefill valid Thumb NOPs (MOV R8, R8).
        bus.inner.write_halfword(0x100, 0x46C0);
        bus.inner.write_halfword(0x102, 0x46C0);
        bus.inner.write_halfword(0x104, 0x46C0);

        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Abort);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 4,
            "LR_abt = exec_pc + 4 for Thumb prefetch abort"
        );
        assert!(cpu.regs.i_flag());
        assert!(!cpu.regs.thumb());
        assert_eq!(cpu.regs.r[15], ExceptionVector::PrefetchAbort as u32);
    }

    #[test]
    fn data_abort_dispatch_arm() {
        // ARM mode: bus signals data abort after LDR → CPU enters Abort mode on
        // the *next* step, before any FIQ/IRQ (priority 2 > FIQ 3 > IRQ 4).
        // LR_abt = exec_pc + 8; SUBS PC, LR, #8 retries the faulting instruction.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = FaultBus::new();
        bus.signal_data_abort = true;
        // LDR R0, [R1] at 0x100 — data access faults; abort is saved as pending.
        bus.inner.write_word(0x100, 0xE591_0000); // LDR R0, [R1]
        bus.inner.write_word(0x104, 0xE320_F000);
        bus.inner.write_word(0x108, 0xE320_F000);

        // Step 1: LDR executes; data abort is saved as pending (not yet dispatched).
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "abort not yet dispatched after step 1"
        );
        // Step 2: pending data abort is dispatched before any FIQ/IRQ.
        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Abort,
            "CPU should enter Abort mode on data abort"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_abt should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 8,
            "LR_abt = exec_pc + 8 for data abort"
        );
        assert!(cpu.regs.i_flag(), "IRQ must be masked after data abort");
        assert!(!cpu.regs.thumb(), "T bit must be clear on exception entry");
        assert_eq!(
            cpu.regs.r[15],
            ExceptionVector::DataAbort as u32,
            "PC should jump to data abort vector (0x10)"
        );
    }

    #[test]
    fn data_abort_dispatch_thumb() {
        // Thumb mode: bus signals data abort → LR_abt = exec_pc + 8 (fixed per spec).
        // Abort is dispatched on the next step before any FIQ/IRQ.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = FaultBus::new();
        bus.signal_data_abort = true;
        // Thumb LDR R0, [R1] (format 9, offset=0) at 0x100.
        bus.inner.write_halfword(0x100, 0x6808); // LDR R0, [R1]
        bus.inner.write_halfword(0x102, 0x46C0);
        bus.inner.write_halfword(0x104, 0x46C0);

        // Step 1: LDR executes; data abort is saved as pending (not yet dispatched).
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "abort not yet dispatched after step 1"
        );
        // Step 2: pending data abort is dispatched before any FIQ/IRQ.
        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Abort);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 8,
            "LR_abt = exec_pc + 8 for Thumb data abort"
        );
        assert!(cpu.regs.i_flag());
        assert!(!cpu.regs.thumb());
        assert_eq!(cpu.regs.r[15], ExceptionVector::DataAbort as u32);
    }

    #[test]
    fn data_abort_preempts_irq() {
        // Verify Data Abort (priority 2) preempts a pending IRQ (priority 4).
        // Step 1: LDR executes and faults → abort saved as pending.
        // Step 2: IRQ is raised.  Data abort must be dispatched first.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I; // unmask IRQ
        cpu.regs.r[15] = 0x100;

        let mut bus = FaultBus::new();
        bus.signal_data_abort = true;
        bus.inner.write_word(0x100, 0xE591_0000); // LDR R0, [R1]
        bus.inner.write_word(0x104, 0xE320_F000);
        bus.inner.write_word(0x108, 0xE320_F000);

        // Step 1: fault occurs, abort saved.
        cpu.step(&mut bus);
        // Raise IRQ while abort is still pending.
        cpu.raise_irq();
        // Step 2: data abort must win over IRQ.
        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Abort,
            "Data Abort should preempt IRQ (priority 2 > 4)"
        );
        assert_eq!(cpu.regs.r[15], ExceptionVector::DataAbort as u32);
    }

    /// ARM step (32-bit fetch) should use Word width; Thumb step should use HalfwordOrByte.
    #[test]
    fn step_arm_uses_word_width_thumb_uses_halfword() {
        // ARM MOV R0, #42 → 1S, resolved with Word s_cycles = 3
        let mov_arm: u32 = 0xE3A0_002A;
        let mut cpu = Arm7tdmi::new();
        let mut bus = WidthAwareBus::new();
        bus.inner.write_word(0x00, mov_arm);
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;
        let arm_cycles = cpu.step(&mut bus);
        assert_eq!(arm_cycles, 3, "ARM MOV (1S) should use Word s_cycles=3");

        // Thumb MOV R0, #42 → 1S, resolved with HalfwordOrByte s_cycles = 1
        let mov_thumb: u16 = 0x202A; // MOV R0, #42
        let mut cpu = Arm7tdmi::new();
        cpu.regs.set_thumb(true); // Thumb mode
        bus.inner.write_halfword(0x00, mov_thumb);
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;
        let thumb_cycles = cpu.step(&mut bus);
        assert_eq!(
            thumb_cycles, 1,
            "Thumb MOV (1S) should use HalfwordOrByte s_cycles=1"
        );
    }
}
