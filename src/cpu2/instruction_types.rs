//! Instruction implementations for cycle-accurate CPU
//!
//! This module contains implementations of 6502 instructions that work
//! cycle-by-cycle following the InstructionType trait.

use super::traits::InstructionType;
use super::types::CpuState;
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

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
        self.cycle == 6
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
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
                // Cycle 5: Fetch high byte of target address from PC
                let high = memory.borrow().read(cpu_state.pc);
                self.target_address |= (high as u16) << 8;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 5;
            }
            5 => {
                // Cycle 6: Copy target address to PC
                cpu_state.pc = self.target_address;
                self.cycle = 6;
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;
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
        for i in 1..=6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory));
            if i < 6 {
                assert!(!jsr.is_done(), "Should not be done after cycle {}", i);
            }
        }

        assert!(jsr.is_done(), "Should be done after 6 cycles");
        assert_eq!(cpu_state.pc, 0x1234, "PC should be set to target address");
        assert_eq!(cpu_state.sp, 0xFB, "Stack pointer should have decremented by 2");

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

        for _ in 0..6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory));
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

        for _ in 0..6 {
            jsr.tick(&mut cpu_state, Rc::clone(&memory));
        }

        assert!(jsr.is_done());
        assert_eq!(cpu_state.pc, 0x0600);
        assert_eq!(cpu_state.sp, 0xFF, "Stack pointer should wrap around");
    }
}
