//! Black-box coverage for issue #2959 (SA-1 BW-RAM mapping and write protection): a hand-built
//! SA-1-chipset ROM where the main CPU enables SNES-side BW-RAM writes (`$2226` SBWE), writes a
//! marker byte into the mappable `$6000-$7FFF` window, releases SA-1, and the SA-1 CPU -- after
//! enabling its own `$2227` CBWE -- reads that byte through its *own* `$6000-$7FFF` window (same
//! physical BW-RAM, since both sides default to block 0) and writes a transformed copy back for
//! the main CPU to observe.

use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;

const HEADER: usize = 0x7FC0;

fn write_lorom_header(rom: &mut [u8], title: &[u8], chipset: u8, ram_size_field: u8) {
    rom[HEADER..HEADER + 21].fill(b' ');
    rom[HEADER..HEADER + title.len()].copy_from_slice(title);
    rom[HEADER + 0x15] = 0x20; // Map mode: LoROM, slow.
    rom[HEADER + 0x16] = chipset;
    rom[HEADER + 0x17] = 0x07; // ROM size.
    rom[HEADER + 0x18] = ram_size_field;
    rom[HEADER + 0x1C] = 0x34; // Complement check (not validated by this codebase).
    rom[HEADER + 0x1D] = 0x12;
    rom[HEADER + 0x1E] = 0xCB; // Checksum (not validated by this codebase).
    rom[HEADER + 0x1F] = 0xED;
    rom[HEADER + 0x3C] = 0x00; // Main CPU reset vector low byte.
    rom[HEADER + 0x3D] = 0x80; // Main CPU reset vector high byte -> $8000.
}

/// Builds a 64 KiB LoROM SA-1 ROM (chipset `$35`, 32KB BW-RAM) whose main-CPU program at bank
/// `$00:$8000` enables SNES-side BW-RAM writes, stores a marker byte (`$77`) into BW-RAM via the
/// `$6000-$7FFF` window, points the SA-1 reset vector at `$9000`, releases SA-1, then idles; and
/// whose SA-1-side program at bank `$00:$9000` enables its own BW-RAM writes, reads the marker
/// byte back through its own `$6000-$7FFF` window, increments it, stores the result at BW-RAM
/// offset `$10`, then idles.
fn build_sa1_bwram_roundtrip_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_header(&mut rom, b"SA1 BWRAM TEST", 0x35, 0x05);

    // Main CPU program, bank $00:$8000 (file offset $0000).
    #[rustfmt::skip]
    let main_program: [u8; 28] = [
        0xA9, 0x80,       // LDA #$80
        0x8D, 0x26, 0x22, // STA $2226 (SBWE: SNES-side BW-RAM write enable)
        0xA9, 0x77,       // LDA #$77
        0x8D, 0x00, 0x60, // STA $6000 (marker byte into BW-RAM, block 0 default)
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
        0xA9, 0x80,       // LDA #$80
        0x8D, 0x27, 0x22, // STA $2227 (CBWE: SA-1-side BW-RAM write enable)
        0xAD, 0x00, 0x60, // LDA $6000 (read the marker byte through SA-1's own window)
        0x1A,             // INC A
        0x8D, 0x10, 0x60, // STA $6010 (write the transformed byte, block 0, offset $10)
        0x4C, 0x0C, 0x90, // JMP $900C (idle loop)
    ];
    rom[0x1000..0x1000 + sa1_program.len()].copy_from_slice(&sa1_program);

    rom
}

#[test]
fn sa1_and_main_cpu_exchange_data_through_shared_bwram() {
    let rom = build_sa1_bwram_roundtrip_rom();
    let mut snes = Snes::new(crate::snes::test_support::snes_test_app_context());
    snes.load_rom(&rom, "sa1-bwram-roundtrip-test.sfc")
        .expect("failed to load SA-1 BW-RAM roundtrip fixture ROM");

    for _ in 0..4000 {
        snes.run_tick();
    }

    assert_eq!(
        snes.read_bus_for_debugger_for_tests(0x00_6000),
        Some(0x77),
        "main CPU's marker byte should still be readable through the BW-RAM window"
    );
    assert_eq!(
        snes.read_bus_for_debugger_for_tests(0x00_6010),
        Some(0x78),
        "SA-1 should have read the marker byte and written back its increment"
    );
}
