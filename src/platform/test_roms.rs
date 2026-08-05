//! Minimal synthetic ROMs shared by platform-level tests.
//!
//! Each builder produces the smallest byte sequence the corresponding core
//! accepts from `Emulator::load_rom`, so tests that only care about *which*
//! console was constructed do not need a real ROM asset on disk. Tests that
//! need a ROM to render meaningful pixels use the committed ROMs under
//! `roms/` instead.

/// A 16 KB NROM image whose reset vector points at an infinite `JMP` loop.
///
/// `pal` sets the iNES flags9 TV-system bit, which is what
/// `Cartridge::rom_timing_mode` reads.
pub fn minimal_nes_rom(pal: bool) -> Vec<u8> {
    let mut rom = Vec::with_capacity(16 + 16 * 1024);
    rom.extend_from_slice(b"NES\x1A");
    rom.push(1); // 16 KB PRG
    rom.push(0); // no CHR ROM (CHR RAM)
    rom.push(0x00); // flags6
    rom.push(0x00); // flags7
    rom.push(0x00); // flags8
    rom.push(if pal { 0x01 } else { 0x00 }); // flags9: TV system
    rom.extend_from_slice(&[0u8; 6]); // padding to a 16-byte header

    let mut prg = vec![0xEAu8; 16 * 1024]; // NOPs
    // Reset vector at $FFFC -> $C000
    prg[0x3FFC] = 0x00;
    prg[0x3FFD] = 0xC0;
    // JMP $C000 at $C000
    prg[0x0000] = 0x4C;
    prg[0x0001] = 0x00;
    prg[0x0002] = 0xC0;
    rom.extend_from_slice(&prg);
    rom
}

/// A 32 KB ROM-only Game Boy image with a valid header checksum.
pub fn minimal_gb_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x00; // ROM only
    rom[0x0148] = 0x00; // 32 KB
    rom[0x0149] = 0x00; // no cartridge RAM
    rom[0x014D] = rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &byte| acc.wrapping_sub(byte).wrapping_sub(1));
    rom
}

/// A GBA image carrying the fixed byte and header complement the loader checks.
pub fn minimal_gba_rom() -> Vec<u8> {
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
    };

    let mut rom = vec![0u8; 0xC0];
    rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
    rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
    rom
}

/// A 64 KB LoROM image with an infinite loop at the reset vector.
pub fn minimal_snes_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x10000];

    let header = 0x7FC0;
    let title = b"NESER PLATFORM TEST";
    rom[header..header + 21].fill(b' ');
    rom[header..header + title.len()].copy_from_slice(title);
    rom[header + 0x15] = 0x20; // LoROM, slow
    rom[header + 0x16] = 0x00; // ROM only
    // Size is encoded as (1 SHL n) KB (fullsnes, "Cartridge Header"), so a
    // 64 KB image is n=6. Rounding up applies only to non-power-of-two carts.
    rom[header + 0x17] = 0x06; // 1 << 6 = 64 KB, matching the buffer above
    rom[header + 0x18] = 0x00; // no SRAM
    rom[header + 0x1C] = 0x34; // checksum complement
    rom[header + 0x1D] = 0x12;
    rom[header + 0x1E] = 0xCB; // checksum
    rom[header + 0x1F] = 0xED;
    rom[header + 0x3C] = 0x00; // reset vector -> $8000
    rom[header + 0x3D] = 0x80;

    // BRA to self at $8000, so the CPU idles instead of running off into RAM.
    rom[0x0000] = 0x80;
    rom[0x0001] = 0xFE;
    rom
}
