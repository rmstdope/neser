export function makeMinimalSnesRomBytes() {
    const rom = Buffer.alloc(0x10000);
    const header = 0x7FC0;
    rom.write("SNES TEST ROM        ", header, "ascii");
    rom[header + 0x3C] = 0x00;
    rom[header + 0x3D] = 0x80;
    rom[header + 0xD5] = 0x20;
    rom[header + 0xD6] = 0x00;
    rom[header + 0xD7] = 0x07;
    rom[header + 0xD8] = 0x00;
    rom[header + 0xD9] = 0x00;
    rom[header + 0xDC] = 0x34;
    rom[header + 0xDD] = 0x12;
    rom[header + 0xDE] = 0xCB;
    rom[header + 0xDF] = 0xED;
    rom[0x0000] = 0xEA;
    return rom;
}
