//! Black-box coverage for the SA-1 arithmetic unit reached from the SA-1 CPU itself (nr-ps1):
//! a hand-built SA-1 fixture ROM whose SA-1 program programs `$2250-$2254` and loads a result
//! register from `$2306-$230B` into A.
//!
//! Super Mario RPG stays on a black screen after its opening without this: its SA-1 code loops on
//! multiply results, and a result that always read 0 never lets the loop finish. Expected values
//! follow fullsnes "SNES Cart SA-1 Arithmetic Maths" (see `src/snes/sa1/arithmetic.rs`).

use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;

const HEADER: usize = 0x7FC0;
const SA1_PROGRAM: usize = 0x1000; // bank $00:$9000

fn write_lorom_sa1_header(rom: &mut [u8]) {
    rom[HEADER..HEADER + 21].fill(b' ');
    rom[HEADER..HEADER + 13].copy_from_slice(b"SA1 MATH TEST");
    rom[HEADER + 0x15] = 0x20; // Map mode: LoROM, slow.
    rom[HEADER + 0x16] = 0x35; // Chipset: SA-1 (as the absindx ROMs declare it).
    rom[HEADER + 0x17] = 0x06; // ROM size: 1 << 6 KB = 64 KB.
    rom[HEADER + 0x18] = 0x00; // RAM size.
    rom[HEADER + 0x3C] = 0x00; // Main CPU reset vector -> $8000.
    rom[HEADER + 0x3D] = 0x80;
}

/// Builds a 64 KiB SA-1 ROM. The main CPU sets the SA-1 reset vector to `$9000`, releases it
/// and idles. The SA-1 CPU switches to native mode, writes `mcnt` to `$2250` with an 8-bit
/// accumulator, then, with a 16-bit accumulator, writes each `(ma, mb)` pair as words to
/// `$2251` and `$2253` (the high byte to `$2254` last, which starts the operation), loads the
/// word at `result_port` into A and idles.
fn build_sa1_math_rom(mcnt: u8, operations: &[(u16, u16)], result_port: u16) -> Vec<u8> {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_sa1_header(&mut rom);

    #[rustfmt::skip]
    let main_program: [u8; 18] = [
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x03, 0x22, // STA $2203 (CRVL)
        0xA9, 0x90,       // LDA #$90
        0x8D, 0x04, 0x22, // STA $2204 (CRVH) -> SA-1 reset vector = $9000
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x00, 0x22, // STA $2200 (CCNT) -> release SA-1 from reset
        0x4C, 0x0F, 0x80, // JMP $800F (idle loop)
    ];
    rom[..main_program.len()].copy_from_slice(&main_program);

    #[rustfmt::skip]
    let mut sa1_program: Vec<u8> = vec![
        0x18,             // CLC
        0xFB,             // XCE -> native mode
        0xE2, 0x20,       // SEP #$20 -> 8-bit A
        0xA9, mcnt,       // LDA #mcnt
        0x8D, 0x50, 0x22, // STA $2250 (MCNT)
        0xC2, 0x20,       // REP #$20 -> 16-bit A
    ];
    for &(ma, mb) in operations {
        let [ma_low, ma_high] = ma.to_le_bytes();
        let [mb_low, mb_high] = mb.to_le_bytes();
        #[rustfmt::skip]
        sa1_program.extend_from_slice(&[
            0xA9, ma_low, ma_high, // LDA #ma
            0x8D, 0x51, 0x22,      // STA $2251 (MA)
            0xA9, mb_low, mb_high, // LDA #mb
            0x8D, 0x53, 0x22,      // STA $2253 (MB; the $2254 byte starts the operation)
        ]);
    }
    let [port_low, port_high] = result_port.to_le_bytes();
    sa1_program.extend_from_slice(&[0xAD, port_low, port_high]); // LDA result_port
    let idle = 0x9000 + sa1_program.len() as u16;
    let [idle_low, idle_high] = idle.to_le_bytes();
    sa1_program.extend_from_slice(&[0x4C, idle_low, idle_high]); // JMP idle
    rom[SA1_PROGRAM..SA1_PROGRAM + sa1_program.len()].copy_from_slice(&sa1_program);

    rom
}

/// Runs the fixture until the SA-1 CPU has reached its idle loop and returns its A.
fn sa1_result(mcnt: u8, operations: &[(u16, u16)], result_port: u16) -> u16 {
    let rom = build_sa1_math_rom(mcnt, operations, result_port);
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "sa1-math-test.sfc")
        .expect("failed to load SA-1 arithmetic fixture ROM");
    for _ in 0..4000 {
        snes.run_tick();
    }
    snes.sa1_cpu_a_for_tests()
        .expect("an SA-1 cartridge has an SA-1 CPU")
}

#[test]
fn sa1_cpu_reads_its_signed_multiply_result() {
    // -2 * 3 = -6 = $FFFFFFFA.
    assert_eq!(sa1_result(0x00, &[(0xFFFE, 0x0003)], 0x2306), 0xFFFA);
    assert_eq!(sa1_result(0x00, &[(0xFFFE, 0x0003)], 0x2308), 0xFFFF);
}

#[test]
fn sa1_cpu_reads_its_division_quotient_and_unsigned_remainder() {
    // -7 / 2 = quotient -4, remainder +1 (the remainder is unsigned).
    assert_eq!(sa1_result(0x01, &[(0xFFF9, 0x0002)], 0x2306), 0xFFFC);
    assert_eq!(sa1_result(0x01, &[(0xFFF9, 0x0002)], 0x2308), 0x0001);
}

#[test]
fn sa1_cpu_reads_its_cumulative_sum_and_overflow_flag() {
    // 3*4 + 5*6 = 42, no overflow.
    assert_eq!(sa1_result(0x02, &[(3, 4), (5, 6)], 0x2306), 42);
    // A 16-bit read of $230A returns MR bits 32-39 and, in its high byte, OF ($230B): the sum
    // stepping below zero wraps to $FF_FFFF_FFFF and carries out of bit 39.
    assert_eq!(sa1_result(0x02, &[(0xFFFF, 0x0001)], 0x230A), 0x80FF);
}
