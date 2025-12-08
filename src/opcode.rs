/// Represents a 6502 instruction opcode with its mnemonic and addressing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpCode {
    /// The opcode byte value
    pub code: u8,
    /// The instruction mnemonic (e.g., "ADC", "LDA")
    pub mnemonic: &'static str,
    /// The addressing mode (e.g., "IMM", "ABS", "ZP")
    pub mode: &'static str,
    /// The base number of cycles this instruction takes
    pub cycles: u8,
}

impl OpCode {
    /// Create a new OpCode
    pub const fn new(code: u8, mnemonic: &'static str, mode: &'static str, cycles: u8) -> Self {
        Self {
            code,
            mnemonic,
            mode,
            cycles,
        }
    }

    /// Get the full instruction name (e.g., "ADC_IMM")
    #[cfg(test)]
    pub fn name(&self) -> String {
        format!("{}_{}", self.mnemonic, self.mode)
    }

    /// Get the number of bytes for this instruction based on its addressing mode
    pub fn bytes(&self) -> u8 {
        match self.mode {
            "IMP" | "ACC" => 1,
            "IMM" | "ZP" | "ZPX" | "ZPY" | "INDX" | "INDY" | "REL" => 2,
            "ABS" | "ABSX" | "ABSY" | "IND" => 3,
            _ => panic!("Unknown addressing mode"),
        }
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

// Undocumented opcodes
pub const AAC_IMM: u8 = 0x0B;
pub const AAC_IMM2: u8 = 0x2B;
pub const ARR_IMM: u8 = 0x6B;
pub const ASR_IMM: u8 = 0x4B;
pub const ATX_IMM: u8 = 0xAB;
pub const SAX_INDX: u8 = 0x83;
pub const SAX_ZP: u8 = 0x87;
pub const SAX_ABS: u8 = 0x8F;
pub const SAX_ZPY: u8 = 0x97;
pub const AXA_INDY: u8 = 0x93;
pub const AXA_ABSY: u8 = 0x9F;
pub const AXS_IMM: u8 = 0xCB;
pub const DCP_INDX: u8 = 0xC3;
pub const DCP_ZP: u8 = 0xC7;
pub const DCP_ABS: u8 = 0xCF;
pub const DCP_INDY: u8 = 0xD3;
pub const DCP_ZPX: u8 = 0xD7;
pub const DCP_ABSY: u8 = 0xDB;
pub const DCP_ABSX: u8 = 0xDF;
pub const DOP_ZP: u8 = 0x04;
pub const DOP_ZPX: u8 = 0x14;
pub const DOP_ZPX2: u8 = 0x34;
pub const DOP_ZP2: u8 = 0x44;
pub const DOP_ZPX3: u8 = 0x54;
pub const DOP_ZP3: u8 = 0x64;
pub const DOP_ZPX4: u8 = 0x74;
pub const DOP_IMM: u8 = 0x80;
pub const DOP_IMM2: u8 = 0x82;
pub const DOP_IMM3: u8 = 0x89;
pub const DOP_IMM4: u8 = 0xC2;
pub const DOP_ZPX5: u8 = 0xD4;
pub const DOP_IMM5: u8 = 0xE2;
pub const DOP_ZPX6: u8 = 0xF4;
pub const ISB_INDX: u8 = 0xE3;
pub const ISB_ZP: u8 = 0xE7;
pub const ISB_ABS: u8 = 0xEF;
pub const ISB_INDY: u8 = 0xF3;
pub const ISB_ZPX: u8 = 0xF7;
pub const ISB_ABSY: u8 = 0xFB;
pub const ISB_ABSX: u8 = 0xFF;
pub const KIL: u8 = 0x02;
pub const KIL2: u8 = 0x12;
pub const KIL3: u8 = 0x22;
pub const KIL4: u8 = 0x32;
pub const KIL5: u8 = 0x42;
pub const KIL6: u8 = 0x52;
pub const KIL7: u8 = 0x62;
pub const KIL8: u8 = 0x72;
pub const KIL9: u8 = 0x92;
pub const KIL10: u8 = 0xB2;
pub const KIL11: u8 = 0xD2;
pub const KIL12: u8 = 0xF2;
pub const LAR_ABSY: u8 = 0xBB;
pub const LAX_INDX: u8 = 0xA3;
pub const LAX_ZP: u8 = 0xA7;
pub const LAX_ABS: u8 = 0xAF;
pub const LAX_INDY: u8 = 0xB3;
pub const LAX_ZPY: u8 = 0xB7;
pub const LAX_ABSY: u8 = 0xBF;
pub const NOP_IMP: u8 = 0x1A;
pub const NOP_IMP2: u8 = 0x3A;
pub const NOP_IMP3: u8 = 0x5A;
pub const NOP_IMP4: u8 = 0x7A;
pub const NOP_IMP5: u8 = 0xDA;
pub const NOP_IMP6: u8 = 0xFA;
pub const RLA_INDX: u8 = 0x23;
pub const RLA_ZP: u8 = 0x27;
pub const RLA_ABS: u8 = 0x2F;
pub const RLA_INDY: u8 = 0x33;
pub const RLA_ZPX: u8 = 0x37;
pub const RLA_ABSY: u8 = 0x3B;
pub const RLA_ABSX: u8 = 0x3F;
pub const RRA_INDX: u8 = 0x63;
pub const RRA_ZP: u8 = 0x67;
pub const RRA_ABS: u8 = 0x6F;
pub const RRA_INDY: u8 = 0x73;
pub const RRA_ZPX: u8 = 0x77;
pub const RRA_ABSY: u8 = 0x7B;
pub const RRA_ABSX: u8 = 0x7F;
pub const SBC_IMM2: u8 = 0xEB;
pub const SLO_INDX: u8 = 0x03;
pub const SLO_ZP: u8 = 0x07;
pub const SLO_ABS: u8 = 0x0F;
pub const SLO_INDY: u8 = 0x13;
pub const SLO_ZPX: u8 = 0x17;
pub const SLO_ABSY: u8 = 0x1B;
pub const SLO_ABSX: u8 = 0x1F;
pub const SRE_INDX: u8 = 0x43;
pub const SRE_ZP: u8 = 0x47;
pub const SRE_ABS: u8 = 0x4F;
pub const SRE_INDY: u8 = 0x53;
pub const SRE_ZPX: u8 = 0x57;
pub const SRE_ABSY: u8 = 0x5B;
pub const SRE_ABSX: u8 = 0x5F;
pub const SXA_ABSY: u8 = 0x9E;
pub const SYA_ABSX: u8 = 0x9C;
pub const TOP_ABS: u8 = 0x0C;
pub const TOP_ABSX: u8 = 0x1C;
pub const TOP_ABSX2: u8 = 0x3C;
pub const TOP_ABSX3: u8 = 0x5C;
pub const TOP_ABSX4: u8 = 0x7C;
pub const TOP_ABSX5: u8 = 0xDC;
pub const TOP_ABSX6: u8 = 0xFC;
pub const XAA_IMM: u8 = 0x8B;
pub const XAS_ABSY: u8 = 0x9B;

// Complete NES 6502 opcode table
pub static OPCODE_TABLE: &[OpCode; 256] = &[
    OpCode::new(BRK, "BRK", "IMP", 7),
    OpCode::new(ORA_INDX, "ORA", "INDX", 6),
    OpCode::new(KIL, "KIL", "IMP", 2),
    OpCode::new(SLO_INDX, "*SLO", "INDX", 8),
    OpCode::new(ORA_ZP, "ORA", "ZP", 3),
    OpCode::new(ASL_ZP, "ASL", "ZP", 5),
    OpCode::new(DOP_ZP, "*NOP", "ZP", 3),
    OpCode::new(SLO_ZP, "*SLO", "ZP", 5),
    OpCode::new(PHP, "PHP", "IMP", 3),
    OpCode::new(ORA_IMM, "ORA", "IMM", 2),
    OpCode::new(ASL_A, "ASL", "ACC", 2),
    OpCode::new(AAC_IMM, "*AAC", "IMM", 2),
    OpCode::new(ORA_ABS, "ORA", "ABS", 4),
    OpCode::new(ASL_ABS, "ASL", "ABS", 6),
    OpCode::new(TOP_ABS, "*NOP", "ABS", 4),
    OpCode::new(SLO_ABS, "*SLO", "ABS", 6),
    OpCode::new(BPL, "BPL", "REL", 2),
    OpCode::new(ORA_INDY, "ORA", "INDY", 5),
    OpCode::new(KIL2, "KIL", "IMP", 2),
    OpCode::new(SLO_INDY, "*SLO", "INDY", 8),
    OpCode::new(ORA_ZPX, "ORA", "ZPX", 4),
    OpCode::new(ASL_ZPX, "ASL", "ZPX", 6),
    OpCode::new(CLC, "CLC", "IMP", 2),
    OpCode::new(DOP_ZPX, "*NOP", "ZPX", 4),
    OpCode::new(SLO_ZPX, "*SLO", "ZPX", 6),
    OpCode::new(ORA_ABSY, "ORA", "ABSY", 4),
    OpCode::new(NOP_IMP, "*NOP", "IMP", 2),
    OpCode::new(SLO_ABSY, "*SLO", "ABSY", 7),
    OpCode::new(ORA_ABSX, "ORA", "ABSX", 4),
    OpCode::new(ASL_ABSX, "ASL", "ABSX", 7),
    OpCode::new(TOP_ABSX, "*NOP", "ABSX", 4),
    OpCode::new(SLO_ABSX, "*SLO", "ABSX", 7),
    OpCode::new(JSR, "JSR", "ABS", 6),
    OpCode::new(AND_INDX, "AND", "INDX", 6),
    OpCode::new(KIL3, "KIL", "IMP", 2),
    OpCode::new(RLA_INDX, "*RLA", "INDX", 8),
    OpCode::new(BIT_ZP, "BIT", "ZP", 3),
    OpCode::new(AND_ZP, "AND", "ZP", 3),
    OpCode::new(ROL_ZP, "ROL", "ZP", 5),
    OpCode::new(RLA_ZP, "*RLA", "ZP", 5),
    OpCode::new(PLP, "PLP", "IMP", 4),
    OpCode::new(AND_IMM, "AND", "IMM", 2),
    OpCode::new(ROL_ACC, "ROL", "ACC", 2),
    OpCode::new(AAC_IMM2, "*AAC", "IMM", 2),
    OpCode::new(BIT_ABS, "BIT", "ABS", 4),
    OpCode::new(AND_ABS, "AND", "ABS", 4),
    OpCode::new(ROL_ABS, "ROL", "ABS", 6),
    OpCode::new(RLA_ABS, "*RLA", "ABS", 6),
    OpCode::new(BMI, "BMI", "REL", 2),
    OpCode::new(AND_INDY, "AND", "INDY", 5),
    OpCode::new(KIL4, "KIL", "IMP", 2),
    OpCode::new(RLA_INDY, "*RLA", "INDY", 8),
    OpCode::new(DOP_ZPX2, "*NOP", "ZPX", 4),
    OpCode::new(AND_ZPX, "AND", "ZPX", 4),
    OpCode::new(ROL_ZPX, "ROL", "ZPX", 6),
    OpCode::new(RLA_ZPX, "*RLA", "ZPX", 6),
    OpCode::new(SEC, "SEC", "IMP", 2),
    OpCode::new(AND_ABSY, "AND", "ABSY", 4),
    OpCode::new(NOP_IMP2, "*NOP", "IMP", 2),
    OpCode::new(RLA_ABSY, "*RLA", "ABSY", 7),
    OpCode::new(AND_ABSX, "AND", "ABSX", 4),
    OpCode::new(ROL_ABSX, "ROL", "ABSX", 7),
    OpCode::new(TOP_ABSX2, "*NOP", "ABSX", 4),
    OpCode::new(RLA_ABSX, "*RLA", "ABSX", 7),
    OpCode::new(RTI, "RTI", "IMP", 6),
    OpCode::new(EOR_INDX, "EOR", "INDX", 6),
    OpCode::new(KIL5, "KIL", "IMP", 2),
    OpCode::new(SRE_INDX, "*SRE", "INDX", 8),
    OpCode::new(DOP_ZP2, "*NOP", "ZP", 3),
    OpCode::new(EOR_ZP, "EOR", "ZP", 3),
    OpCode::new(LSR_ZP, "LSR", "ZP", 5),
    OpCode::new(SRE_ZP, "*SRE", "ZP", 5),
    OpCode::new(PHA, "PHA", "IMP", 3),
    OpCode::new(EOR_IMM, "EOR", "IMM", 2),
    OpCode::new(ASR_IMM, "*ASR", "IMM", 2),
    OpCode::new(LSR_ACC, "LSR", "ACC", 2),
    OpCode::new(JMP_ABS, "JMP", "ABS", 3),
    OpCode::new(EOR_ABS, "EOR", "ABS", 4),
    OpCode::new(LSR_ABS, "LSR", "ABS", 6),
    OpCode::new(SRE_ABS, "*SRE", "ABS", 6),
    OpCode::new(BVC, "BVC", "REL", 2),
    OpCode::new(EOR_INDY, "EOR", "INDY", 5),
    OpCode::new(KIL6, "KIL", "IMP", 2),
    OpCode::new(SRE_INDY, "*SRE", "INDY", 8),
    OpCode::new(DOP_ZPX3, "*NOP", "ZPX", 4),
    OpCode::new(EOR_ZPX, "EOR", "ZPX", 4),
    OpCode::new(LSR_ZPX, "LSR", "ZPX", 6),
    OpCode::new(SRE_ZPX, "*SRE", "ZPX", 6),
    OpCode::new(CLI, "CLI", "IMP", 2),
    OpCode::new(EOR_ABSY, "EOR", "ABSY", 4),
    OpCode::new(NOP_IMP3, "*NOP", "IMP", 2),
    OpCode::new(SRE_ABSY, "*SRE", "ABSY", 7),
    OpCode::new(EOR_ABSX, "EOR", "ABSX", 4),
    OpCode::new(LSR_ABSX, "LSR", "ABSX", 7),
    OpCode::new(TOP_ABSX3, "*NOP", "ABSX", 4),
    OpCode::new(SRE_ABSX, "*SRE", "ABSX", 7),
    OpCode::new(RTS, "RTS", "IMP", 6),
    OpCode::new(ADC_INDX, "ADC", "INDX", 6),
    OpCode::new(KIL7, "KIL", "IMP", 2),
    OpCode::new(RRA_INDX, "*RRA", "INDX", 8),
    OpCode::new(ADC_ZP, "ADC", "ZP", 3),
    OpCode::new(ROR_ZP, "ROR", "ZP", 5),
    OpCode::new(PLA, "PLA", "IMP", 4),
    OpCode::new(DOP_ZP3, "*NOP", "ZP", 3),
    OpCode::new(RRA_ZP, "*RRA", "ZP", 5),
    OpCode::new(ADC_IMM, "ADC", "IMM", 2),
    OpCode::new(ARR_IMM, "*ARR", "IMM", 2),
    OpCode::new(ROR_ACC, "ROR", "ACC", 2),
    OpCode::new(JMP_IND, "JMP", "IND", 5),
    OpCode::new(ADC_ABS, "ADC", "ABS", 4),
    OpCode::new(ROR_ABS, "ROR", "ABS", 6),
    OpCode::new(RRA_ABS, "*RRA", "ABS", 6),
    OpCode::new(BVS, "BVS", "REL", 2),
    OpCode::new(ADC_INDY, "ADC", "INDY", 5),
    OpCode::new(KIL8, "KIL", "IMP", 2),
    OpCode::new(RRA_INDY, "*RRA", "INDY", 8),
    OpCode::new(ADC_ZPX, "ADC", "ZPX", 4),
    OpCode::new(ROR_ZPX, "ROR", "ZPX", 6),
    OpCode::new(SEI, "SEI", "IMP", 2),
    OpCode::new(DOP_ZPX4, "*NOP", "ZPX", 4),
    OpCode::new(RRA_ZPX, "*RRA", "ZPX", 6),
    OpCode::new(ADC_ABSY, "ADC", "ABSY", 4),
    OpCode::new(NOP_IMP4, "*NOP", "IMP", 2),
    OpCode::new(RRA_ABSY, "*RRA", "ABSY", 7),
    OpCode::new(ADC_ABSX, "ADC", "ABSX", 4),
    OpCode::new(ROR_ABSX, "ROR", "ABSX", 7),
    OpCode::new(TOP_ABSX4, "*NOP", "ABSX", 4),
    OpCode::new(RRA_ABSX, "*RRA", "ABSX", 7),
    OpCode::new(DOP_IMM, "*NOP", "IMM", 2),
    OpCode::new(STA_INDX, "STA", "INDX", 6),
    OpCode::new(DOP_IMM2, "*NOP", "IMM", 2),
    OpCode::new(SAX_INDX, "*SAX", "INDX", 6),
    OpCode::new(STY_ZP, "STY", "ZP", 3),
    OpCode::new(STA_ZP, "STA", "ZP", 3),
    OpCode::new(STX_ZP, "STX", "ZP", 3),
    OpCode::new(SAX_ZP, "*SAX", "ZP", 3),
    OpCode::new(DEY, "DEY", "IMP", 2),
    OpCode::new(DOP_IMM3, "*NOP", "IMM", 2),
    OpCode::new(TXA, "TXA", "IMP", 2),
    OpCode::new(XAA_IMM, "*XAA", "IMM", 2),
    OpCode::new(STY_ABS, "STY", "ABS", 4),
    OpCode::new(STA_ABS, "STA", "ABS", 4),
    OpCode::new(STX_ABS, "STX", "ABS", 4),
    OpCode::new(SAX_ABS, "*SAX", "ABS", 4),
    OpCode::new(BCC, "BCC", "REL", 2),
    OpCode::new(STA_INDY, "STA", "INDY", 6),
    OpCode::new(KIL9, "KIL", "IMP", 2),
    OpCode::new(AXA_INDY, "*AXA", "INDY", 6),
    OpCode::new(STY_ZPX, "STY", "ZPX", 4),
    OpCode::new(STA_ZPX, "STA", "ZPX", 4),
    OpCode::new(STX_ZPY, "STX", "ZPY", 4),
    OpCode::new(SAX_ZPY, "*SAX", "ZPY", 4),
    OpCode::new(TYA, "TYA", "IMP", 2),
    OpCode::new(STA_ABSY, "STA", "ABSY", 5),
    OpCode::new(TXS, "TXS", "IMP", 2),
    OpCode::new(XAS_ABSY, "*XAS", "ABSY", 5),
    OpCode::new(SYA_ABSX, "*SYA", "ABSX", 5),
    OpCode::new(STA_ABSX, "STA", "ABSX", 5),
    OpCode::new(SXA_ABSY, "*SXA", "ABSY", 5),
    OpCode::new(AXA_ABSY, "*AXA", "ABSY", 5),
    OpCode::new(LDY_IMM, "LDY", "IMM", 2),
    OpCode::new(LDA_INDX, "LDA", "INDX", 6),
    OpCode::new(LDX_IMM, "LDX", "IMM", 2),
    OpCode::new(LAX_INDX, "*LAX", "INDX", 6),
    OpCode::new(LDY_ZP, "LDY", "ZP", 3),
    OpCode::new(LDA_ZP, "LDA", "ZP", 3),
    OpCode::new(LDX_ZP, "LDX", "ZP", 3),
    OpCode::new(LAX_ZP, "*LAX", "ZP", 3),
    OpCode::new(TAY, "TAY", "IMP", 2),
    OpCode::new(LDA_IMM, "LDA", "IMM", 2),
    OpCode::new(TAX, "TAX", "IMP", 2),
    OpCode::new(ATX_IMM, "*ATX", "IMM", 2),
    OpCode::new(LDY_ABS, "LDY", "ABS", 4),
    OpCode::new(LDA_ABS, "LDA", "ABS", 4),
    OpCode::new(LDX_ABS, "LDX", "ABS", 4),
    OpCode::new(LAX_ABS, "*LAX", "ABS", 4),
    OpCode::new(BCS, "BCS", "REL", 2),
    OpCode::new(LDA_INDY, "LDA", "INDY", 5),
    OpCode::new(KIL10, "KIL", "IMP", 2),
    OpCode::new(LAX_INDY, "*LAX", "INDY", 5),
    OpCode::new(LDY_ZPX, "LDY", "ZPX", 4),
    OpCode::new(LDA_ZPX, "LDA", "ZPX", 4),
    OpCode::new(LDX_ZPY, "LDX", "ZPY", 4),
    OpCode::new(LAX_ZPY, "*LAX", "ZPY", 4),
    OpCode::new(CLV, "CLV", "IMP", 2),
    OpCode::new(LDA_ABSY, "LDA", "ABSY", 4),
    OpCode::new(TSX, "TSX", "IMP", 2),
    OpCode::new(LAR_ABSY, "*LAR", "ABSY", 4),
    OpCode::new(LDY_ABSX, "LDY", "ABSX", 4),
    OpCode::new(LDA_ABSX, "LDA", "ABSX", 4),
    OpCode::new(LDX_ABSY, "LDX", "ABSY", 4),
    OpCode::new(LAX_ABSY, "*LAX", "ABSY", 4),
    OpCode::new(CPY_IMM, "CPY", "IMM", 2),
    OpCode::new(CMP_INDX, "CMP", "INDX", 6),
    OpCode::new(DOP_IMM4, "*NOP", "IMM", 2),
    OpCode::new(DCP_INDX, "*DCP", "INDX", 8),
    OpCode::new(CPY_ZP, "CPY", "ZP", 3),
    OpCode::new(CMP_ZP, "CMP", "ZP", 3),
    OpCode::new(DEC_ZP, "DEC", "ZP", 5),
    OpCode::new(DCP_ZP, "*DCP", "ZP", 5),
    OpCode::new(INY, "INY", "IMP", 2),
    OpCode::new(CMP_IMM, "CMP", "IMM", 2),
    OpCode::new(DEX, "DEX", "IMP", 2),
    OpCode::new(AXS_IMM, "*AXS", "IMM", 2),
    OpCode::new(CPY_ABS, "CPY", "ABS", 4),
    OpCode::new(CMP_ABS, "CMP", "ABS", 4),
    OpCode::new(DEC_ABS, "DEC", "ABS", 6),
    OpCode::new(DCP_ABS, "*DCP", "ABS", 6),
    OpCode::new(BNE, "BNE", "REL", 2),
    OpCode::new(CMP_INDY, "CMP", "INDY", 5),
    OpCode::new(KIL11, "KIL", "IMP", 2),
    OpCode::new(DCP_INDY, "*DCP", "INDY", 8),
    OpCode::new(DOP_ZPX5, "*NOP", "ZPX", 4),
    OpCode::new(CMP_ZPX, "CMP", "ZPX", 4),
    OpCode::new(DEC_ZPX, "DEC", "ZPX", 6),
    OpCode::new(DCP_ZPX, "*DCP", "ZPX", 6),
    OpCode::new(CLD, "CLD", "IMP", 2),
    OpCode::new(CMP_ABSY, "CMP", "ABSY", 4),
    OpCode::new(NOP_IMP5, "*NOP", "IMP", 2),
    OpCode::new(DCP_ABSY, "*DCP", "ABSY", 7),
    OpCode::new(CMP_ABSX, "CMP", "ABSX", 4),
    OpCode::new(DEC_ABSX, "DEC", "ABSX", 7),
    OpCode::new(TOP_ABSX5, "*NOP", "ABSX", 4),
    OpCode::new(DCP_ABSX, "*DCP", "ABSX", 7),
    OpCode::new(CPX_IMM, "CPX", "IMM", 2),
    OpCode::new(SBC_INDX, "SBC", "INDX", 6),
    OpCode::new(DOP_IMM5, "*NOP", "IMM", 2),
    OpCode::new(ISB_INDX, "*ISB", "INDX", 8),
    OpCode::new(CPX_ZP, "CPX", "ZP", 3),
    OpCode::new(SBC_ZP, "SBC", "ZP", 3),
    OpCode::new(INC_ZP, "INC", "ZP", 5),
    OpCode::new(ISB_ZP, "*ISB", "ZP", 5),
    OpCode::new(INX, "INX", "IMP", 2),
    OpCode::new(SBC_IMM, "SBC", "IMM", 2),
    OpCode::new(NOP, "NOP", "IMP", 2),
    OpCode::new(SBC_IMM2, "*SBC", "IMM", 2),
    OpCode::new(CPX_ABS, "CPX", "ABS", 4),
    OpCode::new(SBC_ABS, "SBC", "ABS", 4),
    OpCode::new(INC_ABS, "INC", "ABS", 6),
    OpCode::new(ISB_ABS, "*ISB", "ABS", 6),
    OpCode::new(BEQ, "BEQ", "REL", 2),
    OpCode::new(SBC_INDY, "SBC", "INDY", 5),
    OpCode::new(KIL12, "KIL", "IMP", 2),
    OpCode::new(DOP_ZPX6, "*NOP", "ZPX", 4),
    OpCode::new(ISB_INDY, "*ISB", "INDY", 8),
    OpCode::new(SBC_ZPX, "SBC", "ZPX", 4),
    OpCode::new(INC_ZPX, "INC", "ZPX", 6),
    OpCode::new(ISB_ZPX, "*ISB", "ZPX", 6),
    OpCode::new(SED, "SED", "IMP", 2),
    OpCode::new(SBC_ABSY, "SBC", "ABSY", 4),
    OpCode::new(NOP_IMP6, "*NOP", "IMP", 2),
    OpCode::new(ISB_ABSY, "*ISB", "ABSY", 7),
    OpCode::new(SBC_ABSX, "SBC", "ABSX", 4),
    OpCode::new(INC_ABSX, "INC", "ABSX", 7),
    OpCode::new(TOP_ABSX6, "*NOP", "ABSX", 4),
    OpCode::new(ISB_ABSX, "*ISB", "ABSX", 7)
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
        let opcode = OpCode::new(0x69, "ADC", "IMM", 2);
        assert_eq!(opcode.code, 0x69);
        assert_eq!(opcode.mnemonic, "ADC");
        assert_eq!(opcode.mode, "IMM");
    }

    #[test]
    fn test_opcode_name() {
        let opcode = OpCode::new(0x69, "ADC", "IMM", 2);
        assert_eq!(opcode.name(), "ADC_IMM");
    }

    #[test]
    fn test_opcode_name_different_instruction() {
        let opcode = OpCode::new(0xA9, "LDA", "IMM", 2);
        assert_eq!(opcode.name(), "LDA_IMM");
    }

    #[test]
    fn test_opcode_equality() {
        let opcode1 = OpCode::new(0x69, "ADC", "IMM", 2);
        let opcode2 = OpCode::new(0x69, "ADC", "IMM", 2);
        assert_eq!(opcode1, opcode2);
    }

    #[test]
    fn test_opcode_inequality() {
        let opcode1 = OpCode::new(0x69, "ADC", "IMM", 2);
        let opcode2 = OpCode::new(0x6D, "ADC", "ABS", 4);
        assert_ne!(opcode1, opcode2);
    }

    #[test]
    fn test_opcodes_table_count() {
        assert_eq!(OPCODE_TABLE.len(), 256);
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

    #[test]
    fn test_bytes_imp_mode() {
        let opcode = OpCode::new(BRK, "BRK", "IMP", 7);
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_acc_mode() {
        let opcode = OpCode::new(ASL_A, "ASL", "ACC", 2);
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_imm_mode() {
        let opcode = OpCode::new(LDA_IMM, "LDA", "IMM", 2);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zp_mode() {
        let opcode = OpCode::new(LDA_ZP, "LDA", "ZP", 3);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpx_mode() {
        let opcode = OpCode::new(LDA_ZPX, "LDA", "ZPX", 4);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpy_mode() {
        let opcode = OpCode::new(LDX_ZPY, "LDX", "ZPY", 4);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indx_mode() {
        let opcode = OpCode::new(LDA_INDX, "LDA", "INDX", 6);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indy_mode() {
        let opcode = OpCode::new(LDA_INDY, "LDA", "INDY", 5);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_rel_mode() {
        let opcode = OpCode::new(BPL, "BPL", "REL", 2);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_abs_mode() {
        let opcode = OpCode::new(LDA_ABS, "LDA", "ABS", 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absx_mode() {
        let opcode = OpCode::new(LDA_ABSX, "LDA", "ABSX", 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absy_mode() {
        let opcode = OpCode::new(LDA_ABSY, "LDA", "ABSY", 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_ind_mode() {
        let opcode = OpCode::new(JMP_IND, "JMP", "IND", 5);
        assert_eq!(opcode.bytes(), 3);
    }
}
