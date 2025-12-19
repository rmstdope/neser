//! Instruction sequencing logic for cycle-accurate CPU
//!
//! This module implements the cycle-by-cycle sequencing of instructions,
//! coordinating addressing modes, operations, and instruction types.

use super::traits::{AddressingMode, Operation};
use super::types::{AddressingState, InstructionPhase, InstructionType};

/// Result of ticking one cycle of an instruction
#[derive(Debug, Clone, PartialEq)]
pub enum TickResult {
    /// Instruction is still executing, needs more cycles
    InProgress,
    /// Instruction completed in this cycle
    Complete,
}

/// Sequences a single cycle of instruction execution
///
/// This function implements the cycle-by-cycle state machine for executing
/// instructions based on their type (Read, Write, RMW, Branch, Stack, Control).
///
/// # Arguments
/// * `instruction_type` - The type of instruction being executed
/// * `phase` - Current execution phase
/// * `addressing_mode` - The addressing mode implementation
/// * `operation` - The operation implementation
/// * `pc` - Program counter
/// * `x` - X register
/// * `y` - Y register
/// * `state` - Addressing state
/// * `read_fn` - Function to read from memory
/// * `write_fn` - Function to write to memory
///
/// # Returns
/// * `(TickResult, InstructionPhase)` - Result and next phase
pub fn tick_instruction<AM: AddressingMode, OP: Operation>(
    instruction_type: InstructionType,
    phase: InstructionPhase,
    addressing_mode: &AM,
    operation: &OP,
    pc: &mut u16,
    x: u8,
    y: u8,
    cpu_state: &mut super::traits::CpuState,
    state: &mut AddressingState,
    read_fn: &dyn Fn(u16) -> u8,
    write_fn: &mut dyn FnMut(u16, u8),
) -> (TickResult, InstructionPhase) {
    match phase {
        InstructionPhase::Opcode => {
            // Opcode already fetched, move to addressing
            (TickResult::InProgress, InstructionPhase::Addressing(0))
        }
        
        InstructionPhase::Addressing(cycle) => {
            // Execute one cycle of address resolution
            if let Some(addr) = addressing_mode.tick_addressing(
                cycle,
                pc,
                x,
                y,
                state,
                read_fn,
            ) {
                // Address resolved, determine next phase based on instruction type
                state.addr = Some(addr);
                
                match instruction_type {
                    InstructionType::Read => {
                        // Read instructions: read operand then execute
                        let value = read_fn(addr);
                        state.value = Some(value);
                        operation.execute(cpu_state, value);
                        (TickResult::Complete, InstructionPhase::Opcode)
                    }
                    
                    InstructionType::Write => {
                        // Write instructions: execute (to get value) then write
                        (TickResult::InProgress, InstructionPhase::Execute)
                    }
                    
                    InstructionType::RMW => {
                        // RMW: read operand first
                        let value = read_fn(addr);
                        state.value = Some(value);
                        (TickResult::InProgress, InstructionPhase::Execute)
                    }
                    
                    InstructionType::Branch | InstructionType::Stack | InstructionType::Control => {
                        // These need special handling in Execute phase
                        (TickResult::InProgress, InstructionPhase::Execute)
                    }
                }
            } else {
                // Address not yet resolved, continue addressing
                (TickResult::InProgress, InstructionPhase::Addressing(cycle + 1))
            }
        }
        
        InstructionPhase::Execute => {
            match instruction_type {
                InstructionType::Write => {
                    // For write instructions, operation determines what to write
                    // For now, just write the accumulator (simplified)
                    let addr = state.addr.unwrap();
                    write_fn(addr, cpu_state.a);
                    (TickResult::Complete, InstructionPhase::Opcode)
                }
                
                InstructionType::RMW => {
                    // Execute RMW operation
                    let value = state.value.unwrap();
                    let result = operation.execute_rmw(cpu_state, value);
                    state.value = Some(result);
                    (TickResult::InProgress, InstructionPhase::Writeback)
                }
                
                InstructionType::Branch | InstructionType::Stack | InstructionType::Control => {
                    // These are handled specially - for now just mark complete
                    (TickResult::Complete, InstructionPhase::Opcode)
                }
                
                _ => {
                    // Should not reach here for Read (handled in Addressing phase)
                    panic!("Unexpected instruction type in Execute phase: {:?}", instruction_type);
                }
            }
        }
        
        InstructionPhase::Writeback => {
            // Write back the result (for RMW operations)
            let addr = state.addr.unwrap();
            let value = state.value.unwrap();
            write_fn(addr, value);
            (TickResult::Complete, InstructionPhase::Opcode)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newcpu::addressing::*;
    use crate::newcpu::operations::*;
    use crate::newcpu::traits::CpuState;

    #[test]
    fn test_read_instruction_immediate_mode() {
        // LDA #$42
        let addressing = Immediate;
        let operation = LDA;
        let mut pc = 0x8000;
        let mut cpu_state = CpuState { a: 0, x: 0, y: 0, sp: 0xFF, p: 0 };
        let mut state = AddressingState::default();
        
        let read_fn = |addr: u16| if addr == 0x8000 { 0x42 } else { 0x00 };
        let mut write_fn = |_addr: u16, _val: u8| {};
        
        // Start: Opcode phase
        let (result, next_phase) = tick_instruction(
            InstructionType::Read,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::InProgress);
        assert_eq!(next_phase, InstructionPhase::Addressing(0));
        
        // Addressing cycle 0: fetch immediate value and execute
        let (result, next_phase) = tick_instruction(
            InstructionType::Read,
            InstructionPhase::Addressing(0),
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::Complete);
        assert_eq!(next_phase, InstructionPhase::Opcode);
        assert_eq!(cpu_state.a, 0x42);
    }

    #[test]
    fn test_write_instruction() {
        // STA $20
        let addressing = ZeroPage;
        let operation = STA;
        let mut pc = 0x8000;
        let mut cpu_state = CpuState { a: 0x99, x: 0, y: 0, sp: 0xFF, p: 0 };
        let mut state = AddressingState::default();
        
        let read_fn = |addr: u16| if addr == 0x8000 { 0x20 } else { 0x00 };
        let mut written_addr = None;
        let mut written_value = None;
        let mut write_fn = |addr: u16, val: u8| {
            written_addr = Some(addr);
            written_value = Some(val);
        };
        
        // Opcode -> Addressing
        let (result, next_phase) = tick_instruction(
            InstructionType::Write,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::InProgress);
        assert_eq!(next_phase, InstructionPhase::Addressing(0));
        
        // Addressing: resolve address
        let (result, next_phase) = tick_instruction(
            InstructionType::Write,
            InstructionPhase::Addressing(0),
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::InProgress);
        assert_eq!(next_phase, InstructionPhase::Execute);
        
        // Execute: write value
        let (result, next_phase) = tick_instruction(
            InstructionType::Write,
            InstructionPhase::Execute,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::Complete);
        assert_eq!(next_phase, InstructionPhase::Opcode);
        assert_eq!(written_addr, Some(0x20));
        assert_eq!(written_value, Some(0x99));
    }

    #[test]
    fn test_rmw_instruction() {
        // INC $20
        let addressing = ZeroPage;
        let operation = INC;
        let mut pc = 0x8000;
        let mut cpu_state = CpuState { a: 0, x: 0, y: 0, sp: 0xFF, p: 0 };
        let mut state = AddressingState::default();
        
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x20,  // Zero page address
            0x20 => 0x42,    // Value at $20
            _ => 0x00,
        };
        
        let mut written_addr = None;
        let mut written_value = None;
        let mut write_fn = |addr: u16, val: u8| {
            written_addr = Some(addr);
            written_value = Some(val);
        };
        
        // Opcode -> Addressing
        let (_result, next_phase) = tick_instruction(
            InstructionType::RMW,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(next_phase, InstructionPhase::Addressing(0));
        
        // Addressing: resolve and read
        let (_result, next_phase) = tick_instruction(
            InstructionType::RMW,
            InstructionPhase::Addressing(0),
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(next_phase, InstructionPhase::Execute);
        assert_eq!(state.value, Some(0x42));
        
        // Execute: perform operation
        let (_result, next_phase) = tick_instruction(
            InstructionType::RMW,
            InstructionPhase::Execute,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(next_phase, InstructionPhase::Writeback);
        assert_eq!(state.value, Some(0x43)); // 0x42 + 1
        
        // Writeback: write result
        let (result, next_phase) = tick_instruction(
            InstructionType::RMW,
            InstructionPhase::Writeback,
            &addressing,
            &operation,
            &mut pc,
            0,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        
        assert_eq!(result, TickResult::Complete);
        assert_eq!(next_phase, InstructionPhase::Opcode);
        assert_eq!(written_addr, Some(0x20));
        assert_eq!(written_value, Some(0x43));
    }
}
