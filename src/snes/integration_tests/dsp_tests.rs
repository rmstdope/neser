//! ROM-level coverage for the DSP-1 (nr-auv), the DSP-2 (nr-608) and the DSP-4 (nr-tfq).
//!
//! Nintendo's DSP-1 program cannot be shipped, so this drives the whole path (bus decode, the
//! uPD77C25 core, its clock and the DR/SR handshake) with a synthetic firmware written in the
//! test assembler. The firmware asks for a 16-bit word on DR, multiplies it by 6 with the K*L
//! multiplier (L=3, and the "K*L*2" low word doubles it), and answers on DR. A hand-built 65816 fixture on a DSP
//! LoROM cartridge polls RQM in SR, writes the operand LSB first and checks the answer,
//! reporting through the `rom_runner` marker protocol.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};
use crate::snes::dsp::DspChip;
use crate::snes::upd77c25::asm::*;
use crate::snes::upd77c25::{DATA_WORDS, PROGRAM_WORDS};

/// The synthetic firmware as an 8192-byte little-endian image.
fn multiply_by_six_firmware() -> Vec<u8> {
    let program = [
        alu(AluOp::Nop, ALU_A, SRC_DR, DST_NON), // 0: request input (RQM=1)
        jp(JRQM, 1),                             // 1: wait until the SNES has written DR
        alu(AluOp::Nop, ALU_A, SRC_DRNF, DST_K), // 2: K = DR
        ld(DST_L, 3),                            // 3: L = 3
        alu(AluOp::Xor, ALU_A, SRC_A, DST_NON),  // 4: A = 0
        alu_p(AluOp::Or, P_N, ALU_A, SRC_TRB, DST_NON), // 5: A = K*L*2 (low word)
        alu(AluOp::Nop, ALU_A, SRC_A, DST_DR),   // 6: DR = A (RQM=1)
        jp(JRQM, 7),                             // 7: wait until the SNES has read DR
        jp(JMP, 0),                              // 8: next request
    ];
    let mut image = Vec::with_capacity(PROGRAM_WORDS * 3 + DATA_WORDS * 2);
    for i in 0..PROGRAM_WORDS {
        let op = program.get(i).copied().unwrap_or(NOP);
        image.extend_from_slice(&[op as u8, (op >> 8) as u8, (op >> 16) as u8]);
    }
    image.resize(PROGRAM_WORDS * 3 + DATA_WORDS * 2, 0);
    image
}

/// Polls SR at `sr` until RQM (bit 7) is set.
fn wait_for_rqm(fixture: &mut FixtureRom, sr: u32) {
    let top = fixture.pos();
    fixture.lda_long(sr);
    fixture.and_imm(0x80);
    fixture.beq_to(top);
}

/// Sends `operand` and checks that `expected` comes back, SR at `sr` and DR at `dr`.
fn exchange(fixture: &mut FixtureRom, sr: u32, dr: u32, operand: u16, expected: u16) {
    wait_for_rqm(fixture, sr);
    fixture.lda_imm(operand as u8);
    fixture.sta_long(dr);
    fixture.lda_imm((operand >> 8) as u8);
    fixture.sta_long(dr);
    wait_for_rqm(fixture, sr);
    fixture.lda_long(dr);
    fixture.branch_fail_if_ne(expected as u8);
    fixture.lda_long(dr);
    fixture.branch_fail_if_ne((expected >> 8) as u8);
}

fn build_dsp1_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"DSP1 FIXTURE");
    fixture.dsp_chipset();
    // fullsnes "SNES Cart DSP-n": LoROM DR at 30-3F:8000-BFFF, SR at 30-3F:C000-FFFF, and the
    // 60-6F:0000-7FFF window with A14 choosing SR.
    exchange(&mut fixture, 0x30_C000, 0x30_8000, 0x0123, 0x06D2);
    exchange(&mut fixture, 0x6F_7FFF, 0x60_0000, 0x1000, 0x6000);
    // A signed operand: -2 * 6 = -12.
    exchange(&mut fixture, 0xBF_C000, 0xB0_BFFF, 0xFFFE, 0xFFF4);
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn dsp1_fixture_exchanges_words_with_the_firmware_through_dr_and_sr() {
    let firmware = multiply_by_six_firmware();
    let result = run_rom(
        &build_dsp1_fixture(),
        "dsp1-fixture.sfc",
        RunConfig::new(0, 30).with_dsp_firmware(DspChip::Dsp1, &firmware),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "DSP-1 fixture should pass: {result:?}"
    );
}

fn build_dsp2_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"DUNGEON MASTER");
    fixture.dsp_chipset();
    // fullsnes "SNES Cart DSP-n": the DSP-2 board (SHVC-1B5B-01, LoROM 1 MB + RAM) has DR at
    // 20-3F:8000-BFFF and SR at 20-3F:C000-FFFF; ares maps it at 20-3f,a0-bf. Banks $20-$2F
    // are the part a DSP-1 board does not decode.
    exchange(&mut fixture, 0x20_C000, 0x20_8000, 0x0123, 0x06D2);
    exchange(&mut fixture, 0xAF_FFFF, 0xA0_BFFF, 0xFFFE, 0xFFF4);
    exchange(&mut fixture, 0x3F_C000, 0x3F_8000, 0x1000, 0x6000);
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn dsp2_fixture_exchanges_words_through_the_20_3f_window() {
    let firmware = multiply_by_six_firmware();
    let result = run_rom(
        &build_dsp2_fixture(),
        "dsp2-fixture.sfc",
        RunConfig::new(0, 30).with_dsp_firmware(DspChip::Dsp2, &firmware),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "DSP-2 fixture should pass: {result:?}"
    );
}

fn build_dsp4_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"TOP GEAR 3000");
    fixture.dsp_chipset();
    // fullsnes "SNES Cart DSP-n": Top Gear 3000's board (SHVC-1B0N-01, LoROM 1 MB, shared with
    // the DSP-1) has DR at 30-3F:8000-BFFF and SR at 30-3F:C000-FFFF, mirrored at B0-BF.
    // fullsnes "DSP4 Commands": every DSP-4 transfer is 16-bit, which is what this exchange is.
    exchange(&mut fixture, 0x30_C000, 0x30_8000, 0x0123, 0x06D2);
    exchange(&mut fixture, 0xBF_FFFF, 0xB0_BFFF, 0xFFFE, 0xFFF4);
    exchange(&mut fixture, 0x3F_C000, 0x3F_8000, 0x1000, 0x6000);
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn dsp4_fixture_exchanges_16bit_words_through_the_30_3f_window() {
    let firmware = multiply_by_six_firmware();
    let result = run_rom(
        &build_dsp4_fixture(),
        "dsp4-fixture.sfc",
        RunConfig::new(0, 30).with_dsp_firmware(DspChip::Dsp4, &firmware),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "DSP-4 fixture should pass: {result:?}"
    );
}
