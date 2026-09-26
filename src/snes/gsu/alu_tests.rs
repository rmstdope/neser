//! ALU, shift, byte, multiply and prefix opcodes against fullsnes "SNES Cart GSU-n CPU ALU
//! Opcodes" and "JMP and Prefix Opcodes". Each program ends in STOP; NOP.

use super::core_tests::{PROGRAM, Rig};

const Z: u16 = 1 << 1;
const C: u16 = 1 << 2;
const S: u16 = 1 << 3;
const V: u16 = 1 << 4;
const FLAGS: u16 = Z | C | S | V;

/// Presets `regs`, runs `program` followed by STOP; NOP, and returns the rig.
fn run(regs: &[(u16, u16)], program: &[u8]) -> Rig {
    let mut code = program.to_vec();
    code.extend_from_slice(&[0x00, 0x01]);
    let mut rig = Rig::new(&code);
    for &(n, value) in regs {
        rig.set_reg(n, value);
    }
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    rig
}

fn flags(rig: &mut Rig) -> u16 {
    rig.sfr() & FLAGS
}

#[test]
fn add_register_sets_overflow_and_sign() {
    let mut rig = run(&[(0, 0x7FFF), (1, 0x0001)], &[0x51]); // ADD R1
    assert_eq!(rig.reg(0), 0x8000);
    assert_eq!(flags(&mut rig), V | S);
}

#[test]
fn add_immediate_carries_to_zero() {
    let mut rig = run(&[(0, 0xFFF1)], &[0x3E, 0x5F]); // ADD #15
    assert_eq!(rig.reg(0), 0x0000);
    assert_eq!(flags(&mut rig), C | Z);
}

#[test]
fn adc_adds_the_carry() {
    // ADD R1 leaves C=1 and R0=0; ADC R1 then gives 0 + 1 + 1.
    let mut rig = run(&[(0, 0xFFFF), (1, 0x0001)], &[0x51, 0x3D, 0x51]);
    assert_eq!(rig.reg(0), 0x0002);
    assert_eq!(flags(&mut rig), 0);
}

#[test]
fn adc_immediate_is_alt3() {
    let mut rig = run(&[(0, 0x0010)], &[0x3F, 0x53]); // ADC #3, carry clear
    assert_eq!(rig.reg(0), 0x0013);
}

#[test]
fn sub_register_reports_no_borrow_as_carry() {
    let mut rig = run(&[(0, 0x8000), (1, 0x0001)], &[0x61]); // SUB R1
    assert_eq!(rig.reg(0), 0x7FFF);
    assert_eq!(flags(&mut rig), V | C);
}

#[test]
fn sub_immediate_borrows() {
    let mut rig = run(&[(0, 0x0001)], &[0x3E, 0x62]); // SUB #2
    assert_eq!(rig.reg(0), 0xFFFF);
    assert_eq!(flags(&mut rig), S);
}

#[test]
fn sbc_subtracts_the_inverted_carry() {
    // SUB R1 borrows (C=0, R0=$FFFF); SBC R1 then gives $FFFF - 1 - 1.
    let mut rig = run(&[(0, 0x0000), (1, 0x0001)], &[0x61, 0x3D, 0x61]);
    assert_eq!(rig.reg(0), 0xFFFD);
    assert_eq!(flags(&mut rig), C | S);
}

#[test]
fn cmp_sets_flags_without_writing() {
    let mut rig = run(&[(0, 0x0005), (1, 0x0005)], &[0x3F, 0x61]); // CMP R1
    assert_eq!(rig.reg(0), 0x0005);
    assert_eq!(flags(&mut rig), Z | C);
}

#[test]
fn logic_ops() {
    let cases: [(&[u8], u16, u16); 7] = [
        (&[0x71], 0xF000, S),              // AND R1
        (&[0x3D, 0x71], 0x00F0, 0),        // BIC R1
        (&[0xC1], 0xFFF0, S),              // OR R1
        (&[0x3D, 0xC1], 0x0FF0, 0),        // XOR R1
        (&[0x3E, 0x7F], 0x0000, Z),        // AND #15
        (&[0x3F, 0x7F], 0xF0F0, S),        // BIC #15
        (&[0x4F], 0x0F0F, 0),              // NOT
    ];
    for (program, result, flag_bits) in cases {
        let mut rig = run(&[(0, 0xF0F0), (1, 0xFF00)], program);
        assert_eq!(rig.reg(0), result, "{program:02X?}");
        assert_eq!(flags(&mut rig), flag_bits, "{program:02X?}");
    }
    let mut rig = run(&[(0, 0xF0F0)], &[0x3E, 0xC3]); // OR #3
    assert_eq!(rig.reg(0), 0xF0F3);
    let mut rig = run(&[(0, 0xF0F0)], &[0x3F, 0xC3]); // XOR #3
    assert_eq!(rig.reg(0), 0xF0F3);
}

#[test]
fn shifts_and_rotates() {
    let mut rig = run(&[(0, 0x8001)], &[0x03]); // LSR
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x4000, C));
    let mut rig = run(&[(0, 0x8001)], &[0x96]); // ASR
    assert_eq!((rig.reg(0), flags(&mut rig)), (0xC000, C | S));
    // ROL through carry, carry set by a preceding ADD.
    let mut rig = run(&[(0, 0x8000), (1, 0xFFFF), (2, 0x0001)], &[0x21, 0x52, 0x04]);
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x0001, C));
    let mut rig = run(&[(0, 0x0001), (1, 0xFFFF), (2, 0x0001)], &[0x21, 0x52, 0x97]); // ROR
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x8000, C | S));
}

#[test]
fn div2_rounds_minus_one_to_zero() {
    let mut rig = run(&[(0, 0xFFFF)], &[0x3D, 0x96]);
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x0000, C | Z));
    let mut rig = run(&[(0, 0xFFFC)], &[0x3D, 0x96]);
    assert_eq!(rig.reg(0), 0xFFFE);
}

#[test]
fn inc_and_dec() {
    let mut rig = run(&[(1, 0xFFFF)], &[0xD1]);
    assert_eq!((rig.reg(1), flags(&mut rig)), (0x0000, Z));
    let mut rig = run(&[(1, 0x0000)], &[0xE1]);
    assert_eq!((rig.reg(1), flags(&mut rig)), (0xFFFF, S));
}

#[test]
fn byte_ops() {
    let mut rig = run(&[(0, 0x1234)], &[0x4D]); // SWAP
    assert_eq!(rig.reg(0), 0x3412);
    let mut rig = run(&[(0, 0x1280)], &[0x95]); // SEX
    assert_eq!((rig.reg(0), flags(&mut rig)), (0xFF80, S));
    let mut rig = run(&[(0, 0x1280)], &[0x9E]); // LOB: SF = bit 7
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x0080, S));
    let mut rig = run(&[(0, 0x8012)], &[0xC0]); // HIB: SF = bit 7
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x0080, S));
    let mut rig = run(&[(0, 0x00FF)], &[0x9E]);
    assert_eq!(flags(&mut rig), S);
}

#[test]
fn merge_combines_high_bytes_with_its_own_flags() {
    let mut rig = run(&[(7, 0x12F0), (8, 0x3400)], &[0x70]);
    assert_eq!(rig.reg(0), 0x1234);
    // C: result & $E0E0 != 0; Z: result & $F0F0 != 0 (fullsnes: "not set when zero!").
    assert_eq!(flags(&mut rig), C | Z);
    let mut rig = run(&[(7, 0x8000), (8, 0x0000)], &[0x70]);
    assert_eq!(flags(&mut rig), S | V | C | Z);
}

#[test]
fn byte_multiplies() {
    let mut rig = run(&[(0, 0x00FE), (1, 0x0003)], &[0x81]); // MULT R1: -2 * 3
    assert_eq!((rig.reg(0), flags(&mut rig)), (0xFFFA, S));
    let mut rig = run(&[(0, 0x00FE), (1, 0x0003)], &[0x3D, 0x81]); // UMULT R1: 254 * 3
    assert_eq!(rig.reg(0), 0x02FA);
    let mut rig = run(&[(0, 0x0005)], &[0x3E, 0x83]); // MULT #3
    assert_eq!(rig.reg(0), 0x000F);
    let mut rig = run(&[(0, 0x00FF)], &[0x3F, 0x82]); // UMULT #2
    assert_eq!(rig.reg(0), 0x01FE);
    let mut rig = run(&[(0, 0x1200), (1, 0x3400)], &[0x81]); // only the low bytes multiply
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x0000, Z));
}

#[test]
fn fmult_keeps_the_high_word() {
    let mut rig = run(&[(0, 0x4000), (6, 0x4000)], &[0x9F]);
    assert_eq!((rig.reg(0), flags(&mut rig)), (0x1000, 0));
}

#[test]
fn lmult_puts_the_low_word_in_r4_and_carry_is_its_bit_15() {
    // 1 * -32768 = $FFFF8000.
    let mut rig = run(&[(0, 0x0001), (6, 0x8000)], &[0x3D, 0x9F]);
    assert_eq!(rig.reg(0), 0xFFFF);
    assert_eq!(rig.reg(4), 0x8000);
    assert_eq!(flags(&mut rig), C | S);
}

#[test]
fn lmult_into_r4_leaves_the_high_word() {
    // fullsnes: "When using LMULT with Dreg=R4 then the result will be R4=MSB".
    let mut rig = run(&[(0, 0x0100), (6, 0x0100)], &[0x14, 0x3D, 0x9F]);
    assert_eq!(rig.reg(4), 0x0001);
}

#[test]
fn from_and_to_select_source_and_destination() {
    let mut rig = run(&[(1, 0x0005)], &[0xB1, 0x12, 0x51]); // FROM R1; TO R2; ADD R1
    assert_eq!(rig.reg(2), 0x000A);
    assert_eq!(rig.reg(0), 0x0000);
}

#[test]
fn with_selects_both_registers() {
    let mut rig = run(&[(1, 0x0003), (2, 0x0004)], &[0x22, 0x51]); // WITH R2; ADD R1
    assert_eq!(rig.reg(2), 0x0007);
}

#[test]
fn with_sets_b_so_next_1n_is_move() {
    let mut rig = run(&[(1, 0xBEEF)], &[0x21, 0x12]); // MOVE R2,R1
    assert_eq!(rig.reg(2), 0xBEEF);
    assert_eq!(rig.reg(0), 0x0000);
    assert_eq!(flags(&mut rig), 0, "MOVE leaves flags alone");
}

#[test]
fn from_after_with_is_moves_with_ov_from_bit7() {
    let mut rig = run(&[(1, 0x0080)], &[0x22, 0xB1]); // MOVES R2,R1
    assert_eq!(rig.reg(2), 0x0080);
    assert_eq!(flags(&mut rig), V);
    let mut rig = run(&[(1, 0x0000)], &[0x22, 0xB1]);
    assert_eq!(flags(&mut rig), Z);
}

#[test]
fn prefixes_are_reset_by_every_other_opcode() {
    // ALT1; NOP; ADD R1 is a plain ADD, and TO R2; NOP; ADD R1 writes R0.
    let mut rig = run(&[(0, 0xFFFF), (1, 0x0001)], &[0x51, 0x3D, 0x01, 0x51]);
    assert_eq!(rig.reg(0), 0x0001, "ADD, not ADC");
    let mut rig = run(&[(1, 0x0001)], &[0x12, 0x01, 0x51]);
    assert_eq!((rig.reg(0), rig.reg(2)), (0x0001, 0x0000));
}

#[test]
fn alt3_falls_back_to_alt1_then_plain() {
    // $3F $96 does not exist, so it acts as $3D $96 (DIV2).
    let mut rig = run(&[(0, 0xFFFF)], &[0x3F, 0x96]);
    assert_eq!(rig.reg(0), 0x0000);
    // $3E $96 does not exist and ALT2 has no ALT1 to fall back on: plain ASR.
    let mut rig = run(&[(0, 0xFFFF)], &[0x3E, 0x96]);
    assert_eq!(rig.reg(0), 0xFFFF);
}
