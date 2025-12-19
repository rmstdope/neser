//! OpCode definitions for cycle-accurate CPU
//!
//! This module defines the OpCode struct that ties together mnemonics,
//! addressing modes, operations, and instruction types.

use super::traits::Mnemonic;
use super::types::InstructionType;

/// OpCode definition combining all instruction components
#[derive(Debug, Clone)]
pub struct OpCode {
    /// The opcode byte value
    pub byte: u8,
    /// The mnemonic (instruction name)
    pub mnemonic: Mnemonic,
    /// The instruction type (Read/Write/RMW/Branch/Stack/Control)
    pub instruction_type: InstructionType,
    /// Base cycle count (before addressing mode or page crossing penalties)
    pub base_cycles: u8,
}

impl OpCode {
    /// Create a new OpCode definition
    pub fn new(
        byte: u8,
        mnemonic: Mnemonic,
        instruction_type: InstructionType,
        base_cycles: u8,
    ) -> Self {
        Self {
            byte,
            mnemonic,
            instruction_type,
            base_cycles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_creation() {
        let opcode = OpCode::new(0xA9, Mnemonic::LDA, InstructionType::Read, 2);

        assert_eq!(opcode.byte, 0xA9);
        assert_eq!(opcode.mnemonic, Mnemonic::LDA);
        assert_eq!(opcode.instruction_type, InstructionType::Read);
        assert_eq!(opcode.base_cycles, 2);
    }

    #[test]
    fn test_opcode_different_instruction_types() {
        // Test Read instruction
        let lda = OpCode::new(0xA9, Mnemonic::LDA, InstructionType::Read, 2);
        assert_eq!(lda.instruction_type, InstructionType::Read);

        // Test Write instruction
        let sta = OpCode::new(0x85, Mnemonic::STA, InstructionType::Write, 3);
        assert_eq!(sta.instruction_type, InstructionType::Write);

        // Test RMW instruction
        let inc = OpCode::new(0xE6, Mnemonic::INC, InstructionType::RMW, 5);
        assert_eq!(inc.instruction_type, InstructionType::RMW);

        // Test Branch instruction
        let bne = OpCode::new(0xD0, Mnemonic::BNE, InstructionType::Branch, 2);
        assert_eq!(bne.instruction_type, InstructionType::Branch);

        // Test Stack instruction
        let pha = OpCode::new(0x48, Mnemonic::PHA, InstructionType::Stack, 3);
        assert_eq!(pha.instruction_type, InstructionType::Stack);

        // Test Control instruction
        let jsr = OpCode::new(0x20, Mnemonic::JSR, InstructionType::Control, 6);
        assert_eq!(jsr.instruction_type, InstructionType::Control);
    }

    #[test]
    fn test_opcode_clone() {
        let original = OpCode::new(0xA9, Mnemonic::LDA, InstructionType::Read, 2);
        let cloned = original.clone();

        assert_eq!(cloned.byte, original.byte);
        assert_eq!(cloned.mnemonic, original.mnemonic);
        assert_eq!(cloned.instruction_type, original.instruction_type);
        assert_eq!(cloned.base_cycles, original.base_cycles);
    }

    #[test]
    fn test_opcode_different_cycle_counts() {
        // Immediate mode - 2 cycles
        let lda_imm = OpCode::new(0xA9, Mnemonic::LDA, InstructionType::Read, 2);
        assert_eq!(lda_imm.base_cycles, 2);

        // Absolute mode - 4 cycles
        let lda_abs = OpCode::new(0xAD, Mnemonic::LDA, InstructionType::Read, 4);
        assert_eq!(lda_abs.base_cycles, 4);

        // JSR - 6 cycles
        let jsr = OpCode::new(0x20, Mnemonic::JSR, InstructionType::Control, 6);
        assert_eq!(jsr.base_cycles, 6);
    }
}
