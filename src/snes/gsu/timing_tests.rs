//! GSU timing in master clocks: fullsnes "CPU Misc" gives ROM reads as 5 cycles at 21 MHz and 3
//! at 10 MHz (5 and 6 master clocks) and the cache as faster; the per-fetch costs follow
//! Mesen2's `ReadProgramByte` (cache 1/2, Game Pak 5/6 master clocks).

use super::core_tests::{PROGRAM, Rig};

/// Master clocks to run `nops` NOPs (after a leading CACHE when `cached`) to STOP. Measured on a
/// second run, so the cache is warm (STOP keeps it), after idling long enough for the GSU's
/// catch-up count, which the last instruction of a run can leave ahead of the master clock, to
/// fall back behind it.
fn time(nops: usize, clock_21mhz: bool, cached: bool) -> u64 {
    let mut program = Vec::new();
    if cached {
        program.push(0x02);
    }
    program.extend(std::iter::repeat_n(0x01, nops));
    program.extend_from_slice(&[0x00, 0x01]);
    let mut rig = Rig::new(&program);
    rig.gsu.write_register(0x3039, u8::from(clock_21mhz));
    rig.gsu.write_register(0x303A, 0x18);
    rig.write16(0x301E, PROGRAM);
    rig.run_until_stop();
    rig.tick(1000);
    rig.write16(0x301E, PROGRAM);
    rig.run_until_stop()
}

#[test]
fn rom_fetch_costs_six_master_clocks_at_10mhz_and_five_at_21mhz() {
    assert_eq!(time(64, false, false) - time(0, false, false), 64 * 6);
    assert_eq!(time(64, true, false) - time(0, true, false), 64 * 5);
}

#[test]
fn cache_fetch_costs_two_master_clocks_at_10mhz_and_one_at_21mhz() {
    assert_eq!(time(64, false, true) - time(0, false, true), 64 * 2);
    assert_eq!(time(64, true, true) - time(0, true, true), 64);
}

#[test]
fn rom_fetch_waits_while_ron_is_clear_and_resumes_when_set() {
    let mut rig = Rig::new(&[0xF1, 0x34, 0x12, 0x00, 0x01]);
    rig.gsu.write_register(0x303A, 0x08); // RAN only
    rig.write16(0x301E, PROGRAM);
    rig.tick(500);
    assert!(rig.gsu.state.waiting_for_rom);
    assert_ne!(rig.reg(1), 0x1234);
    rig.gsu.write_register(0x303A, 0x18);
    rig.run_until_stop();
    assert_eq!(rig.reg(1), 0x1234);
}

#[test]
fn ram_access_waits_while_ran_is_clear() {
    // STW (R1) with RAN clear: the store is buffered, and landing it waits for the bus.
    let mut rig = Rig::new(&[0x31, 0x3D, 0x41, 0x00, 0x01]); // STW (R1); LDB (R1)
    rig.set_reg(0, 0x00AA);
    rig.set_reg(1, 0x0010);
    rig.gsu.write_register(0x303A, 0x10); // RON only
    rig.write16(0x301E, PROGRAM);
    rig.tick(500);
    assert!(rig.gsu.state.waiting_for_ram);
    rig.gsu.write_register(0x303A, 0x18);
    rig.run_until_stop();
    assert_eq!(rig.ram.borrow()[0x10], 0xAA);
}

#[test]
fn slow_multiply_costs_more_than_fast() {
    let run = |cfgr: u8| {
        let mut rig = Rig::new(&[0x9F, 0x00, 0x01]); // FMULT
        rig.gsu.write_register(0x3037, cfgr);
        rig.start_at(PROGRAM);
        rig.run_until_stop();
        rig.tick(1000);
        rig.write16(0x301E, PROGRAM);
        rig.run_until_stop()
    };
    // Mesen2: (MS0 ? 3 : 7) GSU cycles on top of the fetch, one master clock each at 21 MHz.
    assert_eq!(run(0x00) - run(0x20), 4);
}
