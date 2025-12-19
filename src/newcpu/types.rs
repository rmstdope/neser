//! Core types for cycle-accurate CPU execution
//!
//! This module defines the fundamental types used throughout the new CPU implementation,
//! separating three orthogonal concerns:
//! 1. **Addressing Modes** - How to fetch operands/addresses (timing-dependent)
//! 2. **Operations** - What to do with operands (timing-independent)  
//! 3. **Instruction Types** - Read/Write/RMW sequences (affects cycle flow)

/// Represents the current phase of instruction execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionPhase {
    /// Fetching the opcode byte (cycle 0)
    Opcode,
    /// Resolving the address for the operand
    /// The u8 indicates which cycle of addressing we're in (0-indexed)
    Addressing(u8),
    /// Executing the operation with the resolved operand
    Execute,
    /// Writing back the result (for RMW and write operations)
    Writeback,
}

/// Holds intermediate state during multi-cycle address resolution
#[derive(Debug, Clone, Default)]
pub struct AddressingState {
    /// The resolved address (once addressing phase completes)
    pub addr: Option<u16>,
    /// The value read from memory (for RMW operations)
    pub value: Option<u8>,
    /// Base address before indexing (for page crossing detection)
    pub base_addr: Option<u16>,
    /// Temporary bytes collected during address resolution
    pub temp_bytes: Vec<u8>,
}

/// Tracks the state of an instruction being executed across multiple cycles
#[derive(Debug, Clone)]
pub struct InstructionExecution {
    /// The opcode being executed
    pub opcode: u8,
    /// Current phase of execution
    pub phase: InstructionPhase,
    /// State for address resolution
    pub addressing_state: AddressingState,
    /// Number of cycles remaining for this instruction
    pub cycles_remaining: u8,
}

/// Classification of instruction types by their cycle sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionType {
    /// Read instruction: Address → Read → Execute
    Read,
    /// Write instruction: Address → Execute → Write
    Write,
    /// Read-Modify-Write: Address → Read → Execute → DummyWrite → Write
    RMW,
    /// Branch instruction with variable cycles
    Branch,
    /// Stack operation (push/pull)
    Stack,
    /// Control flow (JMP, JSR, RTS, RTI, BRK)
    Control,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_phase_enum() {
        // Test that instruction phases can be created and compared
        let opcode_phase = InstructionPhase::Opcode;
        let addressing_phase = InstructionPhase::Addressing(0);
        let execute_phase = InstructionPhase::Execute;
        let writeback_phase = InstructionPhase::Writeback;

        assert_eq!(opcode_phase, InstructionPhase::Opcode);
        assert_eq!(addressing_phase, InstructionPhase::Addressing(0));
        assert_ne!(InstructionPhase::Addressing(0), InstructionPhase::Addressing(1));
        assert_eq!(execute_phase, InstructionPhase::Execute);
        assert_eq!(writeback_phase, InstructionPhase::Writeback);
    }

    #[test]
    fn test_addressing_state_default() {
        // Test that AddressingState can be created with defaults
        let state = AddressingState::default();
        
        assert_eq!(state.addr, None);
        assert_eq!(state.value, None);
        assert_eq!(state.base_addr, None);
        assert!(state.temp_bytes.is_empty());
    }

    #[test]
    fn test_addressing_state_can_store_values() {
        // Test that AddressingState can store intermediate values
        let mut state = AddressingState::default();
        
        state.temp_bytes.push(0x34);
        state.temp_bytes.push(0x12);
        assert_eq!(state.temp_bytes.len(), 2);
        
        state.addr = Some(0x1234);
        assert_eq!(state.addr, Some(0x1234));
        
        state.value = Some(0x42);
        assert_eq!(state.value, Some(0x42));
        
        state.base_addr = Some(0x1200);
        assert_eq!(state.base_addr, Some(0x1200));
    }

    #[test]
    fn test_instruction_execution_creation() {
        // Test that InstructionExecution can be created
        let execution = InstructionExecution {
            opcode: 0xA9, // LDA immediate
            phase: InstructionPhase::Opcode,
            addressing_state: AddressingState::default(),
            cycles_remaining: 2,
        };

        assert_eq!(execution.opcode, 0xA9);
        assert_eq!(execution.phase, InstructionPhase::Opcode);
        assert_eq!(execution.cycles_remaining, 2);
    }

    #[test]
    fn test_instruction_type_enum() {
        // Test that instruction types can be compared
        assert_eq!(InstructionType::Read, InstructionType::Read);
        assert_ne!(InstructionType::Read, InstructionType::Write);
        
        let instr_type = InstructionType::RMW;
        match instr_type {
            InstructionType::RMW => { /* expected */ }
            _ => panic!("Should match RMW"),
        }
    }
}
