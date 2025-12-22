//! Core types for cycle-accurate CPU execution
//!
//! This module defines the fundamental types used throughout the new CPU implementation,
//! separating three orthogonal concerns:
//! 1. **Addressing Modes** - How to fetch operands/addresses (timing-dependent)
//! 2. **Operations** - What to do with operands (timing-independent)  
//! 3. **Instruction Types** - Read/Write/RMW sequences (affects cycle flow)

// Status register flags
pub const FLAG_CARRY: u8 = 0b0000_0001;
pub const FLAG_ZERO: u8 = 0b0000_0010;
pub const FLAG_INTERRUPT: u8 = 0b0000_0100;
pub const FLAG_DECIMAL: u8 = 0b0000_1000;
pub const FLAG_BREAK: u8 = 0b0001_0000;
pub const FLAG_UNUSED: u8 = 0b0010_0000;
pub const FLAG_OVERFLOW: u8 = 0b0100_0000;
pub const FLAG_NEGATIVE: u8 = 0b1000_0000;

// Interrupt vectors
pub const NMI_VECTOR: u16 = 0xFFFA;
pub const RESET_VECTOR: u16 = 0xFFFC;
pub const IRQ_VECTOR: u16 = 0xFFFE;

/// Represents the complete state of the CPU registers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuState {
    /// Accumulator register
    pub a: u8,
    /// X index register
    pub x: u8,
    /// Y index register
    pub y: u8,
    /// Stack pointer (points to next free location, grows downward from 0x01FF)
    pub sp: u8,
    /// Program counter
    pub pc: u16,
    /// Status register (processor flags)
    /// Bit 7: N (Negative)
    /// Bit 6: V (Overflow)
    /// Bit 5: - (unused, always 1)
    /// Bit 4: B (Break)
    /// Bit 3: D (Decimal mode, not used on NES)
    /// Bit 2: I (Interrupt disable)
    /// Bit 1: Z (Zero)
    /// Bit 0: C (Carry)
    pub p: u8,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            pc: 0,
            p: 0,
        }
    }
}

// /// Represents the current phase of instruction execution
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionPhase {
//     /// Fetching the opcode byte (cycle 0)
//     Opcode,
//     /// Resolving the address for the operand
//     /// The u8 indicates which cycle of addressing we're in (0-indexed)
//     Addressing(u8),
//     /// Executing the operation with the resolved operand
//     Execute,
//     /// Writing back the result (for RMW and write operations)
//     Writeback,
// }

// /// Holds intermediate state during multi-cycle address resolution
// #[derive(Debug, Clone)]
// pub struct AddressingState {
//     /// The resolved address (once addressing phase completes)
//     pub addr: Option<u16>,
//     /// The value read from memory (for RMW operations)
//     pub value: Option<u8>,
//     /// The original value before modification (for RMW dummy write)
//     pub original_value: Option<u8>,
//     /// Base address before indexing (for page crossing detection)
//     pub base_addr: Option<u16>,
//     /// Temporary bytes collected during address resolution (max 4 bytes needed)
//     pub temp_bytes: [u8; 4],
//     /// Track if dummy read has been performed (for RMW operations)
//     pub dummy_read_done: bool,
//     /// Track if dummy write has been performed (for RMW operations)
//     pub dummy_write_done: bool,
// }

// impl Default for AddressingState {
//     fn default() -> Self {
//         Self {
//             addr: None,
//             value: None,
//             original_value: None,
//             base_addr: None,
//             temp_bytes: [0; 4],
//             dummy_read_done: false,
//             dummy_write_done: false,
//         }
//     }
// }

// /// Tracks the state of an instruction being executed across multiple cycles
// #[derive(Debug, Clone)]
// pub struct InstructionExecution {
//     /// The opcode being executed
//     pub opcode: u8,
//     /// Current phase of execution
//     pub phase: InstructionPhase,
//     /// State for address resolution
//     pub addressing_state: AddressingState,
//     /// Number of cycles remaining for this instruction
//     pub cycles_remaining: u8,
// }

// /// Classification of instruction types by their cycle sequence
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionType {
//     /// Read instruction: Address → Read → Execute
//     Read,
//     /// Write instruction: Address → Execute → Write
//     Write,
//     /// Read-Modify-Write: Address → Read → Execute → DummyWrite → Write
//     RMW,
//     /// Branch instruction with variable cycles
//     Branch,
// /// Represents the current phase of instruction execution
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionPhase {
//     /// Fetching the opcode byte (cycle 0)
//     Opcode,
//     /// Resolving the address for the operand
//     /// The u8 indicates which cycle of addressing we're in (0-indexed)
//     Addressing(u8),
//     /// Executing the operation with the resolved operand
//     Execute,
//     /// Writing back the result (for RMW and write operations)
//     Writeback,
// }

// /// Holds intermediate state during multi-cycle address resolution
// #[derive(Debug, Clone)]
// pub struct AddressingState {
//     /// The resolved address (once addressing phase completes)
//     pub addr: Option<u16>,
//     /// The value read from memory (for RMW operations)
//     pub value: Option<u8>,
//     /// The original value before modification (for RMW dummy write)
//     pub original_value: Option<u8>,
//     /// Base address before indexing (for page crossing detection)
//     pub base_addr: Option<u16>,
//     /// Temporary bytes collected during address resolution (max 4 bytes needed)
//     pub temp_bytes: [u8; 4],
//     /// Track if dummy read has been performed (for RMW operations)
//     pub dummy_read_done: bool,
//     /// Track if dummy write has been performed (for RMW operations)
//     pub dummy_write_done: bool,
// }

// impl Default for AddressingState {
//     fn default() -> Self {
//         Self {
//             addr: None,
//             value: None,
//             original_value: None,
//             base_addr: None,
//             temp_bytes: [0; 4],
//             dummy_read_done: false,
//             dummy_write_done: false,
//         }
//     }
// }

// /// Tracks the state of an instruction being executed across multiple cycles
// #[derive(Debug, Clone)]
// pub struct InstructionExecution {
//     /// The opcode being executed
//     pub opcode: u8,
//     /// Current phase of execution
//     pub phase: InstructionPhase,
//     /// State for address resolution
//     pub addressing_state: AddressingState,
//     /// Number of cycles remaining for this instruction
//     pub cycles_remaining: u8,
// }

// /// Classification of instruction types by their cycle sequence
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionType {
//     /// Read instruction: Address → Read → Execute
//     Read,
//     /// Write instruction: Address → Execute → Write
//     Write,
//     /// Read-Modify-Write: Address → Read → Execute → DummyWrite → Write
//     RMW,
//     /// Branch instruction with variable cycles
//     Branch,
// /// Represents the current phase of instruction execution
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionPhase {
//     /// Fetching the opcode byte (cycle 0)
//     Opcode,
//     /// Resolving the address for the operand
//     /// The u8 indicates which cycle of addressing we're in (0-indexed)
//     Addressing(u8),
//     /// Executing the operation with the resolved operand
//     Execute,
//     /// Writing back the result (for RMW and write operations)
//     Writeback,
// }

// /// Holds intermediate state during multi-cycle address resolution
// #[derive(Debug, Clone)]
// pub struct AddressingState {
//     /// The resolved address (once addressing phase completes)
//     pub addr: Option<u16>,
//     /// The value read from memory (for RMW operations)
//     pub value: Option<u8>,
//     /// The original value before modification (for RMW dummy write)
//     pub original_value: Option<u8>,
//     /// Base address before indexing (for page crossing detection)
//     pub base_addr: Option<u16>,
//     /// Temporary bytes collected during address resolution (max 4 bytes needed)
//     pub temp_bytes: [u8; 4],
//     /// Track if dummy read has been performed (for RMW operations)
//     pub dummy_read_done: bool,
//     /// Track if dummy write has been performed (for RMW operations)
//     pub dummy_write_done: bool,
// }

// impl Default for AddressingState {
//     fn default() -> Self {
//         Self {
//             addr: None,
//             value: None,
//             original_value: None,
//             base_addr: None,
//             temp_bytes: [0; 4],
//             dummy_read_done: false,
//             dummy_write_done: false,
//         }
//     }
// }

// /// Tracks the state of an instruction being executed across multiple cycles
// #[derive(Debug, Clone)]
// pub struct InstructionExecution {
//     /// The opcode being executed
//     pub opcode: u8,
//     /// Current phase of execution
//     pub phase: InstructionPhase,
//     /// State for address resolution
//     pub addressing_state: AddressingState,
//     /// Number of cycles remaining for this instruction
//     pub cycles_remaining: u8,
// }

// /// Classification of instruction types by their cycle sequence
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionType {
//     /// Read instruction: Address → Read → Execute
//     Read,
//     /// Write instruction: Address → Execute → Write
//     Write,
//     /// Read-Modify-Write: Address → Read → Execute → DummyWrite → Write
//     RMW,
//     /// Branch instruction with variable cycles
//     Branch,
// /// Represents the current phase of instruction execution
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionPhase {
//     /// Fetching the opcode byte (cycle 0)
//     Opcode,
//     /// Resolving the address for the operand
//     /// The u8 indicates which cycle of addressing we're in (0-indexed)
//     Addressing(u8),
//     /// Executing the operation with the resolved operand
//     Execute,
//     /// Writing back the result (for RMW and write operations)
//     Writeback,
// }

// /// Holds intermediate state during multi-cycle address resolution
// #[derive(Debug, Clone)]
// pub struct AddressingState {
//     /// The resolved address (once addressing phase completes)
//     pub addr: Option<u16>,
//     /// The value read from memory (for RMW operations)
//     pub value: Option<u8>,
//     /// The original value before modification (for RMW dummy write)
//     pub original_value: Option<u8>,
//     /// Base address before indexing (for page crossing detection)
//     pub base_addr: Option<u16>,
//     /// Temporary bytes collected during address resolution (max 4 bytes needed)
//     pub temp_bytes: [u8; 4],
//     /// Track if dummy read has been performed (for RMW operations)
//     pub dummy_read_done: bool,
//     /// Track if dummy write has been performed (for RMW operations)
//     pub dummy_write_done: bool,
// }

// impl Default for AddressingState {
//     fn default() -> Self {
//         Self {
//             addr: None,
//             value: None,
//             original_value: None,
//             base_addr: None,
//             temp_bytes: [0; 4],
//             dummy_read_done: false,
//             dummy_write_done: false,
//         }
//     }
// }

// /// Tracks the state of an instruction being executed across multiple cycles
// #[derive(Debug, Clone)]
// pub struct InstructionExecution {
//     /// The opcode being executed
//     pub opcode: u8,
//     /// Current phase of execution
//     pub phase: InstructionPhase,
//     /// State for address resolution
//     pub addressing_state: AddressingState,
//     /// Number of cycles remaining for this instruction
//     pub cycles_remaining: u8,
// }

// /// Classification of instruction types by their cycle sequence
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum InstructionType {
//     /// Read instruction: Address → Read → Execute
//     Read,
//     /// Write instruction: Address → Execute → Write
//     Write,
//     /// Read-Modify-Write: Address → Read → Execute → DummyWrite → Write
//     RMW,
//     /// Branch instruction with variable cycles
//     Branch,
//     /// Stack operation (push/pull)
//     Stack,
//     /// Control flow (JMP, JSR, RTS, RTI, BRK)
//     Control,
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_instruction_phase_enum() {
//         // Test that instruction phases can be created and compared
//         let opcode_phase = InstructionPhase::Opcode;
//         let addressing_phase = InstructionPhase::Addressing(0);
//         let execute_phase = InstructionPhase::Execute;
//         let writeback_phase = InstructionPhase::Writeback;

//         assert_eq!(opcode_phase, InstructionPhase::Opcode);
//         assert_eq!(addressing_phase, InstructionPhase::Addressing(0));
//         assert_ne!(
//             InstructionPhase::Addressing(0),
//             InstructionPhase::Addressing(1)
//         );
//         assert_eq!(execute_phase, InstructionPhase::Execute);
//         assert_eq!(writeback_phase, InstructionPhase::Writeback);
//     }

//     #[test]
//     fn test_addressing_state_default() {
//         // Test that AddressingState can be created with defaults
//         let state = AddressingState::default();

//         assert_eq!(state.addr, None);
//         assert_eq!(state.value, None);
//         assert_eq!(state.base_addr, None);
//         assert_eq!(state.temp_bytes, [0; 4]);
//     }

//     #[test]
//     fn test_addressing_state_can_store_values() {
//         // Test that AddressingState can store intermediate values
//         let mut state = AddressingState::default();

//         state.temp_bytes[0] = 0x34;
//         state.temp_bytes[1] = 0x12;
//         assert_eq!(state.temp_bytes[0], 0x34);
//         assert_eq!(state.temp_bytes[1], 0x12);

//         state.addr = Some(0x1234);
//         assert_eq!(state.addr, Some(0x1234));

//         state.value = Some(0x42);
//         assert_eq!(state.value, Some(0x42));

//         state.base_addr = Some(0x1200);
//         assert_eq!(state.base_addr, Some(0x1200));
//     }

//     #[test]
//     fn test_instruction_execution_creation() {
//         // Test that InstructionExecution can be created
//         let execution = InstructionExecution {
//             opcode: 0xA9, // LDA immediate
//             phase: InstructionPhase::Opcode,
//             addressing_state: AddressingState::default(),
//             cycles_remaining: 2,
//         };

//         assert_eq!(execution.opcode, 0xA9);
//         assert_eq!(execution.phase, InstructionPhase::Opcode);
//         assert_eq!(execution.cycles_remaining, 2);
//     }

//     #[test]
//     fn test_instruction_type_enum() {
//         // Test that instruction types can be compared
//         assert_eq!(InstructionType::Read, InstructionType::Read);
//         assert_ne!(InstructionType::Read, InstructionType::Write);

//         let instr_type = InstructionType::RMW;
//         match instr_type {
//             InstructionType::RMW => { /* expected */ }
//             _ => panic!("Should match RMW"),
//         }
//     }
// }
