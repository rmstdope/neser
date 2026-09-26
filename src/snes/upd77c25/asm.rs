//! A tiny uPD77C25 assembler for tests, encoding opcodes exactly as fullsnes' "uPD77C25 Opcode
//! Encoding" table lays them out. It lets tests write synthetic firmware without Nintendo's.

/// ALU opcodes (bits 19-16).
#[derive(Clone, Copy)]
pub enum AluOp {
    Nop = 0x0,
    Or = 0x1,
    And = 0x2,
    Xor = 0x3,
    Sub = 0x4,
    Add = 0x5,
    Sbb = 0x6,
    Adc = 0x7,
    Dec = 0x8,
    Inc = 0x9,
    Not = 0xA,
    Sar1 = 0xB,
    Rcl1 = 0xC,
    Sll2 = 0xD,
    Sll4 = 0xE,
    Xchg = 0xF,
}

// P select (bits 21-20).
pub const P_RAM: u32 = 0;
pub const P_IDB: u32 = 1;
pub const P_M: u32 = 2;
pub const P_N: u32 = 3;

// Accumulator (bit 15).
pub const ALU_A: u32 = 0;
pub const ALU_B: u32 = 1;

// DPL adjust (bits 14-13).
pub const DPL_INC: u32 = 1;
pub const DPL_DEC: u32 = 2;
pub const DPL_CLR: u32 = 3;

// SRC (bits 7-4).
pub const SRC_TRB: u32 = 0x0;
pub const SRC_A: u32 = 0x1;
pub const SRC_B: u32 = 0x2;
pub const SRC_TR: u32 = 0x3;
pub const SRC_DP: u32 = 0x4;
pub const SRC_RP: u32 = 0x5;
pub const SRC_RO: u32 = 0x6;
pub const SRC_SGN: u32 = 0x7;
pub const SRC_DR: u32 = 0x8;
pub const SRC_DRNF: u32 = 0x9;
pub const SRC_K: u32 = 0xD;
pub const SRC_L: u32 = 0xE;

// DST (bits 3-0).
pub const DST_NON: u32 = 0x0;
pub const DST_A: u32 = 0x1;
pub const DST_B: u32 = 0x2;
pub const DST_TR: u32 = 0x3;
pub const DST_DP: u32 = 0x4;
pub const DST_RP: u32 = 0x5;
pub const DST_DR: u32 = 0x6;
pub const DST_SR: u32 = 0x7;
pub const DST_SO: u32 = 0x8;
pub const DST_K: u32 = 0xA;
pub const DST_KLR: u32 = 0xB;
pub const DST_KLM: u32 = 0xC;
pub const DST_L: u32 = 0xD;
pub const DST_TRB: u32 = 0xE;
pub const DST_MEM: u32 = 0xF;

// BRCH (bits 21-13).
pub const JMPSO: u16 = 0x000;
pub const JMP: u16 = 0x100;
pub const CALL: u16 = 0x140;
pub const JNCA: u16 = 0x080;
pub const JCA: u16 = 0x082;
pub const JNCB: u16 = 0x084;
pub const JCB: u16 = 0x086;
pub const JNZA: u16 = 0x088;
pub const JZA: u16 = 0x08A;
pub const JNZB: u16 = 0x08C;
pub const JZB: u16 = 0x08E;
pub const JNOVA0: u16 = 0x090;
pub const JOVA0: u16 = 0x092;
pub const JNOVB0: u16 = 0x094;
pub const JOVB0: u16 = 0x096;
pub const JNOVA1: u16 = 0x098;
pub const JOVA1: u16 = 0x09A;
pub const JNOVB1: u16 = 0x09C;
pub const JOVB1: u16 = 0x09E;
pub const JNSA0: u16 = 0x0A0;
pub const JSA0: u16 = 0x0A2;
pub const JNSB0: u16 = 0x0A4;
pub const JSB0: u16 = 0x0A6;
pub const JNSA1: u16 = 0x0A8;
pub const JSA1: u16 = 0x0AA;
pub const JNSB1: u16 = 0x0AC;
pub const JSB1: u16 = 0x0AE;
pub const JDPL0: u16 = 0x0B0;
pub const JDPLN0: u16 = 0x0B1;
pub const JDPLF: u16 = 0x0B2;
pub const JDPLNF: u16 = 0x0B3;
pub const JNSIAK: u16 = 0x0B4;
pub const JSIAK: u16 = 0x0B6;
pub const JNSOAK: u16 = 0x0B8;
pub const JSOAK: u16 = 0x0BA;
pub const JNRQM: u16 = 0x0BC;
pub const JRQM: u16 = 0x0BE;

/// The all-zero opcode: an ALU NOP moving TRB to nowhere.
pub const NOP: u32 = 0;

/// An ALU instruction with P = IDB (the source value).
pub fn alu(op: AluOp, acc: u32, src: u32, dst: u32) -> u32 {
    alu_p(op, P_IDB, acc, src, dst)
}

/// An ALU instruction with an explicit P select.
pub fn alu_p(op: AluOp, p: u32, acc: u32, src: u32, dst: u32) -> u32 {
    (p << 20) | ((op as u32) << 16) | (acc << 15) | (src << 4) | dst
}

/// A NOP ALU instruction that only adjusts DP.
pub fn alu_dp(dpl: u32, dph: u32, dst: u32) -> u32 {
    (dpl << 13) | (dph << 9) | dst
}

/// Adds a DPL adjustment to an ALU instruction.
pub fn with_dpl(opcode: u32, dpl: u32) -> u32 {
    opcode | (dpl << 13)
}

/// Adds RPDEC to an ALU instruction.
pub fn with_rpdec(opcode: u32) -> u32 {
    opcode | 0x100
}

/// Sets RT (return after the ALU instruction).
pub fn rt(opcode: u32) -> u32 {
    opcode | 0x40_0000
}

/// A JP instruction to an 11-bit word address.
pub fn jp(brch: u16, next_address: u16) -> u32 {
    0x80_0000 | (u32::from(brch) << 13) | ((u32::from(next_address) & 0x7FF) << 2)
}

/// An LD instruction.
pub fn ld(dst: u32, immediate: u16) -> u32 {
    0xC0_0000 | (u32::from(immediate) << 6) | dst
}
