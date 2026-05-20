//! Thumb 16-bit instruction decoder and executor.
//!
//! Implements the subset of the Thumb instruction set required by the
//! foundational sub-issue:
//!
//! * Format 1 (move-shifted register): LSL/LSR/ASR immediate
//! * Format 2 (add/subtract): ADD/SUB register or 3-bit immediate
//! * Format 3 (move/compare/add/subtract immediate)
//! * Format 5 (Hi-register operations / BX): ADD, CMP, MOV (Hi), BX
//! * Format 6 (PC-relative load): LDR Rd, [PC, #imm]
//! * Format 14 (push/pop registers): PUSH/POP, including LR/PC variants
//! * Format 16 (conditional branch)
//! * Format 18 (unconditional branch)
//!
//! Cycle counts returned here are the *execute* cycles only; pipeline / fetch
//! cycles are accounted for by the [`Arm7tdmi`](super::Arm7tdmi) wrapper.

// Many literals in this module mirror instruction-encoding bit fields (e.g.
// `0b1011_0_10_1_0000_1111` for Thumb PUSH) so we deliberately keep the
// non-uniform digit grouping for readability.
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::manual_range_patterns)]

use super::arm::ExecOutcome;
use super::bus::Bus;
#[cfg(test)]
use super::registers::{FLAG_C, FLAG_N, FLAG_V, FLAG_Z};
use super::registers::{Registers, condition_met};
use crate::gba::bus::WidthClass;

fn is_cart_sram_region(addr: u32) -> bool {
    matches!((addr >> 24) & 0xF, 0xE | 0xF)
}

fn thumb_store_word_addr(addr: u32) -> u32 {
    if is_cart_sram_region(addr) {
        addr
    } else {
        addr & !0x3
    }
}

fn thumb_store_halfword_addr(addr: u32) -> u32 {
    if is_cart_sram_region(addr) {
        addr
    } else {
        addr & !1
    }
}

/// Execute one Thumb instruction.
pub fn execute<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let top5 = instr >> 11;
    match top5 {
        // Format 1: 000xx (LSL/LSR/ASR immediate) — but exclude format 2 (00011).
        0b00000 | 0b00001 | 0b00010 | 0b00011 => {
            if (instr >> 11) & 0b11111 == 0b00011 {
                exec_format2(regs, instr)
            } else {
                exec_format1(regs, instr)
            }
        }
        // Format 3: 001xx
        0b00100 | 0b00101 | 0b00110 | 0b00111 => exec_format3(regs, instr),
        // Format 4 / 5 / 6 / 7 / 8: 010xx
        0b01000 | 0b01001 | 0b01010 | 0b01011 => {
            if instr & 0xFC00 == 0x4000 {
                // ALU register (format 4)
                exec_format4(regs, instr)
            } else if instr & 0xFC00 == 0x4400 {
                // Hi-register / BX (format 5)
                exec_format5(regs, instr)
            } else if instr & 0xF800 == 0x4800 {
                // PC-relative load (format 6)
                exec_format6(regs, bus, instr)
            } else if instr & 0xF200 == 0x5000 {
                // Format 7: Register offset load/store (bit 9 = 0)
                exec_format7(regs, bus, instr)
            } else if instr & 0xF200 == 0x5200 {
                // Format 8: Sign-extended load/store (bit 9 = 1)
                exec_format8(regs, bus, instr)
            } else {
                ExecOutcome::sni(1, 0, 0)
            }
        }
        // Format 9: 011xx — Immediate offset load/store
        0b01100 | 0b01101 | 0b01110 | 0b01111 => exec_format9(regs, bus, instr),
        // Format 10: 1000x — Halfword load/store
        0b10000 | 0b10001 => exec_format10(regs, bus, instr),
        // Format 11: 1001x — SP-relative load/store
        0b10010 | 0b10011 => exec_format11(regs, bus, instr),
        // Format 12: 1010x — Load address (ADD PC/SP)
        0b10100 | 0b10101 => exec_format12(regs, instr),
        // Format 13 / 14: 1011xxxx
        0b10110 | 0b10111 => {
            if instr & 0xFF00 == 0xB000 {
                // Format 13: Add offset to SP
                exec_format13(regs, instr)
            } else if instr & 0xF600 == 0xB400 {
                // Format 14: PUSH/POP
                exec_format14(regs, bus, instr)
            } else if instr & 0xFF00 == 0xBE00 {
                // BKPT (ARMv5TE): undefined on ARMv4T.
                ExecOutcome::undefined()
            } else {
                ExecOutcome::sni(1, 0, 0)
            }
        }
        // Format 15: 1100x — Multiple load/store
        0b11000 | 0b11001 => exec_format15(regs, bus, instr),
        // Format 16 (cond branch): 1101xxxx
        0b11010 | 0b11011 => exec_format16(regs, instr),
        // Format 18 (uncond branch): 11100
        0b11100 => exec_format18(regs, instr),
        // BLX (ARMv5TE Thumb): 11101_xxxxxxxxxxx — undefined on ARMv4T.
        0b11101 => ExecOutcome::undefined(),
        // Format 19: 1111x — Long branch with link
        0b11110 | 0b11111 => exec_format19(regs, instr),
        _ => ExecOutcome::sni(1, 0, 0),
    }
}

// ---------------------------------------------------------------------------
// Format 1 — move-shifted register
// ---------------------------------------------------------------------------

fn exec_format1(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let op = (instr >> 11) & 0x3;
    let amount = ((instr >> 6) & 0x1F) as u32;
    let rs = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let value = regs.r[rs];

    let (result, carry) = match op {
        0b00 => {
            // LSL
            if amount == 0 {
                (value, regs.c_flag())
            } else {
                let c = (value >> (32 - amount)) & 1 != 0;
                (value << amount, c)
            }
        }
        0b01 => {
            // LSR (#0 means #32)
            if amount == 0 {
                (0, value & 0x8000_0000 != 0)
            } else {
                let c = (value >> (amount - 1)) & 1 != 0;
                (value >> amount, c)
            }
        }
        0b10 => {
            // ASR (#0 means #32)
            if amount == 0 {
                let sign = value & 0x8000_0000 != 0;
                (if sign { 0xFFFF_FFFF } else { 0 }, sign)
            } else {
                let signed = value as i32;
                let c = (signed >> (amount - 1)) & 1 != 0;
                ((signed >> amount) as u32, c)
            }
        }
        _ => unreachable!(),
    };

    regs.r[rd] = result;
    regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, regs.v_flag());
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 2 — add/subtract (register or 3-bit immediate)
// ---------------------------------------------------------------------------

fn exec_format2(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let imm_flag = (instr >> 10) & 1 != 0;
    let op_sub = (instr >> 9) & 1 != 0;
    let rn_or_imm = ((instr >> 6) & 0x7) as u32;
    let rs = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let lhs = regs.r[rs];
    let rhs = if imm_flag {
        rn_or_imm
    } else {
        regs.r[rn_or_imm as usize]
    };

    let (result, carry, overflow) = if op_sub {
        sub_flags(lhs, rhs)
    } else {
        add_flags(lhs, rhs)
    };
    regs.r[rd] = result;
    regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 3 — MOV/CMP/ADD/SUB immediate
// ---------------------------------------------------------------------------

fn exec_format3(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let op = (instr >> 11) & 0x3;
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;

    match op {
        0b00 => {
            // MOV Rd, #imm
            regs.r[rd] = imm;
            regs.set_nzcv(false, imm == 0, regs.c_flag(), regs.v_flag());
        }
        0b01 => {
            // CMP Rd, #imm
            let (result, carry, overflow) = sub_flags(regs.r[rd], imm);
            regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);
        }
        0b10 => {
            // ADD Rd, #imm
            let (result, carry, overflow) = add_flags(regs.r[rd], imm);
            regs.r[rd] = result;
            regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);
        }
        0b11 => {
            // SUB Rd, #imm
            let (result, carry, overflow) = sub_flags(regs.r[rd], imm);
            regs.r[rd] = result;
            regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);
        }
        _ => unreachable!(),
    }
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 4 — ALU operations (register)
// ---------------------------------------------------------------------------

fn exec_format4(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let op = (instr >> 6) & 0xF;
    let rs = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let a = regs.r[rd];
    let b = regs.r[rs];

    let (result, carry, overflow, write) = match op {
        0x0 => (a & b, regs.c_flag(), regs.v_flag(), true), // AND
        0x1 => (a ^ b, regs.c_flag(), regs.v_flag(), true), // EOR
        0x2 => {
            // LSL: Rd = Rd << Rs[7:0]
            let shift = b & 0xFF;
            if shift == 0 {
                (a, regs.c_flag(), regs.v_flag(), true)
            } else if shift < 32 {
                let c = (a >> (32 - shift)) & 1 != 0;
                (a << shift, c, regs.v_flag(), true)
            } else if shift == 32 {
                (0, a & 1 != 0, regs.v_flag(), true)
            } else {
                (0, false, regs.v_flag(), true)
            }
        }
        0x3 => {
            // LSR: Rd = Rd >> Rs[7:0] (logical)
            let shift = b & 0xFF;
            if shift == 0 {
                (a, regs.c_flag(), regs.v_flag(), true)
            } else if shift < 32 {
                let c = (a >> (shift - 1)) & 1 != 0;
                (a >> shift, c, regs.v_flag(), true)
            } else if shift == 32 {
                (0, a & 0x8000_0000 != 0, regs.v_flag(), true)
            } else {
                (0, false, regs.v_flag(), true)
            }
        }
        0x4 => {
            // ASR: Rd = Rd >> Rs[7:0] (arithmetic)
            let shift = b & 0xFF;
            if shift == 0 {
                (a, regs.c_flag(), regs.v_flag(), true)
            } else if shift < 32 {
                let signed = a as i32;
                let c = (signed >> (shift - 1)) & 1 != 0;
                ((signed >> shift) as u32, c, regs.v_flag(), true)
            } else {
                // shift >= 32: result is all sign bits
                let sign = a & 0x8000_0000 != 0;
                (
                    if sign { 0xFFFF_FFFF } else { 0 },
                    sign,
                    regs.v_flag(),
                    true,
                )
            }
        }
        0x5 => {
            // ADC: Rd = Rd + Rs + C
            let cin = if regs.c_flag() { 1u64 } else { 0 };
            let sum64 = a as u64 + b as u64 + cin;
            let result = sum64 as u32;
            let carry = sum64 > 0xFFFF_FFFF;
            let a_sign = a & 0x8000_0000;
            let b_sign = b & 0x8000_0000;
            let r_sign = result & 0x8000_0000;
            let overflow = (a_sign == b_sign) && (a_sign != r_sign);
            (result, carry, overflow, true)
        }
        0x6 => {
            // SBC: Rd = Rd - Rs - !C
            let cin = if regs.c_flag() { 1u64 } else { 0 };
            let sum64 = a as u64 + (!b) as u64 + cin;
            let result = sum64 as u32;
            let carry = sum64 > 0xFFFF_FFFF;
            let a_sign = a & 0x8000_0000;
            let b_sign = b & 0x8000_0000;
            let r_sign = result & 0x8000_0000;
            let overflow = (a_sign != b_sign) && (a_sign != r_sign);
            (result, carry, overflow, true)
        }
        0x7 => {
            // ROR: Rd = Rd ROR Rs[7:0]
            let shift = b & 0xFF;
            if shift == 0 {
                (a, regs.c_flag(), regs.v_flag(), true)
            } else {
                let amt = shift & 0x1F; // effective rotation
                if amt == 0 {
                    // shift is multiple of 32
                    (a, a & 0x8000_0000 != 0, regs.v_flag(), true)
                } else {
                    let result = a.rotate_right(amt);
                    let c = result & 0x8000_0000 != 0;
                    (result, c, regs.v_flag(), true)
                }
            }
        }
        0x8 => {
            // TST: set flags for Rd AND Rs (no write)
            let result = a & b;
            (result, regs.c_flag(), regs.v_flag(), false)
        }
        0x9 => {
            // NEG: Rd = 0 - Rs
            let (r, c, v) = sub_flags(0, b);
            (r, c, v, true)
        }
        0xA => {
            // CMP
            let (r, c, v) = sub_flags(a, b);
            (r, c, v, false)
        }
        0xB => {
            // CMN: set flags for Rd + Rs (no write)
            let (r, c, v) = add_flags(a, b);
            (r, c, v, false)
        }
        0xC => (a | b, regs.c_flag(), regs.v_flag(), true), // ORR
        0xD => {
            // MUL: Rd = Rd * Rs
            // C flag is computed by the ARM7TDMI Booth multiplier
            let result = a.wrapping_mul(b);
            let (_, full) = super::arm::multiply_cycles_and_full(b, true);
            let c = if full {
                super::arm::multiply_carry_simple(b)
            } else {
                super::arm::multiply_carry_lo(a, b, 0)
            };
            (result, c, regs.v_flag(), true)
        }
        0xE => (a & !b, regs.c_flag(), regs.v_flag(), true), // BIC
        0xF => (!b, regs.c_flag(), regs.v_flag(), true),     // MVN
        _ => unreachable!(),
    };
    if write {
        regs.r[rd] = result;
    }
    regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);

    // GBATek: 1S+1I for shift-by-register (opcodes 2-4,7),
    //         1S+mI for MUL (opcode 0xD), else 1S.
    let internal = match op {
        0x2 | 0x3 | 0x4 | 0x7 => 1, // register shifts: +1I
        0xD => {
            let (cycles, _) = super::arm::multiply_cycles_and_full(b, true);
            cycles - 1 // subtract the 1S already counted
        }
        _ => 0,
    };
    ExecOutcome::sni(1, 0, internal)
}

// ---------------------------------------------------------------------------
// Format 5 — Hi-register operations / Branch and Exchange
// ---------------------------------------------------------------------------

fn exec_format5(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let op = (instr >> 8) & 0x3;
    let h1 = (instr >> 7) & 1;
    let h2 = (instr >> 6) & 1;
    let rs = (((instr >> 3) & 0x7) | (h2 << 3)) as usize;
    let rd = ((instr & 0x7) | (h1 << 3)) as usize;
    let src_value = regs.r[rs];

    match op {
        0b00 => {
            // ADD Rd, Rs (no flags)
            regs.r[rd] = regs.r[rd].wrapping_add(src_value);
            if rd == 15 {
                regs.r[15] &= !1;
                return ExecOutcome::branch_sni(2, 1, 0);
            }
        }
        0b01 => {
            // CMP Rd, Rs (sets flags)
            let (r, c, v) = sub_flags(regs.r[rd], src_value);
            regs.set_nzcv(r & 0x8000_0000 != 0, r == 0, c, v);
        }
        0b10 => {
            // MOV Rd, Rs (no flags)
            regs.r[rd] = src_value;
            if rd == 15 {
                regs.r[15] &= !1;
                return ExecOutcome::branch_sni(2, 1, 0);
            }
        }
        0b11 => {
            // BX Rs
            if src_value & 1 != 0 {
                regs.r[15] = src_value & !1;
                regs.set_thumb(true);
            } else {
                regs.r[15] = src_value & !0x3;
                regs.set_thumb(false);
            }
            return ExecOutcome::branch_sni(2, 1, 0);
        }
        _ => unreachable!(),
    }
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 6 — PC-relative load
// ---------------------------------------------------------------------------

fn exec_format6<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;
    // PC bit 1 forced to zero, then add imm * 4.
    let pc = regs.r[15] & !0x2;
    let addr = pc.wrapping_add(imm << 2);
    regs.r[rd] = bus.read32(addr);
    // LDR: 1S(code) + 1N(data) + 1I
    ExecOutcome::data_access(1, 0, 1, 0, 1, addr, WidthClass::Word)
}

// ---------------------------------------------------------------------------
// Format 7 — Load/store with register offset
// ---------------------------------------------------------------------------

fn exec_format7<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let l = (instr >> 11) & 1 != 0; // load/store
    let b = (instr >> 10) & 1 != 0; // byte/word
    let ro = ((instr >> 6) & 0x7) as usize;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;

    let addr = regs.r[rb].wrapping_add(regs.r[ro]);

    if l {
        // Load
        regs.r[rd] = if b {
            bus.read8(addr) as u32
        } else {
            // ARMv4 LDR rotation on unaligned addresses
            let raw = bus.read32(addr);
            let rot = (addr & 0x3) * 8;
            raw.rotate_right(rot)
        };
    } else {
        // Store
        if b {
            bus.write8(addr, regs.r[rd] as u8);
        } else {
            bus.write32(thumb_store_word_addr(addr), regs.r[rd]);
        }
    }
    if l {
        let width = if b {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, width)
    } else {
        let width = if b {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, width)
    }
}

// ---------------------------------------------------------------------------
// Format 8 — Load/store sign-extended byte/halfword
// ---------------------------------------------------------------------------

fn exec_format8<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let h = (instr >> 11) & 1 != 0;
    let s = (instr >> 10) & 1 != 0;
    let ro = ((instr >> 6) & 0x7) as usize;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;

    let addr = regs.r[rb].wrapping_add(regs.r[ro]);

    // S=0, H=0: STRH (store halfword)
    // S=0, H=1: LDRH (load unsigned halfword)
    // S=1, H=0: LDSB (load signed byte)
    // S=1, H=1: LDSH (load signed halfword)
    if !s && !h {
        // STRH: 1N(code) + 1N(data)
        bus.write16(thumb_store_halfword_addr(addr), regs.r[rd] as u16);
        return ExecOutcome::data_access(0, 1, 0, 0, 1, addr, WidthClass::HalfwordOrByte);
    }

    regs.r[rd] = match (s, h) {
        (false, true) => {
            // LDRH: odd address rotates aligned halfword right by 8.
            let raw = bus.read16(addr) as u32;
            if addr & 1 != 0 {
                raw.rotate_right(8)
            } else {
                raw
            }
        }
        (true, false) => bus.read8(addr) as i8 as i32 as u32, // LDSB
        (true, true) => {
            // LDSH: odd address behaves like signed byte load.
            if addr & 1 != 0 {
                bus.read8(addr) as i8 as i32 as u32
            } else {
                bus.read16(addr) as i16 as i32 as u32
            }
        }
        _ => unreachable!(),
    };
    // Load: 1S(code) + 1N(data) + 1I
    ExecOutcome::data_access(1, 0, 1, 0, 1, addr, WidthClass::HalfwordOrByte)
}

// ---------------------------------------------------------------------------
// Format 9 — Load/store with immediate offset
// ---------------------------------------------------------------------------

fn exec_format9<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let b = (instr >> 12) & 1 != 0; // byte/word
    let l = (instr >> 11) & 1 != 0; // load/store
    let offset = ((instr >> 6) & 0x1F) as u32;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;

    let addr = if b {
        regs.r[rb].wrapping_add(offset) // byte: offset is in bytes
    } else {
        regs.r[rb].wrapping_add(offset << 2) // word: offset is in words
    };

    if l {
        regs.r[rd] = if b {
            bus.read8(addr) as u32
        } else {
            // ARMv4 LDR rotation on unaligned addresses
            let raw = bus.read32(addr);
            let rot = (addr & 0x3) * 8;
            raw.rotate_right(rot)
        };
    } else if b {
        bus.write8(addr, regs.r[rd] as u8);
    } else {
        bus.write32(thumb_store_word_addr(addr), regs.r[rd]);
    }
    if l {
        let width = if b {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, width)
    } else {
        let width = if b {
            WidthClass::HalfwordOrByte
        } else {
            WidthClass::Word
        };
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, width)
    }
}

// ---------------------------------------------------------------------------
// Format 10 — Load/store halfword
// ---------------------------------------------------------------------------

fn exec_format10<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let l = (instr >> 11) & 1 != 0;
    let offset = (((instr >> 6) & 0x1F) << 1) as u32; // offset * 2
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;

    let addr = regs.r[rb].wrapping_add(offset);

    if l {
        let raw = bus.read16(addr) as u32;
        regs.r[rd] = if addr & 1 != 0 {
            raw.rotate_right(8)
        } else {
            raw
        };
    } else {
        bus.write16(thumb_store_halfword_addr(addr), regs.r[rd] as u16);
    }
    if l {
        // LDRH: 1S(code) + 1N(data) + 1I
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, WidthClass::HalfwordOrByte)
    } else {
        // STRH: 1N(code) + 1N(data)
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, WidthClass::HalfwordOrByte)
    }
}

// ---------------------------------------------------------------------------
// Format 11 — SP-relative load/store
// ---------------------------------------------------------------------------

fn exec_format11<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let l = (instr >> 11) & 1 != 0;
    let rd = ((instr >> 8) & 0x7) as usize;
    let offset = ((instr & 0xFF) << 2) as u32; // offset * 4

    let addr = regs.r[13].wrapping_add(offset);

    if l {
        // ARMv4 LDR rotation on unaligned addresses
        let raw = bus.read32(addr);
        let rot = (addr & 0x3) * 8;
        regs.r[rd] = raw.rotate_right(rot);
    } else {
        bus.write32(thumb_store_word_addr(addr), regs.r[rd]);
    }
    if l {
        // LDR: 1S(code) + 1N(data) + 1I
        ExecOutcome::data_access(1, 0, 1, 0, 1, addr, WidthClass::Word)
    } else {
        // STR: 1N(code) + 1N(data)
        ExecOutcome::data_access(0, 1, 0, 0, 1, addr, WidthClass::Word)
    }
}

// ---------------------------------------------------------------------------
// Format 12 — Load address (get relative address)
// ---------------------------------------------------------------------------

fn exec_format12(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let sp = (instr >> 11) & 1 != 0; // 0=PC, 1=SP
    let rd = ((instr >> 8) & 0x7) as usize;
    let offset = ((instr & 0xFF) << 2) as u32; // offset * 4

    let base = if sp {
        regs.r[13]
    } else {
        regs.r[15] & !0x2 // PC with bit 1 forced to 0
    };
    regs.r[rd] = base.wrapping_add(offset);
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 13 — Add offset to stack pointer
// ---------------------------------------------------------------------------

fn exec_format13(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let s = (instr >> 7) & 1 != 0; // 0=add, 1=subtract
    let offset = ((instr & 0x7F) << 2) as u32; // offset * 4

    if s {
        regs.r[13] = regs.r[13].wrapping_sub(offset);
    } else {
        regs.r[13] = regs.r[13].wrapping_add(offset);
    }
    ExecOutcome::sni(1, 0, 0)
}

// ---------------------------------------------------------------------------
// Format 14 — PUSH / POP
// ---------------------------------------------------------------------------

fn exec_format14<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let load = (instr >> 11) & 1 != 0;
    let extra = (instr >> 8) & 1 != 0;
    let reg_list = (instr & 0xFF) as u8;

    let count = reg_list.count_ones() + if extra { 1 } else { 0 };
    let mut branched = false;

    let data_addr;
    if !load {
        // PUSH: SP decremented first, then write low → high regs at low → high addresses.
        let mut sp = regs.r[13].wrapping_sub(count * 4);
        data_addr = sp;
        regs.r[13] = sp;
        for i in 0..8 {
            if reg_list & (1 << i) != 0 {
                bus.write32(thumb_store_word_addr(sp), regs.r[i]);
                sp = sp.wrapping_add(4);
            }
        }
        if extra {
            // PUSH stores LR.
            bus.write32(thumb_store_word_addr(sp), regs.r[14]);
        }
    } else {
        // POP: read low → high regs from low → high addresses, then update SP.
        let mut sp = regs.r[13];
        data_addr = sp;
        for i in 0..8 {
            if reg_list & (1 << i) != 0 {
                regs.r[i] = bus.read32(sp);
                sp = sp.wrapping_add(4);
            }
        }
        if extra {
            // POP loads PC and may switch state on bit 0.
            let value = bus.read32(sp);
            sp = sp.wrapping_add(4);
            // Stay in Thumb state regardless of bit 0 on ARM7TDMI POP {PC}.
            regs.r[15] = value & !1;
            branched = true;
        }
        regs.r[13] = sp;
    }

    let n = count as u8;
    if branched {
        // POP {PC}: 2S(code) + 1N(data) + (n-1)S(data) + 1I
        ExecOutcome::branch_data_access(
            2,
            0,
            1,
            n.saturating_sub(1),
            1,
            data_addr,
            WidthClass::Word,
        )
    } else if load {
        // POP: 1S(code) + 1N(data) + (n-1)S(data) + 1I
        ExecOutcome::data_access(1, 0, 1, n.saturating_sub(1), 1, data_addr, WidthClass::Word)
    } else {
        // PUSH: 1N(code) + 1N(data) + (n-1)S(data)
        ExecOutcome::data_access(0, 1, 0, n.saturating_sub(1), 1, data_addr, WidthClass::Word)
    }
}

// ---------------------------------------------------------------------------
// Format 15 — Multiple load/store (STMIA/LDMIA)
// ---------------------------------------------------------------------------

fn exec_format15<B: Bus>(regs: &mut Registers, bus: &mut B, instr: u16) -> ExecOutcome {
    let l = (instr >> 11) & 1 != 0;
    let rb = ((instr >> 8) & 0x7) as usize;
    let rlist = (instr & 0xFF) as u8;

    // Empty rlist on ARM7TDMI is a special transfer of PC with a
    // 16-register writeback span (+0x40).
    let (effective_rlist, count) = if rlist == 0 {
        (1u16 << 15, 16u32)
    } else {
        (rlist as u16, rlist.count_ones())
    };
    let base = regs.r[rb]; // Capture original base before any modifications
    let mut addr = base;
    let writeback_value = base.wrapping_add(count * 4);
    let first_reg_in_list = effective_rlist.trailing_zeros() as usize;
    let mut branched = false;

    if l {
        // LDMIA: Load multiple, increment after
        for i in 0..16 {
            if effective_rlist & (1 << i) != 0 {
                let value = bus.read32(addr);
                if i == 15 {
                    // Like POP {PC} on ARM7TDMI in Thumb state: clear bit 0, keep T set.
                    regs.r[15] = value & !1;
                    branched = true;
                } else {
                    regs.r[i] = value;
                }
                addr = addr.wrapping_add(4);
            }
        }
    } else {
        // STMIA: Store multiple, increment after
        for i in 0..16 {
            if effective_rlist & (1 << i) != 0 {
                let value = if i == 15 {
                    // In Thumb state, storing PC in block transfer uses PC+2.
                    regs.r[15].wrapping_add(2)
                } else if i == rb && rb != first_reg_in_list {
                    // Base in rlist stores updated base when it is not first.
                    writeback_value
                } else {
                    regs.r[i]
                };
                bus.write32(addr, value);
                addr = addr.wrapping_add(4);
            }
        }
    }

    // Writeback uses original base, not potentially modified regs.r[rb]
    regs.r[rb] = base.wrapping_add(count * 4);

    let n = count as u8;
    if branched {
        // LDMIA with PC: 2S(code) + 1N(data) + (n-1)S(data) + 1I
        ExecOutcome::branch_data_access(2, 0, 1, n.saturating_sub(1), 1, base, WidthClass::Word)
    } else if l {
        // LDMIA: 1S(code) + 1N(data) + (n-1)S(data) + 1I
        ExecOutcome::data_access(1, 0, 1, n.saturating_sub(1), 1, base, WidthClass::Word)
    } else {
        // STMIA: 1N(code) + 1N(data) + (n-1)S(data)
        ExecOutcome::data_access(0, 1, 0, n.saturating_sub(1), 1, base, WidthClass::Word)
    }
}

// ---------------------------------------------------------------------------
// Format 16 — Conditional branch
// ---------------------------------------------------------------------------

fn exec_format16(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let cond = ((instr >> 8) & 0xF) as u8;
    if cond == 0xF {
        // SWI in Thumb: 2S + 1N
        return ExecOutcome::swi_sni(2, 1, 0);
    }
    if !condition_met(regs.cpsr, cond) {
        return ExecOutcome::sni(1, 0, 0);
    }
    let offset = ((instr & 0xFF) as i8) as i32 * 2;
    regs.r[15] = (regs.r[15] as i32).wrapping_add(offset) as u32;
    ExecOutcome::branch_sni(2, 1, 0)
}

// ---------------------------------------------------------------------------
// Format 18 — Unconditional branch
// ---------------------------------------------------------------------------

fn exec_format18(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let offset11 = (instr & 0x7FF) as i32;
    // Sign extend 11 bits then shift left 1.
    let signed = ((offset11 << 21) >> 21) << 1;
    regs.r[15] = (regs.r[15] as i32).wrapping_add(signed) as u32;
    ExecOutcome::branch_sni(2, 1, 0)
}

// ---------------------------------------------------------------------------
// Format 19 — Long branch with link
// ---------------------------------------------------------------------------

fn exec_format19(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let h = (instr >> 11) & 1 != 0;
    let offset11 = (instr & 0x7FF) as u32;

    if !h {
        // First instruction (H=0): LR = PC + (offset << 12), sign-extended
        // Sign extend 11-bit offset to 23 bits (in upper position)
        let sign_ext = if offset11 & 0x400 != 0 {
            0xFFFF_F800 // negative
        } else {
            0
        };
        let offset = ((sign_ext | offset11) << 12) as i32;
        regs.r[14] = (regs.r[15] as i32).wrapping_add(offset) as u32;
        return ExecOutcome::sni(1, 0, 0);
    }

    // Second instruction (H=1): PC = LR + (offset << 1); LR = old_PC | 1
    let old_pc = regs.r[15].wrapping_sub(2); // PC of this instruction
    let target = regs.r[14].wrapping_add(offset11 << 1);
    regs.r[14] = old_pc | 1; // Set bit 0 to indicate return to Thumb
    regs.r[15] = target & !1;
    ExecOutcome::branch_sni(2, 1, 0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn add_flags(a: u32, b: u32) -> (u32, bool, bool) {
    let sum64 = a as u64 + b as u64;
    let result = sum64 as u32;
    let carry = sum64 > 0xFFFF_FFFF;
    let a_sign = a & 0x8000_0000;
    let b_sign = b & 0x8000_0000;
    let r_sign = result & 0x8000_0000;
    (result, carry, (a_sign == b_sign) && (a_sign != r_sign))
}

fn sub_flags(a: u32, b: u32) -> (u32, bool, bool) {
    let sum64 = a as u64 + (!b) as u64 + 1;
    let result = sum64 as u32;
    let carry = sum64 > 0xFFFF_FFFF;
    let a_sign = a & 0x8000_0000;
    let b_sign = b & 0x8000_0000;
    let r_sign = result & 0x8000_0000;
    (result, carry, (a_sign != b_sign) && (a_sign != r_sign))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::bus::GbaBus;
    use crate::gba::cpu::bus::RamBus;
    use crate::gba::cpu::registers::CpuMode;

    fn make_regs() -> Registers {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::User);
        regs.set_thumb(true);
        regs.cpsr &= !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V);
        regs.r[15] = 4; // simulate Thumb prefetch (PC + 4)
        regs
    }

    #[test]
    fn thumb_mov_imm() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // MOV R0, #42  -> 0010_0000_0010_1010
        let instr = 0b00100_000_00101010u16;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], 42);
        assert!(!regs.z_flag());
    }

    #[test]
    fn thumb_add_register_format2() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 1;
        regs.r[1] = 2;
        // ADD R2, R0, R1  -> 0001100_001_000_010
        let instr = 0b0001100_001_000_010u16;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[2], 3);
    }

    #[test]
    fn thumb_format4_lsl_by_zero_preserves_value() {
        // LSL with shift amount of 0 should preserve the value and carry flag.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xDEAD_BEEF;
        regs.r[1] = 0; // shift by 0
        regs.set_nzcv(true, true, true, true); // C=1 should be preserved
        let r0_before = regs.r[0];
        // LSL R0, R1: op=0x2, Rs=R1, Rd=R0
        let instr = 0b0100_00_0010_001_000u16;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], r0_before, "value preserved when shift=0");
        assert!(regs.c_flag(), "C flag preserved when shift=0");
        assert!(regs.n_flag(), "N flag set from negative result");
        assert!(!regs.z_flag(), "Z flag clear from non-zero result");
    }

    #[test]
    fn thumb_unconditional_branch() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x100 + 4;
        // B #+8 -> offset11 = 4 (shifted by 1 = 8)
        let instr = 0b11100_000_0000_0100u16;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[15], 0x100 + 4 + 8);
    }

    #[test]
    fn thumb_conditional_branch_skips_when_false() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x100;
        // BNE +4: cond=NE(1), offset=2 (shifted 1 = +4)
        let instr = 0b1101_0001_0000_0010u16;
        // Z=0 by default so NE is true → branch taken.
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[15], 0x104);

        // Now set Z=1 and ensure no branch occurs.
        let mut regs2 = make_regs();
        regs2.cpsr |= FLAG_Z;
        regs2.r[15] = 0x200;
        execute(&mut regs2, &mut bus, instr);
        assert_eq!(regs2.r[15], 0x200);
    }

    #[test]
    fn thumb_bx_to_arm() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 0x100; // bit 0 clear -> ARM
        // BX R1: 0100_0111_0_0001_000
        let instr = 0b0100_0111_0_0001_000u16;
        execute(&mut regs, &mut bus, instr);
        assert!(!regs.thumb());
        assert_eq!(regs.r[15], 0x100);
    }

    #[test]
    fn thumb_push_pop_round_trip() {
        // Test vector: PUSH {R0-R3, LR}; POP {R0-R3, PC} → registers restored.
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x400);
        regs.r[13] = 0x200;
        regs.r[0] = 0x11;
        regs.r[1] = 0x22;
        regs.r[2] = 0x33;
        regs.r[3] = 0x44;
        regs.r[14] = 0x80; // LR
        let sp_before = regs.r[13];

        // PUSH {R0-R3, LR} -> 1011_0_10_R_RRRR_RRRR with R-bit=1
        // 1011 010 1 0000_1111
        let push = 0b1011_0_10_1_0000_1111u16;
        execute(&mut regs, &mut bus, push);
        assert_eq!(regs.r[13], sp_before - 5 * 4);

        // Clobber registers, then POP {R0-R3, PC}.
        regs.r[0] = 0;
        regs.r[1] = 0;
        regs.r[2] = 0;
        regs.r[3] = 0;
        // POP {R0-R3, PC} -> 1011 110 1 0000_1111
        let pop = 0b1011_1_10_1_0000_1111u16;
        execute(&mut regs, &mut bus, pop);
        assert_eq!(regs.r[0], 0x11);
        assert_eq!(regs.r[1], 0x22);
        assert_eq!(regs.r[2], 0x33);
        assert_eq!(regs.r[3], 0x44);
        // PC was loaded from the saved LR (0x80), bit 0 cleared.
        assert_eq!(regs.r[15], 0x80);
        assert_eq!(regs.r[13], sp_before, "SP restored after pop");
    }

    #[test]
    fn thumb_pc_relative_load() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[15] = 0x100; // PC during execution
        bus.write32(0x108, 0xCAFE_BABE);
        // LDR R0, [PC, #4]: opcode 01001_000 imm=2 -> addr = (PC & ~2) + (2*4) = 0x108
        let instr = 0b01001_000_00000010u16;
        execute(&mut regs, &mut bus, instr);
        assert_eq!(regs.r[0], 0xCAFE_BABE);
    }

    #[test]
    fn cart_store_addr_helpers_preserve_lane_bits_only_for_cart_region() {
        assert_eq!(thumb_store_word_addr(0x0200_0003), 0x0200_0000);
        assert_eq!(thumb_store_halfword_addr(0x0200_0001), 0x0200_0000);

        assert_eq!(thumb_store_word_addr(0x0E00_0003), 0x0E00_0003);
        assert_eq!(thumb_store_halfword_addr(0x0F00_0001), 0x0F00_0001);
    }

    // -------------------------------------------------------------------------
    // Format 4 Missing ALU Operations Tests
    // -------------------------------------------------------------------------

    /// Build a Format 4 ALU instruction: 0100_00_op(4)_Rs(3)_Rd(3)
    fn thumb_alu_op(op: u8, rs: u8, rd: u8) -> u16 {
        0x4000 | ((op as u16 & 0xF) << 6) | ((rs as u16 & 0x7) << 3) | (rd as u16 & 0x7)
    }

    #[test]
    fn thumb_lsl_register() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x0000_0001;
        regs.r[1] = 4;
        // LSL R0, R1: Rd = Rd << Rs[7:0]
        execute(&mut regs, &mut bus, thumb_alu_op(0x2, 1, 0));
        assert_eq!(regs.r[0], 0x0000_0010);
    }

    #[test]
    fn thumb_lsr_register() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x8000_0000;
        regs.r[1] = 4;
        // LSR R0, R1: Rd = Rd >> Rs[7:0] (logical)
        execute(&mut regs, &mut bus, thumb_alu_op(0x3, 1, 0));
        assert_eq!(regs.r[0], 0x0800_0000);
    }

    #[test]
    fn thumb_asr_register() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x8000_0000u32; // negative
        regs.r[1] = 4;
        // ASR R0, R1: Rd = Rd >> Rs[7:0] (arithmetic)
        execute(&mut regs, &mut bus, thumb_alu_op(0x4, 1, 0));
        assert_eq!(regs.r[0], 0xF800_0000); // sign extended
    }

    #[test]
    fn thumb_adc() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 5;
        regs.r[1] = 3;
        regs.set_nzcv(false, false, true, false); // C=1
        // ADC R0, R1: Rd = Rd + Rs + C
        execute(&mut regs, &mut bus, thumb_alu_op(0x5, 1, 0));
        assert_eq!(regs.r[0], 9); // 5 + 3 + 1
    }

    #[test]
    fn thumb_sbc() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 10;
        regs.r[1] = 3;
        regs.set_nzcv(false, false, true, false); // C=1 (no borrow)
        // SBC R0, R1: Rd = Rd - Rs - !C
        execute(&mut regs, &mut bus, thumb_alu_op(0x6, 1, 0));
        assert_eq!(regs.r[0], 7); // 10 - 3 - 0
    }

    #[test]
    fn thumb_ror() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x0000_000F;
        regs.r[1] = 4;
        // ROR R0, R1: Rd = Rd ROR Rs[7:0]
        execute(&mut regs, &mut bus, thumb_alu_op(0x7, 1, 0));
        assert_eq!(regs.r[0], 0xF000_0000);
    }

    #[test]
    fn thumb_tst() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFF00;
        regs.r[1] = 0x00FF;
        // TST R0, R1: sets flags for Rd AND Rs
        execute(&mut regs, &mut bus, thumb_alu_op(0x8, 1, 0));
        assert!(regs.z_flag()); // 0xFF00 & 0x00FF = 0
        assert_eq!(regs.r[0], 0xFF00); // Rd unchanged
    }

    #[test]
    fn thumb_neg() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[1] = 5;
        // NEG R0, R1: Rd = 0 - Rs
        execute(&mut regs, &mut bus, thumb_alu_op(0x9, 1, 0));
        assert_eq!(regs.r[0], 0xFFFF_FFFBu32); // -5 in two's complement
    }

    #[test]
    fn thumb_cmn() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 1;
        regs.r[1] = 0xFFFF_FFFFu32; // -1
        // CMN R0, R1: sets flags for Rd + Rs (tests for negative)
        execute(&mut regs, &mut bus, thumb_alu_op(0xB, 1, 0));
        assert!(regs.z_flag()); // 1 + (-1) = 0
        assert_eq!(regs.r[0], 1); // Rd unchanged
    }

    #[test]
    fn thumb_mul() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 7;
        regs.r[1] = 6;
        // MUL R0, R1: Rd = Rd * Rs
        execute(&mut regs, &mut bus, thumb_alu_op(0xD, 1, 0));
        assert_eq!(regs.r[0], 42);
    }

    // -------------------------------------------------------------------------
    // Load/Store Format Tests
    // -------------------------------------------------------------------------

    #[test]
    fn thumb_format7_str_ldr_register_offset() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40; // base
        regs.r[1] = 4; // offset
        regs.r[2] = 0xDEAD_BEEF;
        // STR R2, [R0, R1]: 0101_00_0_Ro_Rb_Rd = 0101_000_001_000_010
        let str_instr = 0b0101_000_001_000_010u16;
        execute(&mut regs, &mut bus, str_instr);
        assert_eq!(bus.read32(0x44), 0xDEAD_BEEF);

        // LDR R3, [R0, R1]: 0101_10_0_Ro_Rb_Rd = 0101_100_001_000_011
        let ldr_instr = 0b0101_100_001_000_011u16;
        execute(&mut regs, &mut bus, ldr_instr);
        assert_eq!(regs.r[3], 0xDEAD_BEEF);
    }

    #[test]
    fn thumb_format7_ldr_from_sram_uses_unaligned_cart_bus_lane() {
        let mut bus = GbaBus::new();
        let base = 0x0E00_0000;
        bus.write8(base + 1, 0x61);
        bus.write8(base + 2, 0x6D);
        bus.write8(base + 3, 0x65);

        for (offset, expected) in [(1, 0x6161_6161), (2, 0x6D6D_6D6D), (3, 0x6565_6565)] {
            let mut regs = make_regs();
            regs.r[0] = base;
            regs.r[1] = offset;

            let ldr = 0b0101_100_001_000_010u16;
            execute(&mut regs, &mut bus, ldr);

            assert_eq!(regs.r[2], expected, "offset {offset}");
        }
    }

    #[test]
    fn thumb_format8_strh_ldrh() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 2;
        regs.r[2] = 0x1234;
        // STRH R2, [R0, R1]: 0101_001_Ro_Rb_Rd
        let strh = 0b0101_001_001_000_010u16;
        execute(&mut regs, &mut bus, strh);
        assert_eq!(bus.read16(0x42), 0x1234);

        // LDRH R3, [R0, R1]: 0101_101_Ro_Rb_Rd
        let ldrh = 0b0101_101_001_000_011u16;
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[3], 0x1234);
    }

    #[test]
    fn thumb_format8_misaligned_ldrh_rotates_and_ldrsh_is_signed_byte() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 1;

        bus.write16(0x40, 0x00FF);
        // LDRH R2, [R0, R1]
        let ldrh = 0b0101_101_001_000_010u16;
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[2], 0xFF00_0000);

        bus.write16(0x40, 0xFF00);
        // LDSH R3, [R0, R1]
        let ldrsh = 0b0101_111_001_000_011u16;
        execute(&mut regs, &mut bus, ldrsh);
        assert_eq!(regs.r[3], 0xFFFF_FFFF);
    }

    #[test]
    fn thumb_format8_ldrh_from_sram_uses_unaligned_cart_bus_lane() {
        let mut regs = make_regs();
        let mut bus = GbaBus::new();
        let base = 0x0E00_0000;
        regs.r[0] = base;
        regs.r[1] = 1;
        bus.write8(base, 0x47);
        bus.write8(base + 1, 0x61);

        let ldrh = 0b0101_101_001_000_010u16;
        execute(&mut regs, &mut bus, ldrh);

        assert_eq!(regs.r[2], 0x6100_0061);
    }

    #[test]
    fn thumb_format9_str_ldr_immediate() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 0xCAFE_BABE;
        // STR R1, [R0, #8]: 0110_0_imm5_Rb_Rd, imm5=2 (offset=8)
        let str_instr = 0b0110_0_00010_000_001u16;
        execute(&mut regs, &mut bus, str_instr);
        assert_eq!(bus.read32(0x48), 0xCAFE_BABE);

        // LDR R2, [R0, #8]: 0110_1_imm5_Rb_Rd
        let ldr_instr = 0b0110_1_00010_000_010u16;
        execute(&mut regs, &mut bus, ldr_instr);
        assert_eq!(regs.r[2], 0xCAFE_BABE);
    }

    #[test]
    fn thumb_format10_strh_ldrh() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        regs.r[1] = 0xABCD;
        // STRH R1, [R0, #4]: 1000_0_imm5_Rb_Rd, imm5=2 (offset=4)
        let strh = 0b1000_0_00010_000_001u16;
        execute(&mut regs, &mut bus, strh);
        assert_eq!(bus.read16(0x44), 0xABCD);

        // LDRH R2, [R0, #4]: 1000_1_imm5_Rb_Rd
        let ldrh = 0b1000_1_00010_000_010u16;
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[2], 0xABCD);
    }

    #[test]
    fn thumb_format10_misaligned_ldrh_rotates() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x41;
        bus.write16(0x40, 0x00FF);

        // LDRH R1, [R0, #0]
        let ldrh = 0b1000_1_00000_000_001u16;
        execute(&mut regs, &mut bus, ldrh);
        assert_eq!(regs.r[1], 0xFF00_0000);
    }

    #[test]
    fn thumb_format10_ldrh_from_sram_uses_unaligned_cart_bus_lane() {
        let mut regs = make_regs();
        let mut bus = GbaBus::new();
        let base = 0x0E00_0000;
        regs.r[0] = base + 1;
        bus.write8(base, 0x47);
        bus.write8(base + 1, 0x61);

        let ldrh = 0b1000_1_00000_000_001u16;
        execute(&mut regs, &mut bus, ldrh);

        assert_eq!(regs.r[1], 0x6100_0061);
    }

    #[test]
    fn thumb_format11_sp_relative() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[13] = 0x100; // SP
        regs.r[0] = 0x1234_5678;
        // STR R0, [SP, #8]: 1001_0_Rd_imm8, imm8=2 (offset=8)
        let str_instr = 0b1001_0_000_00000010u16;
        execute(&mut regs, &mut bus, str_instr);
        assert_eq!(bus.read32(0x108), 0x1234_5678);

        // LDR R1, [SP, #8]: 1001_1_Rd_imm8
        let ldr_instr = 0b1001_1_001_00000010u16;
        execute(&mut regs, &mut bus, ldr_instr);
        assert_eq!(regs.r[1], 0x1234_5678);
    }

    #[test]
    fn thumb_format12_load_address() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x100;
        regs.r[13] = 0x200;
        // ADD R0, PC, #8: 1010_0_Rd_imm8, imm8=2 (offset=8)
        let add_pc = 0b1010_0_000_00000010u16;
        execute(&mut regs, &mut bus, add_pc);
        assert_eq!(regs.r[0], 0x108); // PC & ~2 + 8

        // ADD R1, SP, #8: 1010_1_Rd_imm8
        let add_sp = 0b1010_1_001_00000010u16;
        execute(&mut regs, &mut bus, add_sp);
        assert_eq!(regs.r[1], 0x208);
    }

    #[test]
    fn thumb_format13_add_offset_sp() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[13] = 0x100;
        // ADD SP, #16: 1011_0000_0_imm7, imm7=4 (offset=16)
        let add_sp = 0b1011_0000_0_0000100u16;
        execute(&mut regs, &mut bus, add_sp);
        assert_eq!(regs.r[13], 0x110);

        // SUB SP, #8: 1011_0000_1_imm7, imm7=2
        let sub_sp = 0b1011_0000_1_0000010u16;
        execute(&mut regs, &mut bus, sub_sp);
        assert_eq!(regs.r[13], 0x108);
    }

    #[test]
    fn thumb_format15_stmia_ldmia() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[0] = 0x100;
        regs.r[1] = 0x11;
        regs.r[2] = 0x22;
        regs.r[3] = 0x33;
        // STMIA R0!, {R1-R3}: 1100_0_Rb_rlist = 1100_0_000_00001110
        let stmia = 0b1100_0_000_00001110u16;
        execute(&mut regs, &mut bus, stmia);
        assert_eq!(bus.read32(0x100), 0x11);
        assert_eq!(bus.read32(0x104), 0x22);
        assert_eq!(bus.read32(0x108), 0x33);
        assert_eq!(regs.r[0], 0x10C);

        // LDMIA R0!, {R4-R6}: 1100_1_Rb_rlist
        regs.r[0] = 0x100;
        let ldmia = 0b1100_1_000_01110000u16;
        execute(&mut regs, &mut bus, ldmia);
        assert_eq!(regs.r[4], 0x11);
        assert_eq!(regs.r[5], 0x22);
        assert_eq!(regs.r[6], 0x33);
    }

    #[test]
    fn thumb_format15_ldmia_empty_rlist_loads_pc_and_writes_back_64() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0x40;
        bus.write32(0x40, 0x80);

        // LDMIA R0!, {}
        let ldm_empty = 0b1100_1_000_00000000u16;
        let outcome = execute(&mut regs, &mut bus, ldm_empty);

        assert!(outcome.branched);
        assert_eq!(regs.r[15], 0x80);
        assert_eq!(regs.r[0], 0x80);
    }

    #[test]
    fn thumb_format15_stmia_base_in_rlist_non_first_stores_updated_base() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[0] = 0x11;
        regs.r[1] = 0x100;
        regs.r[2] = 0x22;
        regs.r[3] = 0x33;

        // STMIA R1!, {R0-R3}
        let stmia = 0b1100_0_001_00001111u16;
        execute(&mut regs, &mut bus, stmia);

        assert_eq!(bus.read32(0x100), 0x11);
        assert_eq!(bus.read32(0x104), 0x110);
        assert_eq!(bus.read32(0x108), 0x22);
        assert_eq!(bus.read32(0x10C), 0x33);
        assert_eq!(regs.r[1], 0x110);
    }

    #[test]
    fn thumb_format19_bl() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        // ARM pipeline: PC = current_instruction_address + 4
        // First instruction at 0x0FFC, so PC = 0x1000
        regs.r[15] = 0x1000;
        // BL to +0x100: two-part instruction
        // First part (H=0): set LR = PC + (offset << 12)
        let bl_hi = 0b1111_0_00000000000u16; // offset = 0
        execute(&mut regs, &mut bus, bl_hi);
        assert_eq!(regs.r[14], 0x1000); // LR = PC + (0 << 12) = 0x1000

        // Second instruction at 0x0FFE, so PC = 0x1002
        regs.r[15] = 0x1002;

        // Second part (H=1): jump, set LR to return address
        let bl_lo = 0b1111_1_00010000000u16; // offset = 0x80 << 1 = 0x100
        let outcome = execute(&mut regs, &mut bus, bl_lo);
        assert_eq!(regs.r[15], 0x1100); // Target = LR + (offset << 1) = 0x1000 + 0x100
        // LR = (PC - 2) | 1 = 0x1000 | 1 (next instruction address with Thumb bit)
        assert_eq!(regs.r[14] & !1, 0x1000);
        assert!(outcome.branched);
    }

    // -----------------------------------------------------------------------
    // ARMv5TE undefined instruction tests (Thumb)
    // -----------------------------------------------------------------------

    #[test]
    fn thumb_blx_triggers_undefined() {
        // BLX (Thumb, v5TE): 11101_xxxxxxxxxxx (top5 = 0b11101)
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr: u16 = 0b11101_00000000000; // BLX offset=0
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "Thumb BLX should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn thumb_bkpt_triggers_undefined() {
        // BKPT (Thumb, v5TE): 1011_1110_xxxxxxxx = 0xBExx
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        let instr: u16 = 0xBE00; // BKPT #0
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            outcome.undefined,
            "Thumb BKPT should trigger undefined on ARMv4T, got: {outcome:?}"
        );
    }

    #[test]
    fn thumb_valid_unconditional_branch_still_works() {
        // Format 18: 11100_xxxxxxxxxxx (top5 = 0b11100) - should NOT be undefined
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[15] = 0x100;
        let instr: u16 = 0b11100_00000000100; // B #8 (offset=4, <<1 = 8)
        let outcome = execute(&mut regs, &mut bus, instr);
        assert!(
            !outcome.undefined,
            "Unconditional B should NOT trigger undefined, got: {outcome:?}"
        );
    }

    // -----------------------------------------------------------------------
    // S/N/I cycle decomposition tests (GBATek Thumb timing)
    // -----------------------------------------------------------------------

    /// Thumb Format 1 (shift immediate): 1S per GBATek.
    #[test]
    fn thumb_format1_sni_is_1s() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x100);
        regs.r[0] = 0xFF;
        // LSL R1, R0, #2
        let instr: u16 = 0x0081; // 00000_00010_000_001
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "Thumb shift should be 1S");
        assert_eq!(outcome.nonseq, 0);
        assert_eq!(outcome.internal, 0);
    }

    /// Thumb Format 6 (PC-relative load): 1S + 1N + 1I per GBATek.
    #[test]
    fn thumb_pc_relative_load_sni_is_1s_1n_1i() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[15] = 0x100;
        bus.write32(0x100, 0xDEAD_BEEF);
        // LDR R0, [PC, #0]
        let instr: u16 = 0x4800; // 01001_000_00000000
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 1, "PC-relative LDR should be 1S(code)");
        assert_eq!(
            outcome.data_nonseq, 1,
            "PC-relative LDR should have 1N(data)"
        );
        assert_eq!(outcome.internal, 1, "PC-relative LDR should have 1I");
        assert_eq!(outcome.total_flat_cycles(), 3);
    }

    /// Thumb unconditional branch: 2S + 1N per GBATek.
    #[test]
    fn thumb_unconditional_branch_sni_is_2s_1n() {
        let mut regs = make_regs();
        let mut bus = RamBus::new(0x200);
        regs.r[15] = 0x100;
        // B #8 (offset=4, <<1 = 8)
        let instr: u16 = 0b11100_00000000100;
        let outcome = execute(&mut regs, &mut bus, instr);
        assert_eq!(outcome.seq, 2, "Thumb branch should be 2S");
        assert_eq!(outcome.nonseq, 1, "Thumb branch should have 1N");
        assert_eq!(outcome.internal, 0);
    }
}
