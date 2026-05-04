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
        // Format 4 / 5 / 6: 010xx
        0b01000 | 0b01001 => {
            if instr & 0xFC00 == 0x4000 {
                // ALU register (format 4)
                exec_format4(regs, instr)
            } else if instr & 0xFC00 == 0x4400 {
                // Hi-register / BX (format 5)
                exec_format5(regs, instr)
            } else if instr & 0xF800 == 0x4800 {
                // PC-relative load (format 6)
                exec_format6(regs, bus, instr)
            } else {
                ExecOutcome {
                    cycles: 1,
                    branched: false,
                    swi: false,
                }
            }
        }
        // Format 14 (push/pop): 1011x10x
        0b10110 | 0b10111 => {
            if instr & 0xF600 == 0xB400 {
                exec_format14(regs, bus, instr)
            } else {
                ExecOutcome {
                    cycles: 1,
                    branched: false,
                    swi: false,
                }
            }
        }
        // Format 16 (cond branch): 1101xxxx
        0b11010 | 0b11011 => exec_format16(regs, instr),
        // Format 18 (uncond branch): 11100
        0b11100 => exec_format18(regs, instr),
        _ => ExecOutcome {
            cycles: 1,
            branched: false,
            swi: false,
        },
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
    ExecOutcome {
        cycles: 1,
        branched: false,
        swi: false,
    }
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
    ExecOutcome {
        cycles: 1,
        branched: false,
        swi: false,
    }
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
    ExecOutcome {
        cycles: 1,
        branched: false,
        swi: false,
    }
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
            let shift = (b & 0xFF) as u32;
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
            let shift = (b & 0xFF) as u32;
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
            let shift = (b & 0xFF) as u32;
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
            let shift = (b & 0xFF) as u32;
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
            let result = a.wrapping_mul(b);
            // C and V flags are destroyed (set to meaningless values) per ARM spec
            (result, regs.c_flag(), regs.v_flag(), true)
        }
        0xE => (a & !b, regs.c_flag(), regs.v_flag(), true), // BIC
        0xF => (!b, regs.c_flag(), regs.v_flag(), true),     // MVN
        _ => unreachable!(),
    };
    if write {
        regs.r[rd] = result;
    }
    regs.set_nzcv(result & 0x8000_0000 != 0, result == 0, carry, overflow);
    ExecOutcome {
        cycles: 1,
        branched: false,
        swi: false,
    }
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
                return ExecOutcome {
                    cycles: 3,
                    branched: true,
                    swi: false,
                };
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
                return ExecOutcome {
                    cycles: 3,
                    branched: true,
                    swi: false,
                };
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
            return ExecOutcome {
                cycles: 3,
                branched: true,
                swi: false,
            };
        }
        _ => unreachable!(),
    }
    ExecOutcome {
        cycles: 1,
        branched: false,
        swi: false,
    }
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
    ExecOutcome {
        cycles: 3,
        branched: false,
        swi: false,
    }
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

    if !load {
        // PUSH: SP decremented first, then write low → high regs at low → high addresses.
        let mut sp = regs.r[13].wrapping_sub(count * 4);
        regs.r[13] = sp;
        for i in 0..8 {
            if reg_list & (1 << i) != 0 {
                bus.write32(sp & !0x3, regs.r[i]);
                sp = sp.wrapping_add(4);
            }
        }
        if extra {
            // PUSH stores LR.
            bus.write32(sp & !0x3, regs.r[14]);
        }
    } else {
        // POP: read low → high regs from low → high addresses, then update SP.
        let mut sp = regs.r[13];
        for i in 0..8 {
            if reg_list & (1 << i) != 0 {
                regs.r[i] = bus.read32(sp & !0x3);
                sp = sp.wrapping_add(4);
            }
        }
        if extra {
            // POP loads PC and may switch state on bit 0.
            let value = bus.read32(sp & !0x3);
            sp = sp.wrapping_add(4);
            // Stay in Thumb state regardless of bit 0 on ARM7TDMI POP {PC}.
            regs.r[15] = value & !1;
            branched = true;
        }
        regs.r[13] = sp;
    }

    ExecOutcome {
        cycles: 3,
        branched,
        swi: false,
    }
}

// ---------------------------------------------------------------------------
// Format 16 — Conditional branch
// ---------------------------------------------------------------------------

fn exec_format16(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let cond = ((instr >> 8) & 0xF) as u8;
    if cond == 0xF {
        // SWI in Thumb is encoded here (1101_1111_xxxxxxxx).
        return ExecOutcome {
            cycles: 3,
            branched: true,
            swi: true,
        };
    }
    if !condition_met(regs.cpsr, cond) {
        return ExecOutcome {
            cycles: 1,
            branched: false,
            swi: false,
        };
    }
    let offset = ((instr & 0xFF) as i8) as i32 * 2;
    regs.r[15] = (regs.r[15] as i32).wrapping_add(offset) as u32;
    ExecOutcome {
        cycles: 3,
        branched: true,
        swi: false,
    }
}

// ---------------------------------------------------------------------------
// Format 18 — Unconditional branch
// ---------------------------------------------------------------------------

fn exec_format18(regs: &mut Registers, instr: u16) -> ExecOutcome {
    let offset11 = (instr & 0x7FF) as i32;
    // Sign extend 11 bits then shift left 1.
    let signed = ((offset11 << 21) >> 21) << 1;
    regs.r[15] = (regs.r[15] as i32).wrapping_add(signed) as u32;
    ExecOutcome {
        cycles: 3,
        branched: true,
        swi: false,
    }
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
}
