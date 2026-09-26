//! GSU data access to Game Pak ROM and RAM against fullsnes "SNES Cart GSU-n CPU MOV Opcodes"
//! and "Other Caches" (ROM read buffer, RAM write buffer, RAM address for SBK).

use super::core_tests::{PROGRAM, Rig};

/// A rig whose ROM holds `program` at `$00:8000` and `data` at `$00:8100`.
fn rig_with_data(program: &[u8], data: &[u8]) -> Rig {
    Rig::with_rom(|rom| {
        rom[..program.len()].copy_from_slice(program);
        rom[0x100..0x100 + data.len()].copy_from_slice(data);
    })
}

fn run(mut rig: Rig, regs: &[(u16, u16)]) -> Rig {
    for &(n, value) in regs {
        rig.set_reg(n, value);
    }
    rig.start_at(PROGRAM);
    rig.run_until_stop_and_settle();
    rig
}

#[test]
fn getb_family_reads_rombr_r14() {
    let cases: [(&[u8], u16); 4] = [
        (&[0xEF], 0x0085),       // GETB: zero-extended
        (&[0x3D, 0xEF], 0x8534), // GETBH: high byte, low unchanged
        (&[0x3E, 0xEF], 0x1285), // GETBL: low byte, high unchanged
        (&[0x3F, 0xEF], 0xFF85), // GETBS: sign-extended
    ];
    for (getb, expected) in cases {
        let mut program = vec![0xFE, 0x00, 0x81]; // IWT R14,#$8100
        program.extend_from_slice(getb);
        program.extend_from_slice(&[0x00, 0x01]);
        let mut rig = run(rig_with_data(&program, &[0x85]), &[(0, 0x1234)]);
        assert_eq!(rig.reg(0), expected, "{getb:02X?}");
    }
}

#[test]
fn changing_r14_prefetches_rom_buffer() {
    #[rustfmt::skip]
    let program = [
        0xFE, 0x00, 0x81, // IWT R14,#$8100
        0xEF,             // GETB -> $11
        0xDE,             // INC R14
        0x11, 0xEF,       // TO R1; GETB -> $22
        0x00, 0x01,
    ];
    let mut rig = run(rig_with_data(&program, &[0x11, 0x22]), &[]);
    assert_eq!((rig.reg(0), rig.reg(1)), (0x11, 0x22));
}

#[test]
fn snes_write_of_r14_also_prefetches() {
    let mut rig = rig_with_data(&[0xEF, 0x00, 0x01], &[0x5A]);
    rig.set_reg(14, 0x8100);
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    assert_eq!(rig.reg(0), 0x5A);
}

#[test]
fn romb_changes_bank_for_following_reads() {
    let mut rig = Rig::with_rom(|rom| {
        #[rustfmt::skip]
        rom[..11].copy_from_slice(&[
            0xA1, 0x01,       // IBT R1,#1
            0xB1, 0x3F, 0xDF, // FROM R1; ROMB
            0xFE, 0x00, 0x80, // IWT R14,#$8000 -> ROM $01:8000
            0xEF,             // GETB
            0x00, 0x01,
        ]);
        rom[0x8000] = 0x99;
    });
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    assert_eq!(rig.reg(0), 0x99);
    assert_eq!(rig.gsu.peek_register(0x3036), Some(0x01), "ROMBR");
}

#[test]
fn ramb_selects_bank_71() {
    let mut rig = Rig::new(&[0xA1, 0x01, 0xB1, 0x3E, 0xDF, 0x00, 0x01]); // RAMB from R1
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    assert_eq!(rig.gsu.peek_register(0x303C), Some(0x01), "RAMBR");
}

#[test]
fn stw_and_ldw_swap_bytes_at_odd_addresses() {
    // fullsnes: "Words at odd addresses are accessing [addr AND NOT 1], with data LSB/MSB
    // swapped".
    #[rustfmt::skip]
    let program = [
        0x31,       // STW (R1) -- R1 odd
        0x32,       // STW (R2) -- R2 even
        0x13, 0x41, // TO R3; LDW (R1)
        0x14, 0x42, // TO R4; LDW (R2)
        0x00, 0x01,
    ];
    let mut rig = run(
        Rig::new(&program),
        &[(0, 0xBEEF), (1, 0x0101), (2, 0x0200)],
    );
    let ram = rig.ram.borrow().clone();
    assert_eq!((ram[0x100], ram[0x101]), (0xBE, 0xEF));
    assert_eq!((ram[0x200], ram[0x201]), (0xEF, 0xBE));
    assert_eq!((rig.reg(3), rig.reg(4)), (0xBEEF, 0xBEEF));
}

#[test]
fn stb_and_ldb_move_one_byte() {
    #[rustfmt::skip]
    let program = [
        0x3D, 0x31,       // STB (R1)
        0x13, 0x3D, 0x41, // TO R3; LDB (R1)
        0x00, 0x01,
    ];
    let mut rig = run(Rig::new(&program), &[(0, 0xBEEF), (1, 0x0300)]);
    let ram = rig.ram.borrow().clone();
    assert_eq!((ram[0x300], ram[0x301]), (0xEF, 0x00));
    assert_eq!(rig.reg(3), 0x00EF, "LDB zero-extends");
}

#[test]
fn lm_lms_sm_sms_address_forms() {
    #[rustfmt::skip]
    let program = [
        0x3E, 0xF1, 0x34, 0x12, // SM ($1234),R1
        0x3E, 0xA2, 0x40,       // SMS ($80),R2   -- short address kk*2
        0x3D, 0xF5, 0x34, 0x12, // LM R5,($1234)
        0x3D, 0xA6, 0x40,       // LMS R6,($80)
        0x00, 0x01,
    ];
    let mut rig = run(Rig::new(&program), &[(1, 0xA1B2), (2, 0xC3D4)]);
    let ram = rig.ram.borrow().clone();
    assert_eq!((ram[0x1234], ram[0x1235]), (0xB2, 0xA1));
    assert_eq!((ram[0x80], ram[0x81]), (0xD4, 0xC3));
    assert_eq!((rig.reg(5), rig.reg(6)), (0xA1B2, 0xC3D4));
}

#[test]
fn sbk_writes_to_last_ram_address() {
    #[rustfmt::skip]
    let program = [
        0x3D, 0xF0, 0x00, 0x05, // LM R0,($0500)
        0xD0,                   // INC R0
        0x90,                   // SBK
        0x00, 0x01,
    ];
    let mut rig = Rig::new(&program);
    rig.ram.borrow_mut()[0x500] = 0x41;
    rig.start_at(PROGRAM);
    rig.run_until_stop_and_settle();
    assert_eq!(rig.ram.borrow()[0x500], 0x42);
}

#[test]
fn ram_bank_71_is_the_second_64k() {
    let mut rig = Rig::new(&[0xA1, 0x01, 0xB1, 0x3E, 0xDF, 0x32, 0x00, 0x01]); // RAMB 1; STW (R2)
    // Replace the 32 KB RAM with 128 KB so bank $71 is distinct.
    *rig.ram.borrow_mut() = vec![0; 0x2_0000];
    rig.set_reg(0, 0x7777);
    rig.set_reg(2, 0x0010);
    rig.start_at(PROGRAM);
    rig.run_until_stop_and_settle();
    assert_eq!(rig.ram.borrow()[0x1_0010], 0x77);
    assert_eq!(rig.ram.borrow()[0x0010], 0x00);
}
