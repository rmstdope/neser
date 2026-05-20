//! ARM 32-bit instruction decoder and executor.
//!
//! Implements a representative subset of the ARM7TDMI instruction set,
//! sufficient for the foundational sub-issue. Specifically:
//!
//! * Data processing (AND, EOR, SUB, RSB, ADD, ADC, SBC, RSC, TST, TEQ, CMP,
//!   CMN, ORR, MOV, BIC, MVN) with immediate / register-shift operands.
//! * Branch / Branch with Link (B / BL).
//! * Branch and Exchange (BX) – switches between ARM and Thumb state based on
//!   bit 0 of the target address.
//! * Single-data transfer (LDR / STR / LDRB / STRB) with immediate offset.
//! * Software Interrupt (SWI).
//!
//! Conditional execution is honoured for every instruction. Cycle counts
//! reported here are the *execute* cycles only — pipeline / fetch cycles are
//! tracked by the [`Arm7tdmi`](super::Arm7tdmi) wrapper.

// Many literals in this module are deliberately written using bit fields that
// mirror the ARM instruction encoding for readability, hence the non-uniform
// digit grouping and explicit identity shifts (`0_u32 << 20`, etc.).
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::identity_op)]

use super::bus::Bus;
use super::registers::CpuMode;
use super::registers::{FLAG_T, Registers, condition_met};
use crate::gba::bus::WidthClass;

fn is_cart_sram_region(addr: u32) -> bool {
    matches!((addr >> 24) & 0xF, 0xE | 0xF)
}

#[cfg(test)]
use super::registers::{FLAG_C, FLAG_N, FLAG_V, FLAG_Z};

/// Outcome of executing one ARM instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecOutcome {
    /// Number of sequential (S) code-fetch cycles (resolved at PC address).
    pub seq: u8,
    /// Number of non-sequential (N) code-fetch cycles (resolved at PC address).
    pub nonseq: u8,
    /// Number of internal (I) CPU-only cycles.
    pub internal: u8,
    /// Number of sequential (S) data-access cycles (resolved at data_addr).
    pub data_seq: u8,
    /// Number of non-sequential (N) data-access cycles (resolved at data_addr).
    pub data_nonseq: u8,
    /// Address of the data access (used for resolving data cycle timing).
    pub data_addr: u32,
    /// Width of the data access.
    pub data_width: WidthClass,
    /// `true` if PC was modified by the instruction (pipeline must refill).
    pub branched: bool,
    /// `true` if the instruction triggered a software interrupt (SWI).
    pub swi: bool,
    /// `true` if the instruction is undefined (triggers undefined exception).
    pub undefined: bool,
}

impl ExecOutcome {
    /// Non-branching instruction with explicit S/N/I cycle counts (no data access).
    pub(super) fn sni(seq: u8, nonseq: u8, internal: u8) -> Self {
        Self {
            seq,
            nonseq,
            internal,
            data_seq: 0,
            data_nonseq: 0,
            data_addr: 0,
            data_width: WidthClass::Word,
            branched: false,
            swi: false,
            undefined: false,
        }
    }
    /// Non-branching data-access instruction with separate code and data cycles.
    pub(super) fn data_access(
        code_seq: u8,
        code_nonseq: u8,
        internal: u8,
        data_seq: u8,
        data_nonseq: u8,
        data_addr: u32,
        data_width: WidthClass,
    ) -> Self {
        Self {
            seq: code_seq,
            nonseq: code_nonseq,
            internal,
            data_seq,
            data_nonseq,
            data_addr,
            data_width,
            branched: false,
            swi: false,
            undefined: false,
        }
    }
    /// Branching data-access instruction with separate code and data cycles.
    pub(super) fn branch_data_access(
        code_seq: u8,
        code_nonseq: u8,
        internal: u8,
        data_seq: u8,
        data_nonseq: u8,
        data_addr: u32,
        data_width: WidthClass,
    ) -> Self {
        Self {
            seq: code_seq,
            nonseq: code_nonseq,
            internal,
            data_seq,
            data_nonseq,
            data_addr,
            data_width,
            branched: true,
            swi: false,
            undefined: false,
        }
    }
    /// Branching instruction with explicit S/N/I cycle counts (no data access).
    pub(super) fn branch_sni(seq: u8, nonseq: u8, internal: u8) -> Self {
        Self {
            seq,
            nonseq,
            internal,
            data_seq: 0,
            data_nonseq: 0,
            data_addr: 0,
            data_width: WidthClass::Word,
            branched: true,
            swi: false,
            undefined: false,
        }
    }
    /// SWI instruction with explicit S/N/I cycle counts.
    pub(super) fn swi_sni(seq: u8, nonseq: u8, internal: u8) -> Self {
        Self {
            seq,
            nonseq,
            internal,
            data_seq: 0,
            data_nonseq: 0,
            data_addr: 0,
            data_width: WidthClass::Word,
            branched: true,
            swi: true,
            undefined: false,
        }
    }
    pub(super) fn undefined() -> Self {
        // Undefined exception: 2S + 1N (pipeline flush + vector fetch)
        Self {
            seq: 2,
            nonseq: 1,
            internal: 0,
            data_seq: 0,
            data_nonseq: 0,
            data_addr: 0,
            data_width: WidthClass::Word,
            branched: true,
            undefined: true,
            swi: false,
        }
    }

    /// Resolve S/N/I cycle counts to a concrete total using bus timing.
    ///
    /// Code cycles use `s_cost`/`n_cost` (resolved at PC).
    /// Data cycles use `data_s_cost`/`data_n_cost` (resolved at data address).
    /// Internal cycles always cost 1.
    pub fn resolve_cycles(
        &self,
        s_cost: u32,
        n_cost: u32,
        data_s_cost: u32,
        data_n_cost: u32,
    ) -> u32 {
        (self.seq as u32) * s_cost
            + (self.nonseq as u32) * n_cost
            + (self.data_seq as u32) * data_s_cost
            + (self.data_nonseq as u32) * data_n_cost
            + (self.internal as u32)
    }

    /// Total flat cycles (all seq + nonseq + internal) — convenience for
    /// unit-test buses where all access costs are 1.
    pub fn total_flat_cycles(&self) -> u8 {
        self.seq + self.nonseq + self.data_seq + self.data_nonseq + self.internal
    }
}

/// Execute a single ARM instruction word.
pub fn execute<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u32) -> ExecOutcome {
    let cond = (instr >> 28) as u8;
    if !condition_met(regs.cpsr, cond) {
        return ExecOutcome::sni(1, 0, 0); // 1S: condition failed
    }

    // Decode the major instruction class. The encoding hierarchy mostly
    // follows section A3.1 of the ARM Architecture Reference Manual.
    let bits_27_25 = (instr >> 25) & 0x7;
    match bits_27_25 {
        0b000 | 0b001 => {
            // BX: 0001_0010_1111_1111_1111_0001_xxxx
            if (instr & 0x0FFF_FFF0) == 0x012F_FF10 {
                return execute_bx(regs, instr);
            }
            // MRS: 0001_0x00_1111_xxxx_0000_0000_0000
            if (instr & 0x0FBF_0FFF) == 0x010F_0000 {
                return execute_mrs(regs, instr);
            }
            // MSR (register): 0001_0x10_xxxx_1111_0000_0000_xxxx
            if (instr & 0x0FB0_FFF0) == 0x0120_F000 {
                return execute_msr_reg(regs, instr);
            }
            // MSR (immediate): 0011_0x10_xxxx_1111_xxxx_xxxx_xxxx
            if (instr & 0x0FB0_F000) == 0x0320_F000 {
                return execute_msr_imm(regs, instr);
            }
            // Multiply instructions: bits[27:24]=0000, bits[7:4]=1001
            // MUL/MLA: bits[27:22]=000000, bits[7:4]=1001
            // Long multiply: bits[27:23]=00001, bits[7:4]=1001
            if (instr & 0x0FC0_00F0) == 0x0000_0090 {
                return execute_multiply(regs, instr);
            }
            if (instr & 0x0F80_00F0) == 0x0080_0090 {
                return execute_long_multiply(regs, instr);
            }
            // Single Data Swap: bits[27:23]=00010, bits[21:20]=00, bits[7:4]=1001
            // SWP: cond 0001 0000 Rn Rd 0000 1001 Rm
            // SWPB: cond 0001 0100 Rn Rd 0000 1001 Rm
            if (instr & 0x0FB0_0FF0) == 0x0100_0090 {
                return execute_swap(regs, bus, instr);
            }
            // Halfword/Signed data transfer: bits[27:25]=000, bits[7:4]=1xx1 where xx=SH
            // Encoding: 000P U0WL for register offset, 000P U1WL for immediate offset
            // bits[7:4] = 1011 (STRH/LDRH), 1101 (LDRSB), 1111 (LDRSH)
            if (instr & 0x0E00_0090) == 0x0000_0090 && (instr & 0x60) != 0 {
                return execute_halfword_transfer(regs, bus, instr);
            }
            // ARMv4T undefined-instruction guard: opcodes 0x8–0xB (TST/TEQ/CMP/CMN)
            // with S=0 encode MRS/MSR/BX/SWP (already handled above). Anything
            // else in this space is undefined on ARMv4T (CLZ, QADD/QSUB, DSP
            // multiplies, BLX register, BKPT in ARMv5TE).
            if (instr & 0x0190_0000) == 0x0100_0000 {
                return ExecOutcome::undefined();
            }
            // Data-processing immediate / register
            execute_data_processing(regs, instr)
        }
        0b010 | 0b011 => {
            // Single data transfer (LDR/STR with immediate or register offset).
            execute_single_data_transfer(regs, bus, instr)
        }
        0b100 => {
            // Block data transfer (LDM/STM)
            execute_block_transfer(regs, bus, instr)
        }
        0b101 => execute_branch(regs, instr),
        0b111 => {
            // SWI is encoded as 1111_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
            if (instr >> 24) & 0xF == 0xF {
                return ExecOutcome::swi_sni(2, 1, 0); // 2S + 1N
            }
            ExecOutcome::sni(1, 0, 0) // 1S: coprocessor (treated as NOP)
        }
        _ => ExecOutcome::sni(1, 0, 0), // 1S: unrecognized
    }
}

// ---------------------------------------------------------------------------
// Branch / Branch with Link
// ---------------------------------------------------------------------------

fn execute_branch(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let link = (instr >> 24) & 1 != 0;
    // 24-bit signed offset, shifted left by 2.
    let offset24 = (instr & 0x00FF_FFFF) as i32;
    // Sign extend the 24-bit value.
    let offset = ((offset24 << 8) >> 8) << 2;

    if link {
        // Link register holds address of the instruction after the branch.
        // PC during execution is already 8 bytes past `instr`, so PC-4 is the
        // next instruction address.
        regs.r[14] = regs.r[15].wrapping_sub(4);
    }

    regs.r[15] = regs.r[15].wrapping_add(offset as u32);
    ExecOutcome::branch_sni(2, 1, 0) // 2S + 1N
}

fn execute_bx(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let rm = (instr & 0xF) as usize;
    let target = regs.r[rm];
    if target & 1 != 0 {
        regs.cpsr |= FLAG_T;
        regs.r[15] = target & !1;
    } else {
        regs.cpsr &= !FLAG_T;
        regs.r[15] = target & !0x3;
    }
    ExecOutcome::branch_sni(2, 1, 0) // 2S + 1N
}

// ---------------------------------------------------------------------------
// Data processing
// ---------------------------------------------------------------------------

fn execute_data_processing(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let opcode = ((instr >> 21) & 0xF) as u8;
    let s_bit = (instr >> 20) & 1 != 0;
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;
    let i_bit = (instr >> 25) & 1 != 0;
    let reg_shift_by_register = !i_bit && ((instr >> 4) & 1 != 0);

    // Resolve operand2 (with shifter carry-out for logical operations).
    let (op2, shifter_carry) = if i_bit {
        // Immediate: 8-bit value, rotated right by (rotate_imm * 2).
        let imm = instr & 0xFF;
        let rotate = ((instr >> 8) & 0xF) * 2;
        let value = imm.rotate_right(rotate);
        let carry = if rotate == 0 {
            regs.c_flag()
        } else {
            (value >> 31) & 1 != 0
        };
        (value, carry)
    } else {
        let rm = (instr & 0xF) as usize;
        let shift_imm_bit = (instr >> 4) & 1 == 0;
        let shift_type = ((instr >> 5) & 0x3) as u8;
        let amount = if shift_imm_bit {
            (instr >> 7) & 0x1F
        } else {
            // Register-specified shift amounts only use the bottom byte.
            let rs = ((instr >> 8) & 0xF) as usize;
            regs.r[rs] & 0xFF
        };
        // With register-specified shifts, using PC as operand observes PC+12.
        let rm_val = if !shift_imm_bit && rm == 15 {
            regs.r[rm].wrapping_add(4)
        } else {
            regs.r[rm]
        };
        compute_shift(rm_val, shift_type, amount, regs.c_flag(), shift_imm_bit)
    };

    // For register-specified shifts, Rn == PC also observes PC+12.
    let rn_val = if reg_shift_by_register && rn == 15 {
        regs.r[rn].wrapping_add(4)
    } else {
        regs.r[rn]
    };
    let cf_in = regs.c_flag();

    let (result, carry, overflow, write) = match opcode {
        0x0 => (rn_val & op2, shifter_carry, regs.v_flag(), true), // AND
        0x1 => (rn_val ^ op2, shifter_carry, regs.v_flag(), true), // EOR
        0x2 => {
            // SUB
            let (r, c, v) = sub_with_flags(rn_val, op2, true);
            (r, c, v, true)
        }
        0x3 => {
            // RSB
            let (r, c, v) = sub_with_flags(op2, rn_val, true);
            (r, c, v, true)
        }
        0x4 => {
            // ADD
            let (r, c, v) = add_with_flags(rn_val, op2, false);
            (r, c, v, true)
        }
        0x5 => {
            // ADC
            let (r, c, v) = add_with_flags(rn_val, op2, cf_in);
            (r, c, v, true)
        }
        0x6 => {
            // SBC: Rn - Op2 - !C
            let (r, c, v) = sub_with_flags(rn_val, op2, cf_in);
            (r, c, v, true)
        }
        0x7 => {
            // RSC: Op2 - Rn - !C
            let (r, c, v) = sub_with_flags(op2, rn_val, cf_in);
            (r, c, v, true)
        }
        0x8 => (rn_val & op2, shifter_carry, regs.v_flag(), false), // TST
        0x9 => (rn_val ^ op2, shifter_carry, regs.v_flag(), false), // TEQ
        0xA => {
            // CMP
            let (r, c, v) = sub_with_flags(rn_val, op2, true);
            (r, c, v, false)
        }
        0xB => {
            // CMN
            let (r, c, v) = add_with_flags(rn_val, op2, false);
            (r, c, v, false)
        }
        0xC => (rn_val | op2, shifter_carry, regs.v_flag(), true), // ORR
        0xD => (op2, shifter_carry, regs.v_flag(), true),          // MOV
        0xE => (rn_val & !op2, shifter_carry, regs.v_flag(), true), // BIC
        0xF => (!op2, shifter_carry, regs.v_flag(), true),         // MVN
        _ => unreachable!(),
    };

    if write {
        regs.r[rd] = result;
    }

    let mut branched = write && rd == 15;

    if s_bit {
        if rd == 15 {
            // S-bit + Rd=PC restores CPSR from SPSR.
            // For non-writing test ops (CMP/CMN/TST/TEQ), this must not
            // flush the pipeline; only true PC writes branch.
            if regs.mode().has_spsr() {
                let spsr = regs.spsr();
                regs.write_cpsr(spsr);
            }
            if write {
                branched = true;
            }
        } else {
            let n = result & 0x8000_0000 != 0;
            let z = result == 0;
            regs.set_nzcv(n, z, carry, overflow);
        }
    }

    if branched {
        if reg_shift_by_register {
            ExecOutcome::branch_sni(2, 1, 1) // 2S + 1N + 1I: PC written + reg shift
        } else {
            ExecOutcome::branch_sni(2, 1, 0) // 2S + 1N: PC written
        }
    } else if reg_shift_by_register {
        ExecOutcome::sni(1, 0, 1) // 1S + 1I: register-specified shift
    } else {
        ExecOutcome::sni(1, 0, 0) // 1S
    }
}

/// Compute a barrel-shift result and the resulting shifter carry-out.
///
/// `shift_imm` is true when the shift amount comes from an immediate; this
/// matters for the special encodings where amount==0 (e.g. LSR #0 means
/// LSR #32).
fn compute_shift(
    value: u32,
    shift_type: u8,
    amount: u32,
    carry_in: bool,
    shift_imm: bool,
) -> (u32, bool) {
    match shift_type {
        0b00 => {
            // LSL
            if amount == 0 {
                (value, carry_in)
            } else if amount < 32 {
                let carry = (value >> (32 - amount)) & 1 != 0;
                (value << amount, carry)
            } else if amount == 32 {
                (0, value & 1 != 0)
            } else {
                (0, false)
            }
        }
        0b01 => {
            // LSR
            if amount == 0 {
                if shift_imm {
                    // LSR #0 == LSR #32
                    (0, value & 0x8000_0000 != 0)
                } else {
                    (value, carry_in)
                }
            } else if amount < 32 {
                let carry = (value >> (amount - 1)) & 1 != 0;
                (value >> amount, carry)
            } else if amount == 32 {
                (0, value & 0x8000_0000 != 0)
            } else {
                (0, false)
            }
        }
        0b10 => {
            // ASR
            if amount == 0 {
                if shift_imm {
                    let sign = value & 0x8000_0000 != 0;
                    (if sign { 0xFFFF_FFFF } else { 0 }, sign)
                } else {
                    (value, carry_in)
                }
            } else if amount < 32 {
                let signed = value as i32;
                let carry = (signed >> (amount - 1)) & 1 != 0;
                ((signed >> amount) as u32, carry)
            } else {
                let sign = value & 0x8000_0000 != 0;
                (if sign { 0xFFFF_FFFF } else { 0 }, sign)
            }
        }
        0b11 => {
            // ROR / RRX
            if amount == 0 {
                if shift_imm {
                    // RRX: rotate right one bit through carry.
                    let new_carry = value & 1 != 0;
                    let result = (value >> 1) | (if carry_in { 0x8000_0000 } else { 0 });
                    (result, new_carry)
                } else {
                    (value, carry_in)
                }
            } else {
                let amt = amount % 32;
                if amt == 0 {
                    (value, value & 0x8000_0000 != 0)
                } else {
                    let result = value.rotate_right(amt);
                    let carry = (value >> (amt - 1)) & 1 != 0;
                    (result, carry)
                }
            }
        }
        _ => unreachable!(),
    }
}

fn add_with_flags(a: u32, b: u32, carry_in: bool) -> (u32, bool, bool) {
    let cin = if carry_in { 1u64 } else { 0 };
    let sum64 = (a as u64) + (b as u64) + cin;
    let result = sum64 as u32;
    let carry = sum64 > 0xFFFF_FFFF;
    let a_sign = a & 0x8000_0000;
    let b_sign = b & 0x8000_0000;
    let r_sign = result & 0x8000_0000;
    // V is set when both operands have the same sign and the result has the
    // opposite sign.
    let overflow = (a_sign == b_sign) && (a_sign != r_sign);
    (result, carry, overflow)
}

/// Subtraction helper. `borrow_in` follows the ARM convention where the
/// subtraction is `a - b - !C`. For SUB/CMP/RSB the caller passes `true` (no
/// extra borrow). For SBC/RSC the caller passes the current C flag.
fn sub_with_flags(a: u32, b: u32, borrow_in: bool) -> (u32, bool, bool) {
    // a - b - (1 - carry_in) == a + (~b) + carry_in
    let cin = if borrow_in { 1u64 } else { 0 };
    let sum64 = (a as u64) + (!b as u64) + cin;
    let result = sum64 as u32;
    let carry = sum64 > 0xFFFF_FFFF;
    let a_sign = a & 0x8000_0000;
    let b_sign = b & 0x8000_0000;
    let r_sign = result & 0x8000_0000;
    let overflow = (a_sign != b_sign) && (a_sign != r_sign);
    (result, carry, overflow)
}

// ---------------------------------------------------------------------------
// Single data transfer (LDR/STR)
// ---------------------------------------------------------------------------

fn execute_single_data_transfer<B: Bus>(
    regs: &mut Registers,
    bus: &mut B,
    instr: u32,
) -> ExecOutcome {
    let i = (instr >> 25) & 1 != 0; // immediate-flag is INVERTED for LDR/STR vs DP
    let p = (instr >> 24) & 1 != 0; // pre/post indexing
    let u = (instr >> 23) & 1 != 0; // up/down
    let b_byte = (instr >> 22) & 1 != 0; // byte/word
    let w = (instr >> 21) & 1 != 0; // writeback
    let l = (instr >> 20) & 1 != 0; // load/store
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;

    let offset = if !i {
        instr & 0xFFF
    } else {
        // Register offset: shift_imm | shift_type | 0 | Rm (no register shift).
        let rm = (instr & 0xF) as usize;
        let amount = (instr >> 7) & 0x1F;
        let shift_type = ((instr >> 5) & 0x3) as u8;
        let (val, _) = compute_shift(regs.r[rm], shift_type, amount, regs.c_flag(), true);
        val
    };

    let base = regs.r[rn];
    let offset_addr = if u {
        base.wrapping_add(offset)
    } else {
        base.wrapping_sub(offset)
    };
    let addr = if p { offset_addr } else { base };

    let result_branch;
    if l {
        // Load
        let value = if b_byte {
            bus.read8(addr) as u32
        } else {
            // ARMv4 LDR rotation on unaligned addresses.
            let raw = bus.read32(addr);
            let rot = (addr & 0x3) * 8;
            raw.rotate_right(rot)
        };
        regs.r[rd] = value;
        result_branch = rd == 15;
    } else {
        // Store. On ARM7TDMI, storing PC writes PC+12 (R15 already reads as
        // PC+8 during execute, then store adds +4).
        let value = if rd == 15 {
            regs.r[rd].wrapping_add(4)
        } else {
            regs.r[rd]
        };
        if b_byte {
            bus.write8(addr, value as u8);
        } else {
            let store_addr = if is_cart_sram_region(addr) {
                addr
            } else {
                addr & !0x3
            };
            bus.write32(store_addr, value);
        }
        result_branch = false;
    }

    // Writeback (post-indexing always writes back; pre-indexing only when W=1).
    if (!p || w) && !(l && rd == rn) {
        regs.r[rn] = offset_addr;
    }

    if result_branch {
        // LDR PC: 2S(code) + 1N(code) + 1N(data) + 1I
        let width = if b_byte {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::branch_data_access(2, 1, 1, 0, 1, addr, width)
    } else if l {
        // LDR: 1S(code) + 1N(data) + 1I
        let width = if b_byte {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, width)
    } else {
        // STR: 1N(code) + 1N(data)
        let width = if b_byte {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, width)
    }
}

// ---------------------------------------------------------------------------
// Multiply (MUL, MLA)
// ---------------------------------------------------------------------------

fn execute_multiply(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let acc = (instr >> 21) & 1 != 0; // A bit: accumulate
    let s_bit = (instr >> 20) & 1 != 0;
    let rd = ((instr >> 16) & 0xF) as usize;
    let rn = ((instr >> 12) & 0xF) as usize;
    let rs = ((instr >> 8) & 0xF) as usize;
    let rm = (instr & 0xF) as usize;

    let rm_val = regs.r[rm];
    let rs_val = regs.r[rs];

    let product = rm_val.wrapping_mul(rs_val);
    let accum_val = if acc { regs.r[rn] } else { 0 };
    let result = product.wrapping_add(accum_val);

    regs.r[rd] = result;

    let (cycles, full) = multiply_cycles_and_full(rs_val, true);

    if s_bit {
        let n = result & 0x8000_0000 != 0;
        let z = result == 0;
        let c = if full {
            multiply_carry_simple(rs_val)
        } else {
            multiply_carry_lo(rm_val, rs_val, accum_val)
        };
        regs.set_nzcv(n, z, c, regs.v_flag());
    }

    ExecOutcome::sni(1, 0, cycles - 1) // MUL/MLA: 1S + mI
}

// ---------------------------------------------------------------------------
// Long Multiply (UMULL, UMLAL, SMULL, SMLAL)
// ---------------------------------------------------------------------------

fn execute_long_multiply(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let signed = (instr >> 22) & 1 != 0; // U bit: 1 = signed
    let acc = (instr >> 21) & 1 != 0; // A bit: accumulate
    let s_bit = (instr >> 20) & 1 != 0;
    let rd_hi = ((instr >> 16) & 0xF) as usize;
    let rd_lo = ((instr >> 12) & 0xF) as usize;
    let rs = ((instr >> 8) & 0xF) as usize;
    let rm = (instr & 0xF) as usize;

    let rm_val = regs.r[rm];
    let rs_val = regs.r[rs];

    let product: u64 = if signed {
        ((rm_val as i32) as i64 * (rs_val as i32) as i64) as u64
    } else {
        rm_val as u64 * rs_val as u64
    };

    let (accum_lo, accum_hi) = if acc {
        (regs.r[rd_lo], regs.r[rd_hi])
    } else {
        (0, 0)
    };

    let result = if acc {
        let existing = ((accum_hi as u64) << 32) | (accum_lo as u64);
        product.wrapping_add(existing)
    } else {
        product
    };

    regs.r[rd_lo] = result as u32;
    regs.r[rd_hi] = (result >> 32) as u32;

    let (cycles, full) = long_multiply_cycles(rs_val, signed);

    if s_bit {
        let n = (result >> 63) & 1 != 0;
        let z = result == 0;
        let c = if full {
            multiply_carry_hi(rm_val, rs_val, accum_hi, signed)
        } else {
            multiply_carry_lo(rm_val, rs_val, accum_lo)
        };
        regs.set_nzcv(n, z, c, regs.v_flag());
    }

    // GBATek: UMULL/SMULL = 1S+(m+1)I, UMLAL/SMLAL = 1S+(m+2)I
    // long_multiply_cycles returns m+2, so: non-acc = (m+2)-1 = m+1, acc = m+2
    let internal = if acc { cycles } else { cycles - 1 };
    ExecOutcome::sni(1, 0, internal)
}

/// Compute multiply internal cycles based on Rs magnitude (early termination).
/// Per GBATek, cycles = 1S + mI where mI = 1..4 depending on Rs[31:8].
/// Also returns whether the multiplier used all 4 cycles (no early termination),
/// which determines how the C flag is computed.
pub(crate) fn multiply_cycles_and_full(rs: u32, signed: bool) -> (u8, bool) {
    if signed {
        if rs & 0xFFFF_FF00 == 0 || rs & 0xFFFF_FF00 == 0xFFFF_FF00 {
            (2, false)
        } else if rs & 0xFFFF_0000 == 0 || rs & 0xFFFF_0000 == 0xFFFF_0000 {
            (3, false)
        } else if rs & 0xFF00_0000 == 0 || rs & 0xFF00_0000 == 0xFF00_0000 {
            (4, false)
        } else {
            (5, true) // full 4 internal cycles, no early termination
        }
    } else if rs & 0xFFFF_FF00 == 0 {
        (2, false)
    } else if rs & 0xFFFF_0000 == 0 {
        (3, false)
    } else if rs & 0xFF00_0000 == 0 {
        (4, false)
    } else {
        (5, true)
    }
}

/// Long multiply cycles: similar to multiply but +1 for 64-bit result.
fn long_multiply_cycles(rs: u32, signed: bool) -> (u8, bool) {
    let (cycles, full) = multiply_cycles_and_full(rs, signed);
    (cycles + 1, full)
}

// ---------------------------------------------------------------------------
// ARM7TDMI Booth multiplier carry flag computation
// ---------------------------------------------------------------------------
//
// The ARM spec says the C flag after multiply is "unpredictable", but real
// ARM7TDMI hardware produces deterministic values from the internal Booth
// carry-save adder. The C flag is the carry-out from the final Booth
// iteration, which depends on the operand values and whether the multiplier
// terminated early.
//
// Algorithm based on analysis of ARM7TDMI Booth multiplier internals.
// Reference: zaydlang & calc84maniac's research, verified against
// NanoBoyAdvance (GPL-3.0) and mgba-emu/suite hardware test data.

/// C flag for regular MUL/MLA when the Booth multiplier ran all 4 cycles.
/// The carry comes from the final injected Booth carry bit; the last addend
/// is negative only when the upper 2 bits of the multiplier are 0b10.
pub(crate) fn multiply_carry_simple(multiplier: u32) -> bool {
    (multiplier >> 30) == 2
}

/// C flag from the low partial result, used when the Booth multiplier
/// terminated early (for both MUL and long multiply).
///
/// Simulates the carry-save adder processing Rs in Booth-encoded pairs,
/// tracking the carry bit through the iterations.
pub(crate) fn multiply_carry_lo(multiplicand: u32, multiplier: u32, accum: u32) -> bool {
    // Set low bit to cause negation to invert upper bits.
    // This bit can't propagate to the carry output.
    let multiplicand = multiplicand | 1;

    // First Booth iteration: factor is sign-extension of bit 0
    let booth: i32 = ((multiplier as i32) << 31) >> 31;
    let carry = multiplicand.wrapping_mul(booth as u32);
    let sum = carry.wrapping_add(accum);

    let mut current_booth = booth;
    let mut current_carry = carry;
    let mut current_sum = sum;
    let mut current_accum = accum;

    let mut shift: i32 = 29;
    loop {
        // Process 8 multiplier bits using 4 Booth iterations
        for _ in 0..4 {
            let next_booth: i32 = ((multiplier as i32) << shift) >> shift;
            let factor = next_booth.wrapping_sub(current_booth);
            current_booth = next_booth;
            let addend = multiplicand.wrapping_mul(factor as u32);
            current_accum ^= current_carry ^ addend;
            current_sum = current_sum.wrapping_add(addend);
            current_carry = current_sum.wrapping_sub(current_accum);
            shift -= 2;
        }
        if current_booth == multiplier as i32 {
            break;
        }
    }

    (current_carry >> 31) != 0
}

/// C flag from the high partial result for long multiply when all 4 Booth
/// cycles were used. Handles both signed and unsigned variants.
///
/// Only the last 3 Booth iterations matter for the bit-63 carry. The inputs
/// are scaled down so that the upper bits of the 64-bit addends fit in 32 bits.
fn multiply_carry_hi(multiplicand: u32, multiplier: u32, accum_hi: u32, signed: bool) -> bool {
    // Scale inputs: only last 3 Booth iterations (6 bits of multiplier) matter.
    let (md, mp) = if signed {
        (
            (multiplicand as i32 >> 6) as u32,
            (multiplier as i32 >> 26) as u32,
        )
    } else {
        (multiplicand >> 6, multiplier >> 26)
    };
    let md = md | 1; // Set low bit for negation handling

    // Pre-populate magic bit 61 for carry
    let carry = (!accum_hi) & 0x2000_0000;
    // Pre-populate magic bits 63-60 for accum (with carry magic pre-added)
    let accum = accum_hi.wrapping_sub(0x0800_0000);

    // Get Booth factors for last 3 iterations
    let booth0: i32 = ((mp as i32) << 27) >> 27;
    let booth1: i32 = ((mp as i32) << 29) >> 29;
    let booth2: i32 = ((mp as i32) << 31) >> 31;
    let factor0 = (mp as i32).wrapping_sub(booth0) as u32;
    let factor1 = booth0.wrapping_sub(booth1) as u32;
    let factor2 = booth1.wrapping_sub(booth2) as u32;

    // Get scaled value of 3rd-last Booth addend
    let addend2 = md.wrapping_mul(factor2);
    // Finalize bits 61-60 of accum magic using its sign
    let accum = accum.wrapping_sub(addend2 & 0x1000_0000);

    // Get scaled value of 2nd-last Booth addend
    let addend1 = md.wrapping_mul(factor1);
    // Finalize bits 63-62 of accum magic using its sign
    let accum = accum.wrapping_sub(addend1 & 0x4000_0000);

    // Get carry from carry-save add in bit 61 and propagate to bit 62
    let sum = accum.wrapping_add(addend1 & 0x2000_0000);
    // Subtract carry magic to get actual accum magic
    let accum = accum.wrapping_sub(carry);

    // Get scaled value of last Booth addend
    let addend0 = md.wrapping_mul(factor0);
    // Add to bit 62 and propagate carry
    let sum = sum.wrapping_add(addend0 & 0x4000_0000);

    // Cancel out accum magic bit 63 to get carry bit 63
    ((sum ^ accum) >> 31) != 0
}

// ---------------------------------------------------------------------------
// PSR Transfer (MRS, MSR)
// ---------------------------------------------------------------------------

/// MRS: Move PSR to register. Rd = CPSR or SPSR.
fn execute_mrs(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let spsr = (instr >> 22) & 1 != 0;
    let rd = ((instr >> 12) & 0xF) as usize;

    let value = if spsr { regs.spsr() } else { regs.cpsr };
    regs.r[rd] = value;

    ExecOutcome::sni(1, 0, 0) // MRS: 1S
}

/// MSR (register): Move register to PSR with field mask.
fn execute_msr_reg(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let spsr = (instr >> 22) & 1 != 0;
    let mask = ((instr >> 16) & 0xF) as u8;
    let rm = (instr & 0xF) as usize;
    let value = regs.r[rm];

    apply_msr(regs, value, mask, spsr);
    ExecOutcome::sni(1, 0, 0) // MSR reg: 1S
}

/// MSR (immediate): Move rotated immediate to PSR with field mask.
fn execute_msr_imm(regs: &mut Registers, instr: u32) -> ExecOutcome {
    let spsr = (instr >> 22) & 1 != 0;
    let mask = ((instr >> 16) & 0xF) as u8;
    let imm8 = instr & 0xFF;
    let rotate = ((instr >> 8) & 0xF) * 2;
    let value = imm8.rotate_right(rotate);

    apply_msr(regs, value, mask, spsr);
    ExecOutcome::sni(1, 0, 0) // MSR imm: 1S
}

/// Apply MSR value to CPSR or SPSR with field mask.
/// Mask bits: bit 0 = c (control), bit 1 = x (extension), bit 2 = s (status), bit 3 = f (flags).
fn apply_msr(regs: &mut Registers, value: u32, mask: u8, spsr: bool) {
    // Build the field mask from the 4 mask bits.
    let mut field_mask = 0u32;
    if mask & 0x1 != 0 {
        field_mask |= 0x0000_00FF; // c: control (bits 7:0)
    }
    if mask & 0x2 != 0 {
        field_mask |= 0x0000_FF00; // x: extension (bits 15:8)
    }
    if mask & 0x4 != 0 {
        field_mask |= 0x00FF_0000; // s: status (bits 23:16)
    }
    if mask & 0x8 != 0 {
        field_mask |= 0xFF00_0000; // f: flags (bits 31:24)
    }

    if spsr {
        let old_spsr = regs.spsr();
        let new_spsr = (old_spsr & !field_mask) | (value & field_mask);
        regs.set_spsr(new_spsr);
    } else {
        // In User mode, only flag bits can be modified.
        // System mode is privileged but has no SPSR, so check mode directly.
        let is_privileged = regs.mode() != CpuMode::User;
        let effective_mask = if is_privileged {
            field_mask
        } else {
            // User mode: only flags field allowed
            field_mask & 0xFF00_0000
        };
        let old_cpsr = regs.cpsr;
        let new_cpsr = (old_cpsr & !effective_mask) | (value & effective_mask);
        regs.write_cpsr(new_cpsr);
    }
}

// ---------------------------------------------------------------------------
// Halfword and Signed Data Transfer (LDRH, STRH, LDRSB, LDRSH)
// ---------------------------------------------------------------------------

fn execute_halfword_transfer<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u32) -> ExecOutcome {
    let p = (instr >> 24) & 1 != 0; // pre/post indexing
    let u = (instr >> 23) & 1 != 0; // up/down
    let i = (instr >> 22) & 1 != 0; // immediate offset (1) or register offset (0)
    let w = (instr >> 21) & 1 != 0; // writeback
    let l = (instr >> 20) & 1 != 0; // load/store
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;
    let sh = (instr >> 5) & 0x3; // S and H bits: 01=unsigned halfword, 10=signed byte, 11=signed halfword

    // Offset is either immediate (bits[11:8] | bits[3:0]) or register (Rm in bits[3:0])
    let offset = if i {
        let hi = (instr >> 4) & 0xF0;
        let lo = instr & 0xF;
        hi | lo
    } else {
        let rm = (instr & 0xF) as usize;
        regs.r[rm]
    };

    let base = regs.r[rn];
    let offset_addr = if u {
        base.wrapping_add(offset)
    } else {
        base.wrapping_sub(offset)
    };
    let addr = if p { offset_addr } else { base };

    let result_branch;
    if l {
        // Load
        let value = match sh {
            0b01 => {
                // LDRH: Load unsigned halfword
                let raw = bus.read16(addr) as u32;
                if addr & 1 != 0 {
                    raw.rotate_right(8)
                } else {
                    raw
                }
            }
            0b10 => {
                // LDRSB: Load signed byte, sign-extend to 32 bits
                let byte = bus.read8(addr) as i8;
                byte as i32 as u32
            }
            0b11 => {
                // LDRSH: Load signed halfword, sign-extend to 32 bits
                if addr & 1 != 0 {
                    let byte = bus.read8(addr) as i8;
                    byte as i32 as u32
                } else {
                    let hw = bus.read16(addr & !1) as i16;
                    hw as i32 as u32
                }
            }
            _ => 0, // SH=00 would be SWP, which is handled elsewhere
        };
        regs.r[rd] = value;
        result_branch = rd == 15;
    } else {
        // Store: only STRH (SH=01) is valid in ARMv4T.
        // SH=10 (LDRD) and SH=11 (STRD) are ARMv5TE-exclusive and undefined.
        if sh != 0b01 {
            return ExecOutcome::undefined();
        }
        let value = regs.r[rd] as u16;
        let store_addr = if is_cart_sram_region(addr) {
            addr
        } else {
            addr & !1
        };
        bus.write16(store_addr, value);
        result_branch = false;
    }

    // Writeback (post-indexing always writes back; pre-indexing only when W=1).
    if (!p || w) && !(l && rd == rn) {
        regs.r[rn] = offset_addr;
    }

    if result_branch {
        // LDRH PC: 2S(code) + 1N(code) + 1N(data) + 1I
        ExecOutcome::branch_data_access(2, 1, 1, 0, 1, addr, WidthClass::HalfwordOrByte)
    } else if l {
        // LDRH/LDRSB/LDRSH: 1S(code) + 1N(data) + 1I
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, WidthClass::HalfwordOrByte)
    } else {
        // STRH: 1N(code) + 1N(data)
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, WidthClass::HalfwordOrByte)
    }
}

// ---------------------------------------------------------------------------
// Block Data Transfer (LDM/STM)
// ---------------------------------------------------------------------------

fn execute_block_transfer<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u32) -> ExecOutcome {
    let p = (instr >> 24) & 1 != 0; // pre/post indexing
    let u = (instr >> 23) & 1 != 0; // up/down
    let s_bit = (instr >> 22) & 1 != 0; // PSR & force user mode
    let w = (instr >> 21) & 1 != 0; // writeback
    let l = (instr >> 20) & 1 != 0; // load/store
    let rn = ((instr >> 16) & 0xF) as usize;
    let rlist = (instr & 0xFFFF) as u16;

    // S-bit semantics:
    // - For LDM with PC in rlist: CPSR = SPSR (mode switch)
    // - Otherwise, transfer user-mode register bank (the '^' forms)
    let restore_cpsr = s_bit && l && (rlist & (1 << 15)) != 0;
    let transfer_user_regs = s_bit && !restore_cpsr;

    // Empty rlist has ARM7TDMI special behavior: transfer R15 and use
    // 16-register writeback span (+/- 0x40).
    let (effective_rlist, reg_count) = if rlist == 0 {
        (1u16 << 15, 16u32)
    } else {
        (rlist, rlist.count_ones())
    };

    let base = regs.r[rn];

    // Calculate the lowest and highest addresses based on direction
    // For U=1 (up), addresses go base, base+4, base+8, ...
    // For U=0 (down), addresses go base-n*4, ..., base-8, base-4
    let (start_addr, final_addr) = if u {
        let start = if p { base.wrapping_add(4) } else { base };
        let end = base.wrapping_add(reg_count * 4);
        (start, end)
    } else {
        let end = base.wrapping_sub(reg_count * 4);
        let start = if p { end } else { end.wrapping_add(4) };
        (start, base)
    };
    let writeback_value = if u {
        final_addr
    } else {
        base.wrapping_sub(reg_count * 4)
    };
    let first_reg_in_list = effective_rlist.trailing_zeros() as usize;

    // Process registers in order (lowest numbered first for ascending,
    // but addresses increase regardless of direction)
    let mut addr = start_addr;
    let mut branch_occurred = false;

    for i in 0..16 {
        if effective_rlist & (1 << i) != 0 {
            if l {
                // Load
                let value = bus.read32(addr);
                if transfer_user_regs {
                    regs.write_user_reg(i, value);
                } else {
                    regs.r[i] = value;
                }
                if i == 15 && !transfer_user_regs {
                    branch_occurred = true;
                }
            } else {
                // Store
                let value = if transfer_user_regs {
                    regs.read_user_reg(i)
                } else if w && i == rn && rn != first_reg_in_list {
                    // STM with base in rlist and writeback stores updated base
                    // when base isn't the first transferred register.
                    writeback_value
                } else if i == 15 {
                    regs.r[i].wrapping_add(4)
                } else {
                    regs.r[i]
                };
                bus.write32(addr, value);
            }
            addr = addr.wrapping_add(4);
        }
    }

    // S-bit: restore CPSR from SPSR when loading PC
    if restore_cpsr && regs.mode().has_spsr() {
        let spsr = regs.spsr();
        regs.write_cpsr(spsr);
    }

    // Writeback the final address to Rn.
    // For LDM with base in the register list, loaded value must be preserved.
    let base_in_rlist = (effective_rlist & (1 << rn)) != 0;
    if w && !(l && base_in_rlist) {
        regs.r[rn] = writeback_value;
    }

    // Cycle timing per GBATek:
    // LDM: 1S(code) + 1N(data) + (n-1)S(data) + 1I
    // STM: 1N(code) + 1N(data) + (n-1)S(data)
    // LDM with PC: 2S(code) + 1N(data) + (n-1)S(data) + 1I
    if branch_occurred {
        let n = reg_count as u8;
        ExecOutcome::branch_data_access(
            2,
            0,
            1,
            n.saturating_sub(1),
            1,
            start_addr,
            WidthClass::Word,
        )
    } else if l {
        let n = reg_count as u8;
        ExecOutcome::data_access(
            1,
            0,
            1,
            n.saturating_sub(1),
            1,
            start_addr,
            WidthClass::Word,
        )
    } else {
        let n = reg_count as u8;
        ExecOutcome::data_access(
            0,
            1,
            0,
            n.saturating_sub(1),
            1,
            start_addr,
            WidthClass::Word,
        )
    }
}

// ---------------------------------------------------------------------------
// Single Data Swap (SWP/SWPB)
// ---------------------------------------------------------------------------

fn execute_swap<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u32) -> ExecOutcome {
    let b_byte = (instr >> 22) & 1 != 0; // byte/word
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;
    let rm = (instr & 0xF) as usize;

    let addr = regs.r[rn];
    let rm_val = regs.r[rm]; // Read Rm value BEFORE any writes

    if b_byte {
        // SWPB: Swap byte
        let old_val = bus.read8(addr) as u32;
        bus.write8(addr, rm_val as u8);
        regs.r[rd] = old_val; // Zero-extended to 32 bits
    } else {
        // SWP: Swap word
        let aligned = addr & !0x3;
        let raw = bus.read32(aligned);
        let old_val = raw.rotate_right((addr & 0x3) * 8);
        bus.write32(aligned, rm_val);
        regs.r[rd] = old_val;
    }

    // Cycle timing: 1S(code) + 2N(data) + 1I
    let width = if b_byte {
        WidthClass::HalfwordOrByte
    } else {
        WidthClass::Word
    };
    ExecOutcome::data_access(1, 0, 1, 0, 2, addr, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::bus::GbaBus;
    use crate::gba::cpu::bus::RamBus;

    fn make_regs() -> Registers {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::User);
        regs.cpsr &= !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V);
        // Simulate the prefetch state expected by ARM execution: R15 == PC+8.
        regs.r[15] = 0x0000_0008;
        regs
    }

    /// Build an ALU immediate: cond | 001 | opcode(4) | S | Rn | Rd | rotate(4) | imm(8)
    fn arm_alu_imm(cond: u8, opcode: u8, s: bool, rn: u8, rd: u8, imm: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b001 << 25)
            | ((opcode as u32 & 0xF) << 21)
            | ((s as u32) << 20)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (imm as u32)
    }

    /// Build an ALU register: cond | 000 | opcode(4) | S | Rn | Rd | shift_imm(5) | shift_type(2) | 0 | Rm
    fn arm_alu_reg(cond: u8, opcode: u8, s: bool, rn: u8, rd: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | ((opcode as u32 & 0xF) << 21)
            | ((s as u32) << 20)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (rm as u32 & 0xF)
    }

    #[test]
    fn arm_add_register_test_vector() {
        // Test vector from issue: R0=1, R1=2, ADD R2,R0,R1 -> R2=3, flags unchanged.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 1;
        regs.r[1] = 2;
        let cpsr_before = regs.cpsr;
        let instr = arm_alu_reg(0xE, 0x4, false, 0, 2, 1);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 3);
        assert_eq!(regs.cpsr, cpsr_before, "flags must be unchanged when S=0");
    }

    #[test]
    fn arm_adds_sets_flags() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = u32::MAX;
        regs.r[1] = 1;
        let instr = arm_alu_reg(0xE, 0x4, true, 0, 2, 1); // ADDS R2,R0,R1
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0);
        assert!(regs.z_flag());
        assert!(regs.c_flag());
        assert!(!regs.v_flag());
        assert!(!regs.n_flag());
    }

    #[test]
    fn arm_subs_overflow_flag() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x8000_0000; // INT_MIN
        regs.r[1] = 1;
        let instr = arm_alu_reg(0xE, 0x2, true, 0, 2, 1); // SUBS R2,R0,R1
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0x7FFF_FFFF);
        assert!(regs.v_flag());
        assert!(!regs.n_flag());
    }

    #[test]
    fn arm_mov_imm() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // MOV R3, #0xFF
        let instr = arm_alu_imm(0xE, 0xD, false, 0, 3, 0xFF);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[3], 0xFF);
    }

    #[test]
    fn arm_mov_imm_with_rotation() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // MOV R0, #0x3F000000  (imm=0x3F, rotate=4 -> rotated by 8)
        let instr = ((0xE_u32) << 28)
            | (0b001 << 25)
            | ((0xD_u32) << 21) // MOV
            | (0u32 << 16)
            | (0u32 << 12)
            | (4u32 << 8)
            | 0x3F;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], 0x3F00_0000);
    }

    #[test]
    fn arm_mov_pc_with_register_shift_uses_pc_plus_12() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0;
        regs.r[15] = 0x100 + 8;

        // From gba-tests t224: mov r0, pc, lsl r0
        let instr = 0xE1A0_001F;
        execute(&mut regs, &mut bus, instr);

        assert_eq!(regs.r[0], 0x100 + 12);
    }

    #[test]
    fn arm_add_pc_rn_with_register_shift_uses_pc_plus_12() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0;
        regs.r[15] = 0x100 + 8;

        // From gba-tests t225: add r0, pc, r0, lsl r0
        let instr = 0xE08F_0010;
        execute(&mut regs, &mut bus, instr);

        assert_eq!(regs.r[0], 0x100 + 12);
    }

    #[test]
    fn arm_cmp_pc_with_s_bit_restores_mode_without_branching() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);

        regs.switch_mode(CpuMode::Supervisor);
        regs.r[8] = 32;

        let to_fiq = arm_msr_imm(0xE, CpuMode::Fiq.bits() as u8, 0, false, 0x1);
        execute(&mut regs, &mut bus, to_fiq);
        regs.r[8] = 64;

        let set_spsr = arm_msr_imm(0xE, CpuMode::System.bits() as u8, 0, true, 0x1);
        execute(&mut regs, &mut bus, set_spsr);

        regs.r[0] = 1;

        // From gba-tests t234: cmp pc, pc, r0 (S=1, Rd=PC, opcode=CMP)
        let instr = 0xE15F_F000;
        let outcome = execute(&mut regs, &mut bus, instr);

        assert!(!outcome.branched);
        assert_eq!(regs.mode(), CpuMode::System);
        assert_eq!(regs.r[8], 32);
    }

    #[test]
    fn arm_addne_skips_when_z_set() {
        // Test vector: ADDNE R0,R0,#1 with Z set -> R0 unchanged.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0;
        regs.cpsr |= FLAG_Z;
        let instr = arm_alu_imm(0x1, 0x4, false, 0, 0, 1); // cond=NE, ADD
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], 0, "ADDNE must not execute when Z=1");
    }

    #[test]
    fn arm_branch_offsets_pc() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x0000_1000 + 8; // simulate PC+8 prefetch state
        // Branch with 24-bit offset == 4 -> target = PC + 8 + (4<<2) = +0x10 = 0x1018
        let instr = (0xE_u32 << 28) | (0b101 << 25) | 0x4;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[15], 0x0000_1000 + 8 + (4 << 2));
        assert!(outcome.branched);
    }

    #[test]
    fn arm_branch_with_link() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x0000_2000 + 8;
        let pc_before = regs.r[15];
        // BL with offset 1 -> target = PC + 8 + 4
        let instr = (0xE_u32 << 28) | (0b101 << 25) | (1 << 24) | 0x1;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[14], pc_before - 4);
    }

    #[test]
    fn arm_bx_to_thumb() {
        // BX with bit 0 set -> Thumb state; target masked to halfword.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x0000_2001;
        // BX R1: cond=AL, 0001_0010_1111_1111_1111_0001_Rm
        let instr = 0xE12F_FF11;
        execute(&mut regs, &mut bus, instr);
        assert!(regs.thumb());
        assert_eq!(regs.r[15], 0x0000_2000);
    }

    #[test]
    fn arm_bx_to_arm_clears_t() {
        let mut regs = make_regs();
        regs.cpsr |= FLAG_T;
        let mut bus = RamBus::new(0x100);
        regs.r[2] = 0x0000_3000; // bit 0 clear
        let instr = 0xE12F_FF12;
        execute(&mut regs, &mut bus, instr);
        assert!(!regs.thumb());
        assert_eq!(regs.r[15], 0x0000_3000);
    }

    #[test]
    fn arm_swi_signals_outcome() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = 0xEF00_0000; // SWI #0
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(outcome.swi);
    }

    #[test]
    fn arm_str_ldr_word() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xCAFE_BABE;
        regs.r[1] = 0x40;
        // STR R0, [R1]      cond=AL I=0 P=1 U=1 B=0 W=0 L=0
        let str_instr = (0xE_u32 << 28) | (0b010 << 25) | (1 << 24) | (1 << 23) | (1 << 16);
        execute(&mut regs, &mut bus, str_instr);
        assert_eq!(bus.read32(0x40), 0xCAFE_BABE);

        // LDR R2, [R1]
        let ldr_instr = (0xE_u32 << 28)
            | (0b010 << 25)
            | (1 << 24)
            | (1 << 23)
            | (1 << 20)
            | (1 << 16)
            | (2 << 12);
        execute(&mut regs, &mut bus, ldr_instr);
        assert_eq!(regs.r[2], 0xCAFE_BABE);
    }

    #[test]
    fn arm_ldr_from_sram_uses_unaligned_cart_bus_lane() {
        for base in [0x0E00_0000, 0x0F00_0000] {
            let mut bus = GbaBus::new();
            bus.write8(base + 1, 0x61);
            bus.write8(base + 2, 0x6D);
            bus.write8(base + 3, 0x65);

            for (offset, expected) in [(1, 0x6161_6161), (2, 0x6D6D_6D6D), (3, 0x6565_6565)] {
                let mut regs = make_regs();
                regs.r[1] = base;
                let ldr = (0xE_u32 << 28)
                    | (0b010 << 25)
                    | (1 << 24)
                    | (1 << 23)
                    | (1 << 20)
                    | (1 << 16)
                    | offset;

                execute(&mut regs, &mut bus, ldr);

                assert_eq!(regs.r[0], expected, "base {base:#010X}, offset {offset}");
            }
        }
    }

    #[test]
    fn arm_str_pc_stores_pc_plus_4() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        regs.r[15] = 0x100 + 8;

        // STR R15, [R1]
        let str_pc =
            (0xE_u32 << 28) | (0b010 << 25) | (1 << 24) | (1 << 23) | (1 << 16) | (15 << 12);
        execute(&mut regs, &mut bus, str_pc);

        assert_eq!(bus.read32(0x40), 0x100 + 12);
    }

    #[test]
    fn arm_ldr_pre_index_writeback_same_register_keeps_loaded_value() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        let mem = 0x80;
        bus.write32(mem, 32);
        regs.r[0] = mem.wrapping_sub(4);

        // From gba-tests t360: ldr r0, [r0, 4]!
        let instr = 0xE5B0_0004;
        execute(&mut regs, &mut bus, instr);

        assert_eq!(regs.r[0], 32);
    }

    #[test]
    fn arm_ldr_post_index_writeback_same_register_keeps_loaded_value() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        let mem = 0x80;
        bus.write32(mem, 32);
        regs.r[0] = mem;

        // From gba-tests t361: ldr r0, [r0], 4
        let instr = 0xE490_0004;
        execute(&mut regs, &mut bus, instr);

        assert_eq!(regs.r[0], 32);
    }

    #[test]
    fn arm_strb_ldrb() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xAB;
        regs.r[1] = 0x50;
        // STRB R0, [R1]
        let str_instr =
            (0xE_u32 << 28) | (0b010 << 25) | (1 << 24) | (1 << 23) | (1 << 22) | (1 << 16);
        execute(&mut regs, &mut bus, str_instr);
        assert_eq!(bus.read8(0x50), 0xAB);
        // LDRB R2, [R1]
        let ldr_instr = (0xE_u32 << 28)
            | (0b010 << 25)
            | (1 << 24)
            | (1 << 23)
            | (1 << 22)
            | (1 << 20)
            | (1 << 16)
            | (2 << 12);
        execute(&mut regs, &mut bus, ldr_instr);
        assert_eq!(regs.r[2], 0xAB);
    }

    // -------------------------------------------------------------------------
    // Multiply Instructions Tests (MUL, MLA, UMULL, UMLAL, SMULL, SMLAL)
    // -------------------------------------------------------------------------

    /// Build a MUL instruction: cond | 0000 | 00AS | Rd | 0000 | Rs | 1001 | Rm
    fn arm_mul(cond: u8, s: bool, rd: u8, rs: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | ((s as u32) << 20)
            | ((rd as u32 & 0xF) << 16)
            | ((rs as u32 & 0xF) << 8)
            | (0b1001 << 4)
            | (rm as u32 & 0xF)
    }

    /// Build a MLA instruction: cond | 0000 | 001S | Rd | Rn | Rs | 1001 | Rm
    fn arm_mla(cond: u8, s: bool, rd: u8, rn: u8, rs: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | (1 << 21)
            | ((s as u32) << 20)
            | ((rd as u32 & 0xF) << 16)
            | ((rn as u32 & 0xF) << 12)
            | ((rs as u32 & 0xF) << 8)
            | (0b1001 << 4)
            | (rm as u32 & 0xF)
    }

    /// Build a long multiply: cond | 0000 | 1UAS | RdHi | RdLo | Rs | 1001 | Rm
    /// U=1 for signed, A=1 for accumulate
    #[allow(clippy::too_many_arguments)]
    fn arm_long_mul(
        cond: u8,
        signed: bool,
        acc: bool,
        s: bool,
        rd_hi: u8,
        rd_lo: u8,
        rs: u8,
        rm: u8,
    ) -> u32 {
        ((cond as u32) << 28)
            | (1 << 23)
            | (if signed { 1 << 22 } else { 0 })
            | (if acc { 1 << 21 } else { 0 })
            | ((s as u32) << 20)
            | ((rd_hi as u32 & 0xF) << 16)
            | ((rd_lo as u32 & 0xF) << 12)
            | ((rs as u32 & 0xF) << 8)
            | (0b1001 << 4)
            | (rm as u32 & 0xF)
    }

    #[test]
    fn arm_mul_basic() {
        // MUL R2, R0, R1: 3 * 7 = 21
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 3;
        regs.r[1] = 7;
        let instr = arm_mul(0xE, false, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 21);
    }

    #[test]
    fn arm_muls_zero_flag() {
        // MULS R2, R0, R1: 0 * 7 = 0, Z flag set
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0;
        regs.r[1] = 7;
        let instr = arm_mul(0xE, true, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0);
        assert!(regs.z_flag());
        assert!(!regs.n_flag());
    }

    #[test]
    fn arm_muls_negative_flag() {
        // MULS R2, R0, R1: large positive * 2 = negative (overflow wraps)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x8000_0000;
        regs.r[1] = 1;
        let instr = arm_mul(0xE, true, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0x8000_0000);
        assert!(regs.n_flag());
        assert!(!regs.z_flag());
    }

    #[test]
    fn arm_mla_basic() {
        // MLA R3, R0, R1, R2: 3 * 7 + 10 = 31
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 3;
        regs.r[1] = 7;
        regs.r[2] = 10;
        let instr = arm_mla(0xE, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[3], 31);
    }

    #[test]
    fn arm_umull_basic() {
        // UMULL R2, R3, R0, R1: 0xFFFFFFFF * 0x2 = 0x1_FFFFFFFE
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFFFF_FFFF;
        regs.r[1] = 2;
        // UMULL: signed=false, acc=false
        let instr = arm_long_mul(0xE, false, false, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0xFFFF_FFFE); // low 32 bits
        assert_eq!(regs.r[3], 0x0000_0001); // high 32 bits
    }

    #[test]
    fn arm_umlal_basic() {
        // UMLAL R2, R3, R0, R1: 0xFFFFFFFF * 0x2 + (R3:R2)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFFFF_FFFF;
        regs.r[1] = 2;
        regs.r[2] = 0x10; // existing low
        regs.r[3] = 0x5; // existing high
        // UMLAL: signed=false, acc=true
        let instr = arm_long_mul(0xE, false, true, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        // 0x1_FFFFFFFE + 0x5_00000010 = 0x6_0000000E
        assert_eq!(regs.r[2], 0x0000_000E); // low
        assert_eq!(regs.r[3], 0x0000_0007); // high (1 + 5 + carry = 7)
    }

    #[test]
    fn arm_smull_positive() {
        // SMULL R2, R3, R0, R1: 100 * 200 = 20000 (signed)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 100;
        regs.r[1] = 200;
        // SMULL: signed=true, acc=false
        let instr = arm_long_mul(0xE, true, false, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 20000);
        assert_eq!(regs.r[3], 0);
    }

    #[test]
    fn arm_smull_negative() {
        // SMULL R2, R3, R0, R1: -1 * 2 = -2 (0xFFFF_FFFF_FFFF_FFFE)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFFFF_FFFF; // -1
        regs.r[1] = 2;
        let instr = arm_long_mul(0xE, true, false, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 0xFFFF_FFFE); // low 32 bits of -2
        assert_eq!(regs.r[3], 0xFFFF_FFFF); // high 32 bits (sign extension)
    }

    #[test]
    fn arm_smlal_basic() {
        // SMLAL: -1 * 2 + 10 = 8
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFFFF_FFFF; // -1
        regs.r[1] = 2;
        regs.r[2] = 10;
        regs.r[3] = 0;
        let instr = arm_long_mul(0xE, true, true, false, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 8);
        assert_eq!(regs.r[3], 0);
    }

    #[test]
    fn arm_smulls_sets_flags() {
        // SMULLS: -1 * 2 = -2, should set N flag
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFFFF_FFFF;
        regs.r[1] = 2;
        let instr = arm_long_mul(0xE, true, false, true, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        assert!(regs.n_flag());
        assert!(!regs.z_flag());
    }

    // -------------------------------------------------------------------------
    // Multiply C flag (ARM7TDMI Booth carry) Tests
    // -------------------------------------------------------------------------
    // The ARM spec says C/V are "unpredictable" after multiply, but real
    // ARM7TDMI hardware produces deterministic C values from the internal
    // Booth multiplier carry-save adder. These tests verify that behavior
    // using known input/output pairs from mgba-emu/suite's multiply-long.c.
    //
    // Test format: flags are cleared to 0 before each multiply (matching
    // mgba suite's `msr cpsr_f, #0`), then we check the C flag after.

    /// Helper: execute SMULLS with cleared flags, return (result_lo, result_hi, c_flag).
    fn smulls_with_cleared_flags(rm: u32, rs: u32) -> (u32, u32, bool) {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = rm;
        regs.r[1] = rs;
        // Clear all flags to 0
        regs.set_nzcv(false, false, false, false);
        // SMULLS R2, R3, R0, R1
        let instr = arm_long_mul(0xE, true, false, true, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        (regs.r[2], regs.r[3], regs.c_flag())
    }

    /// Helper: execute UMULLS with cleared flags, return (result_lo, result_hi, c_flag).
    fn umulls_with_cleared_flags(rm: u32, rs: u32) -> (u32, u32, bool) {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = rm;
        regs.r[1] = rs;
        regs.set_nzcv(false, false, false, false);
        // UMULLS R2, R3, R0, R1
        let instr = arm_long_mul(0xE, false, false, true, 3, 2, 1, 0);
        execute(&mut regs, &mut bus, instr);
        (regs.r[2], regs.r[3], regs.c_flag())
    }

    #[test]
    fn smulls_c_flag_zero_times_0x80000000() {
        // SMULLS: 0 * 0x80000000 → result=0, expected C=1
        // (mgba suite test case #25)
        let (lo, hi, c) = smulls_with_cleared_flags(0x0000_0000, 0x8000_0000);
        assert_eq!(lo, 0);
        assert_eq!(hi, 0);
        assert!(c, "SMULLS 0 * 0x80000000: C should be 1");
    }

    #[test]
    fn smulls_c_flag_neg1_times_0x7fffffff() {
        // SMULLS: -1 * 0x7FFFFFFF → result=0xFFFFFFFF_80000001, expected C=1
        // (mgba suite test case #21)
        let (lo, hi, c) = smulls_with_cleared_flags(0xFFFF_FFFF, 0x7FFF_FFFF);
        assert_eq!(lo, 0x8000_0001);
        assert_eq!(hi, 0xFFFF_FFFF);
        assert!(c, "SMULLS -1 * 0x7FFFFFFF: C should be 1");
    }

    #[test]
    fn smulls_c_flag_1_times_1_no_carry() {
        // SMULLS: 1 * 1 → result=1, expected C=0
        // (mgba suite test case #7)
        let (lo, hi, c) = smulls_with_cleared_flags(0x0000_0001, 0x0000_0001);
        assert_eq!(lo, 1);
        assert_eq!(hi, 0);
        assert!(!c, "SMULLS 1 * 1: C should be 0");
    }

    #[test]
    fn umulls_c_flag_neg1_times_neg1() {
        // UMULLS: 0xFFFFFFFF * 0xFFFFFFFF → 0xFFFFFFFE_00000001, expected C=1
        // (mgba suite test case #15)
        let (lo, hi, c) = umulls_with_cleared_flags(0xFFFF_FFFF, 0xFFFF_FFFF);
        assert_eq!(lo, 0x0000_0001);
        assert_eq!(hi, 0xFFFF_FFFE);
        assert!(c, "UMULLS 0xFFFFFFFF * 0xFFFFFFFF: C should be 1");
    }

    #[test]
    fn umulls_c_flag_0x80000000_times_0x80000000() {
        // UMULLS: 0x80000000 * 0x80000000 → 0x40000000_00000000, expected C=1
        // (mgba suite test case #29)
        let (lo, hi, c) = umulls_with_cleared_flags(0x8000_0000, 0x8000_0000);
        assert_eq!(lo, 0x0000_0000);
        assert_eq!(hi, 0x4000_0000);
        assert!(c, "UMULLS 0x80000000 * 0x80000000: C should be 1");
    }

    #[test]
    fn umulls_c_flag_1_times_1_no_carry() {
        // UMULLS: 1 * 1 → result=1, expected C=0
        let (lo, hi, c) = umulls_with_cleared_flags(0x0000_0001, 0x0000_0001);
        assert_eq!(lo, 1);
        assert_eq!(hi, 0);
        assert!(!c, "UMULLS 1 * 1: C should be 0");
    }

    #[test]
    fn umulls_c_flag_asymmetric_operand_order() {
        // UMULLS: operand order matters for C flag on ARM7TDMI
        // 0x7FFFFFFF * 0x80000000: C=0 (rm=0x7F, rs=0x80)
        let (_, _, c1) = umulls_with_cleared_flags(0x7FFF_FFFF, 0x8000_0000);
        assert!(!c1, "UMULLS 0x7FFFFFFF * 0x80000000: C should be 0");

        // 0x80000000 * 0x7FFFFFFF: C=1 (rm=0x80, rs=0x7F)
        let (_, _, c2) = umulls_with_cleared_flags(0x8000_0000, 0x7FFF_FFFF);
        assert!(c2, "UMULLS 0x80000000 * 0x7FFFFFFF: C should be 1");
    }

    // -------------------------------------------------------------------------
    // PSR Transfer Tests (MRS, MSR)
    // -------------------------------------------------------------------------

    /// Build MRS instruction: cond | 0001 | 0R00 | 1111 | Rd | 0000 | 0000 | 0000
    /// R=0 for CPSR, R=1 for SPSR
    fn arm_mrs(cond: u8, rd: u8, spsr: bool) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0000 << 20)
            | (if spsr { 1 << 22 } else { 0 })
            | (0xF << 16)
            | ((rd as u32 & 0xF) << 12)
    }

    /// Build MSR (register) instruction: cond | 0001 | 0R10 | mask | 1111 | 0000 | 0000 | Rm
    fn arm_msr_reg(cond: u8, rm: u8, spsr: bool, mask: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0010 << 20)
            | (if spsr { 1 << 22 } else { 0 })
            | ((mask as u32 & 0xF) << 16)
            | (0xF << 12)
            | (rm as u32 & 0xF)
    }

    /// Build MSR (immediate) instruction: cond | 0011 | 0R10 | mask | 1111 | rotate | imm8
    fn arm_msr_imm(cond: u8, imm8: u8, rotate: u8, spsr: bool, mask: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0011_0010 << 20)
            | (if spsr { 1 << 22 } else { 0 })
            | ((mask as u32 & 0xF) << 16)
            | (0xF << 12)
            | ((rotate as u32 & 0xF) << 8)
            | (imm8 as u32)
    }

    #[test]
    fn arm_mrs_cpsr() {
        // MRS R0, CPSR: copy CPSR to R0
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.cpsr = 0xABCD_1234;
        let instr = arm_mrs(0xE, 0, false);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], 0xABCD_1234);
    }

    #[test]
    fn arm_mrs_spsr() {
        // MRS R1, SPSR: copy SPSR to R1 (requires privileged mode)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.switch_mode(CpuMode::Supervisor);
        regs.set_spsr(0x1234_5678);
        let instr = arm_mrs(0xE, 1, true);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[1], 0x1234_5678);
    }

    #[test]
    fn arm_msr_cpsr_flags_only() {
        // MSR CPSR_f, R0: update only flags (mask = 0x8 = f)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.cpsr = 0x0000_001F; // User mode, no flags
        regs.r[0] = 0xF000_0000; // NZCV all set
        let instr = arm_msr_reg(0xE, 0, false, 0x8); // mask = f (flags only)
        execute(&mut regs, &mut bus, instr);
        // Flags should be updated, mode bits preserved
        assert_eq!(regs.cpsr & 0xF000_0000, 0xF000_0000);
        assert_eq!(regs.cpsr & 0x1F, 0x1F); // mode preserved
    }

    #[test]
    fn arm_msr_cpsr_control() {
        // MSR CPSR_c, R0: update control bits (mask = 0x1 = c)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // Start in supervisor mode so we can change mode
        regs.switch_mode(CpuMode::Supervisor);
        let original_flags = regs.cpsr & 0xF000_0000;
        regs.r[0] = 0x0000_00D2; // IRQ mode bits
        let instr = arm_msr_reg(0xE, 0, false, 0x1); // mask = c (control only)
        execute(&mut regs, &mut bus, instr);
        // Mode should be changed to IRQ, flags preserved
        assert_eq!(regs.cpsr & 0x1F, 0x12); // IRQ mode
        assert_eq!(regs.cpsr & 0xF000_0000, original_flags);
    }

    #[test]
    fn arm_msr_imm_flags() {
        // MSR CPSR_f, #0xF0000000: set all flags using immediate
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.cpsr = 0x0000_001F;
        // imm8=0xF0, rotate=4 -> 0xF0 ROR 8 = 0xF000_0000
        let instr = arm_msr_imm(0xE, 0xF0, 4, false, 0x8);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.cpsr & 0xF000_0000, 0xF000_0000);
    }

    #[test]
    fn arm_msr_spsr() {
        // MSR SPSR_f, R0: update SPSR flags
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.switch_mode(CpuMode::Supervisor);
        regs.set_spsr(0x0000_0000);
        regs.r[0] = 0xA000_0000;
        let instr = arm_msr_reg(0xE, 0, true, 0x8);
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.spsr() & 0xF000_0000, 0xA000_0000);
    }

    #[test]
    fn arm_subs_pc_restores_mode_and_banked_registers() {
        // Mirrors gba-tests ARM data_processing case t223:
        // 1) Set R8 in non-FIQ bank to 32
        // 2) Enter FIQ and set banked R8 to 64
        // 3) Program SPSR to SYSTEM mode
        // 4) Execute SUBS PC, PC, #4 and verify CPSR is restored from SPSR
        //    and non-FIQ R8 bank is visible again.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);

        regs.switch_mode(CpuMode::Supervisor);
        regs.r[8] = 32;

        // MSR CPSR_c, #FIQ
        let to_fiq = arm_msr_imm(0xE, CpuMode::Fiq.bits() as u8, 0, false, 0x1);
        execute(&mut regs, &mut bus, to_fiq);
        assert_eq!(regs.mode(), CpuMode::Fiq);

        regs.r[8] = 64;

        // MSR SPSR_c, #SYSTEM
        let set_spsr = arm_msr_imm(0xE, CpuMode::System.bits() as u8, 0, true, 0x1);
        execute(&mut regs, &mut bus, set_spsr);

        // SUBS PC, PC, #4
        regs.r[15] = 0x200 + 8;
        let subs_pc = arm_alu_imm(0xE, 0x2, true, 15, 15, 4);
        let outcome = execute(&mut regs, &mut bus, subs_pc);

        assert!(outcome.branched);
        assert_eq!(regs.mode(), CpuMode::System);
        assert_eq!(regs.r[8], 32);
        assert_eq!(regs.r[15], 0x200 + 4);
    }

    // -------------------------------------------------------------------------
    // Halfword/Signed Data Transfer Tests (LDRH, STRH, LDRSB, LDRSH)
    // -------------------------------------------------------------------------

    /// Build halfword transfer instruction
    /// cond | 000P | U1WL | Rn | Rd | imm_hi | 1SH1 | imm_lo (immediate)
    /// cond | 000P | U0WL | Rn | Rd | 0000 | 1SH1 | Rm (register)
    /// S=1,H=0 -> LDRSB; S=0,H=1 -> LDRH/STRH; S=1,H=1 -> LDRSH
    #[allow(clippy::too_many_arguments)]
    fn arm_halfword_imm(
        cond: u8,
        p: bool,
        u: bool,
        w: bool,
        l: bool,
        rn: u8,
        rd: u8,
        s: bool,
        h: bool,
        offset: u8,
    ) -> u32 {
        let imm_hi = (offset >> 4) & 0xF;
        let imm_lo = offset & 0xF;
        ((cond as u32) << 28)
            | ((p as u32) << 24)
            | ((u as u32) << 23)
            | (1 << 22) // immediate flag
            | ((w as u32) << 21)
            | ((l as u32) << 20)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | ((imm_hi as u32) << 8)
            | (1 << 7)
            | ((s as u32) << 6)
            | ((h as u32) << 5)
            | (1 << 4)
            | (imm_lo as u32)
    }

    #[test]
    fn arm_strh_ldrh() {
        // STRH R0, [R1] then LDRH R2, [R1]
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x1234_ABCD; // only low 16 bits stored
        regs.r[1] = 0x40;
        // STRH: P=1, U=1, W=0, L=0, S=0, H=1
        let strh = arm_halfword_imm(0xE, true, true, false, false, 1, 0, false, true, 0);
        execute(&mut regs, &mut bus, strh);
        assert_eq!(bus.read16(0x40), 0xABCD);

        // LDRH: P=1, U=1, W=0, L=1, S=0, H=1
        let ldrh = arm_halfword_imm(0xE, true, true, false, true, 1, 2, false, true, 0);
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[2], 0x0000_ABCD); // zero-extended
    }

    #[test]
    fn arm_ldrsb() {
        // LDRSB R0, [R1]: load signed byte
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write8(0x40, 0x80); // -128 as signed byte
        regs.r[1] = 0x40;
        // LDRSB: P=1, U=1, W=0, L=1, S=1, H=0
        let ldrsb = arm_halfword_imm(0xE, true, true, false, true, 1, 0, true, false, 0);
        execute(&mut regs, &mut bus, ldrsb);
        assert_eq!(regs.r[0], 0xFFFF_FF80); // sign-extended
    }

    #[test]
    fn arm_ldrsh() {
        // LDRSH R0, [R1]: load signed halfword
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write16(0x40, 0x8000); // -32768 as signed halfword
        regs.r[1] = 0x40;
        // LDRSH: P=1, U=1, W=0, L=1, S=1, H=1
        let ldrsh = arm_halfword_imm(0xE, true, true, false, true, 1, 0, true, true, 0);
        execute(&mut regs, &mut bus, ldrsh);
        assert_eq!(regs.r[0], 0xFFFF_8000); // sign-extended
    }

    #[test]
    fn arm_ldrh_misaligned_rotates_by_8() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        bus.write16(0x40, 0x0020);

        let ldrh = arm_halfword_imm(0xE, true, true, false, true, 1, 0, false, true, 1);
        execute(&mut regs, &mut bus, ldrh);

        assert_eq!(regs.r[0], 0x2000_0000);
    }

    #[test]
    fn arm_ldrh_from_sram_uses_unaligned_cart_bus_lane() {
        for base in [0x0E00_0000, 0x0F00_0000] {
            let mut regs = make_regs();
            let mut bus = GbaBus::new();
            regs.r[1] = base;
            bus.write8(base, 0x47);
            bus.write8(base + 1, 0x61);

            let ldrh = arm_halfword_imm(0xE, true, true, false, true, 1, 0, false, true, 1);
            execute(&mut regs, &mut bus, ldrh);

            assert_eq!(regs.r[0], 0x6100_0061, "base {base:#010X}");
        }
    }

    #[test]
    fn arm_ldrsh_misaligned_behaves_like_ldrsb() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        bus.write16(0x40, 0xFF00);

        let ldrsh = arm_halfword_imm(0xE, true, true, false, true, 1, 0, true, true, 1);
        execute(&mut regs, &mut bus, ldrsh);

        assert_eq!(regs.r[0], 0xFFFF_FFFF);
    }

    #[test]
    fn arm_ldrh_with_offset() {
        // LDRH R0, [R1, #4]: load with immediate offset
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write16(0x44, 0x5678);
        regs.r[1] = 0x40;
        let ldrh = arm_halfword_imm(0xE, true, true, false, true, 1, 0, false, true, 4);
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[0], 0x5678);
    }

    #[test]
    fn arm_strh_post_indexed() {
        // STRH R0, [R1], #4: store and then add offset
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x9ABC;
        regs.r[1] = 0x40;
        // Post-indexed: P=0, U=1, W=0
        let strh = arm_halfword_imm(0xE, false, true, false, false, 1, 0, false, true, 4);
        execute(&mut regs, &mut bus, strh);
        assert_eq!(bus.read16(0x40), 0x9ABC);
        assert_eq!(regs.r[1], 0x44); // base updated
    }

    // -------------------------------------------------------------------------
    // Block Data Transfer (LDM/STM) Tests
    // -------------------------------------------------------------------------

    /// Build a block data transfer (LDM/STM): cond | 100 | P | U | S | W | L | Rn | register_list
    #[allow(clippy::too_many_arguments)]
    fn arm_block_transfer(
        cond: u8,
        p: bool,    // pre/post
        u: bool,    // up/down
        s: bool,    // PSR/force user
        w: bool,    // writeback
        l: bool,    // load/store
        rn: u8,     // base register
        rlist: u16, // register list
    ) -> u32 {
        ((cond as u32) << 28)
            | (0b100 << 25)
            | ((p as u32) << 24)
            | ((u as u32) << 23)
            | ((s as u32) << 22)
            | ((w as u32) << 21)
            | ((l as u32) << 20)
            | ((rn as u32 & 0xF) << 16)
            | (rlist as u32)
    }

    #[test]
    fn arm_stmia_ldmia() {
        // STMIA R0!, {R1-R3}: Store R1, R2, R3 starting at [R0], increment after
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 0x1111_1111;
        regs.r[2] = 0x2222_2222;
        regs.r[3] = 0x3333_3333;
        // STMIA: P=0, U=1, S=0, W=1, L=0
        let stmia = arm_block_transfer(0xE, false, true, false, true, false, 0, 0b1110);
        execute(&mut regs, &mut bus, stmia);
        assert_eq!(bus.read32(0x40), 0x1111_1111);
        assert_eq!(bus.read32(0x44), 0x2222_2222);
        assert_eq!(bus.read32(0x48), 0x3333_3333);
        assert_eq!(regs.r[0], 0x4C); // base updated by 12 bytes

        // LDMIA R4!, {R5-R7}: Load into R5, R6, R7 from [R4]
        regs.r[4] = 0x40;
        let ldmia = arm_block_transfer(0xE, false, true, false, true, true, 4, 0b1110_0000);
        execute(&mut regs, &mut bus, ldmia);
        assert_eq!(regs.r[5], 0x1111_1111);
        assert_eq!(regs.r[6], 0x2222_2222);
        assert_eq!(regs.r[7], 0x3333_3333);
        assert_eq!(regs.r[4], 0x4C);
    }

    #[test]
    fn arm_stm_stores_pc_plus_4() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[11] = 0x80;
        regs.r[0] = 0x1111_1111;
        regs.r[15] = 0x100 + 8;

        // STMFD R11!, {R0, R15}
        let stmfd = arm_block_transfer(
            0xE,
            true,
            false,
            false,
            true,
            false,
            11,
            (1 << 0) | (1 << 15),
        );
        execute(&mut regs, &mut bus, stmfd);

        assert_eq!(bus.read32(0x78), 0x1111_1111);
        assert_eq!(bus.read32(0x7C), 0x100 + 12);
    }

    #[test]
    fn arm_stm_with_s_bit_uses_user_bank_registers() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);

        regs.switch_mode(CpuMode::System);
        regs.r[0] = 0x80;
        regs.r[8] = 32;
        regs.r[9] = 0x1234_5678;

        regs.switch_mode(CpuMode::Fiq);
        regs.r[8] = 64;
        regs.r[9] = 0xAAAA_BBBB;

        // STMFD R0, {R8, R9}^
        let stm_user =
            arm_block_transfer(0xE, true, false, true, false, false, 0, (1 << 8) | (1 << 9));
        execute(&mut regs, &mut bus, stm_user);

        assert_eq!(bus.read32(0x78), 32);
        assert_eq!(bus.read32(0x7C), 0x1234_5678);
    }

    #[test]
    fn arm_stmdb_ldmdb() {
        // STMDB R0!, {R1-R2}: Store R1, R2 decrementing before, full descending stack
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x48;
        regs.r[1] = 0xAAAA_BBBB;
        regs.r[2] = 0xCCCC_DDDD;
        // STMDB: P=1, U=0, S=0, W=1, L=0
        let stmdb = arm_block_transfer(0xE, true, false, false, true, false, 0, 0b0110);
        execute(&mut regs, &mut bus, stmdb);
        // Pre-decrement: [0x40] = R1, [0x44] = R2
        assert_eq!(bus.read32(0x40), 0xAAAA_BBBB);
        assert_eq!(bus.read32(0x44), 0xCCCC_DDDD);
        assert_eq!(regs.r[0], 0x40);

        // LDMDB: Load back with decrement before
        regs.r[0] = 0x48;
        regs.r[3] = 0;
        regs.r[4] = 0;
        let ldmdb = arm_block_transfer(0xE, true, false, false, true, true, 0, 0b0001_1000);
        execute(&mut regs, &mut bus, ldmdb);
        assert_eq!(regs.r[3], 0xAAAA_BBBB);
        assert_eq!(regs.r[4], 0xCCCC_DDDD);
        assert_eq!(regs.r[0], 0x40);
    }

    #[test]
    fn arm_ldm_no_writeback() {
        // LDMIA R0, {R1-R2}: Load without writeback
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write32(0x40, 0x1234_5678);
        bus.write32(0x44, 0x9ABC_DEF0);
        regs.r[0] = 0x40;
        // W=0, no writeback
        let ldmia = arm_block_transfer(0xE, false, true, false, false, true, 0, 0b0110);
        execute(&mut regs, &mut bus, ldmia);
        assert_eq!(regs.r[1], 0x1234_5678);
        assert_eq!(regs.r[2], 0x9ABC_DEF0);
        assert_eq!(regs.r[0], 0x40); // unchanged
    }

    #[test]
    fn arm_stm_empty_rlist() {
        // Edge case: empty register list (undefined behavior, but should not crash)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        let stmia = arm_block_transfer(0xE, false, true, false, true, false, 0, 0);
        let outcome = execute(&mut regs, &mut bus, stmia);
        // Should complete without crashing; base unchanged since no registers transferred
        assert!(outcome.total_flat_cycles() > 0);
    }

    #[test]
    fn arm_ldmia_empty_rlist_loads_pc_and_writes_back_64() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        bus.write32(0x40, 0x2000_0000);

        let ldmia = arm_block_transfer(0xE, false, true, false, true, true, 0, 0);
        let outcome = execute(&mut regs, &mut bus, ldmia);

        assert!(outcome.branched);
        assert_eq!(regs.r[15], 0x2000_0000);
        assert_eq!(regs.r[0], 0x80);
    }

    #[test]
    fn arm_stmia_empty_rlist_stores_pc_and_writes_back_64() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[15] = 0x100 + 8;

        let stmia = arm_block_transfer(0xE, false, true, false, true, false, 0, 0);
        execute(&mut regs, &mut bus, stmia);

        assert_eq!(bus.read32(0x40), 0x100 + 12);
        assert_eq!(regs.r[0], 0x80);
    }

    // -------------------------------------------------------------------------
    // Single Data Swap (SWP/SWPB) Tests
    // -------------------------------------------------------------------------

    /// Build a single data swap: cond | 0001 0B00 | Rn | Rd | 0000 1001 | Rm
    fn arm_swap(cond: u8, b: bool, rn: u8, rd: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0000 << 20)
            | ((b as u32) << 22)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (0b1001 << 4)
            | (rm as u32 & 0xF)
    }

    #[test]
    fn arm_swp_word() {
        // SWP R2, R1, [R0]: Atomically swap [R0] and R1, result in R2
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write32(0x40, 0x1111_2222); // memory value
        regs.r[0] = 0x40; // address
        regs.r[1] = 0x3333_4444; // value to store
        let swp = arm_swap(0xE, false, 0, 2, 1);
        execute(&mut regs, &mut bus, swp);
        assert_eq!(regs.r[2], 0x1111_2222); // old memory value
        assert_eq!(bus.read32(0x40), 0x3333_4444); // new memory value
    }

    #[test]
    fn arm_swpb_byte() {
        // SWPB R2, R1, [R0]: Atomically swap byte [R0] and R1[7:0], result in R2
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write8(0x40, 0xAB); // memory value
        regs.r[0] = 0x40; // address
        regs.r[1] = 0xFFFF_FF12; // only low byte used
        let swpb = arm_swap(0xE, true, 0, 2, 1);
        execute(&mut regs, &mut bus, swpb);
        assert_eq!(regs.r[2], 0x0000_00AB); // old memory value (zero-extended)
        assert_eq!(bus.read8(0x40), 0x12); // new memory value
    }

    #[test]
    fn arm_swp_same_rd_rm() {
        // SWP R1, R1, [R0]: Edge case where Rd == Rm (in-place swap)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        bus.write32(0x40, 0xAAAA_BBBB);
        regs.r[0] = 0x40;
        regs.r[1] = 0xCCCC_DDDD;
        let swp = arm_swap(0xE, false, 0, 1, 1);
        execute(&mut regs, &mut bus, swp);
        // Rd gets old memory value, memory gets old Rm value
        // Since we read Rm BEFORE writing to Rd, Rd should get memory value
        assert_eq!(regs.r[1], 0xAAAA_BBBB);
        assert_eq!(bus.read32(0x40), 0xCCCC_DDDD);
    }

    #[test]
    fn arm_swp_misaligned_rotates_old_value_and_writes_aligned() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);

        regs.r[2] = 0x41; // misaligned base
        regs.r[0] = 32;
        bus.write32(0x40, 64);

        // From gba-tests t452: swp r3, r0, [r2]
        let swp = arm_swap(0xE, false, 2, 3, 0);
        execute(&mut regs, &mut bus, swp);

        assert_eq!(regs.r[3], 64_u32.rotate_right(8));
        assert_eq!(bus.read32(0x40), 32);
    }

    // -----------------------------------------------------------------------
    // ARMv5TE undefined instruction tests
    // -----------------------------------------------------------------------

    /// Encode CLZ: cond 0001 0110 1111 Rd 1111 0001 Rm
    fn arm_clz(cond: u8, rd: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0110 << 20)
            | (0xF << 16)
            | ((rd as u32 & 0xF) << 12)
            | (0xF << 8)
            | (0b0001 << 4)
            | (rm as u32 & 0xF)
    }

    /// Encode QADD: cond 0001 0000 Rn Rd 0000 0101 Rm
    fn arm_qadd(cond: u8, rn: u8, rd: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0000 << 20)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (0b0000_0101 << 4)
            | (rm as u32 & 0xF)
    }

    /// Encode BLX register (ARMv5TE): cond 0001 0010 1111 1111 1111 0011 Rm
    fn arm_blx_reg(cond: u8, rm: u8) -> u32 {
        ((cond as u32) << 28) | (0x012F_FF30) | (rm as u32 & 0xF)
    }

    /// Encode LDRD: cond 000P U I W 0 Rn Rd offset_hi 1101 offset_lo
    /// Simplified: immediate offset=0, pre-indexed, up, no writeback.
    fn arm_ldrd(cond: u8, rn: u8, rd: u8) -> u32 {
        ((cond as u32) << 28)
            | (1 << 24)       // P=1 (pre)
            | (1 << 23)       // U=1 (up)
            | (1 << 22)       // I=1 (immediate offset)
            // bit[21]=0 (W=0), bit[20]=0 (L=0 for LDRD encoding)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (0b1101 << 4) // SH=10, bits[7:4]=1101
    }

    /// Encode STRD: cond 000P U I W 0 Rn Rd offset_hi 1111 offset_lo
    fn arm_strd(cond: u8, rn: u8, rd: u8) -> u32 {
        ((cond as u32) << 28)
            | (1 << 24)       // P=1 (pre)
            | (1 << 23)       // U=1 (up)
            | (1 << 22)       // I=1 (immediate offset)
            | ((rn as u32 & 0xF) << 16)
            | ((rd as u32 & 0xF) << 12)
            | (0b1111 << 4) // SH=11, bits[7:4]=1111
    }

    /// Encode SMLABB: cond 0001 0000 Rd Rn Rs 1000 Rm
    fn arm_smlabb(cond: u8, rd: u8, rn: u8, rs: u8, rm: u8) -> u32 {
        ((cond as u32) << 28)
            | (0b0001_0000 << 20)
            | ((rd as u32 & 0xF) << 16)
            | ((rn as u32 & 0xF) << 12)
            | ((rs as u32 & 0xF) << 8)
            | (0b1000 << 4)
            | (rm as u32 & 0xF)
    }

    /// Encode BKPT: cond 0001 0010 imm12 0111 imm4
    fn arm_bkpt(cond: u8) -> u32 {
        ((cond as u32) << 28) | (0b0001_0010 << 20) | (0b0111 << 4)
    }

    #[test]
    fn arm_clz_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = arm_clz(0xE, 0, 1); // CLZ R0, R1
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "CLZ should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_qadd_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = arm_qadd(0xE, 1, 0, 2); // QADD R0, R2, R1
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "QADD should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_blx_register_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = arm_blx_reg(0xE, 0); // BLX R0
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "BLX (register) should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_ldrd_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40; // base address
        let instr = arm_ldrd(0xE, 1, 0); // LDRD R0, [R1]
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "LDRD should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_strd_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        let instr = arm_strd(0xE, 1, 0); // STRD R0, [R1]
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "STRD should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_smlabb_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = arm_smlabb(0xE, 0, 1, 2, 3); // SMLABB R0, R3, R2, R1
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "SMLABB should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_bkpt_triggers_undefined() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr = arm_bkpt(0xE); // BKPT
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "BKPT should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn arm_valid_mrs_still_works() {
        // MRS R0, CPSR: cond 0001 0000 1111 Rd 0000 0000 0000
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr: u32 = 0xE10F_0000; // MRS R0, CPSR
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            !outcome.undefined,
            "MRS should NOT trigger undefined, got: {outcome:?}"
        );
        assert_eq!(regs.r[0], regs.cpsr);
    }

    #[test]
    fn arm_valid_bx_still_works() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x100;
        let instr: u32 = 0xE12F_FF10; // BX R0
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            !outcome.undefined,
            "BX should NOT trigger undefined, got: {outcome:?}"
        );
        assert_eq!(regs.r[15], 0x100);
    }

    #[test]
    fn arm_valid_swp_still_works() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        regs.r[2] = 42;
        bus.write32(0x40, 99);
        let instr = arm_swap(0xE, false, 1, 0, 2); // SWP R0, R2, [R1]
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            !outcome.undefined,
            "SWP should NOT trigger undefined, got: {outcome:?}"
        );
        assert_eq!(regs.r[0], 99);
        assert_eq!(bus.read32(0x40), 42);
    }

    #[test]
    fn arm_valid_strh_still_works() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x1234;
        regs.r[1] = 0x40;
        // STRH R0, [R1]: cond 000 1 1 1 0 0 Rn Rd 0000 1011 0000
        // P=1, U=1, I=1, W=0, L=0, SH=01
        let instr: u32 = 0xE1C1_00B0; // STRH R0, [R1]
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            !outcome.undefined,
            "STRH should NOT trigger undefined, got: {outcome:?}"
        );
        assert_eq!(bus.read16(0x40), 0x1234);
    }

    // -------------------------------------------------------------------------
    // S/N/I cycle decomposition tests (GBATek ARM7TDMI timing)
    // -------------------------------------------------------------------------

    /// ARM data-processing (register): 1S per GBATek.
    #[test]
    fn arm_data_processing_sni_is_1s() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // MOV R0, #42 (immediate data processing, no register shift)
        let instr = 0xE3A0_002A; // MOV R0, #42
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "data-processing should be 1S");
        assert_eq!(outcome.nonseq, 0, "data-processing should have 0N");
        assert_eq!(outcome.internal, 0, "data-processing should have 0I");
    }

    /// ARM branch: 2S + 1N per GBATek.
    #[test]
    fn arm_branch_sni_is_2s_1n() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x1000);
        regs.r[15] = 0x100;
        // B #0 (branch to PC+8, offset=0)
        let instr = 0xEA00_0000;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 2, "branch should be 2S");
        assert_eq!(outcome.nonseq, 1, "branch should have 1N");
        assert_eq!(outcome.internal, 0, "branch should have 0I");
    }

    /// ARM BX: 2S + 1N per GBATek.
    #[test]
    fn arm_bx_sni_is_2s_1n() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x1000);
        regs.r[0] = 0x200;
        // BX R0
        let instr = 0xE12F_FF10;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 2, "BX should be 2S");
        assert_eq!(outcome.nonseq, 1, "BX should have 1N");
        assert_eq!(outcome.internal, 0, "BX should have 0I");
    }

    /// ARM LDR: 1S + 1N + 1I per GBATek.
    #[test]
    fn arm_ldr_sni_is_1s_1n_1i() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        bus.write32(0x40, 0x1234);
        // LDR R0, [R1] (immediate offset 0)
        let instr = 0xE591_0000;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "LDR should be 1S(code)");
        assert_eq!(outcome.nonseq, 0, "LDR should have 0N(code)");
        assert_eq!(outcome.data_nonseq, 1, "LDR should have 1N(data)");
        assert_eq!(outcome.internal, 1, "LDR should have 1I");
        assert_eq!(outcome.total_flat_cycles(), 3);
    }

    /// ARM LDR PC: 2S + 2N + 1I per GBATek (one extra N vs normal LDR).
    #[test]
    fn arm_ldr_pc_sni_is_2s_2n_1i() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        bus.write32(0x40, 0x50);
        // LDR R15, [R1] (immediate offset 0)
        let instr = 0xE591_F000;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(outcome.branched, "LDR PC should be a branch");
        assert_eq!(outcome.seq, 2, "LDR PC should be 2S(code)");
        assert_eq!(outcome.nonseq, 1, "LDR PC should have 1N(code)");
        assert_eq!(outcome.data_nonseq, 1, "LDR PC should have 1N(data)");
        assert_eq!(outcome.internal, 1, "LDR PC should have 1I");
        assert_eq!(outcome.total_flat_cycles(), 5, "LDR PC total: 2S+2N+1I=5");
    }

    /// ARM LDRH PC: 2S + 2N + 1I per GBATek (one extra N vs normal LDRH).
    #[test]
    fn arm_ldrh_pc_sni_is_2s_2n_1i() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x40;
        bus.write16(0x40, 0x50);
        // LDRH R15, [R1] — P=1, U=1, W=0, L=1, S=0, H=1, Rd=15
        let instr = arm_halfword_imm(0xE, true, true, false, true, 1, 15, false, true, 0);
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(outcome.branched, "LDRH PC should be a branch");
        assert_eq!(outcome.seq, 2, "LDRH PC should be 2S(code)");
        assert_eq!(outcome.nonseq, 1, "LDRH PC should have 1N(code)");
        assert_eq!(outcome.data_nonseq, 1, "LDRH PC should have 1N(data)");
        assert_eq!(outcome.internal, 1, "LDRH PC should have 1I");
        assert_eq!(outcome.total_flat_cycles(), 5, "LDRH PC total: 2S+2N+1I=5");
    }

    /// ARM STR: 2N per GBATek.
    #[test]
    fn arm_str_sni_is_2n() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xDEAD;
        regs.r[1] = 0x40;
        // STR R0, [R1]
        let instr = 0xE581_0000;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 0, "STR should be 0S(code)");
        assert_eq!(outcome.nonseq, 1, "STR should have 1N(code)");
        assert_eq!(outcome.data_nonseq, 1, "STR should have 1N(data)");
        assert_eq!(outcome.internal, 0, "STR should have 0I");
        assert_eq!(outcome.total_flat_cycles(), 2);
    }

    /// ARM SWP: 1S + 2N + 1I per GBATek.
    #[test]
    fn arm_swp_sni_is_1s_2n_1i() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 0xBEEF;
        bus.write32(0x40, 0xCAFE);
        let instr = arm_swap(0xE, false, 0, 2, 1);
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "SWP should be 1S(code)");
        assert_eq!(outcome.nonseq, 0, "SWP should have 0N(code)");
        assert_eq!(outcome.data_nonseq, 2, "SWP should have 2N(data)");
        assert_eq!(outcome.internal, 1, "SWP should have 1I");
        assert_eq!(outcome.total_flat_cycles(), 4);
    }

    /// ARM MUL (simple, Rs=1 → early termination 1 cycle): 1S + 1I per GBATek.
    #[test]
    fn arm_mul_sni_has_seq_and_internal() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 5; // Rm
        regs.r[1] = 1; // Rs (small → early termination, m=1)
        // MUL R0, R0, R1: cond=E, bits[27:22]=000000, A=0, S=0, Rd=0, Rn=0, Rs=1, 1001, Rm=0
        let instr = 0xE000_2190; // MUL R0, R0, R1
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "MUL should have 1S");
        assert!(outcome.internal >= 1, "MUL should have at least 1I");
        assert_eq!(outcome.nonseq, 0, "MUL should have 0N");
    }

    /// UMLAL/SMLAL (accumulate) gets +1I vs UMULL/SMULL per GBATek.
    #[test]
    fn arm_long_mul_accumulate_has_extra_internal() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 5; // Rm
        regs.r[1] = 1; // Rs (small → early termination)
        regs.r[2] = 0;
        regs.r[3] = 0;
        // UMULL R2, R3, R0, R1 (no accumulate)
        let umull = arm_long_mul(0xE, false, false, false, 3, 2, 1, 0);
        let outcome_umull = execute(&mut regs, &mut bus, umull);
        // UMLAL R2, R3, R0, R1 (accumulate)
        let umlal = arm_long_mul(0xE, false, true, false, 3, 2, 1, 0);
        let outcome_umlal = execute(&mut regs, &mut bus, umlal);
        assert_eq!(
            outcome_umlal.internal,
            outcome_umull.internal + 1,
            "UMLAL should have +1I vs UMULL"
        );
    }

    /// ARM condition-false (skipped): 1S per GBATek.
    #[test]
    fn arm_condition_false_sni_is_1s() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // EQ = 0000, requires Z=1. With default Z=0, this instruction is skipped.
        let instr = 0x03A0_002A; // MOVEQ R0, #42
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "skipped instruction should be 1S");
        assert_eq!(outcome.nonseq, 0, "skipped instruction should have 0N");
        assert_eq!(outcome.internal, 0, "skipped instruction should have 0I");
    }
}
