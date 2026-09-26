//! Control flow against fullsnes "SNES Cart GSU-n CPU JMP and Prefix Opcodes" and "CPU Misc,
//! Jump Notes": every jump executes the one byte after it before continuing at the target.

use super::core_tests::Rig;

#[test]
fn branch_executes_one_delay_slot_byte_before_target() {
    #[rustfmt::skip]
    let mut rig = Rig::run(&[
        0x05, 0x03, // $8000 BRA $8005 (relative to R15 = $8002)
        0xD1,       // $8002 INC R1  -- delay slot, executed
        0xD2,       // $8003 INC R2  -- skipped
        0xD3,       // $8004 INC R3  -- skipped
        0xD4,       // $8005 INC R4
        0x00, 0x01,
    ]);
    assert_eq!([rig.reg(1), rig.reg(2), rig.reg(3), rig.reg(4)], [1, 0, 0, 1]);
}

#[test]
fn untaken_branch_falls_through() {
    // Z is clear at start, so BEQ is not taken.
    let mut rig = Rig::run(&[0x09, 0x02, 0xD1, 0xD2, 0xD3, 0x00, 0x01]);
    assert_eq!([rig.reg(1), rig.reg(2), rig.reg(3)], [1, 1, 1]);
}

#[test]
fn branch_conditions_follow_the_flags() {
    // After DEC R1 from 0: S=1, Z=0, V=0 and C=0 (DEC touches neither). Each case branches
    // over `INC R2`: the offset is relative to the byte after the operand (the delay-slot NOP),
    // so 2 lands on the STOP.
    let cases = [
        (0x06, false), // BGE: S xor V = 1
        (0x07, true),  // BLT
        (0x08, true),  // BNE
        (0x09, false), // BEQ
        (0x0A, false), // BPL
        (0x0B, true),  // BMI
        (0x0C, true),  // BCC (C clear)
        (0x0D, false), // BCS
        (0x0E, true),  // BVC
        (0x0F, false), // BVS
    ];
    for (opcode, taken) in cases {
        let mut rig = Rig::run(&[0xE1, opcode, 0x02, 0x01, 0xD2, 0x00, 0x01]);
        assert_eq!(rig.reg(2) == 0, taken, "branch ${opcode:02X}");
    }
}

#[test]
fn branch_keeps_prefixes_for_the_split_opcode() {
    // fullsnes "Prefix + jump + ONE-BYTE-SUFFIX": TO R2 before the branch still applies to the
    // delay-slot ADD after it.
    let mut rig = Rig::new(&[0x12, 0x05, 0x02, 0x51, 0xD5, 0x01, 0x00, 0x01]);
    rig.set_reg(0, 0x0010);
    rig.set_reg(1, 0x0001);
    rig.start_at(0x8000);
    rig.run_until_stop();
    assert_eq!(rig.reg(2), 0x0011);
    assert_eq!(rig.reg(0), 0x0010);
    assert_eq!(rig.reg(5), 0);
}

#[test]
fn jmp_resets_prefixes() {
    #[rustfmt::skip]
    let mut rig = Rig::new(&[
        0xF8, 0x07, 0x80, // $8000 IWT R8,#$8007
        0x12,             // $8003 TO R2
        0x98,             // $8004 JMP R8 -- resets the TO
        0x51,             // $8005 ADD R1 -- delay slot, into R0
        0xD5,             // $8006 skipped
        0x00, 0x01,       // $8007
    ]);
    rig.set_reg(1, 0x0042);
    rig.start_at(0x8000);
    rig.run_until_stop();
    assert_eq!([rig.reg(0), rig.reg(2), rig.reg(5)], [0x0042, 0, 0]);
}

#[test]
fn ljmp_sets_pbr_and_cbr() {
    let mut rig = Rig::with_rom(|rom| {
        #[rustfmt::skip]
        rom[..12].copy_from_slice(&[
            0xF8, 0x01, 0x00, // IWT R8,#$0001  (bank)
            0xF9, 0x10, 0x80, // IWT R9,#$8010  (offset)
            0xB9,             // FROM R9
            0x3D, 0x98,       // LJMP R8
            0x01,             // delay slot
            0x00, 0x01,
        ]);
        rom[0x8010..0x8013].copy_from_slice(&[0xD3, 0x00, 0x01]); // $01:8010 INC R3; STOP
    });
    rig.start_at(0x8000);
    rig.run_until_stop();
    assert_eq!(rig.reg(3), 1);
    assert_eq!(rig.gsu.peek_register(0x3034), Some(0x01), "PBR");
    assert_eq!(rig.read16(0x303E), 0x8010, "CBR = target & $FFF0");
}

#[test]
fn loop_decrements_r12_and_jumps_to_r13() {
    #[rustfmt::skip]
    let mut rig = Rig::run(&[
        0xAC, 0x03, // $8000 IBT R12,#3
        0x2F, 0x1D, // $8002 MOVE R13,R15 -- R13 = $8004
        0xD1,       // $8004 INC R1
        0x3C,       // $8005 LOOP
        0x01,       // $8006 delay slot
        0x00, 0x01,
    ]);
    assert_eq!(rig.reg(1), 3);
    assert_eq!(rig.reg(12), 0);
    assert_ne!(rig.sfr() & 0x02, 0, "Z set when the counter runs out");
}

#[test]
fn link_sets_r11_to_next_plus_n() {
    let mut rig = Rig::run(&[0x93, 0x00, 0x01]); // LINK #3 at $8000
    assert_eq!(rig.reg(11), 0x8004);
}

#[test]
fn alu_to_r15_jumps() {
    #[rustfmt::skip]
    let mut rig = Rig::run(&[
        0xFF, 0x06, 0x80, // $8000 IWT R15,#$8006
        0xD1,             // $8003 delay slot
        0xD2, 0xD3,       // skipped
        0x00, 0x01,       // $8006
    ]);
    assert_eq!([rig.reg(1), rig.reg(2), rig.reg(3)], [1, 0, 0]);
}
