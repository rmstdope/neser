//! Black-box coverage for issue #2958 (SA-1 I-RAM and write protection): a hand-built
//! SA-1-chipset ROM where the main CPU writes a marker byte into I-RAM (via the `$003000`
//! mirror, after enabling `$2229` SIWP), releases SA-1, and the SA-1 CPU -- after enabling its
//! own `$222A` CIWP -- reads that byte through the *same* physical I-RAM (its own address space
//! sees it at `$3000` too) and writes a transformed copy back for the main CPU to observe.
//!
//! This is the mechanism `SA1RamProtectionTest.sfc` (automated separately under #2962, once all
//! of #2956's sub-issues land) depends on for its cross-CPU message passing.

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

/// Builds a 64 KiB LoROM SA-1 ROM whose main-CPU program at bank `$00:$8000` enables I-RAM
/// writes from the SNES side (`$2229`=`$FF`), stores a marker byte (`$99`) into I-RAM via the
/// `$003000` mirror, points the SA-1 reset vector at `$9000`, releases SA-1 from reset, then
/// idles; and whose SA-1-side program at bank `$00:$9000` enables I-RAM writes from its own
/// side (`$222A`=`$01`), reads the marker byte back, increments it, stores the result at
/// I-RAM offset `$10`, then idles.
fn build_sa1_iram_roundtrip_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_header(&mut rom, b"SA1 IRAM TEST", 0x35);

    // Main CPU program, bank $00:$8000 (file offset $0000).
    #[rustfmt::skip]
    let main_program: [u8; 28] = [
        0xA9, 0xFF,       // LDA #$FF
        0x8D, 0x29, 0x22, // STA $2229 (SIWP: write-enable all I-RAM chunks from SNES side)
        0xA9, 0x99,       // LDA #$99
        0x8D, 0x00, 0x30, // STA $3000 (marker byte into I-RAM)
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x03, 0x22, // STA $2203 (CRVL)
        0xA9, 0x90,       // LDA #$90
        0x8D, 0x04, 0x22, // STA $2204 (CRVH) -> SA-1 reset vector = $9000
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x00, 0x22, // STA $2200 (CCNT) -> release SA-1 from reset
        0x4C, 0x19, 0x80, // JMP $8019 (idle loop)
    ];
    rom[0x0000..main_program.len()].copy_from_slice(&main_program);

    // SA-1 CPU program, bank $00:$9000 (file offset $1000).
    #[rustfmt::skip]
    let sa1_program: [u8; 15] = [
        0xA9, 0x01,       // LDA #$01
        0x8D, 0x2A, 0x22, // STA $222A (CIWP: write-enable chunk 0 from SA-1 side)
        0xAD, 0x00, 0x30, // LDA $3000 (read the marker byte)
        0x1A,             // INC A
        0x8D, 0x10, 0x30, // STA $3010 (write the transformed byte, still within chunk 0)
        0x4C, 0x0C, 0x90, // JMP $900C (idle loop)
    ];
    rom[0x1000..0x1000 + sa1_program.len()].copy_from_slice(&sa1_program);

    rom
}

#[test]
fn sa1_and_main_cpu_exchange_data_through_shared_iram() {
    let rom = build_sa1_iram_roundtrip_rom();
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "sa1-iram-roundtrip-test.sfc")
        .expect("failed to load SA-1 I-RAM roundtrip fixture ROM");

    for _ in 0..4000 {
        snes.run_tick();
    }

    assert_eq!(
        snes.read_bus_for_debugger_for_tests(0x00_3000),
        Some(0x99),
        "main CPU's marker byte should still be readable through the I-RAM mirror"
    );
    assert_eq!(
        snes.read_bus_for_debugger_for_tests(0x00_3010),
        Some(0x9A),
        "SA-1 should have read the marker byte and written back its increment"
    );
}
