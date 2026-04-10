//! Manual test ROMs for debugging audio/PPU/CPU behavior.

/// Returns an iNES ROM image (as bytes) for a minimal NROM-128 cartridge that:
/// - enables triangle only
/// - sets a steady audible triangle tone
/// - loops forever
///
/// Intended for manual/emulator debugging, not automated correctness.
#[allow(dead_code)]
pub fn triangle_only_nrom_128() -> Vec<u8> {
    // iNES header (16 bytes)
    // - NROM-128 (16KB PRG)
    // - 0 CHR ROM (CHR RAM)
    // - Mapper 0
    let mut rom = Vec::with_capacity(16 + 16 * 1024);
    rom.extend_from_slice(b"NES\x1A");
    rom.push(1); // 16KB PRG
    rom.push(0); // 0 CHR (CHR RAM)
    rom.push(0x00); // flags6
    rom.push(0x00); // flags7
    rom.extend_from_slice(&[0u8; 8]); // padding

    // PRG ROM (16KB), mapped at $C000-$FFFF.
    // Fill with NOPs by default.
    let mut prg = vec![0xEAu8; 16 * 1024];

    // Program entry point at CPU address $C000 => PRG offset 0x0000.
    // 6502 program:
    //   SEI
    //   CLD
    //   LDX #$FF
    //   TXS
    //   LDA #$40
    //   STA $4017    ; disable frame IRQ (safe)
    //   LDA #$04
    //   STA $4015    ; enable triangle only
    //   LDA #$FF
    //   STA $4008    ; control=1 (halt length), linear reload=127
    //   LDA #$7E
    //   STA $400A    ; timer low
    //   LDA #$00
    //   STA $400B    ; length index=0, timer high=0 (period = $007E ~ 440 Hz)
    // forever:
    //   JMP forever
    let program: [u8; 33] = [
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0xA9, 0x40, // LDA #$40
        0x8D, 0x17, 0x40, // STA $4017
        0xA9, 0x04, // LDA #$04
        0x8D, 0x15, 0x40, // STA $4015
        0xA9, 0xFF, // LDA #$FF
        0x8D, 0x08, 0x40, // STA $4008
        0xA9, 0x7E, // LDA #$7E
        0x8D, 0x0A, 0x40, // STA $400A
        0xA9, 0x00, // LDA #$00
        0x8D, 0x0B, 0x40, // STA $400B
        0x4C, 0x1C, 0xC0, // JMP $C01C (forever)
    ];
    prg[0..program.len()].copy_from_slice(&program);

    // Interrupt vectors are at $FFFA-$FFFF, which correspond to the last 6 bytes
    // of PRG ROM for NROM-128.
    let vector_base = prg.len() - 6;
    // NMI
    prg[vector_base] = 0x00;
    prg[vector_base + 1] = 0xC0;
    // RESET
    prg[vector_base + 2] = 0x00;
    prg[vector_base + 3] = 0xC0;
    // IRQ/BRK
    prg[vector_base + 4] = 0x00;
    prg[vector_base + 5] = 0xC0;

    rom.extend_from_slice(&prg);
    rom
}

/// Returns an iNES ROM image (as bytes) for a minimal NROM-128 cartridge that:
/// - enables pulse 1 only
/// - sets a steady audible square wave tone
/// - loops forever
#[allow(dead_code)]
pub fn pulse1_only_nrom_128() -> Vec<u8> {
    // iNES header (16 bytes)
    let mut rom = Vec::with_capacity(16 + 16 * 1024);
    rom.extend_from_slice(b"NES\x1A");
    rom.push(1); // 16KB PRG
    rom.push(0); // 0 CHR (CHR RAM)
    rom.push(0x00); // flags6
    rom.push(0x00); // flags7
    rom.extend_from_slice(&[0u8; 8]); // padding

    let mut prg = vec![0xEAu8; 16 * 1024];

    // Program at $C000:
    // - disable frame IRQ
    // - enable pulse 1 only ($4015)
    // - configure pulse 1 for constant volume and looping length (infinite)
    // - set timer for an audible tone
    // - loop forever
    //
    // Pulse frequency (approx): f = CPU / (16 * (timer + 1))
    // For ~440Hz: timer ≈ 0x00FD.
    let program: [u8; 33] = [
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0xA9, 0x40, // LDA #$40
        0x8D, 0x17, 0x40, // STA $4017
        0xA9, 0x01, // LDA #$01
        0x8D, 0x15, 0x40, // STA $4015 (pulse 1 enable)
        0xA9, 0xBF, // LDA #$BF (duty=50%, halt length, constant volume=15)
        0x8D, 0x00, 0x40, // STA $4000
        0xA9, 0xFD, // LDA #$FD
        0x8D, 0x02, 0x40, // STA $4002 (timer low)
        0xA9, 0x00, // LDA #$00
        0x8D, 0x03, 0x40, // STA $4003 (timer high=0, length index=0)
        0x4C, 0x20, 0xC0, // JMP $C020 (forever)
    ];
    prg[0..program.len()].copy_from_slice(&program);

    let vector_base = prg.len() - 6;
    // NMI
    prg[vector_base] = 0x00;
    prg[vector_base + 1] = 0xC0;
    // RESET
    prg[vector_base + 2] = 0x00;
    prg[vector_base + 3] = 0xC0;
    // IRQ/BRK
    prg[vector_base + 4] = 0x00;
    prg[vector_base + 5] = 0xC0;

    rom.extend_from_slice(&prg);
    rom
}

/// Returns an iNES ROM image (as bytes) for a minimal NROM-128 cartridge that:
/// - enables pulse 2 only
/// - sets a steady audible square wave tone
/// - loops forever
#[allow(dead_code)]
pub fn pulse2_only_nrom_128() -> Vec<u8> {
    // iNES header (16 bytes)
    let mut rom = Vec::with_capacity(16 + 16 * 1024);
    rom.extend_from_slice(b"NES\x1A");
    rom.push(1); // 16KB PRG
    rom.push(0); // 0 CHR (CHR RAM)
    rom.push(0x00); // flags6
    rom.push(0x00); // flags7
    rom.extend_from_slice(&[0u8; 8]); // padding

    let mut prg = vec![0xEAu8; 16 * 1024];

    // Pulse frequency (approx): f = CPU / (16 * (timer + 1))
    // For ~440Hz: timer ≈ 0x00FD.
    let program: [u8; 33] = [
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0xA9, 0x40, // LDA #$40
        0x8D, 0x17, 0x40, // STA $4017
        0xA9, 0x02, // LDA #$02
        0x8D, 0x15, 0x40, // STA $4015 (pulse 2 enable)
        0xA9, 0xBF, // LDA #$BF (duty=50%, halt length, constant volume=15)
        0x8D, 0x04, 0x40, // STA $4004
        0xA9, 0xFD, // LDA #$FD
        0x8D, 0x06, 0x40, // STA $4006 (timer low)
        0xA9, 0x00, // LDA #$00
        0x8D, 0x07, 0x40, // STA $4007 (timer high=0, length index=0)
        0x4C, 0x20, 0xC0, // JMP $C020 (forever)
    ];
    prg[0..program.len()].copy_from_slice(&program);

    let vector_base = prg.len() - 6;
    // NMI
    prg[vector_base] = 0x00;
    prg[vector_base + 1] = 0xC0;
    // RESET
    prg[vector_base + 2] = 0x00;
    prg[vector_base + 3] = 0xC0;
    // IRQ/BRK
    prg[vector_base + 4] = 0x00;
    prg[vector_base + 5] = 0xC0;

    rom.extend_from_slice(&prg);
    rom
}

/// Returns an iNES ROM image (as bytes) for a minimal NROM-128 cartridge that:
/// - enables noise only
/// - sets a steady audible noise output
/// - loops forever
#[allow(dead_code)]
pub fn noise_only_nrom_128() -> Vec<u8> {
    // iNES header (16 bytes)
    let mut rom = Vec::with_capacity(16 + 16 * 1024);
    rom.extend_from_slice(b"NES\x1A");
    rom.push(1); // 16KB PRG
    rom.push(0); // 0 CHR (CHR RAM)
    rom.push(0x00); // flags6
    rom.push(0x00); // flags7
    rom.extend_from_slice(&[0u8; 8]); // padding

    let mut prg = vec![0xEAu8; 16 * 1024];

    // Noise:
    // - constant volume=15, length-halt (loop) so it keeps playing
    // - mode=0 (long) and a relatively low noise frequency (period index 0x0F)
    let program: [u8; 33] = [
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0xA9, 0x40, // LDA #$40
        0x8D, 0x17, 0x40, // STA $4017
        0xA9, 0x08, // LDA #$08
        0x8D, 0x15, 0x40, // STA $4015 (noise enable)
        0xA9, 0xBF, // LDA #$BF (halt length, constant volume=15)
        0x8D, 0x0C, 0x40, // STA $400C
        0xA9, 0x0F, // LDA #$0F (period index)
        0x8D, 0x0E, 0x40, // STA $400E
        0xA9, 0x00, // LDA #$00 (length index=0)
        0x8D, 0x0F, 0x40, // STA $400F
        0x4C, 0x20, 0xC0, // JMP $C020 (forever)
    ];
    prg[0..program.len()].copy_from_slice(&program);

    let vector_base = prg.len() - 6;
    // NMI
    prg[vector_base] = 0x00;
    prg[vector_base + 1] = 0xC0;
    // RESET
    prg[vector_base + 2] = 0x00;
    prg[vector_base + 3] = 0xC0;
    // IRQ/BRK
    prg[vector_base + 4] = 0x00;
    prg[vector_base + 5] = 0xC0;

    rom.extend_from_slice(&prg);
    rom
}
