/// Represents a 6502 instruction opcode with its mnemonic and addressing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpCode {
    /// The opcode byte value
    pub code: u8,
    /// The instruction mnemonic (e.g., "ADC", "LDA")
    pub mnemonic: &'static str,
    /// The addressing mode (e.g., "IMM", "ABS", "ZP")
    pub mode: &'static str,
}

impl OpCode {
    /// Create a new OpCode
    pub const fn new(code: u8, mnemonic: &'static str, mode: &'static str) -> Self {
        Self {
            code,
            mnemonic,
            mode,
        }
    }

    /// Get the full instruction name (e.g., "ADC_IMM")
    pub fn name(&self) -> String {
        format!("{}_{}", self.mnemonic, self.mode)
    }
}

// Opcode constants for use in match patterns
pub const BRK: u8 = 0x00;
pub const ORA_INDX: u8 = 0x01;
pub const ORA_ZP: u8 = 0x05;
pub const ASL_ZP: u8 = 0x06;
pub const PHP: u8 = 0x08;
pub const ORA_IMM: u8 = 0x09;
pub const ASL_A: u8 = 0x0A;
pub const ORA_ABS: u8 = 0x0D;
pub const ASL_ABS: u8 = 0x0E;
pub const BPL: u8 = 0x10;
pub const ORA_INDY: u8 = 0x11;
pub const ORA_ZPX: u8 = 0x15;
pub const ASL_ZPX: u8 = 0x16;
pub const CLC: u8 = 0x18;
pub const ORA_ABSY: u8 = 0x19;
pub const ORA_ABSX: u8 = 0x1D;
pub const ASL_ABSX: u8 = 0x1E;
pub const JSR: u8 = 0x20;
pub const AND_INDX: u8 = 0x21;
pub const BIT_ZP: u8 = 0x24;
pub const AND_ZP: u8 = 0x25;
pub const ROL_ZP: u8 = 0x26;
pub const PLP: u8 = 0x28;
pub const AND_IMM: u8 = 0x29;
pub const ROL_ACC: u8 = 0x2A;
pub const BIT_ABS: u8 = 0x2C;
pub const AND_ABS: u8 = 0x2D;
pub const ROL_ABS: u8 = 0x2E;
pub const BMI: u8 = 0x30;
pub const AND_INDY: u8 = 0x31;
pub const AND_ZPX: u8 = 0x35;
pub const ROL_ZPX: u8 = 0x36;
pub const SEC: u8 = 0x38;
pub const AND_ABSY: u8 = 0x39;
pub const AND_ABSX: u8 = 0x3D;
pub const ROL_ABSX: u8 = 0x3E;
pub const RTI: u8 = 0x40;
pub const EOR_INDX: u8 = 0x41;
pub const EOR_ZP: u8 = 0x45;
pub const LSR_ZP: u8 = 0x46;
pub const PHA: u8 = 0x48;
pub const EOR_IMM: u8 = 0x49;
pub const LSR_ACC: u8 = 0x4A;
pub const JMP_ABS: u8 = 0x4C;
pub const EOR_ABS: u8 = 0x4D;
pub const LSR_ABS: u8 = 0x4E;
pub const BVC: u8 = 0x50;
pub const EOR_INDY: u8 = 0x51;
pub const EOR_ZPX: u8 = 0x55;
pub const LSR_ZPX: u8 = 0x56;
pub const CLI: u8 = 0x58;
pub const EOR_ABSY: u8 = 0x59;
pub const EOR_ABSX: u8 = 0x5D;
pub const LSR_ABSX: u8 = 0x5E;
pub const RTS: u8 = 0x60;
pub const ADC_INDX: u8 = 0x61;
pub const ADC_ZP: u8 = 0x65;
pub const ROR_ZP: u8 = 0x66;
pub const PLA: u8 = 0x68;
pub const ADC_IMM: u8 = 0x69;
pub const ROR_ACC: u8 = 0x6A;
pub const JMP_IND: u8 = 0x6C;
pub const ADC_ABS: u8 = 0x6D;
pub const ROR_ABS: u8 = 0x6E;
pub const BVS: u8 = 0x70;
pub const ADC_INDY: u8 = 0x71;
pub const ADC_ZPX: u8 = 0x75;
pub const ROR_ZPX: u8 = 0x76;
pub const SEI: u8 = 0x78;
pub const ADC_ABSY: u8 = 0x79;
pub const ADC_ABSX: u8 = 0x7D;
pub const ROR_ABSX: u8 = 0x7E;
pub const STA_INDX: u8 = 0x81;
pub const STY_ZP: u8 = 0x84;
pub const STA_ZP: u8 = 0x85;
pub const STX_ZP: u8 = 0x86;
pub const DEY: u8 = 0x88;
pub const TXA: u8 = 0x8A;
pub const STY_ABS: u8 = 0x8C;
pub const STA_ABS: u8 = 0x8D;
pub const STX_ABS: u8 = 0x8E;
pub const BCC: u8 = 0x90;
pub const STA_INDY: u8 = 0x91;
pub const STY_ZPX: u8 = 0x94;
pub const STA_ZPX: u8 = 0x95;
pub const STX_ZPY: u8 = 0x96;
pub const TYA: u8 = 0x98;
pub const STA_ABSY: u8 = 0x99;
pub const TXS: u8 = 0x9A;
pub const STA_ABSX: u8 = 0x9D;
pub const LDY_IMM: u8 = 0xA0;
pub const LDA_INDX: u8 = 0xA1;
pub const LDX_IMM: u8 = 0xA2;
pub const LDY_ZP: u8 = 0xA4;
pub const LDA_ZP: u8 = 0xA5;
pub const LDX_ZP: u8 = 0xA6;
pub const TAY: u8 = 0xA8;
pub const LDA_IMM: u8 = 0xA9;
pub const TAX: u8 = 0xAA;
pub const LDY_ABS: u8 = 0xAC;
pub const LDA_ABS: u8 = 0xAD;
pub const LDX_ABS: u8 = 0xAE;
pub const BCS: u8 = 0xB0;
pub const LDA_INDY: u8 = 0xB1;
pub const LDY_ZPX: u8 = 0xB4;
pub const LDA_ZPX: u8 = 0xB5;
pub const LDX_ZPY: u8 = 0xB6;
pub const CLV: u8 = 0xB8;
pub const LDA_ABSY: u8 = 0xB9;
pub const TSX: u8 = 0xBA;
pub const LDY_ABSX: u8 = 0xBC;
pub const LDA_ABSX: u8 = 0xBD;
pub const LDX_ABSY: u8 = 0xBE;
pub const CPY_IMM: u8 = 0xC0;
pub const CMP_INDX: u8 = 0xC1;
pub const CPY_ZP: u8 = 0xC4;
pub const CMP_ZP: u8 = 0xC5;
pub const DEC_ZP: u8 = 0xC6;
pub const INY: u8 = 0xC8;
pub const CMP_IMM: u8 = 0xC9;
pub const DEX: u8 = 0xCA;
pub const CPY_ABS: u8 = 0xCC;
pub const CMP_ABS: u8 = 0xCD;
pub const DEC_ABS: u8 = 0xCE;
pub const BNE: u8 = 0xD0;
pub const CMP_INDY: u8 = 0xD1;
pub const CMP_ZPX: u8 = 0xD5;
pub const DEC_ZPX: u8 = 0xD6;
pub const CLD: u8 = 0xD8;
pub const CMP_ABSY: u8 = 0xD9;
pub const CMP_ABSX: u8 = 0xDD;
pub const DEC_ABSX: u8 = 0xDE;
pub const CPX_IMM: u8 = 0xE0;
pub const SBC_INDX: u8 = 0xE1;
pub const CPX_ZP: u8 = 0xE4;
pub const SBC_ZP: u8 = 0xE5;
pub const INC_ZP: u8 = 0xE6;
pub const INX: u8 = 0xE8;
pub const SBC_IMM: u8 = 0xE9;
pub const NOP: u8 = 0xEA;
pub const CPX_ABS: u8 = 0xEC;
pub const SBC_ABS: u8 = 0xED;
pub const INC_ABS: u8 = 0xEE;
pub const BEQ: u8 = 0xF0;
pub const SBC_INDY: u8 = 0xF1;
pub const SBC_ZPX: u8 = 0xF5;
pub const INC_ZPX: u8 = 0xF6;
pub const SED: u8 = 0xF8;
pub const SBC_ABSY: u8 = 0xF9;
pub const SBC_ABSX: u8 = 0xFD;
pub const INC_ABSX: u8 = 0xFE;

// Complete NES 6502 opcode table
pub static OPCODE_TABLE: &[OpCode] = &[
    OpCode::new(BRK, "BRK", "IMP"),
    OpCode::new(ORA_INDX, "ORA", "INDX"),
    OpCode::new(ORA_ZP, "ORA", "ZP"),
    OpCode::new(ASL_ZP, "ASL", "ZP"),
    OpCode::new(PHP, "PHP", "IMP"),
    OpCode::new(ORA_IMM, "ORA", "IMM"),
    OpCode::new(ASL_A, "ASL", "ACC"),
    OpCode::new(ORA_ABS, "ORA", "ABS"),
    OpCode::new(ASL_ABS, "ASL", "ABS"),
    OpCode::new(BPL, "BPL", "REL"),
    OpCode::new(ORA_INDY, "ORA", "INDY"),
    OpCode::new(ORA_ZPX, "ORA", "ZPX"),
    OpCode::new(ASL_ZPX, "ASL", "ZPX"),
    OpCode::new(CLC, "CLC", "IMP"),
    OpCode::new(ORA_ABSY, "ORA", "ABSY"),
    OpCode::new(ORA_ABSX, "ORA", "ABSX"),
    OpCode::new(ASL_ABSX, "ASL", "ABSX"),
    OpCode::new(JSR, "JSR", "ABS"),
    OpCode::new(AND_INDX, "AND", "INDX"),
    OpCode::new(BIT_ZP, "BIT", "ZP"),
    OpCode::new(AND_ZP, "AND", "ZP"),
    OpCode::new(ROL_ZP, "ROL", "ZP"),
    OpCode::new(PLP, "PLP", "IMP"),
    OpCode::new(AND_IMM, "AND", "IMM"),
    OpCode::new(ROL_ACC, "ROL", "ACC"),
    OpCode::new(BIT_ABS, "BIT", "ABS"),
    OpCode::new(AND_ABS, "AND", "ABS"),
    OpCode::new(ROL_ABS, "ROL", "ABS"),
    OpCode::new(BMI, "BMI", "REL"),
    OpCode::new(AND_INDY, "AND", "INDY"),
    OpCode::new(AND_ZPX, "AND", "ZPX"),
    OpCode::new(ROL_ZPX, "ROL", "ZPX"),
    OpCode::new(SEC, "SEC", "IMP"),
    OpCode::new(AND_ABSY, "AND", "ABSY"),
    OpCode::new(AND_ABSX, "AND", "ABSX"),
    OpCode::new(ROL_ABSX, "ROL", "ABSX"),
    OpCode::new(RTI, "RTI", "IMP"),
    OpCode::new(EOR_INDX, "EOR", "INDX"),
    OpCode::new(EOR_ZP, "EOR", "ZP"),
    OpCode::new(LSR_ZP, "LSR", "ZP"),
    OpCode::new(PHA, "PHA", "IMP"),
    OpCode::new(EOR_IMM, "EOR", "IMM"),
    OpCode::new(LSR_ACC, "LSR", "ACC"),
    OpCode::new(JMP_ABS, "JMP", "ABS"),
    OpCode::new(EOR_ABS, "EOR", "ABS"),
    OpCode::new(LSR_ABS, "LSR", "ABS"),
    OpCode::new(BVC, "BVC", "REL"),
    OpCode::new(EOR_INDY, "EOR", "INDY"),
    OpCode::new(EOR_ZPX, "EOR", "ZPX"),
    OpCode::new(LSR_ZPX, "LSR", "ZPX"),
    OpCode::new(CLI, "CLI", "IMP"),
    OpCode::new(EOR_ABSY, "EOR", "ABSY"),
    OpCode::new(EOR_ABSX, "EOR", "ABSX"),
    OpCode::new(LSR_ABSX, "LSR", "ABSX"),
    OpCode::new(RTS, "RTS", "IMP"),
    OpCode::new(ADC_INDX, "ADC", "INDX"),
    OpCode::new(ADC_ZP, "ADC", "ZP"),
    OpCode::new(ROR_ZP, "ROR", "ZP"),
    OpCode::new(PLA, "PLA", "IMP"),
    OpCode::new(ADC_IMM, "ADC", "IMM"),
    OpCode::new(ROR_ACC, "ROR", "ACC"),
    OpCode::new(JMP_IND, "JMP", "IND"),
    OpCode::new(ADC_ABS, "ADC", "ABS"),
    OpCode::new(ROR_ABS, "ROR", "ABS"),
    OpCode::new(BVS, "BVS", "REL"),
    OpCode::new(ADC_INDY, "ADC", "INDY"),
    OpCode::new(ADC_ZPX, "ADC", "ZPX"),
    OpCode::new(ROR_ZPX, "ROR", "ZPX"),
    OpCode::new(SEI, "SEI", "IMP"),
    OpCode::new(ADC_ABSY, "ADC", "ABSY"),
    OpCode::new(ADC_ABSX, "ADC", "ABSX"),
    OpCode::new(ROR_ABSX, "ROR", "ABSX"),
    OpCode::new(STA_INDX, "STA", "INDX"),
    OpCode::new(STY_ZP, "STY", "ZP"),
    OpCode::new(STA_ZP, "STA", "ZP"),
    OpCode::new(STX_ZP, "STX", "ZP"),
    OpCode::new(DEY, "DEY", "IMP"),
    OpCode::new(TXA, "TXA", "IMP"),
    OpCode::new(STY_ABS, "STY", "ABS"),
    OpCode::new(STA_ABS, "STA", "ABS"),
    OpCode::new(STX_ABS, "STX", "ABS"),
    OpCode::new(BCC, "BCC", "REL"),
    OpCode::new(STA_INDY, "STA", "INDY"),
    OpCode::new(STY_ZPX, "STY", "ZPX"),
    OpCode::new(STA_ZPX, "STA", "ZPX"),
    OpCode::new(STX_ZPY, "STX", "ZPY"),
    OpCode::new(TYA, "TYA", "IMP"),
    OpCode::new(STA_ABSY, "STA", "ABSY"),
    OpCode::new(TXS, "TXS", "IMP"),
    OpCode::new(STA_ABSX, "STA", "ABSX"),
    OpCode::new(LDY_IMM, "LDY", "IMM"),
    OpCode::new(LDA_INDX, "LDA", "INDX"),
    OpCode::new(LDX_IMM, "LDX", "IMM"),
    OpCode::new(LDY_ZP, "LDY", "ZP"),
    OpCode::new(LDA_ZP, "LDA", "ZP"),
    OpCode::new(LDX_ZP, "LDX", "ZP"),
    OpCode::new(TAY, "TAY", "IMP"),
    OpCode::new(LDA_IMM, "LDA", "IMM"),
    OpCode::new(TAX, "TAX", "IMP"),
    OpCode::new(LDY_ABS, "LDY", "ABS"),
    OpCode::new(LDA_ABS, "LDA", "ABS"),
    OpCode::new(LDX_ABS, "LDX", "ABS"),
    OpCode::new(BCS, "BCS", "REL"),
    OpCode::new(LDA_INDY, "LDA", "INDY"),
    OpCode::new(LDY_ZPX, "LDY", "ZPX"),
    OpCode::new(LDA_ZPX, "LDA", "ZPX"),
    OpCode::new(LDX_ZPY, "LDX", "ZPY"),
    OpCode::new(CLV, "CLV", "IMP"),
    OpCode::new(LDA_ABSY, "LDA", "ABSY"),
    OpCode::new(TSX, "TSX", "IMP"),
    OpCode::new(LDY_ABSX, "LDY", "ABSX"),
    OpCode::new(LDA_ABSX, "LDA", "ABSX"),
    OpCode::new(LDX_ABSY, "LDX", "ABSY"),
    OpCode::new(CPY_IMM, "CPY", "IMM"),
    OpCode::new(CMP_INDX, "CMP", "INDX"),
    OpCode::new(CPY_ZP, "CPY", "ZP"),
    OpCode::new(CMP_ZP, "CMP", "ZP"),
    OpCode::new(DEC_ZP, "DEC", "ZP"),
    OpCode::new(INY, "INY", "IMP"),
    OpCode::new(CMP_IMM, "CMP", "IMM"),
    OpCode::new(DEX, "DEX", "IMP"),
    OpCode::new(CPY_ABS, "CPY", "ABS"),
    OpCode::new(CMP_ABS, "CMP", "ABS"),
    OpCode::new(DEC_ABS, "DEC", "ABS"),
    OpCode::new(BNE, "BNE", "REL"),
    OpCode::new(CMP_INDY, "CMP", "INDY"),
    OpCode::new(CMP_ZPX, "CMP", "ZPX"),
    OpCode::new(DEC_ZPX, "DEC", "ZPX"),
    OpCode::new(CLD, "CLD", "IMP"),
    OpCode::new(CMP_ABSY, "CMP", "ABSY"),
    OpCode::new(CMP_ABSX, "CMP", "ABSX"),
    OpCode::new(DEC_ABSX, "DEC", "ABSX"),
    OpCode::new(CPX_IMM, "CPX", "IMM"),
    OpCode::new(SBC_INDX, "SBC", "INDX"),
    OpCode::new(CPX_ZP, "CPX", "ZP"),
    OpCode::new(SBC_ZP, "SBC", "ZP"),
    OpCode::new(INC_ZP, "INC", "ZP"),
    OpCode::new(INX, "INX", "IMP"),
    OpCode::new(SBC_IMM, "SBC", "IMM"),
    OpCode::new(NOP, "NOP", "IMP"),
    OpCode::new(CPX_ABS, "CPX", "ABS"),
    OpCode::new(SBC_ABS, "SBC", "ABS"),
    OpCode::new(INC_ABS, "INC", "ABS"),
    OpCode::new(BEQ, "BEQ", "REL"),
    OpCode::new(SBC_INDY, "SBC", "INDY"),
    OpCode::new(SBC_ZPX, "SBC", "ZPX"),
    OpCode::new(INC_ZPX, "INC", "ZPX"),
    OpCode::new(SED, "SED", "IMP"),
    OpCode::new(SBC_ABSY, "SBC", "ABSY"),
    OpCode::new(SBC_ABSX, "SBC", "ABSX"),
    OpCode::new(INC_ABSX, "INC", "ABSX"),
];

/// Lookup an opcode by its byte value
pub fn lookup(code: u8) -> Option<&'static OpCode> {
    OPCODE_TABLE.iter().find(|op| op.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_creation() {
        let opcode = OpCode::new(0x69, "ADC", "IMM");
        assert_eq!(opcode.code, 0x69);
        assert_eq!(opcode.mnemonic, "ADC");
        assert_eq!(opcode.mode, "IMM");
    }

    #[test]
    fn test_opcode_name() {
        let opcode = OpCode::new(0x69, "ADC", "IMM");
        assert_eq!(opcode.name(), "ADC_IMM");
    }

    #[test]
    fn test_opcode_name_different_instruction() {
        let opcode = OpCode::new(0xA9, "LDA", "IMM");
        assert_eq!(opcode.name(), "LDA_IMM");
    }

    #[test]
    fn test_opcode_equality() {
        let opcode1 = OpCode::new(0x69, "ADC", "IMM");
        let opcode2 = OpCode::new(0x69, "ADC", "IMM");
        assert_eq!(opcode1, opcode2);
    }

    #[test]
    fn test_opcode_inequality() {
        let opcode1 = OpCode::new(0x69, "ADC", "IMM");
        let opcode2 = OpCode::new(0x6D, "ADC", "ABS");
        assert_ne!(opcode1, opcode2);
    }

    #[test]
    fn test_opcodes_table_count() {
        assert_eq!(OPCODE_TABLE.len(), 151);
    }

    #[test]
    fn test_lookup_existing_opcode() {
        let opcode = lookup(0x69).unwrap();
        assert_eq!(opcode.code, 0x69);
        assert_eq!(opcode.mnemonic, "ADC");
        assert_eq!(opcode.mode, "IMM");
    }

    #[test]
    fn test_lookup_brk() {
        let opcode = lookup(0x00).unwrap();
        assert_eq!(opcode.mnemonic, "BRK");
    }

    #[test]
    fn test_lookup_lda_immediate() {
        let opcode = lookup(0xA9).unwrap();
        assert_eq!(opcode.mnemonic, "LDA");
        assert_eq!(opcode.mode, "IMM");
    }

    #[test]
    fn test_all_opcodes_unique() {
        use std::collections::HashSet;
        let mut codes = HashSet::new();
        for opcode in OPCODE_TABLE {
            assert!(
                codes.insert(opcode.code),
                "Duplicate opcode: 0x{:02X}",
                opcode.code
            );
        }
    }
}
