use super::addressing::{
    Absolute, AbsoluteX, AbsoluteY, Immediate, Implied, IndexedIndirect, Indirect, IndirectIndexed,
    MemoryAccess, Relative, ZeroPage, ZeroPageX, ZeroPageY,
};
use super::instruction::Instruction;
use super::instruction_types::{
    Aac, Adc, And, Arr, Asl, AslA, Asr, Atx, Axa, Axs, Bcc, Bcs, Beq, Bit, Bmi, Bne, Bpl, Brk, Bvc,
    Bvs, Clc, Cld, Cli, Clv, Cmp, Cpx, Cpy, Dcp, Dec, Dex, Dey, Dop, Eor, Inc, Inx, Iny, Isb, Jmp,
    Jsr, Kil, Lar, Lax, Lda, Ldx, Ldy, Lsr, LsrA, Nop, Ora, Pha, Php, Pla, Plp, Rla, Rol, RolA,
    Ror, RorA, Rra, Rti, Rts, Sax, Sbc, Sec, Sed, Sei, Slo, Sre, Sta, Stx, Sty, Sxa, Sya, Tax, Tay,
    Top, Tsx, Txa, Txs, Tya, Xaa, Xas,
};
use super::traits::{
    AAC_IMM, AAC_IMM2, ADC_ABS, ADC_ABSX, ADC_ABSY, ADC_IMM, ADC_INDX, ADC_INDY, ADC_ZP, ADC_ZPX,
    AND_ABS, AND_ABSX, AND_ABSY, AND_IMM, AND_INDX, AND_INDY, AND_ZP, AND_ZPX, ARR_IMM, ASL_A,
    ASL_ABS, ASL_ABSX, ASL_ZP, ASL_ZPX, ASR_IMM, ATX_IMM, AXA_ABSY, AXA_INDY, AXS_IMM, BCC, BCS,
    BEQ, BIT_ABS, BIT_ZP, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP_ABS, CMP_ABSX,
    CMP_ABSY, CMP_IMM, CMP_INDX, CMP_INDY, CMP_ZP, CMP_ZPX, CPX_ABS, CPX_IMM, CPX_ZP, CPY_ABS,
    CPY_IMM, CPY_ZP, DCP_ABS, DCP_ABSX, DCP_ABSY, DCP_INDX, DCP_INDY, DCP_ZP, DCP_ZPX, DEC_ABS,
    DEC_ABSX, DEC_ZP, DEC_ZPX, DEX, DEY, DOP_IMM, DOP_IMM2, DOP_IMM3, DOP_IMM4, DOP_IMM5, DOP_ZP,
    DOP_ZP2, DOP_ZP3, DOP_ZPX, DOP_ZPX2, DOP_ZPX3, DOP_ZPX4, DOP_ZPX5, DOP_ZPX6, EOR_ABS, EOR_ABSX,
    EOR_ABSY, EOR_IMM, EOR_INDX, EOR_INDY, EOR_ZP, EOR_ZPX, INC_ABS, INC_ABSX, INC_ZP, INC_ZPX,
    INX, INY, ISB_ABS, ISB_ABSX, ISB_ABSY, ISB_INDX, ISB_INDY, ISB_ZP, ISB_ZPX, JMP_ABS, JMP_IND,
    JSR, KIL, KIL2, KIL3, KIL4, KIL5, KIL6, KIL7, KIL8, KIL9, KIL10, KIL11, KIL12, LAR_ABSY,
    LAX_ABS, LAX_ABSY, LAX_INDX, LAX_INDY, LAX_ZP, LAX_ZPY, LDA_ABS, LDA_ABSX, LDA_ABSY, LDA_IMM,
    LDA_INDX, LDA_INDY, LDA_ZP, LDA_ZPX, LDX_ABS, LDX_ABSY, LDX_IMM, LDX_ZP, LDX_ZPY, LDY_ABS,
    LDY_ABSX, LDY_IMM, LDY_ZP, LDY_ZPX, LSR_ABS, LSR_ABSX, LSR_ACC, LSR_ZP, LSR_ZPX, NOP, NOP_IMP,
    NOP_IMP2, NOP_IMP3, NOP_IMP4, NOP_IMP5, NOP_IMP6, ORA_ABS, ORA_ABSX, ORA_ABSY, ORA_IMM,
    ORA_INDX, ORA_INDY, ORA_ZP, ORA_ZPX, PHA, PHP, PLA, PLP, RLA_ABS, RLA_ABSX, RLA_ABSY, RLA_INDX,
    RLA_INDY, RLA_ZP, RLA_ZPX, ROL_ABS, ROL_ABSX, ROL_ACC, ROL_ZP, ROL_ZPX, ROR_ABS, ROR_ABSX,
    ROR_ACC, ROR_ZP, ROR_ZPX, RRA_ABS, RRA_ABSX, RRA_ABSY, RRA_INDX, RRA_INDY, RRA_ZP, RRA_ZPX,
    RTI, RTS, SAX_ABS, SAX_INDX, SAX_ZP, SAX_ZPY, SBC_ABS, SBC_ABSX, SBC_ABSY, SBC_IMM, SBC_IMM2,
    SBC_INDX, SBC_INDY, SBC_ZP, SBC_ZPX, SEC, SED, SEI, SLO_ABS, SLO_ABSX, SLO_ABSY, SLO_INDX,
    SLO_INDY, SLO_ZP, SLO_ZPX, SRE_ABS, SRE_ABSX, SRE_ABSY, SRE_INDX, SRE_INDY, SRE_ZP, SRE_ZPX,
    STA_ABS, STA_ABSX, STA_ABSY, STA_INDX, STA_INDY, STA_ZP, STA_ZPX, STX_ABS, STX_ZP, STX_ZPY,
    STY_ABS, STY_ZP, STY_ZPX, SXA_ABSY, SYA_ABSX, TAX, TAY, TOP_ABS, TOP_ABSX, TOP_ABSX2,
    TOP_ABSX3, TOP_ABSX4, TOP_ABSX5, TOP_ABSX6, TSX, TXA, TXS, TYA, XAA_IMM, XAS_ABSY,
};
use super::types::{
    FLAG_BREAK, FLAG_CARRY, FLAG_DECIMAL, FLAG_INTERRUPT, FLAG_NEGATIVE, FLAG_OVERFLOW,
    FLAG_UNUSED, FLAG_ZERO, IRQ_VECTOR, NMI_VECTOR, RESET_VECTOR, STACK_BASE,
};
use crate::cpu2::CpuState;
use crate::mem_controller::MemController;
use core::panic;
use std::cell::RefCell;
use std::rc::Rc;

/// NES 6502 CPU
pub struct Cpu2 {
    /// State of the CPU
    state: CpuState,
    /// Memory
    memory: Rc<RefCell<MemController>>,
    /// Halted state (set by KIL instruction)
    halted: bool,
    /// Total cycles executed since last reset
    total_cycles: u64,
    /// Current instruction being executed
    current_instruction: Option<Instruction>,
    /// NMI pending flag - set by external hardware (NES loop)
    /// Checked during BRK execution to determine vector hijacking
    pub nmi_pending: bool,
    /// IRQ pending flag - set by external hardware (NES loop)
    /// Checked at end of instructions if I flag is clear
    pub irq_pending: bool,
    /// Track if we're currently in an interrupt sequence
    /// Used to prevent interrupt polling during interrupt handler execution
    in_interrupt_sequence: bool,
    /// Delay IRQ polling by one instruction after CLI/SEI/PLP
    /// This implements the hardware behavior where these instructions
    /// allow exactly one instruction to execute before IRQ is checked
    delay_interrupt_check: bool,
    /// The I flag value to use for interrupt polling during delay period
    /// When CLI/SEI/PLP execute, they save the OLD I flag value here,
    /// and interrupt polling uses this value during the delay period
    saved_i_flag_for_delay: bool,
}

impl Cpu2 {
    /// Create a new CPU with default register values at power-on
    pub fn new(memory: Rc<RefCell<MemController>>) -> Self {
        Self {
            state: CpuState {
                a: 0,
                x: 0,
                y: 0,
                sp: 0x00, // Stack pointer starts at 0x00 at power-on. The automatic reset
                // sequence then subtracts 3, resulting in SP=0xFD when the reset
                // handler first runs.
                pc: 0,          // Program counter will be loaded from reset vector
                p: FLAG_UNUSED, // Status at power-on before reset: only unused bit set (bit 5)
                delay_interrupt_check: false,
                saved_i_flag: false,
            },
            memory,
            halted: false,
            total_cycles: 0,
            current_instruction: None,
            nmi_pending: false,
            irq_pending: false,
            in_interrupt_sequence: false,
            delay_interrupt_check: false,
            saved_i_flag_for_delay: false,
        }
    }

    /// Check if an opcode is a KIL instruction (any of the 12 variants)
    fn is_kil_opcode(opcode: u8) -> bool {
        matches!(
            opcode,
            KIL | KIL2 | KIL3 | KIL4 | KIL5 | KIL6 | KIL7 | KIL8 | KIL9 | KIL10 | KIL11 | KIL12
        )
    }

    /// Execute a single CPU cycle
    /// Returns true when the current instruction completes
    pub fn tick_cycle(&mut self) -> bool {
        if self.halted {
            return true;
        }

        // If no current instruction, fetch and decode a new one
        if self.current_instruction.is_none() {
            let opcode = self.memory.borrow().read(self.state.pc);
            if let Some(instruction) = Self::decode(opcode) {
                self.state.pc = self.state.pc.wrapping_add(1);
                self.current_instruction = Some(instruction);
                self.total_cycles += 1;

                // Check if this is KIL - it halts the CPU immediately
                if Self::is_kil_opcode(opcode) {
                    self.halted = true;
                }

                return false;
            } else {
                // Unimplemented opcode - halt
                panic!(
                    "Unimplemented opcode {:02X} at address {:04X}",
                    opcode, self.state.pc
                );
            }
        }

        // Execute one cycle of the current instruction
        if let Some(ref mut instruction) = self.current_instruction {
            instruction.tick(&mut self.state, Rc::clone(&self.memory));

            // Check if both addressing and instruction are done
            if instruction.is_done() {
                self.current_instruction = None;
                self.total_cycles += 1;

                // Clear in_interrupt_sequence flag when an instruction completes
                // This allows interrupt polling to happen after at least one
                // instruction has executed from the interrupt handler
                self.in_interrupt_sequence = false;

                // Handle interrupt check delay (1-instruction delay after CLI/SEI/PLP)
                // If a delay was just requested by the current instruction, activate it
                // and save the CURRENT I flag value (before the instruction changed it)
                if self.state.delay_interrupt_check {
                    self.delay_interrupt_check = true;
                    // Save the I flag value BEFORE the instruction modified it
                    // The instruction should have saved this in the state
                    self.saved_i_flag_for_delay = self.state.saved_i_flag;
                    self.state.delay_interrupt_check = false;
                } else if self.delay_interrupt_check {
                    // Delay was active - one instruction has now executed, so clear it
                    self.delay_interrupt_check = false;
                }

                return true; // Instruction completed
            }
        }

        self.total_cycles += 1;
        false // Instruction not yet complete
    }

    /// Decode an opcode into an Instruction
    ///
    /// Creates the appropriate InstructionType and AddressingMode based on the opcode.
    /// Returns None if the opcode is not implemented.
    ///
    /// This is an associated function (not a method) since it doesn't depend on instance state.
    pub fn decode(opcode: u8) -> Option<Instruction> {
        match opcode {
            BRK => {
                // BRK uses Implied addressing since it doesn't use operands
                Some(Instruction::new(Box::new(Implied), Box::new(Brk::new())))
            }
            ORA_INDX => {
                // ORA Indexed Indirect: ORA (zp,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Ora::new()),
                ))
            }
            KIL => {
                // KIL uses Implied addressing - it halts the CPU
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            SLO_INDX => {
                // SLO Indexed Indirect: SLO (zp,X) - shift left and OR
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Slo::new()),
                ))
            }
            DOP_ZP => {
                // DOP Zero Page - read and discard (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dop::new()),
                ))
            }
            ORA_ZP => {
                // ORA Zero Page: ORA zp
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ZP => {
                // ASL Zero Page: ASL zp - Arithmetic Shift Left
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ZP => {
                // SLO Zero Page: SLO zp - Shift left and OR (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Slo::new()),
                ))
            }
            PHP => {
                // PHP - Push Processor Status
                Some(Instruction::new(Box::new(Implied), Box::new(Php::new())))
            }
            ORA_IMM => {
                // ORA Immediate: ORA #imm
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Ora::new()),
                ))
            }
            ASL_A => {
                // ASL A - Arithmetic Shift Left Accumulator
                Some(Instruction::new(Box::new(Implied), Box::new(AslA::new())))
            }
            AAC_IMM => {
                // AAC #imm - AND with Carry (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Aac::new()),
                ))
            }
            TOP_ABS => {
                // TOP abs - Triple NOP (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            ORA_ABS => {
                // ORA Absolute: ORA abs
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ABS => {
                // ASL Absolute: ASL abs
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ABS => {
                // SLO Absolute: SLO abs (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Slo::new()),
                ))
            }
            BPL => {
                // BPL Relative: BPL offset
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bpl::new()),
                ))
            }
            ORA_INDY => {
                // ORA (Indirect),Y: ORA ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Ora::new()),
                ))
            }
            KIL2 => {
                // KIL (illegal opcode - halts CPU)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            SLO_INDY => {
                // SLO (Indirect),Y: SLO ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Slo::new()),
                ))
            }
            DOP_ZPX => {
                // DOP Zero Page,X: DOP $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            ORA_ZPX => {
                // ORA Zero Page,X: ORA $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ZPX => {
                // ASL Zero Page,X: ASL $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ZPX => {
                // SLO Zero Page,X: SLO $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Slo::new()),
                ))
            }
            CLC => {
                // CLC: Clear Carry Flag
                Some(Instruction::new(Box::new(Implied), Box::new(Clc::new())))
            }
            ORA_ABSY => {
                // ORA Absolute,Y: ORA abs,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Ora::new()),
                ))
            }
            NOP_IMP => {
                // NOP Implied (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            SLO_ABSY => {
                // SLO Absolute,Y: SLO abs,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Slo::new()),
                ))
            }
            TOP_ABSX => {
                // TOP Absolute,X: TOP abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            ORA_ABSX => {
                // ORA Absolute,X: ORA abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ABSX => {
                // ASL Absolute,X: ASL abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ABSX => {
                // SLO Absolute,X: SLO abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Slo::new()),
                ))
            }
            JSR => {
                // JSR handles its own address fetching internally, so we use Implied addressing
                Some(Instruction::new(Box::new(Implied), Box::new(Jsr::new())))
            }
            AND_INDX => {
                // AND Indexed Indirect: AND (zp,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(And::new()),
                ))
            }
            KIL3 => {
                // KIL uses Implied addressing - it halts the CPU
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            RLA_INDX => {
                // RLA Indexed Indirect: RLA (zp,X) - rotate left and AND
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Rla::new()),
                ))
            }
            BIT_ZP => {
                // BIT Zero Page: BIT zp - test bits in memory with accumulator
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Bit::new()),
                ))
            }
            AND_ZP => {
                // AND Zero Page: AND zp
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(And::new()),
                ))
            }
            ROL_ZP => {
                // ROL Zero Page: ROL zp - Rotate Left
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ZP => {
                // RLA Zero Page: RLA zp - Rotate left and AND (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Rla::new()),
                ))
            }
            PLP => {
                // PLP - Pull Processor Status
                Some(Instruction::new(Box::new(Implied), Box::new(Plp::new())))
            }
            AND_IMM => {
                // AND Immediate: AND #imm
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(And::new()),
                ))
            }
            ROL_ACC => {
                // ROL Accumulator: ROL A
                Some(Instruction::new(Box::new(Implied), Box::new(RolA::new())))
            }
            AAC_IMM2 => {
                // AAC Immediate (illegal opcode) - AND byte with accumulator and copy result to carry
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Aac::new()),
                ))
            }
            BIT_ABS => {
                // BIT Absolute: BIT abs
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Bit::new()),
                ))
            }
            AND_ABS => {
                // AND Absolute: AND abs
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(And::new()),
                ))
            }
            ROL_ABS => {
                // ROL Absolute: ROL abs
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ABS => {
                // RLA Absolute: RLA abs (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Rla::new()),
                ))
            }
            BMI => {
                // BMI: Branch if Minus (negative flag set)
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bmi::new()),
                ))
            }
            AND_INDY => {
                // AND Indirect,Y: AND (zp),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(And::new()),
                ))
            }
            KIL4 => {
                // KIL uses Implied addressing - it halts the CPU
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            RLA_INDY => {
                // RLA Indirect,Y: RLA (zp),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rla::new()),
                ))
            }
            DOP_ZPX2 => {
                // DOP Zero Page,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            AND_ZPX => {
                // AND Zero Page,X: AND zp,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(And::new()),
                ))
            }
            ROL_ZPX => {
                // ROL Zero Page,X: ROL zp,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ZPX => {
                // RLA Zero Page,X: RLA zp,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Rla::new()),
                ))
            }
            SEC => {
                // SEC: Set Carry Flag
                Some(Instruction::new(Box::new(Implied), Box::new(Sec::new())))
            }
            AND_ABSY => {
                // AND Absolute,Y: AND abs,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(And::new()),
                ))
            }
            NOP_IMP2 => {
                // NOP Implied (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            RLA_ABSY => {
                // RLA Absolute,Y: RLA abs,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rla::new()),
                ))
            }
            TOP_ABSX2 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            AND_ABSX => {
                // AND Absolute,X: AND abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(And::new()),
                ))
            }
            ROL_ABSX => {
                // ROL Absolute,X: ROL abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ABSX => {
                // RLA Absolute,X: RLA abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rla::new()),
                ))
            }
            RTI => {
                // RTI: Return from Interrupt
                Some(Instruction::new(Box::new(Implied), Box::new(Rti::new())))
            }
            EOR_INDX => {
                // EOR (Indirect,X): EOR ($nn,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Eor::new()),
                ))
            }
            KIL5 => {
                // KIL (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            SRE_INDX => {
                // SRE (Indirect,X): SRE ($nn,X) (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Sre::new()),
                ))
            }
            DOP_ZP2 => {
                // DOP Zero Page (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dop::new()),
                ))
            }
            EOR_ZP => {
                // EOR Zero Page: EOR $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ZP => {
                // LSR Zero Page: LSR $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ZP => {
                // SRE Zero Page: SRE $nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Sre::new()),
                ))
            }
            PHA => {
                // PHA: Push Accumulator
                Some(Instruction::new(Box::new(Implied), Box::new(Pha::new())))
            }
            EOR_IMM => {
                // EOR Immediate: EOR #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ACC => {
                // LSR Accumulator: LSR A
                Some(Instruction::new(Box::new(Implied), Box::new(LsrA::new())))
            }
            ASR_IMM => {
                // ASR Immediate: ASR #$nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Asr::new()),
                ))
            }
            JMP_ABS => {
                // JMP Absolute handles its own address fetching internally, like JSR
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Jump)),
                    Box::new(Jmp::new()),
                ))
            }
            EOR_ABS => {
                // EOR Absolute: EOR $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ABS => {
                // LSR Absolute: LSR $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ABS => {
                // SRE Absolute: SRE $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Sre::new()),
                ))
            }
            BVC => {
                // BVC: Branch if Overflow Clear
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bvc::new()),
                ))
            }
            RTS => {
                // RTS: Return from Subroutine
                Some(Instruction::new(Box::new(Implied), Box::new(Rts::new())))
            }
            ADC_INDX => {
                // ADC: Add with Carry (Indexed Indirect)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Adc::new()),
                ))
            }
            KIL7 => {
                // KIL: Halt and Catch Fire
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            RRA_INDX => {
                // RRA: Rotate Right then Add with Carry (Indexed Indirect, illegal)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Rra::new()),
                ))
            }
            DOP_ZP3 => {
                // DOP: Double NOP (Zero Page, illegal)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dop::new()),
                ))
            }
            ADC_ZP => {
                // ADC: Add with Carry (Zero Page)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ZP => {
                // ROR: Rotate Right (Zero Page)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ZP => {
                // RRA: Rotate Right then Add with Carry (Zero Page, illegal)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Rra::new()),
                ))
            }
            PLA => {
                // PLA: Pull Accumulator from Stack
                Some(Instruction::new(Box::new(Implied), Box::new(Pla::new())))
            }
            ADC_IMM => {
                // ADC: Add with Carry (Immediate)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ACC => {
                // ROR: Rotate Right (Accumulator)
                Some(Instruction::new(Box::new(Implied), Box::new(RorA::new())))
            }
            ARR_IMM => {
                // ARR: AND then Rotate Right (Immediate, illegal)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Arr::new()),
                ))
            }
            JMP_IND => {
                // JMP: Jump (Indirect)
                Some(Instruction::new(
                    Box::new(Indirect::new()),
                    Box::new(Jmp::new()),
                ))
            }
            ADC_ABS => {
                // ADC: Add with Carry (Absolute)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ABS => {
                // ROR: Rotate Right (Absolute)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ABS => {
                // RRA: Rotate Right then Add with Carry (Absolute, illegal)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Rra::new()),
                ))
            }
            EOR_INDY => {
                // EOR (Indirect),Y: EOR ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Eor::new()),
                ))
            }
            KIL6 => {
                // KIL (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            SRE_INDY => {
                // SRE (Indirect),Y: SRE ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Sre::new()),
                ))
            }
            DOP_ZPX3 => {
                // DOP Zero Page,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            EOR_ZPX => {
                // EOR Zero Page,X: EOR $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ZPX => {
                // LSR Zero Page,X: LSR $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ZPX => {
                // SRE Zero Page,X: SRE $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Sre::new()),
                ))
            }
            CLI => {
                // CLI: Clear Interrupt Disable Flag
                Some(Instruction::new(Box::new(Implied), Box::new(Cli::new())))
            }
            BVS => {
                // BVS: Branch if Overflow Set
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bvs::new()),
                ))
            }
            ADC_INDY => {
                // ADC (Indirect),Y: ADC ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Adc::new()),
                ))
            }
            KIL8 => {
                // KIL (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            RRA_INDY => {
                // RRA (Indirect),Y: RRA ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rra::new()),
                ))
            }
            DOP_ZPX4 => {
                // DOP Zero Page,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            ADC_ZPX => {
                // ADC Zero Page,X: ADC $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ZPX => {
                // ROR Zero Page,X: ROR $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ZPX => {
                // RRA Zero Page,X: RRA $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Rra::new()),
                ))
            }
            SEI => {
                // SEI: Set Interrupt Disable Flag
                Some(Instruction::new(Box::new(Implied), Box::new(Sei::new())))
            }
            ADC_ABSY => {
                // ADC Absolute,Y: ADC $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Adc::new()),
                ))
            }
            NOP_IMP4 => {
                // NOP Implied (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            RRA_ABSY => {
                // RRA Absolute,Y: RRA $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rra::new()),
                ))
            }
            TOP_ABSX4 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            ADC_ABSX => {
                // ADC Absolute,X: ADC $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ABSX => {
                // ROR Absolute,X: ROR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ABSX => {
                // RRA Absolute,X: RRA $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Rra::new()),
                ))
            }
            DOP_IMM => {
                // DOP Immediate (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Dop::new()),
                ))
            }
            STA_INDX => {
                // STA (Indirect,X): STA ($nn,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Write)),
                    Box::new(Sta::new()),
                ))
            }
            DOP_IMM2 => {
                // DOP Immediate (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Dop::new()),
                ))
            }
            SAX_INDX => {
                // SAX (Indirect,X): SAX ($nn,X) (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Write)),
                    Box::new(Sax::new()),
                ))
            }
            STY_ZP => {
                // STY Zero Page: STY $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Sty::new()),
                ))
            }
            STA_ZP => {
                // STA Zero Page: STA $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Sta::new()),
                ))
            }
            STX_ZP => {
                // STX Zero Page: STX $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Stx::new()),
                ))
            }
            SAX_ZP => {
                // SAX Zero Page: SAX $nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Sax::new()),
                ))
            }
            DEY => {
                // DEY: Decrement Y
                Some(Instruction::new(Box::new(Implied), Box::new(Dey::new())))
            }
            DOP_IMM3 => {
                // DOP Immediate (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Dop::new()),
                ))
            }
            TXA => {
                // TXA: Transfer X to A
                Some(Instruction::new(Box::new(Implied), Box::new(Txa::new())))
            }
            XAA_IMM => {
                // XAA Immediate (illegal opcode, highly unstable)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Xaa::new()),
                ))
            }
            STY_ABS => {
                // STY Absolute: STY $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Write)),
                    Box::new(Sty::new()),
                ))
            }
            STA_ABS => {
                // STA Absolute: STA $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Write)),
                    Box::new(Sta::new()),
                ))
            }
            STX_ABS => {
                // STX Absolute: STX $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Write)),
                    Box::new(Stx::new()),
                ))
            }
            SAX_ABS => {
                // SAX Absolute: SAX $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Write)),
                    Box::new(Sax::new()),
                ))
            }
            LDY_IMM => {
                // LDY Immediate: LDY #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Ldy::new()),
                ))
            }
            LDA_INDX => {
                // LDA Indexed Indirect: LDA ($nn,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Lda::new()),
                ))
            }
            LDX_IMM => {
                // LDX Immediate: LDX #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Ldx::new()),
                ))
            }
            LAX_INDX => {
                // LAX Indexed Indirect: LAX ($nn,X) (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Lax::new()),
                ))
            }
            LDY_ZP => {
                // LDY Zero Page: LDY $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Ldy::new()),
                ))
            }
            LDA_ZP => {
                // LDA Zero Page: LDA $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Lda::new()),
                ))
            }
            LDX_ZP => {
                // LDX Zero Page: LDX $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Ldx::new()),
                ))
            }
            LAX_ZP => {
                // LAX Zero Page: LAX $nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Lax::new()),
                ))
            }
            TAY => {
                // TAY: Transfer A to Y
                Some(Instruction::new(Box::new(Implied), Box::new(Tay::new())))
            }
            LDA_IMM => {
                // LDA Immediate: LDA #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Lda::new()),
                ))
            }
            TAX => {
                // TAX: Transfer A to X
                Some(Instruction::new(Box::new(Implied), Box::new(Tax::new())))
            }
            ATX_IMM => {
                // ATX Immediate: ATX #$nn (illegal, unstable opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Atx::new()),
                ))
            }
            LDY_ABS => {
                // LDY Absolute: LDY $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Ldy::new()),
                ))
            }
            LDA_ABS => {
                // LDA Absolute: LDA $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Lda::new()),
                ))
            }
            LDX_ABS => {
                // LDX Absolute: LDX $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Ldx::new()),
                ))
            }
            LAX_ABS => {
                // LAX Absolute: LAX $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Lax::new()),
                ))
            }
            BCS => {
                // BCS: Branch if Carry Set
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bcs::new()),
                ))
            }
            LDA_INDY => {
                // LDA (Indirect),Y: LDA ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Lda::new()),
                ))
            }
            KIL10 => {
                // KIL (illegal opcode - halt)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            LAX_INDY => {
                // LAX (Indirect),Y: LAX ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Lax::new()),
                ))
            }
            LDY_ZPX => {
                // LDY Zero Page,X: LDY $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Ldy::new()),
                ))
            }
            LDA_ZPX => {
                // LDA Zero Page,X: LDA $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Lda::new()),
                ))
            }
            LDX_ZPY => {
                // LDX Zero Page,Y: LDX $nn,Y
                Some(Instruction::new(
                    Box::new(ZeroPageY::new()),
                    Box::new(Ldx::new()),
                ))
            }
            LAX_ZPY => {
                // LAX Zero Page,Y: LAX $nn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageY::new()),
                    Box::new(Lax::new()),
                ))
            }
            CLV => {
                // CLV: Clear Overflow flag
                Some(Instruction::new(Box::new(Implied), Box::new(Clv::new())))
            }
            LDA_ABSY => {
                // LDA Absolute,Y: LDA $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Lda::new()),
                ))
            }
            TSX => {
                // TSX: Transfer SP to X
                Some(Instruction::new(Box::new(Implied), Box::new(Tsx::new())))
            }
            LAR_ABSY => {
                // LAR Absolute,Y: LAR $nnnn,Y (illegal opcode, also called LAS)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Lar::new()),
                ))
            }
            LDY_ABSX => {
                // LDY Absolute,X: LDY $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Ldy::new()),
                ))
            }
            LDA_ABSX => {
                // LDA Absolute,X: LDA $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Lda::new()),
                ))
            }
            LDX_ABSY => {
                // LDX Absolute,Y: LDX $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Ldx::new()),
                ))
            }
            LAX_ABSY => {
                // LAX Absolute,Y: LAX $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Lax::new()),
                ))
            }
            CPY_IMM => {
                // CPY Immediate: CPY #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Cpy::new()),
                ))
            }
            CMP_INDX => {
                // CMP Indexed Indirect: CMP ($nn,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Cmp::new()),
                ))
            }
            DOP_IMM4 => {
                // DOP Immediate (illegal NOP): NOP #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Dop::new()),
                ))
            }
            DCP_INDX => {
                // DCP Indexed Indirect: DCP ($nn,X) (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Dcp::new()),
                ))
            }
            CPY_ZP => {
                // CPY Zero Page: CPY $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Cpy::new()),
                ))
            }
            CMP_ZP => {
                // CMP Zero Page: CMP $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Cmp::new()),
                ))
            }
            DEC_ZP => {
                // DEC Zero Page: DEC $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dec::new()),
                ))
            }
            DCP_ZP => {
                // DCP Zero Page: DCP $nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dcp::new()),
                ))
            }
            INY => {
                // INY Implied: INY
                Some(Instruction::new(Box::new(Implied), Box::new(Iny::new())))
            }
            CMP_IMM => {
                // CMP Immediate: CMP #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Cmp::new()),
                ))
            }
            DEX => {
                // DEX Implied: DEX
                Some(Instruction::new(Box::new(Implied), Box::new(Dex::new())))
            }
            AXS_IMM => {
                // AXS Immediate: AXS #$nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Axs::new()),
                ))
            }
            CPY_ABS => {
                // CPY Absolute: CPY $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Cpy::new()),
                ))
            }
            CMP_ABS => {
                // CMP Absolute: CMP $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Cmp::new()),
                ))
            }
            DEC_ABS => {
                // DEC Absolute: DEC $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Dec::new()),
                ))
            }
            DCP_ABS => {
                // DCP Absolute: DCP $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Dcp::new()),
                ))
            }
            BNE => {
                // BNE Relative: BNE $nn
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bne::new()),
                ))
            }
            CMP_INDY => {
                // CMP Indirect,Y: CMP ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Cmp::new()),
                ))
            }
            KIL11 => {
                // KIL (illegal opcode that halts CPU)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            DCP_INDY => {
                // DCP Indirect,Y: DCP ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Dcp::new()),
                ))
            }
            DOP_ZPX5 => {
                // DOP Zero Page,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            CMP_ZPX => {
                // CMP Zero Page,X: CMP $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Cmp::new()),
                ))
            }
            DEC_ZPX => {
                // DEC Zero Page,X: DEC $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dec::new()),
                ))
            }
            DCP_ZPX => {
                // DCP Zero Page,X: DCP $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dcp::new()),
                ))
            }
            CLD => {
                // CLD Implied: CLD
                Some(Instruction::new(Box::new(Implied), Box::new(Cld::new())))
            }
            CMP_ABSY => {
                // CMP Absolute,Y: CMP $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Cmp::new()),
                ))
            }
            NOP_IMP5 => {
                // NOP Implied (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            DCP_ABSY => {
                // DCP Absolute,Y: DCP $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Dcp::new()),
                ))
            }
            TOP_ABSX5 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            CMP_ABSX => {
                // CMP Absolute,X: CMP $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Cmp::new()),
                ))
            }
            DEC_ABSX => {
                // DEC Absolute,X: DEC $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Dec::new()),
                ))
            }
            DCP_ABSX => {
                // DCP Absolute,X: DCP $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Dcp::new()),
                ))
            }
            CPX_IMM => {
                // CPX Immediate: CPX #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Cpx::new()),
                ))
            }
            SBC_INDX => {
                // SBC Indexed Indirect: SBC ($nn,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Sbc::new()),
                ))
            }
            DOP_IMM5 => {
                // DOP Immediate (illegal opcode)
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Dop::new()),
                ))
            }
            ISB_INDX => {
                // ISB Indexed Indirect: ISB ($nn,X) (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new(MemoryAccess::Read)),
                    Box::new(Isb::new()),
                ))
            }
            CPX_ZP => {
                // CPX Zero Page: CPX $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Cpx::new()),
                ))
            }
            SBC_ZP => {
                // SBC Zero Page: SBC $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Sbc::new()),
                ))
            }
            INC_ZP => {
                // INC Zero Page: INC $nn
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Inc::new()),
                ))
            }
            ISB_ZP => {
                // ISB Zero Page: ISB $nn (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Isb::new()),
                ))
            }
            INX => {
                // INX Implied: INX
                Some(Instruction::new(Box::new(Implied), Box::new(Inx::new())))
            }
            SBC_IMM => {
                // SBC Immediate: SBC #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Sbc::new()),
                ))
            }
            NOP => {
                // NOP Implied: NOP
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            SBC_IMM2 => {
                // SBC Immediate (illegal duplicate): SBC #$nn
                Some(Instruction::new(
                    Box::new(Immediate::new()),
                    Box::new(Sbc::new()),
                ))
            }
            CPX_ABS => {
                // CPX Absolute: CPX $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Cpx::new()),
                ))
            }
            SBC_ABS => {
                // SBC Absolute: SBC $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Sbc::new()),
                ))
            }
            INC_ABS => {
                // INC Absolute: INC $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Inc::new()),
                ))
            }
            ISB_ABS => {
                // ISB Absolute: ISB $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(MemoryAccess::Read)),
                    Box::new(Isb::new()),
                ))
            }
            BEQ => {
                // BEQ Relative: BEQ $nn (branch if zero flag set)
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Beq::new()),
                ))
            }
            SBC_INDY => {
                // SBC Indirect,Y: SBC ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Read)),
                    Box::new(Sbc::new()),
                ))
            }
            KIL12 => {
                // KIL (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            ISB_INDY => {
                // ISB Indirect,Y: ISB ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Isb::new()),
                ))
            }
            DOP_ZPX6 => {
                // DOP Zero Page,X: DOP $nn,X (illegal NOP)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Dop::new()),
                ))
            }
            SBC_ZPX => {
                // SBC Zero Page,X: SBC $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Sbc::new()),
                ))
            }
            INC_ZPX => {
                // INC Zero Page,X: INC $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Inc::new()),
                ))
            }
            ISB_ZPX => {
                // ISB Zero Page,X: ISB $nn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Isb::new()),
                ))
            }
            SED => {
                // SED Implied: SED
                Some(Instruction::new(Box::new(Implied), Box::new(Sed::new())))
            }
            SBC_ABSY => {
                // SBC Absolute,Y: SBC $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Sbc::new()),
                ))
            }
            NOP_IMP6 => {
                // NOP Implied: NOP (illegal NOP variant)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            ISB_ABSY => {
                // ISB Absolute,Y: ISB $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Isb::new()),
                ))
            }
            TOP_ABSX6 => {
                // TOP Absolute,X: TOP $nnnn,X (illegal NOP)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            SBC_ABSX => {
                // SBC Absolute,X: SBC $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Sbc::new()),
                ))
            }
            INC_ABSX => {
                // INC Absolute,X: INC $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Inc::new()),
                ))
            }
            ISB_ABSX => {
                // ISB Absolute,X: ISB $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Isb::new()),
                ))
            }
            EOR_ABSY => {
                // EOR Absolute,Y: EOR $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Read)),
                    Box::new(Eor::new()),
                ))
            }
            NOP_IMP3 => {
                // NOP Implied: NOP (illegal opcode variant)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            SRE_ABSY => {
                // SRE Absolute,Y: SRE $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Sre::new()),
                ))
            }
            TOP_ABSX3 => {
                // TOP Absolute,X: TOP $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Top::new()),
                ))
            }
            EOR_ABSX => {
                // EOR Absolute,X: EOR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Read)),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ABSX => {
                // LSR Absolute,X: LSR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ABSX => {
                // SRE Absolute,X: SRE $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::ReadModifyWrite)),
                    Box::new(Sre::new()),
                ))
            }
            BCC => {
                // BCC Relative: BCC $nn (branch if carry clear)
                Some(Instruction::new(
                    Box::new(Relative::new()),
                    Box::new(Bcc::new()),
                ))
            }
            STA_INDY => {
                // STA (Indirect),Y: STA ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Write)),
                    Box::new(Sta::new()),
                ))
            }
            KIL9 => {
                // KIL Implied: KIL (illegal opcode that halts CPU)
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            AXA_INDY => {
                // AXA (Indirect),Y: AXA ($nn),Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(MemoryAccess::Write)),
                    Box::new(Axa::new()),
                ))
            }
            STY_ZPX => {
                // STY Zero Page,X: STY $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Sty::new()),
                ))
            }
            STA_ZPX => {
                // STA Zero Page,X: STA $nn,X
                Some(Instruction::new(
                    Box::new(ZeroPageX::new()),
                    Box::new(Sta::new()),
                ))
            }
            STX_ZPY => {
                // STX Zero Page,Y: STX $nn,Y
                Some(Instruction::new(
                    Box::new(ZeroPageY::new()),
                    Box::new(Stx::new()),
                ))
            }
            SAX_ZPY => {
                // SAX Zero Page,Y: SAX $nn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPageY::new()),
                    Box::new(Sax::new()),
                ))
            }
            TYA => {
                // TYA Implied: TYA
                Some(Instruction::new(Box::new(Implied), Box::new(Tya::new())))
            }
            STA_ABSY => {
                // STA Absolute,Y: STA $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Write)),
                    Box::new(Sta::new()),
                ))
            }
            TXS => {
                // TXS Implied: TXS
                Some(Instruction::new(Box::new(Implied), Box::new(Txs::new())))
            }
            XAS_ABSY => {
                // XAS Absolute,Y: XAS $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Write)),
                    Box::new(Xas::new()),
                ))
            }
            SYA_ABSX => {
                // SYA Absolute,X: SYA $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Write)),
                    Box::new(Sya::new()),
                ))
            }
            STA_ABSX => {
                // STA Absolute,X: STA $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(MemoryAccess::Write)),
                    Box::new(Sta::new()),
                ))
            }
            SXA_ABSY => {
                // SXA Absolute,Y: SXA $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Write)),
                    Box::new(Sxa::new()),
                ))
            }
            AXA_ABSY => {
                // AXA Absolute,Y: AXA $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(MemoryAccess::Write)),
                    Box::new(Axa::new()),
                ))
            }
            _ => None, // Unimplemented opcode
        }
    }

    /// Check if the CPU is halted
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Get number of total cycles executed
    pub fn get_total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Reset the CPU
    ///
    /// According to NES hardware behavior, reset goes through the same 7-cycle
    /// sequence as NMI/IRQ interrupts, but suppresses writes to the stack.
    ///
    /// Cycle-by-cycle breakdown:
    /// 1. Fetch opcode (forced to $00, discarded)
    /// 2. Read next instruction byte (discarded, PC increment suppressed)
    /// 3. Dummy read from stack at $0100+SP, decrement SP
    /// 4. Dummy read from stack at $0100+SP, decrement SP
    /// 5. Dummy read from stack at $0100+SP, decrement SP
    /// 6. Read PCL from reset vector ($FFFC), set I flag
    /// 7. Read PCH from reset vector ($FFFD)
    ///
    /// Register behavior:
    /// - A, X, Y registers are UNCHANGED (preserved from before reset)
    /// - Status flags C, Z, D, V, N are UNCHANGED (preserved from before reset)
    /// - I flag is SET (interrupts disabled)
    /// - SP is decremented by 3 (via 3 dummy stack reads, not writes)
    /// - PC is loaded from reset vector at $FFFC-$FFFD
    /// - Takes 7 CPU cycles total
    ///
    /// References:
    /// - https://www.nesdev.org/wiki/CPU_power_up_state
    /// - https://www.nesdev.org/wiki/CPU_interrupts#IRQ_and_NMI_tick-by-tick_execution
    pub fn reset(&mut self) {
        // Cycles 1-2: Opcode fetch and read (both discarded)
        // These happen automatically before reset is called in hardware
        // We don't simulate them explicitly here since reset() is called directly

        // Cycles 3-5: Perform 3 dummy stack reads (writes are suppressed)
        // Each read decrements SP to simulate the stack push sequence
        for _ in 0..3 {
            let stack_addr = STACK_BASE | (self.state.sp as u16);
            let _ = self.memory.borrow().read(stack_addr); // Dummy read
            self.state.sp = self.state.sp.wrapping_sub(1);
        }

        // Cycle 6: Read PCL from reset vector and set I flag
        let pcl = self.memory.borrow().read(RESET_VECTOR);
        self.state.p |= FLAG_INTERRUPT;

        // Cycle 7: Read PCH from reset vector
        let pch = self.memory.borrow().read(RESET_VECTOR + 1);
        self.state.pc = ((pch as u16) << 8) | (pcl as u16);

        // Clear cycle-accurate instruction state
        self.halted = false;
        self.current_instruction = None;
        self.nmi_pending = false;
        self.irq_pending = false;
        self.in_interrupt_sequence = false;

        // Reset takes 7 cycles
        self.total_cycles = 7;
    }

    /// Trigger an NMI (Non-Maskable Interrupt)
    /// Returns the number of cycles consumed (7 cycles)
    pub fn trigger_nmi(&mut self) -> u8 {
        // Push PC and P onto stack
        self.push_word(self.state.pc);
        let mut p_with_break = self.state.p & !FLAG_BREAK; // Clear Break flag
        p_with_break |= FLAG_UNUSED; // Set unused flag
        self.push_byte(p_with_break);

        // Set PC to NMI vector
        self.state.pc = self.memory.borrow().read_u16(NMI_VECTOR);

        // Set Interrupt Disable flag
        self.state.p |= FLAG_INTERRUPT;

        // Clear NMI pending flag (NMI has been serviced)
        self.nmi_pending = false;

        // Mark that we're now in interrupt sequence
        // This prevents interrupt polling until at least one instruction executes
        self.in_interrupt_sequence = true;

        // NMI takes 7 CPU cycles
        self.total_cycles += 7;
        7
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&self) -> u16 {
        self.memory.borrow().read_u16(RESET_VECTOR)
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = STACK_BASE | (self.state.sp as u16);
        self.memory.borrow_mut().write(addr, value, false);
        self.state.sp = self.state.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte first
        self.push_byte(value as u8); // Low byte second
    }

    /// Add cycles to the total cycle count
    pub fn add_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;
    }

    /// Get the current CPU state
    pub fn get_state(&mut self) -> &mut CpuState {
        &mut self.state
    }

    /// Set the current CPU state
    pub fn set_state(&mut self, state: CpuState) {
        self.state = state;
    }

    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    #[cfg(test)]
    pub fn set_total_cycles(&mut self, cycles: u64) {
        self.total_cycles = cycles;
    }
    /// Set the NMI pending flag
    /// This should be called by the NES loop when NMI is detected
    pub fn set_nmi_pending(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Check if an NMI is pending
    pub fn is_nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    /// Set the IRQ pending flag
    /// This should be called by external hardware (NES loop) when IRQ line is asserted
    pub fn set_irq_pending(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    /// Check if an IRQ is pending
    pub fn is_irq_pending(&self) -> bool {
        self.irq_pending
    }

    /// Check if IRQ should be serviced
    /// IRQ is serviced only if:
    /// 1. IRQ is pending (irq_pending flag set)
    /// 2. Interrupt disable flag (I) is clear
    /// 3. During delay period after CLI/SEI/PLP, use the SAVED I flag value (before instruction changed it)
    pub fn should_poll_irq(&self) -> bool {
        // During delay period, use the saved I flag value (from before CLI/SEI/PLP changed it)
        // This allows "CLI SEI" to fire one IRQ using the I=0 value from before SEI set I=1
        let i_flag = if self.delay_interrupt_check {
            self.saved_i_flag_for_delay
        } else {
            (self.state.p & FLAG_INTERRUPT) != 0
        };
        
        self.irq_pending && !i_flag
    }

    /// Trigger an IRQ (Interrupt Request)
    ///
    /// IRQ follows the same 7-cycle sequence as NMI and BRK, but:
    /// - Can be masked by the I flag (unlike NMI which is non-maskable)
    /// - Uses the IRQ vector at $FFFE-$FFFF (same as BRK)
    /// - Pushes status with B flag clear (distinguishes from BRK)
    ///
    /// Cycle-by-cycle breakdown:
    /// 1. Fetch opcode (forced to $00, discarded)
    /// 2. Read next instruction byte (discarded)
    /// 3. Push PCH to stack, decrement SP
    /// 4. Push PCL to stack, decrement SP
    /// 5. Push P to stack (B flag clear, unused flag set), decrement SP
    /// 6. Read PCL from IRQ vector ($FFFE), set I flag
    /// 7. Read PCH from IRQ vector ($FFFF)
    ///
    /// Returns the number of cycles consumed (7 cycles)
    pub fn trigger_irq(&mut self) -> u8 {
        // Push PC to stack (high byte first, then low byte)
        self.push_word(self.state.pc);

        // Push status register to stack with B flag clear, unused flag set
        let mut p_with_flags = self.state.p & !FLAG_BREAK; // Clear B flag (distinguishes IRQ from BRK)
        p_with_flags |= FLAG_UNUSED; // Set unused flag (always set when pushed)
        self.push_byte(p_with_flags);

        // Read PC from IRQ vector at $FFFE-$FFFF
        let pcl = self.memory.borrow().read(IRQ_VECTOR);
        let pch = self.memory.borrow().read(IRQ_VECTOR + 1);
        self.state.pc = ((pch as u16) << 8) | (pcl as u16);

        // Set Interrupt Disable flag to prevent nested IRQs
        self.state.p |= FLAG_INTERRUPT;

        // Clear IRQ pending flag (IRQ has been serviced)
        self.irq_pending = false;

        // Mark that we're now in interrupt sequence
        // This prevents interrupt polling until at least one instruction executes
        self.in_interrupt_sequence = true;

        // IRQ takes 7 CPU cycles
        self.total_cycles += 7;
        7
    }

    /// Set the interrupt check delay flag
    /// This should be called by CLI, SEI, and PLP instructions to implement
    /// the hardware behavior where exactly one instruction executes before IRQ is checked
    pub fn set_interrupt_check_delay(&mut self) {
        self.delay_interrupt_check = true;
    }

    /// Poll for pending interrupts and return which one should be serviced (if any)
    ///
    /// According to NESdev Wiki:
    /// - Interrupt polling happens during the final cycle of most instructions
    /// - NMI has priority over IRQ when both are pending
    /// - Interrupt sequences themselves do not poll for interrupts
    ///   (at least one instruction from interrupt handler executes before next interrupt)
    ///
    /// Returns:
    /// - Some(true) if NMI should be serviced
    /// - Some(false) if IRQ should be serviced  
    /// - None if no interrupt should be serviced
    pub fn poll_pending_interrupt(&self) -> Option<bool> {
        // Don't poll during interrupt sequences
        if self.in_interrupt_sequence {
            return None;
        }

        // NMI has priority over IRQ
        if self.nmi_pending {
            return Some(true); // true = NMI
        }

        // Check IRQ only if I flag is clear
        if self.should_poll_irq() {
            return Some(false); // false = IRQ
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;
    use crate::nes::TvSystem;
    use crate::ppu::Ppu;

    // Helper function to create a test memory controller
    fn create_test_memory() -> Rc<RefCell<MemController>> {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        Rc::new(RefCell::new(MemController::new(ppu, apu)))
    }

    // Helper function to execute CPU until instruction completes
    fn execute_instruction(cpu: &mut Cpu2) -> u64 {
        let start_cycles = cpu.total_cycles();
        let mut instruction_complete = false;
        let mut safety = 0;
        while !instruction_complete && safety < 100 {
            instruction_complete = cpu.tick_cycle();
            safety += 1;
        }
        assert!(instruction_complete, "Instruction did not complete");
        cpu.total_cycles() - start_cycles
    }

    #[test]
    fn test_opcode_00() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Create a 32KB PRG ROM cartridge with IRQ vector at $FFFE-$FFFF
        let mut prg_rom = vec![0; 0x8000]; // 32KB

        // Set up BRK instruction at address $8400 (mapped to $0400 in ROM)
        prg_rom[0x0400] = BRK; // BRK opcode
        prg_rom[0x0401] = 0x00; // Padding byte

        // Set up IRQ vector at $FFFE-$FFFF (end of ROM) to point to $8000
        prg_rom[0x7FFE] = 0x00; // Low byte of IRQ handler ($8000)
        prg_rom[0x7FFF] = 0x80; // High byte of IRQ handler ($8000)

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x8400; // Start at ROM address (not $0400)
        cpu.state.sp = 0xFD;
        cpu.state.p = 0b0010_0000; // Only unused flag set
        cpu.state.a = 0x42; // Some value to verify registers aren't affected

        let cycles = execute_instruction(&mut cpu);

        // PC should now point to the IRQ handler at $8000
        assert_eq!(cpu.state.pc, 0x8000, "PC should be loaded from IRQ vector");

        // Stack should have three values pushed:
        // SP was 0xFD, after pushing 3 bytes it should be 0xFA
        assert_eq!(
            cpu.state.sp, 0xFA,
            "Stack pointer should have decremented by 3"
        );

        // Check return address on stack (PC+2 = $8402)
        let pch = memory.borrow().read(0x01FD); // High byte at original SP
        let pcl = memory.borrow().read(0x01FC); // Low byte at SP-1
        let return_address = ((pch as u16) << 8) | (pcl as u16);
        assert_eq!(return_address, 0x8402, "Return address should be PC+2");

        // Check status register on stack (should have B flag set)
        let status_on_stack = memory.borrow().read(0x01FB); // Status at SP-2
        assert_eq!(
            status_on_stack & FLAG_BREAK,
            FLAG_BREAK,
            "B flag should be set in pushed status"
        );
        assert_eq!(
            status_on_stack & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag should be set in pushed status"
        );

        // I flag should be set in CPU
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set after BRK"
        );

        // A register should be unchanged
        assert_eq!(cpu.state.a, 0x42, "A register should not be affected");

        // BRK takes 7 cycles
        assert_eq!(cycles, 7, "BRK should take 7 cycles");
    }

    #[test]
    fn test_reset_preserves_registers() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Create a simple ROM with reset vector pointing to $8000
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0x7FFC] = 0x00; // Low byte of reset vector ($8000)
        prg_rom[0x7FFD] = 0x80; // High byte of reset vector ($8000)

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set registers to known values before reset
        cpu.state.a = 0x42;
        cpu.state.x = 0x33;
        cpu.state.y = 0x87;
        cpu.state.sp = 0xFD;
        cpu.state.p = FLAG_CARRY | FLAG_ZERO | FLAG_DECIMAL | FLAG_OVERFLOW | FLAG_NEGATIVE;
        cpu.state.pc = 0x1234;

        // Call reset
        cpu.reset();

        // According to https://www.nesdev.org/wiki/CPU_power_up_state:
        // - A, X, Y are unchanged
        assert_eq!(cpu.state.a, 0x42, "A should be preserved after reset");
        assert_eq!(cpu.state.x, 0x33, "X should be preserved after reset");
        assert_eq!(cpu.state.y, 0x87, "Y should be preserved after reset");

        // - Status flags C, Z, D, V, N are unchanged
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            FLAG_CARRY,
            "C flag should be preserved"
        );
        assert_eq!(
            cpu.state.p & FLAG_ZERO,
            FLAG_ZERO,
            "Z flag should be preserved"
        );
        assert_eq!(
            cpu.state.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should be preserved"
        );
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "V flag should be preserved"
        );
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be preserved"
        );

        // - I flag is set
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set after reset"
        );

        // - SP is decremented by 3
        assert_eq!(
            cpu.state.sp, 0xFA,
            "SP should be decremented by 3 (0xFD - 3 = 0xFA)"
        );

        // - PC is loaded from reset vector
        assert_eq!(
            cpu.state.pc, 0x8000,
            "PC should be loaded from reset vector"
        );

        // - Reset takes 7 cycles
        assert_eq!(cpu.total_cycles, 7, "Reset should take 7 cycles");
    }

    #[test]
    fn test_power_on_state() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Create a simple ROM with reset vector
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let cpu = Cpu2::new(Rc::clone(&memory));

        // According to https://www.nesdev.org/wiki/CPU_power_up_state
        // At power-on (before reset sequence):
        assert_eq!(cpu.state.a, 0, "A should be 0 at power-on");
        assert_eq!(cpu.state.x, 0, "X should be 0 at power-on");
        assert_eq!(cpu.state.y, 0, "Y should be 0 at power-on");
        assert_eq!(cpu.state.sp, 0x00, "SP should be 0x00 at power-on");
        assert_eq!(cpu.state.pc, 0, "PC should be 0 at power-on");
        assert_eq!(
            cpu.state.p, FLAG_UNUSED,
            "P should only have unused bit set at power-on"
        );
        assert_eq!(cpu.total_cycles, 0, "Cycle count should be 0 at power-on");
        assert!(!cpu.halted, "CPU should not be halted at power-on");
        assert!(!cpu.nmi_pending, "NMI should not be pending at power-on");
    }

    #[test]
    fn test_reset_after_power_on() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Create a ROM with reset vector pointing to $C000
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00; // Low byte
        prg_rom[0x7FFD] = 0xC0; // High byte ($C000)

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Verify initial power-on state
        assert_eq!(cpu.state.sp, 0x00, "SP should be 0x00 at power-on");
        assert_eq!(cpu.state.p, FLAG_UNUSED, "Only unused bit should be set");

        // Call reset (simulates what happens after power-on)
        cpu.reset();

        // After reset following power-on:
        // - A, X, Y remain 0 (they were 0 at power-on)
        assert_eq!(cpu.state.a, 0, "A should still be 0");
        assert_eq!(cpu.state.x, 0, "X should still be 0");
        assert_eq!(cpu.state.y, 0, "Y should still be 0");

        // - SP should be decremented by 3: 0x00 - 3 = 0xFD (with wrapping)
        assert_eq!(
            cpu.state.sp, 0xFD,
            "SP should wrap to 0xFD (0x00 - 3 with wrapping)"
        );

        // - I flag should be set (in addition to unused bit)
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set"
        );
        assert_eq!(
            cpu.state.p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused bit should remain set"
        );

        // - PC should be loaded from reset vector ($C000)
        assert_eq!(cpu.state.pc, 0xC000, "PC should be $C000 from reset vector");

        // - Reset should take 7 cycles
        assert_eq!(cpu.total_cycles, 7, "Reset should take 7 cycles");
    }

    #[test]
    fn test_reset_with_sp_wrapping() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Test SP wrapping: if SP is 0x01, after subtracting 3 it should wrap to 0xFE
        cpu.state.sp = 0x01;
        cpu.reset();
        assert_eq!(cpu.state.sp, 0xFE, "SP should wrap: 0x01 - 3 = 0xFE");

        // Test SP wrapping: if SP is 0x00, it should wrap to 0xFD
        cpu.state.sp = 0x00;
        cpu.reset();
        assert_eq!(cpu.state.sp, 0xFD, "SP should wrap: 0x00 - 3 = 0xFD");

        // Test SP wrapping: if SP is 0x02, it should become 0xFF
        cpu.state.sp = 0x02;
        cpu.reset();
        assert_eq!(cpu.state.sp, 0xFF, "SP should wrap: 0x02 - 3 = 0xFF");
    }

    #[test]
    fn test_multiple_resets() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x34;
        prg_rom[0x7FFD] = 0x12;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set some register state
        cpu.state.a = 0xAA;
        cpu.state.x = 0xBB;
        cpu.state.y = 0xCC;
        cpu.state.sp = 0xFD;
        cpu.state.p = FLAG_CARRY | FLAG_ZERO;

        // First reset
        cpu.reset();
        assert_eq!(cpu.state.a, 0xAA, "A preserved after first reset");
        assert_eq!(cpu.state.sp, 0xFA, "SP = 0xFD - 3 = 0xFA");
        assert_eq!(cpu.state.pc, 0x1234, "PC loaded from vector");

        // Second reset - registers should still be preserved
        cpu.reset();
        assert_eq!(cpu.state.a, 0xAA, "A still preserved after second reset");
        assert_eq!(cpu.state.x, 0xBB, "X still preserved after second reset");
        assert_eq!(cpu.state.y, 0xCC, "Y still preserved after second reset");
        assert_eq!(cpu.state.sp, 0xF7, "SP = 0xFA - 3 = 0xF7");
        assert_eq!(cpu.state.pc, 0x1234, "PC loaded from vector again");

        // Flags should still be preserved (except I which is always set)
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "Carry preserved");
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Zero preserved");
        assert_eq!(cpu.state.p & FLAG_INTERRUPT, FLAG_INTERRUPT, "I flag set");
    }

    #[test]
    fn test_reset_performs_dummy_stack_reads() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00; // Reset vector low byte
        prg_rom[0x7FFD] = 0x80; // Reset vector high byte

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set up known values in stack memory to verify reads happen
        cpu.state.sp = 0xFD;
        memory.borrow_mut().write(0x01FD, 0xAA, false); // Will be read during cycle 3
        memory.borrow_mut().write(0x01FC, 0xBB, false); // Will be read during cycle 4
        memory.borrow_mut().write(0x01FB, 0xCC, false); // Will be read during cycle 5

        // These values should NOT be affected by reset (reads only, no writes)
        cpu.reset();

        // Verify the stack memory was read but NOT modified
        // (Reset performs dummy reads, not writes)
        assert_eq!(
            memory.borrow().read(0x01FD),
            0xAA,
            "Stack at 0x01FD should be unchanged (read, not written)"
        );
        assert_eq!(
            memory.borrow().read(0x01FC),
            0xBB,
            "Stack at 0x01FC should be unchanged (read, not written)"
        );
        assert_eq!(
            memory.borrow().read(0x01FB),
            0xCC,
            "Stack at 0x01FB should be unchanged (read, not written)"
        );

        // SP should be decremented by 3
        assert_eq!(cpu.state.sp, 0xFA, "SP should be 0xFD - 3 = 0xFA");

        // PC should be loaded from reset vector
        assert_eq!(
            cpu.state.pc, 0x8000,
            "PC should be loaded from reset vector"
        );
    }

    #[test]
    fn test_irq_trigger_basic() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Set up IRQ vector at $FFFE-$FFFF to point to $9000
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00; // Reset vector low ($8000)
        prg_rom[0x7FFD] = 0x80; // Reset vector high
        prg_rom[0x7FFE] = 0x00; // IRQ vector low ($9000)
        prg_rom[0x7FFF] = 0x90; // IRQ vector high

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set up CPU state before IRQ
        cpu.state.pc = 0x1234;
        cpu.state.sp = 0xFD;
        cpu.state.p = FLAG_ZERO | FLAG_CARRY; // I flag is clear, so IRQ can be triggered
        cpu.irq_pending = true;

        let initial_cycles = cpu.total_cycles;

        // Trigger IRQ
        let cycles = cpu.trigger_irq();

        // Verify IRQ took 7 cycles
        assert_eq!(cycles, 7, "IRQ should take 7 cycles");
        assert_eq!(
            cpu.total_cycles,
            initial_cycles + 7,
            "Total cycles should increase by 7"
        );

        // Verify PC was loaded from IRQ vector
        assert_eq!(cpu.state.pc, 0x9000, "PC should be loaded from IRQ vector");

        // Verify I flag was set
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set after IRQ"
        );

        // Verify IRQ pending flag was cleared
        assert!(!cpu.irq_pending, "IRQ pending flag should be cleared");

        // Verify stack pointer was decremented by 3
        assert_eq!(cpu.state.sp, 0xFA, "SP should be decremented by 3");

        // Verify stack contents: PCH, PCL, P (with B flag clear)
        assert_eq!(
            memory.borrow().read(0x01FD),
            0x12,
            "PCH should be pushed to stack"
        );
        assert_eq!(
            memory.borrow().read(0x01FC),
            0x34,
            "PCL should be pushed to stack"
        );

        // Status should have B flag clear (0) and unused flag set (1)
        let pushed_p = memory.borrow().read(0x01FB);
        assert_eq!(
            pushed_p & FLAG_BREAK,
            0,
            "B flag should be clear in pushed status"
        );
        assert_eq!(
            pushed_p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag should be set in pushed status"
        );
        assert_eq!(
            pushed_p & FLAG_ZERO,
            FLAG_ZERO,
            "Z flag should be preserved in pushed status"
        );
        assert_eq!(
            pushed_p & FLAG_CARRY,
            FLAG_CARRY,
            "C flag should be preserved in pushed status"
        );
    }

    #[test]
    fn test_irq_respects_i_flag() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFE] = 0x00; // IRQ vector
        prg_rom[0x7FFF] = 0x90;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set I flag - IRQ should NOT be polled
        cpu.state.p = FLAG_INTERRUPT;
        cpu.irq_pending = true;

        // should_poll_irq() should return false when I flag is set
        assert!(
            !cpu.should_poll_irq(),
            "IRQ should NOT be polled when I flag is set"
        );

        // Clear I flag - IRQ should now be polled
        cpu.state.p = 0;
        assert!(
            cpu.should_poll_irq(),
            "IRQ should be polled when I flag is clear and irq_pending is true"
        );

        // Clear irq_pending - IRQ should not be polled even with I flag clear
        cpu.irq_pending = false;
        assert!(
            !cpu.should_poll_irq(),
            "IRQ should NOT be polled when irq_pending is false"
        );
    }

    #[test]
    fn test_irq_clears_b_flag() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x90;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        cpu.state.pc = 0x1000;
        cpu.state.sp = 0xFD;
        cpu.state.p = FLAG_BREAK; // Set B flag explicitly
        cpu.irq_pending = true;

        cpu.trigger_irq();

        // Check pushed status has B flag clear
        let pushed_p = memory.borrow().read(0x01FB);
        assert_eq!(
            pushed_p & FLAG_BREAK,
            0,
            "B flag must be clear in pushed status during IRQ"
        );
        assert_eq!(
            pushed_p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag must be set in pushed status"
        );
    }

    #[test]
    fn test_irq_set_and_check() {
        let memory = create_test_memory();
        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Initially no IRQ
        assert!(!cpu.is_irq_pending(), "IRQ should not be pending initially");

        // Set IRQ
        cpu.set_irq_pending(true);
        assert!(cpu.is_irq_pending(), "IRQ should be pending after set");

        // Clear IRQ
        cpu.set_irq_pending(false);
        assert!(
            !cpu.is_irq_pending(),
            "IRQ should not be pending after clear"
        );
    }

    #[test]
    fn test_irq_stack_wrapping() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x90;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        // Set SP to a low value that will wrap during push operations
        cpu.state.pc = 0xABCD;
        cpu.state.sp = 0x01; // Stack will wrap: 0x01 -> 0x00 -> 0xFF
        cpu.state.p = 0;
        cpu.irq_pending = true;

        cpu.trigger_irq();

        // Verify stack pointer wrapped correctly
        assert_eq!(
            cpu.state.sp, 0xFE,
            "SP should wrap correctly from 0x01 to 0xFE"
        );

        // Verify data was pushed to correct wrapped addresses
        assert_eq!(memory.borrow().read(0x0101), 0xAB, "PCH pushed to 0x0101");
        assert_eq!(memory.borrow().read(0x0100), 0xCD, "PCL pushed to 0x0100");
        assert_eq!(
            memory.borrow().read(0x01FF) & FLAG_UNUSED,
            FLAG_UNUSED,
            "Status pushed to 0x01FF (wrapped)"
        );
    }

    #[test]
    fn test_interrupt_polling_nmi_priority() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Set up interrupt vectors
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00; // Reset vector ($8000)
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFA] = 0x00; // NMI vector ($9000)
        prg_rom[0x7FFB] = 0x90;
        prg_rom[0x7FFE] = 0x00; // IRQ vector ($A000)
        prg_rom[0x7FFF] = 0xA0;

        // Place a NOP instruction at $8000
        prg_rom[0x0000] = 0xEA; // NOP

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x8000;
        cpu.state.sp = 0xFD;
        cpu.state.p = 0; // I flag clear, so IRQ can be serviced

        // Set both NMI and IRQ pending before executing instruction
        cpu.nmi_pending = true;
        cpu.irq_pending = true;

        // Execute NOP instruction cycle by cycle
        let mut cycles = 0;
        loop {
            let done = cpu.tick_cycle();
            cycles += 1;
            if done {
                break;
            }
        }

        assert_eq!(cycles, 2, "NOP should take 2 cycles");
        assert_eq!(cpu.state.pc, 0x8001, "PC should advance past NOP");

        // After instruction completes, check interrupt polling
        // NMI should have priority over IRQ
        let pending_interrupt = cpu.poll_pending_interrupt();
        assert_eq!(
            pending_interrupt,
            Some(true),
            "NMI (true) should be returned when both NMI and IRQ are pending"
        );

        // Test IRQ alone
        cpu.nmi_pending = false;
        cpu.irq_pending = true;
        let pending_interrupt = cpu.poll_pending_interrupt();
        assert_eq!(
            pending_interrupt,
            Some(false),
            "IRQ (false) should be returned when only IRQ is pending"
        );

        // Test no interrupt
        cpu.irq_pending = false;
        let pending_interrupt = cpu.poll_pending_interrupt();
        assert_eq!(
            pending_interrupt, None,
            "None should be returned when no interrupts are pending"
        );
    }

    #[test]
    fn test_interrupt_not_polled_during_interrupt_sequence() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFC] = 0x00; // Reset vector
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFA] = 0x00; // NMI vector ($9000)
        prg_rom[0x7FFB] = 0x90;

        // Place NOP at NMI handler
        prg_rom[0x1000] = 0xEA; // NOP at $9000

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x1234;
        cpu.state.sp = 0xFD;
        cpu.state.p = 0;

        // Set NMI pending
        cpu.nmi_pending = true;

        // Trigger NMI
        cpu.trigger_nmi();

        // PC should now point to NMI handler
        assert_eq!(cpu.state.pc, 0x9000, "PC should be at NMI handler");
        assert!(!cpu.nmi_pending, "NMI pending should be cleared");
        assert!(
            cpu.in_interrupt_sequence,
            "Should be marked as in interrupt sequence"
        );

        // Set NMI pending again during the interrupt sequence
        cpu.nmi_pending = true;

        // poll_pending_interrupt() should return None during interrupt sequence
        let pending_interrupt = cpu.poll_pending_interrupt();
        assert_eq!(
            pending_interrupt, None,
            "No interrupt should be polled during interrupt sequence"
        );

        // Now execute one instruction (NOP) from the handler
        let mut cycles = 0;
        loop {
            let done = cpu.tick_cycle();
            cycles += 1;
            if done {
                break;
            }
        }

        assert_eq!(cycles, 2, "NOP should take 2 cycles");
        assert!(
            !cpu.in_interrupt_sequence,
            "Should no longer be in interrupt sequence after instruction completes"
        );

        // Now polling should work again
        let pending_interrupt = cpu.poll_pending_interrupt();
        assert_eq!(
            pending_interrupt,
            Some(true),
            "NMI should be polled after one instruction executes from handler"
        );
    }

    #[test]
    fn test_nmi_clears_b_flag() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x7FFA] = 0x00; // NMI vector ($9000)
        prg_rom[0x7FFB] = 0x90;

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));

        cpu.state.pc = 0x1000;
        cpu.state.sp = 0xFD;
        cpu.state.p = FLAG_BREAK | FLAG_ZERO | FLAG_CARRY; // Set B flag and some other flags
        cpu.nmi_pending = true;

        cpu.trigger_nmi();

        // Check pushed status has B flag clear but other flags preserved
        let pushed_p = memory.borrow().read(0x01FB);
        assert_eq!(
            pushed_p & FLAG_BREAK,
            0,
            "B flag must be clear in pushed status during NMI"
        );
        assert_eq!(
            pushed_p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag must be set in pushed status"
        );
        assert_eq!(
            pushed_p & FLAG_ZERO,
            FLAG_ZERO,
            "Zero flag should be preserved in pushed status"
        );
        assert_eq!(
            pushed_p & FLAG_CARRY,
            FLAG_CARRY,
            "Carry flag should be preserved in pushed status"
        );
    }

    #[test]
    fn test_rti_ignores_break_and_unused_bits() {
        let memory = create_test_memory();

        // Set up RTI instruction at address $0400
        memory.borrow_mut().write(0x0400, RTI, false);

        // Set up stack with status byte that has both B and unused bits set
        let status_on_stack = 0xFF; // All flags set including B=1 and unused=1
        memory.borrow_mut().write(0x01FD, status_on_stack, false); // Status
        memory.borrow_mut().write(0x01FE, 0x34, false); // PCL
        memory.borrow_mut().write(0x01FF, 0x12, false); // PCH

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFC; // Points below the three stack bytes
        cpu.state.p = 0x00; // Start with all flags clear

        // Execute RTI instruction
        let mut cycles = 0;
        loop {
            let done = cpu.tick_cycle();
            cycles += 1;
            if done {
                break;
            }
        }

        assert_eq!(cycles, 6, "RTI should take 6 cycles");
        assert_eq!(cpu.state.pc, 0x1234, "PC should be restored from stack");

        // Check that status was restored but with B=0 and unused=1
        // All other bits from stack (0xFF) should be preserved
        assert_eq!(
            cpu.state.p & FLAG_BREAK,
            0,
            "B flag (bit 4) must be clear after RTI, even if set on stack"
        );
        assert_eq!(
            cpu.state.p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag (bit 5) must be set after RTI"
        );
        // Check that other flags were restored correctly
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "Negative flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "Overflow flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "Interrupt flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "Decimal flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_ZERO,
            FLAG_ZERO,
            "Zero flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            FLAG_CARRY,
            "Carry flag should be restored"
        );
    }

    #[test]
    fn test_plp_ignores_break_and_unused_bits() {
        let memory = create_test_memory();

        // Set up PLP instruction at address $0400
        memory.borrow_mut().write(0x0400, PLP, false);

        // Set up stack with status byte that has both B and unused bits set
        let status_on_stack = 0xFF; // All flags set including B=1 and unused=1
        memory.borrow_mut().write(0x01FD, status_on_stack, false); // Status

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFC; // Points below the stack byte
        cpu.state.p = 0x00; // Start with all flags clear

        // Execute PLP instruction
        let mut cycles = 0;
        loop {
            let done = cpu.tick_cycle();
            cycles += 1;
            if done {
                break;
            }
        }

        assert_eq!(cycles, 4, "PLP should take 4 cycles");

        // Check that status was restored but with B=0 and unused=1
        // All other bits from stack (0xFF) should be preserved
        assert_eq!(
            cpu.state.p & FLAG_BREAK,
            0,
            "B flag (bit 4) must be clear after PLP, even if set on stack"
        );
        assert_eq!(
            cpu.state.p & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag (bit 5) must be set after PLP"
        );
        // Check that other flags were restored correctly
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "Negative flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "Overflow flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "Interrupt flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "Decimal flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_ZERO,
            FLAG_ZERO,
            "Zero flag should be restored"
        );
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            FLAG_CARRY,
            "Carry flag should be restored"
        );
    }

    #[test]
    fn test_opcode_01() {
        let memory = create_test_memory();

        // Set up ORA ($20,X) instruction at address $0400
        // With X=0x04, reads from zero page address ($20+$04) = $24
        // At $24-$25 we store the pointer $1234
        // At $1234 we store the value to ORA with
        memory.borrow_mut().write(0x0400, ORA_INDX, false); // ORA (zp,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at zero page $24 (base $20 + X register $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte of target address
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte of target address

        // Set up value to ORA at address $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0b1100_1100;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_1100 | 0b1010_1010 = 0b1110_1110
        assert_eq!(cpu.state.a, 0b1110_1110, "A should contain result of ORA");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cycles, 6, "ORA indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_02() {
        let memory = create_test_memory();

        // Set up KIL instruction at address $0400
        memory.borrow_mut().write(0x0400, KIL, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x42;
        cpu.state.x = 0x55;
        cpu.state.y = 0x66;
        cpu.state.p = 0x24;

        // KIL should halt the CPU - it never completes normally
        // After executing, the CPU should be halted
        let start_cycles = cpu.total_cycles();
        let mut ticks = 0;
        let max_ticks = 10;

        // Tick once to fetch and start the KIL instruction
        cpu.tick_cycle();
        ticks += 1;

        // The CPU should now be halted and subsequent ticks should not advance
        let pc_after_fetch = cpu.state.pc;

        while ticks < max_ticks {
            cpu.tick_cycle();
            ticks += 1;
        }

        // PC should not have advanced beyond the KIL instruction
        assert_eq!(
            cpu.state.pc, pc_after_fetch,
            "PC should not advance after KIL"
        );

        // Registers should remain unchanged
        assert_eq!(cpu.state.a, 0x42, "A should not change");
        assert_eq!(cpu.state.x, 0x55, "X should not change");
        assert_eq!(cpu.state.y, 0x66, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x24, "P should not change");

        // The CPU should be in a halted state
        assert!(cpu.is_halted(), "CPU should be halted after KIL");
    }

    #[test]
    fn test_opcode_03() {
        let memory = create_test_memory();

        // Set up SLO ($20,X) instruction at address $0400
        // With X=0x04, reads from zero page address ($20+$04) = $24
        // At $24-$25 we store the pointer $1234
        // At $1234 we store the value to shift and OR with
        memory.borrow_mut().write(0x0400, SLO_INDX, false); // SLO (zp,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at zero page $24 (base $20 + X register $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte of target address
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte of target address

        // Set up value at target address $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false); // Value to shift left

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0b1100_0011; // Accumulator value to OR with
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let memory_value = memory.borrow().read(0x1234);
        assert_eq!(
            memory_value, 0b1010_1010,
            "Memory should contain shifted value"
        );

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of OR");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO with indexed indirect should take 8 cycles
        // (5 for addressing + 3 for read-modify-write operation)
        assert_eq!(cycles, 8, "SLO indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_04() {
        let memory = create_test_memory();

        // Set up DOP $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, DOP_ZP, false); // DOP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up a value at zero page $20 (will be read but ignored)
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.x = 0x22;
        cpu.state.y = 0x33;
        cpu.state.p = 0x44;

        let cycles = execute_instruction(&mut cpu);

        // DOP reads from memory but does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x22, "X should not change");
        assert_eq!(cpu.state.y, 0x33, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x44, "P should not change");

        // Verify the memory value is still there (not modified)
        let memory_value = memory.borrow().read(0x0020);
        assert_eq!(memory_value, 0x42, "Memory should not be modified");

        // DOP with zero page should take 3 cycles
        // (1 opcode fetch + 1 ZP addressing + 1 read)
        assert_eq!(cycles, 3, "DOP zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_05() {
        let memory = create_test_memory();

        // Set up ORA $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, ORA_ZP, false); // ORA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at zero page $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA with zero page should take 3 cycles
        // (1 opcode fetch + 1 ZP addressing + 1 read/operate)
        assert_eq!(cycles, 3, "ORA zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_06() {
        let memory = create_test_memory();

        // Set up ASL $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, ASL_ZP, false); // ASL Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at zero page $20
        memory.borrow_mut().write(0x0020, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $20 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let memory_value = memory.borrow().read(0x0020);
        assert_eq!(
            memory_value, 0b1010_1010,
            "Memory should contain shifted value"
        );

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // ASL with zero page should take 5 cycles
        // (1 opcode fetch + 1 ZP addressing + 3 RMW operation)
        assert_eq!(cycles, 5, "ASL zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_07() {
        let memory = create_test_memory();

        // Set up SLO $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, SLO_ZP, false); // SLO Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at zero page $20
        memory.borrow_mut().write(0x0020, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011; // Accumulator value to OR with
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $20 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let memory_value = memory.borrow().read(0x0020);
        assert_eq!(
            memory_value, 0b1010_1010,
            "Memory should contain shifted value"
        );

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of OR");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO with zero page should take 5 cycles
        // (1 opcode fetch + 1 ZP addressing + 3 RMW operation)
        assert_eq!(cycles, 5, "SLO zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_08() {
        let memory = create_test_memory();

        // Set up PHP instruction at address $0400
        memory.borrow_mut().write(0x0400, PHP, false); // PHP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.p = 0b1010_0101; // Set some flags: N=1, V=0, unused=1, B=0, D=0, I=1, Z=0, C=1

        let cycles = execute_instruction(&mut cpu);

        // Stack pointer should have decremented by 1
        assert_eq!(cpu.state.sp, 0xFC, "SP should decrement by 1");

        // Check the value pushed to stack (should have B and unused flags set)
        let pushed_value = memory.borrow().read(0x01FD); // Read from original SP location
        assert_eq!(
            pushed_value & FLAG_BREAK,
            FLAG_BREAK,
            "B flag should be set in pushed value"
        );
        assert_eq!(
            pushed_value & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag should be set in pushed value"
        );
        assert_eq!(
            pushed_value & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should match original"
        );
        assert_eq!(
            pushed_value & FLAG_CARRY,
            FLAG_CARRY,
            "C flag should match original"
        );
        assert_eq!(
            pushed_value & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should match original"
        );

        // Processor status register should be unchanged
        assert_eq!(cpu.state.p, 0b1010_0101, "P should not change");

        // PHP should take 3 cycles
        // (1 opcode fetch + 2 execution cycles)
        assert_eq!(cycles, 3, "PHP should take 3 cycles");
    }

    #[test]
    fn test_opcode_09() {
        let memory = create_test_memory();

        // Set up ORA #$AA instruction at address $0400
        memory.borrow_mut().write(0x0400, ORA_IMM, false); // ORA Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA with immediate should take 3 cycles
        // (1 opcode fetch + 1 immediate addressing and operate)
        assert_eq!(cycles, 2, "ORA immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_0a() {
        let memory = create_test_memory();

        // Set up ASL A instruction at address $0400
        memory.borrow_mut().write(0x0400, ASL_A, false); // ASL A opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b0101_0101; // Value to shift left
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        assert_eq!(
            cpu.state.a, 0b1010_1010,
            "A should be shifted left to 0b1010_1010"
        );

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // ASL A should take 2 cycles (1 opcode fetch + 1 operation)
        assert_eq!(cycles, 2, "ASL A should take 2 cycles");
    }

    #[test]
    fn test_opcode_0b() {
        let memory = create_test_memory();

        // Set up AAC #$AA instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, AAC_IMM, false); // AAC Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 & 0b1010_1010 = 0b1000_0010
        assert_eq!(cpu.state.a, 0b1000_0010, "A should contain result of AND");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=1 (bit 7 of result is set)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            0x01,
            "C flag should be set (bit 7 of result)"
        );

        // AAC immediate should take 2 cycles
        assert_eq!(cycles, 2, "AAC immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_0c() {
        let memory = create_test_memory();

        // Set up TOP $1234 instruction at address $0400 (illegal opcode - triple NOP)
        memory.borrow_mut().write(0x0400, TOP_ABS, false); // TOP Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1234, 0x42, false); // Value at $1234 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.x = 0x22;
        cpu.state.y = 0x33;
        cpu.state.p = 0x44;

        let cycles = execute_instruction(&mut cpu);

        // TOP reads from memory but does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x22, "X should not change");
        assert_eq!(cpu.state.y, 0x33, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x44, "P should not change");

        // TOP with absolute should take 4 cycles
        assert_eq!(cycles, 4, "TOP absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_0d() {
        let memory = create_test_memory();

        // Set up ORA $1234 instruction at address $0400
        memory.borrow_mut().write(0x0400, ORA_ABS, false); // ORA Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1234, 0b1010_1010, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA with absolute should take 4 cycles
        // (1 opcode fetch + 3 absolute addressing)
        assert_eq!(cycles, 4, "ORA absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_0e() {
        let memory = create_test_memory();

        // Set up ASL $1234 instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x0E, false); // ASL Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1234, 0b0101_0101, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b1010_1010, "Memory should be shifted left");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // ASL absolute should take 6 cycles
        assert_eq!(cycles, 6, "ASL absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_0f() {
        let memory = create_test_memory();

        // Set up SLO $1234 instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x0F, false); // SLO Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1234, 0b0101_0101, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1010, "Memory should be shifted left");

        // A should be ORed with shifted value: 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(
            cpu.state.a, 0b1110_1011,
            "A should contain result of ORA with shifted value"
        );

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO absolute should take 6 cycles
        assert_eq!(cycles, 6, "SLO absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_10() {
        let memory = create_test_memory();

        // Set up BPL $10 instruction at address $0400 (branch if positive)
        memory.borrow_mut().write(0x0400, 0x10, false); // BPL opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0x00; // N flag clear (positive)

        let cycles = execute_instruction(&mut cpu);

        // PC should be at $0412 (0x0402 + 0x10)
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");

        // BPL taken should take 3 cycles (same page)
        assert_eq!(cycles, 3, "BPL taken (same page) should take 3 cycles");
    }

    #[test]
    fn test_opcode_10_not_taken() {
        let memory = create_test_memory();

        // Set up BPL $10 instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x10, false); // BPL opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_NEGATIVE; // N flag set (negative)

        let cycles = execute_instruction(&mut cpu);

        // PC should be at $0402 (not branched)
        assert_eq!(cpu.state.pc, 0x0402, "PC should not branch");

        // BPL not taken should take 2 cycles
        assert_eq!(cycles, 2, "BPL not taken should take 2 cycles");
    }

    #[test]
    fn test_opcode_11() {
        let memory = create_test_memory();

        // Set up ORA ($20),Y instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x11, false); // ORA Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA (Indirect),Y should take 5 cycles (no page cross)
        assert_eq!(cycles, 5, "ORA (Indirect),Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_12() {
        let memory = create_test_memory();

        // Set up KIL instruction at address $0400 (illegal opcode that halts CPU)
        memory.borrow_mut().write(0x0400, 0x12, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        // CPU should be halted
        assert!(cpu.is_halted(), "CPU should be halted");

        // KIL should take 1 cycle
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_13() {
        let memory = create_test_memory();

        // Set up SLO ($20),Y instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x13, false); // SLO Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1010, "Memory should be shifted left");

        // A should be ORed with shifted value: 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(
            cpu.state.a, 0b1110_1011,
            "A should contain result of ORA with shifted value"
        );

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO (Indirect),Y should take 8 cycles
        assert_eq!(cycles, 8, "SLO (Indirect),Y should take 8 cycles");
    }

    #[test]
    fn test_opcode_14() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x14, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x42, false); // Value at $25 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.y = 0x22;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP reads from memory but does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x05, "X should not change");
        assert_eq!(cpu.state.y, 0x22, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");

        // DOP with zero page,X should take 4 cycles
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_15() {
        let memory = create_test_memory();

        // Set up ORA $20,X instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x15, false); // ORA Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0b1010_1010, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA with zero page,X should take 4 cycles
        assert_eq!(cycles, 4, "ORA zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_16() {
        let memory = create_test_memory();

        // Set up ASL $20,X instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x16, false); // ASL Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0b0101_0101, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $25 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let result = memory.borrow().read(0x0025);
        assert_eq!(result, 0b1010_1010, "Memory should be shifted left");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // ASL zero page,X should take 6 cycles
        assert_eq!(cycles, 6, "ASL zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_17() {
        let memory = create_test_memory();

        // Set up SLO $20,X instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x17, false); // SLO Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0b0101_0101, false); // Value at $25

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $25 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let mem_result = memory.borrow().read(0x0025);
        assert_eq!(mem_result, 0b1010_1010, "Memory should be shifted left");

        // A should be ORed with shifted value: 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO zero page,X should take 6 cycles
        assert_eq!(cycles, 6, "SLO zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_18() {
        let memory = create_test_memory();

        // Set up CLC instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x18, false); // CLC opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0xFF; // All flags set

        let cycles = execute_instruction(&mut cpu);

        // Carry flag should be cleared
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "Carry flag should be clear");
        // Other flags should be unchanged
        assert_eq!(cpu.state.p & 0xFE, 0xFE, "Other flags should be unchanged");

        // CLC should take 2 cycles
        assert_eq!(cycles, 2, "CLC should take 2 cycles");
    }

    #[test]
    fn test_opcode_19() {
        let memory = create_test_memory();

        // Set up ORA $1234,Y instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x19, false); // ORA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0b1010_1010, false); // Value at $1238 (base $1234 + Y offset $04)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA absolute,Y should take 4 cycles (no page cross)
        assert_eq!(cycles, 4, "ORA absolute,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_1a() {
        let memory = create_test_memory();

        // Set up NOP instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x1A, false); // NOP implied opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.x = 0x22;
        cpu.state.y = 0x33;
        cpu.state.p = 0x44;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x22, "X should not change");
        assert_eq!(cpu.state.y, 0x33, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x44, "P should not change");

        // NOP implied should take 2 cycles
        assert_eq!(cycles, 2, "NOP implied should take 2 cycles");
    }

    #[test]
    fn test_opcode_1b() {
        let memory = create_test_memory();

        // Set up SLO $1234,Y instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x1B, false); // SLO Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0b0101_0101, false); // Value at $1238

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1238 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1010_1010, "Memory should be shifted left");

        // A should be ORed with shifted value: 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO absolute,Y should take 7 cycles
        assert_eq!(cycles, 7, "SLO absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_1c() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x1C, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0x42, false); // Value at $1238 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.y = 0x22;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // TOP reads from memory but does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x04, "X should not change");
        assert_eq!(cpu.state.y, 0x22, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");

        // TOP absolute,X should take 4 cycles (no page cross)
        assert_eq!(cycles, 4, "TOP absolute,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_1d() {
        let memory = create_test_memory();

        // Set up ORA $1234,X instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x1D, false); // ORA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0b1010_1010, false); // Value at $1238

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        // ORA absolute,X should take 4 cycles (no page cross)
        assert_eq!(cycles, 4, "ORA absolute,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_1e() {
        let memory = create_test_memory();

        // Set up ASL $1234,X instruction at address $0400
        memory.borrow_mut().write(0x0400, 0x1E, false); // ASL Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0b0101_0101, false); // Value at $1238

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1238 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let result = memory.borrow().read(0x1238);
        assert_eq!(result, 0b1010_1010, "Memory should be shifted left");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // ASL absolute,X should take 7 cycles
        assert_eq!(cycles, 7, "ASL absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_1f() {
        let memory = create_test_memory();

        // Set up SLO $1234,X instruction at address $0400 (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x1F, false); // SLO Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of address
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of address
        memory.borrow_mut().write(0x1238, 0b0101_0101, false); // Value at $1238

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1238 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1010_1010, "Memory should be shifted left");

        // A should be ORed with shifted value: 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        // SLO absolute,X should take 7 cycles
        assert_eq!(cycles, 7, "SLO absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_20() {
        let memory = create_test_memory();

        // JSR is already implemented and tested - this is just for completeness
        memory.borrow_mut().write(0x0400, JSR, false); // JSR opcode
        memory.borrow_mut().write(0x0401, 0x00, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x10, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(cpu.state.pc, 0x1000, "PC should jump to subroutine");
        assert_eq!(cycles, 6, "JSR should take 6 cycles");
    }

    #[test]
    fn test_opcode_21() {
        let memory = create_test_memory();

        // Set up AND ($20,X) instruction
        memory.borrow_mut().write(0x0400, AND_INDX, false); // AND (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24 (base $20 + X $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_1111 & 0b1010_1010 = 0b1010_1010
        assert_eq!(cpu.state.a, 0b1010_1010, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cycles, 6, "AND indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_22() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, KIL3, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_23() {
        let memory = create_test_memory();

        // Set up RLA ($20,X) instruction (illegal opcode - ROL + AND)
        memory.borrow_mut().write(0x0400, RLA_INDX, false); // RLA (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 8, "RLA indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_24() {
        let memory = create_test_memory();

        // Set up BIT $20 instruction
        memory.borrow_mut().write(0x0400, BIT_ZP, false); // BIT Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1100_0000, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Flags: N=1 (bit 7 of memory), V=1 (bit 6 of memory), Z=0 (AND result non-zero)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "V flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        // A should be unchanged
        assert_eq!(cpu.state.a, 0b1111_1111, "A should not change");

        assert_eq!(cycles, 3, "BIT zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_25() {
        let memory = create_test_memory();

        // Set up AND $20 instruction
        memory.borrow_mut().write(0x0400, AND_ZP, false); // AND Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        assert_eq!(cycles, 3, "AND zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_26() {
        let memory = create_test_memory();

        // Set up ROL $20 instruction
        memory.borrow_mut().write(0x0400, ROL_ZP, false); // ROL Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 5, "ROL zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_27() {
        let memory = create_test_memory();

        // Set up RLA $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_ZP, false); // RLA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x0020);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 5, "RLA zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_28() {
        let memory = create_test_memory();

        // Set up PLP instruction
        memory.borrow_mut().write(0x0400, PLP, false); // PLP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFC;
        cpu.state.p = 0x00;

        // Push a value onto the stack
        memory.borrow_mut().write(0x01FD, 0b1110_0101, false);

        let cycles = execute_instruction(&mut cpu);

        // Status should be pulled from stack (ignore bits 4 and 5)
        assert_eq!(
            cpu.state.p & 0xCF,
            0b1100_0101,
            "P should be pulled from stack"
        );
        assert_eq!(cpu.state.sp, 0xFD, "SP should increment by 1");

        assert_eq!(cycles, 4, "PLP should take 4 cycles");
    }

    #[test]
    fn test_opcode_29() {
        let memory = create_test_memory();

        // Set up AND #$AA instruction
        memory.borrow_mut().write(0x0400, AND_IMM, false); // AND Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        assert_eq!(cycles, 2, "AND immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_2a() {
        let memory = create_test_memory();

        // Set up ROL A instruction
        memory.borrow_mut().write(0x0400, 0x2A, false); // ROL A opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b0101_0101;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // A should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should be rotated left");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 2, "ROL A should take 2 cycles");
    }

    #[test]
    fn test_opcode_2b() {
        let memory = create_test_memory();

        // Set up AAC #$AA instruction (illegal opcode, same as 0x0B)
        memory.borrow_mut().write(0x0400, AAC_IMM2, false); // AAC Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 & 0b1010_1010 = 0b1000_0010
        assert_eq!(cpu.state.a, 0b1000_0010, "A should contain result of AND");

        // Flags: N=1, Z=0, C=1 (bit 7 of result)
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C flag should be set");

        assert_eq!(cycles, 2, "AAC immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_2c() {
        let memory = create_test_memory();

        // Set up BIT $1234 instruction
        memory.borrow_mut().write(0x0400, BIT_ABS, false); // BIT Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1100_0000, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Flags: N=1, V=1, Z=0
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            FLAG_OVERFLOW,
            "V flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        // A should be unchanged
        assert_eq!(cpu.state.a, 0b1111_1111, "A should not change");

        assert_eq!(cycles, 4, "BIT absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_2d() {
        let memory = create_test_memory();

        // Set up AND $1234 instruction
        memory.borrow_mut().write(0x0400, AND_ABS, false); // AND Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");

        assert_eq!(cycles, 4, "AND absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_2e() {
        let memory = create_test_memory();

        // Set up ROL $1234 instruction
        memory.borrow_mut().write(0x0400, ROL_ABS, false); // ROL Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");

        // Flags: N=1, Z=0, C=0
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 6, "ROL absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_2f() {
        let memory = create_test_memory();

        // Set up RLA $1234 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_ABS, false); // RLA Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");

        assert_eq!(cycles, 6, "RLA absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_30() {
        let memory = create_test_memory();

        // Set up BMI $10 instruction (branch if minus/negative)
        memory.borrow_mut().write(0x0400, BMI, false); // BMI opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_NEGATIVE; // N flag set (negative)

        let cycles = execute_instruction(&mut cpu);

        // PC should be at $0412 (0x0402 + 0x10)
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");
        assert_eq!(cycles, 3, "BMI taken (same page) should take 3 cycles");
    }

    #[test]
    fn test_opcode_31() {
        let memory = create_test_memory();

        // Set up AND ($20),Y instruction
        memory.borrow_mut().write(0x0400, AND_INDY, false); // AND (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cycles, 5, "AND indirect,Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_32() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, KIL4, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_33() {
        let memory = create_test_memory();

        // Set up RLA ($20),Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_INDY, false); // RLA (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");
        // A should be ANDed with result
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 8, "RLA indirect,Y should take 8 cycles");
    }

    #[test]
    fn test_opcode_34() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_ZPX2, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing - registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_35() {
        let memory = create_test_memory();

        // Set up AND $20,X instruction
        memory.borrow_mut().write(0x0400, AND_ZPX, false); // AND Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25 (base $20 + X $05)
        memory.borrow_mut().write(0x0025, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 4, "AND zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_36() {
        let memory = create_test_memory();

        // Set up ROL $20,X instruction
        memory.borrow_mut().write(0x0400, ROL_ZPX, false); // ROL Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25
        memory.borrow_mut().write(0x0025, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let result = memory.borrow().read(0x0025);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 6, "ROL zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_37() {
        let memory = create_test_memory();

        // Set up RLA $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_ZPX, false); // RLA Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25
        memory.borrow_mut().write(0x0025, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let mem_result = memory.borrow().read(0x0025);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");
        // A should be ANDed with result
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");
        assert_eq!(cycles, 6, "RLA zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_38() {
        let memory = create_test_memory();

        // Set up SEC instruction
        memory.borrow_mut().write(0x0400, SEC, false); // SEC opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0x00; // All flags clear

        let cycles = execute_instruction(&mut cpu);

        // Carry flag should be set
        assert_eq!(cpu.state.p & FLAG_CARRY, 0x01, "Carry flag should be set");
        assert_eq!(cycles, 2, "SEC should take 2 cycles");
    }

    #[test]
    fn test_opcode_39() {
        let memory = create_test_memory();

        // Set up AND $1234,Y instruction
        memory.borrow_mut().write(0x0400, AND_ABSY, false); // AND Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1238 (base $1234 + Y $04)
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 4, "AND absolute,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_3a() {
        let memory = create_test_memory();

        // Set up NOP instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, NOP_IMP2, false); // NOP implied opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x44;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x44, "P should not change");
        assert_eq!(cycles, 2, "NOP implied should take 2 cycles");
    }

    #[test]
    fn test_opcode_3b() {
        let memory = create_test_memory();

        // Set up RLA $1234,Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_ABSY, false); // RLA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1238
        memory.borrow_mut().write(0x1238, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");
        // A should be ANDed with result
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");
        assert_eq!(cycles, 7, "RLA absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_3c() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, TOP_ABSX2, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // TOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 4, "TOP absolute,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_3d() {
        let memory = create_test_memory();

        // Set up AND $1234,X instruction
        memory.borrow_mut().write(0x0400, AND_ABSX, false); // AND Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1238
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 & 0b1010_1010 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result of AND");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 4, "AND absolute,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_3e() {
        let memory = create_test_memory();

        // Set up ROL $1234,X instruction
        memory.borrow_mut().write(0x0400, ROL_ABSX, false); // ROL Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1238
        memory.borrow_mut().write(0x1238, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let result = memory.borrow().read(0x1238);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(cycles, 7, "ROL absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_3f() {
        let memory = create_test_memory();

        // Set up RLA $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RLA_ABSX, false); // RLA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1238
        memory.borrow_mut().write(0x1238, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_1111;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");
        // A should be ANDed with result
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");
        assert_eq!(cycles, 7, "RLA absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_40() {
        let memory = create_test_memory();

        // Set up RTI instruction
        memory.borrow_mut().write(0x0400, 0x40, false); // RTI opcode

        // Push status and return address to stack
        memory.borrow_mut().write(0x01FD, 0x12, false); // PCH
        memory.borrow_mut().write(0x01FC, 0x34, false); // PCL
        memory.borrow_mut().write(0x01FB, 0b1100_0011, false); // Status

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFA;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(cpu.state.pc, 0x1234, "PC should be restored");
        assert_eq!(
            cpu.state.p, 0b1110_0011,
            "Status should be restored (ignore bits 4-5)"
        );
        assert_eq!(cpu.state.sp, 0xFD, "SP should be incremented by 3");
        assert_eq!(cycles, 6, "RTI should take 6 cycles");
    }

    #[test]
    fn test_opcode_41() {
        let memory = create_test_memory();

        // Set up EOR ($20,X) instruction
        memory.borrow_mut().write(0x0400, 0x41, false); // EOR (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24 (base $20 + X $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(cpu.state.p & FLAG_NEGATIVE, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cycles, 6, "EOR indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_42() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, 0x42, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_43() {
        let memory = create_test_memory();

        // Set up SRE ($20,X) instruction (illegal opcode - LSR + EOR)
        memory.borrow_mut().write(0x0400, 0x43, false); // SRE (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right: 0b1010_1010 >> 1 = 0b0101_0101
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result: 0b1111_0000 ^ 0b0101_0101 = 0b1010_0101
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");
        assert_eq!(cycles, 8, "SRE indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_44() {
        let memory = create_test_memory();

        // Set up DOP $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x44, false); // DOP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 3, "DOP zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_45() {
        let memory = create_test_memory();

        // Set up EOR $20 instruction
        memory.borrow_mut().write(0x0400, 0x45, false); // EOR Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(cpu.state.p & FLAG_NEGATIVE, 0, "N flag should be clear");
        assert_eq!(cycles, 3, "EOR zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_46() {
        let memory = create_test_memory();

        // Set up LSR $20 instruction
        memory.borrow_mut().write(0x0400, 0x46, false); // LSR Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1010_1011, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right: 0b1010_1011 >> 1 = 0b0101_0101
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0b0101_0101, "Memory should be shifted right");
        assert_eq!(cpu.state.p & FLAG_NEGATIVE, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            1,
            "C flag should be set (bit 0 was 1)"
        );
        assert_eq!(cycles, 5, "LSR zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_47() {
        let memory = create_test_memory();

        // Set up SRE $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x47, false); // SRE Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x0020);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 5, "SRE zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_48() {
        let memory = create_test_memory();

        // Set up PHA instruction
        memory.borrow_mut().write(0x0400, 0x48, false); // PHA opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // Check stack
        let stack_value = memory.borrow().read(0x01FD);
        assert_eq!(stack_value, 0x42, "A should be pushed to stack");
        assert_eq!(cpu.state.sp, 0xFC, "SP should be decremented");
        assert_eq!(cycles, 3, "PHA should take 3 cycles");
    }

    #[test]
    fn test_opcode_49() {
        let memory = create_test_memory();

        // Set up EOR #$AA instruction
        memory.borrow_mut().write(0x0400, 0x49, false); // EOR Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(cycles, 2, "EOR immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_4a() {
        let memory = create_test_memory();

        // Set up LSR A instruction
        memory.borrow_mut().write(0x0400, 0x4A, false); // LSR Accumulator opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1010_1011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be shifted right: 0b1010_1011 >> 1 = 0b0101_0101
        assert_eq!(cpu.state.a, 0b0101_0101, "A should be shifted right");
        assert_eq!(cpu.state.p & FLAG_NEGATIVE, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            1,
            "C flag should be set (bit 0 was 1)"
        );
        assert_eq!(cycles, 2, "LSR accumulator should take 2 cycles");
    }

    #[test]
    fn test_opcode_4b() {
        let memory = create_test_memory();

        // Set up ASR #$AA instruction (illegal opcode - AND + LSR)
        memory.borrow_mut().write(0x0400, 0x4B, false); // ASR Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be ANDed then shifted: (0b1111_0011 & 0b1010_1010) >> 1 = 0b1010_0010 >> 1 = 0b0101_0001
        assert_eq!(cpu.state.a, 0b0101_0001, "A should contain AND+LSR result");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");
        assert_eq!(cycles, 2, "ASR immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_4c() {
        let memory = create_test_memory();

        // Set up JMP $5678 instruction at address $0800
        memory.borrow_mut().write(0x0800, JMP_ABS, false); // JMP Absolute opcode (0x4C)
        memory.borrow_mut().write(0x0801, 0x78, false); // Low byte of target
        memory.borrow_mut().write(0x0802, 0x56, false); // High byte of target

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0xAA;
        cpu.state.x = 0xBB;
        cpu.state.y = 0xCC;
        cpu.state.p = 0xDD;

        let cycles = execute_instruction(&mut cpu);

        // Verify the CPU state after JMP execution
        assert_eq!(cpu.state.pc, 0x5678, "PC should jump to target address");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.a, 0xAA, "A should not change");
        assert_eq!(cpu.state.x, 0xBB, "X should not change");
        assert_eq!(cpu.state.y, 0xCC, "Y should not change");
        assert_eq!(cpu.state.p, 0xDD, "P should not change");
        assert_eq!(
            cycles, 3,
            "JMP Absolute should take 3 cycles total (2 addressing + 1 execution overlapped)"
        );
    }

    #[test]
    fn test_opcode_4d() {
        let memory = create_test_memory();

        // Set up EOR $1234 instruction
        memory.borrow_mut().write(0x0400, 0x4D, false); // EOR Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(cycles, 4, "EOR absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_4e() {
        let memory = create_test_memory();

        // Set up LSR $1234 instruction
        memory.borrow_mut().write(0x0400, 0x4E, false); // LSR Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1011, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b0101_0101, "Memory should be shifted right");
        assert_eq!(cpu.state.p & FLAG_CARRY, 1, "C flag should be set");
        assert_eq!(cycles, 6, "LSR absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_4f() {
        let memory = create_test_memory();

        // Set up SRE $1234 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x4F, false); // SRE Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 6, "SRE absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_50() {
        let memory = create_test_memory();

        // Set up BVC instruction (Branch if Overflow Clear)
        memory.borrow_mut().write(0x0400, 0x50, false); // BVC opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0; // V flag clear

        let cycles = execute_instruction(&mut cpu);

        // Branch should be taken
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");
        assert_eq!(
            cycles, 3,
            "BVC with branch taken (no page cross) should take 3 cycles"
        );
    }

    #[test]
    fn test_opcode_51() {
        let memory = create_test_memory();

        // Set up EOR ($20),Y instruction
        memory.borrow_mut().write(0x0400, 0x51, false); // EOR (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20
        memory.borrow_mut().write(0x0020, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(
            cycles, 5,
            "EOR indirect indexed should take 5 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_52() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, 0x52, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_53() {
        let memory = create_test_memory();

        // Set up SRE ($20),Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x53, false); // SRE (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20
        memory.borrow_mut().write(0x0020, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 8, "SRE indirect indexed should take 8 cycles");
    }

    #[test]
    fn test_opcode_54() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x54, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_55() {
        let memory = create_test_memory();

        // Set up EOR $20,X instruction
        memory.borrow_mut().write(0x0400, 0x55, false); // EOR Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25
        memory.borrow_mut().write(0x0025, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(cycles, 4, "EOR zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_56() {
        let memory = create_test_memory();

        // Set up LSR $20,X instruction
        memory.borrow_mut().write(0x0400, 0x56, false); // LSR Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25
        memory.borrow_mut().write(0x0025, 0b1010_1011, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let result = memory.borrow().read(0x0025);
        assert_eq!(result, 0b0101_0101, "Memory should be shifted right");
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            1,
            "C flag should be set (bit 0 was 1)"
        );
        assert_eq!(cycles, 6, "LSR zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_57() {
        let memory = create_test_memory();

        // Set up SRE $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x57, false); // SRE Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up value at $25
        memory.borrow_mut().write(0x0025, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x0025);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 6, "SRE zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_58() {
        let memory = create_test_memory();

        // Set up CLI instruction
        memory.borrow_mut().write(0x0400, 0x58, false); // CLI opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0xFF; // All flags set

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(cpu.state.p & 0x04, 0, "I flag should be clear");
        assert_eq!(cycles, 2, "CLI should take 2 cycles");
    }

    #[test]
    fn test_opcode_59() {
        let memory = create_test_memory();

        // Set up EOR $1234,Y instruction
        memory.borrow_mut().write(0x0400, 0x59, false); // EOR Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(
            cycles, 4,
            "EOR absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_5a() {
        let memory = create_test_memory();

        // Set up NOP instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x5A, false); // NOP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 2, "NOP should take 2 cycles");
    }

    #[test]
    fn test_opcode_5b() {
        let memory = create_test_memory();

        // Set up SRE $1234,Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x5B, false); // SRE Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 7, "SRE absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_5c() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x5C, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // TOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(
            cycles, 4,
            "TOP absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_5d() {
        let memory = create_test_memory();

        // Set up EOR $1234,X instruction
        memory.borrow_mut().write(0x0400, 0x5D, false); // EOR Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1111_0000 ^ 0b1010_1010 = 0b0101_1010
        assert_eq!(cpu.state.a, 0b0101_1010, "A should contain result of EOR");
        assert_eq!(
            cycles, 4,
            "EOR absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_5e() {
        let memory = create_test_memory();

        // Set up LSR $1234,X instruction
        memory.borrow_mut().write(0x0400, 0x5E, false); // LSR Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0b1010_1011, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let result = memory.borrow().read(0x1238);
        assert_eq!(result, 0b0101_0101, "Memory should be shifted right");
        assert_eq!(cpu.state.p & FLAG_CARRY, 1, "C flag should be set");
        assert_eq!(cycles, 7, "LSR absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_5f() {
        let memory = create_test_memory();

        // Set up SRE $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, 0x5F, false); // SRE Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be shifted right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b0101_0101, "Memory should be shifted right");
        // A should be EORed with result
        assert_eq!(cpu.state.a, 0b1010_0101, "A should contain EOR result");
        assert_eq!(cycles, 7, "SRE absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_60() {
        let memory = create_test_memory();

        // Set up RTS instruction
        memory.borrow_mut().write(0x0400, RTS, false); // RTS opcode

        // Push return address to stack (RTS pulls PC-1, so push 0x1233)
        memory.borrow_mut().write(0x01FD, 0x33, false); // PCL
        memory.borrow_mut().write(0x01FE, 0x12, false); // PCH

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFC;

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(
            cpu.state.pc, 0x1234,
            "PC should be set to return address + 1"
        );
        assert_eq!(cpu.state.sp, 0xFE, "SP should be incremented by 2");
        assert_eq!(cycles, 6, "RTS should take 6 cycles");
    }

    #[test]
    fn test_opcode_61() {
        let memory = create_test_memory();

        // Set up ADC ($20,X) instruction
        memory.borrow_mut().write(0x0400, ADC_INDX, false); // ADC (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24 (base $20 + X $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(cpu.state.p & FLAG_CARRY, 0, "C flag should be clear");
        assert_eq!(cycles, 6, "ADC indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_62() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, KIL7, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_63() {
        let memory = create_test_memory();

        // Set up RRA ($20,X) instruction (illegal opcode - ROR + ADC)
        memory.borrow_mut().write(0x0400, RRA_INDX, false); // RRA (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base

        // Set up pointer at $24
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right: 0b1010_1010 ROR with C=1 = 0b1101_0101
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result: 0x10 + 0xD5 = 0xE5
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 8, "RRA indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_64() {
        let memory = create_test_memory();

        // Set up DOP $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_ZP3, false); // DOP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 3, "DOP zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_65() {
        let memory = create_test_memory();

        // Set up ADC $20 instruction
        memory.borrow_mut().write(0x0400, ADC_ZP, false); // ADC Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(cycles, 3, "ADC zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_66() {
        let memory = create_test_memory();

        // Set up ROR $20 instruction
        memory.borrow_mut().write(0x0400, ROR_ZP, false); // ROR Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right: 0b0101_0101 ROR with C=1 = 0b1010_1010
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0b1010_1010, "Memory should be rotated right");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            FLAG_CARRY,
            "C flag should be set (bit 0 was 1)"
        );
        assert_eq!(cycles, 5, "ROR zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_67() {
        let memory = create_test_memory();

        // Set up RRA $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_ZP, false); // RRA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x0020);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 5, "RRA zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_68() {
        let memory = create_test_memory();

        // Set up PLA instruction
        memory.borrow_mut().write(0x0400, PLA, false); // PLA opcode

        // Push value to stack
        memory.borrow_mut().write(0x01FD, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFC;
        cpu.state.a = 0x00;

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(cpu.state.a, 0x42, "A should be pulled from stack");
        assert_eq!(cpu.state.sp, 0xFD, "SP should be incremented");
        assert_eq!(cycles, 4, "PLA should take 4 cycles");
    }

    #[test]
    fn test_opcode_69() {
        let memory = create_test_memory();

        // Set up ADC #$05 instruction
        memory.borrow_mut().write(0x0400, ADC_IMM, false); // ADC Immediate opcode
        memory.borrow_mut().write(0x0401, 0x05, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(cycles, 2, "ADC immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_6a() {
        let memory = create_test_memory();

        // Set up ROR A instruction
        memory.borrow_mut().write(0x0400, ROR_ACC, false); // ROR Accumulator opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b0101_0101;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // A should be rotated right: 0b0101_0101 ROR with C=1 = 0b1010_1010
        assert_eq!(cpu.state.a, 0b1010_1010, "A should be rotated right");
        assert_eq!(
            cpu.state.p & FLAG_NEGATIVE,
            FLAG_NEGATIVE,
            "N flag should be set"
        );
        assert_eq!(
            cpu.state.p & FLAG_CARRY,
            FLAG_CARRY,
            "C flag should be set (bit 0 was 1)"
        );
        assert_eq!(cycles, 2, "ROR accumulator should take 2 cycles");
    }

    #[test]
    fn test_opcode_6b() {
        let memory = create_test_memory();

        // Set up ARR #$AA instruction (illegal opcode - AND + ROR)
        memory.borrow_mut().write(0x0400, ARR_IMM, false); // ARR Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1010_1010, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0011;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // A should be ANDed then rotated: (0b1111_0011 & 0b1010_1010) ROR with C=1 = 0b1010_0010 ROR = 0b1101_0001
        assert_eq!(cpu.state.a, 0b1101_0001, "A should contain AND+ROR result");
        assert_eq!(cycles, 2, "ARR immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_6c() {
        let memory = create_test_memory();

        // Set up JMP ($1200) instruction at address $0800
        memory.borrow_mut().write(0x0800, JMP_IND, false); // JMP Indirect opcode (0x6C)
        memory.borrow_mut().write(0x0801, 0x00, false); // Low byte of indirect address
        memory.borrow_mut().write(0x0802, 0x12, false); // High byte of indirect address

        // Set up the target address at the indirect location $1200
        memory.borrow_mut().write(0x1200, 0x34, false); // Low byte of target
        memory.borrow_mut().write(0x1201, 0x56, false); // High byte of target

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0xAA;
        cpu.state.x = 0xBB;
        cpu.state.y = 0xCC;
        cpu.state.p = 0xDD;

        let cycles = execute_instruction(&mut cpu);

        // Verify the CPU state after JMP execution
        assert_eq!(cpu.state.pc, 0x5634, "PC should jump to target address");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.a, 0xAA, "A should not change");
        assert_eq!(cpu.state.x, 0xBB, "X should not change");
        assert_eq!(cpu.state.y, 0xCC, "Y should not change");
        assert_eq!(cpu.state.p, 0xDD, "P should not change");
        assert_eq!(
            cycles, 5,
            "JMP Indirect should take 5 cycles total (4 addressing + 1 execution overlapped)"
        );
    }

    #[test]
    fn test_opcode_6c_boundary_bug() {
        let memory = create_test_memory();

        // Set up JMP ($12FF) instruction at address $0800
        // This tests the page boundary bug where high byte is read from $1200 instead of $1300
        memory.borrow_mut().write(0x0800, JMP_IND, false); // JMP Indirect opcode (0x6C)
        memory.borrow_mut().write(0x0801, 0xFF, false); // Low byte of indirect address
        memory.borrow_mut().write(0x0802, 0x12, false); // High byte of indirect address

        // Set up target address with page boundary bug
        memory.borrow_mut().write(0x12FF, 0x34, false); // Low byte at $12FF
        memory.borrow_mut().write(0x1200, 0x56, false); // High byte wraps to $1200 (bug)
        memory.borrow_mut().write(0x1300, 0x99, false); // This would be correct but is not used

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;

        execute_instruction(&mut cpu);

        // Verify the CPU jumps to $5634 (using $1200 for high byte, not $1300)
        assert_eq!(
            cpu.state.pc, 0x5634,
            "PC should use page boundary bug (high byte from $1200, not $1300)"
        );
    }

    #[test]
    fn test_opcode_6d() {
        let memory = create_test_memory();

        // Set up ADC $1234 instruction
        memory.borrow_mut().write(0x0400, ADC_ABS, false); // ADC Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(cycles, 4, "ADC absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_6e() {
        let memory = create_test_memory();

        // Set up ROR $1234 instruction
        memory.borrow_mut().write(0x0400, ROR_ABS, false); // ROR Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b1010_1010, "Memory should be rotated right");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C flag should be set");
        assert_eq!(cycles, 6, "ROR absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_6f() {
        let memory = create_test_memory();

        // Set up RRA $1234 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_ABS, false); // RRA Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.p = FLAG_CARRY; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 6, "RRA absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_70() {
        let memory = create_test_memory();

        // Set up BVS instruction with branch offset
        memory.borrow_mut().write(0x0400, BVS, false); // BVS opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Branch offset +16

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_OVERFLOW; // Set overflow flag

        let cycles = execute_instruction(&mut cpu);

        // PC should branch: 0x0402 + 0x10 = 0x0412
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch when V flag is set");
        assert_eq!(cycles, 3, "BVS should take 3 cycles when branch taken");
    }

    #[test]
    fn test_opcode_71() {
        let memory = create_test_memory();

        // Set up ADC ($20),Y instruction
        memory.borrow_mut().write(0x0400, ADC_INDY, false); // ADC (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20
        memory.borrow_mut().write(0x0020, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(
            cycles, 5,
            "ADC indirect indexed should take 5 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_72() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, KIL8, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_73() {
        let memory = create_test_memory();

        // Set up RRA ($20),Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_INDY, false); // RRA (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20
        memory.borrow_mut().write(0x0020, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 8, "RRA indirect indexed should take 8 cycles");
    }

    #[test]
    fn test_opcode_74() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_ZPX4, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_75() {
        let memory = create_test_memory();

        // Set up ADC $20,X instruction
        memory.borrow_mut().write(0x0400, ADC_ZPX, false); // ADC Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $24 (wraps in zero page)
        memory.borrow_mut().write(0x0024, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(cycles, 4, "ADC zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_76() {
        let memory = create_test_memory();

        // Set up ROR $20,X instruction
        memory.borrow_mut().write(0x0400, ROR_ZPX, false); // ROR Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $24
        memory.borrow_mut().write(0x0024, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let result = memory.borrow().read(0x0024);
        assert_eq!(result, 0b1010_1010, "Memory should be rotated right");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C flag should be set");
        assert_eq!(cycles, 6, "ROR zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_77() {
        let memory = create_test_memory();

        // Set up RRA $20,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_ZPX, false); // RRA Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $24
        memory.borrow_mut().write(0x0024, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x0024);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 6, "RRA zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_78() {
        let memory = create_test_memory();

        // Set up SEI instruction
        memory.borrow_mut().write(0x0400, SEI, false); // SEI opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set"
        );
        assert_eq!(cycles, 2, "SEI should take 2 cycles");
    }

    #[test]
    fn test_opcode_79() {
        let memory = create_test_memory();

        // Set up ADC $1234,Y instruction
        memory.borrow_mut().write(0x0400, ADC_ABSY, false); // ADC Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(
            cycles, 4,
            "ADC absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_7a() {
        let memory = create_test_memory();

        // Set up NOP instruction
        memory.borrow_mut().write(0x0400, NOP_IMP4, false); // NOP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 2, "NOP should take 2 cycles");
    }

    #[test]
    fn test_opcode_7b() {
        let memory = create_test_memory();

        // Set up RRA $1234,Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_ABSY, false); // RRA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 7, "RRA absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_7c() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, TOP_ABSX4, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // TOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(
            cycles, 4,
            "TOP absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_7d() {
        let memory = create_test_memory();

        // Set up ADC $1234,X instruction
        memory.borrow_mut().write(0x0400, ADC_ABSX, false); // ADC Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0x05, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0x10 + 0x05 = 0x15
        assert_eq!(cpu.state.a, 0x15, "A should contain sum");
        assert_eq!(
            cycles, 4,
            "ADC absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_7e() {
        let memory = create_test_memory();

        // Set up ROR $1234,X instruction
        memory.borrow_mut().write(0x0400, ROR_ABSX, false); // ROR Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0b0101_0101, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let result = memory.borrow().read(0x1238);
        assert_eq!(result, 0b1010_1010, "Memory should be rotated right");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C flag should be set");
        assert_eq!(cycles, 7, "ROR absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_7f() {
        let memory = create_test_memory();

        // Set up RRA $1234,X instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, RRA_ABSX, false); // RRA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X
        memory.borrow_mut().write(0x1238, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x10;
        cpu.state.x = 0x04;
        cpu.state.p = FLAG_CARRY;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated right
        let mem_result = memory.borrow().read(0x1238);
        assert_eq!(mem_result, 0b1101_0101, "Memory should be rotated right");
        // A should be ADCed with result
        assert_eq!(cpu.state.a, 0xE5, "A should contain ADC result");
        assert_eq!(cycles, 7, "RRA absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_80() {
        let memory = create_test_memory();

        // Set up DOP #$12 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_IMM, false); // DOP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x12, false); // Immediate value (ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 2, "DOP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_81() {
        let memory = create_test_memory();

        // Set up STA ($20,X) instruction
        memory.borrow_mut().write(0x0400, STA_INDX, false); // STA (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $24 (0x20 + 0x04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0x42, "A should be stored at target address");
        assert_eq!(cycles, 6, "STA (indirect,X) should take 6 cycles");
    }

    #[test]
    fn test_opcode_82() {
        let memory = create_test_memory();

        // Set up DOP #$12 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_IMM2, false); // DOP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x12, false); // Immediate value (ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 2, "DOP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_83() {
        let memory = create_test_memory();

        // Set up SAX ($20,X) instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, SAX_INDX, false); // SAX (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $24 (0x20 + 0x04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // A AND X should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b0000_0000, "A AND X should be stored");
        assert_eq!(cycles, 6, "SAX (indirect,X) should take 6 cycles");
    }

    #[test]
    fn test_opcode_84() {
        let memory = create_test_memory();

        // Set up STY $20 instruction
        memory.borrow_mut().write(0x0400, STY_ZP, false); // STY Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // Y should be stored at $20
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0x42, "Y should be stored at zero page address");
        assert_eq!(cycles, 3, "STY zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_85() {
        let memory = create_test_memory();

        // Set up STA $20 instruction
        memory.borrow_mut().write(0x0400, STA_ZP, false); // STA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $20
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0x42, "A should be stored at zero page address");
        assert_eq!(cycles, 3, "STA zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_86() {
        let memory = create_test_memory();

        // Set up STX $20 instruction
        memory.borrow_mut().write(0x0400, STX_ZP, false); // STX Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // X should be stored at $20
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0x42, "X should be stored at zero page address");
        assert_eq!(cycles, 3, "STX zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_87() {
        let memory = create_test_memory();

        // Set up SAX $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, SAX_ZP, false); // SAX Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0b1010_1010;

        let cycles = execute_instruction(&mut cpu);

        // A AND X should be stored at $20
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0b1010_0000, "A AND X should be stored");
        assert_eq!(cycles, 3, "SAX zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_88() {
        let memory = create_test_memory();

        // Set up DEY instruction
        memory.borrow_mut().write(0x0400, DEY, false); // DEY opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x42;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be decremented
        assert_eq!(cpu.state.y, 0x41, "Y should be decremented");
        assert_eq!(cycles, 2, "DEY should take 2 cycles");
    }

    #[test]
    fn test_opcode_89() {
        let memory = create_test_memory();

        // Set up DOP #$12 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, DOP_IMM3, false); // DOP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x12, false); // Immediate value (ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x11;
        cpu.state.p = 0x33;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.p, 0x33, "P should not change");
        assert_eq!(cycles, 2, "DOP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_8a() {
        let memory = create_test_memory();

        // Set up TXA instruction
        memory.borrow_mut().write(0x0400, TXA, false); // TXA opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x42;
        cpu.state.a = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should contain X
        assert_eq!(cpu.state.a, 0x42, "A should contain X");
        assert_eq!(cycles, 2, "TXA should take 2 cycles");
    }

    #[test]
    fn test_opcode_8b() {
        let memory = create_test_memory();

        // Set up XAA #$12 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, XAA_IMM, false); // XAA Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1111_0000, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1010_1010;
        cpu.state.x = 0b0011_1100;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be (A OR MAGIC) AND X AND IMM
        // XAA is highly unstable, but we'll implement a basic version
        assert_eq!(cycles, 2, "XAA immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_8c() {
        let memory = create_test_memory();

        // Set up STY $1234 instruction
        memory.borrow_mut().write(0x0400, STY_ABS, false); // STY Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // Y should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0x42, "Y should be stored at absolute address");
        assert_eq!(cycles, 4, "STY absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_8d() {
        let memory = create_test_memory();

        // Set up STA $1234 instruction
        memory.borrow_mut().write(0x0400, STA_ABS, false); // STA Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0x42, "A should be stored at absolute address");
        assert_eq!(cycles, 4, "STA absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_8e() {
        let memory = create_test_memory();

        // Set up STX $1234 instruction
        memory.borrow_mut().write(0x0400, STX_ABS, false); // STX Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x42;

        let cycles = execute_instruction(&mut cpu);

        // X should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0x42, "X should be stored at absolute address");
        assert_eq!(cycles, 4, "STX absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_8f() {
        let memory = create_test_memory();

        // Set up SAX $1234 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, SAX_ABS, false); // SAX Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1111_0000;
        cpu.state.x = 0b1010_1010;

        let cycles = execute_instruction(&mut cpu);

        // A AND X should be stored at $1234
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b1010_0000, "A AND X should be stored");
        assert_eq!(cycles, 4, "SAX absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_a0() {
        let memory = create_test_memory();

        // Set up LDY #$42 instruction
        memory.borrow_mut().write(0x0400, LDY_IMM, false); // LDY Immediate opcode
        memory.borrow_mut().write(0x0401, 0x42, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be loaded with 0x42
        assert_eq!(cpu.state.y, 0x42, "Y should be loaded with immediate value");
        assert_eq!(cycles, 2, "LDY immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_a1() {
        let memory = create_test_memory();

        // Set up LDA ($20,X) instruction
        memory.borrow_mut().write(0x0400, LDA_INDX, false); // LDA (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $24 (0x20 + 0x04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from target address");
        assert_eq!(cycles, 6, "LDA (indirect,X) should take 6 cycles");
    }

    #[test]
    fn test_opcode_a2() {
        let memory = create_test_memory();

        // Set up LDX #$42 instruction
        memory.borrow_mut().write(0x0400, LDX_IMM, false); // LDX Immediate opcode
        memory.borrow_mut().write(0x0401, 0x42, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be loaded with 0x42
        assert_eq!(cpu.state.x, 0x42, "X should be loaded with immediate value");
        assert_eq!(cycles, 2, "LDX immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_a3() {
        let memory = create_test_memory();

        // Set up LAX ($20,X) instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_INDX, false); // LAX (Indirect,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $24 (0x20 + 0x04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from target address");
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from target address");
        assert_eq!(cycles, 6, "LAX (indirect,X) should take 6 cycles");
    }

    #[test]
    fn test_opcode_a4() {
        let memory = create_test_memory();

        // Set up LDY $20 instruction
        memory.borrow_mut().write(0x0400, LDY_ZP, false); // LDY Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be loaded with 0x42
        assert_eq!(cpu.state.y, 0x42, "Y should be loaded from zero page");
        assert_eq!(cycles, 3, "LDY zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_a5() {
        let memory = create_test_memory();

        // Set up LDA $20 instruction
        memory.borrow_mut().write(0x0400, LDA_ZP, false); // LDA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from zero page");
        assert_eq!(cycles, 3, "LDA zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_a6() {
        let memory = create_test_memory();

        // Set up LDX $20 instruction
        memory.borrow_mut().write(0x0400, LDX_ZP, false); // LDX Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be loaded with 0x42
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from zero page");
        assert_eq!(cycles, 3, "LDX zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_a7() {
        let memory = create_test_memory();

        // Set up LAX $20 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_ZP, false); // LAX Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at $20
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from zero page");
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from zero page");
        assert_eq!(cycles, 3, "LAX zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_a8() {
        let memory = create_test_memory();

        // Set up TAY instruction
        memory.borrow_mut().write(0x0400, TAY, false); // TAY opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.y = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should contain A
        assert_eq!(cpu.state.y, 0x42, "Y should contain A");
        assert_eq!(cycles, 2, "TAY should take 2 cycles");
    }

    #[test]
    fn test_opcode_a9() {
        let memory = create_test_memory();

        // Set up LDA #$42 instruction
        memory.borrow_mut().write(0x0400, LDA_IMM, false); // LDA Immediate opcode
        memory.borrow_mut().write(0x0401, 0x42, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded with immediate value");
        assert_eq!(cycles, 2, "LDA immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_aa() {
        let memory = create_test_memory();

        // Set up TAX instruction
        memory.borrow_mut().write(0x0400, TAX, false); // TAX opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should contain A
        assert_eq!(cpu.state.x, 0x42, "X should contain A");
        assert_eq!(cycles, 2, "TAX should take 2 cycles");
    }

    #[test]
    fn test_opcode_ab() {
        let memory = create_test_memory();

        // Set up ATX #$42 instruction (illegal opcode, also called OAL/ANE)
        memory.borrow_mut().write(0x0400, ATX_IMM, false); // ATX Immediate opcode
        memory.borrow_mut().write(0x0401, 0b1111_0000, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1010_1010;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // ATX: A = X = (A OR CONST) AND IMM
        // CONST is typically 0xEE or 0xFF, we'll use 0xFF for simplicity
        // Result: (0b1010_1010 OR 0xFF) AND 0b1111_0000 = 0b1111_0000
        assert_eq!(cpu.state.a, 0b1111_0000, "A should contain result");
        assert_eq!(cpu.state.x, 0b1111_0000, "X should contain result");
        assert_eq!(cycles, 2, "ATX immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_ac() {
        let memory = create_test_memory();

        // Set up LDY $1234 instruction
        memory.borrow_mut().write(0x0400, LDY_ABS, false); // LDY Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be loaded with 0x42
        assert_eq!(
            cpu.state.y, 0x42,
            "Y should be loaded from absolute address"
        );
        assert_eq!(cycles, 4, "LDY absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_ad() {
        let memory = create_test_memory();

        // Set up LDA $1234 instruction
        memory.borrow_mut().write(0x0400, LDA_ABS, false); // LDA Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(
            cpu.state.a, 0x42,
            "A should be loaded from absolute address"
        );
        assert_eq!(cycles, 4, "LDA absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_ae() {
        let memory = create_test_memory();

        // Set up LDX $1234 instruction
        memory.borrow_mut().write(0x0400, LDX_ABS, false); // LDX Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be loaded with 0x42
        assert_eq!(
            cpu.state.x, 0x42,
            "X should be loaded from absolute address"
        );
        assert_eq!(cycles, 4, "LDX absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_af() {
        let memory = create_test_memory();

        // Set up LAX $1234 instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_ABS, false); // LAX Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(
            cpu.state.a, 0x42,
            "A should be loaded from absolute address"
        );
        assert_eq!(
            cpu.state.x, 0x42,
            "X should be loaded from absolute address"
        );
        assert_eq!(cycles, 4, "LAX absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_b0() {
        let memory = create_test_memory();

        // Set up BCS instruction with positive offset
        memory.borrow_mut().write(0x0400, BCS, false); // BCS opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Offset +16

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_CARRY; // Carry is set, branch should be taken

        let cycles = execute_instruction(&mut cpu);

        // PC should be 0x0402 + 0x10 = 0x0412
        assert_eq!(cpu.state.pc, 0x0412, "Branch should be taken");
        assert_eq!(
            cycles, 3,
            "BCS with branch taken (same page) should take 3 cycles"
        );
    }

    #[test]
    fn test_opcode_b1() {
        let memory = create_test_memory();

        // Set up LDA ($20),Y instruction
        memory.borrow_mut().write(0x0400, LDA_INDY, false); // LDA (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address containing pointer

        // Set up pointer at $20-$21 pointing to $1200
        memory.borrow_mut().write(0x0020, 0x00, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1200 + Y ($05) = $1205
        memory.borrow_mut().write(0x1205, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(
            cpu.state.a, 0x42,
            "A should be loaded from indirect indexed address"
        );
        assert_eq!(
            cycles, 5,
            "LDA (indirect),Y should take 5 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_b2() {
        let memory = create_test_memory();

        // Set up KIL instruction
        memory.borrow_mut().write(0x0400, KIL10, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        // KIL halts the CPU, PC should not advance beyond the opcode
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_b3() {
        let memory = create_test_memory();

        // Set up LAX ($20),Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_INDY, false); // LAX (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address containing pointer

        // Set up pointer at $20-$21 pointing to $1200
        memory.borrow_mut().write(0x0020, 0x00, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1200 + Y ($05) = $1205
        memory.borrow_mut().write(0x1205, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(
            cpu.state.a, 0x42,
            "A should be loaded from indirect indexed address"
        );
        assert_eq!(
            cpu.state.x, 0x42,
            "X should be loaded from indirect indexed address"
        );
        assert_eq!(
            cycles, 5,
            "LAX (indirect),Y should take 5 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_b4() {
        let memory = create_test_memory();

        // Set up LDY $20,X instruction
        memory.borrow_mut().write(0x0400, LDY_ZPX, false); // LDY Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up value at $20 + X ($05) = $25
        memory.borrow_mut().write(0x0025, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x00;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be loaded with 0x42
        assert_eq!(cpu.state.y, 0x42, "Y should be loaded from zero page,X");
        assert_eq!(cycles, 4, "LDY zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_b5() {
        let memory = create_test_memory();

        // Set up LDA $20,X instruction
        memory.borrow_mut().write(0x0400, LDA_ZPX, false); // LDA Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up value at $20 + X ($05) = $25
        memory.borrow_mut().write(0x0025, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from zero page,X");
        assert_eq!(cycles, 4, "LDA zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_b6() {
        let memory = create_test_memory();

        // Set up LDX $20,Y instruction
        memory.borrow_mut().write(0x0400, LDX_ZPY, false); // LDX Zero Page,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up value at $20 + Y ($05) = $25
        memory.borrow_mut().write(0x0025, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be loaded with 0x42
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from zero page,Y");
        assert_eq!(cycles, 4, "LDX zero page,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_b7() {
        let memory = create_test_memory();

        // Set up LAX $20,Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_ZPY, false); // LAX Zero Page,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up value at $20 + Y ($05) = $25
        memory.borrow_mut().write(0x0025, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from zero page,Y");
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from zero page,Y");
        assert_eq!(cycles, 4, "LAX zero page,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_b8() {
        let memory = create_test_memory();

        // Set up CLV instruction
        memory.borrow_mut().write(0x0400, CLV, false); // CLV opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_OVERFLOW; // Set overflow flag

        let cycles = execute_instruction(&mut cpu);

        // Overflow flag should be cleared
        assert_eq!(
            cpu.state.p & FLAG_OVERFLOW,
            0,
            "Overflow flag should be cleared"
        );
        assert_eq!(cycles, 2, "CLV should take 2 cycles");
    }

    #[test]
    fn test_opcode_b9() {
        let memory = create_test_memory();

        // Set up LDA $1234,Y instruction
        memory.borrow_mut().write(0x0400, LDA_ABSY, false); // LDA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from absolute,Y");
        assert_eq!(
            cycles, 4,
            "LDA absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_ba() {
        let memory = create_test_memory();

        // Set up TSX instruction
        memory.borrow_mut().write(0x0400, TSX, false); // TSX opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.x = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should contain SP
        assert_eq!(cpu.state.x, 0xFD, "X should contain SP");
        assert_eq!(cycles, 2, "TSX should take 2 cycles");
    }

    #[test]
    fn test_opcode_bb() {
        let memory = create_test_memory();

        // Set up LAR $1234,Y instruction (illegal opcode, also called LAS)
        memory.borrow_mut().write(0x0400, LAR_ABSY, false); // LAR Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0b1111_0000, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.sp = 0b1010_1010;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // LAR: A = X = SP = (SP AND M)
        // Result: 0b1010_1010 AND 0b1111_0000 = 0b1010_0000
        assert_eq!(cpu.state.a, 0b1010_0000, "A should contain result");
        assert_eq!(cpu.state.x, 0b1010_0000, "X should contain result");
        assert_eq!(cpu.state.sp, 0b1010_0000, "SP should contain result");
        assert_eq!(
            cycles, 4,
            "LAR absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_bc() {
        let memory = create_test_memory();

        // Set up LDY $1234,X instruction
        memory.borrow_mut().write(0x0400, LDY_ABSX, false); // LDY Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x00;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be loaded with 0x42
        assert_eq!(cpu.state.y, 0x42, "Y should be loaded from absolute,X");
        assert_eq!(
            cycles, 4,
            "LDY absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_bd() {
        let memory = create_test_memory();

        // Set up LDA $1234,X instruction
        memory.borrow_mut().write(0x0400, LDA_ABSX, false); // LDA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from absolute,X");
        assert_eq!(
            cycles, 4,
            "LDA absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_be() {
        let memory = create_test_memory();

        // Set up LDX $1234,Y instruction
        memory.borrow_mut().write(0x0400, LDX_ABSY, false); // LDX Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be loaded with 0x42
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from absolute,Y");
        assert_eq!(
            cycles, 4,
            "LDX absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_bf() {
        let memory = create_test_memory();

        // Set up LAX $1234,Y instruction (illegal opcode)
        memory.borrow_mut().write(0x0400, LAX_ABSY, false); // LAX Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x00;
        cpu.state.x = 0x00;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Both A and X should be loaded with 0x42
        assert_eq!(cpu.state.a, 0x42, "A should be loaded from absolute,Y");
        assert_eq!(cpu.state.x, 0x42, "X should be loaded from absolute,Y");
        assert_eq!(
            cycles, 4,
            "LAX absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_c0() {
        let memory = create_test_memory();

        // Set up CPY #imm instruction
        memory.borrow_mut().write(0x0400, CPY_IMM, false); // CPY Immediate opcode
        memory.borrow_mut().write(0x0401, 0x50, false); // Compare with 0x50

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y == 0x50: Z=1, C=1, N=0
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 2, "CPY immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_c1() {
        let memory = create_test_memory();

        // Set up CMP ($40,X) instruction
        memory.borrow_mut().write(0x0400, CMP_INDX, false); // CMP Indexed Indirect opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up indirect address at $40 + X ($05) = $45
        memory.borrow_mut().write(0x0045, 0x00, false); // Low byte of address
        memory.borrow_mut().write(0x0046, 0x30, false); // High byte of address

        // Set up value at $3000
        memory.borrow_mut().write(0x3000, 0x50, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A == 0x50: Z=1, C=1, N=0
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 6, "CMP indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_c2() {
        let memory = create_test_memory();

        // Set up DOP #imm instruction (illegal NOP)
        memory.borrow_mut().write(0x0400, DOP_IMM4, false); // DOP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x42, false); // Dummy operand

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(cycles, 2, "DOP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_c3() {
        let memory = create_test_memory();

        // Set up DCP ($40,X) instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_INDX, false); // DCP Indexed Indirect opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up indirect address at $40 + X ($05) = $45
        memory.borrow_mut().write(0x0045, 0x00, false); // Low byte of address
        memory.borrow_mut().write(0x0046, 0x30, false); // High byte of address

        // Set up value at $3000
        memory.borrow_mut().write(0x3000, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $3000 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x3000),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 8, "DCP indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_c4() {
        let memory = create_test_memory();

        // Set up CPY $40 instruction
        memory.borrow_mut().write(0x0400, CPY_ZP, false); // CPY Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up value at $40
        memory.borrow_mut().write(0x0040, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 3, "CPY zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_c5() {
        let memory = create_test_memory();

        // Set up CMP $40 instruction
        memory.borrow_mut().write(0x0400, CMP_ZP, false); // CMP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up value at $40
        memory.borrow_mut().write(0x0040, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 3, "CMP zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_c6() {
        let memory = create_test_memory();

        // Set up DEC $40 instruction
        memory.borrow_mut().write(0x0400, DEC_ZP, false); // DEC Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up value at $40
        memory.borrow_mut().write(0x0040, 0x01, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be decremented: 0x01 -> 0x00
        assert_eq!(
            memory.borrow().read(0x0040),
            0x00,
            "Memory should be decremented"
        );
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0010, "Z should be set");
        assert_eq!(cycles, 5, "DEC zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_c7() {
        let memory = create_test_memory();

        // Set up DCP $40 instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_ZP, false); // DCP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x40, false); // Zero page address

        // Set up value at $40
        memory.borrow_mut().write(0x0040, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $40 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x0040),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 5, "DCP zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_c8() {
        let memory = create_test_memory();

        // Set up INY instruction
        memory.borrow_mut().write(0x0400, INY, false); // INY Implied opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0xFE;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be incremented: 0xFE -> 0xFF
        assert_eq!(cpu.state.y, 0xFF, "Y should be incremented");
        assert_eq!(cpu.state.p & 0b1000_0000, 0b1000_0000, "N should be set");
        assert_eq!(cycles, 2, "INY should take 2 cycles");
    }

    #[test]
    fn test_opcode_c9() {
        let memory = create_test_memory();

        // Set up CMP #imm instruction
        memory.borrow_mut().write(0x0400, CMP_IMM, false); // CMP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x50, false); // Compare with 0x50

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A == 0x50: Z=1, C=1, N=0
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 2, "CMP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_ca() {
        let memory = create_test_memory();

        // Set up DEX instruction
        memory.borrow_mut().write(0x0400, DEX, false); // DEX Implied opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x01;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be decremented: 0x01 -> 0x00
        assert_eq!(cpu.state.x, 0x00, "X should be decremented");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0010, "Z should be set");
        assert_eq!(cycles, 2, "DEX should take 2 cycles");
    }

    #[test]
    fn test_opcode_cb() {
        let memory = create_test_memory();

        // Set up AXS #imm instruction (illegal: (A AND X) - imm -> X without borrow)
        memory.borrow_mut().write(0x0400, AXS_IMM, false); // AXS Immediate opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Subtract 0x10

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0xFF;
        cpu.state.x = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // (A AND X) - imm -> X
        // (0xFF AND 0x50) - 0x10 = 0x50 - 0x10 = 0x40
        assert_eq!(cpu.state.x, 0x40, "X should be (A AND X) - imm");
        assert_eq!(
            cpu.state.p & 0b0000_0001,
            0b0000_0001,
            "C should be set (no borrow)"
        );
        assert_eq!(cycles, 2, "AXS immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_cc() {
        let memory = create_test_memory();

        // Set up CPY $1234 instruction
        memory.borrow_mut().write(0x0400, CPY_ABS, false); // CPY Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 4, "CPY absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_cd() {
        let memory = create_test_memory();

        // Set up CMP $1234 instruction
        memory.borrow_mut().write(0x0400, CMP_ABS, false); // CMP Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 4, "CMP absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_ce() {
        let memory = create_test_memory();

        // Set up DEC $1234 instruction
        memory.borrow_mut().write(0x0400, DEC_ABS, false); // DEC Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x01, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be decremented: 0x01 -> 0x00
        assert_eq!(
            memory.borrow().read(0x1234),
            0x00,
            "Memory should be decremented"
        );
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0010, "Z should be set");
        assert_eq!(cycles, 6, "DEC absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_cf() {
        let memory = create_test_memory();

        // Set up DCP $1234 instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_ABS, false); // DCP Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x1234),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 6, "DCP absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_d0() {
        let memory = create_test_memory();

        // Set up BNE $10 instruction (branch if not equal)
        memory.borrow_mut().write(0x0400, BNE, false); // BNE opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0x00; // Z flag clear (not equal)

        let cycles = execute_instruction(&mut cpu);

        // PC should branch to $0412 (0x0402 + 0x10)
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");
        assert_eq!(cycles, 3, "BNE taken (same page) should take 3 cycles");
    }

    #[test]
    fn test_opcode_d1() {
        let memory = create_test_memory();

        // Set up CMP ($20),Y instruction
        memory.borrow_mut().write(0x0400, CMP_INDY, false); // CMP Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 5, "CMP (Indirect),Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_d2() {
        let memory = create_test_memory();

        // Set up KIL instruction (illegal opcode that halts CPU)
        memory.borrow_mut().write(0x0400, KIL11, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        // CPU should be halted
        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_d3() {
        let memory = create_test_memory();

        // Set up DCP ($20),Y instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_INDY, false); // DCP Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x04;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x1234),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 8, "DCP (Indirect),Y should take 8 cycles");
    }

    #[test]
    fn test_opcode_d4() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction (illegal NOP)
        memory.borrow_mut().write(0x0400, DOP_ZPX5, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x42, false); // Value at $25 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_d5() {
        let memory = create_test_memory();

        // Set up CMP $20,X instruction
        memory.borrow_mut().write(0x0400, CMP_ZPX, false); // CMP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x30, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 4, "CMP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_d6() {
        let memory = create_test_memory();

        // Set up DEC $20,X instruction
        memory.borrow_mut().write(0x0400, DEC_ZPX, false); // DEC Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x01, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be decremented: 0x01 -> 0x00
        assert_eq!(
            memory.borrow().read(0x0025),
            0x00,
            "Memory should be decremented"
        );
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0010, "Z should be set");
        assert_eq!(cycles, 6, "DEC zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_d7() {
        let memory = create_test_memory();

        // Set up DCP $20,X instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_ZPX, false); // DCP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x51, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $25 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x0025),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 6, "DCP zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_d8() {
        let memory = create_test_memory();

        // Set up CLD instruction
        memory.borrow_mut().write(0x0400, CLD, false); // CLD opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0xFF; // All flags set

        let cycles = execute_instruction(&mut cpu);

        // Decimal flag should be cleared
        assert_eq!(
            cpu.state.p & 0b0000_1000,
            0b0000_0000,
            "D flag should be clear"
        );
        assert_eq!(cycles, 2, "CLD should take 2 cycles");
    }

    #[test]
    fn test_opcode_d9() {
        let memory = create_test_memory();

        // Set up CMP $1234,Y instruction
        memory.borrow_mut().write(0x0400, CMP_ABSY, false); // CMP Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(
            cycles, 4,
            "CMP absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_da() {
        let memory = create_test_memory();

        // Set up NOP instruction (implied)
        memory.borrow_mut().write(0x0400, NOP_IMP5, false); // NOP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x55;
        cpu.state.y = 0x66;
        cpu.state.p = 0x24;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x42, "A should not change");
        assert_eq!(cpu.state.x, 0x55, "X should not change");
        assert_eq!(cpu.state.y, 0x66, "Y should not change");
        assert_eq!(cpu.state.p, 0x24, "P should not change");
        assert_eq!(cycles, 2, "NOP should take 2 cycles");
    }

    #[test]
    fn test_opcode_db() {
        let memory = create_test_memory();

        // Set up DCP $1234,Y instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_ABSY, false); // DCP Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1239 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x1239),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 7, "DCP absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_dc() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction (illegal NOP that reads absolute,X)
        memory.borrow_mut().write(0x0400, TOP_ABSX5, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1239, 0x42, false); // Value at $1239 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // TOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(
            cycles, 4,
            "TOP absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_dd() {
        let memory = create_test_memory();

        // Set up CMP $1234,X instruction
        memory.borrow_mut().write(0x0400, CMP_ABSX, false); // CMP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A (0x50) > memory (0x30): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(
            cycles, 4,
            "CMP absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_de() {
        let memory = create_test_memory();

        // Set up DEC $1234,X instruction
        memory.borrow_mut().write(0x0400, DEC_ABSX, false); // DEC Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x01, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be decremented: 0x01 -> 0x00
        assert_eq!(
            memory.borrow().read(0x1239),
            0x00,
            "Memory should be decremented"
        );
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0010, "Z should be set");
        assert_eq!(cycles, 7, "DEC absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_df() {
        let memory = create_test_memory();

        // Set up DCP $1234,X instruction (illegal: DEC then CMP)
        memory.borrow_mut().write(0x0400, DCP_ABSX, false); // DCP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x51, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1239 should be decremented: 0x51 -> 0x50
        // Then compared with A (0x50): Z=1, C=1, N=0
        assert_eq!(
            memory.borrow().read(0x1239),
            0x50,
            "Memory should be decremented"
        );
        assert_eq!(
            cpu.state.p & 0b0000_0011,
            0b0000_0011,
            "Z and C should be set"
        );
        assert_eq!(cycles, 7, "DCP absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_e0() {
        let memory = create_test_memory();

        // Set up CPX #$50 instruction
        memory.borrow_mut().write(0x0400, CPX_IMM, false); // CPX Immediate opcode
        memory.borrow_mut().write(0x0401, 0x50, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x60;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X (0x60) > immediate (0x50): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 2, "CPX immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_e1() {
        let memory = create_test_memory();

        // Set up SBC ($20,X) instruction
        memory.borrow_mut().write(0x0400, SBC_INDX, false); // SBC Indexed Indirect opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $25 ($20 + X offset $05) pointing to $1234
        memory.borrow_mut().write(0x0025, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0026, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 6, "SBC indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_e2() {
        let memory = create_test_memory();

        // Set up DOP #$42 instruction (illegal NOP)
        memory.borrow_mut().write(0x0400, DOP_IMM5, false); // DOP Immediate opcode
        memory.borrow_mut().write(0x0401, 0x42, false); // Immediate value (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(cycles, 2, "DOP immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_e3() {
        let memory = create_test_memory();

        // Set up ISB ($20,X) instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_INDX, false); // ISB Indexed Indirect opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at $25 ($20 + X offset $05) pointing to $1234
        memory.borrow_mut().write(0x0025, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0026, 0x12, false); // High byte

        // Set up value at $1234
        memory.borrow_mut().write(0x1234, 0x2F, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x1234),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 8, "ISB indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_e4() {
        let memory = create_test_memory();

        // Set up CPX $20 instruction
        memory.borrow_mut().write(0x0400, CPX_ZP, false); // CPX Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address
        memory.borrow_mut().write(0x0020, 0x50, false); // Value at $20

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x60;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X (0x60) > memory (0x50): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 3, "CPX zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_e5() {
        let memory = create_test_memory();

        // Set up SBC $20 instruction
        memory.borrow_mut().write(0x0400, SBC_ZP, false); // SBC Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address
        memory.borrow_mut().write(0x0020, 0x30, false); // Value at $20

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 3, "SBC zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_e6() {
        let memory = create_test_memory();

        // Set up INC $20 instruction
        memory.borrow_mut().write(0x0400, INC_ZP, false); // INC Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address
        memory.borrow_mut().write(0x0020, 0xFF, false); // Value at $20

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be incremented: 0xFF -> 0x00
        assert_eq!(
            memory.borrow().read(0x0020),
            0x00,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Z should be set");
        assert_eq!(cycles, 5, "INC zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_e7() {
        let memory = create_test_memory();

        // Set up ISB $20 instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_ZP, false); // ISB Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address
        memory.borrow_mut().write(0x0020, 0x2F, false); // Value at $20

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $20 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x0020),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 5, "ISB zero page should take 5 cycles");
    }

    #[test]
    fn test_opcode_e8() {
        let memory = create_test_memory();

        // Set up INX instruction
        memory.borrow_mut().write(0x0400, INX, false); // INX opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0xFF;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X should be incremented: 0xFF -> 0x00
        assert_eq!(cpu.state.x, 0x00, "X should be 0x00");
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Z should be set");
        assert_eq!(cycles, 2, "INX should take 2 cycles");
    }

    #[test]
    fn test_opcode_e9() {
        let memory = create_test_memory();

        // Set up SBC #$30 instruction
        memory.borrow_mut().write(0x0400, SBC_IMM, false); // SBC Immediate opcode
        memory.borrow_mut().write(0x0401, 0x30, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 2, "SBC immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_ea() {
        let memory = create_test_memory();

        // Set up NOP instruction
        memory.borrow_mut().write(0x0400, NOP, false); // NOP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x55;
        cpu.state.y = 0x66;
        cpu.state.p = 0x24;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x42, "A should not change");
        assert_eq!(cpu.state.x, 0x55, "X should not change");
        assert_eq!(cpu.state.y, 0x66, "Y should not change");
        assert_eq!(cpu.state.p, 0x24, "P should not change");
        assert_eq!(cycles, 2, "NOP should take 2 cycles");
    }

    #[test]
    fn test_opcode_eb() {
        let memory = create_test_memory();

        // Set up SBC #$30 instruction (illegal duplicate of SBC immediate)
        memory.borrow_mut().write(0x0400, SBC_IMM2, false); // SBC Immediate opcode (illegal)
        memory.borrow_mut().write(0x0401, 0x30, false); // Immediate value

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 2, "SBC immediate should take 2 cycles");
    }

    #[test]
    fn test_opcode_ec() {
        let memory = create_test_memory();

        // Set up CPX $1234 instruction
        memory.borrow_mut().write(0x0400, CPX_ABS, false); // CPX Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1234, 0x50, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x60;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // X (0x60) > memory (0x50): Z=0, C=1, N=0
        assert_eq!(cpu.state.p & 0b0000_0001, 0b0000_0001, "C should be set");
        assert_eq!(cpu.state.p & 0b0000_0010, 0b0000_0000, "Z should be clear");
        assert_eq!(cycles, 4, "CPX absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_ed() {
        let memory = create_test_memory();

        // Set up SBC $1234 instruction
        memory.borrow_mut().write(0x0400, SBC_ABS, false); // SBC Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1234, 0x30, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 4, "SBC absolute should take 4 cycles");
    }

    #[test]
    fn test_opcode_ee() {
        let memory = create_test_memory();

        // Set up INC $1234 instruction
        memory.borrow_mut().write(0x0400, INC_ABS, false); // INC Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1234, 0xFF, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be incremented: 0xFF -> 0x00
        assert_eq!(
            memory.borrow().read(0x1234),
            0x00,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Z should be set");
        assert_eq!(cycles, 6, "INC absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_ef() {
        let memory = create_test_memory();

        // Set up ISB $1234 instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_ABS, false); // ISB Absolute opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1234, 0x2F, false); // Value at $1234

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x1234),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 6, "ISB absolute should take 6 cycles");
    }

    #[test]
    fn test_opcode_f0() {
        let memory = create_test_memory();

        // Set up BEQ $10 instruction (branch if equal)
        memory.borrow_mut().write(0x0400, BEQ, false); // BEQ opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = FLAG_ZERO; // Z flag set (equal)

        let cycles = execute_instruction(&mut cpu);

        // PC should branch to $0412 (0x0402 + 0x10)
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");
        assert_eq!(cycles, 3, "BEQ taken (same page) should take 3 cycles");
    }

    #[test]
    fn test_opcode_f1() {
        let memory = create_test_memory();

        // Set up SBC ($20),Y instruction
        memory.borrow_mut().write(0x0400, SBC_INDY, false); // SBC Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 5, "SBC (Indirect),Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_f2() {
        let memory = create_test_memory();

        // Set up KIL instruction (illegal opcode that halts CPU)
        memory.borrow_mut().write(0x0400, KIL12, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        // CPU should be halted
        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_f3() {
        let memory = create_test_memory();

        // Set up ISB ($20),Y instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_INDY, false); // ISB Indirect,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        // Set up value at $1234 (base $1230 + Y offset $04)
        memory.borrow_mut().write(0x1234, 0x2F, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x04;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x1234),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 8, "ISB (Indirect),Y should take 8 cycles");
    }

    #[test]
    fn test_opcode_f4() {
        let memory = create_test_memory();

        // Set up DOP $20,X instruction (illegal NOP)
        memory.borrow_mut().write(0x0400, DOP_ZPX6, false); // DOP Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x42, false); // Value at $25 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // DOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(cycles, 4, "DOP zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_f5() {
        let memory = create_test_memory();

        // Set up SBC $20,X instruction
        memory.borrow_mut().write(0x0400, SBC_ZPX, false); // SBC Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x30, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(cycles, 4, "SBC zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_f6() {
        let memory = create_test_memory();

        // Set up INC $20,X instruction
        memory.borrow_mut().write(0x0400, INC_ZPX, false); // INC Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0xFF, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be incremented: 0xFF -> 0x00
        assert_eq!(
            memory.borrow().read(0x0025),
            0x00,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Z should be set");
        assert_eq!(cycles, 6, "INC zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_f7() {
        let memory = create_test_memory();

        // Set up ISB $20,X instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_ZPX, false); // ISB Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address
        memory.borrow_mut().write(0x0025, 0x2F, false); // Value at $25 (base $20 + X offset $05)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $25 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x0025),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 6, "ISB zero page,X should take 6 cycles");
    }

    #[test]
    fn test_opcode_f8() {
        let memory = create_test_memory();

        // Set up SED instruction
        memory.borrow_mut().write(0x0400, SED, false); // SED opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0x00; // All flags clear

        let cycles = execute_instruction(&mut cpu);

        // Decimal flag should be set
        assert_eq!(
            cpu.state.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should be set"
        );
        assert_eq!(cycles, 2, "SED should take 2 cycles");
    }

    #[test]
    fn test_opcode_f9() {
        let memory = create_test_memory();

        // Set up SBC $1234,Y instruction
        memory.borrow_mut().write(0x0400, SBC_ABSY, false); // SBC Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(
            cycles, 4,
            "SBC absolute,Y should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_fa() {
        let memory = create_test_memory();

        // Set up NOP instruction (implied)
        memory.borrow_mut().write(0x0400, NOP_IMP6, false); // NOP opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x55;
        cpu.state.y = 0x66;
        cpu.state.p = 0x24;

        let cycles = execute_instruction(&mut cpu);

        // NOP does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x42, "A should not change");
        assert_eq!(cpu.state.x, 0x55, "X should not change");
        assert_eq!(cpu.state.y, 0x66, "Y should not change");
        assert_eq!(cpu.state.p, 0x24, "P should not change");
        assert_eq!(cycles, 2, "NOP should take 2 cycles");
    }

    #[test]
    fn test_opcode_fb() {
        let memory = create_test_memory();

        // Set up ISB $1234,Y instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_ABSY, false); // ISB Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + Y ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x2F, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.y = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1239 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x1239),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 7, "ISB absolute,Y should take 7 cycles");
    }

    #[test]
    fn test_opcode_fc() {
        let memory = create_test_memory();

        // Set up TOP $1234,X instruction (illegal NOP that reads absolute,X)
        memory.borrow_mut().write(0x0400, TOP_ABSX6, false); // TOP Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte
        memory.borrow_mut().write(0x1239, 0x42, false); // Value at $1239 (will be read but ignored)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.a = 0x99;
        cpu.state.p = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // TOP does nothing
        assert_eq!(cpu.state.a, 0x99, "A should be unchanged");
        assert_eq!(cpu.state.p, 0xFF, "P should be unchanged");
        assert_eq!(
            cycles, 4,
            "TOP absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_fd() {
        let memory = create_test_memory();

        // Set up SBC $1234,X instruction
        memory.borrow_mut().write(0x0400, SBC_ABSX, false); // SBC Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x30, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cpu.state.p & FLAG_CARRY, FLAG_CARRY, "C should be set");
        assert_eq!(
            cycles, 4,
            "SBC absolute,X should take 4 cycles (no page cross)"
        );
    }

    #[test]
    fn test_opcode_fe() {
        let memory = create_test_memory();

        // Set up INC $1234,X instruction
        memory.borrow_mut().write(0x0400, INC_ABSX, false); // INC Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0xFF, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x05;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory should be incremented: 0xFF -> 0x00
        assert_eq!(
            memory.borrow().read(0x1239),
            0x00,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.p & FLAG_ZERO, FLAG_ZERO, "Z should be set");
        assert_eq!(cycles, 7, "INC absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_ff() {
        let memory = create_test_memory();

        // Set up ISB $1234,X instruction (illegal: INC then SBC)
        memory.borrow_mut().write(0x0400, ISB_ABSX, false); // ISB Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        // Set up value at $1234 + X ($05) = $1239
        memory.borrow_mut().write(0x1239, 0x2F, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x50;
        cpu.state.x = 0x05;
        cpu.state.p = FLAG_CARRY; // Set carry for SBC

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1239 should be incremented: 0x2F -> 0x30
        // Then SBC: A = 0x50 - 0x30 - (1 - C) = 0x50 - 0x30 - 0 = 0x20
        assert_eq!(
            memory.borrow().read(0x1239),
            0x30,
            "Memory should be incremented"
        );
        assert_eq!(cpu.state.a, 0x20, "A should be 0x20");
        assert_eq!(cycles, 7, "ISB absolute,X should take 7 cycles");
    }

    #[test]
    fn test_opcode_90() {
        let memory = create_test_memory();

        // Set up BCC instruction (Branch if Carry Clear)
        memory.borrow_mut().write(0x0400, BCC, false); // BCC opcode
        memory.borrow_mut().write(0x0401, 0x10, false); // Relative offset (+16)

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.p = 0; // C flag clear

        let cycles = execute_instruction(&mut cpu);

        // Branch should be taken to 0x0412 (0x0402 + 0x10)
        assert_eq!(cpu.state.pc, 0x0412, "PC should branch to 0x0412");
        assert_eq!(cycles, 3, "BCC taken (same page) should take 3 cycles");
    }

    #[test]
    fn test_opcode_91() {
        let memory = create_test_memory();

        // Set up STA ($20),Y instruction
        memory.borrow_mut().write(0x0400, STA_INDY, false); // STA (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.y = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $1234 (base $1230 + Y offset $04)
        assert_eq!(memory.borrow().read(0x1234), 0x42, "A should be stored");
        assert_eq!(cycles, 6, "STA (Indirect),Y should take 6 cycles");
    }

    #[test]
    fn test_opcode_92() {
        let memory = create_test_memory();

        // Set up KIL instruction (illegal opcode that halts CPU)
        memory.borrow_mut().write(0x0400, KIL9, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;

        let cycles = execute_instruction(&mut cpu);

        // CPU should be halted
        assert!(cpu.is_halted(), "CPU should be halted");
        assert_eq!(cycles, 1, "KIL should take 1 cycle");
    }

    #[test]
    fn test_opcode_93() {
        let memory = create_test_memory();

        // Set up AXA ($20),Y instruction (illegal: stores A & X & (H+1))
        memory.borrow_mut().write(0x0400, AXA_INDY, false); // AXA (Indirect),Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up pointer at $20-$21 to point to $1230
        memory.borrow_mut().write(0x0020, 0x30, false); // Low byte
        memory.borrow_mut().write(0x0021, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0xFF;
        cpu.state.x = 0xFF;
        cpu.state.y = 0x04;

        let cycles = execute_instruction(&mut cpu);

        // AXA stores A & X & (high_byte + 1)
        // Target is $1234, high byte is 0x12, so stores 0xFF & 0xFF & 0x13 = 0x13
        assert_eq!(
            memory.borrow().read(0x1234),
            0x13,
            "AXA result should be stored"
        );
        assert_eq!(cycles, 6, "AXA (Indirect),Y should take 6 cycles");
    }

    #[test]
    fn test_opcode_94() {
        let memory = create_test_memory();

        // Set up STY $20,X instruction
        memory.borrow_mut().write(0x0400, STY_ZPX, false); // STY Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x42;
        cpu.state.x = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // Y should be stored at $25 (base $20 + X offset $05)
        assert_eq!(memory.borrow().read(0x0025), 0x42, "Y should be stored");
        assert_eq!(cycles, 4, "STY zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_95() {
        let memory = create_test_memory();

        // Set up STA $20,X instruction
        memory.borrow_mut().write(0x0400, STA_ZPX, false); // STA Zero Page,X opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $25 (base $20 + X offset $05)
        assert_eq!(memory.borrow().read(0x0025), 0x42, "A should be stored");
        assert_eq!(cycles, 4, "STA zero page,X should take 4 cycles");
    }

    #[test]
    fn test_opcode_96() {
        let memory = create_test_memory();

        // Set up STX $20,Y instruction
        memory.borrow_mut().write(0x0400, STX_ZPY, false); // STX Zero Page,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x42;
        cpu.state.y = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // X should be stored at $25 (base $20 + Y offset $05)
        assert_eq!(memory.borrow().read(0x0025), 0x42, "X should be stored");
        assert_eq!(cycles, 4, "STX zero page,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_97() {
        let memory = create_test_memory();

        // Set up SAX $20,Y instruction (illegal: stores A & X)
        memory.borrow_mut().write(0x0400, SAX_ZPY, false); // SAX Zero Page,Y opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b11110000;
        cpu.state.x = 0b10101010;
        cpu.state.y = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // A & X = 0b10100000 = 0xA0 should be stored at $25
        assert_eq!(memory.borrow().read(0x0025), 0xA0, "A & X should be stored");
        assert_eq!(cycles, 4, "SAX zero page,Y should take 4 cycles");
    }

    #[test]
    fn test_opcode_98() {
        let memory = create_test_memory();

        // Set up TYA instruction (Transfer Y to A)
        memory.borrow_mut().write(0x0400, TYA, false); // TYA opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0x42;
        cpu.state.a = 0x00;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Y should be transferred to A
        assert_eq!(cpu.state.a, 0x42, "A should equal Y");
        assert_eq!(cpu.state.p & FLAG_NEGATIVE, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & FLAG_ZERO, 0, "Z flag should be clear");
        assert_eq!(cycles, 2, "TYA should take 2 cycles");
    }

    #[test]
    fn test_opcode_99() {
        let memory = create_test_memory();

        // Set up STA $1234,Y instruction
        memory.borrow_mut().write(0x0400, STA_ABSY, false); // STA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.y = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $1239 (base $1234 + Y offset $05)
        assert_eq!(memory.borrow().read(0x1239), 0x42, "A should be stored");
        assert_eq!(cycles, 5, "STA absolute,Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_9a() {
        let memory = create_test_memory();

        // Set up TXS instruction (Transfer X to SP)
        memory.borrow_mut().write(0x0400, TXS, false); // TXS opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x42;
        cpu.state.sp = 0xFF;

        let cycles = execute_instruction(&mut cpu);

        // X should be transferred to SP
        assert_eq!(cpu.state.sp, 0x42, "SP should equal X");
        assert_eq!(cycles, 2, "TXS should take 2 cycles");
    }

    #[test]
    fn test_opcode_9b() {
        let memory = create_test_memory();

        // Set up XAS $1234,Y instruction (illegal: SP = A & X, stores SP & (H+1))
        memory.borrow_mut().write(0x0400, XAS_ABSY, false); // XAS Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0xFF;
        cpu.state.x = 0xFF;
        cpu.state.y = 0x05;
        cpu.state.sp = 0x00;

        let cycles = execute_instruction(&mut cpu);

        // SP should be set to A & X = 0xFF
        assert_eq!(cpu.state.sp, 0xFF, "SP should be A & X");
        // Stores SP & (high_byte + 1) = 0xFF & 0x13 = 0x13 at $1239
        assert_eq!(
            memory.borrow().read(0x1239),
            0x13,
            "XAS result should be stored"
        );
        assert_eq!(cycles, 5, "XAS absolute,Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_9c() {
        let memory = create_test_memory();

        // Set up SYA $1234,X instruction (illegal: stores Y & (H+1))
        memory.borrow_mut().write(0x0400, SYA_ABSX, false); // SYA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.y = 0xFF;
        cpu.state.x = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // Stores Y & (high_byte + 1) = 0xFF & 0x13 = 0x13 at $1239
        assert_eq!(
            memory.borrow().read(0x1239),
            0x13,
            "SYA result should be stored"
        );
        assert_eq!(cycles, 5, "SYA absolute,X should take 5 cycles");
    }

    #[test]
    fn test_opcode_9d() {
        let memory = create_test_memory();

        // Set up STA $1234,X instruction
        memory.borrow_mut().write(0x0400, STA_ABSX, false); // STA Absolute,X opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0x42;
        cpu.state.x = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // A should be stored at $1239 (base $1234 + X offset $05)
        assert_eq!(memory.borrow().read(0x1239), 0x42, "A should be stored");
        assert_eq!(cycles, 5, "STA absolute,X should take 5 cycles");
    }

    #[test]
    fn test_opcode_9e() {
        let memory = create_test_memory();

        // Set up SXA $1234,Y instruction (illegal: stores X & (H+1))
        memory.borrow_mut().write(0x0400, SXA_ABSY, false); // SXA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0xFF;
        cpu.state.y = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // Stores X & (high_byte + 1) = 0xFF & 0x13 = 0x13 at $1239
        assert_eq!(
            memory.borrow().read(0x1239),
            0x13,
            "SXA result should be stored"
        );
        assert_eq!(cycles, 5, "SXA absolute,Y should take 5 cycles");
    }

    #[test]
    fn test_opcode_9f() {
        let memory = create_test_memory();

        // Set up AXA $1234,Y instruction (illegal: stores A & X & (H+1))
        memory.borrow_mut().write(0x0400, AXA_ABSY, false); // AXA Absolute,Y opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0xFF;
        cpu.state.x = 0xFF;
        cpu.state.y = 0x05;

        let cycles = execute_instruction(&mut cpu);

        // Stores A & X & (high_byte + 1) = 0xFF & 0xFF & 0x13 = 0x13 at $1239
        assert_eq!(
            memory.borrow().read(0x1239),
            0x13,
            "AXA result should be stored"
        );
        assert_eq!(cycles, 5, "AXA absolute,Y should take 5 cycles");
    }
}
