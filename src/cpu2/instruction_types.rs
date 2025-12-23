//! Instruction implementations for cycle-accurate CPU
//!
//! This module contains implementations of 6502 instructions that work
//! cycle-by-cycle following the InstructionType trait.

use super::traits::InstructionType;
use super::types::{
    CpuState, FLAG_BREAK, FLAG_CARRY, FLAG_DECIMAL, FLAG_INTERRUPT, FLAG_NEGATIVE, FLAG_UNUSED,
    FLAG_ZERO, IRQ_VECTOR,
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

/// BNE - Branch if Not Equal
///
/// Branch if the zero flag (Z) is clear (not equal).
/// The relative addressing mode provides the signed offset.
///
/// Operation: Branch if Z == 0
/// Flags: None affected
///
/// Cycles:
///   - 2 if branch not taken (opcode + offset fetch)
///   - 3 if branch taken, same page (opcode + offset fetch + branch)
///   - 4 if branch taken, page cross (opcode + offset fetch + branch + fix high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Bne {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bne {
    /// Create a new BNE instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bne {
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
        debug_assert!(!self.is_done(), "Bne::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check zero flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_ZERO) == 0;

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
    fn test_jsr_completes_after_five_cycles() {
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
        for i in 1..=5 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory), &addressing_mode);
            if i < 5 {
                assert!(!jsr.is_done(), "Should not be done after cycle {}", i);
            }
        }

        assert!(jsr.is_done(), "Should be done after 5 cycles");
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
        for _ in 0..5 {
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
        for _ in 0..5 {
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

/// BVC - Branch if Overflow Clear
///
/// If the overflow flag is clear, add the relative displacement to the program counter
/// to cause a branch to a new location.
///
/// Operation: if V == 0, PC = PC + offset
/// Flags: None
///
/// Cycles: 2-4
///   1. Check V flag, if clear add offset to PC (may take extra cycle if page boundary crossed)
#[derive(Debug, Clone, Copy, Default)]
pub struct Bvc {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bvc {
    /// Create a new BVC instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bvc {
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
        debug_assert!(!self.is_done(), "Bvc::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check overflow flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_OVERFLOW) == 0;

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
                // Cycle 2: Additional cycle for branch taken (same page)
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Additional cycle for page boundary crossed
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// CLI - Clear Interrupt Disable
///
/// Clears the interrupt disable flag allowing normal interrupt requests to be serviced.
///
/// Operation: I = 0
/// Flags: I
///
/// Cycles: 1
///   1. Clear I flag
#[derive(Debug, Clone, Copy, Default)]
pub struct Cli {
    cycle: u8,
}

impl Cli {
    /// Create a new CLI instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Cli {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Cli::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Clear interrupt disable flag
                cpu_state.p &= !super::types::FLAG_INTERRUPT;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// CLD - Clear Decimal Flag
///
/// Clears the decimal flag in the status register.
///
/// Operation: D = 0
/// Flags: D
///
/// Cycles: 1
///   1. Clear decimal flag
#[derive(Debug, Clone, Copy, Default)]
pub struct Cld {
    cycle: u8,
}

impl Cld {
    /// Create a new CLD instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Cld {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Cld::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Clear decimal flag
                cpu_state.p &= !super::types::FLAG_DECIMAL;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Rts {
    cycle: u8,
}

impl Rts {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Rts {
    fn is_done(&self) -> bool {
        self.cycle == 5
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 5, "Rts::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read next byte (throw away)
                memory.borrow().read(cpu_state.pc);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Increment stack pointer
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Pull PCL from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                let pcl = memory.borrow().read(stack_addr) as u16;
                cpu_state.pc = (cpu_state.pc & 0xFF00) | pcl;
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 3;
            }
            3 => {
                // Cycle 4: Pull PCH from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                let pch = memory.borrow().read(stack_addr) as u16;
                cpu_state.pc = (pch << 8) | (cpu_state.pc & 0x00FF);
                self.cycle = 4;
            }
            4 => {
                // Cycle 5: Increment PC
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 5;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Adc {
    cycle: u8,
}

impl Adc {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Adc {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Adc::tick called after already done");

        let value = addressing_mode.get_u8_value();
        let a = cpu_state.a;
        let carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };

        let sum = a as u16 + value as u16 + carry as u16;
        let result = sum as u8;

        // Set flags
        set_zero_flag(&mut cpu_state.p, result);
        set_negative_flag(&mut cpu_state.p, result);
        set_carry_flag(&mut cpu_state.p, sum > 0xFF);

        // Set overflow flag: (A^result) & (value^result) & 0x80
        let overflow = (a ^ result) & (value ^ result) & 0x80 != 0;
        if overflow {
            cpu_state.p |= super::types::FLAG_OVERFLOW;
        } else {
            cpu_state.p &= !super::types::FLAG_OVERFLOW;
        }

        cpu_state.a = result;
        self.cycle = 1;
    }
}

/// SBC - Subtract with Carry
///
/// Subtracts a value from the accumulator with borrow (inverted carry).
///
/// Operation: A = A - M - (1 - C)
/// Flags: N, V, Z, C
///
/// Cycles: 1
#[derive(Default)]
pub struct Sbc {
    cycle: u8,
}

impl Sbc {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sbc {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sbc::tick called after already done");

        let value = addressing_mode.get_u8_value();
        let a = cpu_state.a;
        let carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };

        // SBC is A - M - (1 - C), which is the same as A + ~M + C
        let inverted_value = !value;
        let sum = a as u16 + inverted_value as u16 + carry as u16;
        let result = sum as u8;

        // Set flags
        set_zero_flag(&mut cpu_state.p, result);
        set_negative_flag(&mut cpu_state.p, result);
        set_carry_flag(&mut cpu_state.p, sum > 0xFF);

        // Set overflow flag: (A^result) & (~M^result) & 0x80
        let overflow = (a ^ result) & (inverted_value ^ result) & 0x80 != 0;
        if overflow {
            cpu_state.p |= super::types::FLAG_OVERFLOW;
        } else {
            cpu_state.p &= !super::types::FLAG_OVERFLOW;
        }

        cpu_state.a = result;
        self.cycle = 1;
    }
}

#[derive(Default)]
pub struct Ror {
    cycle: u8,
    value: u8,
    address: u16,
}

impl Ror {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Ror {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Ror::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.address = addressing_mode.get_address();
                self.value = memory.borrow().read(self.address);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Write old value back (dummy write)
                memory.borrow_mut().write(self.address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Rotate right and write new value
                let old_carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };
                let new_carry = self.value & 0x01 != 0;
                self.value = (self.value >> 1) | (old_carry << 7);

                set_zero_flag(&mut cpu_state.p, self.value);
                set_negative_flag(&mut cpu_state.p, self.value);
                set_carry_flag(&mut cpu_state.p, new_carry);

                memory.borrow_mut().write(self.address, self.value, false);
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct RorA {
    cycle: u8,
}

impl RorA {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for RorA {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "RorA::tick called after already done");

        let old_carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let new_carry = cpu_state.a & 0x01 != 0;
        cpu_state.a = (cpu_state.a >> 1) | (old_carry << 7);

        set_zero_flag(&mut cpu_state.p, cpu_state.a);
        set_negative_flag(&mut cpu_state.p, cpu_state.a);
        set_carry_flag(&mut cpu_state.p, new_carry);

        self.cycle = 1;
    }
}

#[derive(Default)]
pub struct Pla {
    cycle: u8,
}

impl Pla {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Pla {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Pla::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read next byte (throw away)
                memory.borrow().read(cpu_state.pc);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Increment stack pointer
                cpu_state.sp = cpu_state.sp.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Pull value from stack
                let stack_addr = 0x0100 | (cpu_state.sp as u16);
                cpu_state.a = memory.borrow().read(stack_addr);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Rra {
    cycle: u8,
    value: u8,
    address: u16,
}

impl Rra {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Rra {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Rra::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Read value from memory
                self.address = addressing_mode.get_address();
                self.value = memory.borrow().read(self.address);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Write old value back (dummy write)
                memory.borrow_mut().write(self.address, self.value, false);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Rotate right, ADC, and write
                let old_carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };
                let new_carry = self.value & 0x01 != 0;
                self.value = (self.value >> 1) | (old_carry << 7);

                // ADC with rotated value
                let a = cpu_state.a;
                let carry = if new_carry { 1 } else { 0 };
                let sum = a as u16 + self.value as u16 + carry as u16;
                let result = sum as u8;

                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);
                set_carry_flag(&mut cpu_state.p, sum > 0xFF);

                let overflow = (a ^ result) & (self.value ^ result) & 0x80 != 0;
                if overflow {
                    cpu_state.p |= super::types::FLAG_OVERFLOW;
                } else {
                    cpu_state.p &= !super::types::FLAG_OVERFLOW;
                }

                cpu_state.a = result;
                memory.borrow_mut().write(self.address, self.value, false);
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Arr {
    cycle: u8,
}

impl Arr {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Arr {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Arr::tick called after already done");

        let value = addressing_mode.get_u8_value();

        // AND with accumulator
        cpu_state.a &= value;

        // Rotate right using the carry flag
        let old_carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };
        cpu_state.a = (cpu_state.a >> 1) | (old_carry << 7);

        // Set flags based on the result
        set_zero_flag(&mut cpu_state.p, cpu_state.a);
        set_negative_flag(&mut cpu_state.p, cpu_state.a);
        
        // Carry is set to bit 6 of the result
        let new_carry = (cpu_state.a >> 6) & 1 != 0;
        set_carry_flag(&mut cpu_state.p, new_carry);

        // Overflow is bit 6 XOR bit 5 of the result
        let overflow = ((cpu_state.a >> 6) & 1) ^ ((cpu_state.a >> 5) & 1) != 0;
        if overflow {
            cpu_state.p |= super::types::FLAG_OVERFLOW;
        } else {
            cpu_state.p &= !super::types::FLAG_OVERFLOW;
        }

        self.cycle = 1;
    }
}

#[derive(Default)]
pub struct Bvs {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bvs {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bvs {
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
        debug_assert!(!self.is_done(), "Bvs::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check overflow flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_OVERFLOW) != 0;

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

#[derive(Default)]
pub struct Sei {
    cycle: u8,
}

impl Sei {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sei {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sei::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Set interrupt disable flag
                cpu_state.p |= super::types::FLAG_INTERRUPT;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Sta {
    cycle: u8,
}

impl Sta {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sta {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sta::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Write A to memory at the address
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, cpu_state.a, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Sty {
    cycle: u8,
}

impl Sty {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sty {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sty::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Write Y to memory at the address
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, cpu_state.y, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Stx {
    cycle: u8,
}

impl Stx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Stx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Stx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Write X to memory at the address
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, cpu_state.x, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Dey {
    cycle: u8,
}

impl Dey {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Dey {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Dey::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Decrement Y
                cpu_state.y = cpu_state.y.wrapping_sub(1);

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.y);
                set_negative_flag(&mut cpu_state.p, cpu_state.y);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Txa {
    cycle: u8,
}

impl Txa {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Txa {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Txa::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer X to A
                cpu_state.a = cpu_state.x;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Sax {
    cycle: u8,
}

impl Sax {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sax {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sax::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Store A AND X to memory (illegal opcode)
                let value = cpu_state.a & cpu_state.x;
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, value, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Xaa {
    cycle: u8,
}

impl Xaa {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Xaa {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Xaa::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: XAA - highly unstable illegal opcode
                // Basic implementation: A = (A | CONST) & X & IMM
                // Where CONST is typically 0xEE or 0xFF depending on chip/temperature
                // We'll use 0xFF for simplicity (most common behavior)
                let imm = addressing_mode.get_u8_value();
                cpu_state.a = (cpu_state.a | 0xFF) & cpu_state.x & imm;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Lda {
    cycle: u8,
}

impl Lda {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Lda {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Lda::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Load A from memory
                cpu_state.a = addressing_mode.get_u8_value();

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.a);
                set_negative_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Ldx {
    cycle: u8,
}

impl Ldx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Ldx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Ldx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Load X from memory
                cpu_state.x = addressing_mode.get_u8_value();

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Ldy {
    cycle: u8,
}

impl Ldy {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Ldy {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Ldy::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Load Y from memory
                cpu_state.y = addressing_mode.get_u8_value();

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.y);
                set_negative_flag(&mut cpu_state.p, cpu_state.y);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Tax {
    cycle: u8,
}

impl Tax {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Tax {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Tax::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer A to X
                cpu_state.x = cpu_state.a;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Tay {
    cycle: u8,
}

impl Tay {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Tay {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Tay::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer A to Y
                cpu_state.y = cpu_state.a;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.y);
                set_negative_flag(&mut cpu_state.p, cpu_state.y);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Lax {
    cycle: u8,
}

impl Lax {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Lax {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Lax::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Load both A and X from memory (illegal opcode)
                let value = addressing_mode.get_u8_value();
                cpu_state.a = value;
                cpu_state.x = value;

                // Set flags based on the loaded value
                set_zero_flag(&mut cpu_state.p, value);
                set_negative_flag(&mut cpu_state.p, value);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Atx {
    cycle: u8,
}

impl Atx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Atx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Atx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: ATX (also called OAL/ANE) - illegal, unstable opcode
                // Operation: A = X = (A OR CONST) AND IMM
                // Where CONST is typically 0xEE or 0xFF depending on chip/temperature
                // We'll use 0xFF for simplicity (most common behavior)
                let value = addressing_mode.get_u8_value();
                cpu_state.a = (cpu_state.a | 0xFF) & value;
                cpu_state.x = cpu_state.a;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Bcs {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bcs {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bcs {
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
        debug_assert!(!self.is_done(), "Bcs::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check carry flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & FLAG_CARRY) != 0;

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
                // Cycle 2: Additional cycle for branch taken (same page)
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Additional cycle for page boundary crossed
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Clv {
    cycle: u8,
}

impl Clv {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Clv {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Clv::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Clear overflow flag
                cpu_state.p &= !super::types::FLAG_OVERFLOW;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Tsx {
    cycle: u8,
}

impl Tsx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Tsx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Tsx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer SP to X
                cpu_state.x = cpu_state.sp;

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct Lar {
    cycle: u8,
}

impl Lar {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Lar {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Lar::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: LAR (also called LAS) - illegal opcode
                // Operation: A = X = SP = (SP AND M)
                let value = addressing_mode.get_u8_value();
                let result = cpu_state.sp & value;
                cpu_state.a = result;
                cpu_state.x = result;
                cpu_state.sp = result;

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// CPY - Compare Y with Memory
#[derive(Default)]
pub struct Cpy {
    cycle: u8,
}

impl Cpy {
    pub fn new() -> Self {
        Cpy { cycle: 0 }
    }
}

impl InstructionType for Cpy {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Cpy::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Compare Y with memory
                let value = addressing_mode.get_u8_value();
                let result = cpu_state.y.wrapping_sub(value);

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);
                // Carry flag: set if Y >= value (no borrow)
                if cpu_state.y >= value {
                    cpu_state.p |= FLAG_CARRY;
                } else {
                    cpu_state.p &= !FLAG_CARRY;
                }

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// CMP - Compare A with Memory
#[derive(Default)]
pub struct Cmp {
    cycle: u8,
}

impl Cmp {
    pub fn new() -> Self {
        Cmp { cycle: 0 }
    }
}

impl InstructionType for Cmp {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Cmp::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Compare A with memory
                let value = addressing_mode.get_u8_value();
                let result = cpu_state.a.wrapping_sub(value);

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);
                // Carry flag: set if A >= value (no borrow)
                if cpu_state.a >= value {
                    cpu_state.p |= FLAG_CARRY;
                } else {
                    cpu_state.p &= !FLAG_CARRY;
                }

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// CPX - Compare X with Memory
///
/// Compares the X register with a value from memory by performing a subtraction
/// (X - M) and setting the Z, C, and N flags based on the result.
///
/// Operation: X - M
/// Flags: N, Z, C
///
/// Cycles: 1
#[derive(Default)]
pub struct Cpx {
    cycle: u8,
}

impl Cpx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Cpx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Cpx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Compare X with memory
                let value = addressing_mode.get_u8_value();
                let result = cpu_state.x.wrapping_sub(value);

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);
                // Carry flag: set if X >= value (no borrow)
                if cpu_state.x >= value {
                    cpu_state.p |= FLAG_CARRY;
                } else {
                    cpu_state.p &= !FLAG_CARRY;
                }

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// DEC - Decrement Memory
#[derive(Default)]
pub struct Dec {
    cycle: u8,
    value: u8,
}

impl Dec {
    pub fn new() -> Self {
        Dec { cycle: 0, value: 0 }
    }
}

impl InstructionType for Dec {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Dec::tick called after already done");

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
                // Cycle 3: Write back decremented value
                let result = self.value.wrapping_sub(1);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, result, false);

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

// INY - Increment Y
#[derive(Default)]
pub struct Iny {
    cycle: u8,
}

impl Iny {
    pub fn new() -> Self {
        Iny { cycle: 0 }
    }
}

impl InstructionType for Iny {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Iny::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Increment Y
                cpu_state.y = cpu_state.y.wrapping_add(1);

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.y);
                set_negative_flag(&mut cpu_state.p, cpu_state.y);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// INC - Increment Memory
///
/// Increments a value in memory by 1.
///
/// Operation: M = M + 1
/// Flags: N, Z
///
/// Cycles: 3 (RMW instruction)
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write incremented value
#[derive(Default)]
pub struct Inc {
    cycle: u8,
    value: u8,
}

impl Inc {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Inc {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Inc::tick called after already done");

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
                // Cycle 3: Write back incremented value
                let result = self.value.wrapping_add(1);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, result, false);

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// INX - Increment X
///
/// Increments the X register by 1.
///
/// Operation: X = X + 1
/// Flags: N, Z
///
/// Cycles: 1
#[derive(Default)]
pub struct Inx {
    cycle: u8,
}

impl Inx {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Inx {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Inx::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Increment X
                cpu_state.x = cpu_state.x.wrapping_add(1);

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// DEX - Decrement X
#[derive(Default)]
pub struct Dex {
    cycle: u8,
}

impl Dex {
    pub fn new() -> Self {
        Dex { cycle: 0 }
    }
}

impl InstructionType for Dex {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Dex::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Decrement X
                cpu_state.x = cpu_state.x.wrapping_sub(1);

                // Set flags
                set_zero_flag(&mut cpu_state.p, cpu_state.x);
                set_negative_flag(&mut cpu_state.p, cpu_state.x);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// AXS - Illegal: (A AND X) - imm -> X
#[derive(Default)]
pub struct Axs {
    cycle: u8,
}

impl Axs {
    pub fn new() -> Self {
        Axs { cycle: 0 }
    }
}

impl InstructionType for Axs {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Axs::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: X = (A AND X) - imm
                let value = addressing_mode.get_u8_value();
                let temp = cpu_state.a & cpu_state.x;
                let result = temp.wrapping_sub(value);
                cpu_state.x = result;

                // Set flags
                set_zero_flag(&mut cpu_state.p, result);
                set_negative_flag(&mut cpu_state.p, result);
                // Carry flag: set if no borrow (temp >= value)
                if temp >= value {
                    cpu_state.p |= FLAG_CARRY;
                } else {
                    cpu_state.p &= !FLAG_CARRY;
                }

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

// DCP - Illegal: DEC then CMP
#[derive(Default)]
pub struct Dcp {
    cycle: u8,
    value: u8,
}

impl Dcp {
    pub fn new() -> Self {
        Dcp { cycle: 0, value: 0 }
    }
}

impl InstructionType for Dcp {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Dcp::tick called after already done");

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
                // Cycle 3: Decrement and write back, then compare with A
                let result = self.value.wrapping_sub(1);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, result, false);

                // Compare A with decremented value
                let cmp_result = cpu_state.a.wrapping_sub(result);

                // Set flags based on comparison
                set_zero_flag(&mut cpu_state.p, cmp_result);
                set_negative_flag(&mut cpu_state.p, cmp_result);
                // Carry flag: set if A >= result (no borrow)
                if cpu_state.a >= result {
                    cpu_state.p |= FLAG_CARRY;
                } else {
                    cpu_state.p &= !FLAG_CARRY;
                }

                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// ISB - Illegal: INC then SBC
///
/// Increments a value in memory, then performs SBC with the result.
///
/// Operation: M = M + 1, A = A - M - (1 - C)
/// Flags: N, V, Z, C
///
/// Cycles: 3 (RMW instruction)
///   1. Read value from memory
///   2. Write original value back (dummy write)
///   3. Write incremented value and perform SBC
#[derive(Default)]
pub struct Isb {
    cycle: u8,
    value: u8,
}

impl Isb {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Isb {
    fn is_done(&self) -> bool {
        self.cycle == 3
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 3, "Isb::tick called after already done");

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
                // Cycle 3: Increment and write back, then perform SBC with A
                let result = self.value.wrapping_add(1);
                let address = addressing_mode.get_address();
                memory.borrow_mut().write(address, result, false);

                // Perform SBC: A = A - result - (1 - C)
                let a = cpu_state.a;
                let carry = if cpu_state.p & FLAG_CARRY != 0 { 1 } else { 0 };

                // SBC is A - M - (1 - C), which is the same as A + ~M + C
                let inverted_value = !result;
                let sum = a as u16 + inverted_value as u16 + carry as u16;
                let sbc_result = sum as u8;

                // Set flags
                set_zero_flag(&mut cpu_state.p, sbc_result);
                set_negative_flag(&mut cpu_state.p, sbc_result);
                set_carry_flag(&mut cpu_state.p, sum > 0xFF);

                // Set overflow flag
                let overflow = (a ^ sbc_result) & (inverted_value ^ sbc_result) & 0x80 != 0;
                if overflow {
                    cpu_state.p |= super::types::FLAG_OVERFLOW;
                } else {
                    cpu_state.p &= !super::types::FLAG_OVERFLOW;
                }

                cpu_state.a = sbc_result;
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }
}

/// BEQ - Branch if Equal
///
/// Branch if the zero flag (Z) is set (equal).
/// The relative addressing mode provides the signed offset.
///
/// Operation: Branch if Z == 1
/// Flags: None affected
///
/// Cycles:
///   - 2 if branch not taken (opcode + offset fetch)
///   - 3 if branch taken, same page (opcode + offset fetch + branch)
///   - 4 if branch taken, page cross (opcode + offset fetch + branch + fix high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Beq {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Beq {
    /// Create a new BEQ instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Beq {
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
        debug_assert!(!self.is_done(), "Beq::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check zero flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_ZERO) != 0;

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

/// SED - Set Decimal Flag
///
/// Sets the decimal flag in the status register.
/// Note: The NES 6502 doesn't actually support BCD mode, so this flag has no effect,
/// but some games still use it for various reasons.
///
/// Operation: D = 1
/// Flags: D
///
/// Cycles: 1
///   1. Set decimal flag
#[derive(Debug, Clone, Copy, Default)]
pub struct Sed {
    cycle: u8,
}

impl Sed {
    /// Create a new SED instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sed {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sed::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Set decimal flag
                cpu_state.p |= FLAG_DECIMAL;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// BCC - Branch if Carry Clear
///
/// Branch if the carry flag (C) is clear.
/// The relative addressing mode provides the signed offset.
///
/// Operation: Branch if C == 0
/// Flags: None affected
///
/// Cycles:
///   - 2 if branch not taken (opcode + offset fetch)
///   - 3 if branch taken, same page (opcode + offset fetch + branch)
///   - 4 if branch taken, page cross (opcode + offset fetch + branch + fix high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Bcc {
    cycle: u8,
    branch_taken: bool,
    page_crossed: bool,
}

impl Bcc {
    /// Create a new BCC instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Bcc {
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
        debug_assert!(!self.is_done(), "Bcc::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Check carry flag and decide if branch is taken
                self.branch_taken = (cpu_state.p & super::types::FLAG_CARRY) == 0;

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

/// TYA - Transfer Y to Accumulator
///
/// Copies the value from the Y register to the accumulator.
/// Sets N and Z flags based on the result.
///
/// Operation: A = Y
/// Flags: N, Z
///
/// Cycles: 1
///   1. Transfer Y to A, set flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Tya {
    cycle: u8,
}

impl Tya {
    /// Create a new TYA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Tya {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Tya::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer Y to A
                cpu_state.a = cpu_state.y;

                // Set N and Z flags
                set_negative_flag(&mut cpu_state.p, cpu_state.a);
                set_zero_flag(&mut cpu_state.p, cpu_state.a);

                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// TXS - Transfer X to Stack Pointer
///
/// Copies the value from the X register to the stack pointer.
/// Does not affect any flags.
///
/// Operation: SP = X
/// Flags: None affected
///
/// Cycles: 1
///   1. Transfer X to SP
#[derive(Debug, Clone, Copy, Default)]
pub struct Txs {
    cycle: u8,
}

impl Txs {
    /// Create a new TXS instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Txs {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        _memory: Rc<RefCell<MemController>>,
        _addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Txs::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Transfer X to SP
                cpu_state.sp = cpu_state.x;
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// AXA - AND X with Accumulator then AND with 7 (Illegal Opcode)
///
/// Also known as SHA or AHX. Stores A & X & (high_byte + 1) to memory.
/// This is an unstable illegal opcode - behavior can vary.
///
/// Operation: M = A & X & (H + 1)
/// Flags: None affected
///
/// Cycles: 1
///   1. Store A & X & (H + 1)
#[derive(Debug, Clone, Copy, Default)]
pub struct Axa {
    cycle: u8,
}

impl Axa {
    /// Create a new AXA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Axa {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Axa::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Store A & X & (high_byte + 1)
                // Note: high_byte must be from BASE address (before indexing), not final address
                let address = addressing_mode.get_address();
                // For indirect indexed (base + Y), remove Y to get base high byte
                let base_high_byte = ((address.wrapping_sub(cpu_state.y as u16)) >> 8) as u8;
                let value = cpu_state.a & cpu_state.x & base_high_byte.wrapping_add(1);
                memory.borrow_mut().write(address, value, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// SXA - Store X AND (high-byte + 1) (Illegal Opcode)
///
/// Also known as SHX or XAS. Stores X & (high_byte + 1) to memory.
/// This is an unstable illegal opcode.
///
/// Operation: M = X & (H + 1)
/// Flags: None affected
///
/// Cycles: 1
///   1. Store X & (H + 1)
#[derive(Debug, Clone, Copy, Default)]
pub struct Sxa {
    cycle: u8,
}

impl Sxa {
    /// Create a new SXA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sxa {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sxa::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Store X & (high_byte + 1)
                // Note: high_byte must be from BASE address (before indexing), not final address
                let address = addressing_mode.get_address();
                // For absolute,Y indexed (base + Y), remove Y to get base high byte
                let base_high_byte = ((address.wrapping_sub(cpu_state.y as u16)) >> 8) as u8;
                let value = cpu_state.x & base_high_byte.wrapping_add(1);
                memory.borrow_mut().write(address, value, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// SYA - Store Y AND (high-byte + 1) (Illegal Opcode)
///
/// Also known as SHY or SAY. Stores Y & (high_byte + 1) to memory.
/// This is an unstable illegal opcode.
///
/// Operation: M = Y & (H + 1)
/// Flags: None affected
///
/// Cycles: 1
///   1. Store Y & (H + 1)
#[derive(Debug, Clone, Copy, Default)]
pub struct Sya {
    cycle: u8,
}

impl Sya {
    /// Create a new SYA instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Sya {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Sya::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Store Y & (high_byte + 1)
                // Note: high_byte must be from BASE address (before indexing), not final address
                let address = addressing_mode.get_address();
                // For absolute,X indexed (base + X), remove X to get base high byte
                let base_high_byte = ((address.wrapping_sub(cpu_state.x as u16)) >> 8) as u8;
                let value = cpu_state.y & base_high_byte.wrapping_add(1);
                memory.borrow_mut().write(address, value, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}

/// XAS - Transfer A AND X to SP, then store SP AND (high-byte + 1) (Illegal Opcode)
///
/// Also known as SHS or TAS. Sets SP = A & X, then stores SP & (high_byte + 1) to memory.
/// This is an unstable illegal opcode.
///
/// Operation: SP = A & X, M = SP & (H + 1)
/// Flags: None affected
///
/// Cycles: 1
///   1. Set SP and store result
#[derive(Debug, Clone, Copy, Default)]
pub struct Xas {
    cycle: u8,
}

impl Xas {
    /// Create a new XAS instruction
    pub fn new() -> Self {
        Self::default()
    }
}

impl InstructionType for Xas {
    fn is_done(&self) -> bool {
        self.cycle == 1
    }

    fn tick(
        &mut self,
        cpu_state: &mut CpuState,
        memory: Rc<RefCell<MemController>>,
        addressing_mode: &dyn super::traits::AddressingMode,
    ) {
        debug_assert!(self.cycle < 1, "Xas::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Set SP = A & X, then store SP & (high_byte + 1)
                // Note: high_byte must be from BASE address (before indexing), not final address
                cpu_state.sp = cpu_state.a & cpu_state.x;
                let address = addressing_mode.get_address();
                // For absolute,Y indexed (base + Y), remove Y to get base high byte
                let base_high_byte = ((address.wrapping_sub(cpu_state.y as u16)) >> 8) as u8;
                let value = cpu_state.sp & base_high_byte.wrapping_add(1);
                memory.borrow_mut().write(address, value, false);
                self.cycle = 1;
            }
            _ => unreachable!(),
        }
    }
}
