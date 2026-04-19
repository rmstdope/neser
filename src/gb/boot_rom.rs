/// Open-source DMG boot ROM replacement.
///
/// ## ROM layout
///
/// | Address   | Content                        | Size  |
/// |-----------|--------------------------------|-------|
/// | $0000     | SP init + VRAM clear           | 12 B  |
/// | $000C     | APU init + boot sound          | 22 B  |
/// | $0022     | BGP = $FC                      | 4 B   |
/// | $0026     | Logo tile load (CALL $00A7)    | 20 B  |
/// | $003A     | Tile map setup                 | 18 B  |
/// | $004C     | Pre-LCD delay (N=2046, 2 NOP)  | 10 B  |
/// | $0056     | LCD enable (LCDC=$91)          | 4 B   |
/// | $005A     | SCY = $60                      | 4 B   |
/// | $005E     | Scroll animation (48 frames)   | 23 B  |
/// | $0075     | Hold (3 frames)                | 18 B  |
/// | $0087     | IF = $E1                       | 4 B   |
/// | $008B     | Fine-tune delay (N=2517, 3 NOP)| 11 B  |
/// | $0096     | Register setup                 | 14 B  |
/// | $00A4     | JP $00FE                       | 3 B   |
/// | $00A7     | `DoubleBitsAndWriteRow`        | 21 B  |
/// | $00BC     | Padding                        | 66 B  |
/// | $00FE     | `BootGame` (LDH [$FF50])       | 2 B   |
pub const DMG_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: Clear VRAM ($8000–$9FFF) ─────────────────────────────────────
    0x21, 0x00, 0x80, 0xAF, 0x22, 0xCB, 0x6C, 0x28, 0xFB,
    // ── $000C: Init APU ──────────────────────────────────────────────────────
    0x3E, 0x80, 0xE0, 0x26, // LD A,$80; LDH [$FF26]  NR52
    0x3E, 0xF3, 0xE0, 0x12, // LD A,$F3; LDH [$FF12]  NR12
    0xE0, 0x25, // LDH [$FF25]             NR51
    0x3E, 0x77, 0xE0, 0x24, // LD A,$77; LDH [$FF24]  NR50
    // ── $001A: Trigger boot sound ────────────────────────────────────────────
    0x3E, 0x83, 0xE0, 0x13, // LD A,$83; LDH [$FF13]  NR13
    0x3E, 0x87, 0xE0, 0x14, // LD A,$87; LDH [$FF14]  NR14 (trigger!)
    // ── $0022: Init BG palette ───────────────────────────────────────────────
    0x3E, 0xFC, 0xE0, 0x47, // LD A,$FC; LDH [$FF47]  BGP
    // ── $0026: Load logo tiles from cartridge header → VRAM ──────────────────
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0x10, 0x80, // LD HL, $8010
    // .logoLoop ($002C):
    0x1A, 0x47, // LD A,[DE]; LD B,A
    0xCD, 0xA7, 0x00, // CALL DoubleBitsAndWriteRow ($00A7)
    0xCD, 0xA7, 0x00, // CALL DoubleBitsAndWriteRow ($00A7)
    0x13, // INC DE
    0x7B, 0xEE, 0x34, // LD A,E; XOR $34
    0x20, 0xF2, // JR NZ, .logoLoop
    // ── $003A: Build BG tile map ─────────────────────────────────────────────
    0x3E, 0x19, // LD A, $19
    0x21, 0x2F, 0x99, // LD HL, $992F
    0x0E, 0x0C, // LD C, 12
    // .tmapLoop ($0041):
    0x3D, // DEC A
    0x28, 0x08, // JR Z, .tmapDone (+8 → $004C)
    0x32, // LD [HL-], A
    0x0D, // DEC C
    0x20, 0xF9, // JR NZ, .tmapLoop
    0x2E, 0x0F, // LD L, $0F
    0x18, 0xF5, // JR .tmapLoop
    // .tmapDone ($004C):
    // ── $004C: Pre-LCD delay ─────────────────────────────────────────────────
    // 3 + 2 + 7×2046 + 2 = 14 329 M-cycles.
    0x21, 0xFE, 0x07, // LD HL, $07FE  (2046)
    0x00, 0x00, // 2 × NOP
    // .preLoop ($0051):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $0056: Enable LCD ────────────────────────────────────────────────────
    0x3E, 0x91, 0xE0, 0x40, // LD A,$91; LDH [$FF40]
    // ── $005A: SCY = $60 ─────────────────────────────────────────────────────
    0x3E, 0x60, 0xE0, 0x42, // LD A,$60; LDH [$FF42]
    // ── $005E: Scroll animation — 48 frames, SCY -= 2 each ──────────────────
    0x06, 0x30, // LD B, 48
    // .scrollFrame ($0060):
    0x21, 0xC9, 0x09, // LD HL, $09C9  (2505)
    // .scrollInner ($0063):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    0x00, 0x00, 0x00, 0x00, // 4 × NOP
    0xF0, 0x42, // LDH A, [$FF42]
    0x3D, // DEC A
    0x3D, // DEC A
    0xE0, 0x42, // LDH [$FF42], A
    0x05, // DEC B
    0x20, 0xEB, // JR NZ, .scrollFrame  (→ $0060, offset = -21)
    // ── $0075: Hold — 3 frames at SCY = $00 ─────────────────────────────────
    0x06, 0x03, // LD B, 3
    // .holdFrame ($0077):
    0x21, 0xCA, 0x09, // LD HL, $09CA  (2506)
    // .holdInner ($007A):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    0x00, 0x00, 0x00, 0x00, 0x00, // 5 × NOP
    0x05, // DEC B
    0x20, 0xF0, // JR NZ, .holdFrame  (→ $0077, offset = -16)
    // ── $0087: IF = $E1 ──────────────────────────────────────────────────────
    0x3E, 0xE1, 0xE0, 0x0F, // LD A,$E1; LDH [$FF0F]
    // ── $008B: Fine-tune delay ───────────────────────────────────────────────
    // 3 + 3 + 7×2517 + 2 = 17 627 M-cycles.
    0x21, 0xD5, 0x09, // LD HL, $09D5  (2517)
    0x00, 0x00, 0x00, // 3 × NOP
    // .fineLoop ($0091):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $0096: Set post-boot register state ──────────────────────────────────
    0x21, 0xB0, 0x01, // LD HL, $01B0
    0xE5, 0xF1, // PUSH HL; POP AF  → AF=$01B0
    0x21, 0x4D, 0x01, // LD HL, $014D
    0x01, 0x13, 0x00, // LD BC, $0013
    0x11, 0xD8, 0x00, // LD DE, $00D8
    // ── $00A4: JP BootGame ($00FE) ───────────────────────────────────────────
    0xC3, 0xFE, 0x00,
    // ════════════════════════════════════════════════════════════════════════
    // ── $00A7: DoubleBitsAndWriteRow ─────────────────────────────────────────
    0x3E, 0x04, // LD A, 4
    0x0E, 0x00, // LD C, 0
    // .dblLoop ($00AB):
    0xCB, 0x20, // SLA B
    0xF5, // PUSH AF
    0xCB, 0x11, // RL C
    0xF1, // POP AF
    0xCB, 0x11, // RL C
    0x3D, // DEC A
    0x20, 0xF5, // JR NZ, .dblLoop
    0x79, // LD A, C
    0x22, 0x23, // LD [HL+],A; INC HL
    0x22, 0x23, // LD [HL+],A; INC HL
    0xC9, // RET
    // ════════════════════════════════════════════════════════════════════════
    // ── $00BC–$00FD: Padding ─────────────────────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
    // ════════════════════════════════════════════════════════════════════════
    // ── $00FE: BootGame ──────────────────────────────────────────────────────
    0xE0, 0x50, // LDH [$FF50], A
];

/// Open-source DMG-0 (first production run) boot ROM replacement.
///
/// DMG-0 had a simpler boot ROM than later DMG revisions: no logo scroll
/// animation, no header verification. This replacement reproduces the exact
/// post-boot hardware state expected by the Mooneye `boot_hwio-dmg0` and
/// `boot_div-dmg0` acceptance tests, including the "ba-ding!" boot sound
/// (CH1 triggered via NR13/NR14).
///
/// ## Post-boot state produced
///
/// | Register | Value |    | I/O       | Value |
/// |----------|-------|----|-----------|-------|
/// | A        | $01   |    | DIV       | $18   |
/// | F        | $00   |    | LCDC      | $91   |
/// | B        | $FF   |    | BGP       | $FC   |
/// | C        | $13   |    | NR52      | $F1   |
/// | D        | $00   |    |           |       |
/// | E        | $C1   |    |           |       |
/// | H        | $84   |    |           |       |
/// | L        | $03   |    |           |       |
/// | SP       | $FFFE |    |           |       |
///
/// ## Timing
///
/// Total M-cycles: **17880** (≡ 1496 mod 16384, so DIV phase matches).
/// With `div_counter` initial value 204:
///   `204 + 17880 × 4 = 71724` → `71724 mod 65536 = 6188` → DIV = $18.
///
/// The `boot_div-dmg0` test first reads DIV 53 M-cycles after `$0100`:
///   `6188 + 53 × 4 = 6400 = $1900` → read returns $19 ✓.
///
/// LCD is enabled (`LCDC=$91`) at M-cycle 1322. The PPU runs for
/// 16558 M-cycles = 66232 T-cycles before boot exit, placing it in VBlank
/// (LY≈145). During the `boot_hwio` test's comparison loop the PPU
/// completes its first frame (70220 T, first scanline = 452 T), and by the
/// time the test reads `$FF41`/`$FF44` the PPU has wrapped into mode 3
/// of line 1, yielding STAT=$83 and LY=$01 as expected.
///
/// ## ROM layout
///
/// | Address | Content                                | M-cycles |
/// |---------|----------------------------------------|----------|
/// | $0000   | LD SP / APU init + sound / BGP         | 39       |
/// | $001F   | Pre-LCD delay loop (HL counter = 182)  | 1276     |
/// | $0027   | 2 × NOP (fine-tune timing)             | 2        |
/// | $0029   | Enable LCD (LCDC = $91)                | 5        |
/// | $002D   | Post-LCD delay loop (HL = 2362)        | 16536    |
/// | $0035   | 3 × NOP (fine-tune timing)             | 3        |
/// | $0038   | Set post-boot CPU registers            | 12       |
/// | $0044   | JP $00FE                               | 4        |
/// | $00FE   | LDH [$FF50], A (unmap boot ROM)        | 3        |
/// |         | **Total**                              | **17880**|
pub const DMG0_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: APU init + boot sound ─────────────────────────────────────────
    0x3E, 0x80, 0xE0, 0x26, // LD A,$80; LDH ($26),A  → NR52 = $80 (APU on)
    0xE0, 0x11, // LDH ($11),A            → NR11 = $80 (50% duty)
    0x3E, 0xF3, 0xE0, 0x12, // LD A,$F3; LDH ($12),A → NR12 = $F3 (envelope)
    0xE0, 0x25, // LDH ($25),A            → NR51 = $F3 (panning)
    0x3E, 0x77, 0xE0, 0x24, // LD A,$77; LDH ($24),A → NR50 = $77 (volume)
    0x3E, 0x83, 0xE0, 0x13, // LD A,$83; LDH ($13),A → NR13 = $83 (freq low)
    0x3E, 0x87, 0xE0, 0x14, // LD A,$87; LDH ($14),A → NR14 = $87 (trigger!)
    // ── $001B: BGP = $FC ─────────────────────────────────────────────────────
    0x3E, 0xFC, 0xE0, 0x47,
    // ── $001F: Pre-LCD delay loop (182 iterations × 7 M-cycles + overhead) ──
    // LD HL, 182  ($00B6)
    0x21, 0xB6, 0x00,
    // Loop: DEC HL; LD A,H; OR L; JR NZ → $0022  (7 M taken / 6 M last)
    0x2B, 0x7C, 0xB5, 0x20, 0xFB,
    // ── $0027: Fine-tune NOPs (2 × 1 M-cycle) ──────────────────────────────
    0x00, 0x00,
    // ── $0029: Enable LCD ────────────────────────────────────────────────────
    // LD A, $91  (LCDC: LCD on, BG on, BG tile data $8000, BG map $9800)
    0x3E, 0x91, // LDH ($FF40), A  — write LCDC; PPU begins at M-cycle 1322
    0xE0, 0x40,
    // ── $002D: Post-LCD delay loop (2362 iterations × 7 M-cycles + overhead)─
    // LD HL, 2362  ($093A)
    0x21, 0x3A, 0x09,
    // Loop: DEC HL; LD A,H; OR L; JR NZ → $0030  (7 M taken / 6 M last)
    0x2B, 0x7C, 0xB5, 0x20, 0xFB,
    // ── $0035: Fine-tune NOPs (3 × 1 M-cycle) ──────────────────────────────
    0x00, 0x00, 0x00,
    // ── $0038: Set post-boot CPU registers ───────────────────────────────────
    0x01, 0x13, 0xFF, // LD BC, $FF13  →  B=$FF, C=$13
    0x11, 0xC1, 0x00, // LD DE, $00C1  →  D=$00, E=$C1
    0x21, 0x03, 0x84, // LD HL, $8403  →  H=$84, L=$03
    0x3E, 0x01, // LD A, $01
    0xB7, // OR A          →  F=$00 (Z=0, N=0, H=0, C=0)
    // ── $0044: Jump to boot exit at $00FE ───────────────────────────────────
    0xC3, 0xFE, 0x00, // JP $00FE
    // ── $0047–$00FD: Unused (fill with $00) ─────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0047–$004E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $004F–$0056
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0057–$005E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $005F–$0066
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0067–$006E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $006F–$0076
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0077–$007E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $007F–$0086
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0087–$008E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $008F–$0096
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0097–$009E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $009F–$00A6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00A7–$00AE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00AF–$00B6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00B7–$00BE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00BF–$00C6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00C7–$00CE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00CF–$00D6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00D7–$00DE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00DF–$00E6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00E7–$00EE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00EF–$00F6
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $00F7–$00FD
    // ── $00FE: BootGame ──────────────────────────────────────────────────────
    // Placed at $00FE so that PC = $0100 after the instruction executes,
    // which is the standard cartridge entry point.
    0xE0, 0x50, // LDH [$FF50], A  (unmap boot ROM → execute $0100)
];
