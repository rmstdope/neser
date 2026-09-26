//! ROM-level coverage for the Capcom CX4 (nr-t7d).
//!
//! - `cx4test.sfc` (Overload, 2010, vendored under `jonasquinn-test-roms/cx4test/`) probes the
//!   chip's SNES-visible memory map: that `$6000-$6BFF`, `$7000-$7BFF` and `$7F40-$7FFF` are
//!   not open bus, that data RAM is read/write and mirrored, which bits the control ports keep,
//!   and that the register file mirrors at `$7FC0`. It prints each verdict into a WRAM text
//!   buffer, which is what these tests read: no screen capture is involved.
//! - A hand-built fixture whose 65816 program DMAs a table into CX4 RAM, runs a CX4 program
//!   (a square, a data-ROM lookup and a RAM read, as the games' own `test_square` and
//!   "immediate ROM" self-tests do) and checks the results itself, reporting through the
//!   `rom_runner` marker protocol.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};
use crate::platform::emulator::Emulator;
use crate::snes::bus::SnesBus;
use crate::snes::console::Snes;

const CX4TEST_ROM: &str =
    "roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/cx4test/cx4test.sfc";

/// `StrOutput` in `CX4TEST.X65`: the BG3 text buffer, two bytes (tile, colour) per character.
const TEXT_BUFFER: u32 = 0x7E_1000;

/// Where `StrBool` prints "PASS" over the "FAIL" of each result line, in `CX4TEST.X65` order.
const RESULT_LINES: [(&str, u16); 7] = [
    ("MEMORY TEST", 0x0176),
    ("$6000-$6BFF", 0x01B6),
    ("$7000-$7BFF", 0x01F6),
    ("$7F40-$7F4F", 0x0236),
    ("$7F50-$7F5F", 0x0276),
    ("$7F60-$7F7F", 0x02B6),
    ("$7F80-$7FFF", 0x02F6),
];

/// The four characters printed at `position` of the text buffer.
fn word_at(snes: &Snes, position: u16) -> String {
    let bus = snes.bus_for_tests().expect("ROM loaded");
    (0..4)
        .map(|i| bus.read_for_debugger(TEXT_BUFFER + u32::from(position) + i * 2) as char)
        .collect()
}

#[test]
fn cx4test_reports_pass_for_every_line() {
    let rom = std::fs::read(CX4TEST_ROM).expect("read cx4test.sfc");
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "cx4test.sfc")
        .expect("load cx4test.sfc");
    // The ROM runs its tests straight after boot and then idles. Its two RAM walks write and
    // compare every bit of 3 KB through slow-ROM code: measured, the last verdict is printed
    // between frames 30 and 60, so 120 frames leaves double margin. Mesen2 shows the same seven
    // PASS lines, pixel for pixel, at frame 180.
    while snes.bus_for_tests().expect("ROM loaded").master_clock() < 120 * 357_368 {
        snes.run_tick();
    }

    let verdicts: Vec<(&str, String)> = RESULT_LINES
        .iter()
        .map(|&(line, position)| (line, word_at(&snes, position)))
        .collect();
    assert!(
        verdicts.iter().all(|(_, word)| word == "PASS"),
        "cx4test verdicts: {verdicts:?}"
    );
}

/// CX4 program at `$01:8000`, run from page 0: `R1:R2 = R0 * R0` (signed), `R3` = data ROM
/// entry 240h (sin 45°), `R4` = the 24-bit word at CX4 RAM 000h plus one.
#[rustfmt::skip]
const CX4_PROGRAM: [u16; 17] = [
    0x6060, // mov   A,R0
    0x9860, // smul  MH:ML,A,R0
    0x0000, // nop   (the multiplier result is not ready on the next opcode)
    0x6002, // mov   A,ML
    0xE061, // mov   R1,A
    0x6001, // mov   A,MH
    0xE062, // mov   R2,A
    0x7640, // mov   rom_dta,cx4rom[240h]
    0x6008, // mov   A,rom_dta
    0xE063, // mov   R3,A
    0x6400, // mov   A,00h
    0xE01C, // mov   ram_ptr,A
    0x6C00, // movb  ram_dta.lsb,cx4ram[ram_ptr+0]
    0x6D01, // movb  ram_dta.mid,cx4ram[ram_ptr+1]
    0x6E02, // movb  ram_dta.msb,cx4ram[ram_ptr+2]
    0x600C, // mov   A,ram_dta
    0x8401, // add   A,A,01h
];

/// Emits `LDA $7F5E / AND #$40 / BNE loop`: waits for the CX4's busy bit to clear.
fn wait_while_cx4_busy(fixture: &mut FixtureRom) {
    let poll = fixture.pos();
    fixture.lda_abs(0x7F5E);
    fixture.and_imm(0x40);
    fixture.bne_to(poll);
}

fn build_cx4_program_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"CX4 PROGRAM FIXTURE");
    fixture.cx4_chipset();
    let mut program: Vec<u8> = CX4_PROGRAM.iter().flat_map(|op| op.to_le_bytes()).collect();
    program.extend_from_slice(&0xE064_u16.to_le_bytes()); // mov R4,A
    program.extend_from_slice(&0xFC00_u16.to_le_bytes()); // stop
    fixture.place_in_bank1(0x0000, &program);
    fixture.place_in_bank1(0x4000, &[0xFF, 0x12, 0x34]); // DMA source table at $01:C000

    // DMA the table into CX4 RAM 000h: source $01:C000, 3 bytes, destination $00:6000.
    for (port, value) in [
        (0x7F40, 0x00),
        (0x7F41, 0xC0),
        (0x7F42, 0x01),
        (0x7F43, 0x03),
        (0x7F44, 0x00),
        (0x7F45, 0x00),
        (0x7F46, 0x60),
        (0x7F47, 0x00), // destination bank; starts the DMA
    ] {
        fixture.store_imm_abs(port, value);
    }
    wait_while_cx4_busy(&mut fixture);

    // R0 = 000123h; program base $01:8000, page 0; writing PC 00h starts the program.
    for (port, value) in [
        (0x7F80, 0x23),
        (0x7F81, 0x01),
        (0x7F82, 0x00),
        (0x7F49, 0x00),
        (0x7F4A, 0x80),
        (0x7F4B, 0x01),
        (0x7F4D, 0x00),
        (0x7F4E, 0x00),
        (0x7F4F, 0x00),
    ] {
        fixture.store_imm_abs(port, value);
    }
    wait_while_cx4_busy(&mut fixture);

    // 123h * 123h = 014AC9h; sin 45° = B504F3h; 3412FFh + 1 = 341300h.
    for (register, expected) in [
        (0x7F83, 0xC9),
        (0x7F84, 0x4A),
        (0x7F85, 0x01),
        (0x7F86, 0x00),
        (0x7F89, 0xF3),
        (0x7F8A, 0x04),
        (0x7F8B, 0xB5),
        (0x7F8C, 0x00),
        (0x7F8D, 0x13),
        (0x7F8E, 0x34),
    ] {
        fixture.lda_abs(register);
        fixture.branch_fail_if_ne(expected);
    }
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn cx4_program_fixture_computes_through_the_snes_bus() {
    let result = run_rom(
        &build_cx4_program_fixture(),
        "cx4-program-fixture.sfc",
        RunConfig::new(0, 30),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "CX4 program fixture should pass: {result:?}"
    );
}
