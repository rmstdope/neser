use crate::mem_controller::MemController;
use crate::opcode::*;
use std::cell::RefCell;
use std::rc::Rc;

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
    pub memory: Rc<RefCell<MemController>>,
    /// Halted state (set by KIL instruction)
    pub halted: bool,
    /// Total cycles executed since last reset
    pub total_cycles: u64,
}

// Status register flags
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
const FLAG_DECIMAL: u8 = 0b0000_1000;
const FLAG_BREAK: u8 = 0b0001_0000;
const FLAG_UNUSED: u8 = 0b0010_0000;
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

const NMI_VECTOR: u16 = 0xFFFA;
const RESET_VECTOR: u16 = 0xFFFC;
const IRQ_VECTOR: u16 = 0xFFFE;

impl Cpu {
    /// Create a new CPU with default register values
    pub fn new(memory: Rc<RefCell<MemController>>) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD, // Stack pointer starts at 0xFD
            pc: 0,    // Program counter will be loaded from reset vector
            p: 0x24,  // Status: IRQ disabled, unused bit set
            memory,
            halted: false,
            total_cycles: 0,
        }
    }

    /// Get the total number of cycles executed since last reset
    #[cfg(test)]
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.p = 0x24;
        self.halted = false;
        self.total_cycles = 0;
        self.pc = self.read_reset_vector();
    }

    // /// Load a program and run the CPU emulation
    // pub fn load_and_run(&mut self, program: Vec<u8>) {
    //     self.load_program(&program, 0x0600);
    //     // Reset CPU
    //     self.reset();
    //     self.run();
    // }

    /// Execute a single opcode and return the number of cycles consumed
    pub fn run_opcode(&mut self) -> u8 {
        if self.halted {
            return 0;
        }

        let opcode_byte = self.memory.borrow().read(self.pc);
        self.pc += 1;

        let opcode = crate::opcode::lookup(opcode_byte)
            .unwrap_or_else(|| panic!("Invalid opcode: 0x{:02X}", opcode_byte));
        let mut cycles = opcode.cycles;

        match opcode_byte {
            ADC_IMM => {
                let value = self.read_byte();
                self.adc(value);
            }
            ADC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            ADC_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.adc(value);
            }
            AND_IMM => {
                let value = self.read_byte();
                self.and(value);
            }
            AND_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            AND_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.and(value);
            }
            ASL_A => {
                self.a = self.asl(self.a);
            }
            ASL_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.asl(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ASL_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.asl(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ASL_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                let result = self.asl(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ASL_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.borrow().read(addr);
                let result = self.asl(value);
                self.memory.borrow_mut().write(addr, result);
            }
            BIT_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.bit(value);
            }
            BIT_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.bit(value);
            }
            BCC => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_CARRY == 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BCS => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_CARRY != 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BEQ => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_ZERO != 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BMI => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_NEGATIVE != 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BNE => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_ZERO == 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BPL => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_NEGATIVE == 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BVC => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_OVERFLOW == 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BVS => {
                let offset = self.read_byte() as i8;
                if self.p & FLAG_OVERFLOW != 0 {
                    cycles += 1; // +1 cycle when branch is taken
                    let old_pc = self.pc;
                    self.branch(offset);
                    if Self::page_crossed(old_pc, self.pc) {
                        cycles += 1; // +1 cycle if page boundary crossed
                    }
                }
            }
            BRK => {
                // BRK is a software interrupt instruction
                // Push PC+2 to stack (PC has already been incremented past BRK opcode,
                // so we push PC+1 which points to the byte after BRK's padding byte)
                let return_addr = self.pc.wrapping_add(1);
                self.push_word(return_addr);
                
                // Push P with B flag and unused flag set to distinguish BRK from IRQ
                self.push_byte(self.p | FLAG_BREAK | FLAG_UNUSED);
                
                // Set Interrupt Disable flag to prevent further interrupts
                self.p |= FLAG_INTERRUPT;
                
                // Load PC from IRQ/BRK vector
                self.pc = self.memory.borrow().read_u16(IRQ_VECTOR);
            }
            CMP_IMM => {
                let value = self.read_byte();
                self.cmp(value);
            }
            CMP_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_ZPX => {
                let base = self.read_byte();
                let addr = base.wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_INDX => {
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CMP_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.cmp(value);
            }
            CPX_IMM => {
                let value = self.read_byte();
                self.cpx(value);
            }
            CPX_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.cpx(value);
            }
            CPX_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.cpx(value);
            }
            CPY_IMM => {
                let value = self.read_byte();
                self.cpy(value);
            }
            CPY_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.cpy(value);
            }
            CPY_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.cpy(value);
            }
            DEC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.dec(value);
                self.memory.borrow_mut().write(addr, result);
            }
            DEC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.dec(value);
                self.memory.borrow_mut().write(addr, result);
            }
            DEC_ABS => {
                let addr = self.read_word() as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.dec(value);
                self.memory.borrow_mut().write(addr, result);
            }
            DEC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16) as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.dec(value);
                self.memory.borrow_mut().write(addr, result);
            }
            EOR_IMM => {
                let value = self.read_byte();
                self.eor(value);
            }
            EOR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.eor(value);
            }
            EOR_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
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
                let value = self.memory.borrow().read(addr as u16);
                let result = self.inc(value);
                self.memory.borrow_mut().write(addr, result);
            }
            INC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.inc(value);
                self.memory.borrow_mut().write(addr, result);
            }
            INC_ABS => {
                let addr = self.read_word() as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.inc(value);
                self.memory.borrow_mut().write(addr, result);
            }
            INC_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16) as u16;
                let value = self.memory.borrow().read(addr as u16);
                let result = self.inc(value);
                self.memory.borrow_mut().write(addr, result);
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
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDA_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.lda(value);
            }
            LDX_IMM => {
                let value = self.read_byte();
                self.ldx(value);
            }
            LDX_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.ldx(value);
            }
            LDX_ZPY => {
                let addr = self.read_byte().wrapping_add(self.y) as u16;
                let value = self.memory.borrow().read(addr);
                self.ldx(value);
            }
            LDX_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.ldx(value);
            }
            LDX_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.ldx(value);
            }
            LDY_IMM => {
                let value = self.read_byte();
                self.ldy(value);
            }
            LDY_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.ldy(value);
            }
            LDY_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.ldy(value);
            }
            LDY_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.ldy(value);
            }
            LDY_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.ldy(value);
            }
            LSR_ACC => {
                self.a = self.lsr(self.a);
            }
            LSR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.lsr(value);
                self.memory.borrow_mut().write(addr, result);
            }
            LSR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.lsr(value);
                self.memory.borrow_mut().write(addr, result);
            }
            LSR_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                let result = self.lsr(value);
                self.memory.borrow_mut().write(addr, result);
            }
            LSR_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.borrow().read(addr);
                let result = self.lsr(value);
                self.memory.borrow_mut().write(addr, result);
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
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.ora(value);
            }
            ORA_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
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
                let value = self.memory.borrow().read(addr);
                let result = self.rol(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROL_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.rol(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROL_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                let result = self.rol(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROL_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.borrow().read(addr);
                let result = self.rol(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROR_ACC => {
                self.a = self.ror(self.a);
            }
            ROR_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.ror(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROR_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                let result = self.ror(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROR_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                let result = self.ror(value);
                self.memory.borrow_mut().write(addr, result);
            }
            ROR_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                let value = self.memory.borrow().read(addr);
                let result = self.ror(value);
                self.memory.borrow_mut().write(addr, result);
            }
            RTI => {
                let value = self.pop_byte();
                // RTI behaves like PLP - ignores B flag and unused bit
                // Load bits 0-3 and 6-7 from stack, always set unused bit to 1, clear B flag
                self.p = (value & !(FLAG_BREAK | FLAG_UNUSED)) | FLAG_UNUSED;
                self.pc = self.pop_word();
            }
            RTS => {
                self.pc = self.pop_word();
                self.pc = self.pc.wrapping_add(1);
            }
            SBC_IMM | SBC_IMM2 => {
                // SBC_IMM2 is undocumented but identical to SBC_IMM
                let value = self.read_byte();
                self.sbc(value);
            }
            SBC_ZP => {
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_ABS => {
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_ABSX => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_ABSY => {
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            SBC_INDY => {
                let ptr = self.read_byte();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.sbc(value);
            }
            STA_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.borrow_mut().write(addr, self.a);
            }
            STA_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.memory.borrow_mut().write(addr, self.a);
            }
            STA_ABS => {
                let addr = self.read_word();
                self.memory.borrow_mut().write(addr, self.a);
            }
            STA_ABSX => {
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.memory.borrow_mut().write(addr, self.a);
            }
            SXA_ABSY => {
                // Undocumented: Store X AND (HIGH(addr) + 1) at addr,Y
                let base_addr = self.read_word();
                let addr = base_addr.wrapping_add(self.y as u16);
                let high_byte = (base_addr >> 8) as u8;
                let result = self.x & high_byte.wrapping_add(1);
                self.memory.borrow_mut().write(addr, result);
            }
            SYA_ABSX => {
                // Undocumented: Store Y AND (HIGH(addr) + 1) at addr,X
                let base_addr = self.read_word();
                let addr = base_addr.wrapping_add(self.x as u16);
                let high_byte = (base_addr >> 8) as u8;
                let result = self.y & high_byte.wrapping_add(1);
                self.memory.borrow_mut().write(addr, result);
            }
            STA_ABSY => {
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.memory.borrow_mut().write(addr, self.a);
            }
            STA_INDX => {
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                self.memory.borrow_mut().write(addr, self.a);
            }
            STA_INDY => {
                let ptr = self.read_byte();
                let addr = self.read_word_from_zp(ptr).wrapping_add(self.y as u16);
                self.memory.borrow_mut().write(addr, self.a);
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
                // PHP pushes P with B flag and unused bit set to 1
                self.push_byte(self.p | FLAG_BREAK | FLAG_UNUSED);
            }
            PLP => {
                let value = self.pop_byte();
                // PLP ignores B flag (bit 4) and unused bit (bit 5)
                // Load bits 0-3 and 6-7 from stack, always set unused bit to 1, clear B flag
                self.p = (value & !(FLAG_BREAK | FLAG_UNUSED)) | FLAG_UNUSED;
            }
            STX_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.borrow_mut().write(addr, self.x);
            }
            STX_ZPY => {
                let addr = self.read_byte().wrapping_add(self.y) as u16;
                self.memory.borrow_mut().write(addr, self.x);
            }
            STX_ABS => {
                let addr = self.read_word();
                self.memory.borrow_mut().write(addr, self.x);
            }
            STY_ZP => {
                let addr = self.read_byte() as u16;
                self.memory.borrow_mut().write(addr, self.y);
            }
            STY_ZPX => {
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.memory.borrow_mut().write(addr, self.y);
            }
            STY_ABS => {
                let addr = self.read_word();
                self.memory.borrow_mut().write(addr, self.y);
            }
            // Undocumented opcodes (alphabetical order)
            AAC_IMM | AAC_IMM2 => {
                // Undocumented: AND with accumulator, then copy bit 7 to carry
                let value = self.read_byte();
                self.and(value);
                let carry = if self.a & 0x80 != 0 { FLAG_CARRY } else { 0 };
                self.p = (self.p & !FLAG_CARRY) | carry;
            }
            SAX_INDX => {
                // Undocumented: Store A AND X
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                let value = self.a & self.x;
                self.memory.borrow_mut().write(addr, value);
            }
            SAX_ZP => {
                // Undocumented: Store A AND X
                let addr = self.read_byte() as u16;
                let value = self.a & self.x;
                self.memory.borrow_mut().write(addr, value);
            }
            SAX_ZPY => {
                // Undocumented: Store A AND X
                let addr = self.read_byte().wrapping_add(self.y) as u16;
                let value = self.a & self.x;
                self.memory.borrow_mut().write(addr, value);
            }
            SAX_ABS => {
                // Undocumented: Store A AND X
                let addr = self.read_word();
                let value = self.a & self.x;
                self.memory.borrow_mut().write(addr, value);
            }
            ARR_IMM => {
                // Undocumented: AND then rotate right
                let value = self.read_byte();
                self.a &= value;

                // Rotate right using current carry flag
                let old_carry = if self.p & FLAG_CARRY != 0 { 0x80 } else { 0 };
                let new_carry = if self.a & 0x01 != 0 { FLAG_CARRY } else { 0 };

                self.a = (self.a >> 1) | old_carry;

                // Set carry from bit 0 of AND result
                self.p = (self.p & !FLAG_CARRY) | new_carry;

                // Set overflow to bit 6 XOR bit 5 of result
                let bit6 = (self.a >> 6) & 1;
                let bit5 = (self.a >> 5) & 1;
                let overflow = if bit6 ^ bit5 != 0 { FLAG_OVERFLOW } else { 0 };
                self.p = (self.p & !FLAG_OVERFLOW) | overflow;

                self.update_zero_and_negative_flags(self.a);
            }
            ASR_IMM => {
                // Undocumented: AND then logical shift right (LSR)
                let value = self.read_byte();
                self.a &= value;

                // Set carry from bit 0 before shift
                let carry = if self.a & 0x01 != 0 { FLAG_CARRY } else { 0 };
                self.p = (self.p & !FLAG_CARRY) | carry;

                // Logical shift right (no carry involved in shift)
                self.a >>= 1;

                self.update_zero_and_negative_flags(self.a);
            }
            ATX_IMM => {
                // Undocumented: AND then transfer to both A and X
                let value = self.read_byte();
                self.a &= value;
                self.x = self.a;
                self.update_zero_and_negative_flags(self.a);
            }
            AXA_INDY => {
                // Undocumented: Store A AND X AND (high byte of address + 1)
                let ptr = self.read_byte();
                let base_addr = self.read_word_from_zp(ptr);
                let addr = base_addr.wrapping_add(self.y as u16);
                let high_byte = (addr >> 8) as u8;
                let value = self.a & self.x & high_byte.wrapping_add(1);
                self.memory.borrow_mut().write(addr, value);
            }
            AXA_ABSY => {
                // Undocumented: Store A AND X AND (high byte of address + 1)
                let base_addr = self.read_word();
                let addr = base_addr.wrapping_add(self.y as u16);
                let high_byte = (addr >> 8) as u8;
                let value = self.a & self.x & high_byte.wrapping_add(1);
                self.memory.borrow_mut().write(addr, value);
            }
            AXS_IMM => {
                // Undocumented: AND X with A, then subtract immediate (without borrow)
                let value = self.read_byte();
                let temp = self.a & self.x;
                let result = temp.wrapping_sub(value);

                // Set carry flag (like CMP: set if no borrow, clear if borrow)
                self.p = (self.p & !FLAG_CARRY) | if temp >= value { FLAG_CARRY } else { 0 };

                self.x = result;
                self.update_zero_and_negative_flags(self.x);
            }
            DCP_INDX => {
                // Undocumented: Decrement memory then compare with A
                let ptr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(ptr);
                self.dcp(addr);
            }
            DCP_ZP => {
                // Undocumented: Decrement memory then compare with A
                let addr = self.read_byte() as u16;
                self.dcp(addr);
            }
            DCP_ABS => {
                // Undocumented: Decrement memory then compare with A
                let addr = self.read_word();
                self.dcp(addr);
            }
            DCP_INDY => {
                // Undocumented: Decrement memory then compare with A
                let ptr = self.read_byte();
                let base_addr = self.read_word_from_zp(ptr);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.dcp(addr);
            }
            DCP_ZPX => {
                // Undocumented: Decrement memory then compare with A
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.dcp(addr);
            }
            DCP_ABSY => {
                // Undocumented: Decrement memory then compare with A
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.dcp(addr);
            }
            DCP_ABSX => {
                // Undocumented: Decrement memory then compare with A
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.dcp(addr);
            }
            DOP_ZP | DOP_ZP2 | DOP_ZP3 | DOP_ZPX | DOP_ZPX2 | DOP_ZPX3 | DOP_ZPX4 | DOP_ZPX5
            | DOP_ZPX6 | DOP_IMM | DOP_IMM2 | DOP_IMM3 | DOP_IMM4 | DOP_IMM5 => {
                // Undocumented: Double NOP - read operand byte and discard
                let _ = self.read_byte();
            }
            ISB_INDX => {
                // Undocumented: Increment memory then subtract from A with borrow
                let zp_addr = self.read_byte().wrapping_add(self.x);
                let addr_lo = self.memory.borrow().read(zp_addr as u16);
                let addr_hi = self.memory.borrow().read(zp_addr.wrapping_add(1) as u16);
                let addr = u16::from_le_bytes([addr_lo, addr_hi]);
                self.isc(addr);
            }
            ISB_ZP => {
                // Undocumented: Increment memory then subtract from A with borrow
                let addr = self.read_byte() as u16;
                self.isc(addr);
            }
            ISB_ABS => {
                // Undocumented: Increment memory then subtract from A with borrow
                let addr = self.read_word();
                self.isc(addr);
            }
            ISB_INDY => {
                // Undocumented: Increment memory then subtract from A with borrow
                let zp_addr = self.read_byte();
                let addr_lo = self.memory.borrow().read(zp_addr as u16);
                let addr_hi = self.memory.borrow().read(zp_addr.wrapping_add(1) as u16);
                let base_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.isc(addr);
            }
            ISB_ZPX => {
                // Undocumented: Increment memory then subtract from A with borrow
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.isc(addr);
            }
            ISB_ABSY => {
                // Undocumented: Increment memory then subtract from A with borrow
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.isc(addr);
            }
            ISB_ABSX => {
                // Undocumented: Increment memory then subtract from A with borrow
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.isc(addr);
            }
            KIL | KIL2 | KIL3 | KIL4 | KIL5 | KIL6 | KIL7 | KIL8 | KIL9 | KIL10 | KIL11 | KIL12 => {
                // Undocumented: Halt the processor
                self.halted = true;
                return cycles;
            }
            LAR_ABSY => {
                // Undocumented: AND memory with stack pointer, store in A, X, and SP
                let addr = self.read_word().wrapping_add(self.y as u16);
                let value = self.memory.borrow().read(addr);
                let result = self.sp & value;
                self.a = result;
                self.x = result;
                self.sp = result;
                // Set flags based on result
                if result == 0 {
                    self.p |= FLAG_ZERO;
                } else {
                    self.p &= !FLAG_ZERO;
                }
                if result & 0x80 != 0 {
                    self.p |= FLAG_NEGATIVE;
                } else {
                    self.p &= !FLAG_NEGATIVE;
                }
            }
            LAX_INDX => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let base = self.read_byte();
                let ptr = base.wrapping_add(self.x);
                let lo = self.memory.borrow().read(ptr as u16) as u16;
                let hi = self.memory.borrow().read(ptr.wrapping_add(1) as u16) as u16;
                let addr = (hi << 8) | lo;
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            LAX_ZP => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let addr = self.read_byte() as u16;
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            LAX_ABS => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let addr = self.read_word();
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            LAX_INDY => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let ptr = self.read_byte() as u16;
                let lo = self.memory.borrow().read(ptr) as u16;
                let hi = self.memory.borrow().read((ptr + 1) & 0xFF) as u16;
                let base = (hi << 8) | lo;
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            LAX_ZPY => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let base = self.read_byte();
                let addr = base.wrapping_add(self.y) as u16;
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            LAX_ABSY => {
                // Undocumented: Load A and X with memory value (LDA + LDX)
                let base = self.read_word();
                let addr = base.wrapping_add(self.y as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                let value = self.memory.borrow().read(addr);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(value);
            }
            NOP_IMP | NOP_IMP2 | NOP_IMP3 | NOP_IMP4 | NOP_IMP5 | NOP_IMP6 => {
                // Undocumented: No operation (same as official NOP)
                // Do nothing
            }
            RLA_INDX => {
                // Undocumented: ROL memory, then AND with accumulator (Indirect,X)
                let zp_addr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(zp_addr);
                self.rla(addr);
            }
            RLA_ZP => {
                // Undocumented: ROL memory, then AND with accumulator (Zero Page)
                let addr = self.read_byte() as u16;
                self.rla(addr);
            }
            RLA_ABS => {
                // Undocumented: ROL memory, then AND with accumulator (Absolute)
                let addr = self.read_word();
                self.rla(addr);
            }
            RLA_INDY => {
                // Undocumented: ROL memory, then AND with accumulator (Indirect,Y)
                let zp_addr = self.read_byte();
                let base_addr = self.read_word_from_zp(zp_addr);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.rla(addr);
            }
            RLA_ZPX => {
                // Undocumented: ROL memory, then AND with accumulator (Zero Page,X)
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.rla(addr);
            }
            RLA_ABSY => {
                // Undocumented: ROL memory, then AND with accumulator (Absolute,Y)
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.rla(addr);
            }
            RLA_ABSX => {
                // Undocumented: ROL memory, then AND with accumulator (Absolute,X)
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.rla(addr);
            }
            RRA_INDX => {
                // Undocumented: ROR memory, then ADC with accumulator (Indirect,X)
                let zp_addr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(zp_addr);
                self.rra(addr);
            }
            RRA_ZP => {
                // Undocumented: ROR memory, then ADC with accumulator (Zero Page)
                let addr = self.read_byte() as u16;
                self.rra(addr);
            }
            RRA_ABS => {
                // Undocumented: ROR memory, then ADC with accumulator (Absolute)
                let addr = self.read_word();
                self.rra(addr);
            }
            RRA_INDY => {
                // Undocumented: ROR memory, then ADC with accumulator (Indirect,Y)
                let zp_addr = self.read_byte();
                let base_addr = self.read_word_from_zp(zp_addr);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.rra(addr);
            }
            RRA_ZPX => {
                // Undocumented: ROR memory, then ADC with accumulator (Zero Page,X)
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.rra(addr);
            }
            RRA_ABSY => {
                // Undocumented: ROR memory, then ADC with accumulator (Absolute,Y)
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.rra(addr);
            }
            RRA_ABSX => {
                // Undocumented: ROR memory, then ADC with accumulator (Absolute,X)
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.rra(addr);
            }
            SLO_INDX => {
                // Undocumented: ASL memory, then ORA with accumulator (Indirect,X)
                let zp_addr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(zp_addr);
                self.slo(addr);
            }
            SLO_ZP => {
                // Undocumented: ASL memory, then ORA with accumulator (Zero Page)
                let addr = self.read_byte() as u16;
                self.slo(addr);
            }
            SLO_ABS => {
                // Undocumented: ASL memory, then ORA with accumulator (Absolute)
                let addr = self.read_word();
                self.slo(addr);
            }
            SLO_INDY => {
                // Undocumented: ASL memory, then ORA with accumulator (Indirect,Y)
                let zp_addr = self.read_byte();
                let base_addr = self.read_word_from_zp(zp_addr);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.slo(addr);
            }
            SLO_ZPX => {
                // Undocumented: ASL memory, then ORA with accumulator (Zero Page,X)
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.slo(addr);
            }
            SLO_ABSY => {
                // Undocumented: ASL memory, then ORA with accumulator (Absolute,Y)
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.slo(addr);
            }
            SLO_ABSX => {
                // Undocumented: ASL memory, then ORA with accumulator (Absolute,X)
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.slo(addr);
            }
            SRE_INDX => {
                // Undocumented: LSR memory, then EOR with accumulator (Indirect,X)
                let zp_addr = self.read_byte().wrapping_add(self.x);
                let addr = self.read_word_from_zp(zp_addr);
                self.sre(addr);
            }
            SRE_ZP => {
                // Undocumented: LSR memory, then EOR with accumulator (Zero Page)
                let addr = self.read_byte() as u16;
                self.sre(addr);
            }
            SRE_ABS => {
                // Undocumented: LSR memory, then EOR with accumulator (Absolute)
                let addr = self.read_word();
                self.sre(addr);
            }
            SRE_INDY => {
                // Undocumented: LSR memory, then EOR with accumulator (Indirect,Y)
                let zp_addr = self.read_byte();
                let base_addr = self.read_word_from_zp(zp_addr);
                let addr = base_addr.wrapping_add(self.y as u16);
                self.sre(addr);
            }
            SRE_ZPX => {
                // Undocumented: LSR memory, then EOR with accumulator (Zero Page,X)
                let addr = self.read_byte().wrapping_add(self.x) as u16;
                self.sre(addr);
            }
            SRE_ABSY => {
                // Undocumented: LSR memory, then EOR with accumulator (Absolute,Y)
                let addr = self.read_word().wrapping_add(self.y as u16);
                self.sre(addr);
            }
            SRE_ABSX => {
                // Undocumented: LSR memory, then EOR with accumulator (Absolute,X)
                let addr = self.read_word().wrapping_add(self.x as u16);
                self.sre(addr);
            }
            TOP_ABS => {
                // Undocumented: Triple NOP - 3-byte no operation (absolute addressing)
                self.read_word(); // Read and discard the 2-byte argument
            }
            TOP_ABSX | TOP_ABSX2 | TOP_ABSX3 | TOP_ABSX4 | TOP_ABSX5 | TOP_ABSX6 => {
                // Undocumented: Triple NOP - 3-byte no operation (absolute,X addressing)
                let base = self.read_word();
                let addr = base.wrapping_add(self.x as u16);
                if Self::page_crossed(base, addr) {
                    cycles += 1;
                }
                // Note: Real hardware performs a dummy read at addr, but we skip it to avoid
                // issues with reading from write-only registers or unmapped memory
            }
            XAA_IMM => {
                // Undocumented: Highly unstable opcode
                // A = (A | MAGIC) & X & immediate
                // MAGIC constant is typically 0xEE on most CPUs
                let value = self.read_byte();
                const MAGIC: u8 = 0xEE;
                self.a = (self.a | MAGIC) & self.x & value;
                self.update_zero_and_negative_flags(self.a);
            }
            XAS_ABSY => {
                // Undocumented: Store A AND X in SP, then store SP AND (HIGH(addr) + 1) at addr,Y
                let base_addr = self.read_word();
                let addr = base_addr.wrapping_add(self.y as u16);
                self.sp = self.a & self.x;
                let high_byte = (base_addr >> 8) as u8;
                let result = self.sp & high_byte.wrapping_add(1);
                self.memory.borrow_mut().write(addr, result);
            }
        }

        self.total_cycles += cycles as u64;
        cycles
    }

    /// Check if two addresses are on different pages
    fn page_crossed(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    /// Read a byte from memory at PC and increment PC
    fn read_byte(&mut self) -> u8 {
        let value = self.memory.borrow().read(self.pc);
        self.pc += 1;
        value
    }

    /// Read a 16-bit word from memory at PC (little-endian) and increment PC
    fn read_word(&mut self) -> u16 {
        let lo = self.read_byte() as u16;
        let hi = self.read_byte() as u16;
        (hi << 8) | lo
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&self) -> u16 {
        self.memory.borrow().read_u16(RESET_VECTOR)
    }

    /// Read a 16-bit word from zero page (wraps at page boundary)
    fn read_word_from_zp(&self, addr: u8) -> u16 {
        let lo = self.memory.borrow().read(addr as u16) as u16;
        let hi = self.memory.borrow().read(addr.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }

    /// Read a word from an indirect address with 6502 page boundary bug
    /// If the address is at a page boundary (e.g., 0x10FF), the high byte
    /// is read from the start of the same page (0x1000) instead of the next page (0x1100)
    fn read_word_indirect(&self, addr: u16) -> u16 {
        let lo = self.memory.borrow().read(addr) as u16;
        let hi_addr = if addr & 0xFF == 0xFF {
            // Page boundary bug: wrap within the same page
            addr & 0xFF00
        } else {
            addr + 1
        };
        let hi = self.memory.borrow().read(hi_addr) as u16;
        (hi << 8) | lo
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.sp as u16);
        self.memory.borrow_mut().write(addr, value);
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
        self.memory.borrow().read(addr)
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

    /// Decrement and Compare - DCP undocumented operation
    fn dcp(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let result = self.dec(value);
        self.memory.borrow_mut().write(addr, result);
        self.cmp(result);
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

    /// ISC - Undocumented opcode: Increment memory then subtract from A with borrow
    fn isc(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let incremented = value.wrapping_add(1);
        self.memory.borrow_mut().write(addr, incremented);
        self.sbc(incremented);
    }

    /// RLA - Undocumented opcode: Rotate left memory then AND with accumulator
    fn rla(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let rotated = self.rol(value);
        self.memory.borrow_mut().write(addr, rotated);
        self.a &= rotated;
        self.update_zero_and_negative_flags(self.a);
    }

    /// RRA - Undocumented opcode: Rotate right memory then ADC with accumulator
    fn rra(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let rotated = self.ror(value);
        self.memory.borrow_mut().write(addr, rotated);
        self.adc(rotated);
    }

    /// SLO - Undocumented opcode: Shift left memory then ORA with accumulator
    fn slo(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let shifted = self.asl(value);
        self.memory.borrow_mut().write(addr, shifted);
        self.ora(shifted);
    }

    /// SRE - Undocumented opcode: Shift right memory then EOR with accumulator
    fn sre(&mut self, addr: u16) {
        let value = self.memory.borrow().read(addr);
        let shifted = self.lsr(value);
        self.memory.borrow_mut().write(addr, shifted);
        self.eor(shifted);
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

    pub fn trigger_nmi(&mut self) {
        // Push PC and P onto stack
        self.push_word(self.pc);
        let mut p_with_break = self.p & !FLAG_BREAK; // Clear Break flag
        p_with_break |= FLAG_UNUSED; // Set unused flag
        self.push_byte(p_with_break);

        // Set PC to NMI vector
        self.pc = self.memory.borrow().read_u16(NMI_VECTOR);

        // Set Interrupt Disable flag
        self.p |= FLAG_INTERRUPT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Cartridge, MirroringMode};
    use std::cell::RefCell;
    use std::rc::Rc;

    // Test helper function to create a Memory instance with a PPU for testing
    fn create_test_memory() -> MemController {
        let ppu = Rc::new(RefCell::new(crate::ppu_modules::PPUModular::new(
            crate::nes::TvSystem::Ntsc,
        )));
        MemController::new(ppu)
    }

    // Test helper function to run the CPU until halted (KIL instruction)
    fn run(cpu: &mut Cpu) {
        while !cpu.halted {
            cpu.run_opcode();
        }
    }

    // Test helper function to load a program into memory and set reset vector
    fn fake_cartridge(cpu: &mut Cpu, program: &[u8]) {
        // Create a fake cartridge with the program in PRG ROM
        // PRG ROM is 16KB (0x4000 bytes), mapped at $8000-$BFFF (and mirrored at $C000-$FFFF)
        let mut prg_rom = vec![0; 0x4000]; // 16KB

        // Place the program at the beginning of PRG ROM
        for (i, &byte) in program.iter().enumerate() {
            prg_rom[i] = byte;
        }

        // Set reset vector to point to 0x8000 (which is index 0x0000 in PRG ROM)
        // Reset vector is at 0xFFFC-0xFFFD
        // For 16KB ROM: (0xFFFC - 0x8000) % 0x4000 = 0x7FFC % 0x4000 = 0x3FFC
        prg_rom[0x3FFC] = 0x00; // Low byte of 0x8000
        prg_rom[0x3FFD] = 0x80; // High byte of 0x8000

        // Create CHR ROM with zeros only (8KB)
        let chr_rom = vec![0; 0x2000];

        let cartridge = Cartridge {
            prg_rom,
            chr_rom,
            mirroring: MirroringMode::Horizontal,
        };

        cpu.memory.borrow_mut().map_cartridge(cartridge);
    }

    #[test]
    fn test_cpu_new() {
        let memory = create_test_memory();
        let cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.x, 0);
        assert_eq!(cpu.y, 0);
        assert_eq!(cpu.sp, 0xFD);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.p, 0x24);
    }

    #[test]
    fn test_cpu_reset() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Load a minimal program so reset vector is set up
        let program = vec![KIL];
        fake_cartridge(&mut cpu, &program);

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
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_IMM, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x30);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
        assert_eq!(cpu.p & FLAG_ZERO, 0); // Zero flag should be clear
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0); // Overflow flag should be clear
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Negative flag should be clear
    }

    #[test]
    fn test_adc_immediate_with_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_IMM, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        cpu.p |= FLAG_CARRY; // Set carry flag
        run(&mut cpu);
        assert_eq!(cpu.a, 0x31); // 0x10 + 0x20 + 1 (carry)
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
    }

    #[test]
    fn test_adc_immediate_carry_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_IMM, 0x01, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00); // Wraps around
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
    }

    #[test]
    fn test_adc_immediate_overflow_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_IMM, 0x50, KIL]; // Add another positive
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50; // Positive number
        run(&mut cpu);
        assert_eq!(cpu.a, 0xA0); // Result is negative in two's complement
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Negative flag should be set
    }

    #[test]
    fn test_adc_immediate_negative_overflow() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_IMM, 0x80, KIL]; // Add -128
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x80; // -128 in two's complement
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
    }

    #[test]
    fn test_adc_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        cpu.memory.borrow_mut().write(0x42, 0x33);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x43);
    }

    #[test]
    fn test_adc_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_ABS, 0x34, 0x12, KIL]; // Little-endian
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x20;
        cpu.memory.borrow_mut().write(0x1234, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x75);
    }

    #[test]
    fn test_adc_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1239, 0x44); // 0x1234 + 0x05
        run(&mut cpu);
        assert_eq!(cpu.a, 0x54);
    }

    #[test]
    fn test_adc_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x15;
        cpu.x = 0x03;
        cpu.memory.borrow_mut().write(0x45, 0x22); // 0x42 + 0x03
        run(&mut cpu);
        assert_eq!(cpu.a, 0x37);
    }

    #[test]
    fn test_adc_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x08;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0x17); // 0x1000 + 0x10
        run(&mut cpu);
        assert_eq!(cpu.a, 0x1F);
    }

    #[test]
    fn test_adc_indirect_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_INDX, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x05;
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x24, 0x74); // Pointer at 0x20 + 0x04: low byte
        cpu.memory.borrow_mut().write(0x25, 0x10); // Pointer at 0x20 + 0x04: high byte
        cpu.memory.borrow_mut().write(0x1074, 0x33); // Value at address 0x1074
        run(&mut cpu);
        assert_eq!(cpu.a, 0x38);
    }

    #[test]
    fn test_adc_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ADC_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x0A;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28); // Pointer at 0x86: low byte
        cpu.memory.borrow_mut().write(0x87, 0x10); // Pointer at 0x86: high byte
        cpu.memory.borrow_mut().write(0x1038, 0x06); // Value at 0x1028 + 0x10
        run(&mut cpu);
        assert_eq!(cpu.a, 0x10);
    }

    #[test]
    fn test_and_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_IMM, 0b1010_1010, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_0000;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1010_0000);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_and_immediate_zero_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_IMM, 0b0000_1111, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_0000;
        run(&mut cpu);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_and_immediate_clears_negative_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_IMM, 0b0111_1111, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.p = FLAG_NEGATIVE; // Set negative flag initially
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0111_1111);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_and_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1100_1100;
        cpu.memory.borrow_mut().write(0x42, 0b1010_1010);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1000_1000);
    }

    #[test]
    fn test_and_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b0011_1111); // 0x42 + 0x05
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0011_0000);
    }

    #[test]
    fn test_and_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.memory.borrow_mut().write(0x1234, 0b1100_1100);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1000_1000);
    }

    #[test]
    fn test_and_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b0101_0101); // 0x1234 + 0x10
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0101_0101);
    }

    #[test]
    fn test_and_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1100_0011;
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1020, 0b0011_1100); // 0x1000 + 0x20
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0000_0000);
    }

    #[test]
    fn test_and_indirect_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_INDX, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x24, 0x74); // Pointer at 0x20 + 0x04: low byte
        cpu.memory.borrow_mut().write(0x25, 0x10); // Pointer at 0x20 + 0x04: high byte
        cpu.memory.borrow_mut().write(0x1074, 0b0000_1111); // Value at address 0x1074
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0000_0000);
    }

    #[test]
    fn test_and_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AND_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28); // Pointer at 0x86: low byte
        cpu.memory.borrow_mut().write(0x87, 0x10); // Pointer at 0x86: high byte
        cpu.memory.borrow_mut().write(0x1038, 0b1111_0000); // Value at 0x1028 + 0x10
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1010_0000);
    }

    #[test]
    fn test_asl_accumulator() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_A, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b0100_0010;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1000_0100);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_asl_accumulator_sets_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_A, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1000_0001;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0000_0010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_asl_accumulator_sets_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_A, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1000_0000;
        run(&mut cpu);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_asl_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0b0011_0011);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0b0110_0110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_asl_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b1010_0101); // 0x42 + 0x05
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0b0100_1010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_asl_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0b0100_0001);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0b1000_0010);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_asl_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASL_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b0000_0001); // 0x1234 + 0x10
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0b0000_0010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_bit_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BIT_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_0000;
        cpu.memory.borrow_mut().write(0x42, 0b1100_0011);
        run(&mut cpu);
        // A & memory = 0b1111_0000 & 0b1100_0011 = 0b1100_0000 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 1
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bit_zero_page_sets_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BIT_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b0000_1111;
        cpu.memory.borrow_mut().write(0x42, 0b1111_0000);
        run(&mut cpu);
        // A & memory = 0b0000_1111 & 0b1111_0000 = 0b0000_0000 (zero)
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        // Bit 7 of memory is 1
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bit_zero_page_clears_flags() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BIT_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1111_1111;
        cpu.memory.borrow_mut().write(0x42, 0b0011_1111);
        run(&mut cpu);
        // A & memory = 0b1111_1111 & 0b0011_1111 = 0b0011_1111 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 0
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        // Bit 6 of memory is 0
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
    }

    #[test]
    fn test_bit_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BIT_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_1010;
        cpu.memory.borrow_mut().write(0x1234, 0b0101_1010);
        run(&mut cpu);
        // A & memory = 0b1010_1010 & 0b0101_1010 = 0b0000_1010 (not zero)
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        // Bit 7 of memory is 0
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        // Bit 6 of memory is 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_bcc_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BCC, 0x02, 0x00, 0x00, KIL]; // Branch forward 2 bytes to skip padding
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Ensure carry is clear
        run(&mut cpu);
        // PC should be at 0x8000 + 2 (after reading BCC and offset) + 2 (offset) + 1 (BRK) = 0x8005
        assert_eq!(cpu.pc, 0x8005);
    }

    #[test]
    fn test_bcc_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BCC, 0x05, KIL]; // Should not branch
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_CARRY; // Set carry flag
        run(&mut cpu);
        // PC should be at 0x8000 + 2 (instruction) + 1 (BRK) = 0x8003
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bcc_branch_backward() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // BRK at start, then BCC at offset 3 that branches back -5 bytes to the BRK
        let program = vec![KIL, 0x00, 0x00, BCC, 0xFB];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Ensure carry is clear
        cpu.pc = 0x8003; // Start at offset 3 (the BCC instruction)
        run(&mut cpu);
        // Should branch back to 0x8000 where the BRK is
        assert_eq!(cpu.pc, 0x8001);
    }

    #[test]
    fn test_bcs_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BCS, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_CARRY; // Set carry flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bcs_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BCS, 0x03, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_CARRY; // Clear carry flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_beq_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BEQ, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_beq_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BEQ, 0x02, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_ZERO; // Clear zero flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bmi_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BMI, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_NEGATIVE; // Set negative flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bmi_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BMI, 0x04, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bne_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BNE, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_ZERO; // Clear zero flag (not equal)
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bne_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BNE, 0x06, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag (equal)
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bpl_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BPL, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_NEGATIVE; // Clear negative flag (positive)
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bpl_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BPL, 0x07, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_NEGATIVE; // Set negative flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bvc_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BVC, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bvc_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BVC, 0x05, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_OVERFLOW; // Set overflow flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_bvs_branch_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BVS, 0x01, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_OVERFLOW; // Set overflow flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8004);
    }

    #[test]
    fn test_bvs_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![BVS, 0x08, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x8003);
    }

    #[test]
    fn test_cmp_immediate_equal() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // A == value
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is 0, bit 7 = 0
    }

    #[test]
    fn test_cmp_immediate_greater() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_IMM, 0x30, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is positive
    }

    #[test]
    fn test_cmp_immediate_less() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_IMM, 0x50, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x30;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
        assert_eq!(cpu.p & FLAG_CARRY, 0); // A < value
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative (0x30 - 0x50 = 0xE0)
    }

    #[test]
    fn test_cmp_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x80;
        cpu.memory.borrow_mut().write(0x42, 0x80);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cmp_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0x05); // 0x42 + 0x05
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x10 >= 0x05
    }

    #[test]
    fn test_cmp_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x20;
        cpu.memory.borrow_mut().write(0x1234, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
    }

    #[test]
    fn test_cmp_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0xFF);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_cmp_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x55;
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1020, 0x44);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x55 >= 0x44
    }

    #[test]
    fn test_cmp_indirect_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_INDX, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x33;
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x24, 0x74);
        cpu.memory.borrow_mut().write(0x25, 0x10);
        cpu.memory.borrow_mut().write(0x1074, 0x33);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_cmp_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CMP_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x77;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28);
        cpu.memory.borrow_mut().write(0x87, 0x10);
        cpu.memory.borrow_mut().write(0x1038, 0x88);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x77 < 0x88
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpx_immediate_equal() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPX_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpx_immediate_greater() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPX_IMM, 0x30, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x50;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpx_immediate_less() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPX_IMM, 0x50, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x30;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpx_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPX_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x80;
        cpu.memory.borrow_mut().write(0x42, 0x80);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cpx_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPX_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x20;
        cpu.memory.borrow_mut().write(0x1234, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpy_immediate_equal() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPY_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpy_immediate_greater() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPY_IMM, 0x30, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x50;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_cpy_immediate_less() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPY_IMM, 0x50, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x30;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_cpy_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPY_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x80;
        cpu.memory.borrow_mut().write(0x42, 0x80);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_cpy_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CPY_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1234, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dec_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x50);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0x4F);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_zero_page_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x01);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_zero_page_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x00);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0xFF);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dec_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0x80);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0x7F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0x2F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dec_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEC_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0x90);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0x8F);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_eor_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_IMM, 0b1111_0000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_1010;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b0101_1010);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_eor_immediate_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_IMM, 0b1010_1010, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_1010;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_eor_immediate_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_IMM, 0b1111_0000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b0101_0101;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1010_0101);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_eor_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.memory.borrow_mut().write(0x42, 0x0F);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xF0);
    }

    #[test]
    fn test_eor_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xAA);
    }

    #[test]
    fn test_eor_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x12;
        cpu.memory.borrow_mut().write(0x1234, 0x34);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x26);
    }

    #[test]
    fn test_eor_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xAA;
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_eor_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_ABSY, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xF0;
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1254, 0x0F);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_eor_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_INDX, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1100_0011;
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x24, 0x74);
        cpu.memory.borrow_mut().write(0x25, 0x10);
        cpu.memory.borrow_mut().write(0x1074, 0b0011_1100);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b1111_1111);
    }

    #[test]
    fn test_eor_indirect_indexed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![EOR_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b1010_0101;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28);
        cpu.memory.borrow_mut().write(0x87, 0x10);
        cpu.memory.borrow_mut().write(0x1038, 0b0101_1010);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_clc() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CLC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = FLAG_CARRY;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_cld() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CLD, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = FLAG_DECIMAL;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_DECIMAL, 0);
    }

    #[test]
    fn test_cli() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CLI, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = FLAG_INTERRUPT;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_INTERRUPT, 0);
    }

    #[test]
    fn test_clv() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![CLV, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = FLAG_OVERFLOW;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
    }

    #[test]
    fn test_sec() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SEC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_sed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SED, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_DECIMAL, FLAG_DECIMAL);
    }

    #[test]
    fn test_sei() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SEI, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);
    }

    #[test]
    fn test_inc_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x50);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0x51);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_zero_page_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0xFF);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_zero_page_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x7F);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0x80);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_inc_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0x20);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0x21);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0x31);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inc_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INC_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0x8F);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0x90);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_jmp_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        fake_cartridge(&mut cpu, &vec![]);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x0600, JMP_ABS);
        cpu.memory.borrow_mut().write(0x0601, 0x34);
        cpu.memory.borrow_mut().write(0x0602, 0x12);
        cpu.memory.borrow_mut().write(0x1234, KIL);
        cpu.pc = 0x0600;
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x1235); // PC after BRK at 0x1234
    }

    #[test]
    fn test_jmp_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        fake_cartridge(&mut cpu, &vec![]);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x0600, JMP_IND);
        cpu.memory.borrow_mut().write(0x0601, 0x20);
        cpu.memory.borrow_mut().write(0x0602, 0x10);
        cpu.memory.borrow_mut().write(0x1020, 0x56);
        cpu.memory.borrow_mut().write(0x1021, 0x18);
        cpu.memory.borrow_mut().write(0x1856, KIL);
        cpu.pc = 0x0600;
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x1857); // PC after BRK at 0x1856
    }

    #[test]
    fn test_jmp_indirect_page_boundary_bug() {
        // The 6502 has a bug where if the indirect address is on a page boundary
        // (e.g., 0x10FF), it doesn't cross the page boundary to read the high byte
        // Instead of reading from 0x1100, it wraps around to 0x1000
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        fake_cartridge(&mut cpu, &vec![]);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x0600, JMP_IND);
        cpu.memory.borrow_mut().write(0x0601, 0xFF);
        cpu.memory.borrow_mut().write(0x0602, 0x10);
        cpu.memory.borrow_mut().write(0x10FF, 0x34);
        cpu.memory.borrow_mut().write(0x1000, 0x12); // Wraps to start of page, not 0x1100
        cpu.memory.borrow_mut().write(0x1234, KIL);
        cpu.pc = 0x0600;
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x1235); // Should jump to 0x1234 (low=0x34, high=0x12)
    }

    #[test]
    fn test_jsr() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        fake_cartridge(&mut cpu, &vec![]);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x0600, JSR);
        cpu.memory.borrow_mut().write(0x0601, 0x34);
        cpu.memory.borrow_mut().write(0x0602, 0x12);
        cpu.memory.borrow_mut().write(0x1234, KIL);
        cpu.pc = 0x0600;
        cpu.sp = 0xFF;
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x1235); // PC after BRK at 0x1234
        assert_eq!(cpu.sp, 0xFD); // SP decremented by 2 (pushed 2 bytes)
        // Return address should be 0x0602 (address of last byte of JSR instruction)
        assert_eq!(cpu.memory.borrow().read(0x01FF), 0x06); // High byte of return address
        assert_eq!(cpu.memory.borrow().read(0x01FE), 0x02); // Low byte of return address
    }

    #[test]
    fn test_lda_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_lda_immediate_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_IMM, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_lda_immediate_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_IMM, 0x80, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_lda_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn test_lda_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0xAA);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xAA);
    }

    #[test]
    fn test_lda_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x77);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x77);
    }

    #[test]
    fn test_lda_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0x88);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x88);
    }

    #[test]
    fn test_lda_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_ABSY, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1254, 0x99);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x99);
    }

    #[test]
    fn test_lda_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_INDX, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x24, 0x74);
        cpu.memory.borrow_mut().write(0x25, 0x10);
        cpu.memory.borrow_mut().write(0x1074, 0xCC);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xCC);
    }

    #[test]
    fn test_lda_indirect_indexed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28);
        cpu.memory.borrow_mut().write(0x87, 0x10);
        cpu.memory.borrow_mut().write(0x1038, 0xDD);
        run(&mut cpu);
        assert_eq!(cpu.a, 0xDD);
    }

    #[test]
    fn test_ldx_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.x, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ldx_immediate_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_IMM, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ldx_immediate_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_IMM, 0x80, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.x, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ldx_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.x, 0x55);
    }

    #[test]
    fn test_ldx_zero_page_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_ZPY, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0xAA);
        run(&mut cpu);
        assert_eq!(cpu.x, 0xAA);
    }

    #[test]
    fn test_ldx_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x77);
        run(&mut cpu);
        assert_eq!(cpu.x, 0x77);
    }

    #[test]
    fn test_ldx_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDX_ABSY, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1254, 0x99);
        run(&mut cpu);
        assert_eq!(cpu.x, 0x99);
    }

    #[test]
    fn test_ldy_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.y, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ldy_immediate_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_IMM, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.y, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ldy_immediate_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_IMM, 0x80, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.y, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ldy_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x55);
        run(&mut cpu);
        assert_eq!(cpu.y, 0x55);
    }

    #[test]
    fn test_ldy_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0xAA);
        run(&mut cpu);
        assert_eq!(cpu.y, 0xAA);
    }

    #[test]
    fn test_ldy_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x77);
        run(&mut cpu);
        assert_eq!(cpu.y, 0x77);
    }

    #[test]
    fn test_ldy_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDY_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0x88);
        run(&mut cpu);
        assert_eq!(cpu.y, 0x88);
    }

    #[test]
    fn test_lsr_accumulator() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10110101;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b01011010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_lsr_accumulator_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b00000001;
        run(&mut cpu);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_lsr_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0b11001100);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0b01100110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_lsr_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b10101011);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0b01010101);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_lsr_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0b01010100);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0b00101010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_lsr_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LSR_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b00000011);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0b00000001);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_nop() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![NOP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x33;
        cpu.y = 0x24;
        cpu.p = 0xFF;
        run(&mut cpu);
        // NOP should not affect any registers or flags
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.x, 0x33);
        assert_eq!(cpu.y, 0x24);
        assert_eq!(cpu.p, 0xFF);
    }

    #[test]
    fn test_ora_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_IMM, 0b01010101, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10101010;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ora_immediate_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_IMM, 0x00, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_ora_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.memory.borrow_mut().write(0x42, 0b00001111);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10000000;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b01000000);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11000000);
    }

    #[test]
    fn test_ora_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b00110011;
        cpu.memory.borrow_mut().write(0x1234, 0b11001100);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b00001111;
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b11110000);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_ABSY, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.y = 0x20;
        cpu.memory.borrow_mut().write(0x1254, 0b10101010);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_INDX, 0x82, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b00110011;
        cpu.x = 0x04;
        cpu.memory.borrow_mut().write(0x86, 0x34);
        cpu.memory.borrow_mut().write(0x87, 0x12);
        cpu.memory.borrow_mut().write(0x1234, 0b11001100);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_indirect_indexed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10101010;
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x86, 0x28);
        cpu.memory.borrow_mut().write(0x87, 0x10);
        cpu.memory.borrow_mut().write(0x1038, 0b01010101);
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11111111);
    }

    #[test]
    fn test_ora_negative_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ORA_IMM, 0x80, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_dex() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x41);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dex_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x01;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_dex_wrap() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.x, 0xFF);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_dey() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DEY, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.y, 0x41);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inx() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x43);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_inx_wrap() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0xFF;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_iny() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![INY, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.y, 0x43);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tax() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TAX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tax_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TAX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_tax_negative() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TAX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x80;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_tay() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TAY, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.y, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_txa() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TXA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_tya() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TYA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rol_accumulator() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10110101;
        cpu.p = 0; // Clear carry
        run(&mut cpu);
        assert_eq!(cpu.a, 0b01101010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rol_accumulator_with_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.p = FLAG_CARRY; // Set carry
        run(&mut cpu);
        assert_eq!(cpu.a, 0b10101011);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_rol_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0b11001100);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0b10011000);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rol_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b10101011);
        cpu.p = FLAG_CARRY;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0b01010111);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rol_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0b01010100);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0b10101000);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_rol_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROL_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b00000011);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0b00000110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_accumulator() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b10110101;
        cpu.p = 0; // Clear carry
        run(&mut cpu);
        assert_eq!(cpu.a, 0b01011010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_ror_accumulator_with_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ACC, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b01010101;
        cpu.p = FLAG_CARRY; // Set carry
        run(&mut cpu);
        assert_eq!(cpu.a, 0b10101010);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_ror_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0b11001100);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x42), 0b01100110);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x47, 0b10101011);
        cpu.p = FLAG_CARRY;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x47), 0b11010101);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_ror_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0b01010100);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0b00101010);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
    }

    #[test]
    fn test_ror_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ROR_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1244, 0b00000011);
        cpu.p = 0;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1244), 0b00000001);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_rti() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RTI, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        // Set up stack with saved processor status and return address
        cpu.sp = 0xFC;
        cpu.memory.borrow_mut().write(0x01FD, 0b11010011); // Saved status flags
        cpu.memory.borrow_mut().write(0x01FE, 0x34); // PC low byte
        cpu.memory.borrow_mut().write(0x01FF, 0x12); // PC high byte
        cpu.memory.borrow_mut().write(0x1234, KIL); // BRK at return address
        run(&mut cpu);
        // RTI should behave like PLP - ignore B flag and unused bit, always set unused to 1
        // 0b11010011 with B flag cleared and unused set: 0b11100011 = 0xE3
        assert_eq!(cpu.p, 0b11100011);
        assert_eq!(cpu.pc, 0x1235); // PC after BRK instruction
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_rts() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RTS, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        // Set up stack with saved return address (PC-1)
        cpu.sp = 0xFD;
        cpu.memory.borrow_mut().write(0x01FE, 0x33); // PC-1 low byte (0x1233)
        cpu.memory.borrow_mut().write(0x01FF, 0x12); // PC-1 high byte
        cpu.memory.borrow_mut().write(0x1234, KIL); // BRK at return address
        run(&mut cpu);
        assert_eq!(cpu.pc, 0x1235); // PC after BRK instruction (0x1234 + 1)
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_sbc_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_IMM, 0x30, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        run(&mut cpu);
        assert_eq!(cpu.a, 0x20);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_sbc_immediate_with_borrow() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_IMM, 0x30, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p &= !FLAG_CARRY; // Clear carry (borrow)
        run(&mut cpu);
        assert_eq!(cpu.a, 0x1F);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    }

    #[test]
    fn test_sbc_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x80;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x42, 0x40);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_ZPX, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50;
        cpu.x = 0x05;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x47, 0x10);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x60;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x1234, 0x20);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_ABSX, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x70;
        cpu.x = 0x10;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x1244, 0x30);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_ABSY, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x90;
        cpu.y = 0x20;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x1254, 0x50);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_INDX, 0x82, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xA0;
        cpu.x = 0x04;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x86, 0x34);
        cpu.memory.borrow_mut().write(0x87, 0x12);
        cpu.memory.borrow_mut().write(0x1234, 0x60);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_indirect_indexed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_INDY, 0x86, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xB0;
        cpu.y = 0x10;
        cpu.p |= FLAG_CARRY;
        cpu.memory.borrow_mut().write(0x86, 0x28);
        cpu.memory.borrow_mut().write(0x87, 0x10);
        cpu.memory.borrow_mut().write(0x1038, 0x70);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x40);
    }

    #[test]
    fn test_sbc_overflow() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_IMM, 0xB0, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x50;
        cpu.p |= FLAG_CARRY;
        run(&mut cpu);
        assert_eq!(cpu.a, 0xA0);
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_sta_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_ZP, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x10), 0x42);
    }

    #[test]
    fn test_sta_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_ZPX, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x15), 0x42);
    }

    #[test]
    fn test_sta_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_ABS, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1000), 0x42);
    }

    #[test]
    fn test_sta_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x42);
    }

    #[test]
    fn test_sta_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.y = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x42);
    }

    #[test]
    fn test_sta_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_INDX, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x15, 0x00);
        cpu.memory.borrow_mut().write(0x16, 0x10);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1000), 0x42);
    }

    #[test]
    fn test_sta_indirect_indexed() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STA_INDY, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.y = 0x05;
        cpu.memory.borrow_mut().write(0x10, 0x00);
        cpu.memory.borrow_mut().write(0x11, 0x10);
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x42);
    }

    #[test]
    fn test_txs() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TXS, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0xFF;
        run(&mut cpu);
        assert_eq!(cpu.sp, 0xFF);
    }

    #[test]
    fn test_tsx() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TSX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xAB;
        run(&mut cpu);
        assert_eq!(cpu.x, 0xAB);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_tsx_zero_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![TSX, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_pha() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PHA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.sp = 0xFD;
        run(&mut cpu);
        assert_eq!(cpu.sp, 0xFC);
        assert_eq!(cpu.memory.borrow().read(0x01FD), 0x42);
    }

    #[test]
    fn test_pla() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PLA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.borrow_mut().write(0x01FD, 0x42);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.sp, 0xFD);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_pla_zero_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PLA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.borrow_mut().write(0x01FD, 0x00);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    }

    #[test]
    fn test_pla_negative_flag() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PLA, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.borrow_mut().write(0x01FD, 0x80);
        run(&mut cpu);
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_php() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PHP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p = 0xFF;
        cpu.sp = 0xFD;
        run(&mut cpu);
        assert_eq!(cpu.sp, 0xFC);
        // PHP should push P with B flag (bit 4) and unused bit (bit 5) set to 1
        assert_eq!(cpu.memory.borrow().read(0x01FD), 0xFF);
    }

    #[test]
    fn test_php_sets_break_and_unused_bits() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PHP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        // Set status to 0xC0 (only N and V flags set, B and unused are 0)
        cpu.p = 0xC0;
        cpu.sp = 0xFD;
        run(&mut cpu);
        assert_eq!(cpu.sp, 0xFC);
        // Should push 0xF0 (0xC0 | 0x30) - B flag and unused bit both set
        assert_eq!(cpu.memory.borrow().read(0x01FD), 0xF0);
    }

    #[test]
    fn test_plp() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PLP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xFC;
        cpu.memory.borrow_mut().write(0x01FD, 0xC3);
        run(&mut cpu);
        // PLP should load flags but ignore B flag and always set unused bit (bit 5)
        // 0xC3 = 0b11000011, after PLP with unused bit set: 0b11100011 = 0xE3
        assert_eq!(cpu.p, 0xE3);
        assert_eq!(cpu.sp, 0xFD);
    }

    #[test]
    fn test_plp_ignores_break_and_unused_bits() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![PLP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.sp = 0xFC;
        // Stack has 0xFF (all bits set including B and unused)
        cpu.memory.borrow_mut().write(0x01FD, 0xFF);
        // But P register starts with B and unused cleared
        cpu.p = 0xC0; // Only N and V set
        run(&mut cpu);
        // After PLP, P should be 0xEF (all bits except B flag)
        // B flag (bit 4) should remain at its previous state
        // Unused bit (bit 5) should remain set (always 1)
        assert_eq!(cpu.p, 0xEF);
        assert_eq!(cpu.sp, 0xFD);
    }

    #[test]
    fn test_stx_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STX_ZP, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x10), 0x42);
    }

    #[test]
    fn test_stx_zero_page_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STX_ZPY, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        cpu.y = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x15), 0x42);
    }

    #[test]
    fn test_stx_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STX_ABS, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1000), 0x42);
    }

    #[test]
    fn test_sty_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STY_ZP, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x10), 0x42);
    }

    #[test]
    fn test_sty_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STY_ZPX, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        cpu.x = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x15), 0x42);
    }

    #[test]
    fn test_sty_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![STY_ABS, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x42;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1000), 0x42);
    }

    #[test]
    fn test_write_u16_to_addr() {
        let memory = create_test_memory();
        let cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write_u16(0x1234, 0xABCD);
        assert_eq!(cpu.memory.borrow().read(0x1234), 0xCD); // Low byte
        assert_eq!(cpu.memory.borrow().read(0x1235), 0xAB); // High byte
    }

    #[test]
    fn test_read_u16_from_addr() {
        let memory = create_test_memory();
        let cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x1234, 0xCD); // Low byte
        cpu.memory.borrow_mut().write(0x1235, 0xAB); // High byte
        let result = cpu.memory.borrow().read_u16(0x1234);
        assert_eq!(result, 0xABCD);
    }

    #[test]
    fn test_write_and_read_u16() {
        let memory = create_test_memory();
        let cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write_u16(0x1000, 0x1234);
        let result = cpu.memory.borrow().read_u16(0x1000);
        assert_eq!(result, 0x1234);
    }

    #[test]
    fn test_load_program_at_custom_address() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        run(&mut cpu);
        assert_eq!(cpu.a, 0x42);
        // Verify program was loaded at 0x8000
        assert_eq!(cpu.memory.borrow().read(0x8000), LDA_IMM);
        assert_eq!(cpu.memory.borrow().read(0x8001), 0x42);
        assert_eq!(cpu.memory.borrow().read(0x8002), KIL);
    }

    #[test]
    fn test_aac_sets_carry_when_bit7_set() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AAC_IMM, 0b11000000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = 0x00;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b11000000);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_aac_clears_carry_when_bit7_clear() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AAC_IMM, 0b01000000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = FLAG_CARRY;
        run(&mut cpu);
        assert_eq!(cpu.a, 0b01000000);
        assert_eq!(cpu.p & FLAG_CARRY, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_sax_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SAX_ZP, 0x50, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.x = 0b10101010;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x0050), 0b10100000);
    }

    #[test]
    fn test_sax_zero_page_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SAX_ZPY, 0x50, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.x = 0b10101010;
        cpu.y = 0x05;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x0055), 0b10100000);
    }

    #[test]
    fn test_sax_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SAX_ABS, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.x = 0b10101010;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1000), 0b10100000);
    }

    #[test]
    fn test_sax_indexed_indirect() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SAX_INDX, 0x40, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.x = 0b10101010;
        // The pointer is at 0x40 + X (wrapping in zero page)
        // So we need to set up the pointer at 0x40 + 0xAA = 0xEA
        cpu.memory.borrow_mut().write(0x00EA, 0x00);
        cpu.memory.borrow_mut().write(0x00EB, 0x10);
        run(&mut cpu);
        // Should store A & X = 0b11111111 & 0b10101010 = 0b10101010 at 0x1000
        assert_eq!(cpu.memory.borrow().read(0x1000), 0b10101010);
    }

    #[test]
    fn test_arr_basic() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ARR_IMM, 0b11110000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = 0x00; // No carry
        run(&mut cpu);
        // A = 0b11111111 AND 0b11110000 = 0b11110000
        // Then shift right: 0b11110000 >> 1 = 0b01111000
        assert_eq!(cpu.a, 0b01111000);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // bit 7 is 0
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_arr_with_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ARR_IMM, 0b11110000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = FLAG_CARRY; // Carry set
        run(&mut cpu);
        // A = 0b11111111 AND 0b11110000 = 0b11110000
        // Then shift right with carry: (0b11110000 >> 1) | 0b10000000 = 0b11111000
        assert_eq!(cpu.a, 0b11111000);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // bit 7 is 1
    }

    #[test]
    fn test_arr_sets_carry_and_overflow() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ARR_IMM, 0b01100001, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = 0x00;
        run(&mut cpu);
        // A = 0b11111111 AND 0b01100001 = 0b01100001
        // Then shift right: 0b01100001 >> 1 = 0b00110000 (bit 0 was 1, sets carry)
        assert_eq!(cpu.a, 0b00110000);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // bit 0 was 1
        // Overflow is bit 6 XOR bit 5 of result
        // Result is 0b00110000: bit 6 = 0, bit 5 = 1, so 0 XOR 1 = 1
        assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    }

    #[test]
    fn test_asr_basic() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASR_IMM, 0b11110000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = FLAG_CARRY; // Carry should be ignored for LSR
        run(&mut cpu);
        // A = 0b11111111 AND 0b11110000 = 0b11110000
        // Then LSR (logical shift right): 0b11110000 >> 1 = 0b01111000
        assert_eq!(cpu.a, 0b01111000);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // bit 7 is 0
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // bit 0 of AND result was 0
    }

    #[test]
    fn test_asr_sets_carry() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASR_IMM, 0b11110001, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.p = 0x00;
        run(&mut cpu);
        // A = 0b11111111 AND 0b11110001 = 0b11110001
        // Then LSR: 0b11110001 >> 1 = 0b01111000
        assert_eq!(cpu.a, 0b01111000);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // bit 0 of AND result was 1
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_asr_zero_result() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ASR_IMM, 0b00000001, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b00000001;
        cpu.p = 0x00;
        run(&mut cpu);
        // A = 0b00000001 AND 0b00000001 = 0b00000001
        // Then LSR: 0b00000001 >> 1 = 0b00000000
        assert_eq!(cpu.a, 0b00000000);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // bit 0 was 1
    }

    #[test]
    fn test_atx_basic() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ATX_IMM, 0b11110000, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11111111;
        cpu.x = 0x00;
        run(&mut cpu);
        // A = A AND immediate = 0b11111111 AND 0b11110000 = 0b11110000
        // Then transfer to both A and X
        assert_eq!(cpu.a, 0b11110000);
        assert_eq!(cpu.x, 0b11110000);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_atx_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ATX_IMM, 0b00001111, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11110000;
        cpu.x = 0xFF;
        run(&mut cpu);
        // A = A AND immediate = 0b11110000 AND 0b00001111 = 0b00000000
        assert_eq!(cpu.a, 0b00000000);
        assert_eq!(cpu.x, 0b00000000);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_atx_preserves_result() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ATX_IMM, 0b10101010, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b11001100;
        cpu.x = 0x33;
        run(&mut cpu);
        // A = A AND immediate = 0b11001100 AND 0b10101010 = 0b10001000
        assert_eq!(cpu.a, 0b10001000);
        assert_eq!(cpu.x, 0b10001000);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    }

    #[test]
    fn test_axa_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Set up indirect address at ZP location 0x20
        cpu.memory.borrow_mut().write(0x20, 0x00); // Low byte
        cpu.memory.borrow_mut().write(0x21, 0x10); // High byte = 0x10, so address is 0x1000
        let program = vec![AXA_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x7F;
        cpu.y = 0x05; // Add 5 to base address, final address = 0x1005
        run(&mut cpu);
        // Value = A AND X AND (high byte of address + 1)
        // high byte of final address 0x1005 is 0x10
        // Value = 0xFF AND 0x7F AND (0x10 + 1) = 0xFF AND 0x7F AND 0x11 = 0x11
        let stored_value = cpu.memory.borrow().read(0x1005);
        assert_eq!(stored_value, 0x11);
    }

    #[test]
    fn test_axa_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AXA_ABSY, 0x00, 0x10, KIL]; // Base address 0x1000
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0x3F;
        cpu.y = 0x10; // Final address = 0x1010
        run(&mut cpu);
        // Value = A AND X AND (high byte of address + 1)
        // high byte of final address 0x1010 is 0x10
        // Value = 0xFF AND 0x3F AND (0x10 + 1) = 0xFF AND 0x3F AND 0x11 = 0x11
        let stored_value = cpu.memory.borrow().read(0x1010);
        assert_eq!(stored_value, 0x11);
    }

    #[test]
    fn test_axa_page_boundary() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AXA_ABSY, 0xFF, 0x10, KIL]; // Base address 0x10FF
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0xFF;
        cpu.y = 0x01; // Final address = 0x1100
        run(&mut cpu);
        // Value = A AND X AND (high byte of address + 1)
        // high byte of final address 0x1100 is 0x11
        // Value = 0xFF AND 0xFF AND (0x11 + 1) = 0xFF AND 0xFF AND 0x12 = 0x12
        let stored_value = cpu.memory.borrow().read(0x1100);
        assert_eq!(stored_value, 0x12);
    }

    #[test]
    fn test_axs_basic() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AXS_IMM, 0x05, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x0F;
        cpu.x = 0xFF;
        run(&mut cpu);
        // X = (A AND X) - immediate (without borrow)
        // X = (0x0F AND 0xFF) - 0x05 = 0x0F - 0x05 = 0x0A
        assert_eq!(cpu.x, 0x0A);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow occurred
    }

    #[test]
    fn test_axs_with_borrow() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AXS_IMM, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x0F;
        cpu.x = 0x0F;
        run(&mut cpu);
        // X = (A AND X) - immediate (without borrow)
        // X = (0x0F AND 0x0F) - 0x10 = 0x0F - 0x10 = 0xFF (wraps around)
        assert_eq!(cpu.x, 0xFF);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Borrow occurred
    }

    #[test]
    fn test_axs_zero_result() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![AXS_IMM, 0x08, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x0F;
        cpu.x = 0xF8;
        run(&mut cpu);
        // X = (A AND X) - immediate
        // X = (0x0F AND 0xF8) - 0x08 = 0x08 - 0x08 = 0x00
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
    }

    #[test]
    fn test_dcp_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DCP_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x10);
        cpu.a = 0x0F;
        run(&mut cpu);
        // Memory at 0x42: 0x10 - 1 = 0x0F
        assert_eq!(cpu.memory.borrow().read(0x42), 0x0F);
        // Compare A (0x0F) with memory (0x0F)
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Equal
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= memory
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dcp_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DCP_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0x20);
        cpu.a = 0x30;
        run(&mut cpu);
        // Memory at 0x1005: 0x20 - 1 = 0x1F
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x1F);
        // Compare A (0x30) with memory (0x1F): 0x30 > 0x1F
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= memory
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_dcp_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![DCP_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0x05);
        cpu.a = 0x03;
        run(&mut cpu);
        // Memory at 0x1010: 0x05 - 1 = 0x04
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x04);
        // Compare A (0x03) with memory (0x04): 0x03 < 0x04
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // A < memory (borrow)
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result would be negative
    }

    #[test]
    fn test_dop_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DOP_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0xFF);
        cpu.a = 0x10;
        cpu.x = 0x20;
        cpu.y = 0x30;
        let saved_status = cpu.p;
        run(&mut cpu);
        // DOP does nothing - just reads memory and discards
        assert_eq!(cpu.memory.borrow().read(0x42), 0xFF); // Memory unchanged
        assert_eq!(cpu.a, 0x10); // A unchanged
        assert_eq!(cpu.x, 0x20); // X unchanged
        assert_eq!(cpu.y, 0x30); // Y unchanged
        assert_eq!(cpu.p, saved_status); // Status unchanged
    }

    #[test]
    fn test_dop_zero_page_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DOP_ZPX, 0x40, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x45, 0xAA);
        cpu.a = 0x10;
        cpu.y = 0x30;
        let saved_status = cpu.p;
        run(&mut cpu);
        // DOP does nothing - just reads memory at 0x40 + X = 0x45 and discards
        assert_eq!(cpu.memory.borrow().read(0x45), 0xAA); // Memory unchanged
        assert_eq!(cpu.a, 0x10); // A unchanged
        assert_eq!(cpu.x, 0x05); // X unchanged
        assert_eq!(cpu.y, 0x30); // Y unchanged
        assert_eq!(cpu.p, saved_status); // Status unchanged
    }

    #[test]
    fn test_dop_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![DOP_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x10;
        cpu.x = 0x20;
        cpu.y = 0x30;
        let saved_status = cpu.p;
        run(&mut cpu);
        // DOP does nothing - just reads immediate value and discards
        assert_eq!(cpu.a, 0x10); // A unchanged
        assert_eq!(cpu.x, 0x20); // X unchanged
        assert_eq!(cpu.y, 0x30); // Y unchanged
        assert_eq!(cpu.p, saved_status); // Status unchanged
    }

    #[test]
    fn test_isb_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ISB_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x10);
        cpu.a = 0x50;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        run(&mut cpu);
        // Memory at 0x42: 0x10 + 1 = 0x11
        assert_eq!(cpu.memory.borrow().read(0x42), 0x11);
        // Then SBC: A = 0x50 - 0x11 - (1 - carry) = 0x50 - 0x11 - 0 = 0x3F
        assert_eq!(cpu.a, 0x3F);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_isb_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![ISB_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0xFF);
        cpu.a = 0x00;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        run(&mut cpu);
        // Memory at 0x1005: 0xFF + 1 = 0x00 (wraps)
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x00);
        // Then SBC: A = 0x00 - 0x00 - 0 = 0x00
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
    }

    #[test]
    fn test_isb_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![ISB_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0x05);
        cpu.a = 0x10;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        run(&mut cpu);
        // Memory at 0x1010: 0x05 + 1 = 0x06
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x06);
        // Then SBC: A = 0x10 - 0x06 - 0 = 0x0A
        assert_eq!(cpu.a, 0x0A);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_kil_opcode_0x02() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        // KIL should return false to indicate the CPU is halted
        cpu.run_opcode();
        assert!(cpu.halted);
    }

    #[test]
    fn test_kil_opcode_0x12() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![KIL2];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.run_opcode();
        assert!(cpu.halted);
    }

    #[test]
    fn test_kil_opcode_0xf2() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![KIL12];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.run_opcode();
        assert!(cpu.halted);
    }

    #[test]
    fn test_kil_halts_until_reset() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![KIL, NOP, NOP];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        // Execute KIL - should halt
        cpu.run_opcode();
        assert!(cpu.halted);
        // Try to execute next opcode - should still be halted
        cpu.run_opcode();
        assert!(cpu.halted);
        // Reset should clear halt
        cpu.reset();
        assert!(!cpu.halted);
        // Load a simple NOP program and verify we can execute it
        let program2 = vec![NOP];
        fake_cartridge(&mut cpu, &program2);
        cpu.reset();
        cpu.run_opcode();
        assert!(!cpu.halted);
    }

    #[test]
    fn test_lar_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LAR_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x05;
        cpu.sp = 0xFD;
        cpu.memory.borrow_mut().write(0x1005, 0xAB);
        run(&mut cpu);
        // LAR: SP & M -> A, X, SP
        // 0xFD & 0xAB = 0xA9
        assert_eq!(cpu.a, 0xA9);
        assert_eq!(cpu.x, 0xA9);
        assert_eq!(cpu.sp, 0xA9);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Bit 7 is set
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_lax_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LAX_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0x55);
        run(&mut cpu);
        // LAX: Load both A and X with memory value
        assert_eq!(cpu.a, 0x55);
        assert_eq!(cpu.x, 0x55);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_lax_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LAX_ABSY, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0x80);
        run(&mut cpu);
        // LAX: Load both A and X with memory value
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.x, 0x80);
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Bit 7 is set
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_lax_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![LAX_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0x00);
        run(&mut cpu);
        // LAX: Load both A and X with memory value (0x00)
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.x, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag set
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_nop_undocumented_0x1a() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![NOP_IMP, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        let a_before = cpu.a;
        let x_before = cpu.x;
        let y_before = cpu.y;
        let p_before = cpu.p;
        run(&mut cpu);
        // NOP should not change any registers or flags
        assert_eq!(cpu.a, a_before);
        assert_eq!(cpu.x, x_before);
        assert_eq!(cpu.y, y_before);
        assert_eq!(cpu.p, p_before);
    }

    #[test]
    fn test_nop_undocumented_0xda() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![NOP_IMP5, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        cpu.x = 0x55;
        let a_before = cpu.a;
        let x_before = cpu.x;
        run(&mut cpu);
        // NOP should not change any registers
        assert_eq!(cpu.a, a_before);
        assert_eq!(cpu.x, x_before);
    }

    #[test]
    fn test_rla_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RLA_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x42, 0b0110_1010); // 0x6A
        cpu.a = 0b1111_0000; // 0xF0
        cpu.p &= !FLAG_CARRY; // Clear carry
        run(&mut cpu);
        // RLA: ROL memory (0x6A << 1 = 0xD4), then AND with A
        // Memory should be 0xD4, A should be 0xF0 & 0xD4 = 0xD0
        assert_eq!(cpu.memory.borrow().read(0x42), 0xD4);
        assert_eq!(cpu.a, 0xD0);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry clear (bit 7 was 0)
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Negative set
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_rla_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RLA_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0b1000_0001); // 0x81
        cpu.a = 0xFF;
        cpu.p |= FLAG_CARRY; // Set carry
        run(&mut cpu);
        // RLA: ROL memory (0x81 << 1 + carry = 0x03), then AND with A
        // Memory should be 0x03, A should be 0xFF & 0x03 = 0x03
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x03);
        assert_eq!(cpu.a, 0x03);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry set (bit 7 was 1)
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_rla_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![RLA_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0x01);
        cpu.a = 0x01;
        cpu.p &= !FLAG_CARRY;
        run(&mut cpu);
        // RLA: ROL memory (0x01 << 1 = 0x02), then AND with A
        // Memory should be 0x02, A should be 0x01 & 0x02 = 0x00
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x02);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag set
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rra_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RRA_ZP, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x10, 0b1010_1010); // 0xAA
        cpu.a = 0x10;
        cpu.p &= !FLAG_CARRY; // Clear carry
        run(&mut cpu);
        // RRA: ROR memory (0xAA >> 1 = 0x55), then ADC with A (0x10 + 0x55 = 0x65)
        assert_eq!(cpu.memory.borrow().read(0x10), 0x55);
        assert_eq!(cpu.a, 0x65);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from addition
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_rra_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![RRA_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0b0000_0001); // 0x01
        cpu.a = 0xFF;
        cpu.p |= FLAG_CARRY; // Set carry
        run(&mut cpu);
        // RRA: ROR memory (0x01 >> 1 with carry = 0x80), then ADC with A (0xFF + 0x80 + carry=1)
        // Memory rotates to 0x80 (carry goes into bit 7), bit 0 goes to carry
        // Then: 0xFF + 0x80 + 1 (carry from ROR) = 0x180 = 0x80 with carry set
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x80);
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from addition
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative
    }

    #[test]
    fn test_rra_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![RRA_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0b0000_0010); // 0x02
        cpu.a = 0x00;
        cpu.p &= !FLAG_CARRY;
        run(&mut cpu);
        // RRA: ROR memory (0x02 >> 1 = 0x01), then ADC with A (0x00 + 0x01 = 0x01)
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x01);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_sbc_immediate_undocumented() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SBC_IMM2, 0x01, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x05;
        cpu.p |= FLAG_CARRY; // Set carry (no borrow)
        run(&mut cpu);
        // Undocumented SBC: same as legal SBC #byte
        // 0x05 - 0x01 = 0x04
        assert_eq!(cpu.a, 0x04);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
        assert_eq!(cpu.p & FLAG_ZERO, 0);
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_slo_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SLO_ZP, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x10, 0b0101_0101); // 0x55
        cpu.a = 0b0000_1111; // 0x0F
        run(&mut cpu);
        // SLO: ASL memory (0x55 << 1 = 0xAA), then ORA with A (0x0F | 0xAA = 0xAF)
        assert_eq!(cpu.memory.borrow().read(0x10), 0xAA);
        assert_eq!(cpu.a, 0xAF);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from shift
        assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative
    }

    #[test]
    fn test_slo_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![SLO_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1005, 0b1000_0001); // 0x81
        cpu.a = 0b0000_0010; // 0x02
        run(&mut cpu);
        // SLO: ASL memory (0x81 << 1 = 0x02, carry set), then ORA with A (0x02 | 0x02 = 0x02)
        assert_eq!(cpu.memory.borrow().read(0x1005), 0x02);
        assert_eq!(cpu.a, 0x02);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from shift
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_slo_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![SLO_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0b0000_0001); // 0x01
        cpu.a = 0b0000_0000; // 0x00
        run(&mut cpu);
        // SLO: ASL memory (0x01 << 1 = 0x02), then ORA with A (0x00 | 0x02 = 0x02)
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x02);
        assert_eq!(cpu.a, 0x02);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_sre_zero_page() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x42, 0b0000_0110); // 0x06
        let program = vec![SRE_ZP, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0b0000_0001; // 0x01
        run(&mut cpu);
        // SRE: LSR memory (0x06 >> 1 = 0x03), then EOR with A (0x01 ^ 0x03 = 0x02)
        assert_eq!(cpu.memory.borrow().read(0x42), 0x03);
        assert_eq!(cpu.a, 0x02);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from shift
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_sre_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![SRE_ABSX, 0x00, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0b0000_0101); // 0x05
        cpu.a = 0b0000_0011; // 0x03
        run(&mut cpu);
        // SRE: LSR memory (0x05 >> 1 = 0x02 with carry), then EOR with A (0x03 ^ 0x02 = 0x01)
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x02);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from LSR
        assert_eq!(cpu.p & FLAG_ZERO, 0);
    }

    #[test]
    fn test_sre_indirect_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        cpu.memory.borrow_mut().write(0x20, 0x00);
        cpu.memory.borrow_mut().write(0x21, 0x10);
        let program = vec![SRE_INDY, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0x10;
        cpu.memory.borrow_mut().write(0x1010, 0b0000_1000); // 0x08
        cpu.a = 0b0000_0100; // 0x04
        run(&mut cpu);
        // SRE: LSR memory (0x08 >> 1 = 0x04), then EOR with A (0x04 ^ 0x04 = 0x00)
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x04);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Result is zero
    }

    #[test]
    fn test_sxa_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Test SXA with Absolute,Y addressing
        // SXA stores X AND (HIGH(addr) + 1) at the target address
        // If addr = 0x1000 and Y = 0x10, target = 0x1010
        // HIGH(0x1000) + 1 = 0x10 + 1 = 0x11
        // If X = 0xFF, result = 0xFF AND 0x11 = 0x11
        let program = vec![SXA_ABSY, 0x00, 0x10, KIL]; // SXA $1000,Y
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0xFF;
        cpu.y = 0x10;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x11); // X AND (0x10 + 1)
    }

    #[test]
    fn test_sya_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Test SYA with Absolute,X addressing
        // SYA stores Y AND (HIGH(addr) + 1) at the target address
        // If addr = 0x1000 and X = 0x10, target = 0x1010
        // HIGH(0x1000) + 1 = 0x10 + 1 = 0x11
        // If Y = 0xFF, result = 0xFF AND 0x11 = 0x11
        let program = vec![SYA_ABSX, 0x00, 0x10, KIL]; // SYA $1000,X
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.y = 0xFF;
        cpu.x = 0x10;
        run(&mut cpu);
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x11); // Y AND (0x10 + 1)
    }

    #[test]
    fn test_top_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Test TOP with Absolute addressing - should do nothing
        let program = vec![TOP_ABS, 0x00, 0x30, KIL]; // TOP $3000
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x42;
        run(&mut cpu);
        // TOP should not affect any registers or memory
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn test_top_absolute_x() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // Test TOP with Absolute,X addressing - should do nothing
        let program = vec![TOP_ABSX, 0x00, 0x30, KIL]; // TOP $3000,X
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x10;
        cpu.a = 0x42;
        run(&mut cpu);
        // TOP should not affect any registers or memory
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.x, 0x10);
    }

    #[test]
    fn test_xaa_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // XAA performs: A = (A | MAGIC) & X & immediate
        // Using MAGIC = 0xEE (common value)
        // A = 0xFF, X = 0xF0, immediate = 0x0F
        // Result: (0xFF | 0xEE) & 0xF0 & 0x0F = 0xFF & 0xF0 & 0x0F = 0x00
        let program = vec![XAA_IMM, 0x0F, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0xF0;
        run(&mut cpu);
        assert_eq!(cpu.a, 0x00);
        // Zero flag should be set
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
        // Negative flag should be clear
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    }

    #[test]
    fn test_xas_absolute_y() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // XAS performs: SP = A & X, then M = SP & (HIGH(addr) + 1)
        // A = 0xFF, X = 0xF0 -> SP = 0xF0
        // addr = 0x1000, Y = 0x10 -> effective addr = 0x1010
        // HIGH(0x1000) = 0x10, so result = 0xF0 & 0x11 = 0x10
        let program = vec![XAS_ABSY, 0x00, 0x10, KIL]; // XAS $1000,Y
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0xFF;
        cpu.x = 0xF0;
        cpu.y = 0x10;
        run(&mut cpu);
        // SP should be A & X
        assert_eq!(cpu.sp, 0xF0);
        // Memory at $1010 should be SP & (HIGH(addr) + 1) = 0xF0 & 0x11 = 0x10
        assert_eq!(cpu.memory.borrow().read(0x1010), 0x10);
    }

    // Cycle counting tests
    #[test]
    fn test_cycles_lda_immediate() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA #$42 - should take 2 cycles
        let program = vec![LDA_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 2);
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn test_cycles_lda_absolute() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA $1234 - should take 4 cycles
        let program = vec![LDA_ABS, 0x34, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1234, 0x55);
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 4);
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn test_cycles_lda_absolute_x_no_page_cross() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA $1200,X with X=$05 -> $1205 (no page cross)
        // Should take 4 cycles (base)
        let program = vec![LDA_ABSX, 0x00, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1205, 0x77);
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 4);
        assert_eq!(cpu.a, 0x77);
    }

    #[test]
    fn test_cycles_lda_absolute_x_with_page_cross() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA $12FF,X with X=$05 -> $1304 (page cross from $12 to $13)
        // Should take 5 cycles (4 base + 1 for page cross)
        let program = vec![LDA_ABSX, 0xFF, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.x = 0x05;
        cpu.memory.borrow_mut().write(0x1304, 0x88);
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 5);
        assert_eq!(cpu.a, 0x88);
    }

    #[test]
    fn test_cycles_sta_absolute_x_no_extra_cycle() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // STA $12FF,X with X=$05 -> $1304 (page cross)
        // STA always takes 5 cycles regardless of page crossing
        let program = vec![STA_ABSX, 0xFF, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.a = 0x99;
        cpu.x = 0x05;
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 5);
        assert_eq!(cpu.memory.borrow().read(0x1304), 0x99);
    }

    #[test]
    fn test_cycles_branch_not_taken() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // BEQ with Z flag clear - branch not taken
        // Should take 2 cycles
        let program = vec![BEQ, 0x10, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p &= !FLAG_ZERO; // Clear zero flag
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 2);
    }

    #[test]
    fn test_cycles_branch_taken_no_page_cross() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // BEQ +5 with Z flag set - branch taken, no page cross
        // Should take 3 cycles (2 base + 1 for branch taken)
        let program = vec![BEQ, 0x05, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 3);
    }

    #[test]
    fn test_cycles_branch_taken_with_page_cross() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // BEQ with negative offset causing page cross
        // PC after instruction read: 0x8002
        // Branch to: 0x8002 + (-3) = 0x7FFF (crosses from page 0x80 to 0x7F)
        // Should take 4 cycles (2 base + 1 for branch + 1 for page cross)
        let program = vec![BEQ, 0xFD, KIL]; // -3 offset (0xFD as i8)
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.p |= FLAG_ZERO; // Set zero flag
        let cycles = cpu.run_opcode();
        assert_eq!(cycles, 4);
    }

    // Cycle counter tests
    #[test]
    fn test_cycle_counter_starts_at_zero() {
        let memory = create_test_memory();
        let cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        assert_eq!(cpu.total_cycles(), 0);
    }

    #[test]
    fn test_cycle_counter_increments_on_instruction() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA #$42 - should take 2 cycles
        let program = vec![LDA_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        assert_eq!(cpu.total_cycles(), 0);
        cpu.run_opcode();
        assert_eq!(cpu.total_cycles(), 2);
    }

    #[test]
    fn test_cycle_counter_accumulates_multiple_instructions() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA #$42 (2 cycles), LDX #$10 (2 cycles), LDY #$20 (2 cycles)
        let program = vec![LDA_IMM, 0x42, LDX_IMM, 0x10, LDY_IMM, 0x20, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.run_opcode(); // LDA - 2 cycles
        assert_eq!(cpu.total_cycles(), 2);
        cpu.run_opcode(); // LDX - 2 cycles
        assert_eq!(cpu.total_cycles(), 4);
        cpu.run_opcode(); // LDY - 2 cycles
        assert_eq!(cpu.total_cycles(), 6);
    }

    #[test]
    fn test_cycle_counter_resets_to_zero() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        let program = vec![LDA_IMM, 0x42, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.run_opcode();
        assert_eq!(cpu.total_cycles(), 2);
        cpu.reset();
        assert_eq!(cpu.total_cycles(), 0);
    }

    #[test]
    fn test_cycle_counter_with_page_crossing() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));
        // LDA $12FF,X with X=$05 - should take 5 cycles (4 base + 1 for page cross)
        let program = vec![LDA_ABSX, 0xFF, 0x12, KIL];
        fake_cartridge(&mut cpu, &program);
        cpu.reset();
        cpu.memory.borrow_mut().write(0x1304, 0x99);
        cpu.x = 0x05;
        cpu.run_opcode();
        assert_eq!(cpu.total_cycles(), 5);
    }

    #[test]
    fn test_brk_interrupt() {
        let memory = create_test_memory();
        let mut cpu = Cpu::new(Rc::new(RefCell::new(memory)));

        // Setup: BRK at 0x8000, with IRQ handler at 0x9000 that contains RTI
        // We need to manually create a cartridge that has the IRQ vector set up
        let mut prg_rom = vec![0; 0x4000]; // 16KB

        // Place BRK at the beginning (0x8000)
        prg_rom[0] = BRK;
        // Place NOP at 0x8002 (where we should return after RTI)
        prg_rom[2] = NOP;

        // Place RTI at 0x9000 (IRQ handler)
        // 0x9000 - 0x8000 = 0x1000
        prg_rom[0x1000] = RTI;

        // Set reset vector to point to 0x8000
        prg_rom[0x3FFC] = 0x00; // Low byte of 0x8000
        prg_rom[0x3FFD] = 0x80; // High byte of 0x8000

        // Set IRQ/BRK vector to point to 0x9000
        // IRQ vector is at 0xFFFE-0xFFFF
        // For 16KB ROM: (0xFFFE - 0x8000) % 0x4000 = 0x7FFE % 0x4000 = 0x3FFE
        prg_rom[0x3FFE] = 0x00; // Low byte of 0x9000
        prg_rom[0x3FFF] = 0x90; // High byte of 0x9000

        let chr_rom = vec![0; 0x2000];
        let cartridge = crate::cartridge::Cartridge {
            prg_rom,
            chr_rom,
            mirroring: crate::cartridge::MirroringMode::Horizontal,
        };

        cpu.memory.borrow_mut().map_cartridge(cartridge);
        cpu.reset();

        // Set initial status register to a known value (without I flag set, with carry, overflow, and zero set)
        cpu.p = 0b0110_0011; // Overflow, unused, zero, and carry flags set (NO I flag - bit 2 is 0)
        let initial_p = cpu.p;
        let initial_sp = cpu.sp;

        // Execute BRK
        cpu.run_opcode();

        // Verify PC was loaded from IRQ vector
        assert_eq!(
            cpu.pc, 0x9000,
            "PC should be loaded from IRQ vector at 0xFFFE-0xFFFF"
        );

        // Verify stack: should have pushed PC+2 (high byte first), then P
        // PC was at 0x8000, BRK is 1 byte, but we push PC+2 = 0x8002
        let stack_base = 0x0100;
        assert_eq!(
            cpu.memory.borrow().read(stack_base + initial_sp as u16),
            0x80,
            "High byte of PC+2 should be pushed first"
        );
        assert_eq!(
            cpu.memory
                .borrow()
                .read(stack_base + initial_sp.wrapping_sub(1) as u16),
            0x02,
            "Low byte of PC+2 should be pushed second"
        );

        // Verify P was pushed with B flag and unused flag set
        let pushed_p = cpu
            .memory
            .borrow()
            .read(stack_base + initial_sp.wrapping_sub(2) as u16);
        assert_eq!(
            pushed_p & FLAG_BREAK,
            FLAG_BREAK,
            "B flag should be set in pushed P"
        );
        assert_eq!(
            pushed_p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag should be set in pushed P"
        );
        // Verify the other flags from initial P are preserved in pushed P
        assert_eq!(
            pushed_p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "Overflow flag should be preserved in pushed P"
        );
        assert_eq!(
            pushed_p & FLAG_CARRY,
            FLAG_CARRY,
            "Carry flag should be preserved in pushed P"
        );
        assert_eq!(
            pushed_p & FLAG_ZERO,
            FLAG_ZERO,
            "Zero flag should be preserved in pushed P"
        );

        // Verify stack pointer was decremented by 3
        assert_eq!(
            cpu.sp,
            initial_sp.wrapping_sub(3),
            "Stack pointer should be decremented by 3"
        );

        // Verify I flag is set in current P register
        assert_eq!(
            cpu.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set after BRK"
        );

        // Verify CPU is not halted (BRK should not halt like KIL)
        assert_eq!(cpu.halted, false, "BRK should not halt the CPU");

        // Now execute RTI to return from interrupt
        let sp_before_rti = cpu.sp;
        cpu.run_opcode();

        // Verify PC was restored to 0x8002 (PC+2 from BRK)
        assert_eq!(cpu.pc, 0x8002, "PC should be restored to 0x8002 after RTI");

        // Verify stack pointer was incremented by 3
        assert_eq!(
            cpu.sp,
            sp_before_rti.wrapping_add(3),
            "Stack pointer should be incremented by 3 after RTI"
        );
        assert_eq!(
            cpu.sp, initial_sp,
            "Stack pointer should be back to initial value"
        );

        // Verify P was restored (RTI should restore P without B flag, but with original flags)
        // RTI ignores the B flag from the stack, so we should get back initial_p without B flag
        let expected_p = (initial_p & !FLAG_BREAK) | FLAG_UNUSED; // B cleared, unused always set
        assert_eq!(
            cpu.p, expected_p,
            "P should be restored after RTI (without B flag)"
        );
        assert_eq!(
            cpu.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "Overflow flag should be restored"
        );
        assert_eq!(
            cpu.p & FLAG_CARRY,
            FLAG_CARRY,
            "Carry flag should be restored"
        );
        assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be restored");
        assert_eq!(
            cpu.p & FLAG_INTERRUPT,
            0,
            "I flag should be cleared after RTI"
        );
    }
}
