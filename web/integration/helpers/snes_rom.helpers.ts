export function makeMinimalSnesRomBytes() {
    const rom = Buffer.alloc(0x10000);
    const header = 0x7FC0;
    rom.write("SNES TEST ROM        ", header, "ascii");
    rom[header + 0x3C] = 0x00;
    rom[header + 0x3D] = 0x80;
    rom[header + 0x15] = 0x20;
    rom[header + 0x16] = 0x00;
    rom[header + 0x17] = 0x07;
    rom[header + 0x18] = 0x00;
    rom[header + 0x19] = 0x00;
    rom[header + 0x1C] = 0x34;
    rom[header + 0x1D] = 0x12;
    rom[header + 0x1E] = 0xCB;
    rom[header + 0x1F] = 0xED;
    // NOP at $8000, then BRA -2 at $8001 to create a stable infinite loop.
    // Without this, the CPU falls through to BRK at $8002 which recursively
    // reads the zero-filled BRK vector ($FFFE/$FFFF = $0000) and loops through
    // RAM, potentially interfering with save-state timing during tests.
    rom[0x0000] = 0xEA; // NOP
    rom[0x0001] = 0x80; // BRA (relative branch, always taken on 65816)
    rom[0x0002] = 0xFE; // offset -2: PC after fetch = $8003; $8003 + (-2) = $8001 → infinite loop
    return rom;
}
