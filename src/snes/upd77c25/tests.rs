//! uPD77C25 unit tests, each pinned to a row of fullsnes "SNES Cart DSP-n/ST010/ST011 - NEC
//! uPD77C25" (Registers & Flags, ALU and LD Instructions, JP Instructions). Where fullsnes is
//! silent the test names Mesen2 `NecDsp.cpp` as its source.

use super::asm::*;
use super::*;
use crate::snes::ppu::SnesVideoRegion;
use std::rc::Rc;

/// Builds a chip whose program ROM starts with `program` (the rest `nop`) and whose data ROM
/// holds `data` at the bottom.
fn chip_with(program: &[u32], data: &[u16]) -> Upd77c25 {
    let mut prog = vec![NOP; PROGRAM_WORDS];
    prog[..program.len()].copy_from_slice(program);
    let mut data_rom = vec![0u16; DATA_WORDS];
    data_rom[..data.len()].copy_from_slice(data);
    let firmware = Upd77c25Firmware::from_words(prog, data_rom);
    Upd77c25::new(Rc::new(firmware), SnesVideoRegion::Ntsc)
}

fn chip(program: &[u32]) -> Upd77c25 {
    chip_with(program, &[])
}

/// Executes `n` instructions.
fn run(chip: &mut Upd77c25, n: usize) {
    for _ in 0..n {
        chip.step();
    }
}

// ---------------------------------------------------------------- firmware images

fn image_le(program: &[u32], data: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DSP_IMAGE_SIZE);
    for i in 0..PROGRAM_WORDS {
        let op = program.get(i).copied().unwrap_or(0);
        bytes.extend_from_slice(&[op as u8, (op >> 8) as u8, (op >> 16) as u8]);
    }
    for i in 0..DATA_WORDS {
        let w = data.get(i).copied().unwrap_or(0);
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    bytes
}

#[test]
fn from_image_splits_little_endian_program_and_data() {
    let jrqm = jp(JRQM, 0);
    let bytes = image_le(&[jrqm, 0x123456], &[0xBEEF, 0x0102]);
    assert_eq!(bytes.len(), DSP_IMAGE_SIZE);
    let fw = Upd77c25Firmware::from_image(&bytes).expect("8192 bytes parse");
    assert_eq!(fw.program[0], jrqm);
    assert_eq!(fw.program[1], 0x123456);
    assert_eq!(fw.data[0], 0xBEEF);
    assert_eq!(fw.data[1], 0x0102);
}

#[test]
fn from_image_detects_big_endian_by_jrqm() {
    // fullsnes "ROM-Images": old files store each opcode big-endian ("97h,C0h,0xh"); every real
    // ROM has "JRQM $" within its first four opcodes.
    let program = [NOP, jp(JRQM, 1), 0x00ABCD];
    let mut bytes = Vec::new();
    for i in 0..PROGRAM_WORDS {
        let op = program.get(i).copied().unwrap_or(0);
        bytes.extend_from_slice(&[(op >> 16) as u8, (op >> 8) as u8, op as u8]);
    }
    for i in 0..DATA_WORDS {
        bytes.extend_from_slice(&(i as u16).to_be_bytes());
    }
    let fw = Upd77c25Firmware::from_image(&bytes).expect("parse");
    assert_eq!(fw.program[1], jp(JRQM, 1));
    assert_eq!(fw.program[2], 0x00ABCD);
    assert_eq!(fw.data[5], 5);
}

#[test]
fn to_le_image_round_trips_both_orders() {
    // The little-endian ("newer") layout: 2048 opcodes of 3 bytes LSB first, then 1024 data
    // words LSB first. A big-endian image converts to exactly the little-endian one.
    let program = [NOP, jp(JRQM, 1), 0x00ABCD];
    let mut le = Vec::new();
    let mut be = Vec::new();
    for i in 0..PROGRAM_WORDS {
        let op = program.get(i).copied().unwrap_or(0);
        le.extend_from_slice(&[op as u8, (op >> 8) as u8, (op >> 16) as u8]);
        be.extend_from_slice(&[(op >> 16) as u8, (op >> 8) as u8, op as u8]);
    }
    for i in 0..DATA_WORDS {
        le.extend_from_slice(&(i as u16 * 3).to_le_bytes());
        be.extend_from_slice(&(i as u16 * 3).to_be_bytes());
    }
    assert_eq!(Upd77c25Firmware::from_image(&le).unwrap().to_le_image(), le);
    assert_eq!(Upd77c25Firmware::from_image(&be).unwrap().to_le_image(), le);
}

#[test]
fn from_image_rejects_other_sizes_reporting_size() {
    assert_eq!(
        Upd77c25Firmware::from_image(&[0u8; 12288]).err(),
        Some(12288)
    );
    assert_eq!(Upd77c25Firmware::from_image(&[]).err(), Some(0));
    // The 10 KB padded "oldest" format is not accepted either.
    assert_eq!(
        Upd77c25Firmware::from_image(&[0u8; 10240]).err(),
        Some(10240)
    );
}

// ---------------------------------------------------------------- LD and move

#[test]
fn ld_writes_each_destination() {
    let mut c = chip(&[
        ld(DST_A, 0x1111),
        ld(DST_B, 0x2222),
        ld(DST_TR, 0x3333),
        ld(DST_DP, 0x0044),
        ld(DST_RP, 0x0155),
        ld(DST_K, 0x0006),
        ld(DST_L, 0x0007),
        ld(DST_TRB, 0x8888),
        ld(DST_MEM, 0x9999),
        ld(DST_SO, 0xAAAA),
    ]);
    run(&mut c, 10);
    let s = &c.state;
    assert_eq!(
        (s.a, s.b, s.tr, s.dp, s.rp),
        (0x1111, 0x2222, 0x3333, 0x44, 0x155)
    );
    assert_eq!((s.k, s.l, s.trb, s.so), (6, 7, 0x8888, 0xAAAA));
    assert_eq!(s.ram[0x44], 0x9999);
}

#[test]
fn ld_to_dr_sets_rqm() {
    let mut c = chip(&[ld(DST_DR, 0x1234)]);
    run(&mut c, 1);
    assert_eq!(c.state.dr, 0x1234);
    assert_ne!(c.state.sr & SR_RQM, 0);
}

#[test]
fn ld_to_sr_keeps_read_only_bits() {
    // Mesen2 `Load` case 0x07: RQM, DRS and bits 6-2 survive a write.
    let mut c = chip(&[ld(DST_DR, 0), ld(DST_SR, 0x6FFF)]);
    run(&mut c, 2);
    assert_eq!(c.state.sr, SR_RQM | (0x6FFF & !0x907C));
}

#[test]
fn alu_move_copies_src_to_dst() {
    let mut c = chip(&[ld(DST_A, 0x4321), alu(AluOp::Nop, ALU_A, SRC_A, DST_B)]);
    run(&mut c, 2);
    assert_eq!(c.state.b, 0x4321);
}

// ---------------------------------------------------------------- ALU

/// Runs `op` with AccA=`acc` and P=IDB(`p` via TR), returning (A, flags A).
fn alu_a(op: AluOp, acc: u16, p: u16) -> (u16, Flags) {
    let mut c = chip(&[
        ld(DST_A, acc),
        ld(DST_TR, p),
        alu_p(op, P_IDB, ALU_A, SRC_TR, DST_NON),
    ]);
    run(&mut c, 3);
    (c.state.a, c.state.flags_a)
}

fn f(s0: bool, z: bool, c: bool, ov0: bool) -> (bool, bool, bool, bool) {
    (s0, z, c, ov0)
}

fn sf(flags: Flags) -> (bool, bool, bool, bool) {
    (flags.s0, flags.z, flags.c, flags.ov0)
}

#[test]
fn alu_logic_results_and_flags() {
    let (r, fl) = alu_a(AluOp::Or, 0x8001, 0x0100);
    assert_eq!((r, sf(fl)), (0x8101, f(true, false, false, false)));
    let (r, fl) = alu_a(AluOp::And, 0x00F0, 0x0F00);
    assert_eq!((r, sf(fl)), (0x0000, f(false, true, false, false)));
    let (r, fl) = alu_a(AluOp::Xor, 0xFFFF, 0x0FFF);
    assert_eq!((r, sf(fl)), (0xF000, f(true, false, false, false)));
    let (r, fl) = alu_a(AluOp::Not, 0x00FF, 0);
    assert_eq!((r, sf(fl)), (0xFF00, f(true, false, false, false)));
}

#[test]
fn alu_add_sub_results_and_flags() {
    let (r, fl) = alu_a(AluOp::Add, 0x7FFF, 0x0001);
    assert_eq!((r, sf(fl)), (0x8000, f(true, false, false, true)));
    let (r, fl) = alu_a(AluOp::Add, 0xFFFF, 0x0001);
    assert_eq!((r, sf(fl)), (0x0000, f(false, true, true, false)));
    let (r, fl) = alu_a(AluOp::Sub, 0x0000, 0x0001);
    assert_eq!((r, sf(fl)), (0xFFFF, f(true, false, true, false)));
    let (r, fl) = alu_a(AluOp::Sub, 0x8000, 0x0001);
    assert_eq!((r, sf(fl)), (0x7FFF, f(false, false, false, true)));
    let (r, fl) = alu_a(AluOp::Inc, 0x7FFF, 0);
    assert_eq!((r, sf(fl)), (0x8000, f(true, false, false, true)));
    let (r, fl) = alu_a(AluOp::Dec, 0x0000, 0);
    assert_eq!((r, sf(fl)), (0xFFFF, f(true, false, true, false)));
}

#[test]
fn alu_carry_ops_use_the_other_accumulators_carry() {
    // fullsnes: "OtherCy is the incoming carry flag from other accumulator".
    let mut c = chip(&[
        ld(DST_B, 0xFFFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_B, SRC_TR, DST_NON), // CB = 1
        ld(DST_A, 0x0010),
        alu_p(AluOp::Adc, P_IDB, ALU_A, SRC_TR, DST_NON), // A = 0x10 + 1 + CB
    ]);
    run(&mut c, 5);
    assert_eq!(c.state.a, 0x0012);

    let mut c = chip(&[
        ld(DST_B, 0x0000),
        ld(DST_TR, 1),
        alu_p(AluOp::Sub, P_IDB, ALU_B, SRC_TR, DST_NON), // CB = 1 (borrow)
        ld(DST_A, 0x0010),
        alu_p(AluOp::Sbb, P_IDB, ALU_A, SRC_TR, DST_NON), // A = 0x10 - 1 - CB
    ]);
    run(&mut c, 5);
    assert_eq!(c.state.a, 0x000E);
}

#[test]
fn alu_shifts_and_exchange() {
    let (r, fl) = alu_a(AluOp::Sar1, 0x8003, 0);
    assert_eq!((r, fl.c), (0xC001, true));
    let (r, _) = alu_a(AluOp::Sll2, 0x0001, 0);
    assert_eq!(r, 0x0007);
    let (r, _) = alu_a(AluOp::Sll4, 0x0001, 0);
    assert_eq!(r, 0x001F);
    let (r, _) = alu_a(AluOp::Xchg, 0x12AB, 0);
    assert_eq!(r, 0xAB12);

    // RCL1 shifts in the other accumulator's carry and takes bit 15 out.
    let mut c = chip(&[
        ld(DST_B, 0xFFFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_B, SRC_TR, DST_NON), // CB = 1
        ld(DST_A, 0x8000),
        alu(AluOp::Rcl1, ALU_A, SRC_TRB, DST_NON),
    ]);
    run(&mut c, 5);
    assert_eq!((c.state.a, c.state.flags_a.c), (0x0001, true));
}

#[test]
fn ov1_s1_track_overflow_direction() {
    // fullsnes: after one overflow OV1 is odd (1) and S1 holds its direction; a second overflow
    // in the opposite direction makes OV1 even again (the flag model is Mesen2's, which ares
    // shares; see the module docs).
    let mut c = chip(&[
        ld(DST_A, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON), // +overflow
    ]);
    run(&mut c, 3);
    assert!(c.state.flags_a.ov1);
    assert!(
        c.state.flags_a.s1,
        "S1 records the positive overflow's sign (result 8000h)"
    );

    let mut c = chip(&[
        ld(DST_A, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON), // 8000h, overflow up
        alu_p(AluOp::Sub, P_IDB, ALU_A, SRC_TR, DST_NON), // 7FFFh, overflow back down
    ]);
    run(&mut c, 4);
    assert!(!c.state.flags_a.ov1, "two opposite overflows cancel");

    let (_, fl) = alu_a(AluOp::Or, 0, 0);
    assert!(!fl.ov1 && !fl.ov0);
}

#[test]
fn sgn_source_is_8000_minus_sa1() {
    let mut c = chip(&[
        ld(DST_A, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON), // SA1 = 1
        alu(AluOp::Nop, ALU_A, SRC_SGN, DST_B),
    ]);
    run(&mut c, 4);
    assert_eq!(c.state.b, 0x7FFF);

    let mut c = chip(&[alu(AluOp::Nop, ALU_A, SRC_SGN, DST_B)]);
    run(&mut c, 1);
    assert_eq!(c.state.b, 0x8000);
}

#[test]
fn alu_p_select_reads_ram_and_multiplier() {
    // P=0: RAM[DP]; P=2: K*L*2/10000h; P=3: K*L*2 (low 16 bits).
    let mut c = chip(&[
        ld(DST_DP, 0x12),
        ld(DST_MEM, 0x0040),
        ld(DST_A, 0x0002),
        alu_p(AluOp::Add, P_RAM, ALU_A, SRC_TRB, DST_NON),
    ]);
    run(&mut c, 4);
    assert_eq!(c.state.a, 0x0042);

    let mut c = chip(&[
        ld(DST_K, 0x4000),
        ld(DST_L, 0x0003),
        alu_p(AluOp::Or, P_M, ALU_A, SRC_TRB, DST_NON),
        alu_p(AluOp::Or, P_N, ALU_B, SRC_TRB, DST_NON),
    ]);
    run(&mut c, 4);
    // 4000h * 3 * 2 = 18000h
    assert_eq!((c.state.a, c.state.b), (0x0001, 0x8000));
}

#[test]
fn multiplier_is_signed_and_updates_after_each_instruction() {
    let mut c = chip(&[ld(DST_K, 0xFFFF), ld(DST_L, 0x0002)]);
    run(&mut c, 2);
    // -1 * 2 * 2 = -4 = FFFFFFFCh
    assert_eq!((c.state.m, c.state.n), (0xFFFF, 0xFFFC));
}

#[test]
fn dp_adjust_dpl_dph_and_dst_dp_suppresses_it() {
    let mut c = chip(&[
        ld(DST_DP, 0x1F),
        alu_dp(DPL_INC, 0, DST_NON), // low nibble wraps without carry
        alu_dp(DPL_DEC, 0, DST_NON),
        alu_dp(DPL_CLR, 0x3, DST_NON), // clear low, XOR high with 3
    ]);
    run(&mut c, 2);
    assert_eq!(c.state.dp, 0x10);
    run(&mut c, 1);
    assert_eq!(c.state.dp, 0x1F);
    run(&mut c, 1);
    assert_eq!(c.state.dp, 0x20);

    // An instruction whose destination is DP does not also adjust it (Mesen2 `ExecOp`).
    let mut c = chip(&[
        ld(DST_TR, 0x55),
        with_dpl(alu(AluOp::Nop, ALU_A, SRC_TR, DST_DP), DPL_INC),
    ]);
    run(&mut c, 2);
    assert_eq!(c.state.dp, 0x55);
}

#[test]
fn rp_decrement_and_dst_rp_suppresses_it() {
    let mut c = chip_with(
        &[
            ld(DST_RP, 2),
            with_rpdec(alu(AluOp::Nop, ALU_A, SRC_RO, DST_A)),
            alu(AluOp::Nop, ALU_A, SRC_RO, DST_B),
        ],
        &[0xAAAA, 0xBBBB, 0xCCCC],
    );
    run(&mut c, 3);
    assert_eq!((c.state.a, c.state.b, c.state.rp), (0xCCCC, 0xBBBB, 1));

    let mut c = chip(&[
        ld(DST_TR, 9),
        with_rpdec(alu(AluOp::Nop, ALU_A, SRC_TR, DST_RP)),
    ]);
    run(&mut c, 2);
    assert_eq!(c.state.rp, 9);
}

#[test]
fn reset_rp_is_3ff_and_ro_reads_data_rom_at_rp() {
    let mut data = vec![0u16; DATA_WORDS];
    data[0x3FF] = 0x5A5A;
    let mut c = chip_with(&[alu(AluOp::Nop, ALU_A, SRC_RO, DST_A)], &data);
    assert_eq!(c.state.rp, 0x3FF);
    run(&mut c, 1);
    assert_eq!(c.state.a, 0x5A5A);
}

#[test]
fn klr_klm_load_pairs() {
    // @KLR: K=SRC, L=ROM[RP]. @KLM: L=SRC, K=RAM[DP OR 40h].
    let mut c = chip_with(
        &[
            ld(DST_RP, 1),
            ld(DST_TR, 0x0123),
            alu(AluOp::Nop, ALU_A, SRC_TR, DST_KLR),
        ],
        &[0, 0x0456],
    );
    run(&mut c, 3);
    assert_eq!((c.state.k, c.state.l), (0x0123, 0x0456));

    let mut c = chip(&[
        ld(DST_DP, 0x45),
        ld(DST_MEM, 0x0777),
        ld(DST_DP, 0x05),
        ld(DST_TR, 0x0888),
        alu(AluOp::Nop, ALU_A, SRC_TR, DST_KLM),
    ]);
    run(&mut c, 5);
    assert_eq!((c.state.k, c.state.l), (0x0777, 0x0888));
}

#[test]
fn sources_read_registers() {
    let mut c = chip(&[
        ld(DST_K, 0x0101),
        ld(DST_L, 0x0202),
        ld(DST_DP, 0x03),
        ld(DST_RP, 0x004),
        alu(AluOp::Nop, ALU_A, SRC_K, DST_A),
        alu(AluOp::Nop, ALU_A, SRC_L, DST_B),
        alu(AluOp::Nop, ALU_A, SRC_DP, DST_TR),
        alu(AluOp::Nop, ALU_A, SRC_RP, DST_TRB),
    ]);
    run(&mut c, 8);
    assert_eq!(
        (c.state.a, c.state.b, c.state.tr, c.state.trb),
        (0x0101, 0x0202, 0x03, 0x004)
    );
}

// ---------------------------------------------------------------- JP

#[test]
fn jp_every_condition_taken_and_not() {
    // (branch when true, branch when false, setup that makes the condition true)
    let carry_a = vec![
        ld(DST_A, 0xFFFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON),
    ];
    let carry_b = vec![
        ld(DST_B, 0xFFFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_B, SRC_TR, DST_NON),
    ];
    let zero_a = vec![alu(AluOp::Xor, ALU_A, SRC_A, DST_NON)];
    let zero_b = vec![alu(AluOp::Xor, ALU_B, SRC_B, DST_NON)];
    let ov_a = vec![
        ld(DST_A, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON),
    ];
    let ov_b = vec![
        ld(DST_B, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_B, SRC_TR, DST_NON),
    ];
    let sign_a = vec![
        ld(DST_TR, 0x8000),
        alu_p(AluOp::Or, P_IDB, ALU_A, SRC_TR, DST_NON),
    ];
    let sign_b = vec![
        ld(DST_TR, 0x8000),
        alu_p(AluOp::Or, P_IDB, ALU_B, SRC_TR, DST_NON),
    ];
    let dpl0 = vec![ld(DST_DP, 0x20)];
    let dplf = vec![ld(DST_DP, 0x2F)];
    let rqm = vec![ld(DST_DR, 0)];
    let sic = vec![ld(DST_SR, 0x0100)];
    let soc = vec![ld(DST_SR, 0x0200)];
    let cases: Vec<(u16, u16, Vec<u32>)> = vec![
        (JCA, JNCA, carry_a),
        (JCB, JNCB, carry_b),
        (JZA, JNZA, zero_a),
        (JZB, JNZB, zero_b),
        (JOVA0, JNOVA0, ov_a.clone()),
        (JOVB0, JNOVB0, ov_b.clone()),
        (JOVA1, JNOVA1, ov_a.clone()),
        (JOVB1, JNOVB1, ov_b.clone()),
        (JSA0, JNSA0, sign_a.clone()),
        (JSB0, JNSB0, sign_b.clone()),
        (JSA1, JNSA1, sign_a),
        (JSB1, JNSB1, sign_b),
        (JDPL0, JDPLN0, dpl0),
        (JDPLF, JDPLNF, dplf),
        (JRQM, JNRQM, rqm),
        (JSIAK, JNSIAK, sic),
        (JSOAK, JNSOAK, soc),
    ];
    for (yes, no, setup) in cases {
        for (brch, expect_taken) in [(yes, true), (no, false)] {
            let mut program = setup.clone();
            program.push(jp(brch, 0x400));
            let mut c = chip(&program);
            run(&mut c, program.len());
            let taken = c.state.pc == 0x400;
            assert_eq!(taken, expect_taken, "branch {brch:03X} after setup");
        }
        // And with every condition false (fresh chip: flags clear, DPL=1, SR=0).
        let mut c = chip(&[ld(DST_DP, 0x21), jp(no, 0x400)]);
        run(&mut c, 2);
        assert_eq!(
            c.state.pc, 0x400,
            "branch {no:03X} taken when its condition is false"
        );
    }
}

#[test]
fn jmp_and_jmpso_are_unconditional() {
    let mut c = chip(&[jp(JMP, 0x123)]);
    run(&mut c, 1);
    assert_eq!(c.state.pc, 0x123);

    let mut c = chip(&[ld(DST_SO, 0x0077), jp(JMPSO, 0)]);
    run(&mut c, 2);
    assert_eq!(c.state.pc, 0x077);
}

#[test]
fn call_and_rt_use_four_level_stack() {
    let mut program = vec![NOP; 0x20];
    program[0] = jp(CALL, 0x10);
    program[1] = ld(DST_A, 0xD0E0);
    program[0x10] = rt(alu(AluOp::Nop, ALU_A, SRC_TRB, DST_NON));
    let mut c = chip(&program);
    run(&mut c, 1);
    assert_eq!((c.state.pc, c.state.sp), (0x10, 1));
    run(&mut c, 1);
    assert_eq!((c.state.pc, c.state.sp), (0x01, 0));
    run(&mut c, 1);
    assert_eq!(c.state.a, 0xD0E0);

    // Five nested calls wrap the 4-level stack: the fifth overwrites the first slot.
    let mut c = chip(&[
        jp(CALL, 1),
        jp(CALL, 2),
        jp(CALL, 3),
        jp(CALL, 4),
        jp(CALL, 5),
    ]);
    run(&mut c, 5);
    assert_eq!(c.state.sp, 1);
    assert_eq!(c.state.stack[0], 5);
}

#[test]
fn reset_sets_pc_flags_sr_rp_and_keeps_other_registers() {
    // fullsnes "Reset (Vector 000h)": PC=000h, FlagA=FlagB=00h, SR=0000h, RP=3FFh; other
    // registers and data RAM unchanged.
    let mut c = chip(&[
        ld(DST_A, 0x7FFF),
        ld(DST_TR, 1),
        alu_p(AluOp::Add, P_IDB, ALU_A, SRC_TR, DST_NON),
        ld(DST_DR, 0x55),
        ld(DST_RP, 3),
        ld(DST_MEM, 0x4444),
    ]);
    run(&mut c, 6);
    c.reset();
    assert_eq!(c.state.pc, 0);
    assert_eq!(c.state.flags_a, Flags::default());
    assert_eq!(c.state.sr, 0);
    assert_eq!(c.state.rp, 0x3FF);
    assert_eq!(c.state.a, 0x8000);
    assert_eq!(c.state.ram[0], 0x4444);
}

#[test]
fn runs_at_7_6_mhz_of_master_clock() {
    // fullsnes component list: "DSPn 7.600MHz"; one opcode per clock.
    let mut c = chip(&[]);
    let master = 21_477_270u64;
    for _ in 0..master {
        c.tick_master_clock();
    }
    assert_eq!(c.state.cycle_count, 7_600_000);
}

// ---------------------------------------------------------------- host interface

#[test]
fn dr_16bit_transfer_lsb_then_msb_toggles_drs_and_clears_rqm() {
    // The chip outputs 1234h and waits; the SNES reads LSB then MSB.
    let mut c = chip(&[ld(DST_DR, 0x1234), jp(JRQM, 1), ld(DST_A, 1)]);
    run(&mut c, 1);
    assert_ne!(c.read_sr() & 0x80, 0, "RQM visible in SR bit 15");
    assert_eq!(c.read_dr(), 0x34);
    assert_ne!(c.state.sr & SR_DRS, 0, "DRS busy between halves");
    assert_ne!(c.state.sr & SR_RQM, 0, "RQM stays until the second half");
    assert_eq!(c.read_dr(), 0x12);
    assert_eq!(c.state.sr & (SR_DRS | SR_RQM), 0);

    // SNES writes LSB then MSB.
    let mut c = chip(&[ld(DST_DR, 0)]);
    run(&mut c, 1);
    c.write_dr(0xCD);
    assert_ne!(c.state.sr & SR_RQM, 0);
    c.write_dr(0xAB);
    assert_eq!(c.state.dr, 0xABCD);
    assert_eq!(c.state.sr & (SR_DRS | SR_RQM), 0);
}

#[test]
fn dr_8bit_mode_clears_rqm_on_one_access() {
    let mut c = chip(&[ld(DST_SR, SR_DRC), ld(DST_DR, 0x12EF)]);
    run(&mut c, 2);
    assert_eq!(c.read_dr(), 0xEF);
    assert_eq!(c.state.sr & SR_RQM, 0);

    let mut c = chip(&[ld(DST_SR, SR_DRC), ld(DST_DR, 0x1200)]);
    run(&mut c, 2);
    c.write_dr(0x77);
    assert_eq!(c.state.dr, 0x1277);
    assert_eq!(c.state.sr & SR_RQM, 0);
}

#[test]
fn source_dr_sets_rqm_and_drnf_does_not() {
    let mut c = chip(&[alu(AluOp::Nop, ALU_A, SRC_DRNF, DST_A)]);
    run(&mut c, 1);
    assert_eq!(c.state.sr & SR_RQM, 0);
    let mut c = chip(&[alu(AluOp::Nop, ALU_A, SRC_DR, DST_A)]);
    run(&mut c, 1);
    assert_ne!(c.state.sr & SR_RQM, 0);
}

#[test]
fn sr_read_returns_high_byte_and_host_writes_are_ignored() {
    let mut c = chip(&[ld(DST_SR, 0x6000 | SR_DRC | 0x0003)]);
    run(&mut c, 1);
    assert_eq!(c.read_sr(), 0x64);
    assert_eq!(c.peek(true), 0x64);
}

#[test]
fn rqm_wait_loop_idles_until_dr_access() {
    // A JRQM to itself parks the chip (Mesen2's RQM-loop shortcut); cycles still pass.
    let mut c = chip(&[ld(DST_DR, 0x0102), jp(JRQM, 1), ld(DST_A, 0x99)]);
    for _ in 0..200 {
        c.tick_master_clock();
    }
    assert!(c.state.in_rqm_loop);
    assert_eq!(c.state.pc, 1);
    c.read_dr();
    c.read_dr();
    assert!(!c.state.in_rqm_loop);
    for _ in 0..20 {
        c.tick_master_clock();
    }
    assert_eq!(c.state.a, 0x99);
}

#[test]
fn state_round_trips_and_rejects_bad_sizes() {
    let mut c = chip(&[ld(DST_A, 0x1357), ld(DST_MEM, 0x2468)]);
    run(&mut c, 2);
    let saved = c.capture_state();
    let mut d = chip(&[]);
    d.restore_state(&saved).expect("restores");
    assert_eq!(d.state, c.state);

    let mut bad = saved.clone();
    bad.ram.truncate(3);
    assert!(d.restore_state(&bad).is_err());
}
