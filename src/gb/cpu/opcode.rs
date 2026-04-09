#[allow(dead_code)]
// The opcode table is infrastructure for debugging/tracing; not yet wired into execute().
/// Metadata for a single opcode.
#[derive(Clone, Copy)]
pub struct Opcode {
    pub mnemonic: &'static str,
    /// M-cycle count for the non-branching path (branch taken may be longer).
    pub cycles: u8,
}

#[allow(dead_code)]
impl Opcode {
    const fn new(mnemonic: &'static str, cycles: u8) -> Self {
        Self { mnemonic, cycles }
    }
}

/// Look up base-opcode metadata by opcode byte.
#[allow(dead_code)]
pub fn lookup(byte: u8) -> &'static Opcode {
    &BASE[byte as usize]
}

/// Look up CB-prefixed opcode metadata by the byte following 0xCB.
#[allow(dead_code)]
pub fn lookup_cb(byte: u8) -> &'static Opcode {
    &CB[byte as usize]
}

// ---------------------------------------------------------------------------
// Base opcode table (256 entries)
// ---------------------------------------------------------------------------
// Cycles listed are the minimum (non-branch) M-cycle count per Pan Docs.
// Branch-taken paths add cycles in the execute() handler for conditional ops.

#[rustfmt::skip]
static BASE: [Opcode; 256] = [
    // 0x00
    Opcode::new("NOP",        1), Opcode::new("LD BC,n16",  3), Opcode::new("LD (BC),A",  2), Opcode::new("INC BC",     2),
    Opcode::new("INC B",      1), Opcode::new("DEC B",      1), Opcode::new("LD B,n8",    2), Opcode::new("RLCA",       1),
    Opcode::new("LD (n16),SP",5), Opcode::new("ADD HL,BC",  2), Opcode::new("LD A,(BC)",  2), Opcode::new("DEC BC",     2),
    Opcode::new("INC C",      1), Opcode::new("DEC C",      1), Opcode::new("LD C,n8",    2), Opcode::new("RRCA",       1),
    // 0x10
    Opcode::new("STOP",       1), Opcode::new("LD DE,n16",  3), Opcode::new("LD (DE),A",  2), Opcode::new("INC DE",     2),
    Opcode::new("INC D",      1), Opcode::new("DEC D",      1), Opcode::new("LD D,n8",    2), Opcode::new("RLA",        1),
    Opcode::new("JR e8",      3), Opcode::new("ADD HL,DE",  2), Opcode::new("LD A,(DE)",  2), Opcode::new("DEC DE",     2),
    Opcode::new("INC E",      1), Opcode::new("DEC E",      1), Opcode::new("LD E,n8",    2), Opcode::new("RRA",        1),
    // 0x20
    Opcode::new("JR NZ,e8",   2), Opcode::new("LD HL,n16",  3), Opcode::new("LD (HL+),A", 2), Opcode::new("INC HL",     2),
    Opcode::new("INC H",      1), Opcode::new("DEC H",      1), Opcode::new("LD H,n8",    2), Opcode::new("DAA",        1),
    Opcode::new("JR Z,e8",    2), Opcode::new("ADD HL,HL",  2), Opcode::new("LD A,(HL+)", 2), Opcode::new("DEC HL",     2),
    Opcode::new("INC L",      1), Opcode::new("DEC L",      1), Opcode::new("LD L,n8",    2), Opcode::new("CPL",        1),
    // 0x30
    Opcode::new("JR NC,e8",   2), Opcode::new("LD SP,n16",  3), Opcode::new("LD (HL-),A", 2), Opcode::new("INC SP",     2),
    Opcode::new("INC (HL)",   3), Opcode::new("DEC (HL)",   3), Opcode::new("LD (HL),n8", 3), Opcode::new("SCF",        1),
    Opcode::new("JR C,e8",    2), Opcode::new("ADD HL,SP",  2), Opcode::new("LD A,(HL-)", 2), Opcode::new("DEC SP",     2),
    Opcode::new("INC A",      1), Opcode::new("DEC A",      1), Opcode::new("LD A,n8",    2), Opcode::new("CCF",        1),
    // 0x40  LD r,r block
    Opcode::new("LD B,B",     1), Opcode::new("LD B,C",     1), Opcode::new("LD B,D",     1), Opcode::new("LD B,E",     1),
    Opcode::new("LD B,H",     1), Opcode::new("LD B,L",     1), Opcode::new("LD B,(HL)",  2), Opcode::new("LD B,A",     1),
    Opcode::new("LD C,B",     1), Opcode::new("LD C,C",     1), Opcode::new("LD C,D",     1), Opcode::new("LD C,E",     1),
    Opcode::new("LD C,H",     1), Opcode::new("LD C,L",     1), Opcode::new("LD C,(HL)",  2), Opcode::new("LD C,A",     1),
    // 0x50
    Opcode::new("LD D,B",     1), Opcode::new("LD D,C",     1), Opcode::new("LD D,D",     1), Opcode::new("LD D,E",     1),
    Opcode::new("LD D,H",     1), Opcode::new("LD D,L",     1), Opcode::new("LD D,(HL)",  2), Opcode::new("LD D,A",     1),
    Opcode::new("LD E,B",     1), Opcode::new("LD E,C",     1), Opcode::new("LD E,D",     1), Opcode::new("LD E,E",     1),
    Opcode::new("LD E,H",     1), Opcode::new("LD E,L",     1), Opcode::new("LD E,(HL)",  2), Opcode::new("LD E,A",     1),
    // 0x60
    Opcode::new("LD H,B",     1), Opcode::new("LD H,C",     1), Opcode::new("LD H,D",     1), Opcode::new("LD H,E",     1),
    Opcode::new("LD H,H",     1), Opcode::new("LD H,L",     1), Opcode::new("LD H,(HL)",  2), Opcode::new("LD H,A",     1),
    Opcode::new("LD L,B",     1), Opcode::new("LD L,C",     1), Opcode::new("LD L,D",     1), Opcode::new("LD L,E",     1),
    Opcode::new("LD L,H",     1), Opcode::new("LD L,L",     1), Opcode::new("LD L,(HL)",  2), Opcode::new("LD L,A",     1),
    // 0x70
    Opcode::new("LD (HL),B",  2), Opcode::new("LD (HL),C",  2), Opcode::new("LD (HL),D",  2), Opcode::new("LD (HL),E",  2),
    Opcode::new("LD (HL),H",  2), Opcode::new("LD (HL),L",  2), Opcode::new("HALT",       1), Opcode::new("LD (HL),A",  2),
    Opcode::new("LD A,B",     1), Opcode::new("LD A,C",     1), Opcode::new("LD A,D",     1), Opcode::new("LD A,E",     1),
    Opcode::new("LD A,H",     1), Opcode::new("LD A,L",     1), Opcode::new("LD A,(HL)",  2), Opcode::new("LD A,A",     1),
    // 0x80  ALU A,r block
    Opcode::new("ADD A,B",    1), Opcode::new("ADD A,C",    1), Opcode::new("ADD A,D",    1), Opcode::new("ADD A,E",    1),
    Opcode::new("ADD A,H",    1), Opcode::new("ADD A,L",    1), Opcode::new("ADD A,(HL)", 2), Opcode::new("ADD A,A",    1),
    Opcode::new("ADC A,B",    1), Opcode::new("ADC A,C",    1), Opcode::new("ADC A,D",    1), Opcode::new("ADC A,E",    1),
    Opcode::new("ADC A,H",    1), Opcode::new("ADC A,L",    1), Opcode::new("ADC A,(HL)", 2), Opcode::new("ADC A,A",    1),
    // 0x90
    Opcode::new("SUB A,B",    1), Opcode::new("SUB A,C",    1), Opcode::new("SUB A,D",    1), Opcode::new("SUB A,E",    1),
    Opcode::new("SUB A,H",    1), Opcode::new("SUB A,L",    1), Opcode::new("SUB A,(HL)", 2), Opcode::new("SUB A,A",    1),
    Opcode::new("SBC A,B",    1), Opcode::new("SBC A,C",    1), Opcode::new("SBC A,D",    1), Opcode::new("SBC A,E",    1),
    Opcode::new("SBC A,H",    1), Opcode::new("SBC A,L",    1), Opcode::new("SBC A,(HL)", 2), Opcode::new("SBC A,A",    1),
    // 0xA0
    Opcode::new("AND A,B",    1), Opcode::new("AND A,C",    1), Opcode::new("AND A,D",    1), Opcode::new("AND A,E",    1),
    Opcode::new("AND A,H",    1), Opcode::new("AND A,L",    1), Opcode::new("AND A,(HL)", 2), Opcode::new("AND A,A",    1),
    Opcode::new("XOR A,B",    1), Opcode::new("XOR A,C",    1), Opcode::new("XOR A,D",    1), Opcode::new("XOR A,E",    1),
    Opcode::new("XOR A,H",    1), Opcode::new("XOR A,L",    1), Opcode::new("XOR A,(HL)", 2), Opcode::new("XOR A,A",    1),
    // 0xB0
    Opcode::new("OR A,B",     1), Opcode::new("OR A,C",     1), Opcode::new("OR A,D",     1), Opcode::new("OR A,E",     1),
    Opcode::new("OR A,H",     1), Opcode::new("OR A,L",     1), Opcode::new("OR A,(HL)",  2), Opcode::new("OR A,A",     1),
    Opcode::new("CP A,B",     1), Opcode::new("CP A,C",     1), Opcode::new("CP A,D",     1), Opcode::new("CP A,E",     1),
    Opcode::new("CP A,H",     1), Opcode::new("CP A,L",     1), Opcode::new("CP A,(HL)",  2), Opcode::new("CP A,A",     1),
    // 0xC0
    Opcode::new("RET NZ",     2), Opcode::new("POP BC",     3), Opcode::new("JP NZ,n16",  3), Opcode::new("JP n16",     4),
    Opcode::new("CALL NZ,n16",3), Opcode::new("PUSH BC",    4), Opcode::new("ADD A,n8",   2), Opcode::new("RST $00",    4),
    Opcode::new("RET Z",      2), Opcode::new("RET",        4), Opcode::new("JP Z,n16",   3), Opcode::new("PREFIX CB",  1),
    Opcode::new("CALL Z,n16", 3), Opcode::new("CALL n16",   6), Opcode::new("ADC A,n8",   2), Opcode::new("RST $08",    4),
    // 0xD0
    Opcode::new("RET NC",     2), Opcode::new("POP DE",     3), Opcode::new("JP NC,n16",  3), Opcode::new("ILLEGAL_D3", 1),
    Opcode::new("CALL NC,n16",3), Opcode::new("PUSH DE",    4), Opcode::new("SUB A,n8",   2), Opcode::new("RST $10",    4),
    Opcode::new("RET C",      2), Opcode::new("RETI",       4), Opcode::new("JP C,n16",   3), Opcode::new("ILLEGAL_DB", 1),
    Opcode::new("CALL C,n16", 3), Opcode::new("ILLEGAL_DD", 1), Opcode::new("SBC A,n8",   2), Opcode::new("RST $18",    4),
    // 0xE0
    Opcode::new("LDH (n8),A", 3), Opcode::new("POP HL",     3), Opcode::new("LDH (C),A",  2), Opcode::new("ILLEGAL_E3", 1),
    Opcode::new("ILLEGAL_E4", 1), Opcode::new("PUSH HL",    4), Opcode::new("AND A,n8",   2), Opcode::new("RST $20",    4),
    Opcode::new("ADD SP,e8",  4), Opcode::new("JP HL",      1), Opcode::new("LD (n16),A",  4), Opcode::new("ILLEGAL_EB", 1),
    Opcode::new("ILLEGAL_EC", 1), Opcode::new("ILLEGAL_ED", 1), Opcode::new("XOR A,n8",   2), Opcode::new("RST $28",    4),
    // 0xF0
    Opcode::new("LDH A,(n8)", 3), Opcode::new("POP AF",     3), Opcode::new("LDH A,(C)",  2), Opcode::new("DI",         1),
    Opcode::new("ILLEGAL_F4", 1), Opcode::new("PUSH AF",    4), Opcode::new("OR A,n8",    2), Opcode::new("RST $30",    4),
    Opcode::new("LD HL,SP+e8",3), Opcode::new("LD SP,HL",   2), Opcode::new("LD A,(n16)", 4), Opcode::new("EI",         1),
    Opcode::new("ILLEGAL_FC", 1), Opcode::new("ILLEGAL_FD", 1), Opcode::new("CP A,n8",    2), Opcode::new("RST $38",    4),
];

// ---------------------------------------------------------------------------
// CB-prefixed opcode table (256 entries).
// Register variants: 2 M-cycles; (HL) variants: 4 M-cycles.
// Exception: `BIT b,(HL)` is 3 M-cycles (read only, no write-back).
// ---------------------------------------------------------------------------
#[rustfmt::skip]
static CB: [Opcode; 256] = {
    // helper to generate the 8 register variants for one CB operation
    // (register order: B, C, D, E, H, L, (HL), A)
    const fn is_bit_op(mnemonic: &'static str) -> bool {
        let bytes = mnemonic.as_bytes();
        bytes.len() >= 4
            && bytes[0] == b'B'
            && bytes[1] == b'I'
            && bytes[2] == b'T'
            && bytes[3] == b' '
    }

    const fn op(mnemonic: &'static str, is_hl: bool) -> Opcode {
        let cycles = if !is_hl {
            2
        } else if is_bit_op(mnemonic) {
            3 // BIT b,(HL): prefix + CB byte + read(HL) — no write-back
        } else {
            4 // SET/RES/rotate (HL): prefix + CB byte + read(HL) + write(HL)
        };
        Opcode::new(mnemonic, cycles)
    }

    // Rows: RLC, RRC, RL, RR, SLA, SRA, SWAP, SRL  (8 rows × 8 regs = 64)
    //       BIT 0..7 (8 rows × 8 regs = 64)
    //       RES 0..7 (8 rows × 8 regs = 64)
    //       SET 0..7 (8 rows × 8 regs = 64)
    [
        // 0x00  RLC r
        op("RLC B",false),op("RLC C",false),op("RLC D",false),op("RLC E",false),
        op("RLC H",false),op("RLC L",false),op("RLC (HL)",true),op("RLC A",false),
        // 0x08  RRC r
        op("RRC B",false),op("RRC C",false),op("RRC D",false),op("RRC E",false),
        op("RRC H",false),op("RRC L",false),op("RRC (HL)",true),op("RRC A",false),
        // 0x10  RL r
        op("RL B",false),op("RL C",false),op("RL D",false),op("RL E",false),
        op("RL H",false),op("RL L",false),op("RL (HL)",true),op("RL A",false),
        // 0x18  RR r
        op("RR B",false),op("RR C",false),op("RR D",false),op("RR E",false),
        op("RR H",false),op("RR L",false),op("RR (HL)",true),op("RR A",false),
        // 0x20  SLA r
        op("SLA B",false),op("SLA C",false),op("SLA D",false),op("SLA E",false),
        op("SLA H",false),op("SLA L",false),op("SLA (HL)",true),op("SLA A",false),
        // 0x28  SRA r
        op("SRA B",false),op("SRA C",false),op("SRA D",false),op("SRA E",false),
        op("SRA H",false),op("SRA L",false),op("SRA (HL)",true),op("SRA A",false),
        // 0x30  SWAP r
        op("SWAP B",false),op("SWAP C",false),op("SWAP D",false),op("SWAP E",false),
        op("SWAP H",false),op("SWAP L",false),op("SWAP (HL)",true),op("SWAP A",false),
        // 0x38  SRL r
        op("SRL B",false),op("SRL C",false),op("SRL D",false),op("SRL E",false),
        op("SRL H",false),op("SRL L",false),op("SRL (HL)",true),op("SRL A",false),
        // 0x40  BIT 0,r
        op("BIT 0,B",false),op("BIT 0,C",false),op("BIT 0,D",false),op("BIT 0,E",false),
        op("BIT 0,H",false),op("BIT 0,L",false),op("BIT 0,(HL)",true),op("BIT 0,A",false),
        // 0x48  BIT 1,r
        op("BIT 1,B",false),op("BIT 1,C",false),op("BIT 1,D",false),op("BIT 1,E",false),
        op("BIT 1,H",false),op("BIT 1,L",false),op("BIT 1,(HL)",true),op("BIT 1,A",false),
        // 0x50  BIT 2,r
        op("BIT 2,B",false),op("BIT 2,C",false),op("BIT 2,D",false),op("BIT 2,E",false),
        op("BIT 2,H",false),op("BIT 2,L",false),op("BIT 2,(HL)",true),op("BIT 2,A",false),
        // 0x58  BIT 3,r
        op("BIT 3,B",false),op("BIT 3,C",false),op("BIT 3,D",false),op("BIT 3,E",false),
        op("BIT 3,H",false),op("BIT 3,L",false),op("BIT 3,(HL)",true),op("BIT 3,A",false),
        // 0x60  BIT 4,r
        op("BIT 4,B",false),op("BIT 4,C",false),op("BIT 4,D",false),op("BIT 4,E",false),
        op("BIT 4,H",false),op("BIT 4,L",false),op("BIT 4,(HL)",true),op("BIT 4,A",false),
        // 0x68  BIT 5,r
        op("BIT 5,B",false),op("BIT 5,C",false),op("BIT 5,D",false),op("BIT 5,E",false),
        op("BIT 5,H",false),op("BIT 5,L",false),op("BIT 5,(HL)",true),op("BIT 5,A",false),
        // 0x70  BIT 6,r
        op("BIT 6,B",false),op("BIT 6,C",false),op("BIT 6,D",false),op("BIT 6,E",false),
        op("BIT 6,H",false),op("BIT 6,L",false),op("BIT 6,(HL)",true),op("BIT 6,A",false),
        // 0x78  BIT 7,r
        op("BIT 7,B",false),op("BIT 7,C",false),op("BIT 7,D",false),op("BIT 7,E",false),
        op("BIT 7,H",false),op("BIT 7,L",false),op("BIT 7,(HL)",true),op("BIT 7,A",false),
        // 0x80  RES 0,r
        op("RES 0,B",false),op("RES 0,C",false),op("RES 0,D",false),op("RES 0,E",false),
        op("RES 0,H",false),op("RES 0,L",false),op("RES 0,(HL)",true),op("RES 0,A",false),
        // 0x88  RES 1,r
        op("RES 1,B",false),op("RES 1,C",false),op("RES 1,D",false),op("RES 1,E",false),
        op("RES 1,H",false),op("RES 1,L",false),op("RES 1,(HL)",true),op("RES 1,A",false),
        // 0x90  RES 2,r
        op("RES 2,B",false),op("RES 2,C",false),op("RES 2,D",false),op("RES 2,E",false),
        op("RES 2,H",false),op("RES 2,L",false),op("RES 2,(HL)",true),op("RES 2,A",false),
        // 0x98  RES 3,r
        op("RES 3,B",false),op("RES 3,C",false),op("RES 3,D",false),op("RES 3,E",false),
        op("RES 3,H",false),op("RES 3,L",false),op("RES 3,(HL)",true),op("RES 3,A",false),
        // 0xA0  RES 4,r
        op("RES 4,B",false),op("RES 4,C",false),op("RES 4,D",false),op("RES 4,E",false),
        op("RES 4,H",false),op("RES 4,L",false),op("RES 4,(HL)",true),op("RES 4,A",false),
        // 0xA8  RES 5,r
        op("RES 5,B",false),op("RES 5,C",false),op("RES 5,D",false),op("RES 5,E",false),
        op("RES 5,H",false),op("RES 5,L",false),op("RES 5,(HL)",true),op("RES 5,A",false),
        // 0xB0  RES 6,r
        op("RES 6,B",false),op("RES 6,C",false),op("RES 6,D",false),op("RES 6,E",false),
        op("RES 6,H",false),op("RES 6,L",false),op("RES 6,(HL)",true),op("RES 6,A",false),
        // 0xB8  RES 7,r
        op("RES 7,B",false),op("RES 7,C",false),op("RES 7,D",false),op("RES 7,E",false),
        op("RES 7,H",false),op("RES 7,L",false),op("RES 7,(HL)",true),op("RES 7,A",false),
        // 0xC0  SET 0,r
        op("SET 0,B",false),op("SET 0,C",false),op("SET 0,D",false),op("SET 0,E",false),
        op("SET 0,H",false),op("SET 0,L",false),op("SET 0,(HL)",true),op("SET 0,A",false),
        // 0xC8  SET 1,r
        op("SET 1,B",false),op("SET 1,C",false),op("SET 1,D",false),op("SET 1,E",false),
        op("SET 1,H",false),op("SET 1,L",false),op("SET 1,(HL)",true),op("SET 1,A",false),
        // 0xD0  SET 2,r
        op("SET 2,B",false),op("SET 2,C",false),op("SET 2,D",false),op("SET 2,E",false),
        op("SET 2,H",false),op("SET 2,L",false),op("SET 2,(HL)",true),op("SET 2,A",false),
        // 0xD8  SET 3,r
        op("SET 3,B",false),op("SET 3,C",false),op("SET 3,D",false),op("SET 3,E",false),
        op("SET 3,H",false),op("SET 3,L",false),op("SET 3,(HL)",true),op("SET 3,A",false),
        // 0xE0  SET 4,r
        op("SET 4,B",false),op("SET 4,C",false),op("SET 4,D",false),op("SET 4,E",false),
        op("SET 4,H",false),op("SET 4,L",false),op("SET 4,(HL)",true),op("SET 4,A",false),
        // 0xE8  SET 5,r
        op("SET 5,B",false),op("SET 5,C",false),op("SET 5,D",false),op("SET 5,E",false),
        op("SET 5,H",false),op("SET 5,L",false),op("SET 5,(HL)",true),op("SET 5,A",false),
        // 0xF0  SET 6,r
        op("SET 6,B",false),op("SET 6,C",false),op("SET 6,D",false),op("SET 6,E",false),
        op("SET 6,H",false),op("SET 6,L",false),op("SET 6,(HL)",true),op("SET 6,A",false),
        // 0xF8  SET 7,r
        op("SET 7,B",false),op("SET 7,C",false),op("SET 7,D",false),op("SET 7,E",false),
        op("SET 7,H",false),op("SET 7,L",false),op("SET 7,(HL)",true),op("SET 7,A",false),
    ]
};
