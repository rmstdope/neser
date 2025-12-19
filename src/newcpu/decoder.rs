//! Complete opcode lookup table for all 256 6502 opcodes
//!
//! This module maps each opcode byte to its addressing mode, operation,
//! instruction type, and base cycle count.

use super::addressing::*;
use super::operations::*;
use super::traits::{AddressingMode, Mnemonic, Operation};
use super::types::InstructionType;

/// Complete specification for an opcode
pub struct OpcodeSpec {
    pub byte: u8,
    pub mnemonic: Mnemonic,
    pub instruction_type: InstructionType,
    pub base_cycles: u8,
    // Note: Addressing mode and operation are provided as trait objects
    // to allow dynamic dispatch
}

/// Returns the addressing mode, operation, instruction type, and cycles for an opcode
pub fn decode_opcode(
    opcode: u8,
) -> (
    Box<dyn AddressingMode>,
    Box<dyn Operation>,
    InstructionType,
    u8,
) {
    match opcode {
        // ADC - Add with Carry
        0x69 => (Box::new(Immediate), Box::new(ADC), InstructionType::Read, 2),
        0x65 => (Box::new(ZeroPage), Box::new(ADC), InstructionType::Read, 3),
        0x75 => (Box::new(ZeroPageX), Box::new(ADC), InstructionType::Read, 4),
        0x6D => (Box::new(Absolute), Box::new(ADC), InstructionType::Read, 4),
        0x7D => (Box::new(AbsoluteX), Box::new(ADC), InstructionType::Read, 4),
        0x79 => (Box::new(AbsoluteY), Box::new(ADC), InstructionType::Read, 4),
        0x61 => (
            Box::new(IndexedIndirect),
            Box::new(ADC),
            InstructionType::Read,
            6,
        ),
        0x71 => (
            Box::new(IndirectIndexed),
            Box::new(ADC),
            InstructionType::Read,
            5,
        ),

        // AND - Logical AND
        0x29 => (Box::new(Immediate), Box::new(AND), InstructionType::Read, 2),
        0x25 => (Box::new(ZeroPage), Box::new(AND), InstructionType::Read, 3),
        0x35 => (Box::new(ZeroPageX), Box::new(AND), InstructionType::Read, 4),
        0x2D => (Box::new(Absolute), Box::new(AND), InstructionType::Read, 4),
        0x3D => (Box::new(AbsoluteX), Box::new(AND), InstructionType::Read, 4),
        0x39 => (Box::new(AbsoluteY), Box::new(AND), InstructionType::Read, 4),
        0x21 => (
            Box::new(IndexedIndirect),
            Box::new(AND),
            InstructionType::Read,
            6,
        ),
        0x31 => (
            Box::new(IndirectIndexed),
            Box::new(AND),
            InstructionType::Read,
            5,
        ),

        // ASL - Arithmetic Shift Left
        0x0A => (Box::new(Implied), Box::new(ASL), InstructionType::Read, 2),
        0x06 => (Box::new(ZeroPage), Box::new(ASL), InstructionType::RMW, 5),
        0x16 => (Box::new(ZeroPageX), Box::new(ASL), InstructionType::RMW, 6),
        0x0E => (Box::new(Absolute), Box::new(ASL), InstructionType::RMW, 6),
        0x1E => (Box::new(AbsoluteX), Box::new(ASL), InstructionType::RMW, 7),

        // BCC - Branch if Carry Clear
        0x90 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BCS - Branch if Carry Set
        0xB0 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BEQ - Branch if Equal
        0xF0 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BIT - Bit Test
        0x24 => (Box::new(ZeroPage), Box::new(BIT), InstructionType::Read, 3),
        0x2C => (Box::new(Absolute), Box::new(BIT), InstructionType::Read, 4),

        // BMI - Branch if Minus
        0x30 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BNE - Branch if Not Equal
        0xD0 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BPL - Branch if Positive
        0x10 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BRK - Break
        0x00 => (
            Box::new(Implied),
            Box::new(BRK),
            InstructionType::Control,
            7,
        ),

        // BVC - Branch if Overflow Clear
        0x50 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // BVS - Branch if Overflow Set
        0x70 => (
            Box::new(Relative),
            Box::new(NOP),
            InstructionType::Branch,
            2,
        ),

        // CLC - Clear Carry
        0x18 => (Box::new(Implied), Box::new(CLC), InstructionType::Read, 2),

        // CLD - Clear Decimal
        0xD8 => (Box::new(Implied), Box::new(CLD), InstructionType::Read, 2),

        // CLI - Clear Interrupt Disable
        0x58 => (Box::new(Implied), Box::new(CLI), InstructionType::Read, 2),

        // CLV - Clear Overflow
        0xB8 => (Box::new(Implied), Box::new(CLV), InstructionType::Read, 2),

        // CMP - Compare Accumulator
        0xC9 => (Box::new(Immediate), Box::new(CMP), InstructionType::Read, 2),
        0xC5 => (Box::new(ZeroPage), Box::new(CMP), InstructionType::Read, 3),
        0xD5 => (Box::new(ZeroPageX), Box::new(CMP), InstructionType::Read, 4),
        0xCD => (Box::new(Absolute), Box::new(CMP), InstructionType::Read, 4),
        0xDD => (Box::new(AbsoluteX), Box::new(CMP), InstructionType::Read, 4),
        0xD9 => (Box::new(AbsoluteY), Box::new(CMP), InstructionType::Read, 4),
        0xC1 => (
            Box::new(IndexedIndirect),
            Box::new(CMP),
            InstructionType::Read,
            6,
        ),
        0xD1 => (
            Box::new(IndirectIndexed),
            Box::new(CMP),
            InstructionType::Read,
            5,
        ),

        // CPX - Compare X Register
        0xE0 => (Box::new(Immediate), Box::new(CPX), InstructionType::Read, 2),
        0xE4 => (Box::new(ZeroPage), Box::new(CPX), InstructionType::Read, 3),
        0xEC => (Box::new(Absolute), Box::new(CPX), InstructionType::Read, 4),

        // CPY - Compare Y Register
        0xC0 => (Box::new(Immediate), Box::new(CPY), InstructionType::Read, 2),
        0xC4 => (Box::new(ZeroPage), Box::new(CPY), InstructionType::Read, 3),
        0xCC => (Box::new(Absolute), Box::new(CPY), InstructionType::Read, 4),

        // DEC - Decrement Memory
        0xC6 => (Box::new(ZeroPage), Box::new(DEC), InstructionType::RMW, 5),
        0xD6 => (Box::new(ZeroPageX), Box::new(DEC), InstructionType::RMW, 6),
        0xCE => (Box::new(Absolute), Box::new(DEC), InstructionType::RMW, 6),
        0xDE => (Box::new(AbsoluteX), Box::new(DEC), InstructionType::RMW, 7),

        // DEX - Decrement X
        0xCA => (Box::new(Implied), Box::new(DEX), InstructionType::Read, 2),

        // DEY - Decrement Y
        0x88 => (Box::new(Implied), Box::new(DEY), InstructionType::Read, 2),

        // EOR - Exclusive OR
        0x49 => (Box::new(Immediate), Box::new(EOR), InstructionType::Read, 2),
        0x45 => (Box::new(ZeroPage), Box::new(EOR), InstructionType::Read, 3),
        0x55 => (Box::new(ZeroPageX), Box::new(EOR), InstructionType::Read, 4),
        0x4D => (Box::new(Absolute), Box::new(EOR), InstructionType::Read, 4),
        0x5D => (Box::new(AbsoluteX), Box::new(EOR), InstructionType::Read, 4),
        0x59 => (Box::new(AbsoluteY), Box::new(EOR), InstructionType::Read, 4),
        0x41 => (
            Box::new(IndexedIndirect),
            Box::new(EOR),
            InstructionType::Read,
            6,
        ),
        0x51 => (
            Box::new(IndirectIndexed),
            Box::new(EOR),
            InstructionType::Read,
            5,
        ),

        // INC - Increment Memory
        0xE6 => (Box::new(ZeroPage), Box::new(INC), InstructionType::RMW, 5),
        0xF6 => (Box::new(ZeroPageX), Box::new(INC), InstructionType::RMW, 6),
        0xEE => (Box::new(Absolute), Box::new(INC), InstructionType::RMW, 6),
        0xFE => (Box::new(AbsoluteX), Box::new(INC), InstructionType::RMW, 7),

        // INX - Increment X
        0xE8 => (Box::new(Implied), Box::new(INX), InstructionType::Read, 2),

        // INY - Increment Y
        0xC8 => (Box::new(Implied), Box::new(INY), InstructionType::Read, 2),

        // JMP - Jump
        0x4C => (
            Box::new(Absolute),
            Box::new(NOP),
            InstructionType::Control,
            3,
        ),
        0x6C => (
            Box::new(Indirect),
            Box::new(NOP),
            InstructionType::Control,
            5,
        ),

        // JSR - Jump to Subroutine
        0x20 => (
            Box::new(Absolute),
            Box::new(NOP),
            InstructionType::Control,
            6,
        ),

        // LDA - Load Accumulator
        0xA9 => (Box::new(Immediate), Box::new(LDA), InstructionType::Read, 2),
        0xA5 => (Box::new(ZeroPage), Box::new(LDA), InstructionType::Read, 3),
        0xB5 => (Box::new(ZeroPageX), Box::new(LDA), InstructionType::Read, 4),
        0xAD => (Box::new(Absolute), Box::new(LDA), InstructionType::Read, 4),
        0xBD => (Box::new(AbsoluteX), Box::new(LDA), InstructionType::Read, 4),
        0xB9 => (Box::new(AbsoluteY), Box::new(LDA), InstructionType::Read, 4),
        0xA1 => (
            Box::new(IndexedIndirect),
            Box::new(LDA),
            InstructionType::Read,
            6,
        ),
        0xB1 => (
            Box::new(IndirectIndexed),
            Box::new(LDA),
            InstructionType::Read,
            5,
        ),

        // LDX - Load X Register
        0xA2 => (Box::new(Immediate), Box::new(LDX), InstructionType::Read, 2),
        0xA6 => (Box::new(ZeroPage), Box::new(LDX), InstructionType::Read, 3),
        0xB6 => (Box::new(ZeroPageY), Box::new(LDX), InstructionType::Read, 4),
        0xAE => (Box::new(Absolute), Box::new(LDX), InstructionType::Read, 4),
        0xBE => (Box::new(AbsoluteY), Box::new(LDX), InstructionType::Read, 4),

        // LDY - Load Y Register
        0xA0 => (Box::new(Immediate), Box::new(LDY), InstructionType::Read, 2),
        0xA4 => (Box::new(ZeroPage), Box::new(LDY), InstructionType::Read, 3),
        0xB4 => (Box::new(ZeroPageX), Box::new(LDY), InstructionType::Read, 4),
        0xAC => (Box::new(Absolute), Box::new(LDY), InstructionType::Read, 4),
        0xBC => (Box::new(AbsoluteX), Box::new(LDY), InstructionType::Read, 4),

        // LSR - Logical Shift Right
        0x4A => (Box::new(Implied), Box::new(LSR), InstructionType::Read, 2),
        0x46 => (Box::new(ZeroPage), Box::new(LSR), InstructionType::RMW, 5),
        0x56 => (Box::new(ZeroPageX), Box::new(LSR), InstructionType::RMW, 6),
        0x4E => (Box::new(Absolute), Box::new(LSR), InstructionType::RMW, 6),
        0x5E => (Box::new(AbsoluteX), Box::new(LSR), InstructionType::RMW, 7),

        // NOP - No Operation
        0xEA => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),

        // ORA - Logical OR
        0x09 => (Box::new(Immediate), Box::new(ORA), InstructionType::Read, 2),
        0x05 => (Box::new(ZeroPage), Box::new(ORA), InstructionType::Read, 3),
        0x15 => (Box::new(ZeroPageX), Box::new(ORA), InstructionType::Read, 4),
        0x0D => (Box::new(Absolute), Box::new(ORA), InstructionType::Read, 4),
        0x1D => (Box::new(AbsoluteX), Box::new(ORA), InstructionType::Read, 4),
        0x19 => (Box::new(AbsoluteY), Box::new(ORA), InstructionType::Read, 4),
        0x01 => (
            Box::new(IndexedIndirect),
            Box::new(ORA),
            InstructionType::Read,
            6,
        ),
        0x11 => (
            Box::new(IndirectIndexed),
            Box::new(ORA),
            InstructionType::Read,
            5,
        ),

        // PHA - Push Accumulator
        0x48 => (Box::new(Implied), Box::new(NOP), InstructionType::Stack, 3),

        // PHP - Push Processor Status
        0x08 => (Box::new(Implied), Box::new(NOP), InstructionType::Stack, 3),

        // PLA - Pull Accumulator
        0x68 => (Box::new(Implied), Box::new(NOP), InstructionType::Stack, 4),

        // PLP - Pull Processor Status
        0x28 => (Box::new(Implied), Box::new(NOP), InstructionType::Stack, 4),

        // ROL - Rotate Left
        0x2A => (Box::new(Implied), Box::new(ROL), InstructionType::Read, 2),
        0x26 => (Box::new(ZeroPage), Box::new(ROL), InstructionType::RMW, 5),
        0x36 => (Box::new(ZeroPageX), Box::new(ROL), InstructionType::RMW, 6),
        0x2E => (Box::new(Absolute), Box::new(ROL), InstructionType::RMW, 6),
        0x3E => (Box::new(AbsoluteX), Box::new(ROL), InstructionType::RMW, 7),

        // ROR - Rotate Right
        0x6A => (Box::new(Implied), Box::new(ROR), InstructionType::Read, 2),
        0x66 => (Box::new(ZeroPage), Box::new(ROR), InstructionType::RMW, 5),
        0x76 => (Box::new(ZeroPageX), Box::new(ROR), InstructionType::RMW, 6),
        0x6E => (Box::new(Absolute), Box::new(ROR), InstructionType::RMW, 6),
        0x7E => (Box::new(AbsoluteX), Box::new(ROR), InstructionType::RMW, 7),

        // RTI - Return from Interrupt
        0x40 => (
            Box::new(Implied),
            Box::new(NOP),
            InstructionType::Control,
            6,
        ),

        // RTS - Return from Subroutine
        0x60 => (
            Box::new(Implied),
            Box::new(NOP),
            InstructionType::Control,
            6,
        ),

        // SBC - Subtract with Carry
        0xE9 => (Box::new(Immediate), Box::new(SBC), InstructionType::Read, 2),
        0xE5 => (Box::new(ZeroPage), Box::new(SBC), InstructionType::Read, 3),
        0xF5 => (Box::new(ZeroPageX), Box::new(SBC), InstructionType::Read, 4),
        0xED => (Box::new(Absolute), Box::new(SBC), InstructionType::Read, 4),
        0xFD => (Box::new(AbsoluteX), Box::new(SBC), InstructionType::Read, 4),
        0xF9 => (Box::new(AbsoluteY), Box::new(SBC), InstructionType::Read, 4),
        0xE1 => (
            Box::new(IndexedIndirect),
            Box::new(SBC),
            InstructionType::Read,
            6,
        ),
        0xF1 => (
            Box::new(IndirectIndexed),
            Box::new(SBC),
            InstructionType::Read,
            5,
        ),

        // SEC - Set Carry
        0x38 => (Box::new(Implied), Box::new(SEC), InstructionType::Read, 2),

        // SED - Set Decimal
        0xF8 => (Box::new(Implied), Box::new(SED), InstructionType::Read, 2),

        // SEI - Set Interrupt Disable
        0x78 => (Box::new(Implied), Box::new(SEI), InstructionType::Read, 2),

        // STA - Store Accumulator
        0x85 => (Box::new(ZeroPage), Box::new(STA), InstructionType::Write, 3),
        0x95 => (
            Box::new(ZeroPageX),
            Box::new(STA),
            InstructionType::Write,
            4,
        ),
        0x8D => (Box::new(Absolute), Box::new(STA), InstructionType::Write, 4),
        0x9D => (
            Box::new(AbsoluteX),
            Box::new(STA),
            InstructionType::Write,
            5,
        ),
        0x99 => (
            Box::new(AbsoluteY),
            Box::new(STA),
            InstructionType::Write,
            5,
        ),
        0x81 => (
            Box::new(IndexedIndirect),
            Box::new(STA),
            InstructionType::Write,
            6,
        ),
        0x91 => (
            Box::new(IndirectIndexed),
            Box::new(STA),
            InstructionType::Write,
            6,
        ),

        // STX - Store X Register
        0x86 => (Box::new(ZeroPage), Box::new(STX), InstructionType::Write, 3),
        0x96 => (
            Box::new(ZeroPageY),
            Box::new(STX),
            InstructionType::Write,
            4,
        ),
        0x8E => (Box::new(Absolute), Box::new(STX), InstructionType::Write, 4),

        // STY - Store Y Register
        0x84 => (Box::new(ZeroPage), Box::new(STY), InstructionType::Write, 3),
        0x94 => (
            Box::new(ZeroPageX),
            Box::new(STY),
            InstructionType::Write,
            4,
        ),
        0x8C => (Box::new(Absolute), Box::new(STY), InstructionType::Write, 4),

        // TAX - Transfer A to X
        0xAA => (Box::new(Implied), Box::new(TAX), InstructionType::Read, 2),

        // TAY - Transfer A to Y
        0xA8 => (Box::new(Implied), Box::new(TAY), InstructionType::Read, 2),

        // TSX - Transfer SP to X
        0xBA => (Box::new(Implied), Box::new(TSX), InstructionType::Read, 2),

        // TXA - Transfer X to A
        0x8A => (Box::new(Implied), Box::new(TXA), InstructionType::Read, 2),

        // TXS - Transfer X to SP
        0x9A => (Box::new(Implied), Box::new(TXS), InstructionType::Read, 2),

        // TYA - Transfer Y to A
        0x98 => (Box::new(Implied), Box::new(TYA), InstructionType::Read, 2),

        // Unofficial opcodes
        // LAX - Load A and X
        0xA3 => (
            Box::new(IndexedIndirect),
            Box::new(LAX),
            InstructionType::Read,
            6,
        ),
        0xA7 => (Box::new(ZeroPage), Box::new(LAX), InstructionType::Read, 3),
        0xAF => (Box::new(Absolute), Box::new(LAX), InstructionType::Read, 4),
        0xB3 => (
            Box::new(IndirectIndexed),
            Box::new(LAX),
            InstructionType::Read,
            5,
        ),
        0xB7 => (Box::new(ZeroPageY), Box::new(LAX), InstructionType::Read, 4),
        0xBF => (Box::new(AbsoluteY), Box::new(LAX), InstructionType::Read, 4),

        // SAX - Store A AND X
        0x83 => (
            Box::new(IndexedIndirect),
            Box::new(SAX),
            InstructionType::Write,
            6,
        ),
        0x87 => (Box::new(ZeroPage), Box::new(SAX), InstructionType::Write, 3),
        0x8F => (Box::new(Absolute), Box::new(SAX), InstructionType::Write, 4),
        0x97 => (
            Box::new(ZeroPageY),
            Box::new(SAX),
            InstructionType::Write,
            4,
        ),

        // DCP - Decrement then Compare
        0xC3 => (
            Box::new(IndexedIndirect),
            Box::new(DCP),
            InstructionType::RMW,
            8,
        ),
        0xC7 => (Box::new(ZeroPage), Box::new(DCP), InstructionType::RMW, 5),
        0xCF => (Box::new(Absolute), Box::new(DCP), InstructionType::RMW, 6),
        0xD3 => (
            Box::new(IndirectIndexed),
            Box::new(DCP),
            InstructionType::RMW,
            8,
        ),
        0xD7 => (Box::new(ZeroPageX), Box::new(DCP), InstructionType::RMW, 6),
        0xDB => (Box::new(AbsoluteY), Box::new(DCP), InstructionType::RMW, 7),
        0xDF => (Box::new(AbsoluteX), Box::new(DCP), InstructionType::RMW, 7),

        // ISB - Increment then Subtract
        0xE3 => (
            Box::new(IndexedIndirect),
            Box::new(ISB),
            InstructionType::RMW,
            8,
        ),
        0xE7 => (Box::new(ZeroPage), Box::new(ISB), InstructionType::RMW, 5),
        0xEF => (Box::new(Absolute), Box::new(ISB), InstructionType::RMW, 6),
        0xF3 => (
            Box::new(IndirectIndexed),
            Box::new(ISB),
            InstructionType::RMW,
            8,
        ),
        0xF7 => (Box::new(ZeroPageX), Box::new(ISB), InstructionType::RMW, 6),
        0xFB => (Box::new(AbsoluteY), Box::new(ISB), InstructionType::RMW, 7),
        0xFF => (Box::new(AbsoluteX), Box::new(ISB), InstructionType::RMW, 7),

        // SLO - Shift Left then OR
        0x03 => (
            Box::new(IndexedIndirect),
            Box::new(SLO),
            InstructionType::RMW,
            8,
        ),
        0x07 => (Box::new(ZeroPage), Box::new(SLO), InstructionType::RMW, 5),
        0x0F => (Box::new(Absolute), Box::new(SLO), InstructionType::RMW, 6),
        0x13 => (
            Box::new(IndirectIndexed),
            Box::new(SLO),
            InstructionType::RMW,
            8,
        ),
        0x17 => (Box::new(ZeroPageX), Box::new(SLO), InstructionType::RMW, 6),
        0x1B => (Box::new(AbsoluteY), Box::new(SLO), InstructionType::RMW, 7),
        0x1F => (Box::new(AbsoluteX), Box::new(SLO), InstructionType::RMW, 7),

        // RLA - Rotate Left then AND
        0x23 => (
            Box::new(IndexedIndirect),
            Box::new(RLA),
            InstructionType::RMW,
            8,
        ),
        0x27 => (Box::new(ZeroPage), Box::new(RLA), InstructionType::RMW, 5),
        0x2F => (Box::new(Absolute), Box::new(RLA), InstructionType::RMW, 6),
        0x33 => (
            Box::new(IndirectIndexed),
            Box::new(RLA),
            InstructionType::RMW,
            8,
        ),
        0x37 => (Box::new(ZeroPageX), Box::new(RLA), InstructionType::RMW, 6),
        0x3B => (Box::new(AbsoluteY), Box::new(RLA), InstructionType::RMW, 7),
        0x3F => (Box::new(AbsoluteX), Box::new(RLA), InstructionType::RMW, 7),

        // SRE - Shift Right then XOR
        0x43 => (
            Box::new(IndexedIndirect),
            Box::new(SRE),
            InstructionType::RMW,
            8,
        ),
        0x47 => (Box::new(ZeroPage), Box::new(SRE), InstructionType::RMW, 5),
        0x4F => (Box::new(Absolute), Box::new(SRE), InstructionType::RMW, 6),
        0x53 => (
            Box::new(IndirectIndexed),
            Box::new(SRE),
            InstructionType::RMW,
            8,
        ),
        0x57 => (Box::new(ZeroPageX), Box::new(SRE), InstructionType::RMW, 6),
        0x5B => (Box::new(AbsoluteY), Box::new(SRE), InstructionType::RMW, 7),
        0x5F => (Box::new(AbsoluteX), Box::new(SRE), InstructionType::RMW, 7),

        // RRA - Rotate Right then ADC
        0x63 => (
            Box::new(IndexedIndirect),
            Box::new(RRA),
            InstructionType::RMW,
            8,
        ),
        0x67 => (Box::new(ZeroPage), Box::new(RRA), InstructionType::RMW, 5),
        0x6F => (Box::new(Absolute), Box::new(RRA), InstructionType::RMW, 6),
        0x73 => (
            Box::new(IndirectIndexed),
            Box::new(RRA),
            InstructionType::RMW,
            8,
        ),
        0x77 => (Box::new(ZeroPageX), Box::new(RRA), InstructionType::RMW, 6),
        0x7B => (Box::new(AbsoluteY), Box::new(RRA), InstructionType::RMW, 7),
        0x7F => (Box::new(AbsoluteX), Box::new(RRA), InstructionType::RMW, 7),

        // AAC (ANC) - AND then copy N to C
        0x0B => (Box::new(Immediate), Box::new(AAC), InstructionType::Read, 2),
        0x2B => (Box::new(Immediate), Box::new(AAC), InstructionType::Read, 2),

        // ARR - AND then ROR with special flags
        0x6B => (Box::new(Immediate), Box::new(ARR), InstructionType::Read, 2),

        // ASR (ALR) - AND then LSR
        0x4B => (Box::new(Immediate), Box::new(ASR), InstructionType::Read, 2),

        // ATX (LXA) - (A | 0xEE) & operand -> A, X
        0xAB => (Box::new(Immediate), Box::new(ATX), InstructionType::Read, 2),

        // AXS (SBX) - (A & X) - operand -> X
        0xCB => (Box::new(Immediate), Box::new(AXS), InstructionType::Read, 2),

        // XAA - X & operand -> A
        0x8B => (Box::new(Immediate), Box::new(XAA), InstructionType::Read, 2),

        // DOP - Double NOP (2-byte NOP)
        0x04 => (Box::new(ZeroPage), Box::new(DOP), InstructionType::Read, 3),
        0x14 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),
        0x34 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),
        0x44 => (Box::new(ZeroPage), Box::new(DOP), InstructionType::Read, 3),
        0x54 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),
        0x64 => (Box::new(ZeroPage), Box::new(DOP), InstructionType::Read, 3),
        0x74 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),
        0x80 => (Box::new(Immediate), Box::new(DOP), InstructionType::Read, 2),
        0x82 => (Box::new(Immediate), Box::new(DOP), InstructionType::Read, 2),
        0x89 => (Box::new(Immediate), Box::new(DOP), InstructionType::Read, 2),
        0xC2 => (Box::new(Immediate), Box::new(DOP), InstructionType::Read, 2),
        0xD4 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),
        0xE2 => (Box::new(Immediate), Box::new(DOP), InstructionType::Read, 2),
        0xF4 => (Box::new(ZeroPageX), Box::new(DOP), InstructionType::Read, 4),

        // TOP - Triple NOP (3-byte NOP)
        0x0C => (Box::new(Absolute), Box::new(TOP), InstructionType::Read, 4),
        0x1C => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),
        0x3C => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),
        0x5C => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),
        0x7C => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),
        0xDC => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),
        0xFC => (Box::new(AbsoluteX), Box::new(TOP), InstructionType::Read, 4),

        // KIL - Halt/Jam CPU
        0x02 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x12 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x22 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x32 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x42 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x52 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x62 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x72 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0x92 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0xB2 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0xD2 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),
        0xF2 => (Box::new(Implied), Box::new(KIL), InstructionType::Read, 2),

        // Single-byte NOPs (Implied addressing)
        0x1A => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
        0x3A => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
        0x5A => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
        0x7A => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
        0xDA => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
        0xFA => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),

        // Duplicate SBC (same as official 0xE9)
        0xEB => (Box::new(Immediate), Box::new(SBC), InstructionType::Read, 2),

        // Highly unstable unofficial operations (mapped as NOP with correct addressing)
        // SHA/AHX - Store A & X & (H+1) - highly unstable, mapped as NOP
        0x93 => (
            Box::new(IndirectIndexed),
            Box::new(NOP),
            InstructionType::Read,
            6,
        ),
        0x9F => (Box::new(AbsoluteY), Box::new(NOP), InstructionType::Read, 5),

        // SHY - Store Y & (H+1) - highly unstable, mapped as NOP
        0x9C => (Box::new(AbsoluteX), Box::new(NOP), InstructionType::Read, 5),

        // SHX - Store X & (H+1) - highly unstable, mapped as NOP
        0x9E => (Box::new(AbsoluteY), Box::new(NOP), InstructionType::Read, 5),

        // SHS/TAS - Store A & X in SP and A & X & (H+1) - highly unstable, mapped as NOP
        0x9B => (Box::new(AbsoluteY), Box::new(NOP), InstructionType::Read, 5),

        // LAS - Load A, X, SP with memory & SP - unstable, mapped as NOP
        0xBB => (Box::new(AbsoluteY), Box::new(NOP), InstructionType::Read, 4),

        // Catch-all for any remaining unmapped opcodes (should not be reached)
        _ => (Box::new(Implied), Box::new(NOP), InstructionType::Read, 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_opcodes_decodeable() {
        // Verify all 256 opcodes can be decoded without panicking
        for opcode in 0..=255 {
            let (_addr, _op, _type, _cycles) = decode_opcode(opcode);
            // If we get here, it decoded successfully
        }
    }

    #[test]
    fn test_sample_opcodes() {
        // Test a few specific opcodes
        let (_, _, inst_type, cycles) = decode_opcode(0xA9); // LDA #
        assert_eq!(inst_type, InstructionType::Read);
        assert_eq!(cycles, 2);

        let (_, _, inst_type, cycles) = decode_opcode(0x85); // STA $
        assert_eq!(inst_type, InstructionType::Write);
        assert_eq!(cycles, 3);

        let (_, _, inst_type, cycles) = decode_opcode(0xE6); // INC $
        assert_eq!(inst_type, InstructionType::RMW);
        assert_eq!(cycles, 5);
    }
}
