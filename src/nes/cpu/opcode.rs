/// 6502 instruction mnemonic (official + undocumented).
/// Undocumented opcodes are prefixed with `U` instead of `*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(clippy::upper_case_acronyms)]
pub enum Mnemonic {
    ADC,
    AND,
    ASL,
    BCC,
    BCS,
    BEQ,
    BIT,
    BMI,
    BNE,
    BPL,
    BRK,
    BVC,
    BVS,
    CLC,
    CLD,
    CLI,
    CLV,
    CMP,
    CPX,
    CPY,
    DEC,
    DEX,
    DEY,
    EOR,
    HLT,
    INC,
    INX,
    INY,
    JMP,
    JSR,
    KIL,
    LDA,
    LDX,
    LDY,
    LSR,
    NOP,
    ORA,
    PHA,
    PHP,
    PLA,
    PLP,
    ROL,
    ROR,
    RTI,
    RTS,
    SBC,
    SEC,
    SED,
    SEI,
    STA,
    STX,
    STY,
    TAX,
    TAY,
    TSX,
    TXA,
    TXS,
    TYA,
    // Undocumented (prefix U = unofficial, was "*" in string form)
    UAAC,
    UARR,
    UASR,
    UATX,
    UAXA,
    UAXS,
    UDCP,
    UISB,
    ULAR,
    ULAX,
    UNOP,
    URLA,
    URRA,
    USAX,
    USBC,
    USLO,
    USRE,
    USXA,
    USYA,
    UXAA,
    UXAS,
}

impl std::fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::ADC => "ADC",
            Self::AND => "AND",
            Self::ASL => "ASL",
            Self::BCC => "BCC",
            Self::BCS => "BCS",
            Self::BEQ => "BEQ",
            Self::BIT => "BIT",
            Self::BMI => "BMI",
            Self::BNE => "BNE",
            Self::BPL => "BPL",
            Self::BRK => "BRK",
            Self::BVC => "BVC",
            Self::BVS => "BVS",
            Self::CLC => "CLC",
            Self::CLD => "CLD",
            Self::CLI => "CLI",
            Self::CLV => "CLV",
            Self::CMP => "CMP",
            Self::CPX => "CPX",
            Self::CPY => "CPY",
            Self::DEC => "DEC",
            Self::DEX => "DEX",
            Self::DEY => "DEY",
            Self::EOR => "EOR",
            Self::HLT => "HLT",
            Self::INC => "INC",
            Self::INX => "INX",
            Self::INY => "INY",
            Self::JMP => "JMP",
            Self::JSR => "JSR",
            Self::KIL => "KIL",
            Self::LDA => "LDA",
            Self::LDX => "LDX",
            Self::LDY => "LDY",
            Self::LSR => "LSR",
            Self::NOP => "NOP",
            Self::ORA => "ORA",
            Self::PHA => "PHA",
            Self::PHP => "PHP",
            Self::PLA => "PLA",
            Self::PLP => "PLP",
            Self::ROL => "ROL",
            Self::ROR => "ROR",
            Self::RTI => "RTI",
            Self::RTS => "RTS",
            Self::SBC => "SBC",
            Self::SEC => "SEC",
            Self::SED => "SED",
            Self::SEI => "SEI",
            Self::STA => "STA",
            Self::STX => "STX",
            Self::STY => "STY",
            Self::TAX => "TAX",
            Self::TAY => "TAY",
            Self::TSX => "TSX",
            Self::TXA => "TXA",
            Self::TXS => "TXS",
            Self::TYA => "TYA",
            Self::UAAC => "*AAC",
            Self::UARR => "*ARR",
            Self::UASR => "*ASR",
            Self::UATX => "*ATX",
            Self::UAXA => "*AXA",
            Self::UAXS => "*AXS",
            Self::UDCP => "*DCP",
            Self::UISB => "*ISB",
            Self::ULAR => "*LAR",
            Self::ULAX => "*LAX",
            Self::UNOP => "*NOP",
            Self::URLA => "*RLA",
            Self::URRA => "*RRA",
            Self::USAX => "*SAX",
            Self::USBC => "*SBC",
            Self::USLO => "*SLO",
            Self::USRE => "*SRE",
            Self::USXA => "*SXA",
            Self::USYA => "*SYA",
            Self::UXAA => "*XAA",
            Self::UXAS => "*XAS",
        };
        f.write_str(s)
    }
}

/// 6502 addressing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(clippy::upper_case_acronyms)]
pub enum AddrMode {
    IMP,
    ACC,
    IMM,
    ZP,
    ZPX,
    ZPY,
    ABS,
    ABSX,
    ABSXW,
    ABSY,
    ABSYW,
    IND,
    INDX,
    INDY,
    INDYW,
    REL,
}

impl std::fmt::Display for AddrMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::IMP => "IMP",
            Self::ACC => "ACC",
            Self::IMM => "IMM",
            Self::ZP => "ZP",
            Self::ZPX => "ZPX",
            Self::ZPY => "ZPY",
            Self::ABS => "ABS",
            Self::ABSX => "ABSX",
            Self::ABSXW => "ABSXW",
            Self::ABSY => "ABSY",
            Self::ABSYW => "ABSYW",
            Self::IND => "IND",
            Self::INDX => "INDX",
            Self::INDY => "INDY",
            Self::INDYW => "INDYW",
            Self::REL => "REL",
        };
        f.write_str(s)
    }
}

/// Represents a 6502 instruction opcode with its mnemonic and addressing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpCode {
    /// The opcode byte value
    pub code: u8,
    /// The instruction mnemonic (e.g., ADC, LDA)
    pub mnemonic: Mnemonic,
    /// The addressing mode (e.g., IMM, ABS, ZP)
    pub mode: AddrMode,
    /// The base number of cycles this instruction takes
    pub cycles: u8,
}

impl OpCode {
    /// Create a new OpCode
    pub const fn new(code: u8, mnemonic: Mnemonic, mode: AddrMode, cycles: u8) -> Self {
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
            AddrMode::IMP | AddrMode::ACC => 1,
            AddrMode::IMM
            | AddrMode::ZP
            | AddrMode::ZPX
            | AddrMode::ZPY
            | AddrMode::INDX
            | AddrMode::INDY
            | AddrMode::INDYW
            | AddrMode::REL => 2,
            AddrMode::ABS
            | AddrMode::ABSX
            | AddrMode::ABSXW
            | AddrMode::ABSY
            | AddrMode::ABSYW
            | AddrMode::IND => 3,
        }
    }
}

// Opcode constants for use in match patterns
pub const BRK: u8 = 0x00;
pub const ORA_INDX: u8 = 0x01;
pub const KIL: u8 = 0x02;
pub const SLO_INDX: u8 = 0x03;
pub const DOP_ZP: u8 = 0x04;
pub const ORA_ZP: u8 = 0x05;
pub const ASL_ZP: u8 = 0x06;
pub const SLO_ZP: u8 = 0x07;
pub const PHP: u8 = 0x08;
pub const ORA_IMM: u8 = 0x09;
pub const ASL_A: u8 = 0x0A;
pub const AAC_IMM: u8 = 0x0B;
pub const TOP_ABS: u8 = 0x0C;
pub const ORA_ABS: u8 = 0x0D;
pub const ASL_ABS: u8 = 0x0E;
pub const SLO_ABS: u8 = 0x0F;
pub const BPL: u8 = 0x10;
pub const ORA_INDY: u8 = 0x11;
pub const KIL2: u8 = 0x12;
pub const SLO_INDYW: u8 = 0x13;
pub const DOP_ZPX: u8 = 0x14;
pub const ORA_ZPX: u8 = 0x15;
pub const ASL_ZPX: u8 = 0x16;
pub const SLO_ZPX: u8 = 0x17;
pub const CLC: u8 = 0x18;
pub const ORA_ABSY: u8 = 0x19;
pub const NOP_IMP: u8 = 0x1A;
pub const SLO_ABSYW: u8 = 0x1B;
pub const TOP_ABSX: u8 = 0x1C;
pub const ORA_ABSX: u8 = 0x1D;
pub const ASL_ABSXW: u8 = 0x1E;
pub const SLO_ABSXW: u8 = 0x1F;
pub const JSR: u8 = 0x20;
pub const AND_INDX: u8 = 0x21;
pub const KIL3: u8 = 0x22;
pub const RLA_INDX: u8 = 0x23;
pub const BIT_ZP: u8 = 0x24;
pub const AND_ZP: u8 = 0x25;
pub const ROL_ZP: u8 = 0x26;
pub const RLA_ZP: u8 = 0x27;
pub const PLP: u8 = 0x28;
pub const AND_IMM: u8 = 0x29;
pub const ROL_ACC: u8 = 0x2A;
pub const AAC_IMM2: u8 = 0x2B;
pub const BIT_ABS: u8 = 0x2C;
pub const AND_ABS: u8 = 0x2D;
pub const ROL_ABS: u8 = 0x2E;
pub const RLA_ABS: u8 = 0x2F;
pub const BMI: u8 = 0x30;
pub const AND_INDY: u8 = 0x31;
pub const KIL4: u8 = 0x32;
pub const RLA_INDYW: u8 = 0x33;
pub const DOP_ZPX2: u8 = 0x34;
pub const AND_ZPX: u8 = 0x35;
pub const ROL_ZPX: u8 = 0x36;
pub const RLA_ZPX: u8 = 0x37;
pub const SEC: u8 = 0x38;
pub const AND_ABSY: u8 = 0x39;
pub const NOP_IMP2: u8 = 0x3A;
pub const RLA_ABSYW: u8 = 0x3B;
pub const TOP_ABSX2: u8 = 0x3C;
pub const AND_ABSX: u8 = 0x3D;
pub const ROL_ABSXW: u8 = 0x3E;
pub const RLA_ABSXW: u8 = 0x3F;
pub const RTI: u8 = 0x40;
pub const EOR_INDX: u8 = 0x41;
pub const KIL5: u8 = 0x42;
pub const SRE_INDX: u8 = 0x43;
pub const DOP_ZP2: u8 = 0x44;
pub const EOR_ZP: u8 = 0x45;
pub const LSR_ZP: u8 = 0x46;
pub const SRE_ZP: u8 = 0x47;
pub const PHA: u8 = 0x48;
pub const EOR_IMM: u8 = 0x49;
pub const LSR_ACC: u8 = 0x4A;
pub const ASR_IMM: u8 = 0x4B;
pub const JMP_ABS: u8 = 0x4C;
pub const EOR_ABS: u8 = 0x4D;
pub const LSR_ABS: u8 = 0x4E;
pub const SRE_ABS: u8 = 0x4F;
pub const BVC: u8 = 0x50;
pub const EOR_INDY: u8 = 0x51;
pub const KIL6: u8 = 0x52;
pub const SRE_INDYW: u8 = 0x53;
pub const DOP_ZPX3: u8 = 0x54;
pub const EOR_ZPX: u8 = 0x55;
pub const LSR_ZPX: u8 = 0x56;
pub const SRE_ZPX: u8 = 0x57;
pub const CLI: u8 = 0x58;
pub const EOR_ABSY: u8 = 0x59;
pub const NOP_IMP3: u8 = 0x5A;
pub const SRE_ABSYW: u8 = 0x5B;
pub const TOP_ABSX3: u8 = 0x5C;
pub const EOR_ABSX: u8 = 0x5D;
pub const LSR_ABSXW: u8 = 0x5E;
pub const SRE_ABSXW: u8 = 0x5F;
pub const RTS: u8 = 0x60;
pub const ADC_INDX: u8 = 0x61;
pub const KIL7: u8 = 0x62;
pub const RRA_INDX: u8 = 0x63;
pub const DOP_ZP3: u8 = 0x64;
pub const ADC_ZP: u8 = 0x65;
pub const ROR_ZP: u8 = 0x66;
pub const RRA_ZP: u8 = 0x67;
pub const PLA: u8 = 0x68;
pub const ADC_IMM: u8 = 0x69;
pub const ROR_ACC: u8 = 0x6A;
pub const ARR_IMM: u8 = 0x6B;
pub const JMP_IND: u8 = 0x6C;
pub const ADC_ABS: u8 = 0x6D;
pub const ROR_ABS: u8 = 0x6E;
pub const RRA_ABS: u8 = 0x6F;
pub const BVS: u8 = 0x70;
pub const ADC_INDY: u8 = 0x71;
pub const KIL8: u8 = 0x72;
pub const RRA_INDYW: u8 = 0x73;
pub const DOP_ZPX4: u8 = 0x74;
pub const ADC_ZPX: u8 = 0x75;
pub const ROR_ZPX: u8 = 0x76;
pub const RRA_ZPX: u8 = 0x77;
pub const SEI: u8 = 0x78;
pub const ADC_ABSY: u8 = 0x79;
pub const NOP_IMP4: u8 = 0x7A;
pub const RRA_ABSYW: u8 = 0x7B;
pub const TOP_ABSX4: u8 = 0x7C;
pub const ADC_ABSX: u8 = 0x7D;
pub const ROR_ABSXW: u8 = 0x7E;
pub const RRA_ABSXW: u8 = 0x7F;
pub const DOP_IMM: u8 = 0x80;
pub const STA_INDX: u8 = 0x81;
pub const DOP_IMM2: u8 = 0x82;
pub const SAX_INDX: u8 = 0x83;
pub const STY_ZP: u8 = 0x84;
pub const STA_ZP: u8 = 0x85;
pub const STX_ZP: u8 = 0x86;
pub const SAX_ZP: u8 = 0x87;
pub const DEY: u8 = 0x88;
pub const DOP_IMM3: u8 = 0x89;
pub const TXA: u8 = 0x8A;
pub const XAA_IMM: u8 = 0x8B;
pub const STY_ABS: u8 = 0x8C;
pub const STA_ABS: u8 = 0x8D;
pub const STX_ABS: u8 = 0x8E;
pub const SAX_ABS: u8 = 0x8F;
pub const BCC: u8 = 0x90;
pub const STA_INDYW: u8 = 0x91;
pub const KIL9: u8 = 0x92;
pub const AXA_INDY: u8 = 0x93;
pub const STY_ZPX: u8 = 0x94;
pub const STA_ZPX: u8 = 0x95;
pub const STX_ZPY: u8 = 0x96;
pub const SAX_ZPY: u8 = 0x97;
pub const TYA: u8 = 0x98;
pub const STA_ABSYW: u8 = 0x99;
pub const TXS: u8 = 0x9A;
pub const XAS_ABSY: u8 = 0x9B;
pub const SYA_ABSX: u8 = 0x9C;
pub const STA_ABSXW: u8 = 0x9D;
pub const SXA_ABSY: u8 = 0x9E;
pub const AXA_ABSY: u8 = 0x9F;
pub const LDY_IMM: u8 = 0xA0;
pub const LDA_INDX: u8 = 0xA1;
pub const LDX_IMM: u8 = 0xA2;
pub const LAX_INDX: u8 = 0xA3;
pub const LDY_ZP: u8 = 0xA4;
pub const LDA_ZP: u8 = 0xA5;
pub const LDX_ZP: u8 = 0xA6;
pub const LAX_ZP: u8 = 0xA7;
pub const TAY: u8 = 0xA8;
pub const LDA_IMM: u8 = 0xA9;
pub const TAX: u8 = 0xAA;
pub const ATX_IMM: u8 = 0xAB;
pub const LDY_ABS: u8 = 0xAC;
pub const LDA_ABS: u8 = 0xAD;
pub const LDX_ABS: u8 = 0xAE;
pub const LAX_ABS: u8 = 0xAF;
pub const BCS: u8 = 0xB0;
pub const LDA_INDY: u8 = 0xB1;
pub const KIL10: u8 = 0xB2;
pub const LAX_INDY: u8 = 0xB3;
pub const LDY_ZPX: u8 = 0xB4;
pub const LDA_ZPX: u8 = 0xB5;
pub const LDX_ZPY: u8 = 0xB6;
pub const LAX_ZPY: u8 = 0xB7;
pub const CLV: u8 = 0xB8;
pub const LDA_ABSY: u8 = 0xB9;
pub const TSX: u8 = 0xBA;
pub const LAR_ABSY: u8 = 0xBB;
pub const LDY_ABSX: u8 = 0xBC;
pub const LDA_ABSX: u8 = 0xBD;
pub const LDX_ABSY: u8 = 0xBE;
pub const LAX_ABSY: u8 = 0xBF;
pub const CPY_IMM: u8 = 0xC0;
pub const CMP_INDX: u8 = 0xC1;
pub const DOP_IMM4: u8 = 0xC2;
pub const DCP_INDX: u8 = 0xC3;
pub const CPY_ZP: u8 = 0xC4;
pub const CMP_ZP: u8 = 0xC5;
pub const DEC_ZP: u8 = 0xC6;
pub const DCP_ZP: u8 = 0xC7;
pub const INY: u8 = 0xC8;
pub const CMP_IMM: u8 = 0xC9;
pub const DEX: u8 = 0xCA;
pub const AXS_IMM: u8 = 0xCB;
pub const CPY_ABS: u8 = 0xCC;
pub const CMP_ABS: u8 = 0xCD;
pub const DEC_ABS: u8 = 0xCE;
pub const DCP_ABS: u8 = 0xCF;
pub const BNE: u8 = 0xD0;
pub const CMP_INDY: u8 = 0xD1;
pub const KIL11: u8 = 0xD2;
pub const DCP_INDYW: u8 = 0xD3;
pub const DOP_ZPX5: u8 = 0xD4;
pub const CMP_ZPX: u8 = 0xD5;
pub const DEC_ZPX: u8 = 0xD6;
pub const DCP_ZPX: u8 = 0xD7;
pub const CLD: u8 = 0xD8;
pub const CMP_ABSY: u8 = 0xD9;
pub const NOP_IMP5: u8 = 0xDA;
pub const DCP_ABSYW: u8 = 0xDB;
pub const TOP_ABSX5: u8 = 0xDC;
pub const CMP_ABSX: u8 = 0xDD;
pub const DEC_ABSXW: u8 = 0xDE;
pub const DCP_ABSXW: u8 = 0xDF;
pub const CPX_IMM: u8 = 0xE0;
pub const SBC_INDX: u8 = 0xE1;
pub const DOP_IMM5: u8 = 0xE2;
pub const ISB_INDX: u8 = 0xE3;
pub const CPX_ZP: u8 = 0xE4;
pub const SBC_ZP: u8 = 0xE5;
pub const INC_ZP: u8 = 0xE6;
pub const ISB_ZP: u8 = 0xE7;
pub const INX: u8 = 0xE8;
pub const SBC_IMM: u8 = 0xE9;
pub const NOP: u8 = 0xEA;
pub const SBC_IMM2: u8 = 0xEB;
pub const CPX_ABS: u8 = 0xEC;
pub const SBC_ABS: u8 = 0xED;
pub const INC_ABS: u8 = 0xEE;
pub const ISB_ABS: u8 = 0xEF;
pub const BEQ: u8 = 0xF0;
pub const SBC_INDY: u8 = 0xF1;
pub const KIL12: u8 = 0xF2;
pub const ISB_INDYW: u8 = 0xF3;
pub const DOP_ZPX6: u8 = 0xF4;
pub const SBC_ZPX: u8 = 0xF5;
pub const INC_ZPX: u8 = 0xF6;
pub const ISB_ZPX: u8 = 0xF7;
pub const SED: u8 = 0xF8;
pub const SBC_ABSY: u8 = 0xF9;
pub const NOP_IMP6: u8 = 0xFA;
pub const ISB_ABSYW: u8 = 0xFB;
pub const TOP_ABSX6: u8 = 0xFC;
pub const SBC_ABSX: u8 = 0xFD;
pub const INC_ABSXW: u8 = 0xFE;
pub const ISB_ABSXW: u8 = 0xFF;

// Complete NES 6502 opcode table
pub static OPCODE_TABLE: &[OpCode; 256] = &[
    OpCode::new(BRK, Mnemonic::BRK, AddrMode::IMP, 7),
    OpCode::new(ORA_INDX, Mnemonic::ORA, AddrMode::INDX, 6),
    OpCode::new(KIL, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(SLO_INDX, Mnemonic::USLO, AddrMode::INDX, 8),
    OpCode::new(DOP_ZP, Mnemonic::UNOP, AddrMode::ZP, 3),
    OpCode::new(ORA_ZP, Mnemonic::ORA, AddrMode::ZP, 3),
    OpCode::new(ASL_ZP, Mnemonic::ASL, AddrMode::ZP, 5),
    OpCode::new(SLO_ZP, Mnemonic::USLO, AddrMode::ZP, 5),
    OpCode::new(PHP, Mnemonic::PHP, AddrMode::IMP, 3),
    OpCode::new(ORA_IMM, Mnemonic::ORA, AddrMode::IMM, 2),
    OpCode::new(ASL_A, Mnemonic::ASL, AddrMode::ACC, 2),
    OpCode::new(AAC_IMM, Mnemonic::UAAC, AddrMode::IMM, 2),
    OpCode::new(TOP_ABS, Mnemonic::UNOP, AddrMode::ABS, 4),
    OpCode::new(ORA_ABS, Mnemonic::ORA, AddrMode::ABS, 4),
    OpCode::new(ASL_ABS, Mnemonic::ASL, AddrMode::ABS, 6),
    OpCode::new(SLO_ABS, Mnemonic::USLO, AddrMode::ABS, 6),
    OpCode::new(BPL, Mnemonic::BPL, AddrMode::REL, 2),
    OpCode::new(ORA_INDY, Mnemonic::ORA, AddrMode::INDY, 5),
    OpCode::new(KIL2, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(SLO_INDYW, Mnemonic::USLO, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(ORA_ZPX, Mnemonic::ORA, AddrMode::ZPX, 4),
    OpCode::new(ASL_ZPX, Mnemonic::ASL, AddrMode::ZPX, 6),
    OpCode::new(SLO_ZPX, Mnemonic::USLO, AddrMode::ZPX, 6),
    OpCode::new(CLC, Mnemonic::CLC, AddrMode::IMP, 2),
    OpCode::new(ORA_ABSY, Mnemonic::ORA, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(SLO_ABSYW, Mnemonic::USLO, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(ORA_ABSX, Mnemonic::ORA, AddrMode::ABSX, 4),
    OpCode::new(ASL_ABSXW, Mnemonic::ASL, AddrMode::ABSXW, 7),
    OpCode::new(SLO_ABSXW, Mnemonic::USLO, AddrMode::ABSXW, 7),
    OpCode::new(JSR, Mnemonic::JSR, AddrMode::ABS, 6),
    OpCode::new(AND_INDX, Mnemonic::AND, AddrMode::INDX, 6),
    OpCode::new(KIL3, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(RLA_INDX, Mnemonic::URLA, AddrMode::INDX, 8),
    OpCode::new(BIT_ZP, Mnemonic::BIT, AddrMode::ZP, 3),
    OpCode::new(AND_ZP, Mnemonic::AND, AddrMode::ZP, 3),
    OpCode::new(ROL_ZP, Mnemonic::ROL, AddrMode::ZP, 5),
    OpCode::new(RLA_ZP, Mnemonic::URLA, AddrMode::ZP, 5),
    OpCode::new(PLP, Mnemonic::PLP, AddrMode::IMP, 4),
    OpCode::new(AND_IMM, Mnemonic::AND, AddrMode::IMM, 2),
    OpCode::new(ROL_ACC, Mnemonic::ROL, AddrMode::ACC, 2),
    OpCode::new(AAC_IMM2, Mnemonic::UAAC, AddrMode::IMM, 2),
    OpCode::new(BIT_ABS, Mnemonic::BIT, AddrMode::ABS, 4),
    OpCode::new(AND_ABS, Mnemonic::AND, AddrMode::ABS, 4),
    OpCode::new(ROL_ABS, Mnemonic::ROL, AddrMode::ABS, 6),
    OpCode::new(RLA_ABS, Mnemonic::URLA, AddrMode::ABS, 6),
    OpCode::new(BMI, Mnemonic::BMI, AddrMode::REL, 2),
    OpCode::new(AND_INDY, Mnemonic::AND, AddrMode::INDY, 5),
    OpCode::new(KIL4, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(RLA_INDYW, Mnemonic::URLA, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX2, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(AND_ZPX, Mnemonic::AND, AddrMode::ZPX, 4),
    OpCode::new(ROL_ZPX, Mnemonic::ROL, AddrMode::ZPX, 6),
    OpCode::new(RLA_ZPX, Mnemonic::URLA, AddrMode::ZPX, 6),
    OpCode::new(SEC, Mnemonic::SEC, AddrMode::IMP, 2),
    OpCode::new(AND_ABSY, Mnemonic::AND, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP2, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(RLA_ABSYW, Mnemonic::URLA, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX2, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(AND_ABSX, Mnemonic::AND, AddrMode::ABSX, 4),
    OpCode::new(ROL_ABSXW, Mnemonic::ROL, AddrMode::ABSXW, 7),
    OpCode::new(RLA_ABSXW, Mnemonic::URLA, AddrMode::ABSXW, 7),
    OpCode::new(RTI, Mnemonic::RTI, AddrMode::IMP, 6),
    OpCode::new(EOR_INDX, Mnemonic::EOR, AddrMode::INDX, 6),
    OpCode::new(KIL5, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(SRE_INDX, Mnemonic::USRE, AddrMode::INDX, 8),
    OpCode::new(DOP_ZP2, Mnemonic::UNOP, AddrMode::ZP, 3),
    OpCode::new(EOR_ZP, Mnemonic::EOR, AddrMode::ZP, 3),
    OpCode::new(LSR_ZP, Mnemonic::LSR, AddrMode::ZP, 5),
    OpCode::new(SRE_ZP, Mnemonic::USRE, AddrMode::ZP, 5),
    OpCode::new(PHA, Mnemonic::PHA, AddrMode::IMP, 3),
    OpCode::new(EOR_IMM, Mnemonic::EOR, AddrMode::IMM, 2),
    OpCode::new(LSR_ACC, Mnemonic::LSR, AddrMode::ACC, 2),
    OpCode::new(ASR_IMM, Mnemonic::UASR, AddrMode::IMM, 2),
    OpCode::new(JMP_ABS, Mnemonic::JMP, AddrMode::ABS, 3),
    OpCode::new(EOR_ABS, Mnemonic::EOR, AddrMode::ABS, 4),
    OpCode::new(LSR_ABS, Mnemonic::LSR, AddrMode::ABS, 6),
    OpCode::new(SRE_ABS, Mnemonic::USRE, AddrMode::ABS, 6),
    OpCode::new(BVC, Mnemonic::BVC, AddrMode::REL, 2),
    OpCode::new(EOR_INDY, Mnemonic::EOR, AddrMode::INDY, 5),
    OpCode::new(KIL6, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(SRE_INDYW, Mnemonic::USRE, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX3, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(EOR_ZPX, Mnemonic::EOR, AddrMode::ZPX, 4),
    OpCode::new(LSR_ZPX, Mnemonic::LSR, AddrMode::ZPX, 6),
    OpCode::new(SRE_ZPX, Mnemonic::USRE, AddrMode::ZPX, 6),
    OpCode::new(CLI, Mnemonic::CLI, AddrMode::IMP, 2),
    OpCode::new(EOR_ABSY, Mnemonic::EOR, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP3, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(SRE_ABSYW, Mnemonic::USRE, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX3, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(EOR_ABSX, Mnemonic::EOR, AddrMode::ABSX, 4),
    OpCode::new(LSR_ABSXW, Mnemonic::LSR, AddrMode::ABSXW, 7),
    OpCode::new(SRE_ABSXW, Mnemonic::USRE, AddrMode::ABSXW, 7),
    OpCode::new(RTS, Mnemonic::RTS, AddrMode::IMP, 6),
    OpCode::new(ADC_INDX, Mnemonic::ADC, AddrMode::INDX, 6),
    OpCode::new(KIL7, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(RRA_INDX, Mnemonic::URRA, AddrMode::INDX, 8),
    OpCode::new(DOP_ZP3, Mnemonic::UNOP, AddrMode::ZP, 3),
    OpCode::new(ADC_ZP, Mnemonic::ADC, AddrMode::ZP, 3),
    OpCode::new(ROR_ZP, Mnemonic::ROR, AddrMode::ZP, 5),
    OpCode::new(RRA_ZP, Mnemonic::URRA, AddrMode::ZP, 5),
    OpCode::new(PLA, Mnemonic::PLA, AddrMode::IMP, 4),
    OpCode::new(ADC_IMM, Mnemonic::ADC, AddrMode::IMM, 2),
    OpCode::new(ROR_ACC, Mnemonic::ROR, AddrMode::ACC, 2),
    OpCode::new(ARR_IMM, Mnemonic::UARR, AddrMode::IMM, 2),
    OpCode::new(JMP_IND, Mnemonic::JMP, AddrMode::IND, 5),
    OpCode::new(ADC_ABS, Mnemonic::ADC, AddrMode::ABS, 4),
    OpCode::new(ROR_ABS, Mnemonic::ROR, AddrMode::ABS, 6),
    OpCode::new(RRA_ABS, Mnemonic::URRA, AddrMode::ABS, 6),
    OpCode::new(BVS, Mnemonic::BVS, AddrMode::REL, 2),
    OpCode::new(ADC_INDY, Mnemonic::ADC, AddrMode::INDY, 5),
    OpCode::new(KIL8, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(RRA_INDYW, Mnemonic::URRA, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX4, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(ADC_ZPX, Mnemonic::ADC, AddrMode::ZPX, 4),
    OpCode::new(ROR_ZPX, Mnemonic::ROR, AddrMode::ZPX, 6),
    OpCode::new(RRA_ZPX, Mnemonic::URRA, AddrMode::ZPX, 6),
    OpCode::new(SEI, Mnemonic::SEI, AddrMode::IMP, 2),
    OpCode::new(ADC_ABSY, Mnemonic::ADC, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP4, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(RRA_ABSYW, Mnemonic::URRA, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX4, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(ADC_ABSX, Mnemonic::ADC, AddrMode::ABSX, 4),
    OpCode::new(ROR_ABSXW, Mnemonic::ROR, AddrMode::ABSXW, 7),
    OpCode::new(RRA_ABSXW, Mnemonic::URRA, AddrMode::ABSXW, 7),
    OpCode::new(DOP_IMM, Mnemonic::UNOP, AddrMode::IMM, 2),
    OpCode::new(STA_INDX, Mnemonic::STA, AddrMode::INDX, 6),
    OpCode::new(DOP_IMM2, Mnemonic::UNOP, AddrMode::IMM, 2),
    OpCode::new(SAX_INDX, Mnemonic::USAX, AddrMode::INDX, 6),
    OpCode::new(STY_ZP, Mnemonic::STY, AddrMode::ZP, 3),
    OpCode::new(STA_ZP, Mnemonic::STA, AddrMode::ZP, 3),
    OpCode::new(STX_ZP, Mnemonic::STX, AddrMode::ZP, 3),
    OpCode::new(SAX_ZP, Mnemonic::USAX, AddrMode::ZP, 3),
    OpCode::new(DEY, Mnemonic::DEY, AddrMode::IMP, 2),
    OpCode::new(DOP_IMM3, Mnemonic::UNOP, AddrMode::IMM, 2),
    OpCode::new(TXA, Mnemonic::TXA, AddrMode::IMP, 2),
    OpCode::new(XAA_IMM, Mnemonic::UXAA, AddrMode::IMM, 2),
    OpCode::new(STY_ABS, Mnemonic::STY, AddrMode::ABS, 4),
    OpCode::new(STA_ABS, Mnemonic::STA, AddrMode::ABS, 4),
    OpCode::new(STX_ABS, Mnemonic::STX, AddrMode::ABS, 4),
    OpCode::new(SAX_ABS, Mnemonic::USAX, AddrMode::ABS, 4),
    OpCode::new(BCC, Mnemonic::BCC, AddrMode::REL, 2),
    OpCode::new(STA_INDYW, Mnemonic::STA, AddrMode::INDYW, 6),
    OpCode::new(KIL9, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(AXA_INDY, Mnemonic::UAXA, AddrMode::INDYW, 6),
    OpCode::new(STY_ZPX, Mnemonic::STY, AddrMode::ZPX, 4),
    OpCode::new(STA_ZPX, Mnemonic::STA, AddrMode::ZPX, 4),
    OpCode::new(STX_ZPY, Mnemonic::STX, AddrMode::ZPY, 4),
    OpCode::new(SAX_ZPY, Mnemonic::USAX, AddrMode::ZPY, 4),
    OpCode::new(TYA, Mnemonic::TYA, AddrMode::IMP, 2),
    OpCode::new(STA_ABSYW, Mnemonic::STA, AddrMode::ABSYW, 5),
    OpCode::new(TXS, Mnemonic::TXS, AddrMode::IMP, 2),
    OpCode::new(XAS_ABSY, Mnemonic::UXAS, AddrMode::ABSYW, 5),
    OpCode::new(SYA_ABSX, Mnemonic::USYA, AddrMode::ABSXW, 5),
    OpCode::new(STA_ABSXW, Mnemonic::STA, AddrMode::ABSXW, 5),
    OpCode::new(SXA_ABSY, Mnemonic::USXA, AddrMode::ABSYW, 5),
    OpCode::new(AXA_ABSY, Mnemonic::UAXA, AddrMode::ABSYW, 5),
    OpCode::new(LDY_IMM, Mnemonic::LDY, AddrMode::IMM, 2),
    OpCode::new(LDA_INDX, Mnemonic::LDA, AddrMode::INDX, 6),
    OpCode::new(LDX_IMM, Mnemonic::LDX, AddrMode::IMM, 2),
    OpCode::new(LAX_INDX, Mnemonic::ULAX, AddrMode::INDX, 6),
    OpCode::new(LDY_ZP, Mnemonic::LDY, AddrMode::ZP, 3),
    OpCode::new(LDA_ZP, Mnemonic::LDA, AddrMode::ZP, 3),
    OpCode::new(LDX_ZP, Mnemonic::LDX, AddrMode::ZP, 3),
    OpCode::new(LAX_ZP, Mnemonic::ULAX, AddrMode::ZP, 3),
    OpCode::new(TAY, Mnemonic::TAY, AddrMode::IMP, 2),
    OpCode::new(LDA_IMM, Mnemonic::LDA, AddrMode::IMM, 2),
    OpCode::new(TAX, Mnemonic::TAX, AddrMode::IMP, 2),
    OpCode::new(ATX_IMM, Mnemonic::UATX, AddrMode::IMM, 2),
    OpCode::new(LDY_ABS, Mnemonic::LDY, AddrMode::ABS, 4),
    OpCode::new(LDA_ABS, Mnemonic::LDA, AddrMode::ABS, 4),
    OpCode::new(LDX_ABS, Mnemonic::LDX, AddrMode::ABS, 4),
    OpCode::new(LAX_ABS, Mnemonic::ULAX, AddrMode::ABS, 4),
    OpCode::new(BCS, Mnemonic::BCS, AddrMode::REL, 2),
    OpCode::new(LDA_INDY, Mnemonic::LDA, AddrMode::INDY, 5),
    OpCode::new(KIL10, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(LAX_INDY, Mnemonic::ULAX, AddrMode::INDY, 5),
    OpCode::new(LDY_ZPX, Mnemonic::LDY, AddrMode::ZPX, 4),
    OpCode::new(LDA_ZPX, Mnemonic::LDA, AddrMode::ZPX, 4),
    OpCode::new(LDX_ZPY, Mnemonic::LDX, AddrMode::ZPY, 4),
    OpCode::new(LAX_ZPY, Mnemonic::ULAX, AddrMode::ZPY, 4),
    OpCode::new(CLV, Mnemonic::CLV, AddrMode::IMP, 2),
    OpCode::new(LDA_ABSY, Mnemonic::LDA, AddrMode::ABSY, 4),
    OpCode::new(TSX, Mnemonic::TSX, AddrMode::IMP, 2),
    OpCode::new(LAR_ABSY, Mnemonic::ULAR, AddrMode::ABSY, 4),
    OpCode::new(LDY_ABSX, Mnemonic::LDY, AddrMode::ABSX, 4),
    OpCode::new(LDA_ABSX, Mnemonic::LDA, AddrMode::ABSX, 4),
    OpCode::new(LDX_ABSY, Mnemonic::LDX, AddrMode::ABSY, 4),
    OpCode::new(LAX_ABSY, Mnemonic::ULAX, AddrMode::ABSY, 4),
    OpCode::new(CPY_IMM, Mnemonic::CPY, AddrMode::IMM, 2),
    OpCode::new(CMP_INDX, Mnemonic::CMP, AddrMode::INDX, 6),
    OpCode::new(DOP_IMM4, Mnemonic::UNOP, AddrMode::IMM, 2),
    OpCode::new(DCP_INDX, Mnemonic::UDCP, AddrMode::INDX, 8),
    OpCode::new(CPY_ZP, Mnemonic::CPY, AddrMode::ZP, 3),
    OpCode::new(CMP_ZP, Mnemonic::CMP, AddrMode::ZP, 3),
    OpCode::new(DEC_ZP, Mnemonic::DEC, AddrMode::ZP, 5),
    OpCode::new(DCP_ZP, Mnemonic::UDCP, AddrMode::ZP, 5),
    OpCode::new(INY, Mnemonic::INY, AddrMode::IMP, 2),
    OpCode::new(CMP_IMM, Mnemonic::CMP, AddrMode::IMM, 2),
    OpCode::new(DEX, Mnemonic::DEX, AddrMode::IMP, 2),
    OpCode::new(AXS_IMM, Mnemonic::UAXS, AddrMode::IMM, 2),
    OpCode::new(CPY_ABS, Mnemonic::CPY, AddrMode::ABS, 4),
    OpCode::new(CMP_ABS, Mnemonic::CMP, AddrMode::ABS, 4),
    OpCode::new(DEC_ABS, Mnemonic::DEC, AddrMode::ABS, 6),
    OpCode::new(DCP_ABS, Mnemonic::UDCP, AddrMode::ABS, 6),
    OpCode::new(BNE, Mnemonic::BNE, AddrMode::REL, 2),
    OpCode::new(CMP_INDY, Mnemonic::CMP, AddrMode::INDY, 5),
    OpCode::new(KIL11, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(DCP_INDYW, Mnemonic::UDCP, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX5, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(CMP_ZPX, Mnemonic::CMP, AddrMode::ZPX, 4),
    OpCode::new(DEC_ZPX, Mnemonic::DEC, AddrMode::ZPX, 6),
    OpCode::new(DCP_ZPX, Mnemonic::UDCP, AddrMode::ZPX, 6),
    OpCode::new(CLD, Mnemonic::CLD, AddrMode::IMP, 2),
    OpCode::new(CMP_ABSY, Mnemonic::CMP, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP5, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(DCP_ABSYW, Mnemonic::UDCP, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX5, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(CMP_ABSX, Mnemonic::CMP, AddrMode::ABSX, 4),
    OpCode::new(DEC_ABSXW, Mnemonic::DEC, AddrMode::ABSXW, 7),
    OpCode::new(DCP_ABSXW, Mnemonic::UDCP, AddrMode::ABSXW, 7),
    OpCode::new(CPX_IMM, Mnemonic::CPX, AddrMode::IMM, 2),
    OpCode::new(SBC_INDX, Mnemonic::SBC, AddrMode::INDX, 6),
    OpCode::new(DOP_IMM5, Mnemonic::UNOP, AddrMode::IMM, 2),
    OpCode::new(ISB_INDX, Mnemonic::UISB, AddrMode::INDX, 8),
    OpCode::new(CPX_ZP, Mnemonic::CPX, AddrMode::ZP, 3),
    OpCode::new(SBC_ZP, Mnemonic::SBC, AddrMode::ZP, 3),
    OpCode::new(INC_ZP, Mnemonic::INC, AddrMode::ZP, 5),
    OpCode::new(ISB_ZP, Mnemonic::UISB, AddrMode::ZP, 5),
    OpCode::new(INX, Mnemonic::INX, AddrMode::IMP, 2),
    OpCode::new(SBC_IMM, Mnemonic::SBC, AddrMode::IMM, 2),
    OpCode::new(NOP, Mnemonic::NOP, AddrMode::IMP, 2),
    OpCode::new(SBC_IMM2, Mnemonic::USBC, AddrMode::IMM, 2),
    OpCode::new(CPX_ABS, Mnemonic::CPX, AddrMode::ABS, 4),
    OpCode::new(SBC_ABS, Mnemonic::SBC, AddrMode::ABS, 4),
    OpCode::new(INC_ABS, Mnemonic::INC, AddrMode::ABS, 6),
    OpCode::new(ISB_ABS, Mnemonic::UISB, AddrMode::ABS, 6),
    OpCode::new(BEQ, Mnemonic::BEQ, AddrMode::REL, 2),
    OpCode::new(SBC_INDY, Mnemonic::SBC, AddrMode::INDY, 5),
    OpCode::new(KIL12, Mnemonic::KIL, AddrMode::IMP, 2),
    OpCode::new(ISB_INDYW, Mnemonic::UISB, AddrMode::INDYW, 8),
    OpCode::new(DOP_ZPX6, Mnemonic::UNOP, AddrMode::ZPX, 4),
    OpCode::new(SBC_ZPX, Mnemonic::SBC, AddrMode::ZPX, 4),
    OpCode::new(INC_ZPX, Mnemonic::INC, AddrMode::ZPX, 6),
    OpCode::new(ISB_ZPX, Mnemonic::UISB, AddrMode::ZPX, 6),
    OpCode::new(SED, Mnemonic::SED, AddrMode::IMP, 2),
    OpCode::new(SBC_ABSY, Mnemonic::SBC, AddrMode::ABSY, 4),
    OpCode::new(NOP_IMP6, Mnemonic::UNOP, AddrMode::IMP, 2),
    OpCode::new(ISB_ABSYW, Mnemonic::UISB, AddrMode::ABSYW, 7),
    OpCode::new(TOP_ABSX6, Mnemonic::UNOP, AddrMode::ABSX, 4),
    OpCode::new(SBC_ABSX, Mnemonic::SBC, AddrMode::ABSX, 4),
    OpCode::new(INC_ABSXW, Mnemonic::INC, AddrMode::ABSXW, 7),
    OpCode::new(ISB_ABSXW, Mnemonic::UISB, AddrMode::ABSXW, 7),
];

/// Lookup an opcode by its byte value
pub fn lookup(code: u8) -> &'static OpCode {
    &OPCODE_TABLE[code as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_creation() {
        let opcode = OpCode::new(0x69, Mnemonic::ADC, AddrMode::IMM, 2);
        assert_eq!(opcode.code, 0x69);
        assert_eq!(opcode.mnemonic, Mnemonic::ADC);
        assert_eq!(opcode.mode, AddrMode::IMM);
    }

    #[test]
    fn test_opcode_name() {
        let opcode = OpCode::new(0x69, Mnemonic::ADC, AddrMode::IMM, 2);
        assert_eq!(opcode.name(), "ADC_IMM");
    }

    #[test]
    fn test_opcode_name_different_instruction() {
        let opcode = OpCode::new(0xA9, Mnemonic::LDA, AddrMode::IMM, 2);
        assert_eq!(opcode.name(), "LDA_IMM");
    }

    #[test]
    fn test_opcode_equality() {
        let opcode1 = OpCode::new(0x69, Mnemonic::ADC, AddrMode::IMM, 2);
        let opcode2 = OpCode::new(0x69, Mnemonic::ADC, AddrMode::IMM, 2);
        assert_eq!(opcode1, opcode2);
    }

    #[test]
    fn test_opcode_inequality() {
        let opcode1 = OpCode::new(0x69, Mnemonic::ADC, AddrMode::IMM, 2);
        let opcode2 = OpCode::new(0x6D, Mnemonic::ADC, AddrMode::ABS, 4);
        assert_ne!(opcode1, opcode2);
    }

    #[test]
    fn test_opcodes_table_count() {
        assert_eq!(OPCODE_TABLE.len(), 256);
    }

    #[test]
    fn test_lookup_existing_opcode() {
        let opcode = lookup(0x69);
        assert_eq!(opcode.code, 0x69);
        assert_eq!(opcode.mnemonic, Mnemonic::ADC);
        assert_eq!(opcode.mode, AddrMode::IMM);
    }

    #[test]
    fn test_lookup_brk() {
        let opcode = lookup(0x00);
        assert_eq!(opcode.mnemonic, Mnemonic::BRK);
    }

    #[test]
    fn test_lookup_lda_immediate() {
        let opcode = lookup(0xA9);
        assert_eq!(opcode.mnemonic, Mnemonic::LDA);
        assert_eq!(opcode.mode, AddrMode::IMM);
    }

    #[test]
    fn test_lookup_returns_entry_for_all_codes() {
        for code in 0u8..=u8::MAX {
            let opcode = lookup(code);
            assert_eq!(opcode.code, code);
        }
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
        let opcode = OpCode::new(BRK, Mnemonic::BRK, AddrMode::IMP, 7);
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_acc_mode() {
        let opcode = OpCode::new(ASL_A, Mnemonic::ASL, AddrMode::ACC, 2);
        assert_eq!(opcode.bytes(), 1);
    }

    #[test]
    fn test_bytes_imm_mode() {
        let opcode = OpCode::new(LDA_IMM, Mnemonic::LDA, AddrMode::IMM, 2);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zp_mode() {
        let opcode = OpCode::new(LDA_ZP, Mnemonic::LDA, AddrMode::ZP, 3);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpx_mode() {
        let opcode = OpCode::new(LDA_ZPX, Mnemonic::LDA, AddrMode::ZPX, 4);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_zpy_mode() {
        let opcode = OpCode::new(LDX_ZPY, Mnemonic::LDX, AddrMode::ZPY, 4);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indx_mode() {
        let opcode = OpCode::new(LDA_INDX, Mnemonic::LDA, AddrMode::INDX, 6);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_indy_mode() {
        let opcode = OpCode::new(LDA_INDY, Mnemonic::LDA, AddrMode::INDY, 5);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_rel_mode() {
        let opcode = OpCode::new(BPL, Mnemonic::BPL, AddrMode::REL, 2);
        assert_eq!(opcode.bytes(), 2);
    }

    #[test]
    fn test_bytes_abs_mode() {
        let opcode = OpCode::new(LDA_ABS, Mnemonic::LDA, AddrMode::ABS, 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absx_mode() {
        let opcode = OpCode::new(LDA_ABSX, Mnemonic::LDA, AddrMode::ABSX, 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_absy_mode() {
        let opcode = OpCode::new(LDA_ABSY, Mnemonic::LDA, AddrMode::ABSY, 4);
        assert_eq!(opcode.bytes(), 3);
    }

    #[test]
    fn test_bytes_ind_mode() {
        let opcode = OpCode::new(JMP_IND, Mnemonic::JMP, AddrMode::IND, 5);
        assert_eq!(opcode.bytes(), 3);
    }
}
