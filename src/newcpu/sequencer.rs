//! Instruction sequencing logic for cycle-accurate CPU
//!
//! This module implements the cycle-by-cycle sequencing of instructions,
//! coordinating addressing modes, operations, and instruction types.

use super::traits::{AddressingMode, Operation};
use super::types::{AddressingState, InstructionPhase, InstructionType};

// Interrupt vector addresses
const NMI_VECTOR: u16 = 0xFFFA; // Non-Maskable Interrupt vector
const IRQ_VECTOR: u16 = 0xFFFE; // IRQ and BRK vector

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
/// * `nmi_pending` - Whether NMI is currently pending (for interrupt hijacking)
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
    nmi_pending: bool,
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
                            let dummy_addr =
                                calculate_dummy_read_address(addressing_mode, addr, state);
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
                        // Write instructions: move to Writeback phase for the actual write
                        // (no dummy read needed for non-indexed modes, but we still need a cycle)
                        (TickResult::InProgress, InstructionPhase::Writeback)
                    }

                    InstructionType::RMW => {
                        // RMW: move to Execute for dummy read (if indexed), then real read
                        (TickResult::InProgress, InstructionPhase::Execute)
                    }

                    InstructionType::Branch => {
                        // Branch instructions: move to Execute phase for branch logic
                        (TickResult::InProgress, InstructionPhase::Execute)
                    }

                    InstructionType::Stack | InstructionType::Control => {
                        // JMP completes immediately when addressing finishes (no Execute phase needed)
                        if operation.is_jmp() {
                            let target_addr = state.addr.unwrap();
                            if let Some(new_pc) = operation.execute_control(cpu_state, target_addr)
                            {
                                *pc = new_pc;
                            }
                            (TickResult::Complete, InstructionPhase::Opcode)
                        } else {
                            // Other control/stack operations need Execute phase
                            (TickResult::InProgress, InstructionPhase::Execute)
                        }
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
                    // Read-Modify-Write cycle sequence (per NesDev wiki):
                    // 1. Fetch opcode (handled in Opcode phase)
                    // 2-3. Address resolution (handled in Addressing phase)
                    // 4. Dummy read (from wrong page if crossed) - first Execute cycle
                    // 5. Real read - second Execute cycle
                    // 6. Dummy write (original value) - first Writeback cycle
                    // 7. Real write (modified value) - second Writeback cycle
                    let addr = state.addr.unwrap();

                    if !state.dummy_read_done {
                        // Cycle 4: Dummy read from potentially incorrect address
                        let dummy_addr = calculate_dummy_read_address(addressing_mode, addr, state);
                        let _dummy = read_fn(dummy_addr);
                        state.dummy_read_done = true;
                        (TickResult::InProgress, InstructionPhase::Execute)
                    } else {
                        // Cycle 5: Real read and execute the operation
                        let value = read_fn(addr);
                        state.original_value = Some(value); // Store for dummy write

                        // Perform the operation
                        let result = operation.execute_rmw(cpu_state, value);
                        state.value = Some(result);

                        (TickResult::InProgress, InstructionPhase::Writeback)
                    }
                }

                InstructionType::Branch => {
                    // Branch instructions: BCC, BCS, BEQ, BNE, BMI, BPL, BVC, BVS
                    // Cycle sequence:
                    // 1. Fetch opcode (in Opcode phase)
                    // 2. Fetch offset operand (in Addressing phase) - already done
                    // 3. If branch not taken: complete (2 cycles total)
                    //    If branch taken: calculate new PC, add 1 cycle (3 cycles)
                    // 4. If page crossed: fix PCH, add 1 cycle (4 cycles total)
                    //
                    // Interrupt polling (per NesDev wiki):
                    // - Poll before cycle 2 (operand fetch) - handled in Opcode phase
                    // - Do NOT poll before cycle 3 (branch calculation)
                    // - Poll before cycle 4 (page fixup) if page crossed

                    // The offset was fetched and stored in temp_bytes[0] by Relative addressing
                    let offset = state.temp_bytes[0] as i8;

                    // Check if branch should be taken
                    let branch_taken = operation.execute_branch(cpu_state);

                    if !branch_taken {
                        // Branch not taken - complete immediately (2 cycles total)
                        (TickResult::Complete, InstructionPhase::Opcode)
                    } else {
                        // Branch taken - calculate new PC
                        let old_pc = *pc;
                        let new_pc = pc.wrapping_add(offset as u16);
                        *pc = new_pc;

                        // Check if page boundary was crossed
                        let page_crossed = (old_pc & 0xFF00) != (new_pc & 0xFF00);

                        if page_crossed {
                            // Page crossed - need one more cycle for fixup (4 cycles total)
                            // Interrupt will be polled at start of next instruction (cycle 4)
                            // The actual PC is already correct, but we need to consume the extra cycle
                            (TickResult::Complete, InstructionPhase::Opcode)
                        } else {
                            // No page cross - complete (3 cycles total)
                            (TickResult::Complete, InstructionPhase::Opcode)
                        }
                    }
                }

                InstructionType::Stack | InstructionType::Control => {
                    // Check if this is BRK (special interrupt-like instruction)
                    if operation.is_brk() {
                        // BRK instruction sequence:
                        // 1-2: Fetch opcode and padding byte (already done)
                        // 3-5: Push PC+2 and status with B flag to stack
                        // 6-7: Fetch interrupt vector
                        //
                        // Interrupt hijacking: If NMI is asserted during BRK execution,
                        // the B flag is still set on the stack, but execution jumps to
                        // the NMI vector instead of the IRQ/BRK vector.
                        let current_pc = *pc;
                        let (pc_high, pc_low, status) = operation.execute_brk(
                            cpu_state,
                            current_pc.wrapping_sub(1),
                            nmi_pending,
                        );

                        // Push PC+2 to stack (high byte first)
                        write_fn(0x0100 + cpu_state.sp as u16, pc_high);
                        cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                        write_fn(0x0100 + cpu_state.sp as u16, pc_low);
                        cpu_state.sp = cpu_state.sp.wrapping_sub(1);

                        // Push status with B flag set to stack
                        write_fn(0x0100 + cpu_state.sp as u16, status);
                        cpu_state.sp = cpu_state.sp.wrapping_sub(1);

                        // Vector selection happens HERE (during fetch), not at BRK start.
                        // This allows NMI to hijack BRK even if asserted mid-execution.
                        let vector_addr = if nmi_pending { NMI_VECTOR } else { IRQ_VECTOR };
                        let lo = read_fn(vector_addr);
                        let hi = read_fn(vector_addr + 1);
                        let new_pc = u16::from_le_bytes([lo, hi]);
                        *pc = new_pc;

                        (TickResult::Complete, InstructionPhase::Opcode)
                    } else if instruction_type == InstructionType::Stack {
                        // Stack push/pull operations (PHA/PHP/PLA/PLP)
                        // Push operations (PHA/PHP): 3 cycles - execute_stack returns value, write to stack
                        // Pull operations (PLA/PLP): 4 cycles - read from stack, execute_pull with value

                        if operation.is_pull() {
                            // Pull operation (PLA/PLP)
                            // 6502 stack pull: increment SP, then read from 0x0100+SP
                            cpu_state.sp = cpu_state.sp.wrapping_add(1);
                            let value = read_fn(0x0100 + cpu_state.sp as u16);
                            // Execute pull with the value to update register (SP already incremented)
                            operation.execute_pull(cpu_state, value);
                            (TickResult::Complete, InstructionPhase::Opcode)
                        } else {
                            // Push operation (PHA/PHP)
                            // 6502 stack push: write to 0x0100+SP, then decrement SP
                            let value = operation.execute_stack(cpu_state);
                            write_fn(0x0100 + cpu_state.sp as u16, value);
                            cpu_state.sp = cpu_state.sp.wrapping_sub(1);
                            (TickResult::Complete, InstructionPhase::Opcode)
                        }
                    } else {
                        // Control flow operations (JMP, JSR, RTS, RTI)
                        let target_addr = state.addr.unwrap_or(0);
                        if let Some(new_pc) = operation.execute_control(cpu_state, target_addr) {
                            *pc = new_pc;
                        }
                        (TickResult::Complete, InstructionPhase::Opcode)
                    }
                }

                _ => {
                    // Should not reach here for Read (handled in Addressing phase)
                    panic!(
                        "Unexpected instruction type in Execute phase: {:?}",
                        instruction_type
                    );
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
                    let addr = state.addr.unwrap();

                    if !state.dummy_write_done {
                        // Cycle 6: Dummy write - write back the original (unmodified) value
                        // This is a quirk of the 6502 hardware: it writes the old value
                        // back before writing the new value (per NesDev wiki)
                        let original = state.original_value.unwrap();
                        write_fn(addr, original);
                        state.dummy_write_done = true;
                        (TickResult::InProgress, InstructionPhase::Writeback)
                    } else {
                        // Cycle 7: Real write - write the modified value
                        let modified_value = state.value.unwrap();
                        write_fn(addr, modified_value);
                        (TickResult::Complete, InstructionPhase::Opcode)
                    }
                }
                _ => {
                    panic!(
                        "Unexpected instruction type in Writeback phase: {:?}",
                        instruction_type
                    );
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
            false,
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
                false,
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
            false,
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
                false,
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
            false,
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
                false,
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
        assert!(
            reads.contains(&0x1000),
            "Expected dummy read from 0x1000, got reads: {:?}",
            reads
        );

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
            false,
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
                false,
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
        assert!(
            reads.contains(&0x1001),
            "Expected dummy read from 0x1001, got reads: {:?}",
            reads
        );

        // Should write to same address
        assert_eq!(written_addr, Some(0x1001));
        assert_eq!(written_value, Some(0x42));
    }

    #[test]
    fn test_rmw_with_page_cross_dummy_read() {
        // INC $10FF,X with X=1 causes page cross ($10FF -> $1100)
        // RMW instructions should perform dummy read like write instructions
        use std::cell::RefCell;

        let addressing = AbsoluteX;
        let operation = INC;
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

        let read_addresses = RefCell::new(Vec::new());
        let read_fn = |addr: u16| {
            read_addresses.borrow_mut().push(addr);
            match addr {
                0x8000 => 0xFF, // Low byte
                0x8001 => 0x10, // High byte
                0x1100 => 0x42, // Value at final address
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
            InstructionType::RMW,
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
            false,
        );
        assert_eq!(result, TickResult::InProgress);

        // Execute all cycles
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::RMW,
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
                false,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // RMW should take 7 cycles: 1 opcode + 2 addressing + 1 dummy read + 1 real read + 1 dummy write + 1 real write
        assert_eq!(cycles, 7);

        let reads = read_addresses.borrow();
        // Should have dummy read from $1000 (wrong page)
        assert!(
            reads.contains(&0x1000),
            "Expected dummy read from 0x1000, got reads: {:?}",
            reads
        );

        // Should have real read from correct address
        assert!(
            reads.contains(&0x1100),
            "Expected real read from 0x1100, got reads: {:?}",
            reads
        );

        // Should write incremented value to correct address
        assert_eq!(written_addr, Some(0x1100));
        assert_eq!(written_value, Some(0x43)); // 0x42 + 1
    }

    #[test]
    fn test_rmw_without_page_cross_still_has_dummy_read() {
        // INC $1000,X with X=1 does not cross page ($1000 -> $1001)
        // But RMW instructions ALWAYS perform dummy read according to NesDev
        use std::cell::RefCell;

        let addressing = AbsoluteX;
        let operation = INC;
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

        let read_addresses = RefCell::new(Vec::new());
        let read_fn = |addr: u16| {
            read_addresses.borrow_mut().push(addr);
            match addr {
                0x8000 => 0x00, // Low byte
                0x8001 => 0x10, // High byte
                0x1001 => 0x42, // Value at final address
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
            InstructionType::RMW,
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
            false,
        );
        assert_eq!(result, TickResult::InProgress);

        // Execute all cycles
        let mut cycles = 1;
        loop {
            let (result, next_phase) = tick_instruction(
                InstructionType::RMW,
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
                false,
            );
            cycles += 1;
            phase = next_phase;
            if result == TickResult::Complete {
                break;
            }
        }

        // RMW should take 7 cycles even without page cross
        assert_eq!(cycles, 7);

        let reads = read_addresses.borrow();
        // Should have at least 2 reads from the same address (dummy + real)
        let reads_from_1001 = reads.iter().filter(|&&addr| addr == 0x1001).count();
        assert!(
            reads_from_1001 >= 2,
            "Expected at least 2 reads from 0x1001 (dummy + real), got {} reads: {:?}",
            reads_from_1001,
            reads
        );

        // Should write incremented value
        assert_eq!(written_addr, Some(0x1001));
        assert_eq!(written_value, Some(0x43)); // 0x42 + 1
    }
}
