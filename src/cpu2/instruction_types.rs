//! Instruction implementations for cycle-accurate CPU
//!
//! This module contains implementations of 6502 instructions that work
//! cycle-by-cycle following the InstructionType trait.

use super::traits::InstructionType;
use super::types::{
    CpuState, FLAG_BREAK, FLAG_CARRY, FLAG_INTERRUPT, FLAG_NEGATIVE, FLAG_UNUSED, FLAG_ZERO,
    IRQ_VECTOR,
};
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

/// Helper function to set or clear the Zero flag based on a value
#[inline]
fn set_zero_flag(p: &mut u8, value: u8) {
    *p = (*p & !FLAG_ZERO) | if value == 0 { FLAG_ZERO } else { 0 };
}

/// Helper function to set or clear the Negative flag based on a value
#[inline]
fn set_negative_flag(p: &mut u8, value: u8) {
    *p = (*p & !FLAG_NEGATIVE) | (value & FLAG_NEGATIVE);
}

/// Helper function to set or clear the Carry flag
#[inline]
fn set_carry_flag(p: &mut u8, carry: bool) {
    *p = (*p & !FLAG_CARRY) | if carry { FLAG_CARRY } else { 0 };
}

/// Helper function to perform arithmetic shift left
/// Returns the shifted value and sets the carry flag based on bit 7
#[inline]
fn shift_left(value: u8) -> (u8, bool) {
    let carry = (value & 0x80) != 0;
    let shifted = value << 1;
    (shifted, carry)
}

/// AAC - AND with Carry (Illegal Opcode)
///
/// Also known as ANC. Performs AND between accumulator and immediate value,
/// then copies bit 7 of the result to the carry flag.
///
/// Operation: A = A & M, C = N
/// Flags: N, Z, C
///
/// Cycles: 1
///   1. AND value with A, set flags, copy N to C
#[derive(Debug, Clone, Copy, Default)]
pub struct Aac {
    cycle: u8,
}

impl Aac {
    /// Create a new AAC instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Aac {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Aac::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: AND value with A
                cpu_state.a &= addressing_mode.get_u8_value();

                // Set N and Z flags based on result
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);

                // Copy bit 7 (N flag) to carry flag
                let carry = (cpu_state.a & 0x80) != 0;
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// TOP - Triple NOP (Illegal Opcode)
///
/// Also known as NOP (absolute). Reads from memory but does nothing with the value.
/// It's essentially a NOP that uses absolute addressing.
///
/// Operation: Read value (do nothing with it)
/// Flags: None affected
///
/// Cycles: 1
///   1. Value already read by addressing mode, do nothing
#[derive(Debug, Clone, Copy, Default)]
pub struct Top {
    cycle: u8,
}

impl Top {
    /// Create a new TOP instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Top {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        _cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Top::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Do nothing, value is already read by addressing mode
                // This is a no-op
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// BPL - Branch if Positive
///
/// Branch if the negative flag (N) is clear (positive).
/// The relative addressing mode provides the signed offset.
///
/// Operation: Branch if N == 0
/// Flags: None affected
///
/// Cycles:
///   - 2 if branch not taken (opcode + offset fetch)
///   - 3 if branch taken, same page (opcode + offset fetch + branch)
///   - 4 if branch taken, page cross (opcode + offset fetch + branch + fix high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Bpl {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bpl {
    /// Create a new BPL instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bpl {
    fn is_done(&self) -> bool {
        (!self.branch_taken && self.cycle == 1)
            || (self.branch_taken && !self.page_crossed && self.cycle == 2)
            || (self.branch_taken && self.page_crossed && self.cycle == 3)
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(!self.is_done(), "Bpl::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check negative flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_NEGATIVE) == 0;

                if !self.branch_taken {
                    // Branch not taken - we're done
                    self.cycle = 1;
                } else {
                    // Branch taken - get target address and check for page cross
                    let target = addressing_mode.get_address();
                    let current_page = cpu_state.pc & 0xFF00;
                    let target_page = target & 0xFF00;
                    self.page_crossed = current_page != target_page;

                    // Update PC to target
                    cpu_state.pc = target;
                    self.cycle = 1;
                }
            }
            1 => {
                // Cycle 2: Extra cycle for branch taken (same page)
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Extra cycle for page crossing
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// CLC - Clear Carry Flag
///
/// Clears the carry flag in the status register.
///
/// Operation: C = 0
/// Flags: C
///
/// Cycles: 1
///   1. Clear carry flag
#[derive(Debug, Clone, Copy, Default)]
pub struct Clc {
    cycle: u8,
}

impl Clc {
    /// Create a new CLC instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Clc {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Clc::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Clear carry flag
                cpu_state.p &= !super::types::FLAG_CARRY;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// SEC - Set Carry Flag
///
/// Sets the carry flag in the status register.
///
/// Operation: C = 1
/// Flags: C
///
/// Cycles: 1
///   1. Set carry flag
#[derive(Debug, Clone, Copy, Default)]
pub struct Sec {
    cycle: u8,
}

impl Sec {
    /// Create a new SEC instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sec {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sec::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Set carry flag
                cpu_state.p |= super::types::FLAG_CARRY;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// BMI - Branch if Minus
///
/// Branch if the negative flag (N) is set (minus/negative).
/// The relative addressing mode provides the signed offset.
///
/// Operation: Branch if N == 1
/// Flags: None affected
///
/// Cycles:
///   - 2 if branch not taken (opcode + offset fetch)
///   - 3 if branch taken, same page (opcode + offset fetch + branch)
///   - 4 if branch taken, page cross (opcode + offset fetch + branch + fix high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Bmi {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bmi {
    /// Create a new BMI instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bmi {
    fn is_done(&self) -> bool {
        (!self.branch_taken && self.cycle == 1)
            || (self.branch_taken && !self.page_crossed && self.cycle == 2)
            || (self.branch_taken && self.page_crossed && self.cycle == 3)
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(!self.is_done(), "Bmi::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check negative flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_NEGATIVE) != 0;

                if !self.branch_taken {
                    // Branch not taken - we're done
                    self.cycle = 1;
                } else {
                    // Branch taken - get target address and check for page cross
                    let target = addressing_mode.get_address();
                    let current_page = cpu_state.pc & 0xFF00;
                    let target_page = target & 0xFF00;
                    self.page_crossed = current_page != target_page;

                    // Update PC to target
                    cpu_state.pc = target;
                    self.cycle = 1;
                }
            }
            1 => {
                // Cycle 2: Extra cycle for branch taken (same page)
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Extra cycle for page crossing
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// NOP - No Operation (Illegal Opcode)
///
/// Does nothing for one cycle. This is an implied addressing mode NOP.
/// The official NOP is 0xEA, but this is one of several illegal NOP variants.
///
/// Operation: None
/// Flags: None affected
///
/// Cycles: 1
///   1. Do nothing
#[derive(Debug, Clone, Copy, Default)]
pub struct Nop {
    cycle: u8,
}

impl Nop {
    /// Create a new NOP instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Nop {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        _cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Nop::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Do nothing
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// JSR - Jump to Subroutine
///
/// Pushes the address of the next instruction minus 1 onto the stack
/// and jumps to the target address.
///
/// Addressing mode: Absolute
/// Cycles: 6
///   1. Fetch low byte of target address
///   2. Internal operation (stack pointer access)
///   3. Push PCH (high byte of return address)
///   4. Push PCL (low byte of return address)
///   5. Fetch high byte of target address
///   6. Copy target to PC
#[derive(Debug, Clone, Copy, Default)]
pub struct Jsr {
    cycle: u8,
    target_address: u16,
}

impl Jsr {
    /// Create a new JSR instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Jsr {
    fn is_done(&self) -> bool {
        self.cycle == 5
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 6, "Jsr::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Fetch low byte of target address from PC
                let low = memory.borrow().read(cpu_state.pc);
                self.target_address = low as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Internal operation (prepare for stack operations)
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Push PCH (high byte of return address) to stack
                // Return address is PC - 1 (points to last byte of JSR instruction)
                let return_address = cpu_state.pc;
                let pch = (return_address >> 8) as u8;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, pch, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 3;
            }
            3 => {
                // Cycle 4: Push PCL (low byte of return address) to stack
                let return_address = cpu_state.pc;
                let pcl = (return_address & 0xFF) as u8;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, pcl, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 4;
            }
            4 => {
                // Cycle 5: Fetch high byte of target address from PC and jump
                let high = memory.borrow().read(cpu_state.pc);
                self.target_address |= (high as u16) << 8;
                cpu_state.pc = self.target_address;
                self.cycle = 5;
            }
            _ => unreachable!(),
        }
    }
}

/// JMP - Jump
///
/// Sets PC to the target address without affecting the stack.
///
/// Cycles: 1
///   1. Copy target to PC
#[derive(Debug, Clone, Copy, Default)]
pub struct Jmp {
    cycle: u8,
}

impl Jmp {
    /// Create a new JMP
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Jmp {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Jmp::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Copy target address to PC from addressing mode
                cpu_state.pc = addressing_mode.get_address();
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// BRK - Break / Software Interrupt
///
/// Pushes the return address (PC+2) and status register onto the stack,
/// sets the I flag, and loads PC from the IRQ vector at $FFFE-$FFFF.
///
/// Total cycles: 7 (opcode fetch + 5 execution cycles + completion cycle)
///   1. Opcode fetch (handled by CPU)
///   2. Fetch next byte (padding byte, ignored)
///   3. Push PCH (high byte of PC+2) to stack
///   4. Push PCL (low byte of PC+2) to stack
///   5. Push status register with B flag set to stack
///   6. Load PCL from IRQ vector ($FFFE), set I flag
///   7. Load PCH from IRQ vector ($FFFF) (completion handled by CPU)
#[derive(Debug, Clone, Copy, Default)]
pub struct Brk {
    cycle: u8,
    return_address: u16,
}

impl Brk {
    /// Create a new BRK instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Brk {
    fn is_done(&self) -> bool {
        self.cycle == 6
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 6, "Brk::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 2: Fetch padding byte (ignored) and calculate return address (PC+2)
                let _padding = memory.borrow().read(cpu_state.pc);
                self.return_address = cpu_state.pc.wrapping_add(1); // PC+2 after opcode+padding
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Cycle 3: Push PCH (high byte of return address) to stack
                let pch = (self.return_address >> 8) as u8;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, pch, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 2;
            }
            2 => {
                // Cycle 4: Push PCL (low byte of return address) to stack
                let pcl = (self.return_address & 0xFF) as u8;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, pcl, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 3;
            }
            3 => {
                // Cycle 5: Push status register with B and unused flags set
                let status = cpu_state.p | FLAG_BREAK | FLAG_UNUSED;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, status, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 4;
            }
            4 => {
                // Cycle 6: Load PCL from IRQ vector and set I flag
                let pcl = memory.borrow().read(IRQ_VECTOR);
                cpu_state.pc = pcl as u16;
                cpu_state.p |= FLAG_INTERRUPT;
                self.cycle = 5;
            }
            5 => {
                // Cycle 7: Load PCH from IRQ vector
                let pch = memory.borrow().read(IRQ_VECTOR + 1);
                cpu_state.pc |= (pch as u16) << 8;
                self.cycle = 6;
            }
            _ => unreachable!(),
        }
    }
}

/// PHP - Push Processor Status
///
/// Pushes a copy of the status register (with B and unused flags set) onto the stack.
///
/// Addressing mode: Implied
/// Cycles: 3
///   1. Internal operation (increment PC, prepare for push)
///   2. Push status register to stack
///   3. Complete
#[derive(Debug, Clone, Copy, Default)]
pub struct Php {
    cycle: u8,
}

impl Php {
    /// Create a new PHP instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Php {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 2, "Php::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Internal operation (does nothing, overlaps with fetch)
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Push status register with B and unused flags set
                let status = cpu_state.p | FLAG_BREAK | FLAG_UNUSED;
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, status, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }
}

/// ORA - Logical Inclusive OR
///
/// Performs a bitwise OR between the accumulator and the value at the target address,
/// storing the result in the accumulator. Sets N and Z flags based on the result.
///
/// Cycles: 1
///   1. Read value from target address, OR with A, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Ora {
    cycle: u8,
}

impl Ora {
    /// Create a new ORA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Ora {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Ora::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: OR value with A
                cpu_state.a |= addressing_mode.get_u8_value();

                // Set N and Z flags based on result
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;
    use crate::cpu2::traits::AddressingMode;
    use crate::nes::TvSystem;
    use crate::ppu::Ppu;

    #[test]
    fn test_jsr_starts_not_done() {
        let jsr = Jsr::new();
        assert!(!jsr.is_done(), "Should not be done initially");
    }

    #[test]
    fn test_jsr_completes_after_six_cycles() {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // JSR $1234 at address $0400
        memory.borrow_mut().write(0x0400, 0x34, false); // Low byte of target
        memory.borrow_mut().write(0x0401, 0x12, false); // High byte of target

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut jsr = Jsr::new();

        // Execute 6 cycles
        let addressing_mode = crate::cpu2::addressing::Implied;
        for i in 1..=6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory), &addressing_mode);
            if i < 6 {
                assert!(!jsr.is_done(), "Should not be done after cycle {}", i);
            }
        }

        assert!(jsr.is_done(), "Should be done after 6 cycles");
        assert_eq!(cpu_state.pc, 0x1234, "PC should be set to target address");
        assert_eq!(
            cpu_state.sp, 0xFB,
            "Stack pointer should have decremented by 2"
        );

        // Check return address on stack (PC was 0x0402 when returning, so we push 0x0401)
        // Stack grows downward: PCH at 0x01FD, PCL at 0x01FC
        let pch = memory.borrow().read(0x01FD);
        let pcl = memory.borrow().read(0x01FC);
        let return_address = ((pch as u16) << 8) | (pcl as u16);
        assert_eq!(
            return_address, 0x0401,
            "Return address on stack should be PC-1 of next instruction"
        );
    }

    #[test]
    fn test_jsr_pushes_correct_return_address() {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // JSR $0234 at address $0500
        memory.borrow_mut().write(0x0500, 0x34, false);
        memory.borrow_mut().write(0x0501, 0x02, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFF, // Full stack
            pc: 0x0500,
            p: 0,
        };

        let mut jsr = Jsr::new();

        let addressing_mode = crate::cpu2::addressing::Implied;
        for _ in 0..6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory), &addressing_mode);
        }

        assert!(jsr.is_done());
        assert_eq!(cpu_state.pc, 0x0234, "PC should jump to target");
        assert_eq!(cpu_state.sp, 0xFD, "SP should decrement by 2");

        // Return address should be 0x0501 (points to high byte of JSR operand)
        let pch = memory.borrow().read(0x01FF);
        let pcl = memory.borrow().read(0x01FE);
        assert_eq!(pch, 0x05, "High byte of return address should be correct");
        assert_eq!(pcl, 0x01, "Low byte of return address should be correct");
    }

    #[test]
    fn test_jsr_with_stack_wrap() {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        memory.borrow_mut().write(0x0500, 0x00, false);
        memory.borrow_mut().write(0x0501, 0x06, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0x01, // Near bottom of stack
            pc: 0x0500,
            p: 0,
        };

        let mut jsr = Jsr::new();

        let addressing_mode = crate::cpu2::addressing::Implied;
        for _ in 0..6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory), &addressing_mode);
        }

        assert!(jsr.is_done());
        assert_eq!(cpu_state.pc, 0x0600);
        assert_eq!(cpu_state.sp, 0xFF, "Stack pointer should wrap around");
    }

    #[test]
    fn test_jmp_starts_not_done() {
        let jmp = Jmp::new();
        assert!(!jmp.is_done(), "Should not be done initially");
    }

    #[test]
    fn test_jmp_completes_after_one_cycle() {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        let mut cpu_state = CpuState {
            a: 0xAA,
            x: 0xBB,
            y: 0xCC,
            sp: 0xFD,
            pc: 0x0400,
            p: 0xDD,
        };

        let mut jmp = Jmp::new();

        // Create addressing mode that will provide the target address
        let mut absolute_addr = crate::cpu2::addressing::Absolute::new(false);
        // Simulate that addressing mode has resolved to 0x1234
        memory.borrow_mut().write(0x0400, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0401, 0x12, false); // High byte
        // Tick addressing mode to completion
        while !absolute_addr.is_done() {
            absolute_addr.tick(&mut cpu_state, Rc::clone(&memory));
        }

        // Execute 1 cycle
        jmp.tick(&mut cpu_state, Rc::clone(&memory), &absolute_addr);

        assert!(jmp.is_done(), "Should be done after 1 cycle");
        assert_eq!(cpu_state.pc, 0x1234, "PC should be set to target address");
        assert_eq!(cpu_state.sp, 0xFD, "Stack pointer should not change");
        assert_eq!(cpu_state.a, 0xAA, "A should not change");
        assert_eq!(cpu_state.x, 0xBB, "X should not change");
        assert_eq!(cpu_state.y, 0xCC, "Y should not change");
        assert_eq!(cpu_state.p, 0xDD, "P should not change");
    }
}

/// KIL - Halt/Jam/Kill (Illegal Opcode)
///
/// An illegal/undocumented opcode that halts the CPU by entering an infinite loop.
/// The CPU becomes stuck and will not respond to interrupts (NMI still works).
/// Used by some games to intentionally crash on copy protection failure.
///
/// Addressing mode: Implied
/// This instruction never completes - it halts the CPU permanently.
#[derive(Debug, Clone, Copy, Default)]
pub struct Kil;

impl Kil {
    /// Create a new KIL instruction
    pub fn new() -> Self {
        Self
    }
}

impl InstructionType for Kil {
    fn is_done(&self) -> bool {
        // KIL never completes - it halts the CPU
        false
    }

    fn tick(
        &mut self,
        _cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        // KIL does nothing - it just loops forever
        // The CPU will be stuck calling tick() repeatedly
    }
}

/// SLO - Shift Left then OR (Illegal Opcode)
///
/// An illegal/undocumented opcode that performs ASL on a memory location,
/// then ORs the result with the accumulator.
/// This is a Read-Modify-Write instruction.
///
/// Operation: M = M << 1, A = A | M
/// Flags: N, Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write modified value, update accumulator and flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Slo {
    cycle: u8,
    value: u8,
}

impl Slo {
    /// Create a new SLO instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Slo {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Slo::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Get value from addressing mode
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Write original value back (dummy write)
                memory
                    .borrow_mut()
                    .write(addressing_mode.get_address(), self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Shift left, write back, OR with accumulator
                let (shifted, carry) = shift_left(self.value);

                // Write shifted value back to memory
                memory
                    .borrow_mut()
                    .write(addressing_mode.get_address(), shifted, false);

                // OR with accumulator
                cpu_state.a |= shifted;

                // Set flags based on accumulator result
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// DOP - Double NOP (Illegal Opcode)
///
/// An illegal/undocumented opcode that reads from memory but does nothing.
/// It's essentially a NOP that takes an extra cycle to read a value that is ignored.
/// Used by some programs as a skip operation (skip next byte).
///
/// Operation: Read value (do nothing with it)
/// Flags: None affected
///
/// Cycles: 1
///   1. Read value from memory (value is discarded)
#[derive(Debug, Clone, Copy, Default)]
pub struct Dop {
    cycle: u8,
}

impl Dop {
    /// Create a new DOP instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Dop {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        _cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Dop::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Do nothing, value is already read from memory
                // Value is intentionally ignored - this is a no-op
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// AND - Logical AND
///
/// Performs a bitwise AND between the accumulator and the value at the target address,
/// storing the result in the accumulator. Sets N and Z flags based on the result.
///
/// Operation: A = A & M
/// Flags: N, Z
///
/// Cycles: 1
///   1. Read value from target address, AND with A, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct And {
    cycle: u8,
}

impl And {
    /// Create a new AND instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for And {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "And::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: AND value with A
                cpu_state.a &= addressing_mode.get_u8_value();

                // Set N and Z flags based on result
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// BIT - Bit Test
///
/// Tests bits in memory with the accumulator. Sets the Zero flag if A & M is zero.
/// Copies bit 7 of memory to the N flag and bit 6 to the V flag.
///
/// Operation: Z = (A & M) == 0, N = M[7], V = M[6]
/// Flags: N, V, Z
///
/// Cycles: 1
///   1. Read value from memory, test with A, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Bit {
    cycle: u8,
}

impl Bit {
    /// Create a new BIT instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bit {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Bit::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Test bits
                let value = addressing_mode.get_u8_value();

                // Set Z flag based on A & M
                let result = cpu_state.a & value;
                set_zero_flag(&mut cpu_state.p, result);

                // Copy bit 7 to N flag
                set_negative_flag(&mut cpu_state.p, value);

                // Copy bit 6 to V flag
                cpu_state.p = (cpu_state.p & !super::types::FLAG_OVERFLOW)
                    | (value & super::types::FLAG_OVERFLOW);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// ROL - Rotate Left (Memory)
///
/// Rotates all bits left one position. The Carry flag is shifted into bit 0,
/// and bit 7 is shifted into the Carry flag.
/// This is a Read-Modify-Write instruction.
///
/// Operation: C <- M[7] <- M[6-0] <- C
/// Flags: N, Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write modified value, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Rol {
    cycle: u8,
    value: u8,
}

impl Rol {
    /// Create a new ROL instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Rol {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Rol::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Dummy write (write original value back)
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Rotate value and write back
                let old_carry = cpu_state.p & super::types::FLAG_CARRY;
                let new_carry = (self.value & 0x80) != 0;
                let rotated = (self.value << 1) | old_carry;

                // Write modified value
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, rotated, false);

                // Set flags
                set_negative_flag(&mut cpu_state.p, rotated);
                set_zero_flag(&mut cpu_state.p, rotated);
                set_carry_flag(&mut cpu_state.p, new_carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// ROL A - Rotate Left (Accumulator)
///
/// Rotates all bits of the accumulator left one position.
/// The Carry flag is shifted into bit 0, and bit 7 is shifted into the Carry flag.
///
/// Operation: C <- A[7] <- A[6-0] <- C
/// Flags: N, Z, C
///
/// Cycles: 1
///   1. Rotate accumulator and set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct RolA {
    cycle: u8,
}

impl RolA {
    /// Create a new ROL A instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for RolA {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "RolA::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Rotate accumulator
                let old_carry = cpu_state.p & super::types::FLAG_CARRY;
                let new_carry = (cpu_state.a & 0x80) != 0;
                cpu_state.a = (cpu_state.a << 1) | old_carry;

                // Set flags
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_carry_flag(&mut cpu_state.p, new_carry);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// PLP - Pull Processor Status
///
/// Pulls the processor status from the stack. Bits 4 and 5 are ignored
/// (they are always set to 0 and 1 respectively in the actual status register).
///
/// Operation: P = Stack
/// Flags: All (except B and unused which are always 0/1)
///
/// Cycles: 3
///   1. Internal operation (increment SP)
///   2. Pull status from stack
///   3. Complete
#[derive(Debug, Clone, Copy, Default)]
pub struct Plp {
    cycle: u8,
}

impl Plp {
    /// Create a new PLP instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Plp {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Plp::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Internal operation (dummy read)
                let _ = memory.borrow().read(0x0100 | (cpu_state.sp as u16));
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Increment SP
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Pull status from stack
                let addr = 0x0100 | (cpu_state.sp as u16);
                let status = memory.borrow().read(addr);

                // Set status register, but preserve bits 4 and 5
                // Bit 5 (unused) is always 1, bit 4 (B flag) is not stored in P
                cpu_state.p = (status & 0xCF) | 0x20;

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// RLA - Rotate Left then AND (Illegal Opcode)
///
/// An illegal/undocumented opcode that performs ROL on a memory location,
/// then ANDs the result with the accumulator.
/// This is a Read-Modify-Write instruction.
///
/// Operation: M = ROL(M), A = A & M
/// Flags: N, Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write modified value, AND with A, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Rla {
    cycle: u8,
    value: u8,
}

impl Rla {
    /// Create a new RLA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Rla {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Rla::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Dummy write (write original value back)
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Rotate value, write back, AND with A
                let old_carry = cpu_state.p & super::types::FLAG_CARRY;
                let new_carry = (self.value & 0x80) != 0;
                let rotated = (self.value << 1) | old_carry;

                // Write modified value
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, rotated, false);

                // AND with accumulator
                cpu_state.a &= rotated;

                // Set flags
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_carry_flag(&mut cpu_state.p, new_carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// ASL - Arithmetic Shift Left (Memory)
///
/// Shifts all bits left one position. Bit 0 is set to 0 and bit 7 is placed in the carry flag.

/// This is a Read-Modify-Write instruction.
///
/// Operation: M = M << 1
/// Flags: N, Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write modified value, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Asl {
    cycle: u8,
    value: u8,
}

impl Asl {
    /// Create a new ASL instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Asl {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Asl::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Write original value back (dummy write)
                memory
                    .borrow_mut()
                    .write(addressing_mode.get_address(), self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Shift left and write back
                let (shifted, carry) = shift_left(self.value);

                // Write shifted value back to memory
                memory
                    .borrow_mut()
                    .write(addressing_mode.get_address(), shifted, false);

                // Set flags based on result
                set_zero_flag(&mut cpu_state.p, shifted);
                set_negative_flag(&mut cpu_state.p, shifted);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// ASL A - Arithmetic Shift Left (Accumulator)
///
/// Shifts all bits of the accumulator left one position.
/// Bit 0 is set to 0 and bit 7 is placed in the carry flag.
///
/// Operation: A = A << 1
/// Flags: N, Z, C
///
/// Cycles: 1
///   1. Shift accumulator left and set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct AslA {
    cycle: u8,
}

impl AslA {
    /// Create a new ASL A instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for AslA {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "AslA::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Shift accumulator left
                let (shifted, carry) = shift_left(cpu_state.a);
                cpu_state.a = shifted;

                // Set flags based on result
                set_zero_flag(&mut cpu_state.p, shifted);
                set_negative_flag(&mut cpu_state.p, shifted);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// Helper function to perform logical shift right
/// Returns the shifted value and sets the carry flag based on bit 0
#[inline]
fn shift_right(value: u8) -> (u8, bool) {
    let carry = (value & 0x01) != 0;
    let shifted = value >> 1;
    (shifted, carry)
}

/// RTI - Return from Interrupt
///
/// Pulls the processor flags and program counter from the stack.
/// The status register is pulled with the break command flag and bit 5 ignored.
///
/// Operation: P = pull(), PC = pull_word()
/// Flags: All (restored from stack)
///
/// Cycles: 5
///   1. Read next byte (and throw it away)
///   2. Increment S
///   3. Pull P from stack
///   4. Increment S, Pull PCL from stack
///   5. Pull PCH from stack
#[derive(Debug, Clone, Copy, Default)]
pub struct Rti {
    cycle: u8,
    p: u8,
    pcl: u8,
}

impl Rti {
    /// Create a new RTI instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Rti {
    fn is_done(&self) -> bool {
        self.cycle == 5
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 5, "Rti::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read next byte (throw away)
                memory.borrow().read(cpu_state.pc);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Increment S
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Pull P from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                self.p = memory.borrow().read(stack_addr);
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 3;
            }
            3 => {
                // Cycle 4: Pull PCL from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                self.pcl = memory.borrow().read(stack_addr);
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 4;
            }
            4 => {
                // Cycle 5: Pull PCH from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                let pch = memory.borrow().read(stack_addr);
                cpu_state.pc = ((pch as u16) << 8) | (self.pcl as u16);

                // Restore processor status from stack
                // Bit 5 (unused) is always set, bit 4 (break) is always clear
                cpu_state.p = (self.p & !0x10) | 0x20;

                self.cycle = 5;
            }
            _ => unreachable!(),
        }
    }
}

/// EOR - Exclusive OR
///
/// Performs a bitwise exclusive OR between the accumulator and a value from memory.
/// The result is stored in the accumulator.
///
/// Operation: A = A ^ M
/// Flags: N, Z
///
/// Cycles: 1
///   1. EOR value with accumulator and set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Eor {
    cycle: u8,
}

impl Eor {
    /// Create a new EOR instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Eor {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Eor::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Perform EOR and set flags
                let value = addressing_mode.get_u8_value();
                cpu_state.a ^= value;

                // Set N and Z flags based on result
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// LSR - Logical Shift Right (Memory)
///
/// Shifts all bits of a memory value right one position.
/// Bit 7 is set to 0 and bit 0 is placed in the carry flag.
/// This is a Read-Modify-Write instruction.
///
/// Operation: M = M >> 1
/// Flags: N (always 0), Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write unmodified value back to memory (dummy write)
///   3. Shift right and write result
#[derive(Debug, Clone, Copy, Default)]
pub struct Lsr {
    cycle: u8,
    value: u8,
}

impl Lsr {
    /// Create a new LSR instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Lsr {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Lsr::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Dummy write (write original value back)
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Shift right and write result
                let (shifted, carry) = shift_right(self.value);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, shifted, false);

                // Set flags based on result
                set_zero_flag(&mut cpu_state.p, shifted);
                set_negative_flag(&mut cpu_state.p, shifted); // Always clears N since bit 7 is 0
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// LSR A - Logical Shift Right (Accumulator)
///
/// Shifts all bits of the accumulator right one position.
/// Bit 7 is set to 0 and bit 0 is placed in the carry flag.
///
/// Operation: A = A >> 1
/// Flags: N (always 0), Z, C
///
/// Cycles: 1
///   1. Shift accumulator right and set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct LsrA {
    cycle: u8,
}

impl LsrA {
    /// Create a new LSR A instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for LsrA {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "LsrA::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Shift accumulator right
                let (shifted, carry) = shift_right(cpu_state.a);
                cpu_state.a = shifted;

                // Set flags based on result
                set_zero_flag(&mut cpu_state.p, shifted);
                set_negative_flag(&mut cpu_state.p, shifted);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// PHA - Push Accumulator
///
/// Pushes a copy of the accumulator onto the stack.
///
/// Operation: push(A)
/// Flags: None
///
/// Cycles: 2
///   1. Read next byte (throw away)
///   2. Push A to stack
#[derive(Debug, Clone, Copy, Default)]
pub struct Pha {
    cycle: u8,
}

impl Pha {
    /// Create a new PHA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Pha {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 2, "Pha::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read next byte (throw away)
                memory.borrow().read(cpu_state.pc);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Push A to stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                memory.borrow_mut().write(stack_addr, cpu_state.a, false);
                cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }
}

/// SRE - Shift Right then EOR (Illegal Opcode)
///
/// Also known as LSE. Shifts the value right one position (LSR),
/// then performs an exclusive OR with the accumulator.
/// This is a Read-Modify-Write instruction.
///
/// Operation: M = M >> 1, A = A ^ M
/// Flags: N, Z, C
///
/// Cycles: 3
///   1. Read value from memory
///   2. Write unmodified value back to memory (dummy write)
///   3. Shift right, EOR with A, write result
#[derive(Debug, Clone, Copy, Default)]
pub struct Sre {
    cycle: u8,
    value: u8,
}

impl Sre {
    /// Create a new SRE instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sre {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Sre::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.value = addressing_mode.get_u8_value();
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Dummy write (write original value back)
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Shift right, EOR with A, write result
                let (shifted, carry) = shift_right(self.value);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, shifted, false);

                // EOR with accumulator
                cpu_state.a ^= shifted;

                // Set flags based on A
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// ASR - AND then Shift Right (Illegal Opcode)
///
/// Also known as ALR. Performs an AND between the accumulator and an immediate value,
/// then shifts the result right one position. The result is stored in the accumulator.
///
/// Operation: A = (A & M) >> 1
/// Flags: N (always 0), Z, C
///
/// Cycles: 1
///   1. AND then shift right and set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Asr {
    cycle: u8,
}

impl Asr {
    /// Create a new ASR instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Asr {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Asr::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: AND then shift right
                let value = addressing_mode.get_u8_value();
                let anded = cpu_state.a & value;
                let (shifted, carry) = shift_right(anded);
                cpu_state.a = shifted;

                // Set flags based on result
                set_zero_flag(&mut cpu_state.p, shifted);
                set_negative_flag(&mut cpu_state.p, shifted);
                set_carry_flag(&mut cpu_state.p, carry);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}
