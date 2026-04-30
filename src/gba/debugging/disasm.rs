//! ARM7TDMI disassembler (ARM 32-bit and Thumb 16-bit).
//!
//! The disassembler is intentionally read-only — it produces human-readable
//! mnemonics for use in the debugger UI, CPU traces and trace-to-file
//! support. It mirrors the subset of instructions implemented by the
//! executor in [`crate::gba::cpu::arm`] and [`crate::gba::cpu::thumb`].
//!
//! Coverage:
//!
//! * **ARM**: data-processing (AND, EOR, SUB, RSB, ADD, ADC, SBC, RSC, TST,
//!   TEQ, CMP, CMN, ORR, MOV, BIC, MVN) with immediate or register-shifted
//!   operands, branch / branch-with-link (B/BL), branch and exchange (BX),
//!   single-data transfer (LDR/STR/LDRB/STRB) with immediate offset, and
//!   software interrupt (SWI). All 16 condition codes are honoured.
//! * **Thumb**: format 1 (LSL/LSR/ASR), format 2 (ADD/SUB), format 3
//!   (MOV/CMP/ADD/SUB immediate), format 4 (ALU register), format 5
//!   (high-register / BX), format 6 (PC-relative load), format 14
//!   (PUSH/POP), format 16 (conditional branch / SWI) and format 18
//!   (unconditional branch).
//!
//! Instructions outside this subset disassemble to `"<undefined>"` so that
//! debugger output remains useful even when the executor encounters an
//! unimplemented opcode.

// Match arms below intentionally enumerate the bit-patterns from the ARM /
// Thumb encoding tables for readability; suppress the lint that suggests
// rewriting them as numeric ranges.
#![allow(clippy::manual_range_patterns)]
// Test literals deliberately mirror instruction-encoding bit fields.
#![allow(clippy::unusual_byte_groupings)]

/// Mnemonic suffixes for the 16 ARM condition codes (cond field 0..=15).
const COND_SUFFIX: [&str; 16] = [
    "EQ", "NE", "CS", "CC", "MI", "PL", "VS", "VC", "HI", "LS", "GE", "LT", "GT", "LE", "", "NV",
];

/// Format an ARM 32-bit instruction as a human-readable mnemonic string.
///
/// `pc` is the address of the instruction word being disassembled (i.e. the
/// value the CPU loaded the word from), *not* the prefetched value.
pub fn disasm_arm(instr: u32, pc: u32) -> String {
    let cond = ((instr >> 28) & 0xF) as usize;
    let cond_str = COND_SUFFIX[cond];

    // BX: 0001_0010_1111_1111_1111_0001_xxxx
    if (instr & 0x0FFF_FFF0) == 0x012F_FF10 {
        let rm = instr & 0xF;
        return format!("BX{} R{}", cond_str, rm);
    }

    let bits_27_25 = (instr >> 25) & 0x7;
    match bits_27_25 {
        0b000 | 0b001 => disasm_arm_data_processing(instr, cond_str),
        0b010 | 0b011 => disasm_arm_single_data_transfer(instr, cond_str),
        0b101 => disasm_arm_branch(instr, pc, cond_str),
        0b111 => {
            if (instr >> 24) & 0xF == 0xF {
                let comment = instr & 0x00FF_FFFF;
                format!("SWI{} #0x{:06X}", cond_str, comment)
            } else {
                "<undefined>".to_string()
            }
        }
        _ => "<undefined>".to_string(),
    }
}

fn disasm_arm_data_processing(instr: u32, cond_str: &str) -> String {
    let opcode = ((instr >> 21) & 0xF) as usize;
    let s_bit = (instr >> 20) & 1 != 0;
    let rn = (instr >> 16) & 0xF;
    let rd = (instr >> 12) & 0xF;
    let i_bit = (instr >> 25) & 1 != 0;

    let mnemonic = [
        "AND", "EOR", "SUB", "RSB", "ADD", "ADC", "SBC", "RSC", "TST", "TEQ", "CMP", "CMN", "ORR",
        "MOV", "BIC", "MVN",
    ][opcode];

    // TST/TEQ/CMP/CMN always set flags; the S-bit suffix is suppressed so
    // disassembly matches the canonical mnemonic.
    let is_test = matches!(opcode, 0x8 | 0x9 | 0xA | 0xB);
    let s_suffix = if s_bit && !is_test { "S" } else { "" };

    let op2 = format_arm_operand2(instr, i_bit);

    match opcode {
        // <op>{cond} Rd, Rn, op2 — write to Rd and read Rn.
        0x0 | 0x1 | 0x2 | 0x3 | 0x4 | 0x5 | 0x6 | 0x7 | 0xC | 0xE => {
            format!(
                "{}{}{} R{}, R{}, {}",
                mnemonic, cond_str, s_suffix, rd, rn, op2
            )
        }
        // TST/TEQ/CMP/CMN: no Rd, only Rn vs op2.
        0x8 | 0x9 | 0xA | 0xB => {
            format!("{}{} R{}, {}", mnemonic, cond_str, rn, op2)
        }
        // MOV/MVN: only Rd, op2 (Rn is ignored).
        0xD | 0xF => {
            format!("{}{}{} R{}, {}", mnemonic, cond_str, s_suffix, rd, op2)
        }
        _ => unreachable!(),
    }
}

fn format_arm_operand2(instr: u32, i_bit: bool) -> String {
    if i_bit {
        let imm = instr & 0xFF;
        let rotate = ((instr >> 8) & 0xF) * 2;
        if rotate == 0 {
            format!("#0x{:X}", imm)
        } else {
            format!("#0x{:X}", imm.rotate_right(rotate))
        }
    } else {
        let rm = instr & 0xF;
        let shift_imm = (instr >> 4) & 1 == 0;
        let shift_type = (instr >> 5) & 0x3;
        let shift_name = ["LSL", "LSR", "ASR", "ROR"][shift_type as usize];
        if shift_imm {
            let amount = (instr >> 7) & 0x1F;
            // LSL #0 is a plain register reference.
            if amount == 0 && shift_type == 0 {
                format!("R{}", rm)
            } else {
                format!("R{}, {} #{}", rm, shift_name, amount)
            }
        } else {
            let rs = (instr >> 8) & 0xF;
            format!("R{}, {} R{}", rm, shift_name, rs)
        }
    }
}

fn disasm_arm_single_data_transfer(instr: u32, cond_str: &str) -> String {
    let i_bit = (instr >> 25) & 1 != 0;
    let p_bit = (instr >> 24) & 1 != 0; // pre-index when set
    let u_bit = (instr >> 23) & 1 != 0; // add when set
    let b_bit = (instr >> 22) & 1 != 0; // byte access when set
    let w_bit = (instr >> 21) & 1 != 0; // writeback when set
    let l_bit = (instr >> 20) & 1 != 0; // load when set
    let rn = (instr >> 16) & 0xF;
    let rd = (instr >> 12) & 0xF;

    let op = if l_bit { "LDR" } else { "STR" };
    let b_suffix = if b_bit { "B" } else { "" };

    let offset = if i_bit {
        // Register offset (with optional shift).
        let rm = instr & 0xF;
        let shift_type = (instr >> 5) & 0x3;
        let amount = (instr >> 7) & 0x1F;
        let sign = if u_bit { "" } else { "-" };
        if amount == 0 && shift_type == 0 {
            format!("{}R{}", sign, rm)
        } else {
            let shift_name = ["LSL", "LSR", "ASR", "ROR"][shift_type as usize];
            format!("{}R{}, {} #{}", sign, rm, shift_name, amount)
        }
    } else {
        // 12-bit immediate offset.
        let imm = instr & 0xFFF;
        let sign = if u_bit { "" } else { "-" };
        format!("#{}0x{:X}", sign, imm)
    };

    if p_bit {
        let writeback = if w_bit { "!" } else { "" };
        format!(
            "{}{}{} R{}, [R{}, {}]{}",
            op, cond_str, b_suffix, rd, rn, offset, writeback
        )
    } else {
        // Post-indexed; W bit selects user-mode access, but for normal
        // disassembly we just emit the post-index form.
        format!(
            "{}{}{} R{}, [R{}], {}",
            op, cond_str, b_suffix, rd, rn, offset
        )
    }
}

fn disasm_arm_branch(instr: u32, pc: u32, cond_str: &str) -> String {
    let link = (instr >> 24) & 1 != 0;
    let offset24 = (instr & 0x00FF_FFFF) as i32;
    let signed = ((offset24 << 8) >> 8) << 2;
    // Real ARM PC is 8 bytes ahead during execute.
    let target = pc.wrapping_add(8).wrapping_add(signed as u32);
    let mnemonic = if link { "BL" } else { "B" };
    format!("{}{} #0x{:08X}", mnemonic, cond_str, target)
}

/// Format a Thumb 16-bit instruction as a human-readable mnemonic string.
///
/// `pc` is the address of the instruction halfword being disassembled.
pub fn disasm_thumb(instr: u16, pc: u32) -> String {
    let top5 = instr >> 11;
    match top5 {
        0b00000 | 0b00001 | 0b00010 => disasm_thumb_format1(instr),
        0b00011 => disasm_thumb_format2(instr),
        0b00100 | 0b00101 | 0b00110 | 0b00111 => disasm_thumb_format3(instr),
        0b01000 | 0b01001 => {
            if instr & 0xFC00 == 0x4000 {
                disasm_thumb_format4(instr)
            } else if instr & 0xFC00 == 0x4400 {
                disasm_thumb_format5(instr)
            } else if instr & 0xF800 == 0x4800 {
                disasm_thumb_format6(instr, pc)
            } else {
                "<undefined>".to_string()
            }
        }
        0b10110 | 0b10111 => {
            if instr & 0xF600 == 0xB400 {
                disasm_thumb_format14(instr)
            } else {
                "<undefined>".to_string()
            }
        }
        0b11010 | 0b11011 => disasm_thumb_format16(instr, pc),
        0b11100 => disasm_thumb_format18(instr, pc),
        _ => "<undefined>".to_string(),
    }
}

fn disasm_thumb_format1(instr: u16) -> String {
    let op = (instr >> 11) & 0x3;
    let amount = (instr >> 6) & 0x1F;
    let rs = (instr >> 3) & 0x7;
    let rd = instr & 0x7;
    let mnemonic = ["LSL", "LSR", "ASR"][op as usize];
    format!("{} R{}, R{}, #{}", mnemonic, rd, rs, amount)
}

fn disasm_thumb_format2(instr: u16) -> String {
    let imm_flag = (instr >> 10) & 1 != 0;
    let op_sub = (instr >> 9) & 1 != 0;
    let rn_or_imm = (instr >> 6) & 0x7;
    let rs = (instr >> 3) & 0x7;
    let rd = instr & 0x7;
    let mnemonic = if op_sub { "SUB" } else { "ADD" };
    if imm_flag {
        format!("{} R{}, R{}, #{}", mnemonic, rd, rs, rn_or_imm)
    } else {
        format!("{} R{}, R{}, R{}", mnemonic, rd, rs, rn_or_imm)
    }
}

fn disasm_thumb_format3(instr: u16) -> String {
    let op = (instr >> 11) & 0x3;
    let rd = (instr >> 8) & 0x7;
    let imm = instr & 0xFF;
    let mnemonic = ["MOV", "CMP", "ADD", "SUB"][op as usize];
    format!("{} R{}, #{}", mnemonic, rd, imm)
}

fn disasm_thumb_format4(instr: u16) -> String {
    let op = (instr >> 6) & 0xF;
    let rs = (instr >> 3) & 0x7;
    let rd = instr & 0x7;
    let mnemonic = [
        "AND", "EOR", "LSL", "LSR", "ASR", "ADC", "SBC", "ROR", "TST", "NEG", "CMP", "CMN", "ORR",
        "MUL", "BIC", "MVN",
    ][op as usize];
    format!("{} R{}, R{}", mnemonic, rd, rs)
}

fn disasm_thumb_format5(instr: u16) -> String {
    let op = (instr >> 8) & 0x3;
    let h1 = (instr >> 7) & 1;
    let h2 = (instr >> 6) & 1;
    let rs = ((instr >> 3) & 0x7) | (h2 << 3);
    let rd = (instr & 0x7) | (h1 << 3);
    match op {
        0b00 => format!("ADD R{}, R{}", rd, rs),
        0b01 => format!("CMP R{}, R{}", rd, rs),
        0b10 => format!("MOV R{}, R{}", rd, rs),
        0b11 => format!("BX R{}", rs),
        _ => unreachable!(),
    }
}

fn disasm_thumb_format6(instr: u16, pc: u32) -> String {
    let rd = (instr >> 8) & 0x7;
    let imm = (instr & 0xFF) as u32;
    // PC bit 1 forced to zero, then add imm * 4. PC is already 4 ahead during execute.
    let base = (pc.wrapping_add(4)) & !0x2;
    let target = base.wrapping_add(imm << 2);
    format!("LDR R{}, [PC, #0x{:X}] ; =0x{:08X}", rd, imm << 2, target)
}

fn disasm_thumb_format14(instr: u16) -> String {
    let load = (instr >> 11) & 1 != 0;
    let extra = (instr >> 8) & 1 != 0;
    let reg_list = (instr & 0xFF) as u8;
    let mnemonic = if load { "POP" } else { "PUSH" };
    let extra_reg = if load { "PC" } else { "LR" };
    let mut regs: Vec<String> = (0..8)
        .filter(|i| reg_list & (1 << i) != 0)
        .map(|i| format!("R{}", i))
        .collect();
    if extra {
        regs.push(extra_reg.to_string());
    }
    format!("{} {{{}}}", mnemonic, regs.join(", "))
}

fn disasm_thumb_format16(instr: u16, pc: u32) -> String {
    let cond = ((instr >> 8) & 0xF) as usize;
    if cond == 0xF {
        let comment = instr & 0xFF;
        return format!("SWI #0x{:02X}", comment);
    }
    let offset = ((instr & 0xFF) as i8) as i32 * 2;
    // Thumb PC is 4 ahead during execute.
    let target = (pc.wrapping_add(4) as i32).wrapping_add(offset) as u32;
    format!("B{} #0x{:08X}", COND_SUFFIX[cond], target)
}

fn disasm_thumb_format18(instr: u16, pc: u32) -> String {
    let offset11 = (instr & 0x7FF) as i32;
    let signed = ((offset11 << 21) >> 21) << 1;
    let target = (pc.wrapping_add(4) as i32).wrapping_add(signed) as u32;
    format!("B #0x{:08X}", target)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // ARM
    // ---------------------------------------------------------------

    #[test]
    fn arm_data_processing_add_immediate() {
        // ADD R1, R2, #0x10  (cond=AL, I=1, opcode=0x4)
        // 1110_001_0100_0_0010_0001_0000_00010000
        let instr = 0xE282_1010;
        assert_eq!(disasm_arm(instr, 0), "ADD R1, R2, #0x10");
    }

    #[test]
    fn arm_data_processing_adds_uses_s_suffix() {
        // ADDS R0, R0, #1
        let instr = 0xE290_0001;
        assert_eq!(disasm_arm(instr, 0), "ADDS R0, R0, #0x1");
    }

    #[test]
    fn arm_data_processing_register_form() {
        // ADD R3, R4, R5
        // cond=1110, 000, opcode=0100, S=0, Rn=4, Rd=3, shifter=R5 (LSL #0)
        let instr = 0xE084_3005;
        assert_eq!(disasm_arm(instr, 0), "ADD R3, R4, R5");
    }

    #[test]
    fn arm_data_processing_register_with_shift() {
        // ADD R0, R1, R2, LSL #4
        // shifter = Rm=2, shift=000_(LSL), shift_imm bit 4 =0, amount=4
        let instr = 0xE081_0202;
        assert_eq!(disasm_arm(instr, 0), "ADD R0, R1, R2, LSL #4");
    }

    #[test]
    fn arm_data_processing_mov_immediate() {
        // MOV R0, #0x12
        let instr = 0xE3A0_0012;
        assert_eq!(disasm_arm(instr, 0), "MOV R0, #0x12");
    }

    #[test]
    fn arm_data_processing_movs_register() {
        // MOVS R0, R1
        let instr = 0xE1B0_0001;
        assert_eq!(disasm_arm(instr, 0), "MOVS R0, R1");
    }

    #[test]
    fn arm_data_processing_cmp_immediate_no_s_suffix() {
        // CMP R0, #5
        let instr = 0xE350_0005;
        assert_eq!(disasm_arm(instr, 0), "CMP R0, #0x5");
    }

    #[test]
    fn arm_data_processing_all_test_ops_render_without_s() {
        // TST R0, #1 / TEQ R0, #1 / CMP R0, #1 / CMN R0, #1
        assert_eq!(disasm_arm(0xE310_0001, 0), "TST R0, #0x1");
        assert_eq!(disasm_arm(0xE330_0001, 0), "TEQ R0, #0x1");
        assert_eq!(disasm_arm(0xE350_0001, 0), "CMP R0, #0x1");
        assert_eq!(disasm_arm(0xE370_0001, 0), "CMN R0, #0x1");
    }

    #[test]
    fn arm_data_processing_mvn_register() {
        // MVN R0, R1
        let instr = 0xE1E0_0001;
        assert_eq!(disasm_arm(instr, 0), "MVN R0, R1");
    }

    #[test]
    fn arm_branch_forward() {
        // B #0x10 from PC=0  -> offset24 = 0x000000 means target = pc+8.
        let instr = 0xEA00_0000;
        assert_eq!(disasm_arm(instr, 0), "B #0x00000008");
    }

    #[test]
    fn arm_branch_with_link_and_negative_offset() {
        // BL with offset24 = 0xFFFFFE -> signed = -8 -> target = pc+8-8 = pc.
        let instr = 0xEBFF_FFFE;
        assert_eq!(disasm_arm(instr, 0x100), "BL #0x00000100");
    }

    #[test]
    fn arm_branch_uses_condition_code() {
        // BEQ #0x10 (cond=0000=EQ) at pc=0 -> target = 8 + 0 = 8
        let instr = 0x0A00_0000;
        assert_eq!(disasm_arm(instr, 0), "BEQ #0x00000008");
    }

    #[test]
    fn arm_bx_register() {
        // BX R14
        let instr = 0xE12F_FF1E;
        assert_eq!(disasm_arm(instr, 0), "BX R14");
    }

    #[test]
    fn arm_ldr_pre_indexed_immediate() {
        // LDR R0, [R1, #4]  cond=1110, 010, P=1, U=1, B=0, W=0, L=1, Rn=1, Rd=0, imm12=4
        let instr = 0xE591_0004;
        assert_eq!(disasm_arm(instr, 0), "LDR R0, [R1, #0x4]");
    }

    #[test]
    fn arm_ldrb_pre_indexed_immediate() {
        // LDRB R0, [R1, #4]
        let instr = 0xE5D1_0004;
        assert_eq!(disasm_arm(instr, 0), "LDRB R0, [R1, #0x4]");
    }

    #[test]
    fn arm_str_post_indexed_immediate_negative() {
        // STR R0, [R1], #-4  P=0, U=0, L=0
        let instr = 0xE401_0004;
        assert_eq!(disasm_arm(instr, 0), "STR R0, [R1], #-0x4");
    }

    #[test]
    fn arm_ldr_with_writeback() {
        // LDR R0, [R1, #4]!  W=1
        let instr = 0xE5B1_0004;
        assert_eq!(disasm_arm(instr, 0), "LDR R0, [R1, #0x4]!");
    }

    #[test]
    fn arm_swi() {
        // SWI #0x123456
        let instr = 0xEF12_3456;
        assert_eq!(disasm_arm(instr, 0), "SWI #0x123456");
    }

    #[test]
    fn arm_undefined_opcode_class() {
        // bits_27_25 = 0b110 — coprocessor data transfer, not yet supported.
        let instr = 0xEC00_0000;
        assert_eq!(disasm_arm(instr, 0), "<undefined>");
    }

    // ---------------------------------------------------------------
    // Thumb
    // ---------------------------------------------------------------

    #[test]
    fn thumb_format1_lsl_immediate() {
        // LSL R0, R1, #2  -> 000_00_00010_001_000
        let instr = 0b00000_00010_001_000u16;
        assert_eq!(disasm_thumb(instr, 0), "LSL R0, R1, #2");
    }

    #[test]
    fn thumb_format1_asr_immediate() {
        // ASR R2, R3, #5
        let instr = 0b00010_00101_011_010u16;
        assert_eq!(disasm_thumb(instr, 0), "ASR R2, R3, #5");
    }

    #[test]
    fn thumb_format2_add_register() {
        // ADD R2, R0, R1 (imm=0, op_sub=0)
        let instr = 0b0001100_001_000_010u16;
        assert_eq!(disasm_thumb(instr, 0), "ADD R2, R0, R1");
    }

    #[test]
    fn thumb_format2_sub_immediate3() {
        // SUB R0, R1, #7 (imm=1, op_sub=1, imm3=7)
        let instr = 0b0001111_111_001_000u16;
        assert_eq!(disasm_thumb(instr, 0), "SUB R0, R1, #7");
    }

    #[test]
    fn thumb_format3_mov_immediate() {
        // MOV R0, #42
        let instr = 0b00100_000_00101010u16;
        assert_eq!(disasm_thumb(instr, 0), "MOV R0, #42");
    }

    #[test]
    fn thumb_format3_cmp_immediate() {
        // CMP R3, #10
        let instr = 0b00101_011_00001010u16;
        assert_eq!(disasm_thumb(instr, 0), "CMP R3, #10");
    }

    #[test]
    fn thumb_format4_and_register() {
        // AND R0, R1: 010000_0000_001_000
        let instr = 0b010000_0000_001_000u16;
        assert_eq!(disasm_thumb(instr, 0), "AND R0, R1");
    }

    #[test]
    fn thumb_format4_mul_register() {
        // MUL R2, R3: 010000_1101_011_010
        let instr = 0b010000_1101_011_010u16;
        assert_eq!(disasm_thumb(instr, 0), "MUL R2, R3");
    }

    #[test]
    fn thumb_format5_mov_high_register() {
        // MOV R8, R1  (op=10, h1=1, h2=0, Rs=1, Rd=0)
        let instr = 0b010001_10_10_001_000u16;
        assert_eq!(disasm_thumb(instr, 0), "MOV R8, R1");
    }

    #[test]
    fn thumb_format5_bx_register() {
        // BX R14  (op=11, h1=0, h2=1, Rs=110, Rd ignored)
        let instr = 0b010001_11_01_110_000u16;
        assert_eq!(disasm_thumb(instr, 0), "BX R14");
    }

    #[test]
    fn thumb_format6_pc_relative_load() {
        // LDR R0, [PC, #0x10]  imm = 4 -> imm*4 = 0x10
        let instr = 0b01001_000_00000100u16;
        // PC = 0 -> base = (0+4) & !2 = 4 -> target = 4 + 0x10 = 0x14
        let s = disasm_thumb(instr, 0);
        assert_eq!(s, "LDR R0, [PC, #0x10] ; =0x00000014");
    }

    #[test]
    fn thumb_format14_push_with_lr() {
        // PUSH {R0, R1, LR}  -> 1011_0_10_1_0000_0011
        let instr = 0b1011_0_10_1_0000_0011u16;
        assert_eq!(disasm_thumb(instr, 0), "PUSH {R0, R1, LR}");
    }

    #[test]
    fn thumb_format14_pop_with_pc() {
        // POP {R0, PC}
        let instr = 0b1011_1_10_1_0000_0001u16;
        assert_eq!(disasm_thumb(instr, 0), "POP {R0, PC}");
    }

    #[test]
    fn thumb_format14_push_no_extra() {
        // PUSH {R3, R4}
        let instr = 0b1011_0_10_0_0001_1000u16;
        assert_eq!(disasm_thumb(instr, 0), "PUSH {R3, R4}");
    }

    #[test]
    fn thumb_format16_conditional_branch() {
        // BEQ #0x10  (cond=0000=EQ, offset8=6 -> *2 = +12, pc+4+12 = 0x10)
        let instr = 0b1101_0000_0000_0110u16;
        assert_eq!(disasm_thumb(instr, 0), "BEQ #0x00000010");
    }

    #[test]
    fn thumb_format16_negative_branch() {
        // BNE with signed offset -2 (offset8 = 0xFF -> -1 *2 = -2). PC=4 -> 4+4-2 = 6
        let instr = 0b1101_0001_1111_1111u16;
        assert_eq!(disasm_thumb(instr, 4), "BNE #0x00000006");
    }

    #[test]
    fn thumb_format16_swi() {
        // SWI #0x12 -> 1101_1111_00010010
        let instr = 0b1101_1111_00010010u16;
        assert_eq!(disasm_thumb(instr, 0), "SWI #0x12");
    }

    #[test]
    fn thumb_format18_unconditional_branch() {
        // B #0xC  (offset11 = 4 -> *2 = 8, PC=0 -> pc+4+8 = 0xC)
        let instr = 0b11100_000_0000_0100u16;
        assert_eq!(disasm_thumb(instr, 0), "B #0x0000000C");
    }

    #[test]
    fn thumb_undefined_opcode() {
        // 0b11111_xxxxxxxxxxxx is BL/BLX prefix — not implemented yet.
        let instr = 0b11111_000_00000000u16;
        assert_eq!(disasm_thumb(instr, 0), "<undefined>");
    }
}
