use crate::memory::Memory;

/// NES 6502 CPU
pub struct Cpu {
    /// Accumulator
    pub a: u8,
    /// X register
    pub x: u8,
    /// Y register
    pub y: u8,
    /// Stack pointer
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
    /// Memory
    pub memory: Memory,
}

// Status register flags
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
const FLAG_DECIMAL: u8 = 0b0000_1000;
//const FLAG_BREAK: u8 = 0b0001_0000;
//const FLAG_UNUSED: u8 = 0b0010_0000;
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

// Suppress dead_code warnings for opcode constants used in match patterns
// Opcodes
const ADC_ABS: u8 = 0x6D;
const ADC_ABSX: u8 = 0x7D;
const ADC_ABSY: u8 = 0x79;
const ADC_IMM: u8 = 0x69;
const ADC_INDX: u8 = 0x61;
const ADC_INDY: u8 = 0x71;
const ADC_ZP: u8 = 0x65;
const ADC_ZPX: u8 = 0x75;
const AND_ABS: u8 = 0x2D;
const AND_ABSX: u8 = 0x3D;
const AND_ABSY: u8 = 0x39;
const AND_IMM: u8 = 0x29;
const AND_INDX: u8 = 0x21;
const AND_INDY: u8 = 0x31;
const AND_ZP: u8 = 0x25;
const AND_ZPX: u8 = 0x35;
const ASL_A: u8 = 0x0A;
const ASL_ZP: u8 = 0x06;
const ASL_ZPX: u8 = 0x16;
const ASL_ABS: u8 = 0x0E;
const ASL_ABSX: u8 = 0x1E;
const BIT_ZP: u8 = 0x24;
const BIT_ABS: u8 = 0x2C;
const BCC: u8 = 0x90;
const BCS: u8 = 0xB0;
const BEQ: u8 = 0xF0;
const BMI: u8 = 0x30;
const BNE: u8 = 0xD0;
const BPL: u8 = 0x10;
const BVC: u8 = 0x50;
const BVS: u8 = 0x70;
pub const BRK: u8 = 0x00;
const CMP_IMM: u8 = 0xC9;
const CMP_ZP: u8 = 0xC5;
const CMP_ZPX: u8 = 0xD5;
const CMP_ABS: u8 = 0xCD;
const CMP_ABSX: u8 = 0xDD;
const CMP_ABSY: u8 = 0xD9;
const CMP_INDX: u8 = 0xC1;
const CMP_INDY: u8 = 0xD1;
const CPX_IMM: u8 = 0xE0;
const CPX_ZP: u8 = 0xE4;
const CPX_ABS: u8 = 0xEC;
const CPY_IMM: u8 = 0xC0;
const CPY_ZP: u8 = 0xC4;
const CPY_ABS: u8 = 0xCC;
const DEC_ZP: u8 = 0xC6;
const DEC_ZPX: u8 = 0xD6;
const DEC_ABS: u8 = 0xCE;
const DEC_ABSX: u8 = 0xDE;
const EOR_IMM: u8 = 0x49;
const EOR_ZP: u8 = 0x45;
const EOR_ZPX: u8 = 0x55;
const EOR_ABS: u8 = 0x4D;
const EOR_ABSX: u8 = 0x5D;
const EOR_ABSY: u8 = 0x59;
const EOR_INDX: u8 = 0x41;
const EOR_INDY: u8 = 0x51;
const CLC: u8 = 0x18;
const CLD: u8 = 0xD8;
const CLI: u8 = 0x58;
const CLV: u8 = 0xB8;
const SEC: u8 = 0x38;
const SED: u8 = 0xF8;
const SEI: u8 = 0x78;
const INC_ZP: u8 = 0xE6;
const INC_ZPX: u8 = 0xF6;
const INC_ABS: u8 = 0xEE;
const INC_ABSX: u8 = 0xFE;
const JMP_ABS: u8 = 0x4C;
const JMP_IND: u8 = 0x6C;
const JSR: u8 = 0x20;
pub const LDA_IMM: u8 = 0xA9;
const LDA_ZP: u8 = 0xA5;
const LDA_ZPX: u8 = 0xB5;
const LDA_ABS: u8 = 0xAD;
const LDA_ABSX: u8 = 0xBD;
const LDA_ABSY: u8 = 0xB9;
const LDA_INDX: u8 = 0xA1;
const LDA_INDY: u8 = 0xB1;
const LDX_IMM: u8 = 0xA2;
const LDX_ZP: u8 = 0xA6;
const LDX_ZPY: u8 = 0xB6;
const LDX_ABS: u8 = 0xAE;
const LDX_ABSY: u8 = 0xBE;
const LDY_IMM: u8 = 0xA0;
const LDY_ZP: u8 = 0xA4;
const LDY_ZPX: u8 = 0xB4;
const LDY_ABS: u8 = 0xAC;
const LDY_ABSX: u8 = 0xBC;
const LSR_ACC: u8 = 0x4A;
const LSR_ZP: u8 = 0x46;
const LSR_ZPX: u8 = 0x56;
const LSR_ABS: u8 = 0x4E;
const LSR_ABSX: u8 = 0x5E;
const NOP: u8 = 0xEA;
const ORA_IMM: u8 = 0x09;
const ORA_ZP: u8 = 0x05;
const ORA_ZPX: u8 = 0x15;
const ORA_ABS: u8 = 0x0D;
const ORA_ABSX: u8 = 0x1D;
const ORA_ABSY: u8 = 0x19;
const ORA_INDX: u8 = 0x01;
const ORA_INDY: u8 = 0x11;
const TAX: u8 = 0xAA;
const TAY: u8 = 0xA8;
const TXA: u8 = 0x8A;
const TYA: u8 = 0x98;
const DEX: u8 = 0xCA;
const DEY: u8 = 0x88;
const INX: u8 = 0xE8;
const INY: u8 = 0xC8;
const ROL_ACC: u8 = 0x2A;
const ROL_ZP: u8 = 0x26;
const ROL_ZPX: u8 = 0x36;
const ROL_ABS: u8 = 0x2E;
const ROL_ABSX: u8 = 0x3E;
const ROR_ACC: u8 = 0x6A;
const ROR_ZP: u8 = 0x66;
const ROR_ZPX: u8 = 0x76;
const ROR_ABS: u8 = 0x6E;
const ROR_ABSX: u8 = 0x7E;
const RTI: u8 = 0x40;
const RTS: u8 = 0x60;
const SBC_IMM: u8 = 0xE9;
const SBC_ZP: u8 = 0xE5;
const SBC_ZPX: u8 = 0xF5;
const SBC_ABS: u8 = 0xED;
const SBC_ABSX: u8 = 0xFD;
const SBC_ABSY: u8 = 0xF9;
const SBC_INDX: u8 = 0xE1;
const SBC_INDY: u8 = 0xF1;
const STA_ZP: u8 = 0x85;
const STA_ZPX: u8 = 0x95;
const STA_ABS: u8 = 0x8D;
const STA_ABSX: u8 = 0x9D;
const STA_ABSY: u8 = 0x99;
const STA_INDX: u8 = 0x81;
const STA_INDY: u8 = 0x91;
const TXS: u8 = 0x9A;
const TSX: u8 = 0xBA;
const PHA: u8 = 0x48;
const PLA: u8 = 0x68;
const PHP: u8 = 0x08;
const PLP: u8 = 0x28;
const STX_ZP: u8 = 0x86;
const STX_ZPY: u8 = 0x96;
const STX_ABS: u8 = 0x8E;
const STY_ZP: u8 = 0x84;
const STY_ZPX: u8 = 0x94;
const STY_ABS: u8 = 0x8C;

const RESET_VECTOR: u16 = 0xFFFC;

impl Cpu {
    /// Create a new CPU with default register values
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,              // Stack pointer starts at 0xFD
            pc: 0,                 // Program counter will be loaded from reset vector
            p: 0x24,               // Status: IRQ disabled, unused bit set
            memory: Memory::new(), // 64KB of memory
        }
    }

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.p = 0x24;
        self.pc = self.read_reset_vector();
    }

    /// Load a program into memory at the specified address and set reset vector
    pub fn load_program(&mut self, program: &[u8], address: u16) {
        for (i, &byte) in program.iter().enumerate() {
            self.memory.write(address + i as u16, byte);
        }
        self.write_reset_vector(address);
    }

    /// Load a program and run the CPU emulation
    pub fn load_and_run(&mut self, program: Vec<u8>) {
        self.load_program(&program, 0x8000);
        // Reset CPU
        self.reset();
        self.run();
    }

    /// Run the CPU emulation loop
    pub fn run(&mut self) {
        loop {
            if !self.run_opcode() {
                break;
            }
        }
    }

    /// Execute a single opcode. Returns false if execution should stop (BRK), true otherwise.
    pub fn run_opcode(&mut self) -> bool {
        let opcode = self.memory.read(self.pc);
        self.pc += 1;

        match opcode {
            ADC_IMM => {
                let value = self.read_byte();
                self.adc(value);
            }
            ADC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.adc(value);
            }
            ADC_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.adc(value);
            }
            AND_IMM => {
                let value = self.read_byte();
                self.and(value);
            }
            AND_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.and(value);
            }
            AND_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.and(value);
            }
            ASL_A => {
                self.a = self.asl(self.a);
            }
            ASL_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                let result = self.asl(value);
                self.memory.write(addr, result);
            }
            ASL_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                let result = self.asl(value);
                self.memory.write(addr, result);
            }
            ASL_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                let result = self.asl(value);
                self.memory.write(addr, result);
            }
            ASL_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                let result = self.asl(value);
                self.memory.write(addr, result);
            }
            BIT_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.bit(value);
            }
            BIT_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.bit(value);
            }
            BCC => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_CARRY == 0 {
                    self.branch(offset);
                }
            }
            BCS => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_CARRY != 0 {
                    self.branch(offset);
                }
            }
            BEQ => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_ZERO != 0 {
                    self.branch(offset);
                }
            }
            BMI => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_NEGATIVE != 0 {
                    self.branch(offset);
                }
            }
            BNE => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_ZERO == 0 {
                    self.branch(offset);
                }
            }
            BPL => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_NEGATIVE == 0 {
                    self.branch(offset);
                }
            }
            BVC => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_OVERFLOW == 0 {
                    self.branch(offset);
                }
            }
            BVS => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_OVERFLOW != 0 {
                    self.branch(offset);
                }
            }
            BRK => {
                return false;
            }
            CMP_IMM => {
                let value = self.read_byte();
                self.cmp(value);
            }
            CMP_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CMP_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.cmp(value);
            }
            CPX_IMM => {
                let value = self.read_byte();
                self.cpx(value);
            }
            CPX_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.cpx(value);
            }
            CPX_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.cpx(value);
            }
            CPY_IMM => {
                let value = self.read_byte();
                self.cpy(value);
            }
            CPY_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.cpy(value);
            }
            CPY_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.cpy(value);
            }
            DEC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr as u16);
                let result = self.dec(value);
                self.memory.write(addr, result);
            }
            DEC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr as u16);
                let result = self.dec(value);
                self.memory.write(addr, result);
            }
            DEC_ABS => {
                let addr = self.read_word() as u16;
                let value = self.memory.read(addr as u16);
                let result = self.dec(value);
                self.memory.write(addr, result);
            }
            DEC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16) as u16;
                let value = self.memory.read(addr as u16);
                let result = self.dec(value);
                self.memory.write(addr, result);
            }
            EOR_IMM => {
                let value = self.read_byte();
                self.eor(value);
            }
            EOR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.eor(value);
            }
            EOR_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.eor(value);
            }
            CLC => {
                self.p &= !FLAG_CARRY;
            }
            CLD => {
                self.p &= !FLAG_DECIMAL;
            }
            CLI => {
                self.p &= !FLAG_INTERRUPT;
            }
            CLV => {
                self.p &= !FLAG_OVERFLOW;
            }
            SEC => {
                self.p |= FLAG_CARRY;
            }
            SED => {
                self.p |= FLAG_DECIMAL;
            }
            SEI => {
                self.p |= FLAG_INTERRUPT;
            }
            INC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr as u16);
                let result = self.inc(value);
                self.memory.write(addr, result);
            }
            INC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr as u16);
                let result = self.inc(value);
                self.memory.write(addr, result);
            }
            INC_ABS => {
                let addr = self.read_word() as u16;
                let value = self.memory.read(addr as u16);
                let result = self.inc(value);
                self.memory.write(addr, result);
            }
            INC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16) as u16;
                let value = self.memory.read(addr as u16);
                let result = self.inc(value);
                self.memory.write(addr, result);
            }
            JMP_ABS => {
                let addr = self.read_word();
                self.pc = addr;
            }
            JMP_IND => {
                let ptr = self.read_word();
                let addr = self.read_word_indirect(ptr);
                self.pc = addr;
            }
            JSR => {
                let addr = self.read_word();
                let return_addr = self.pc - 1; // Address of last byte of JSR instruction
                self.push_word(return_addr);
                self.pc = addr;
            }
            LDA_IMM => {
                let value = self.read_byte();
                self.lda(value);
            }
            LDA_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDA_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.lda(value);
            }
            LDX_IMM => {
                let value = self.read_byte();
                self.ldx(value);
            }
            LDX_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.ldx(value);
            }
            LDX_ZPY => {
                let addr = self.read_byte().wrapping_add(self.y) as u16;
                let value = self.memory.read(addr);
                self.ldx(value);
            }
            LDX_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.ldx(value);
            }
            LDX_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.ldx(value);
            }
            LDY_IMM => {
                let value = self.read_byte();
                self.ldy(value);
            }
            LDY_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.ldy(value);
            }
            LDY_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.ldy(value);
            }
            LDY_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.ldy(value);
            }
            LDY_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.ldy(value);
            }
            LSR_ACC => {
                self.a = self.lsr(self.a);
            }
            LSR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                let result = self.lsr(value);
                self.memory.write(addr, result);
            }
            LSR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                let result = self.lsr(value);
                self.memory.write(addr, result);
            }
            LSR_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                let result = self.lsr(value);
                self.memory.write(addr, result);
            }
            LSR_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                let result = self.lsr(value);
                self.memory.write(addr, result);
            }
            NOP => {
                // No operation - do nothing
            }
            ORA_IMM => {
                let value = self.read_byte();
                self.ora(value);
            }
            ORA_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.ora(value);
            }
            ORA_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.ora(value);
            }
            DEX => {
                self.dex();
            }
            DEY => {
                self.dey();
            }
            INY => {
                self.iny();
            }
            INX => {
                self.inx();
            }
            TAX => {
                self.tax();
            }
            TAY => {
                self.tay();
            }
            TXA => {
                self.txa();
            }
            TYA => {
                self.tya();
            }
            ROL_ACC => {
                self.a = self.rol(self.a);
            }
            ROL_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                let result = self.rol(value);
                self.memory.write(addr, result);
            }
            ROL_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                let result = self.rol(value);
                self.memory.write(addr, result);
            }
            ROL_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                let result = self.rol(value);
                self.memory.write(addr, result);
            }
            ROL_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                let result = self.rol(value);
                self.memory.write(addr, result);
            }
            ROR_ACC => {
                self.a = self.ror(self.a);
            }
            ROR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                let result = self.ror(value);
                self.memory.write(addr, result);
            }
            ROR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                let result = self.ror(value);
                self.memory.write(addr, result);
            }
            ROR_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                let result = self.ror(value);
                self.memory.write(addr, result);
            }
            ROR_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                let result = self.ror(value);
                self.memory.write(addr, result);
            }
            RTI => {
                self.p = self.pop_byte();
                self.pc = self.pop_word();
            }
            RTS => {
                self.pc = self.pop_word();
                self.pc = self.pc.wrapping_add(1);
            }
            SBC_IMM => {
                let value = self.read_byte();
                self.sbc(value);
            }
            SBC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_ABS => {
                let addr = self.read_word();
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            SBC_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                let value = self.memory.read(addr);
                self.sbc(value);
            }
            STA_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.write(addr, self.a);
            }
            STA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.memory.write(addr, self.a);
            }
            STA_ABS => {
                let addr = self.read_word();
                self.memory.write(addr, self.a);
            }
            STA_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.memory.write(addr, self.a);
            }
            STA_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.memory.write(addr, self.a);
            }
            STA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                self.memory.write(addr, self.a);
            }
            STA_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                self.memory.write(addr, self.a);
            }
            TXS => {
                self.sp = self.x;
            }
            TSX => {
                self.x = self.sp;
                self.update_zero_and_negative_flags(self.x);
            }
            PHA => {
                self.push_byte(self.a);
            }
            PLA => {
                self.a = self.pop_byte();
                self.update_zero_and_negative_flags(self.a);
            }
            PHP => {
                self.push_byte(self.p);
            }
            PLP => {
                self.p = self.pop_byte();
            }
            STX_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.write(addr, self.x);
            }
            STX_ZPY => {
                let addr = self.read_byte().wrapping_add(self.y) as u16;
                self.memory.write(addr, self.x);
            }
            STX_ABS => {
                let addr = self.read_word();
                self.memory.write(addr, self.x);
            }
            STY_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.write(addr, self.y);
            }
            STY_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.memory.write(addr, self.y);
            }
            STY_ABS => {
                let addr = self.read_word();
                self.memory.write(addr, self.y);
            }
            _ => todo!(),
        }
        true
    }

    /// Read a byte from memory at PC and increment PC
    fn read_byte(&mut self) -> u8 {
        let value = self.memory.read(self.pc);
        self.pc += 1;
        value
    }

    /// Read a 16-bit word from memory at PC (little-endian) and increment PC
    fn read_word(&mut self) -> u16 {
        let lo = self.read_byte() as u16;
        let hi = self.read_byte() as u16;
        (hi << 8) | lo
    }

    /// Write a 16-bit word to memory at the specified address (little-endian)
    fn write_u16_to_addr(&mut self, addr: u16, value: u16) {
        self.memory.write(addr, (value & 0x00FF) as u8);
        self.memory.write(addr + 1, (value >> 8) as u8);
    }

    /// Write a 16-bit address to the reset vector at 0xFFFC-0xFFFD
    fn write_reset_vector(&mut self, addr: u16) {
        self.write_u16_to_addr(0xFFFC, addr);
    }

    /// Read a 16-bit word from memory at the specified address (little-endian)
    fn read_u16_from_addr(&self, addr: u16) -> u16 {
        let lo = self.memory.read(addr) as u16;
        let hi = self.memory.read(addr + 1) as u16;
        (hi << 8) | lo
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&self) -> u16 {
        self.read_u16_from_addr(RESET_VECTOR)
    }

    /// Read a 16-bit word from zero page (wraps at page boundary)
    fn read_word_from_zp(&self, addr: u8) -> u16 {
        let lo = self.memory.read(addr as u16) as u16;
        let hi = self.memory.read(addr.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }

    /// Read a word from an indirect address with 6502 page boundary bug
    /// If the address is at a page boundary (e.g., 0x10FF), the high byte
    /// is read from the start of the same page (0x1000) instead of the next page (0x1100)
    fn read_word_indirect(&self, addr: u16) -> u16 {
        let lo = self.memory.read(addr) as u16;
        let hi_addr = if addr & 0xFF == 0xFF {
            // Page boundary bug: wrap within the same page
            addr & 0xFF00
        } else {
            addr + 1
        };
        let hi = self.memory.read(hi_addr) as u16;
        (hi << 8) | lo
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.sp as u16);
        self.memory.write(addr, value);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte first
        self.push_byte(value as u8); // Low byte second
    }

    /// Pull a byte from the stack
    fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let addr = 0x0100 | (self.sp as u16);
        self.memory.read(addr)
    }

    /// Pull a word from the stack (low byte first)
    fn pop_word(&mut self) -> u16 {
        let lo = self.pop_byte() as u16; // Low byte first
        let hi = self.pop_byte() as u16; // High byte second
        (hi << 8) | lo
    }

    /// Update Zero and Negative flags based on a value
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        // Clear Z and N flags
        self.p &= !(FLAG_ZERO | FLAG_NEGATIVE);

        // Set Zero flag if value is 0
        if value == 0 {
            self.p |= FLAG_ZERO;
        }

        // Set Negative flag if bit 7 is set
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        }
    }

    /// Add with Carry - ADC operation
    fn adc(&mut self, value: u8) {
        let carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let sum = self.a as u16 + value as u16 + carry as u16;

        // Check for carry (result > 255)
        if sum > 0xFF {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Check for overflow
        // Overflow occurs when:
        // - Two positive numbers add to a negative result
        // - Two negative numbers add to a positive result
        let result = sum as u8;
        let overflow = (self.a ^ result) & (value ^ result) & 0x80;
        if overflow != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Bitwise AND - AND operation
    fn and(&mut self, value: u8) {
        self.a &= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Arithmetic Shift Left - ASL operation
    fn asl(&mut self, value: u8) -> u8 {
        let carry = if value & 0x80 != 0 { FLAG_CARRY } else { 0 };
        let result = value << 1;
        self.p = (self.p & !FLAG_CARRY) | carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Bit Test - BIT operation
    fn bit(&mut self, value: u8) {
        // Test bits: Zero flag is set based on A & value
        let result = self.a & value;
        if result == 0 {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Copy bit 7 of value to Negative flag
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }

        // Copy bit 6 of value to Overflow flag
        if value & 0x40 != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    /// Branch - Apply relative offset to PC
    fn branch(&mut self, offset: i8) {
        self.pc = self.pc.wrapping_add(offset as u16);
    }

    /// Compare operation - sets flags based on register - value
    fn compare(&mut self, register_value: u8, value: u8) {
        let result = register_value.wrapping_sub(value);

        // Set Carry flag if register >= value
        if register_value >= value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set Zero flag if register == value
        if register_value == value {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Set Negative flag based on bit 7 of result
        if result & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    /// Compare - CMP operation
    fn cmp(&mut self, value: u8) {
        self.compare(self.a, value);
    }

    /// Compare X Register - CPX operation
    fn cpx(&mut self, value: u8) {
        self.compare(self.x, value);
    }

    /// Compare Y Register - CPY operation
    fn cpy(&mut self, value: u8) {
        self.compare(self.y, value);
    }

    /// Decrement - DEC operation
    fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Exclusive OR - EOR operation
    fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Increment - INC operation
    fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Load Accumulator - LDA operation
    fn lda(&mut self, value: u8) {
        self.a = value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Load X Register - LDX operation
    fn ldx(&mut self, value: u8) {
        self.x = value;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Load Y Register - LDY operation
    fn ldy(&mut self, value: u8) {
        self.y = value;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Logical Shift Right - LSR operation
    fn lsr(&mut self, value: u8) -> u8 {
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = value >> 1;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Logical Inclusive OR - ORA operation
    fn ora(&mut self, value: u8) {
        self.a |= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Decrement X Register - DEX operation
    fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Decrement Y Register - DEY operation
    fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment Y Register - INY operation
    fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment X Register - INX operation
    fn inx(&mut self) {
        self.x = self.x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to X - TAX operation
    fn tax(&mut self) {
        self.x = self.a;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to Y - TAY operation
    fn tay(&mut self) {
        self.y = self.a;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Transfer X to Accumulator - TXA operation
    fn txa(&mut self) {
        self.a = self.x;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Transfer Y to Accumulator - TYA operation
    fn tya(&mut self) {
        self.a = self.y;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Rotate Left - ROL operation
    fn rol(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        // Bit 7 goes into carry flag
        if value & 0b10000000 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value << 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Rotate Right - ROR operation
    fn ror(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 {
            0b10000000
        } else {
            0
        };
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value >> 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Subtract with Carry - SBC operation
    fn sbc(&mut self, value: u8) {
        // SBC is equivalent to ADC with inverted value
        // A - M - (1 - C) = A + ~M + C
        let carry_in = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let inverted_value = !value;
        let result = self.a as u16 + inverted_value as u16 + carry_in;

        // Set carry flag if no borrow occurred (result >= 0x100)
        if result >= 0x100 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set overflow flag if signed overflow occurred
        // Overflow occurs when subtracting different signs yields wrong sign
        // Same logic as ADC but with inverted value
        let a_sign = self.a & 0x80;
        let m_sign = inverted_value & 0x80;
        let result_sign = (result as u8) & 0x80;
        if a_sign == m_sign && a_sign != result_sign {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result as u8;
        self.update_zero_and_negative_flags(self.a);
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_new() {
        let cpu = Cpu::new();
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.x, 0);
        assert_eq!(cpu.y, 0);
        assert_eq!(cpu.sp, 0xFD);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.p, 0x24);
    }

    #[test]
    fn test_cpu_reset() {
        let mut cpu = Cpu::new();
        cpu.a = 0xFF;
        cpu.x = 0xFF;
        cpu.y = 0xFF;
        cpu.sp = 0x00;
        cpu.p = 0xFF;

        cpu.reset();

        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.x, 0);
        assert_eq!(cpu.y, 0);
        assert_eq!(cpu.sp, 0xFD);
        assert_eq!(cpu.p, 0x24);
    }

    #[test]
    fn test_adc_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_IMM, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x10;
        cpu.run();
        assert_eq!(cpu.a, 0x30);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
        assert_eq!(cpu.p & FLAG_ZERO, 0); // Zero flag should be clear
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0); // Overflow flag should be clear
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Negative flag should be clear
    }

    #[test]
    fn test_adc_immediate_with_carry() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_IMM, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x10;
        cpu.p |= FLAG_CARRY; // Set carry flag
        cpu.run();
        assert_eq!(cpu.a, 0x31); // 0x10 + 0x20 + 1 (carry)
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
    }

    #[test]
    fn test_adc_immediate_carry_flag() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_IMM, 0x01, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.run();
        assert_eq!(cpu.a, 0x00); // Wraps around
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
    }

    #[test]
    fn test_adc_immediate_overflow_flag() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_IMM, 0x50, BRK]; // Add another positive
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50; // Positive number
        cpu.run();
        assert_eq!(cpu.a, 0xA0); // Result is negative in two's complement
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Negative flag should be set
    }

    #[test]
    fn test_adc_immediate_negative_overflow() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_IMM, 0x80, BRK]; // Add -128
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x80; // -128 in two's complement
        cpu.run();
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
    }

    #[test]
    fn test_adc_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x10;
        cpu.memory.write(0x42, 0x33);
        cpu.run();
        assert_eq!(cpu.a, 0x43);
    }

    #[test]
    fn test_adc_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_ABS, 0x34, 0x12, BRK]; // Little-endian
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x20;
        cpu.memory.write(0x1234, 0x55);
        cpu.run();
        assert_eq!(cpu.a, 0x75);
    }

    #[test]
    fn test_adc_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x10;
        cpu.x = 0x05;
        cpu.memory.write(0x1239, 0x44); // 0x1234 + 0x05
        cpu.run();
        assert_eq!(cpu.a, 0x54);
    }

    #[test]
    fn test_adc_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x15;
        cpu.x = 0x03;
        cpu.memory.write(0x45, 0x22); // 0x42 + 0x03
        cpu.run();
        assert_eq!(cpu.a, 0x37);
    }

    #[test]
    fn test_adc_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_ABSY, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x08;
        cpu.y = 0x10;
        cpu.memory.write(0x2010, 0x17); // 0x2000 + 0x10
        cpu.run();
        assert_eq!(cpu.a, 0x1F);
    }

    #[test]
    fn test_adc_indirect_x() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_INDX, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x05;
        cpu.x = 0x04;
        cpu.memory.write(0x24, 0x74); // Pointer at 0x20 + 0x04: low byte
        cpu.memory.write(0x25, 0x20); // Pointer at 0x20 + 0x04: high byte
        cpu.memory.write(0x2074, 0x33); // Value at address 0x2074
        cpu.run();
        assert_eq!(cpu.a, 0x38);
    }

    #[test]
    fn test_adc_indirect_y() {
        let mut cpu = Cpu::new();
        let program = vec![ADC_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x0A;
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28); // Pointer at 0x86: low byte
        cpu.memory.write(0x87, 0x40); // Pointer at 0x86: high byte
        cpu.memory.write(0x4038, 0x06); // Value at 0x4028 + 0x10
        cpu.run();
        assert_eq!(cpu.a, 0x10);
    }

    #[test]
    fn test_and_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![AND_IMM, 0b1010_1010, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.run();
        assert_eq!(cpu.a, 0b1010_0000);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_and_immediate_zero_flag() {
        let mut cpu = Cpu::new();
        let program = vec![AND_IMM, 0b0000_1111, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.run();
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_and_immediate_clears_negative_flag() {
        let mut cpu = Cpu::new();
        let program = vec![AND_IMM, 0b0111_1111, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.p = FLAG_NEGATIVE; // Set negative flag initially
        cpu.run();
        assert_eq!(cpu.a, 0b0111_1111);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_and_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![AND_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1100_1100;
        cpu.memory.write(0x42, 0b1010_1010);
        cpu.run();
        assert_eq!(cpu.a, 0b1000_1000);
    }

    #[test]
    fn test_and_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![AND_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b0011_1111); // 0x42 + 0x05
        cpu.run();
        assert_eq!(cpu.a, 0b0011_0000);
    }

    #[test]
    fn test_and_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![AND_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.memory.write(0x1234, 0b1100_1100);
        cpu.run();
        assert_eq!(cpu.a, 0b1000_1000);
    }

    #[test]
    fn test_and_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![AND_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b0101_0101); // 0x1234 + 0x10
        cpu.run();
        assert_eq!(cpu.a, 0b0101_0101);
    }

    #[test]
    fn test_and_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![AND_ABSY, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1100_0011;
        cpu.y = 0x20;
        cpu.memory.write(0x2020, 0b0011_1100); // 0x2000 + 0x20
        cpu.run();
        assert_eq!(cpu.a, 0b0000_0000);
    }

    #[test]
    fn test_and_indirect_x() {
        let mut cpu = Cpu::new();
        let program = vec![AND_INDX, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.x = 0x04;
        cpu.memory.write(0x24, 0x74); // Pointer at 0x20 + 0x04: low byte
        cpu.memory.write(0x25, 0x20); // Pointer at 0x20 + 0x04: high byte
        cpu.memory.write(0x2074, 0b0000_1111); // Value at address 0x2074
        cpu.run();
        assert_eq!(cpu.a, 0b0000_0000);
    }

    #[test]
    fn test_and_indirect_y() {
        let mut cpu = Cpu::new();
        let program = vec![AND_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28); // Pointer at 0x86: low byte
        cpu.memory.write(0x87, 0x40); // Pointer at 0x86: high byte
        cpu.memory.write(0x4038, 0b1111_0000); // Value at 0x4028 + 0x10
        cpu.run();
        assert_eq!(cpu.a, 0b1010_0000);
    }

    #[test]
    fn test_asl_accumulator() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_A, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b0100_0010;
        cpu.run();
        assert_eq!(cpu.a, 0b1000_0100);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_asl_accumulator_sets_carry() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_A, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1000_0001;
        cpu.run();
        assert_eq!(cpu.a, 0b0000_0010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_asl_accumulator_sets_zero() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_A, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1000_0000;
        cpu.run();
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_asl_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0b0011_0011);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0b0110_0110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_asl_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b1010_0101); // 0x42 + 0x05
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0b0100_1010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_asl_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0b0100_0001);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0b1000_0010);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_asl_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![ASL_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b0000_0001); // 0x1234 + 0x10
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0b0000_0010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_bit_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![BIT_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.memory.write(0x42, 0b1100_0011);
        cpu.run();
        // A & memory = 0b1111_0000 & 0b1100_0011 = 0b1100_0000 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 1
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bit_zero_page_sets_zero() {
        let mut cpu = Cpu::new();
        let program = vec![BIT_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b0000_1111;
        cpu.memory.write(0x42, 0b1111_0000);
        cpu.run();
        // A & memory = 0b0000_1111 & 0b1111_0000 = 0b0000_0000 (zero)
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        // Bit 7 of memory is 1
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bit_zero_page_clears_flags() {
        let mut cpu = Cpu::new();
        let program = vec![BIT_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.memory.write(0x42, 0b0011_1111);
        cpu.run();
        // A & memory = 0b1111_1111 & 0b0011_1111 = 0b0011_1111 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 0
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        // Bit 6 of memory is 0
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
    }

    #[test]
    fn test_bit_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![BIT_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.memory.write(0x1234, 0b0101_1010);
        cpu.run();
        // A & memory = 0b1010_1010 & 0b0101_1010 = 0b0000_1010 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 0
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bcc_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BCC, 0x02, 0x00, 0x00, BRK]; // Branch forward 2 bytes to skip padding
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Ensure carry is clear
        cpu.run();
        // PC should be at 0x8000 + 2 (after reading BCC and offset) + 2 (offset) + 1 (BRK) = 0x8005
        assert_eq!(cpu.pc, 0x8005);
    }

    #[test]
    fn test_bcc_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BCC, 0x05, BRK]; // Should not branch
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_CARRY; // Set carry flag
        cpu.run();
        // PC should be at 0x8000 + 2 (instruction) + 1 (BRK) = 0x8003
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bcc_branch_backward() {
        let mut cpu = Cpu::new();
        let program = vec![0x00, 0x00, 0x00, BCC, 0xFB]; // Branch backward -5 to hit BRK at 0x8000
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Ensure carry is clear
        // Put BRK at 0x8000, then BCC at 0x8003 that branches back to BRK
        cpu.memory.write(0x8000, BRK);
        cpu.run();
        // PC should be at 0x8001 (BRK at 0x8000 + 1)
        assert_eq!(cpu.pc, 0x8001);
    }

    #[test]
    fn test_bcs_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BCS, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_CARRY; // Set carry flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bcs_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BCS, 0x03, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Clear carry flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_beq_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BEQ, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_beq_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BEQ, 0x02, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_ZERO; // Clear zero flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bmi_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BMI, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_NEGATIVE; // Set negative flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bmi_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BMI, 0x04, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bne_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BNE, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_ZERO; // Clear zero flag (not equal)
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bne_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BNE, 0x06, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag (equal)
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bpl_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BPL, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_NEGATIVE; // Clear negative flag (positive)
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bpl_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BPL, 0x07, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_NEGATIVE; // Set negative flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bvc_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BVC, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bvc_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BVC, 0x05, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_OVERFLOW; // Set overflow flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bvs_branch_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BVS, 0x01, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p |= FLAG_OVERFLOW; // Set overflow flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bvs_branch_not_taken() {
        let mut cpu = Cpu::new();
        let program = vec![BVS, 0x08, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
        cpu.run();
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_cmp_immediate_equal() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // A == value
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is 0, bit 7 = 0
    }

    #[test]
    fn test_cmp_immediate_greater() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_IMM, 0x30, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is positive
    }

    #[test]
    fn test_cmp_immediate_less() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_IMM, 0x50, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x30;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
        assert_eq!(cpu.p & FLAG_CARRY, 0); // A < value
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative (0x30 - 0x50 = 0xE0)
    }

    #[test]
    fn test_cmp_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x80;
        cpu.memory.write(0x42, 0x80);
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cmp_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x10;
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0x05); // 0x42 + 0x05
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x10 >= 0x05
    }

    #[test]
    fn test_cmp_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x20;
        cpu.memory.write(0x1234, 0x30);
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
    }

    #[test]
    fn test_cmp_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0xFF);
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_cmp_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_ABSY, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x55;
        cpu.y = 0x20;
        cpu.memory.write(0x2020, 0x44);
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x55 >= 0x44
    }

    #[test]
    fn test_cmp_indirect_x() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_INDX, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x33;
        cpu.x = 0x04;
        cpu.memory.write(0x24, 0x74);
        cpu.memory.write(0x25, 0x20);
        cpu.memory.write(0x2074, 0x33);
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_cmp_indirect_y() {
        let mut cpu = Cpu::new();
        let program = vec![CMP_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x77;
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28);
        cpu.memory.write(0x87, 0x40);
        cpu.memory.write(0x4038, 0x88);
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x77 < 0x88
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpx_immediate_equal() {
        let mut cpu = Cpu::new();
        let program = vec![CPX_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpx_immediate_greater() {
        let mut cpu = Cpu::new();
        let program = vec![CPX_IMM, 0x30, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x50;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpx_immediate_less() {
        let mut cpu = Cpu::new();
        let program = vec![CPX_IMM, 0x50, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x30;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpx_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![CPX_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x80;
        cpu.memory.write(0x42, 0x80);
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cpx_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![CPX_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x20;
        cpu.memory.write(0x1234, 0x30);
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpy_immediate_equal() {
        let mut cpu = Cpu::new();
        let program = vec![CPY_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpy_immediate_greater() {
        let mut cpu = Cpu::new();
        let program = vec![CPY_IMM, 0x30, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x50;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpy_immediate_less() {
        let mut cpu = Cpu::new();
        let program = vec![CPY_IMM, 0x50, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x30;
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpy_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![CPY_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x80;
        cpu.memory.write(0x42, 0x80);
        cpu.run();
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cpy_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![CPY_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.write(0x1234, 0x30);
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dec_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x50);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0x4F);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_zero_page_zero() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x01);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_zero_page_negative() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x00);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0xFF);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dec_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0x80);
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0x7F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0x30);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0x2F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![DEC_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0x90);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0x8F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_eor_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_IMM, 0b1111_0000, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.run();
        assert_eq!(cpu.a, 0b0101_1010);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_eor_immediate_zero() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_IMM, 0b1010_1010, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.run();
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_eor_immediate_negative() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_IMM, 0b1111_0000, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b0101_0101;
        cpu.run();
        assert_eq!(cpu.a, 0b1010_0101);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_eor_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.memory.write(0x42, 0x0F);
        cpu.run();
        assert_eq!(cpu.a, 0xF0);
    }

    #[test]
    fn test_eor_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0x55);
        cpu.run();
        assert_eq!(cpu.a, 0xAA);
    }

    #[test]
    fn test_eor_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x12;
        cpu.memory.write(0x1234, 0x34);
        cpu.run();
        assert_eq!(cpu.a, 0x26);
    }

    #[test]
    fn test_eor_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xAA;
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0x55);
        cpu.run();
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_eor_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_ABSY, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xF0;
        cpu.y = 0x20;
        cpu.memory.write(0x1254, 0x0F);
        cpu.run();
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_eor_indexed_indirect() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_INDX, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1100_0011;
        cpu.x = 0x04;
        cpu.memory.write(0x24, 0x74);
        cpu.memory.write(0x25, 0x20);
        cpu.memory.write(0x2074, 0b0011_1100);
        cpu.run();
        assert_eq!(cpu.a, 0b1111_1111);
    }

    #[test]
    fn test_eor_indirect_indexed() {
        let mut cpu = Cpu::new();
        let program = vec![EOR_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b1010_0101;
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28);
        cpu.memory.write(0x87, 0x40);
        cpu.memory.write(0x4038, 0b0101_1010);
        cpu.run();
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_clc() {
        let mut cpu = Cpu::new();
        let program = vec![CLC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = FLAG_CARRY;
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_cld() {
        let mut cpu = Cpu::new();
        let program = vec![CLD, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = FLAG_DECIMAL;
        cpu.run();
        assert_eq!(cpu.p & FLAG_DECIMAL, 0);
    }

    #[test]
    fn test_cli() {
        let mut cpu = Cpu::new();
        let program = vec![CLI, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = FLAG_INTERRUPT;
        cpu.run();
        assert_eq!(cpu.p & FLAG_INTERRUPT, 0);
    }

    #[test]
    fn test_clv() {
        let mut cpu = Cpu::new();
        let program = vec![CLV, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = FLAG_OVERFLOW;
        cpu.run();
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
    }

    #[test]
    fn test_sec() {
        let mut cpu = Cpu::new();
        let program = vec![SEC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_sed() {
        let mut cpu = Cpu::new();
        let program = vec![SED, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.p & FLAG_DECIMAL, FLAG_DECIMAL);
    }

    #[test]
    fn test_sei() {
        let mut cpu = Cpu::new();
        let program = vec![SEI, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);
    }

    #[test]
    fn test_inc_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x50);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0x51);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_zero_page_zero() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0xFF);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_zero_page_negative() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x7F);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0x80);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_inc_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0x20);
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0x21);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0x30);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0x31);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![INC_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0x8F);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0x90);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_jmp_absolute() {
        let mut cpu = Cpu::new();
        cpu.load_program(&vec![], 0x8000);
        cpu.reset();
        cpu.memory.write(0x8000, JMP_ABS);
        cpu.memory.write(0x8001, 0x34);
        cpu.memory.write(0x8002, 0x12);
        cpu.memory.write(0x1234, BRK);
        cpu.pc = 0x8000;
        cpu.run();
        assert_eq!(cpu.pc, 0x1235); // PC after BRK at 0x1234
    }

    #[test]
    fn test_jmp_indirect() {
        let mut cpu = Cpu::new();
        cpu.load_program(&vec![], 0x8000);
        cpu.reset();
        cpu.memory.write(0x8000, JMP_IND);
        cpu.memory.write(0x8001, 0x20);
        cpu.memory.write(0x8002, 0x40);
        cpu.memory.write(0x4020, 0x56);
        cpu.memory.write(0x4021, 0x78);
        cpu.memory.write(0x7856, BRK);
        cpu.pc = 0x8000;
        cpu.run();
        assert_eq!(cpu.pc, 0x7857); // PC after BRK at 0x7856
    }

    #[test]
    fn test_jmp_indirect_page_boundary_bug() {
        // The 6502 has a bug where if the indirect address is on a page boundary
        // (e.g., 0x10FF), it doesn't cross the page boundary to read the high byte
        // Instead of reading from 0x1100, it wraps around to 0x1000
        let mut cpu = Cpu::new();
        cpu.load_program(&vec![], 0x8000);
        cpu.reset();
        cpu.memory.write(0x8000, JMP_IND);
        cpu.memory.write(0x8001, 0xFF);
        cpu.memory.write(0x8002, 0x10);
        cpu.memory.write(0x10FF, 0x34);
        cpu.memory.write(0x1000, 0x12); // Wraps to start of page, not 0x1100
        cpu.memory.write(0x1234, BRK);
        cpu.pc = 0x8000;
        cpu.run();
        assert_eq!(cpu.pc, 0x1235); // Should jump to 0x1234 (low=0x34, high=0x12)
    }

    #[test]
    fn test_jsr() {
        let mut cpu = Cpu::new();
        cpu.load_program(&vec![], 0x8000);
        cpu.reset();
        cpu.memory.write(0x8000, JSR);
        cpu.memory.write(0x8001, 0x34);
        cpu.memory.write(0x8002, 0x12);
        cpu.memory.write(0x1234, BRK);
        cpu.pc = 0x8000;
        cpu.sp = 0xFF;
        cpu.run();
        assert_eq!(cpu.pc, 0x1235); // PC after BRK at 0x1234
        assert_eq!(cpu.sp, 0xFD); // SP decremented by 2 (pushed 2 bytes)
        // Return address should be 0x8002 (address of last byte of JSR instruction)
        assert_eq!(cpu.memory.read(0x01FF), 0x80); // High byte of return address
        assert_eq!(cpu.memory.read(0x01FE), 0x02); // Low byte of return address
    }

    #[test]
    fn test_lda_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_lda_immediate_zero() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_IMM, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_lda_immediate_negative() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_IMM, 0x80, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_lda_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x55);
        cpu.run();
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn test_lda_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0xAA);
        cpu.run();
        assert_eq!(cpu.a, 0xAA);
    }

    #[test]
    fn test_lda_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0x77);
        cpu.run();
        assert_eq!(cpu.a, 0x77);
    }

    #[test]
    fn test_lda_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0x88);
        cpu.run();
        assert_eq!(cpu.a, 0x88);
    }

    #[test]
    fn test_lda_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_ABSY, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.write(0x1254, 0x99);
        cpu.run();
        assert_eq!(cpu.a, 0x99);
    }

    #[test]
    fn test_lda_indexed_indirect() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_INDX, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x04;
        cpu.memory.write(0x24, 0x74);
        cpu.memory.write(0x25, 0x20);
        cpu.memory.write(0x2074, 0xCC);
        cpu.run();
        assert_eq!(cpu.a, 0xCC);
    }

    #[test]
    fn test_lda_indirect_indexed() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28);
        cpu.memory.write(0x87, 0x40);
        cpu.memory.write(0x4038, 0xDD);
        cpu.run();
        assert_eq!(cpu.a, 0xDD);
    }

    #[test]
    fn test_ldx_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.x, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ldx_immediate_zero() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_IMM, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ldx_immediate_negative() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_IMM, 0x80, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.x, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ldx_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x55);
        cpu.run();
        assert_eq!(cpu.x, 0x55);
    }

    #[test]
    fn test_ldx_zero_page_y() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_ZPY, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x05;
        cpu.memory.write(0x47, 0xAA);
        cpu.run();
        assert_eq!(cpu.x, 0xAA);
    }

    #[test]
    fn test_ldx_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0x77);
        cpu.run();
        assert_eq!(cpu.x, 0x77);
    }

    #[test]
    fn test_ldx_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![LDX_ABSY, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.write(0x1254, 0x99);
        cpu.run();
        assert_eq!(cpu.x, 0x99);
    }

    #[test]
    fn test_ldy_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.y, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ldy_immediate_zero() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_IMM, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.y, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ldy_immediate_negative() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_IMM, 0x80, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.y, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ldy_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0x55);
        cpu.run();
        assert_eq!(cpu.y, 0x55);
    }

    #[test]
    fn test_ldy_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0xAA);
        cpu.run();
        assert_eq!(cpu.y, 0xAA);
    }

    #[test]
    fn test_ldy_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0x77);
        cpu.run();
        assert_eq!(cpu.y, 0x77);
    }

    #[test]
    fn test_ldy_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![LDY_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0x88);
        cpu.run();
        assert_eq!(cpu.y, 0x88);
    }

    #[test]
    fn test_lsr_accumulator() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10110101;
        cpu.run();
        assert_eq!(cpu.a, 0b01011010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_lsr_accumulator_zero() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b00000001;
        cpu.run();
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_lsr_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0b11001100);
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0b01100110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_lsr_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b10101011);
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0b01010101);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_lsr_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0b01010100);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0b00101010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_lsr_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![LSR_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b00000011);
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0b00000001);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_nop() {
        let mut cpu = Cpu::new();
        let program = vec![NOP, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x33;
        cpu.y = 0x24;
        cpu.p = 0xFF;
        cpu.run();
        // NOP should not affect any registers or flags
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.x, 0x33);
        assert_eq!(cpu.y, 0x24);
        assert_eq!(cpu.p, 0xFF);
    }

    #[test]
    fn test_ora_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_IMM, 0b01010101, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10101010;
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ora_immediate_zero() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_IMM, 0x00, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x00;
        cpu.run();
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ora_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.memory.write(0x42, 0b00001111);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10000000;
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b01000000);
        cpu.run();
        assert_eq!(cpu.a, 0b11000000);
    }

    #[test]
    fn test_ora_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b00110011;
        cpu.memory.write(0x1234, 0b11001100);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b00001111;
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b11110000);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_ABSY, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.y = 0x20;
        cpu.memory.write(0x1254, 0b10101010);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_indexed_indirect() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_INDX, 0x82, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b00110011;
        cpu.x = 0x04;
        cpu.memory.write(0x86, 0x34);
        cpu.memory.write(0x87, 0x12);
        cpu.memory.write(0x1234, 0b11001100);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_indirect_indexed() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10101010;
        cpu.y = 0x10;
        cpu.memory.write(0x86, 0x28);
        cpu.memory.write(0x87, 0x40);
        cpu.memory.write(0x4038, 0b01010101);
        cpu.run();
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_negative_flag() {
        let mut cpu = Cpu::new();
        let program = vec![ORA_IMM, 0x80, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x00;
        cpu.run();
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_dex() {
        let mut cpu = Cpu::new();
        let program = vec![DEX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.x, 0x41);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dex_zero() {
        let mut cpu = Cpu::new();
        let program = vec![DEX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x01;
        cpu.run();
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_dex_wrap() {
        let mut cpu = Cpu::new();
        let program = vec![DEX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x00;
        cpu.run();
        assert_eq!(cpu.x, 0xFF);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dey() {
        let mut cpu = Cpu::new();
        let program = vec![DEY, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.y, 0x41);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inx() {
        let mut cpu = Cpu::new();
        let program = vec![INX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.x, 0x43);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inx_wrap() {
        let mut cpu = Cpu::new();
        let program = vec![INX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0xFF;
        cpu.run();
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_iny() {
        let mut cpu = Cpu::new();
        let program = vec![INY, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.y, 0x43);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tax() {
        let mut cpu = Cpu::new();
        let program = vec![TAX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.run();
        assert_eq!(cpu.x, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tax_zero() {
        let mut cpu = Cpu::new();
        let program = vec![TAX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x00;
        cpu.run();
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_tax_negative() {
        let mut cpu = Cpu::new();
        let program = vec![TAX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x80;
        cpu.run();
        assert_eq!(cpu.x, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_tay() {
        let mut cpu = Cpu::new();
        let program = vec![TAY, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.run();
        assert_eq!(cpu.y, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_txa() {
        let mut cpu = Cpu::new();
        let program = vec![TXA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tya() {
        let mut cpu = Cpu::new();
        let program = vec![TYA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rol_accumulator() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10110101;
        cpu.p = 0; // Clear carry
        cpu.run();
        assert_eq!(cpu.a, 0b01101010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rol_accumulator_with_carry() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.p = FLAG_CARRY; // Set carry
        cpu.run();
        assert_eq!(cpu.a, 0b10101011);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_rol_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0b11001100);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0b10011000);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rol_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b10101011);
        cpu.p = FLAG_CARRY;
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0b01010111);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rol_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0b01010100);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0b10101000);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_rol_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![ROL_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b00000011);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0b00000110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_accumulator() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b10110101;
        cpu.p = 0; // Clear carry
        cpu.run();
        assert_eq!(cpu.a, 0b01011010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ror_accumulator_with_carry() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ACC, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.p = FLAG_CARRY; // Set carry
        cpu.run();
        assert_eq!(cpu.a, 0b10101010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ror_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x42, 0b11001100);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x42), 0b01100110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.write(0x47, 0b10101011);
        cpu.p = FLAG_CARRY;
        cpu.run();
        assert_eq!(cpu.memory.read(0x47), 0b11010101);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_ror_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.memory.write(0x1234, 0b01010100);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x1234), 0b00101010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![ROR_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.write(0x1244, 0b00000011);
        cpu.p = 0;
        cpu.run();
        assert_eq!(cpu.memory.read(0x1244), 0b00000001);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rti() {
        let mut cpu = Cpu::new();
        let program = vec![RTI, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        // Set up stack with saved processor status and return address
        cpu.sp = 0xFC;
        cpu.memory.write(0x01FD, 0b11010011); // Saved status flags
        cpu.memory.write(0x01FE, 0x34); // PC low byte
        cpu.memory.write(0x01FF, 0x12); // PC high byte
        cpu.memory.write(0x1234, BRK); // BRK at return address
        cpu.run();
        assert_eq!(cpu.p, 0b11010011);
        assert_eq!(cpu.pc, 0x1235); // PC after BRK instruction
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_rts() {
        let mut cpu = Cpu::new();
        let program = vec![RTS, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        // Set up stack with saved return address (PC-1)
        cpu.sp = 0xFD;
        cpu.memory.write(0x01FE, 0x33); // PC-1 low byte (0x1233)
        cpu.memory.write(0x01FF, 0x12); // PC-1 high byte
        cpu.memory.write(0x1234, BRK); // BRK at return address
        cpu.run();
        assert_eq!(cpu.pc, 0x1235); // PC after BRK instruction (0x1234 + 1)
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_sbc_immediate() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_IMM, 0x30, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        cpu.run();
        assert_eq!(cpu.a, 0x20);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_sbc_immediate_with_borrow() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_IMM, 0x30, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p &= !FLAG_CARRY; // Clear carry (borrow)
        cpu.run();
        assert_eq!(cpu.a, 0x1F);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_sbc_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_ZP, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x80;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x42, 0x40);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_ZPX, 0x42, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50;
        cpu.x = 0x05;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x47, 0x10);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_ABS, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x60;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x1234, 0x20);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_ABSX, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x70;
        cpu.x = 0x10;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x1244, 0x30);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_ABSY, 0x34, 0x12, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x90;
        cpu.y = 0x20;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x1254, 0x50);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_indexed_indirect() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_INDX, 0x82, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xA0;
        cpu.x = 0x04;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x86, 0x34);
        cpu.memory.write(0x87, 0x12);
        cpu.memory.write(0x1234, 0x60);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_indirect_indexed() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_INDY, 0x86, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0xB0;
        cpu.y = 0x10;
        cpu.p |= FLAG_CARRY;
        cpu.memory.write(0x86, 0x28);
        cpu.memory.write(0x87, 0x40);
        cpu.memory.write(0x4038, 0x70);
        cpu.run();
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_overflow() {
        let mut cpu = Cpu::new();
        let program = vec![SBC_IMM, 0xB0, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p |= FLAG_CARRY;
        cpu.run();
        assert_eq!(cpu.a, 0xA0);
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_sta_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![STA_ZP, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x10), 0x42);
    }

    #[test]
    fn test_sta_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![STA_ZPX, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        cpu.run();
        assert_eq!(cpu.memory.read(0x15), 0x42);
    }

    #[test]
    fn test_sta_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![STA_ABS, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x2000), 0x42);
    }

    #[test]
    fn test_sta_absolute_x() {
        let mut cpu = Cpu::new();
        let program = vec![STA_ABSX, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        cpu.run();
        assert_eq!(cpu.memory.read(0x2005), 0x42);
    }

    #[test]
    fn test_sta_absolute_y() {
        let mut cpu = Cpu::new();
        let program = vec![STA_ABSY, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.y = 0x05;
        cpu.run();
        assert_eq!(cpu.memory.read(0x2005), 0x42);
    }

    #[test]
    fn test_sta_indexed_indirect() {
        let mut cpu = Cpu::new();
        let program = vec![STA_INDX, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        cpu.memory.write(0x15, 0x00);
        cpu.memory.write(0x16, 0x20);
        cpu.run();
        assert_eq!(cpu.memory.read(0x2000), 0x42);
    }

    #[test]
    fn test_sta_indirect_indexed() {
        let mut cpu = Cpu::new();
        let program = vec![STA_INDY, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.y = 0x05;
        cpu.memory.write(0x10, 0x00);
        cpu.memory.write(0x11, 0x20);
        cpu.run();
        assert_eq!(cpu.memory.read(0x2005), 0x42);
    }

    #[test]
    fn test_txs() {
        let mut cpu = Cpu::new();
        let program = vec![TXS, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0xFF;
        cpu.run();
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_tsx() {
        let mut cpu = Cpu::new();
        let program = vec![TSX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0xAB;
        cpu.run();
        assert_eq!(cpu.x, 0xAB);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_tsx_zero_flag() {
        let mut cpu = Cpu::new();
        let program = vec![TSX, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0x00;
        cpu.run();
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_pha() {
        let mut cpu = Cpu::new();
        let program = vec![PHA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.a = 0x42;
        cpu.sp = 0xFD;
        cpu.run();
        assert_eq!(cpu.sp, 0xFC);
        assert_eq!(cpu.memory.read(0x01FD), 0x42);
    }

    #[test]
    fn test_pla() {
        let mut cpu = Cpu::new();
        let program = vec![PLA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.write(0x01FD, 0x42);
        cpu.run();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.sp, 0xFD);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_pla_zero_flag() {
        let mut cpu = Cpu::new();
        let program = vec![PLA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.write(0x01FD, 0x00);
        cpu.run();
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_pla_negative_flag() {
        let mut cpu = Cpu::new();
        let program = vec![PLA, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.write(0x01FD, 0x80);
        cpu.run();
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_php() {
        let mut cpu = Cpu::new();
        let program = vec![PHP, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.p = 0xFF;
        cpu.sp = 0xFD;
        cpu.run();
        assert_eq!(cpu.sp, 0xFC);
        assert_eq!(cpu.memory.read(0x01FD), 0xFF);
    }

    #[test]
    fn test_plp() {
        let mut cpu = Cpu::new();
        let program = vec![PLP, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.write(0x01FD, 0xC3);
        cpu.run();
        assert_eq!(cpu.p, 0xC3);
        assert_eq!(cpu.sp, 0xFD);
    }

    #[test]
    fn test_stx_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![STX_ZP, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x10), 0x42);
    }

    #[test]
    fn test_stx_zero_page_y() {
        let mut cpu = Cpu::new();
        let program = vec![STX_ZPY, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.y = 0x05;
        cpu.run();
        assert_eq!(cpu.memory.read(0x15), 0x42);
    }

    #[test]
    fn test_stx_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![STX_ABS, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.x = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x2000), 0x42);
    }

    #[test]
    fn test_sty_zero_page() {
        let mut cpu = Cpu::new();
        let program = vec![STY_ZP, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x10), 0x42);
    }

    #[test]
    fn test_sty_zero_page_x() {
        let mut cpu = Cpu::new();
        let program = vec![STY_ZPX, 0x10, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.x = 0x05;
        cpu.run();
        assert_eq!(cpu.memory.read(0x15), 0x42);
    }

    #[test]
    fn test_sty_absolute() {
        let mut cpu = Cpu::new();
        let program = vec![STY_ABS, 0x00, 0x20, BRK];
        cpu.load_program(&program, 0x8000);
        cpu.reset();
        cpu.y = 0x42;
        cpu.run();
        assert_eq!(cpu.memory.read(0x2000), 0x42);
    }

    #[test]
    fn test_write_u16_to_addr() {
        let mut cpu = Cpu::new();
        cpu.write_u16_to_addr(0x1234, 0xABCD);
        assert_eq!(cpu.memory.read(0x1234), 0xCD); // Low byte
        assert_eq!(cpu.memory.read(0x1235), 0xAB); // High byte
    }

    #[test]
    fn test_read_u16_from_addr() {
        let mut cpu = Cpu::new();
        cpu.memory.write(0x1234, 0xCD); // Low byte
        cpu.memory.write(0x1235, 0xAB); // High byte
        let result = cpu.read_u16_from_addr(0x1234);
        assert_eq!(result, 0xABCD);
    }

    #[test]
    fn test_write_and_read_u16() {
        let mut cpu = Cpu::new();
        cpu.write_u16_to_addr(0x5000, 0x1234);
        let result = cpu.read_u16_from_addr(0x5000);
        assert_eq!(result, 0x1234);
    }

    #[test]
    fn test_load_program_at_custom_address() {
        let mut cpu = Cpu::new();
        let program = vec![LDA_IMM, 0x42, BRK];
        cpu.load_program(&program, 0x0600);
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.a, 0x42);
        // Verify program was loaded at 0x0600
        assert_eq!(cpu.memory.read(0x0600), LDA_IMM);
        assert_eq!(cpu.memory.read(0x0601), 0x42);
        assert_eq!(cpu.memory.read(0x0602), BRK);
    }
}
