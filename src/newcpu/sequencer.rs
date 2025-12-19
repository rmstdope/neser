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

/// Calculate the dummy read address for indexed addressing modes with page crossing
/// Returns the address that should be used for the dummy read cycle
fn calculate_dummy_read_address<AM: AddressingMode + ?Sized>(
    addressing_mode: &AM,
    final_addr: u16,
    state: &AddressingState,
) -> u16 {
    if !addressing_mode.has_page_cross_penalty() {
        // Non-indexed addressing: read from final address
        return final_addr;
    }

    if let Some(base_addr) = state.base_addr {
        let page_crossed = (base_addr & 0xFF00) != (final_addr & 0xFF00);
        if page_crossed {
            // Page crossed: read from base page with final address low byte
            (base_addr & 0xFF00) | (final_addr & 0x00FF)
        } else {
            // No page cross: read from final address
            final_addr
        }
    } else {
        final_addr
    }
}

/// Check if a page boundary was crossed during indexed addressing
fn is_page_crossed<AM: AddressingMode + ?Sized>(
    addressing_mode: &AM,
    state: &AddressingState,
    addr: u16,
) -> bool {
    if !addressing_mode.has_page_cross_penalty() {
        return false;
    }

    if let Some(base_addr) = state.base_addr {
        (base_addr & 0xFF00) != (addr & 0xFF00)
    } else {
        false
    }
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
pub fn tick_instruction<AM: AddressingMode + ?Sized, OP: Operation + ?Sized>(
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
            if let Some(addr) = addressing_mode.tick_addressing(cycle, pc, x, y, state, read_fn) {
                // Address resolved, determine next phase based on instruction type
                state.addr = Some(addr);

                match instruction_type {
                    InstructionType::Read => {
                        // Check for page cross penalty
                        if is_page_crossed(addressing_mode, state, addr) {
                            // Page crossed: add extra cycle with dummy read from wrong address
                            let dummy_addr = calculate_dummy_read_address(addressing_mode, addr, state);
                            let _dummy = read_fn(dummy_addr);
                            // Move to Execute phase for the actual read
                            (TickResult::InProgress, InstructionPhase::Execute)
                        } else {
                            // No page cross: read operand and execute immediately
                            let value = read_fn(addr);
                            state.value = Some(value);
                            operation.execute(cpu_state, value);
                            (TickResult::Complete, InstructionPhase::Opcode)
                        }
                    }

                    InstructionType::Write => {
                        // Write instructions: move to Execute for dummy read, then Writeback for write
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
                (
                    TickResult::InProgress,
                    InstructionPhase::Addressing(cycle + 1),
                )
            }
        }

        InstructionPhase::Execute => {
            match instruction_type {
                InstructionType::Read => {
                    // Read instruction with page cross penalty: do the actual read now
                    let addr = state.addr.unwrap();
                    let value = read_fn(addr);
                    state.value = Some(value);
                    operation.execute(cpu_state, value);
                    (TickResult::Complete, InstructionPhase::Opcode)
                }

                InstructionType::Write => {
                    // Write instructions: perform dummy read, then move to Writeback for actual write
                    let addr = state.addr.unwrap();
                    let dummy_addr = calculate_dummy_read_address(addressing_mode, addr, state);
                    let _dummy = read_fn(dummy_addr);
                    (TickResult::InProgress, InstructionPhase::Writeback)
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
            }
        }

        InstructionPhase::Writeback => {
            match instruction_type {
                InstructionType::Write => {
                    // For write instructions, write the accumulator (simplified for now)
                    let addr = state.addr.unwrap();
                    write_fn(addr, cpu_state.a);
                    (TickResult::Complete, InstructionPhase::Opcode)
                }
                InstructionType::RMW => {
                    // For RMW operations, write back the modified value
                    let addr = state.addr.unwrap();
                    let value = state.value.unwrap();
                    write_fn(addr, value);
                    (TickResult::Complete, InstructionPhase::Opcode)
                }
                _ => {
                    panic!("Unexpected instruction type in Writeback phase: {:?}", instruction_type);
                }
            }
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
    fn test_read_with_page_cross_penalty() {
        // LDA $10FF,X with X=1 causes page cross ($10FF -> $1100)
        let addressing = AbsoluteX;
        let operation = LDA;
        let mut pc = 0x8000;
        let x = 1;
        let mut cpu_state = CpuState {
            a: 0,
            x,
            y: 0,
            sp: 0xFF,
            p: 0,
        };
        let mut state = AddressingState::default();

        let read_fn = |addr: u16| match addr {
            0x8000 => 0xFF, // Low byte
            0x8001 => 0x10, // High byte
            0x1100 => 0x42, // Correct address after page cross
            0x1000 => 0x00, // Dummy read from wrong page
            _ => 0xFF,
        };
        let mut write_fn = |_addr: u16, _val: u8| {};

        // Opcode phase
        let (result, mut phase) = tick_instruction(
            InstructionType::Read,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            x,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(result, TickResult::InProgress);

        // Addressing cycles and page cross fixup
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::Read,
                phase,
                &addressing,
                &operation,
                &mut pc,
                x,
                0,
                &mut cpu_state,
                &mut state,
                &read_fn,
                &mut write_fn,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // Should take 4 cycles total: 1 opcode + 2 addressing + 1 page cross penalty
        assert_eq!(cycles, 4);
        assert_eq!(cpu_state.a, 0x42);
    }

    #[test]
    fn test_read_without_page_cross() {
        // LDA $1000,X with X=1 does not cross page ($1000 -> $1001)
        let addressing = AbsoluteX;
        let operation = LDA;
        let mut pc = 0x8000;
        let x = 1;
        let mut cpu_state = CpuState {
            a: 0,
            x,
            y: 0,
            sp: 0xFF,
            p: 0,
        };
        let mut state = AddressingState::default();

        let read_fn = |addr: u16| match addr {
            0x8000 => 0x00, // Low byte
            0x8001 => 0x10, // High byte
            0x1001 => 0x42, // Correct address, no page cross
            _ => 0xFF,
        };
        let mut write_fn = |_addr: u16, _val: u8| {};

        // Opcode phase
        let (result, mut phase) = tick_instruction(
            InstructionType::Read,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            x,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(result, TickResult::InProgress);

        // Addressing cycles
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::Read,
                phase,
                &addressing,
                &operation,
                &mut pc,
                x,
                0,
                &mut cpu_state,
                &mut state,
                &read_fn,
                &mut write_fn,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // Should take 3 cycles total: 1 opcode + 2 addressing (no penalty)
        assert_eq!(cycles, 3);
        assert_eq!(cpu_state.a, 0x42);
    }

    #[test]
    fn test_write_with_page_cross_dummy_read() {
        // STA $10FF,X with X=1 causes page cross ($10FF -> $1100)
        // Should perform dummy read from $1000 before writing to $1100
        use std::cell::RefCell;
        
        let addressing = AbsoluteX;
        let operation = STA;
        let mut pc = 0x8000;
        let x = 1;
        let mut cpu_state = CpuState {
            a: 0x42,
            x,
            y: 0,
            sp: 0xFF,
            p: 0,
        };
        let mut state = AddressingState::default();

        let read_addresses = RefCell::new(Vec::new());
        let read_fn = |addr: u16| {
            read_addresses.borrow_mut().push(addr);
            match addr {
                0x8000 => 0xFF, // Low byte
                0x8001 => 0x10, // High byte
                _ => 0x00,
            }
        };

        let mut written_addr = None;
        let mut written_value = None;
        let mut write_fn = |addr: u16, val: u8| {
            written_addr = Some(addr);
            written_value = Some(val);
        };

        // Opcode phase
        let (result, mut phase) = tick_instruction(
            InstructionType::Write,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            x,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(result, TickResult::InProgress);

        // Execute all cycles
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::Write,
                phase,
                &addressing,
                &operation,
                &mut pc,
                x,
                0,
                &mut cpu_state,
                &mut state,
                &read_fn,
                &mut write_fn,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // Should take 5 cycles: 1 opcode + 2 addressing + 1 dummy read + 1 write
        assert_eq!(cycles, 5);
        
        let reads = read_addresses.borrow();
        // Should have dummy read from $1000 (wrong page)
        assert!(reads.contains(&0x1000), 
                "Expected dummy read from 0x1000, got reads: {:?}", reads);
        
        // Should write to correct address
        assert_eq!(written_addr, Some(0x1100));
        assert_eq!(written_value, Some(0x42));
    }

    #[test]
    fn test_write_without_page_cross_still_has_dummy_read() {
        // STA $1000,X with X=1 does not cross page ($1000 -> $1001)
        // But write instructions ALWAYS perform dummy read according to NesDev
        use std::cell::RefCell;
        
        let addressing = AbsoluteX;
        let operation = STA;
        let mut pc = 0x8000;
        let x = 1;
        let mut cpu_state = CpuState {
            a: 0x42,
            x,
            y: 0,
            sp: 0xFF,
            p: 0,
        };
        let mut state = AddressingState::default();

        let read_addresses = RefCell::new(Vec::new());
        let read_fn = |addr: u16| {
            read_addresses.borrow_mut().push(addr);
            match addr {
                0x8000 => 0x00, // Low byte
                0x8001 => 0x10, // High byte
                _ => 0x00,
            }
        };

        let mut written_addr = None;
        let mut written_value = None;
        let mut write_fn = |addr: u16, val: u8| {
            written_addr = Some(addr);
            written_value = Some(val);
        };

        // Opcode phase
        let (result, mut phase) = tick_instruction(
            InstructionType::Write,
            InstructionPhase::Opcode,
            &addressing,
            &operation,
            &mut pc,
            x,
            0,
            &mut cpu_state,
            &mut state,
            &read_fn,
            &mut write_fn,
        );
        assert_eq!(result, TickResult::InProgress);

        // Execute all cycles
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::Write,
                phase,
                &addressing,
                &operation,
                &mut pc,
                x,
                0,
                &mut cpu_state,
                &mut state,
                &read_fn,
                &mut write_fn,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // Should take 5 cycles even without page cross: 1 opcode + 2 addressing + 1 dummy read + 1 write
        assert_eq!(cycles, 5);
        
        let reads = read_addresses.borrow();
        // Should have dummy read from $1001 (same address we'll write to)
        assert!(reads.contains(&0x1001), 
                "Expected dummy read from 0x1001, got reads: {:?}", reads);
        
        // Should write to same address
        assert_eq!(written_addr, Some(0x1001));
        assert_eq!(written_value, Some(0x42));
    }
}
