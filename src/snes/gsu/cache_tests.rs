//! The code cache against fullsnes "SNES Cart GSU-n Code-Cache".

use super::core_tests::{PROGRAM, Rig};

/// IWT R1,#$1234; STOP; NOP, padded to one 16-byte cache line.
const INJECTED_LINE: [u8; 16] = [
    0xF1, 0x34, 0x12, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
];

/// Writes `line` into cache line 0 through the S-CPU window after clearing GO (which zeroes CBR
/// and empties the cache), writing `len` of its bytes.
fn inject(rig: &mut Rig, line: &[u8; 16], len: usize) {
    rig.gsu.write_register(0x3030, 0x00);
    for (i, &byte) in line.iter().enumerate().take(len) {
        rig.gsu.write_register(0x3100 + i as u16, byte);
    }
}

/// Starts the GSU at `$00:0000` with RON and RAN both clear, so only the cache can feed it.
fn start_in_cache(rig: &mut Rig) {
    rig.gsu.write_register(0x3039, 0x01);
    rig.gsu.write_register(0x303A, 0x00);
    rig.write16(0x301E, 0x0000);
}

#[test]
fn cache_sets_cbr_to_r15_and_fff0() {
    let mut program = vec![0x01; 0x13];
    program.extend_from_slice(&[0x02, 0x00, 0x01]); // $8013 CACHE: R15 = $8014
    let mut rig = Rig::run(&program);
    assert_eq!(rig.read16(0x303E), 0x8010);
}

#[test]
fn cache_window_shows_code_the_gsu_cached() {
    // fullsnes: GSU address X in the cache appears at SNES $3100 + (X & $1FF).
    let rig_program = [0x02, 0xF1, 0x34, 0x12, 0x00, 0x01]; // CACHE; IWT R1,#$1234
    let mut rig = Rig::run(&rig_program);
    assert_eq!(rig.reg(1), 0x1234);
    assert_eq!(rig.gsu.peek_register(0x3101), Some(0xF1));
    assert_eq!(rig.gsu.peek_register(0x3103), Some(0x12));
}

#[test]
fn cached_code_runs_without_ron_or_ran() {
    let mut rig = Rig::new(&[]);
    inject(&mut rig, &INJECTED_LINE, 16);
    start_in_cache(&mut rig);
    rig.run_until_stop();
    assert_eq!(rig.reg(1), 0x1234);
}

#[test]
fn snes_write_to_last_byte_of_line_marks_it_valid() {
    // Without the line's last byte the line stays empty, so the GSU must fetch from ROM, which
    // it does not own: it waits.
    let mut rig = Rig::new(&[]);
    inject(&mut rig, &INJECTED_LINE, 15);
    start_in_cache(&mut rig);
    rig.tick(1000);
    assert!(rig.gsu.state.waiting_for_rom, "halted on the ROM bus");
    assert_ne!(rig.reg(1), 0x1234);
}

#[test]
fn clearing_go_from_snes_zeroes_cbr_and_empties_cache() {
    let mut program = vec![0x01; 0x13];
    program.extend_from_slice(&[0x02, 0x00, 0x01]);
    let mut rig = Rig::run(&program);
    assert_eq!(rig.read16(0x303E), 0x8010);
    rig.gsu.write_register(0x3030, 0x00);
    assert_eq!(rig.read16(0x303E), 0x0000);
}

#[test]
fn stop_keeps_the_cache() {
    let mut rig = Rig::new(&[]);
    inject(&mut rig, &INJECTED_LINE, 16);
    start_in_cache(&mut rig);
    rig.run_until_stop();
    rig.set_reg(1, 0);
    // Restart without touching SFR: the injected line is still valid.
    rig.write16(0x301E, 0x0000);
    rig.run_until_stop();
    assert_eq!(rig.reg(1), 0x1234);
}

#[test]
fn ljmp_empties_the_cache() {
    // Code cached at $00:8000.. is not reused once LJMP has emptied the cache: after the jump
    // back to it with RON clear, the GSU must refetch from ROM and waits.
    let mut rig = Rig::new(&[0x02, 0x01, 0x00, 0x01]); // CACHE; NOP; STOP
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    // LJMP to $00:8000 (R8 = bank 0, Sreg = R9 = $8000), injected into cache line 0.
    #[rustfmt::skip]
    let line: [u8; 16] = [
        0xF8, 0x00, 0x00, 0xF9, 0x00, 0x80, 0xB9, 0x3D, 0x98, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    ];
    inject(&mut rig, &line, 16);
    start_in_cache(&mut rig);
    rig.tick(2000);
    // Had the cache survived, slot 0 would still hold the injected line and the GSU would loop
    // through it instead.
    assert!(rig.gsu.state.waiting_for_rom, "LJMP emptied the cache");
}
