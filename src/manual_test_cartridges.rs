//! Manual test ROMs for debugging audio/PPU/CPU behavior.

/// Returns an iNES ROM image (as bytes) for a minimal NROM-128 cartridge that:
/// - enables triangle only
/// - sets a steady audible triangle tone
/// - loops forever
///
/// Intended for manual/emulator debugging, not automated correctness.
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
    prg[vector_base + 0] = 0x00;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_only_nrom_128_has_valid_ines_header_and_vectors() {
        let rom = triangle_only_nrom_128();

        // iNES header must exist.
        assert!(rom.len() >= 16, "ROM must include iNES header");

        // Header magic: "NES\x1A"
        assert_eq!(&rom[0..4], b"NES\x1A");

        // NROM-128: 1 x 16KB PRG, 0 x 8KB CHR (CHR RAM)
        assert_eq!(rom[4], 1, "Expected 16KB PRG ROM");
        assert_eq!(rom[5], 0, "Expected CHR RAM (0 CHR ROM banks)");

        // Flags 6/7 should keep mapper 0.
        let flags6 = rom[6];
        let flags7 = rom[7];
        let mapper = (flags7 & 0xF0) | (flags6 >> 4);
        assert_eq!(mapper, 0, "Expected mapper 0 (NROM)");

        // PRG data must exist: header (16) + PRG (16384)
        assert!(rom.len() >= 16 + 16 * 1024, "ROM must include 16KB PRG");

        // Vectors are at the end of the PRG area for NROM-128 mapped at $C000-$FFFF.
        // We expect reset vector to point to $C000.
        let prg_start = 16;
        let prg_end = prg_start + 16 * 1024;
        let vector_base = prg_end - 6;

        let nmi = u16::from_le_bytes([rom[vector_base], rom[vector_base + 1]]);
        let reset = u16::from_le_bytes([rom[vector_base + 2], rom[vector_base + 3]]);
        let irq = u16::from_le_bytes([rom[vector_base + 4], rom[vector_base + 5]]);

        // For a minimal cartridge, it's fine if NMI/IRQ also point at reset.
        assert_eq!(reset, 0xC000, "Reset vector should jump to program start");
        assert_eq!(nmi, 0xC000, "NMI vector should be safe");
        assert_eq!(irq, 0xC000, "IRQ vector should be safe");
    }
}
