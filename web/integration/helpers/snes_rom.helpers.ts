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
    rom[0x0000] = 0xEA;
    return rom;
}
