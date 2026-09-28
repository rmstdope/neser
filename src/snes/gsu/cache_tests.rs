//! The code cache against fullsnes "SNES Cart GSU-n Code-Cache".

use super::core_tests::{PROGRAM, Rig};

/// IWT R1,#$1234; STOP; NOP, padded to one 16-byte cache line.
const INJECTED_LINE: [u8; 16] = [
    0xF1, 0x34, 0x12, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
];

/// Writes `line` into cache line 0 through the S-CPU window after writing GO=0, writing `len` of
/// its bytes. CBR must already be 0: on a GSU that never ran, or after the GO=0 write aborts a
/// running one (which zeroes CBR and empties the cache).
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

/// Starts the GSU at `$00:9000`, outside any cache window, with RON clear, so it is running
/// (waiting for the ROM bus) when the S-CPU next writes SFR. The stalled fetch still completes,
/// so the byte at `$00:9000` is the opcode a later restart executes first.
fn start_waiting_for_rom(rig: &mut Rig) {
    rig.gsu.write_register(0x303A, 0x08); // RAN only
    rig.write16(0x301E, 0x9000);
    // The previous run's last instruction can leave the GSU's catch-up count ahead of the
    // master clock, so give it time to reach the fetch.
    rig.tick(500);
    assert!(rig.gsu.state.waiting_for_rom, "running, waiting for ROM");
}

#[test]
fn aborting_a_running_gsu_zeroes_cbr_and_empties_cache() {
    let mut program = vec![0x01; 0x13];
    program.extend_from_slice(&[0x02, 0x00, 0x01]);
    let mut rig = Rig::run(&program);
    assert_eq!(rig.read16(0x303E), 0x8010);
    start_waiting_for_rom(&mut rig);
    rig.gsu.write_register(0x3030, 0x00);
    assert_eq!(rig.read16(0x303E), 0x0000);
    assert!(
        rig.gsu.state.code_cache_valid.iter().all(|&valid| !valid),
        "every line empty"
    );
}

/// fullsnes says an S-CPU write of GO=0 sets CBR to 0 and empties the cache, and that after STOP
/// "one must clear the cache by writing GO=0". Mesen2 (`Gsu::Write` $3030) and ares
/// (`SuperFX::writeIO` $3030) both do so only when the write stops a running GSU, and
/// Yoshi's Island, which writes SFR=0 before every job, keeps pace with Mesen2 only that way
/// (nr-2cn: its intro stork drifted from frame 1441 on). Navigator's decision: follow them.
#[test]
fn go_0_write_to_a_stopped_gsu_keeps_cbr_and_the_cache() {
    let mut program = vec![0x01; 0x13];
    program.extend_from_slice(&[0x02, 0x00, 0x01]); // $8013 CACHE: CBR = $8010
    let mut rig = Rig::run(&program);
    rig.gsu.write_register(0x3030, 0x00);
    assert_eq!(rig.read16(0x303E), 0x8010, "CBR kept");
    // The line holding CACHE; STOP is still valid: restarted there with neither bus, the GSU
    // runs it from the cache and stops instead of waiting for ROM.
    rig.gsu.write_register(0x303A, 0x00);
    rig.write16(0x301E, PROGRAM + 0x13);
    rig.run_until_stop();
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
    let mut rig = Rig::with_rom(|rom| {
        rom[..4].copy_from_slice(&[0x02, 0x01, 0x00, 0x01]); // CACHE; NOP; STOP
        rom[0x1000] = 0x01; // NOP at $00:9000, the prefetch the restart below executes first
    });
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    // Only aborting a running GSU zeroes CBR for the injection into line 0.
    start_waiting_for_rom(&mut rig);
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

#[test]
fn a_stopped_gsu_does_not_latch_a_wait() {
    // An S-CPU write of R14 with RON clear starts a ROM buffer fill; while the GSU is stopped
    // that must not leave it halted once started on cached code (fullsnes "Writing to
    // Code-Cache": cached code runs without RON/RAN).
    let mut rig = Rig::new(&[]);
    rig.gsu.write_register(0x303A, 0x00);
    rig.write16(0x301C, 0x8000);
    rig.tick(50);
    inject(&mut rig, &INJECTED_LINE, 16);
    start_in_cache(&mut rig);
    rig.run_until_stop();
    assert_eq!(rig.reg(1), 0x1234);
}

#[test]
fn writing_pbr_empties_the_cache() {
    let mut rig = Rig::new(&[0x02, 0x01, 0x00, 0x01]); // CACHE; NOP; STOP
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    rig.gsu.write_register(0x3034, 0x00); // Same bank, still empties the cache.
    rig.gsu.write_register(0x303A, 0x08); // RAN only
    rig.write16(0x301E, PROGRAM + 1);
    rig.tick(500);
    assert!(
        rig.gsu.state.waiting_for_rom,
        "refetching from ROM, not the cache"
    );
}

#[test]
fn aborting_a_waiting_gsu_does_not_carry_the_wait_into_the_next_start() {
    // Started from ROM without RON, the GSU waits; the S-CPU aborts it (SFR GO=0), injects
    // cache code and starts again, still without RON.
    let mut rig = Rig::new(&[0x01, 0x00, 0x01]);
    rig.gsu.write_register(0x303A, 0x08); // RAN only
    rig.write16(0x301E, PROGRAM);
    rig.tick(200);
    assert!(rig.gsu.state.waiting_for_rom);
    inject(&mut rig, &INJECTED_LINE, 16); // Writes SFR = 0 first.
    start_in_cache(&mut rig);
    rig.run_until_stop();
    assert_eq!(rig.reg(1), 0x1234);
}

#[test]
fn a_rom_fill_while_stopped_does_not_block_a_restart_in_cache() {
    // Warm the cache, stop, take RON away, then have the S-CPU write R14 (a ROM buffer fill that
    // lands while the GSU is stopped) and restart on the cached code with an R15 write alone: no
    // SFR write in between to clear anything.
    let mut rig = Rig::new(&[0x02, 0x01, 0x00, 0x01]); // CACHE; NOP; STOP; NOP
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    rig.gsu.write_register(0x303A, 0x00);
    rig.write16(0x301C, 0x8000);
    rig.tick(50);
    rig.write16(0x301E, PROGRAM);
    rig.run_until_stop();
}

#[test]
fn a_wait_latched_by_the_prefetch_after_stop_does_not_outlive_the_stop() {
    // STOP ends its cache line, so the byte after it is fetched from the next, empty line, which
    // with RON clear latches a wait while GO is still set. STOP must not leave the stopped GSU
    // waiting: a GO=0 write to a stopped GSU no longer clears anything (nr-2cn), so a restart
    // on cached code would otherwise never run.
    let mut rig = Rig::new(&[]);
    let mut line = [0x01; 16];
    line[15] = 0x00; // STOP at $000F
    inject(&mut rig, &line, 16);
    start_in_cache(&mut rig);
    rig.run_until_stop();
    assert!(
        !rig.gsu.state.waiting_for_rom,
        "no wait left on a stopped GSU"
    );
}
