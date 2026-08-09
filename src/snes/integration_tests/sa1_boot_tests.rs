//! Black-box coverage for issue #2957 (SA-1 dual-CPU core and control registers): a hand-built
//! SA-1-chipset ROM whose main-CPU program releases the SA-1 CPU from reset via `$2200`/
//! `$2203`/`$2204`, and whose SA-1-side program executes independently once released.
//!
//! This is deliberately a tiny custom fixture, not one of the vendored absindx conformance
//! ROMs (`SA1RamProtectionTest.sfc`/`SA1VersionCodeTest.sfc`) -- those need I-RAM, BW-RAM, and
//! the full cross-CPU IRQ handshake (later sub-issues of #2956) and are automated separately
//! under #2962.

use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;

const HEADER: usize = 0x7FC0;

fn write_lorom_header(rom: &mut [u8], title: &[u8], chipset: u8) {
    rom[HEADER..HEADER + 21].fill(b' ');
    rom[HEADER..HEADER + title.len()].copy_from_slice(title);
    rom[HEADER + 0x15] = 0x20; // Map mode: LoROM, slow.
    rom[HEADER + 0x16] = chipset;
    rom[HEADER + 0x17] = 0x07; // ROM size.
    rom[HEADER + 0x18] = 0x00; // RAM size.
    rom[HEADER + 0x1C] = 0x34; // Complement check (not validated by this codebase).
    rom[HEADER + 0x1D] = 0x12;
    rom[HEADER + 0x1E] = 0xCB; // Checksum (not validated by this codebase).
    rom[HEADER + 0x1F] = 0xED;
    rom[HEADER + 0x3C] = 0x00; // Main CPU reset vector low byte.
    rom[HEADER + 0x3D] = 0x80; // Main CPU reset vector high byte -> $8000.
}

/// Builds a 64 KiB LoROM SA-1 ROM (chipset `$35`, matching the vendored absindx ROMs' own
/// header byte) whose main-CPU program at bank `$00:$8000` writes the SA-1 reset vector
/// ($9000) via `$2203`/`$2204`, releases SA-1 from reset via `$2200`, then idles; and whose
/// SA-1-side program at bank `$00:$9000` loads a marker byte into A and idles.
fn build_sa1_boot_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_header(&mut rom, b"SA1 BOOT TEST", 0x35);

    // Main CPU program, bank $00:$8000 (file offset $0000).
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
    rom[0x0000..main_program.len()].copy_from_slice(&main_program);

    // SA-1 CPU program, bank $00:$9000 (file offset $1000).
    #[rustfmt::skip]
    let sa1_program: [u8; 5] = [
        0xA9, 0x7B,       // LDA #$7B
        0x4C, 0x02, 0x90, // JMP $9002 (idle loop)
    ];
    rom[0x1000..0x1000 + sa1_program.len()].copy_from_slice(&sa1_program);

    rom
}

#[test]
fn sa1_cpu_boots_and_executes_once_released_by_main_cpu() {
    let rom = build_sa1_boot_rom();
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "sa1-boot-test.sfc")
        .expect("failed to load SA-1 boot fixture ROM");

    for _ in 0..2000 {
        snes.run_tick();
    }

    assert_eq!(
        snes.sa1_cpu_a_for_tests(),
        Some(0x7B),
        "SA-1 CPU should have executed LDA #$7B once released from reset"
    );
    assert_eq!(
        snes.sa1_cpu_pc_for_tests(),
        Some(0x9002),
        "SA-1 CPU should be idling in its own self-JMP loop at $9002"
    );
}

#[test]
fn non_sa1_cartridge_has_no_sa1_cpu() {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_header(&mut rom, b"PLAIN ROM TEST", 0x00);
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "plain.sfc")
        .expect("failed to load plain ROM fixture");

    snes.run_tick();

    assert_eq!(snes.sa1_cpu_pc_for_tests(), None);
    assert_eq!(snes.sa1_cpu_a_for_tests(), None);
}
