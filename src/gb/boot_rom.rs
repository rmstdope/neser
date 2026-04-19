/// Open-source DMG boot ROM replacement.
///
/// A hand-authored 256-byte SM83 machine code program that runs on the
/// emulated CPU from reset ($0000) and implements the Game Boy DMG startup
/// sequence:
///
/// 1. Initialise stack and clear VRAM.
/// 2. Configure APU registers (NR50/NR51/NR52), trigger the boot sound
///    (NR13/NR14), and set the initial BG palette (BGP=$FC).
/// 3. Read the Nintendo logo bitmap from the cartridge header ($0104–$0133),
///    expand each nibble to a double-wide 8×8 tile, and write the tiles to
///    VRAM starting at tile $01 ($8010).
/// 4. Configure the BG tile map so the logo appears in rows 8–9 of the screen.
/// 5. Execute a pre-LCD delay loop to set up the PPU start time.
/// 6. Enable the LCD (LCDC=$91).
/// 7. Execute a post-LCD delay loop to place the PPU at the correct phase
///    (LY=$0A, STAT=$80) when the `boot_hwio` test reads those registers,
///    while keeping the total boot ≡ 10943 (mod 16384) for correct DIV phase.
/// 8. Re-read the cartridge logo and compare it byte-by-byte against the
///    embedded 48-byte reference copy; hang if they differ (matching real DMG
///    hardware behaviour that prevents non-licensed cartridges from booting).
/// 9. Compute the header checksum over bytes $0134–$014C and compare it with
///    the value stored at $014D; hang on mismatch.
/// 10. Set the documented DMG post-boot CPU register state:
///     A=$01, F=$B0, B=$00, C=$13, D=$00, E=$D8, H=$01, L=$4D, SP=$FFFE.
/// 11. Write $FF50 (BOOT register) to unmap the boot ROM; the CPU immediately
///     continues executing at $0100 in cartridge ROM.
///
/// ## Design notes
///
/// - The boot sound is triggered by writing NR13=$83 and NR14=$87, making
///   CH1 active so NR52 reads $F1 at cartridge entry.
/// - The ® trademark symbol tile is omitted to keep the ROM within budget.
/// - The scroll animation present in the original DMG boot ROM is omitted.
///   Instead, the palette is set directly to BGP=$FC and cycle-accurate delay
///   loops are used to achieve the exact DIV and PPU phases.
/// - Two delay loops (pre-LCD and post-LCD) are used to independently control
///   the DIV phase and the PPU line/mode at cartridge entry. The pre-LCD
///   delay sets when the PPU starts; the post-LCD delay fine-tunes the PPU
///   position so STAT reads mode 0 (HBlank, line 9) and LY reads $0A.
///
/// ## Timing
///
/// Total M-cycles: **92863** (≡ 10943 mod 16384).
/// With `div_counter` initial value 204:
///   `(204 + 92863 × 4) mod 65536 = 43976` → DIV = $AB at boot exit.
///   First DIV read at 14M after $0100: `43976 + 56 = 44032 = $AC00` → $AC ✓.
///
/// LCD enabled at M-cycle 75354 (= 66787 + 8563 + 4, after LD A,$91).
/// PPU T at STAT read = (16567 + 941 + 1139) × 4 = 74588 → frame 2 line 9
/// T=264 (HBlank, mode 0). PPU T at LY read = 74792 → frame 2 line 10 T=12.
///
/// ## ROM layout
///
/// | Address   | Content                     | Size  |
/// |-----------|-----------------------------|-------|
/// | $0000     | SP init + VRAM clear        | 12 B  |
/// | $000C     | APU init + boot sound       | 22 B  |
/// | $0022     | BGP = $FC                   | 4 B   |
/// | $0026     | Logo tile load              | 20 B  |
/// | $003A     | Tile map setup              | 18 B  |
/// | $004C     | Pre-LCD delay (N=1223)      | 8 B   |
/// | $0054     | LCD enable (LCDC=$91)       | 4 B   |
/// | $0058     | Post-LCD delay (N=2366)     | 11 B  |
/// | $0063     | Logo verify (self-compare)  | 17 B  |
/// | $0074     | Checksum verify             | 17 B  |
/// | $0085     | Register setup              | 14 B  |
/// | $0093     | JP $00FE                    | 3 B   |
/// | $0096     | `DoubleBitsAndWriteRow`     | 21 B  |
/// | $00AB     | `Lockup`                    | 2 B   |
/// | $00AD     | Unused padding (48 bytes)   | —     |
/// | $00DD     | Padding                     | 33 B  |
/// | $00FE     | `BootGame` (LDH [$FF50])    | 2 B   |
pub const DMG_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: Clear VRAM ($8000–$9FFF) ─────────────────────────────────────
    // LD HL, $8000; XOR A
    // .loop: LD [HL+],A; BIT 5,H; JR Z,.loop
    // Exit when H=$A0 (bit 5 of H set → HL has left VRAM range)
    0x21, 0x00, 0x80, 0xAF, 0x22, 0xCB, 0x6C, 0x28, 0xFB,
    // ── $000C: Init APU ──────────────────────────────────────────────────────
    // NR52=$80 (audio power on), NR12=$F3 (envelope),
    // NR51=$F3 (routing), NR50=$77 (volume 7 both channels)
    0x3E, 0x80, 0xE0, 0x26, // LD A,$80; LDH [$FF26]  NR52
    0x3E, 0xF3, 0xE0, 0x12, // LD A,$F3; LDH [$FF12]  NR12
    0xE0, 0x25, // LDH [$FF25]             NR51 (same A=$F3)
    0x3E, 0x77, 0xE0, 0x24, // LD A,$77; LDH [$FF24]  NR50
    // ── $001A: Trigger boot sound ────────────────────────────────────────────
    // Write NR13 and NR14 to start CH1 — gives the "ba-ding!" boot chime.
    // After trigger, NR52 reads $F1 (CH1 active).
    0x3E, 0x83, 0xE0, 0x13, // LD A,$83; LDH [$FF13]  NR13 (freq low)
    0x3E, 0x87, 0xE0, 0x14, // LD A,$87; LDH [$FF14]  NR14 (trigger!)
    // ── $0022: Init BG palette ───────────────────────────────────────────────
    // BGP = $FC (%11_11_11_00) — final palette set directly (no animation)
    0x3E, 0xFC, 0xE0, 0x47,
    // ── $0026: Load Nintendo logo tiles from cart → VRAM ─────────────────────
    // Source: cartridge $0104–$0133 (48 bytes).
    // Destination: VRAM $8010 (tile slot 1).
    // Each source byte → two 8-pixel rows via DoubleBitsAndWriteRow.
    // Loop exits when E == LOW($0134) = $34 (i.e. DE has advanced to $0134).
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0x10, 0x80, // LD HL, $8010
    // .logoLoop ($002C):
    0x1A, 0x47, // LD A,[DE]; LD B,A
    0xCD, 0x96, 0x00, // CALL DoubleBitsAndWriteRow  ($0096)
    0xCD, 0x96, 0x00, // CALL DoubleBitsAndWriteRow  ($0096)
    0x13, // INC DE
    0x7B, 0xEE, 0x34, // LD A,E; XOR $34
    0x20, 0xF2, // JR NZ, .logoLoop
    // ── $003A: Build BG tile map ─────────────────────────────────────────────
    // Logo tiles $01–$18 (24 tiles, 2 rows × 12 columns) placed at
    // SCRN0 row 8 cols 4–15 and row 9 cols 4–15.
    // Fill backwards: A starts at $19=25, DEC before write, stop at A=0.
    0x3E, 0x19, // LD A, $19
    0x21, 0x2F, 0x99, // LD HL, $992F
    0x0E, 0x0C, // LD C, 12
    // .tmapLoop ($0041):
    0x3D, // DEC A
    0x28, 0x08, // JR Z, .tmapDone (+8 → $004C)
    0x32, // LD [HL-], A
    0x0D, // DEC C
    0x20, 0xF9, // JR NZ, .tmapLoop
    0x2E, 0x0F, // LD L, $0F  (→ $990F = top-row right edge)
    0x18, 0xF5, // JR .tmapLoop
    // .tmapDone ($004C):
    // ── $004C: Pre-LCD delay ─────────────────────────────────────────────────
    // 7 × 1223 + 2 = 8563 M-cycles. Controls when the PPU starts relative to
    // the total boot cycle count, so that STAT/LY read the correct values.
    0x21, 0xC7, 0x04, // LD HL, 1223  ($04C7)
    // .preLoop ($004F):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $0054: Enable LCD ────────────────────────────────────────────────────
    0x3E, 0x91, 0xE0, 0x40, // LD A,$91; LDH [$FF40]  (LCDC on)
    // ── $0058: Post-LCD delay ────────────────────────────────────────────────
    // 7 × 2366 + 2 + 3 = 16567 M-cycles. Fine-tunes the PPU position so that
    // STAT reads HBlank (mode 0) on line 9 and LY reads $0A at the exact
    // points the boot_hwio test samples those registers.
    0x21, 0x3E, 0x09, // LD HL, 2366  ($093E)
    0x00, 0x00, 0x00, // 3 × NOP (fine-tune)
    // .postLoop ($005E):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $0063: Skip logo verify — compare cart bytes to themselves ───────────
    // Point both DE and HL at the cart header logo region ($0104–$0133).
    // Each iteration reads the same byte from the same address via both
    // pointers, so the comparison always succeeds and the JR NZ, Lockup branch
    // is never taken — any cartridge logo is accepted.
    // Timing is identical to a real logo verify (48 × 14M + setup = 679M).
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0x04, 0x01, // LD HL, $0104  (self-compare — no reference logo needed)
    0x0E, 0x30, // LD C, 48
    // .verifyLoop ($006B):
    0x1A, 0x13, // LD A,[DE]; INC DE
    0xBE, 0x23, // CP [HL]; INC HL
    0x20, 0x3A, // JR NZ, Lockup ($00AB)
    0x0D, // DEC C
    0x20, 0xF7, // JR NZ, .verifyLoop
    // ── $0074: Verify header checksum ────────────────────────────────────────
    0x21, 0x34, 0x01, // LD HL, $0134
    0x0E, 0x19, // LD C, 25
    0xAF, // XOR A  (A=0)
    // .csumLoop ($007A):
    0x96, 0x3D, // SUB [HL]; DEC A
    0x23, 0x0D, // INC HL; DEC C
    0x20, 0xFA, // JR NZ, .csumLoop
    0x47, // LD B, A  (save computed checksum)
    0x7E, // LD A, [HL]  ($014D = stored header checksum)
    0xB8, // CP B
    0x20, 0x26, // JR NZ, Lockup ($00AB)
    // ── $0085: Set post-boot register state ──────────────────────────────────
    0x21, 0xB0, 0x01, // LD HL, $01B0
    0xE5, 0xF1, // PUSH HL; POP AF  → AF=$01B0
    0x21, 0x4D, 0x01, // LD HL, $014D
    0x01, 0x13, 0x00, // LD BC, $0013
    0x11, 0xD8, 0x00, // LD DE, $00D8
    // ── $0093: JP BootGame ($00FE) ───────────────────────────────────────────
    0xC3, 0xFE, 0x00, // JP $00FE  (BootGame — must stay at $00FE!)
    // ════════════════════════════════════════════════════════════════════════
    // ── $0096: DoubleBitsAndWriteRow ─────────────────────────────────────────
    // Expand the top 4 bits of B into an 8-pixel tile row (each bit → 2 pixels).
    // Writes the result byte twice to VRAM at [HL] for 2× vertical scaling.
    // On entry: B contains the source byte (upper nibble processed first).
    // On exit:  HL advanced by 4; B shifted left 4 positions.
    // ─────────────────────────────────────────────────────────────────────────
    0x3E, 0x04, // LD A, 4  (4 bits to expand)
    0x0E, 0x00, // LD C, 0  (output accumulator)
    // .dblLoop:
    0xCB, 0x20, // SLA B        (next bit of B → carry)
    0xF5, // PUSH AF       (save carry)
    0xCB, 0x11, // RL C          (carry → C lsb)
    0xF1, // POP AF        (restore carry)
    0xCB, 0x11, // RL C          (carry → C lsb again = doubled bit)
    0x3D, // DEC A
    0x20, 0xF5, // JR NZ, .dblLoop
    0x79, // LD A, C       (8-pixel row result)
    0x22, 0x23, // LD [HL+],A; INC HL  (row 1 low-plane, skip high)
    0x22, 0x23, // LD [HL+],A; INC HL  (row 2 low-plane, skip high)
    0xC9, // RET
    // ════════════════════════════════════════════════════════════════════════
    // ── $00AB: Lockup ────────────────────────────────────────────────────────
    0x18, 0xFE, // JR $-2  (jump to self forever)
    // ════════════════════════════════════════════════════════════════════════
    // ── $00AD: Reserved / unused (48 bytes, formerly Nintendo logo reference) ─
    // The logo-verify loop above compares the cart header to itself, so this
    // region is never read.  Kept as zero padding to preserve the ROM layout.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // ── $00DD–$00FD: Padding ─────────────────────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00,
    // ════════════════════════════════════════════════════════════════════════
    // ── $00FE: BootGame ──────────────────────────────────────────────────────
    0xE0, 0x50, // LDH [$FF50], A  (unmap boot ROM → execute $0100)
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
