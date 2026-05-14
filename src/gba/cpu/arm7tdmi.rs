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
    /// When true, known SWI calls (Halt, VBlankIntrWait) are handled via
    /// high-level emulation instead of dispatching to the BIOS vector.
    hle_swi: bool,
    /// Debug counter: number of times dispatch_irq has been called.
    #[cfg(test)]
    irq_dispatch_count: u64,
    /// Debug: tracks unhandled SWI numbers seen when HLE is active.
    #[cfg(test)]
    pub unhandled_swis: Vec<u8>,
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
            prefetch_valid: false,
            prefetch_arm: [0; 3],
            prefetch_thumb: [0; 3],
            halted: false,
            hle_swi: false,
            #[cfg(test)]
            irq_dispatch_count: 0,
            #[cfg(test)]
            unhandled_swis: Vec::new(),
        }
    }

    fn refill_prefetch<B: Bus>(&mut self, bus: &mut B) {
        let pc = self.regs.r[15];
        if self.thumb() {
            self.prefetch_thumb[0] = bus.read16(pc);
            self.prefetch_thumb[1] = bus.read16(pc.wrapping_add(2));
            self.prefetch_thumb[2] = bus.read16(pc.wrapping_add(4));
        } else {
            self.prefetch_arm[0] = bus.read32(pc);
            self.prefetch_arm[1] = bus.read32(pc.wrapping_add(4));
            self.prefetch_arm[2] = bus.read32(pc.wrapping_add(8));
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

    /// Returns true if the CPU is currently in halt state.
    #[cfg(test)]
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Enable or disable high-level emulation of known SWI calls.
    /// When enabled, SWI 0x02 (Halt) and SWI 0x05 (VBlankIntrWait)
    /// are handled in Rust rather than dispatching to the BIOS vector.
    pub fn set_hle_swi(&mut self, enabled: bool) {
        self.hle_swi = enabled;
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
        // Service pending interrupts first (FIQ has higher priority).
        // An interrupt also wakes the CPU from halt state.
        if self.fiq_pending && !self.regs.f_flag() {
            self.halted = false;
            self.dispatch_fiq();
            return 3;
        }
        if self.irq_pending && !self.regs.i_flag() {
            self.halted = false;
            self.dispatch_irq();
            return 3;
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
        let cycles = if self.thumb() {
            // PC during Thumb execution should read as exec_pc + 4.
            self.regs.r[15] = exec_pc.wrapping_add(4);
            let raw = self.prefetch_thumb[0];
            let mut outcome = thumb::execute(&mut self.regs, bus, raw);
            // Try HLE before normal SWI dispatch.
            if outcome.swi && self.try_hle_swi(bus) {
                outcome.swi = false;
                // HLE handled the SWI inline — it did NOT branch anywhere,
                // so clear branched so PC advances normally (exec_pc + 2).
                outcome.branched = false;
            }
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else if !outcome.branched {
                self.regs.r[15] = exec_pc.wrapping_add(2);
                self.prefetch_thumb[0] = self.prefetch_thumb[1];
                self.prefetch_thumb[1] = self.prefetch_thumb[2];
                self.prefetch_thumb[2] = bus.read16(exec_pc.wrapping_add(6));
            }
            outcome.cycles as u32
        } else {
            // PC during ARM execution should read as exec_pc + 8.
            self.regs.r[15] = exec_pc.wrapping_add(8);
            let raw = self.prefetch_arm[0];
            let mut outcome = arm::execute(&mut self.regs, bus, raw);
            // Try HLE before normal SWI dispatch.
            if outcome.swi && self.try_hle_swi(bus) {
                outcome.swi = false;
                // HLE handled the SWI inline — it did NOT branch anywhere,
                // so clear branched so PC advances normally (exec_pc + 4).
                outcome.branched = false;
            }
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else if !outcome.branched {
                self.regs.r[15] = exec_pc.wrapping_add(4);
                self.prefetch_arm[0] = self.prefetch_arm[1];
                self.prefetch_arm[1] = self.prefetch_arm[2];
                self.prefetch_arm[2] = bus.read32(exec_pc.wrapping_add(12));
            }
            outcome.cycles as u32
        };

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

    /// Try to handle a SWI via high-level emulation. Returns `true` if the
    /// SWI was handled (caller should skip normal BIOS dispatch).
    ///
    /// Reads the SWI number from the prefetched instruction. Currently
    /// handles:
    /// - **0x02 (Halt)**: halt CPU until any enabled interrupt.
    /// - **0x05 (VBlankIntrWait)**: set IME=1, clear VBlank in IntrCheck
    ///   at 0x03007FF8, halt CPU.
    fn try_hle_swi<B: Bus>(&mut self, bus: &mut B) -> bool {
        if !self.hle_swi {
            return false;
        }

        // Extract SWI number from the prefetched instruction (still in slot 0).
        let swi_number = if self.thumb() {
            // THUMB SWI: comment byte in bits 7:0
            (self.prefetch_thumb[0] & 0xFF) as u8
        } else {
            // ARM SWI: GBA convention uses bits 23:16 for the SWI number
            ((self.prefetch_arm[0] >> 16) & 0xFF) as u8
        };

        match swi_number {
            0x02 => {
                // Halt: halt CPU until any enabled interrupt fires.
                self.halted = true;
                true
            }
            0x05 => {
                // VBlankIntrWait ≡ IntrWait(1, VBLANK):
                // 1. Set IME = 1
                bus.write16(0x0400_0208, 1);
                // 2. Clear VBlank bit in IntrCheck (BIOS_IF at 0x03007FF8)
                let intr_check = bus.read16(0x0300_7FF8);
                bus.write16(0x0300_7FF8, intr_check & !1);
                // 3. Halt CPU — will wake on next enabled interrupt.
                //    TODO: The real BIOS loops until VBlank specifically fires;
                //    this simplified version wakes on ANY interrupt and returns.
                //    A timer/keypad IRQ could cause premature return.
                self.halted = true;
                true
            }
            _ => {
                #[cfg(test)]
                {
                    if !self.unhandled_swis.contains(&swi_number) {
                        self.unhandled_swis.push(swi_number);
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cpu::bus::RamBus;
    use crate::gba::cpu::registers::FLAG_Z;

    fn write_arm_word(bus: &mut RamBus, addr: u32, word: u32) {
        bus.write_word(addr, word);
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
}
