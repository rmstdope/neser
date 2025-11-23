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
pub const AAX_INDX: u8 = 0x83;
pub const AAX_ZP: u8 = 0x87;
pub const AAX_ABS: u8 = 0x8F;
pub const AAX_ZPY: u8 = 0x97;
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
pub const ISC_INDX: u8 = 0xE3;
pub const ISC_ZP: u8 = 0xE7;
pub const ISC_ABS: u8 = 0xEF;
pub const ISC_INDY: u8 = 0xF3;
pub const ISC_ZPX: u8 = 0xF7;
pub const ISC_ABSY: u8 = 0xFB;
pub const ISC_ABSX: u8 = 0xFF;
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

// Complete NES 6502 opcode table
pub static OPCODE_TABLE: &[OpCode; 238] = &[
    OpCode::new(BRK, "BRK", "IMP"),
    OpCode::new(ORA_INDX, "ORA", "INDX"),
    OpCode::new(KIL, "KIL", "IMP"),
    OpCode::new(SLO_INDX, "SLO", "INDX"),
    OpCode::new(ORA_ZP, "ORA", "ZP"),
    OpCode::new(ASL_ZP, "ASL", "ZP"),
    OpCode::new(DOP_ZP, "*NOP", "ZP"),
    OpCode::new(SLO_ZP, "SLO", "ZP"),
    OpCode::new(PHP, "PHP", "IMP"),
    OpCode::new(ORA_IMM, "ORA", "IMM"),
    OpCode::new(ASL_A, "ASL", "ACC"),
    OpCode::new(AAC_IMM, "AAC", "IMM"),
    OpCode::new(ORA_ABS, "ORA", "ABS"),
    OpCode::new(ASL_ABS, "ASL", "ABS"),
    OpCode::new(SLO_ABS, "SLO", "ABS"),
    OpCode::new(BPL, "BPL", "REL"),
    OpCode::new(ORA_INDY, "ORA", "INDY"),
    OpCode::new(KIL2, "KIL", "IMP"),
    OpCode::new(SLO_INDY, "SLO", "INDY"),
    OpCode::new(ORA_ZPX, "ORA", "ZPX"),
    OpCode::new(ASL_ZPX, "ASL", "ZPX"),
    OpCode::new(CLC, "CLC", "IMP"),
    OpCode::new(DOP_ZPX, "*NOP", "ZPX"),
    OpCode::new(SLO_ZPX, "SLO", "ZPX"),
    OpCode::new(ORA_ABSY, "ORA", "ABSY"),
    OpCode::new(NOP_IMP, "*NOP", "IMP"),
    OpCode::new(SLO_ABSY, "SLO", "ABSY"),
    OpCode::new(ORA_ABSX, "ORA", "ABSX"),
    OpCode::new(ASL_ABSX, "ASL", "ABSX"),
    OpCode::new(SLO_ABSX, "SLO", "ABSX"),
    OpCode::new(JSR, "JSR", "ABS"),
    OpCode::new(AND_INDX, "AND", "INDX"),
    OpCode::new(KIL3, "KIL", "IMP"),
    OpCode::new(RLA_INDX, "RLA", "INDX"),
    OpCode::new(BIT_ZP, "BIT", "ZP"),
    OpCode::new(AND_ZP, "AND", "ZP"),
    OpCode::new(ROL_ZP, "ROL", "ZP"),
    OpCode::new(RLA_ZP, "RLA", "ZP"),
    OpCode::new(PLP, "PLP", "IMP"),
    OpCode::new(AND_IMM, "AND", "IMM"),
    OpCode::new(ROL_ACC, "ROL", "ACC"),
    OpCode::new(AAC_IMM2, "AAC", "IMM"),
    OpCode::new(BIT_ABS, "BIT", "ABS"),
    OpCode::new(AND_ABS, "AND", "ABS"),
    OpCode::new(ROL_ABS, "ROL", "ABS"),
    OpCode::new(RLA_ABS, "RLA", "ABS"),
    OpCode::new(BMI, "BMI", "REL"),
    OpCode::new(AND_INDY, "AND", "INDY"),
    OpCode::new(KIL4, "KIL", "IMP"),
    OpCode::new(RLA_INDY, "RLA", "INDY"),
    OpCode::new(AND_ZPX, "AND", "ZPX"),
    OpCode::new(ROL_ZPX, "ROL", "ZPX"),
    OpCode::new(RLA_ZPX, "RLA", "ZPX"),
    OpCode::new(SEC, "SEC", "IMP"),
    OpCode::new(DOP_ZPX2, "*NOP", "ZPX"),
    OpCode::new(AND_ABSY, "AND", "ABSY"),
    OpCode::new(NOP_IMP2, "*NOP", "IMP"),
    OpCode::new(RLA_ABSY, "RLA", "ABSY"),
    OpCode::new(AND_ABSX, "AND", "ABSX"),
    OpCode::new(ROL_ABSX, "ROL", "ABSX"),
    OpCode::new(RLA_ABSX, "RLA", "ABSX"),
    OpCode::new(RTI, "RTI", "IMP"),
    OpCode::new(EOR_INDX, "EOR", "INDX"),
    OpCode::new(KIL5, "KIL", "IMP"),
    OpCode::new(EOR_ZP, "EOR", "ZP"),
    OpCode::new(LSR_ZP, "LSR", "ZP"),
    OpCode::new(PHA, "PHA", "IMP"),
    OpCode::new(DOP_ZP2, "*NOP", "ZP"),
    OpCode::new(EOR_IMM, "EOR", "IMM"),
    OpCode::new(ASR_IMM, "ASR", "IMM"),
    OpCode::new(LSR_ACC, "LSR", "ACC"),
    OpCode::new(JMP_ABS, "JMP", "ABS"),
    OpCode::new(EOR_ABS, "EOR", "ABS"),
    OpCode::new(LSR_ABS, "LSR", "ABS"),
    OpCode::new(BVC, "BVC", "REL"),
    OpCode::new(EOR_INDY, "EOR", "INDY"),
    OpCode::new(KIL6, "KIL", "IMP"),
    OpCode::new(EOR_ZPX, "EOR", "ZPX"),
    OpCode::new(LSR_ZPX, "LSR", "ZPX"),
    OpCode::new(CLI, "CLI", "IMP"),
    OpCode::new(DOP_ZPX3, "*NOP", "ZPX"),
    OpCode::new(EOR_ABSY, "EOR", "ABSY"),
    OpCode::new(NOP_IMP3, "*NOP", "IMP"),
    OpCode::new(EOR_ABSX, "EOR", "ABSX"),
    OpCode::new(LSR_ABSX, "LSR", "ABSX"),
    OpCode::new(RTS, "RTS", "IMP"),
    OpCode::new(ADC_INDX, "ADC", "INDX"),
    OpCode::new(KIL7, "KIL", "IMP"),
    OpCode::new(RRA_INDX, "RRA", "INDX"),
    OpCode::new(ADC_ZP, "ADC", "ZP"),
    OpCode::new(ROR_ZP, "ROR", "ZP"),
    OpCode::new(PLA, "PLA", "IMP"),
    OpCode::new(DOP_ZP3, "*NOP", "ZP"),
    OpCode::new(RRA_ZP, "RRA", "ZP"),
    OpCode::new(ADC_IMM, "ADC", "IMM"),
    OpCode::new(ARR_IMM, "ARR", "IMM"),
    OpCode::new(ROR_ACC, "ROR", "ACC"),
    OpCode::new(JMP_IND, "JMP", "IND"),
    OpCode::new(ADC_ABS, "ADC", "ABS"),
    OpCode::new(ROR_ABS, "ROR", "ABS"),
    OpCode::new(RRA_ABS, "RRA", "ABS"),
    OpCode::new(BVS, "BVS", "REL"),
    OpCode::new(ADC_INDY, "ADC", "INDY"),
    OpCode::new(KIL8, "KIL", "IMP"),
    OpCode::new(RRA_INDY, "RRA", "INDY"),
    OpCode::new(ADC_ZPX, "ADC", "ZPX"),
    OpCode::new(ROR_ZPX, "ROR", "ZPX"),
    OpCode::new(SEI, "SEI", "IMP"),
    OpCode::new(DOP_ZPX4, "*NOP", "ZPX"),
    OpCode::new(RRA_ZPX, "RRA", "ZPX"),
    OpCode::new(ADC_ABSY, "ADC", "ABSY"),
    OpCode::new(NOP_IMP4, "*NOP", "IMP"),
    OpCode::new(RRA_ABSY, "RRA", "ABSY"),
    OpCode::new(ADC_ABSX, "ADC", "ABSX"),
    OpCode::new(ROR_ABSX, "ROR", "ABSX"),
    OpCode::new(DOP_IMM, "*NOP", "IMM"),
    OpCode::new(RRA_ABSX, "RRA", "ABSX"),
    OpCode::new(STA_INDX, "STA", "INDX"),
    OpCode::new(DOP_IMM2, "*NOP", "IMM"),
    OpCode::new(AAX_INDX, "AAX", "INDX"),
    OpCode::new(STY_ZP, "STY", "ZP"),
    OpCode::new(STA_ZP, "STA", "ZP"),
    OpCode::new(STX_ZP, "STX", "ZP"),
    OpCode::new(AAX_ZP, "AAX", "ZP"),
    OpCode::new(DEY, "DEY", "IMP"),
    OpCode::new(DOP_IMM3, "*NOP", "IMM"),
    OpCode::new(TXA, "TXA", "IMP"),
    OpCode::new(STY_ABS, "STY", "ABS"),
    OpCode::new(STA_ABS, "STA", "ABS"),
    OpCode::new(STX_ABS, "STX", "ABS"),
    OpCode::new(AAX_ABS, "AAX", "ABS"),
    OpCode::new(BCC, "BCC", "REL"),
    OpCode::new(STA_INDY, "STA", "INDY"),
    OpCode::new(KIL9, "KIL", "IMP"),
    OpCode::new(AXA_INDY, "AXA", "INDY"),
    OpCode::new(STY_ZPX, "STY", "ZPX"),
    OpCode::new(STA_ZPX, "STA", "ZPX"),
    OpCode::new(STX_ZPY, "STX", "ZPY"),
    OpCode::new(AAX_ZPY, "AAX", "ZPY"),
    OpCode::new(TYA, "TYA", "IMP"),
    OpCode::new(STA_ABSY, "STA", "ABSY"),
    OpCode::new(TXS, "TXS", "IMP"),
    OpCode::new(AXA_ABSY, "AXA", "ABSY"),
    OpCode::new(STA_ABSX, "STA", "ABSX"),
    OpCode::new(LDY_IMM, "LDY", "IMM"),
    OpCode::new(LDA_INDX, "LDA", "INDX"),
    OpCode::new(LDX_IMM, "LDX", "IMM"),
    OpCode::new(LAX_INDX, "LAX", "INDX"),
    OpCode::new(LDY_ZP, "LDY", "ZP"),
    OpCode::new(LDA_ZP, "LDA", "ZP"),
    OpCode::new(LDX_ZP, "LDX", "ZP"),
    OpCode::new(LAX_ZP, "LAX", "ZP"),
    OpCode::new(TAY, "TAY", "IMP"),
    OpCode::new(LDA_IMM, "LDA", "IMM"),
    OpCode::new(TAX, "TAX", "IMP"),
    OpCode::new(ATX_IMM, "ATX", "IMM"),
    OpCode::new(LDY_ABS, "LDY", "ABS"),
    OpCode::new(LDA_ABS, "LDA", "ABS"),
    OpCode::new(LDX_ABS, "LDX", "ABS"),
    OpCode::new(LAX_ABS, "LAX", "ABS"),
    OpCode::new(BCS, "BCS", "REL"),
    OpCode::new(LDA_INDY, "LDA", "INDY"),
    OpCode::new(KIL10, "KIL", "IMP"),
    OpCode::new(LAX_INDY, "LAX", "INDY"),
    OpCode::new(LDY_ZPX, "LDY", "ZPX"),
    OpCode::new(LDA_ZPX, "LDA", "ZPX"),
    OpCode::new(LDX_ZPY, "LDX", "ZPY"),
    OpCode::new(LAX_ZPY, "LAX", "ZPY"),
    OpCode::new(CLV, "CLV", "IMP"),
    OpCode::new(LDA_ABSY, "LDA", "ABSY"),
    OpCode::new(TSX, "TSX", "IMP"),
    OpCode::new(LAR_ABSY, "LAR", "ABSY"),
    OpCode::new(LDY_ABSX, "LDY", "ABSX"),
    OpCode::new(LDA_ABSX, "LDA", "ABSX"),
    OpCode::new(LDX_ABSY, "LDX", "ABSY"),
    OpCode::new(LAX_ABSY, "LAX", "ABSY"),
    OpCode::new(CPY_IMM, "CPY", "IMM"),
    OpCode::new(CMP_INDX, "CMP", "INDX"),
    OpCode::new(DOP_IMM4, "*NOP", "IMM"),
    OpCode::new(DCP_INDX, "DCP", "INDX"),
    OpCode::new(CPY_ZP, "CPY", "ZP"),
    OpCode::new(CMP_ZP, "CMP", "ZP"),
    OpCode::new(DEC_ZP, "DEC", "ZP"),
    OpCode::new(DCP_ZP, "DCP", "ZP"),
    OpCode::new(INY, "INY", "IMP"),
    OpCode::new(CMP_IMM, "CMP", "IMM"),
    OpCode::new(DEX, "DEX", "IMP"),
    OpCode::new(AXS_IMM, "AXS", "IMM"),
    OpCode::new(CPY_ABS, "CPY", "ABS"),
    OpCode::new(CMP_ABS, "CMP", "ABS"),
    OpCode::new(DEC_ABS, "DEC", "ABS"),
    OpCode::new(DCP_ABS, "DCP", "ABS"),
    OpCode::new(BNE, "BNE", "REL"),
    OpCode::new(CMP_INDY, "CMP", "INDY"),
    OpCode::new(KIL11, "KIL", "IMP"),
    OpCode::new(DCP_INDY, "DCP", "INDY"),
    OpCode::new(DOP_ZPX5, "*NOP", "ZPX"),
    OpCode::new(CMP_ZPX, "CMP", "ZPX"),
    OpCode::new(DEC_ZPX, "DEC", "ZPX"),
    OpCode::new(DCP_ZPX, "DCP", "ZPX"),
    OpCode::new(CLD, "CLD", "IMP"),
    OpCode::new(CMP_ABSY, "CMP", "ABSY"),
    OpCode::new(NOP_IMP5, "*NOP", "IMP"),
    OpCode::new(DCP_ABSY, "DCP", "ABSY"),
    OpCode::new(CMP_ABSX, "CMP", "ABSX"),
    OpCode::new(DEC_ABSX, "DEC", "ABSX"),
    OpCode::new(DCP_ABSX, "DCP", "ABSX"),
    OpCode::new(CPX_IMM, "CPX", "IMM"),
    OpCode::new(SBC_INDX, "SBC", "INDX"),
    OpCode::new(DOP_IMM5, "*NOP", "IMM"),
    OpCode::new(ISC_INDX, "ISC", "INDX"),
    OpCode::new(CPX_ZP, "CPX", "ZP"),
    OpCode::new(SBC_ZP, "SBC", "ZP"),
    OpCode::new(INC_ZP, "INC", "ZP"),
    OpCode::new(ISC_ZP, "ISC", "ZP"),
    OpCode::new(INX, "INX", "IMP"),
    OpCode::new(SBC_IMM, "SBC", "IMM"),
    OpCode::new(NOP, "NOP", "IMP"),
    OpCode::new(SBC_IMM2, "*SBC", "IMM"),
    OpCode::new(CPX_ABS, "CPX", "ABS"),
    OpCode::new(SBC_ABS, "SBC", "ABS"),
    OpCode::new(INC_ABS, "INC", "ABS"),
    OpCode::new(ISC_ABS, "ISC", "ABS"),
    OpCode::new(BEQ, "BEQ", "REL"),
    OpCode::new(SBC_INDY, "SBC", "INDY"),
    OpCode::new(KIL12, "KIL", "IMP"),
    OpCode::new(DOP_ZPX6, "*NOP", "ZPX"),
    OpCode::new(ISC_INDY, "ISC", "INDY"),
    OpCode::new(SBC_ZPX, "SBC", "ZPX"),
    OpCode::new(INC_ZPX, "INC", "ZPX"),
    OpCode::new(ISC_ZPX, "ISC", "ZPX"),
    OpCode::new(SED, "SED", "IMP"),
    OpCode::new(SBC_ABSY, "SBC", "ABSY"),
    OpCode::new(NOP_IMP6, "*NOP", "IMP"),
    OpCode::new(ISC_ABSY, "ISC", "ABSY"),
    OpCode::new(SBC_ABSX, "SBC", "ABSX"),
    OpCode::new(INC_ABSX, "INC", "ABSX"),
    OpCode::new(ISC_ABSX, "ISC", "ABSX"),
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
        assert_eq!(OPCODE_TABLE.len(), 238);
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
        let opcode = OpCode::new(BRK, "BRK", "IMP");
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_acc_mode() {
        let opcode = OpCode::new(ASL_A, "ASL", "ACC");
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_imm_mode() {
        let opcode = OpCode::new(LDA_IMM, "LDA", "IMM");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zp_mode() {
        let opcode = OpCode::new(LDA_ZP, "LDA", "ZP");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpx_mode() {
        let opcode = OpCode::new(LDA_ZPX, "LDA", "ZPX");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpy_mode() {
        let opcode = OpCode::new(LDX_ZPY, "LDX", "ZPY");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indx_mode() {
        let opcode = OpCode::new(LDA_INDX, "LDA", "INDX");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indy_mode() {
        let opcode = OpCode::new(LDA_INDY, "LDA", "INDY");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_rel_mode() {
        let opcode = OpCode::new(BPL, "BPL", "REL");
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_abs_mode() {
        let opcode = OpCode::new(LDA_ABS, "LDA", "ABS");
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absx_mode() {
        let opcode = OpCode::new(LDA_ABSX, "LDA", "ABSX");
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absy_mode() {
        let opcode = OpCode::new(LDA_ABSY, "LDA", "ABSY");
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_ind_mode() {
        let opcode = OpCode::new(JMP_IND, "JMP", "IND");
        assert_eq!(opcode.bytes(), 3);
    }
}
