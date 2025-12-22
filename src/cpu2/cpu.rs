use super::addressing::{
    Absolute, AbsoluteX, AbsoluteY, Immediate, Implied, IndexedIndirect, Indirect, IndirectIndexed,
    Relative, ZeroPage, ZeroPageX,
};
use super::instruction::Instruction;
use super::instruction_types::{
    Aac, Adc, And, Arr, Asl, AslA, Asr, Bit, Bmi, Bpl, Brk, Bvc, Bvs, Clc, Cli, Dop, Eor, Jmp, Jsr,
    Kil, Lsr, LsrA, Nop, Ora, Pha, Php, Pla, Plp, Rla, Rol, RolA, Ror, RorA, Rra, Rti, Rts, Sec,
    Sei, Slo, Sre, Top,
};
use super::traits::{
    AAC_IMM, AAC_IMM2, ADC_ABS, ADC_ABSX, ADC_ABSY, ADC_IMM, ADC_INDX, ADC_INDY, ADC_ZP, ADC_ZPX,
    AND_ABS, AND_ABSX, AND_ABSY, AND_IMM, AND_INDX, AND_INDY, AND_ZP, AND_ZPX, ARR_IMM, ASL_A,
    ASL_ABS, ASL_ABSX, ASL_ZP, ASL_ZPX, ASR_IMM, BIT_ABS, BIT_ZP, BMI, BPL, BRK, BVC, BVS, CLC,
    CLI, DOP_ZP, DOP_ZP2, DOP_ZP3, DOP_ZPX, DOP_ZPX2, DOP_ZPX3, DOP_ZPX4, EOR_ABS, EOR_ABSX,
    EOR_ABSY, EOR_IMM, EOR_INDX, EOR_INDY, EOR_ZP, EOR_ZPX, JMP_ABS, JMP_IND, JSR, KIL, KIL2, KIL3,
    KIL4, KIL5, KIL6, KIL7, KIL8, KIL9, KIL10, KIL11, KIL12, LSR_ABS, LSR_ABSX, LSR_ACC, LSR_ZP,
    LSR_ZPX, NOP_IMP, NOP_IMP2, NOP_IMP3, NOP_IMP4, ORA_ABS, ORA_ABSX, ORA_ABSY, ORA_IMM, ORA_INDX,
    ORA_INDY, ORA_ZP, ORA_ZPX, PHA, PHP, PLA, PLP, RLA_ABS, RLA_ABSX, RLA_ABSY, RLA_INDX, RLA_INDY,
    RLA_ZP, RLA_ZPX, ROL_ABS, ROL_ABSX, ROL_ACC, ROL_ZP, ROL_ZPX, ROR_ABS, ROR_ABSX, ROR_ACC,
    ROR_ZP, ROR_ZPX, RRA_ABS, RRA_ABSX, RRA_ABSY, RRA_INDX, RRA_INDY, RRA_ZP, RRA_ZPX, RTI, RTS,
    SEC, SEI, SLO_ABS, SLO_ABSX, SLO_ABSY, SLO_INDX, SLO_INDY, SLO_ZP, SLO_ZPX, SRE_ABS, SRE_ABSX,
    SRE_ABSY, SRE_INDX, SRE_INDY, SRE_ZP, SRE_ZPX, TOP_ABS, TOP_ABSX, TOP_ABSX2, TOP_ABSX3,
    TOP_ABSX4,
};
use super::types::{
    FLAG_BREAK, FLAG_CARRY, FLAG_DECIMAL, FLAG_INTERRUPT, FLAG_NEGATIVE, FLAG_OVERFLOW,
    FLAG_UNUSED, FLAG_ZERO, IRQ_VECTOR, NMI_VECTOR, RESET_VECTOR,
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
            },
            memory,
            halted: false,
            total_cycles: 0,
            current_instruction: None,
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(Absolute::new(true)),
                    Box::new(Top::new()),
                ))
            }
            ORA_ABS => {
                // ORA Absolute: ORA abs
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ABS => {
                // ASL Absolute: ASL abs
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ABS => {
                // SLO Absolute: SLO abs (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
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
                    Box::new(IndirectIndexed::new(false)),
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
                    Box::new(IndirectIndexed::new(true)),
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
                    Box::new(AbsoluteY::new(false)),
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
                    Box::new(AbsoluteY::new(true)),
                    Box::new(Slo::new()),
                ))
            }
            TOP_ABSX => {
                // TOP Absolute,X: TOP abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Top::new()),
                ))
            }
            ORA_ABSX => {
                // ORA Absolute,X: ORA abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Ora::new()),
                ))
            }
            ASL_ABSX => {
                // ASL Absolute,X: ASL abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Asl::new()),
                ))
            }
            SLO_ABSX => {
                // SLO Absolute,X: SLO abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(Absolute::new(true)),
                    Box::new(Bit::new()),
                ))
            }
            AND_ABS => {
                // AND Absolute: AND abs
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(And::new()),
                ))
            }
            ROL_ABS => {
                // ROL Absolute: ROL abs
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ABS => {
                // RLA Absolute: RLA abs (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
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
                    Box::new(IndirectIndexed::new(false)),
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
                    Box::new(IndirectIndexed::new(true)),
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
                    Box::new(AbsoluteY::new(false)),
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
                    Box::new(AbsoluteY::new(true)),
                    Box::new(Rla::new()),
                ))
            }
            TOP_ABSX2 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Top::new()),
                ))
            }
            AND_ABSX => {
                // AND Absolute,X: AND abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(And::new()),
                ))
            }
            ROL_ABSX => {
                // ROL Absolute,X: ROL abs,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Rol::new()),
                ))
            }
            RLA_ABSX => {
                // RLA Absolute,X: RLA abs,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(Absolute::new(false)),
                    Box::new(Jmp::new()),
                ))
            }
            EOR_ABS => {
                // EOR Absolute: EOR $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ABS => {
                // LSR Absolute: LSR $nnnn
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ABS => {
                // SRE Absolute: SRE $nnnn (illegal opcode)
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(IndexedIndirect::new()),
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
                    Box::new(Absolute::new(true)),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ABS => {
                // ROR: Rotate Right (Absolute)
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ABS => {
                // RRA: Rotate Right then Add with Carry (Absolute, illegal)
                Some(Instruction::new(
                    Box::new(Absolute::new(true)),
                    Box::new(Rra::new()),
                ))
            }
            EOR_INDY => {
                // EOR (Indirect),Y: EOR ($nn),Y
                Some(Instruction::new(
                    Box::new(IndirectIndexed::new(false)),
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
                    Box::new(IndirectIndexed::new(true)),
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
                    Box::new(IndirectIndexed::new(false)),
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
                    Box::new(IndirectIndexed::new(true)),
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
                    Box::new(AbsoluteY::new(false)),
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
                    Box::new(AbsoluteY::new(true)),
                    Box::new(Rra::new()),
                ))
            }
            TOP_ABSX4 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Top::new()),
                ))
            }
            ADC_ABSX => {
                // ADC Absolute,X: ADC $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Adc::new()),
                ))
            }
            ROR_ABSX => {
                // ROR Absolute,X: ROR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Ror::new()),
                ))
            }
            RRA_ABSX => {
                // RRA Absolute,X: RRA $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Rra::new()),
                ))
            }
            EOR_ABSY => {
                // EOR Absolute,Y: EOR $nnnn,Y
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(false)),
                    Box::new(Eor::new()),
                ))
            }
            NOP_IMP3 => {
                // NOP Implied (illegal opcode)
                Some(Instruction::new(Box::new(Implied), Box::new(Nop::new())))
            }
            SRE_ABSY => {
                // SRE Absolute,Y: SRE $nnnn,Y (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteY::new(true)),
                    Box::new(Sre::new()),
                ))
            }
            TOP_ABSX3 => {
                // TOP Absolute,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Top::new()),
                ))
            }
            EOR_ABSX => {
                // EOR Absolute,X: EOR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(false)),
                    Box::new(Eor::new()),
                ))
            }
            LSR_ABSX => {
                // LSR Absolute,X: LSR $nnnn,X
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Lsr::new()),
                ))
            }
            SRE_ABSX => {
                // SRE Absolute,X: SRE $nnnn,X (illegal opcode)
                Some(Instruction::new(
                    Box::new(AbsoluteX::new(true)),
                    Box::new(Sre::new()),
                ))
            }
            JMP_IND => {
                // JMP Indirect uses the Indirect addressing mode to resolve the target address
                Some(Instruction::new(
                    Box::new(Indirect::new()),
                    Box::new(Jmp::new()),
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

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        // Set I flag (bit 2)
        self.state.p |= FLAG_INTERRUPT;

        // Subtract 3 from SP (wrapping if necessary)
        self.state.sp = self.state.sp.wrapping_sub(3);

        // Clear cycle-accurate instruction state
        self.halted = false;
        // self.delayed_i_flag = None;
        self.current_instruction = None;
        // self.cycle_in_instruction = 0;

        // Read reset vector and set PC
        self.state.pc = self.read_reset_vector();

        // Reset takes 7 cycles
        self.total_cycles = 7;
    }

    /// Trigger an NMI (Non-Maskable Interrupt)
    /// Returns the number of cycles consumed (7 cycles)
    pub fn trigger_nmi(&mut self) -> u8 {
        // TODO Implement NMI logic
        // // Push PC and P onto stack
        // self.push_word(self.state.pc);
        // let mut p_with_break = self.state.p & !FLAG_BREAK; // Clear Break flag
        // p_with_break |= FLAG_UNUSED; // Set unused flag
        // self.push_byte(p_with_break);

        // // Set PC to NMI vector
        // self.state.pc = self.memory.borrow().read_u16(NMI_VECTOR);

        // // Set Interrupt Disable flag
        // self.state.p |= FLAG_INTERRUPT;

        // // NMI takes 7 CPU cycles
        // self.total_cycles += 7;
        7
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&self) -> u16 {
        self.memory.borrow().read_u16(RESET_VECTOR)
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.state.sp as u16);
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
        // TODO implement NMI pending logic
    }

    /// Check if an NMI is pending
    pub fn is_nmi_pending(&self) -> bool {
        // TODO implement NMI pending logic
        false
    }

    pub fn should_poll_irq(&self) -> bool {
        // TODO implement IRQ polling logic
        false
    }

    pub fn trigger_irq(&mut self) -> u8 {
        // TODO Implement IRQ logic
        7
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(
            cpu.state.p & 0x01,
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        cpu.state.p = 0x80; // N flag set (negative)

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x01, 0, "Carry flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x40, 0x40, "V flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let result = memory.borrow().read(0x0020);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x0020);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // A should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should be rotated left");

        // Flags: N=1, Z=0, C=0 (bit 7 of original was 0)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0x01, "C flag should be set");

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x40, 0x40, "V flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let result = memory.borrow().read(0x1234);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");

        // Flags: N=1, Z=0, C=0
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left: 0b0101_0101 ROL with C=1 = 0b1010_1011
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");

        // A should be ANDed with result: 0b1111_1111 & 0b1010_1011 = 0b1010_1011
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");

        // Flags: N=1, Z=0, C=0
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

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
        cpu.state.p = 0x80; // N flag set (negative)

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let mem_result = memory.borrow().read(0x1234);
        assert_eq!(mem_result, 0b1010_1011, "Memory should be rotated left");
        // A should be ANDed with result
        assert_eq!(cpu.state.a, 0b1010_1011, "A should contain AND result");
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let result = memory.borrow().read(0x0025);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        cpu.state.p = 0x01; // Set carry flag

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
        assert_eq!(cpu.state.p & 0x01, 0x01, "Carry flag should be set");
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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        cpu.state.p = 0x01; // Set carry flag

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
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        cpu.state.p = 0x01; // Set carry flag

        let cycles = execute_instruction(&mut cpu);

        // Memory should be rotated left
        let result = memory.borrow().read(0x1238);
        assert_eq!(result, 0b1010_1011, "Memory should be rotated left");
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
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
        cpu.state.p = 0x01; // Set carry flag

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
        assert_eq!(cpu.state.p & 0x80, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
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
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0, "N flag should be clear");
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
        assert_eq!(cpu.state.p & 0x80, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 1, "C flag should be set (bit 0 was 1)");
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
        assert_eq!(cpu.state.p & 0x80, 0, "N flag should be clear");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 1, "C flag should be set (bit 0 was 1)");
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
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");
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
        assert_eq!(cpu.state.p & 0x01, 1, "C flag should be set");
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
        assert_eq!(cpu.state.p & 0x01, 1, "C flag should be set (bit 0 was 1)");
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
        assert_eq!(cpu.state.p & 0x01, 1, "C flag should be set");
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

}
