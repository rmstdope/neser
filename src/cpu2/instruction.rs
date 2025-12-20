//! Instruction implementations for cycle-accurate CPU
//!
//! This module contains implementations of 6502 instructions that work
//! cycle-by-cycle following the InstructionType trait.

use super::traits::{AddressingMode, InstructionType};
use super::types::CpuState;
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

/// Instruction struct that combines an addressing mode with an instruction type
///
/// This represents a complete 6502 instruction with its addressing mode.
/// The tick() function coordinates between the addressing mode and the instruction execution.
pub struct Instruction {
    addressing_mode: Box<dyn AddressingMode>,
    instruction_type: Box<dyn InstructionType>,
}

impl Instruction {
    /// Create a new Instruction with the given addressing mode and instruction type
    pub fn new(
        addressing_mode: Box<dyn AddressingMode>,
        instruction_type: Box<dyn InstructionType>,
    ) -> Self {
        Self {
            addressing_mode,
            instruction_type,
        }
    }

    /// Tick the instruction by one cycle
    ///
    /// Coordinates between addressing mode and instruction execution:
    /// 1. First, ticks the addressing mode until it's done
    /// 2. Then, ticks the instruction type until it's done
    pub fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(
            !self.is_done(),
            "Instruction::tick called when instruction is already done"
        );

        // First, complete the addressing mode if not done
        if !self.addressing_mode.is_done() {
            self.addressing_mode.tick(cpu_state, Rc::clone(&memory));
        }
        // Once addressing is done, execute the instruction
        else if !self.instruction_type.is_done() {
            self.instruction_type.tick(cpu_state, memory);
        }
    }

    /// Check if both the addressing mode and instruction execution are complete
    pub fn is_done(&self) -> bool {
        self.addressing_mode.is_done() && self.instruction_type.is_done()
    }
}
