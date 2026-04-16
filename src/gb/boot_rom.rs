/// Open-source DMG boot ROM replacement.
///
/// A hand-authored 256-byte SM83 machine code program that runs on the
/// emulated CPU from reset ($0000) and implements the Game Boy DMG startup
/// sequence:
///
/// 1. Initialise stack and clear VRAM.
/// 2. Configure APU registers (NR50/NR51/NR52) and initial BG palette.
/// 3. Read the Nintendo logo bitmap from the cartridge header ($0104–$0133),
///    expand each nibble to a double-wide 8×8 tile, and write the tiles to
///    VRAM starting at tile $01 ($8010).
/// 4. Configure the BG tile map so the logo appears in rows 8–9 of the screen.
/// 5. Set SCY=30 and animate the logo scrolling down over 15 VBlank frames.
/// 6. After the scroll, darken the palette and wait ~1 second.
/// 7. Re-read the cartridge logo and compare it byte-by-byte against the
///    embedded 48-byte reference copy; hang if they differ (matching real DMG
///    hardware behaviour that prevents non-licensed cartridges from booting).
/// 8. Compute the header checksum over bytes $0134–$014C and compare it with
///    the value stored at $014D; hang on mismatch.
/// 9. Set the documented DMG post-boot CPU register state:
///    A=$01, F=$B0, B=$00, C=$13, D=$00, E=$D8, H=$01, L=$4D, SP=$FFFE.
/// 10. Write $FF50 (BOOT register) to unmap the boot ROM; the CPU immediately
///     continues executing at $0100 in cartridge ROM.
///
/// ## Design notes
///
/// - APU is stubbed: NR50/NR51/NR52 are written with the correct post-boot
///   values but no actual audio is generated.
/// - The ® trademark symbol tile is omitted to keep the ROM within budget.
/// - LCD starts off at power-on (DmgBus initialises LCDC=0x00); the boot ROM
///   explicitly writes LCDC=0x91 to $FF40 just before the scroll animation
///   begins, matching the sequence real hardware follows.
/// - `WaitFrame` clobbers HL (safe because HL is not preserved after the tile
///   map is built).
///
/// ## Subroutine layout (placed backward from $00FE)
///
/// | Address | Routine                  | Size |
/// |---------|--------------------------|------|
/// | $00A6   | `DoubleBitsAndWriteRow`  | 21 B |
/// | $00BB   | `WaitFrame`              | 10 B |
/// | $00C5   | `WaitBFrames`            |  7 B |
/// | $00CC   | `Lockup`                 |  2 B |
/// | $00CE   | Nintendo logo reference  | 48 B |
/// | $00FE   | `BootGame` (LDH [$FF50]) |  2 B |
pub const DMG_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: Clear VRAM ($8000–$9FFF) ─────────────────────────────────────
    // LD HL, $8000; XOR A
    // .loop: LD [HL+],A; BIT 5,H; JR Z,.loop
    // Exit when H=$A0 (bit 5 of H set → HL has left VRAM range)
    0x21, 0x00, 0x80, 0xAF, 0x22, 0xCB, 0x6C, 0x28, 0xFB,
    // ── $000C: Init APU ──────────────────────────────────────────────────────
    // NR52=$80 (audio power on), NR11=$80 (50% duty), NR12=$F3 (envelope),
    // NR51=$F3 (routing), NR50=$77 (volume 7 both channels)
    0x3E, 0x80, 0xE0, 0x26, // LD A,$80; LDH [$FF26]  NR52
    0x3E, 0xF3, 0xE0, 0x12, // LD A,$F3; LDH [$FF12]  NR12
    0xE0, 0x25, // LDH [$FF25]             NR51 (same A)
    0x3E, 0x77, 0xE0, 0x24, // LD A,$77; LDH [$FF24]  NR50
    // ── $001E: Init BG palette ───────────────────────────────────────────────
    // BGP = $44 (%01_01_01_00) — medium grey for logo
    0x3E, 0x44, 0xE0, 0x47,
    // ── $0022: Load Nintendo logo tiles from cart → VRAM ────────────────────
    // Source: cartridge $0104–$0133 (48 bytes).
    // Destination: VRAM $8010 (tile slot 1).
    // Each source byte → two 8-pixel rows via DoubleBitsAndWriteRow.
    // Loop exits when E == LOW($0134) = $34 (i.e. DE has advanced to $0134).
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0x10, 0x80, // LD HL, $8010
    // .logoLoop:
    0x1A, 0x47, // LD A,[DE]; LD B,A
    0xCD, 0xA6, 0x00, // CALL DoubleBitsAndWriteRow  ($00A6)
    0xCD, 0xA6, 0x00, // CALL DoubleBitsAndWriteRow  ($00A6)
    0x13, // INC DE
    0x7B, 0xEE, 0x34, // LD A,E; XOR $34
    0x20, 0xF2, // JR NZ, .logoLoop
    // ── $0033: Build BG tile map ─────────────────────────────────────────────
    // Logo tiles $01–$18 (24 tiles, 2 rows × 12 columns) placed at
    // SCRN0 row 8 cols 4–15 and row 9 cols 4–15.
    // Fill backwards: A starts at $19=25, DEC before write, stop at A=0.
    // Bottom row: SCRN0+9*32+15 = $992F → $9924
    // Top row:    SCRN0+8*32+15 = $990F → $9904
    0x3E, 0x19, // LD A, $19
    0x21, 0x2F, 0x99, // LD HL, $992F
    0x0E, 0x0C, // LD C, 12
    // .tmapLoop:
    0x3D, // DEC A
    0x28, 0x08, // JR Z, .tmapDone (+8 → $0047)
    0x32, // LD [HL-], A
    0x0D, // DEC C
    0x20, 0xF9, // JR NZ, .tmapLoop
    0x2E, 0x0F, // LD L, $0F  (→ $990F = top-row right edge)
    0x18, 0xF5, // JR .tmapLoop
    // .tmapDone:
    // ── Enable LCD (LCDC=$91): LCD on, BG tile data $8000, BG map $9800 ─────
    // The boot ROM runs with LCD *off* (LCDC=$00 at hardware power-on) so that
    // VRAM writes during the logo tile load are never blocked by Mode 3.
    // Enable here — after all tiles and the tile map are in place — so the
    // PPU starts fresh at scanline 0 / Mode 2 just before the scroll animation.
    0x3E, 0x91, 0xE0, 0x40, // LD A,$91; LDH [$FF40] (LCDC on)
    // ── Set SCY=30 ────────────────────────────────────────────────────────────
    0x3E, 0x1E, 0xE0, 0x42, // LD A,30; LDH [$FF42] (SCY)
    // ── $004B: Scroll animation ──────────────────────────────────────────────
    // Scroll the logo down from SCY≈30 to SCY=0 over 15 VBlank frames.
    // D=$89 (=−119 signed, starting scroll value), C=15 (frame counter).
    // SCY = D >> 2 each frame; D += C each frame.
    // At C=8: darken palette to $AA (%10_10_10_00).
    0x16, 0x89, // LD D, $89  (=-119 unsigned)
    0x0E, 0x0F, // LD C, 15
    // .animLoop ($004F):
    0xCD, 0xBB, 0x00, // CALL WaitFrame ($00BB)
    0x7A, 0xCB, 0x2F, 0xCB, 0x2F, // LD A,D; SRA A; SRA A
    0xE0, 0x42, // LDH [$FF42] (SCY = D>>2)
    0x7A, 0x81, 0x57, // LD A,D; ADD C; LD D,A  (D += C)
    0x79, 0xFE, 0x08, // LD A,C; CP 8
    0x20, 0x04, // JR NZ, .noPaletteChange (+4)
    0x3E, 0xAA, 0xE0, 0x47, // LD A,$AA; LDH [$FF47] (BGP dark)
    // .noPaletteChange:
    0x0D, // DEC C
    0x20, 0xE7, // JR NZ, .animLoop (-25, → $004F)
    // ── $006A: Final palette + wait ──────────────────────────────────────────
    // BGP=$FC (%11_11_11_00) = all-dark.  Wait ~60 frames (~1 second).
    0x3E, 0xFC, 0xE0, 0x47, // LD A,$FC; LDH [$FF47] (BGP)
    0x06, 0x3C, // LD B, 60
    0xCD, 0xC5, 0x00, // CALL WaitBFrames ($00C5)
    // ── $0073: Verify Nintendo logo ──────────────────────────────────────────
    // Re-read cart $0104–$0133; compare byte-by-byte against the 48-byte
    // reference copy at $00CE.  Hang at Lockup ($00CC) on any mismatch.
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0xCE, 0x00, // LD HL, $00CE  (logo reference)
    0x0E, 0x30, // LD C, 48
    // .verifyLoop ($007C):
    0x1A, 0x13, // LD A,[DE]; INC DE
    0xBE, 0x23, // CP [HL]; INC HL
    0x20, 0x4C, // JR NZ, Lockup ($00CC)
    0x0D, // DEC C
    0x20, 0xF7, // JR NZ, .verifyLoop
    // ── $0084: Verify header checksum ────────────────────────────────────────
    // Compute: A=0; for bytes $0134–$014C: A -= byte; A -= 1.
    // Save in B; compare with stored checksum at $014D.
    // Hang at Lockup on mismatch.
    0x21, 0x34, 0x01, // LD HL, $0134
    0x0E, 0x19, // LD C, 25
    0xAF, // XOR A  (A=0)
    // .csumLoop ($008B):
    0x96, 0x3D, // SUB [HL]; DEC A
    0x23, 0x0D, // INC HL; DEC C
    0x20, 0xFA, // JR NZ, .csumLoop
    0x47, // LD B, A  (save computed checksum)
    // After csumLoop HL = $0134 + 25 = $014D → no need for a 3-byte absolute
    // load; LD A,[HL] reads the stored checksum byte directly and saves 2 bytes.
    0x7E, // LD A, [HL]  ($014D = stored header checksum)
    0xB8, // CP B
    0x20, 0x38, // JR NZ, Lockup ($00CC)
    // ── $0096: Set post-boot register state ──────────────────────────────────
    // A=$01, F=$B0 via PUSH HL; POP AF with HL=$01B0.
    // HL=$014D (address of header checksum byte).
    // BC=$0013, DE=$00D8 (per Pan Docs post-boot table).
    0x21, 0xB0, 0x01, // LD HL, $01B0
    0xE5, 0xF1, // PUSH HL; POP AF  → AF=$01B0
    0x21, 0x4D, 0x01, // LD HL, $014D
    0x01, 0x13, 0x00, // LD BC, $0013
    0x11, 0xD8, 0x00, // LD DE, $00D8
    // ── $00A3: JP BootGame ($00FE) ────────────────────────────────────────────
    // After all registers are set up, jump to BootGame to unmap the boot ROM.
    // This JP bridges the 4-byte gap between main code ($00A2) and subroutines
    // ($00A6).  Execution flows: register setup → JP $00FE → LDH [$FF50],A
    // → CPU fetches from cartridge at $0100.
    0xC3, 0xFE, 0x00, // JP $00FE  (BootGame)
    0x00, // NOP (1 padding byte)
    // ════════════════════════════════════════════════════════════════════════
    // ── $00A6: DoubleBitsAndWriteRow ─────────────────────────────────────────
    // Expand the top 4 bits of B into an 8-pixel tile row (each bit → 2 pixels).
    // Writes the result byte twice to VRAM at [HL] for 2× vertical scaling:
    //   row-N low-plane  [HL+]  then  skip high-plane  [HL+]
    //   row-N+1 same     [HL+]  then  skip high-plane [HL+]
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
    // ── $00BB: WaitFrame ─────────────────────────────────────────────────────
    // Clear VBlank flag in IF ($FF0F bit 0), then busy-wait until VBlank fires.
    // Clobbers HL (safe; HL is not preserved across this call at runtime).
    // ─────────────────────────────────────────────────────────────────────────
    0x21, 0x0F, 0xFF, // LD HL, $FF0F  (IF register)
    0xCB, 0x86, // RES 0, [HL]   (clear VBlank flag)
    // .wfWait:
    0xCB, 0x46, // BIT 0, [HL]   (test VBlank flag)
    0x28, 0xFC, // JR Z, .wfWait (spin until VBlank fires)
    0xC9, // RET
    // ════════════════════════════════════════════════════════════════════════
    // ── $00C5: WaitBFrames ───────────────────────────────────────────────────
    // Call WaitFrame B times.
    // ─────────────────────────────────────────────────────────────────────────
    // .wbfLoop:
    0xCD, 0xBB, 0x00, // CALL WaitFrame ($00BB)
    0x05, // DEC B
    0x20, 0xFA, // JR NZ, .wbfLoop
    0xC9, // RET
    // ════════════════════════════════════════════════════════════════════════
    // ── $00CC: Lockup ────────────────────────────────────────────────────────
    // Infinite loop — reached on logo or header checksum mismatch.
    // Matches real DMG hardware: the console hangs and never boots the game.
    // ─────────────────────────────────────────────────────────────────────────
    0x18, 0xFE, // JR $-2  (jump to self forever)
    // ════════════════════════════════════════════════════════════════════════
    // ── $00CE: Nintendo logo reference data (48 bytes) ───────────────────────
    // Canonical bitmap per Pan Docs §0104–0133.
    // ─────────────────────────────────────────────────────────────────────────
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
    // ════════════════════════════════════════════════════════════════════════
    // ── $00FE: BootGame ──────────────────────────────────────────────────────
    // Writing any value to $FF50 (BOOT register) unmaps the boot ROM.
    // The CPU's next fetch ($0100) goes to cartridge ROM.
    // This instruction must live at exactly $00FE (hardware constraint).
    // ─────────────────────────────────────────────────────────────────────────
    0xE0, 0x50, // LDH [$FF50], A  (unmap boot ROM → execute $0100)
];

/// Open-source DMG-0 (first production run) boot ROM replacement.
///
/// DMG-0 had a simpler boot ROM than later DMG revisions: no logo scroll
/// animation, no header verification. This replacement reproduces the exact
/// post-boot hardware state measured from real DMG-0 hardware.
///
/// ## Post-boot state produced
///
/// | Register | Value |    | I/O       | Value |
/// |----------|-------|----|-----------|-------|
/// | A        | $01   |    | DIV       | $18   |
/// | F        | $00   |    | LY        | $01   |
/// | B        | $FF   |    | STAT      | $83   |
/// | C        | $13   |    | BGP       | $FC   |
/// | D        | $00   |    | LCDC      | $91   |
/// | E        | $C1   |    |           |       |
/// | H        | $84   |    |           |       |
/// | L        | $03   |    |           |       |
/// | SP       | $FFFE |    |           |       |
///
/// ## Timing
///
/// Total M-cycles: **1496**.  With `div_counter` initial value 204:
///   `204 + 1496 × 4 = 6188`  →  DIV = $18 at boot exit.
///
/// The `boot_div-dmg0` test first reads DIV 53 M-cycles after `$0100`:
///   `6188 + 53 × 4 = 6400 = $1900`  →  read returns $19 ✓, phase = 0 (just after increment).
///
/// LCD is enabled (`LCDC=$91`) at M-cycle 1342, so the PPU runs for
/// 154 M-cycles = 616 T-cycles before the boot exits.
/// Line 1 mode 3 spans T-cycles 536–707 from LCD enable;
/// 616 is inside that window, giving `LY=$01 STAT=$83` at cartridge entry.
///
/// ## ROM layout
///
/// | Address | Content                              | M-cycles |
/// |---------|--------------------------------------|----------|
/// | $0000   | LD SP / APU init / BGP               | 26       |
/// | $0015   | Pre-LCD delay loop (HL counter = 187)| 1311     |
/// | $001D   | Enable LCD (LCDC = $91)              | 5        |
/// | $0021   | Post-LCD delay (B counter = 33)      | 133      |
/// | $0026   | 2 × NOP (fine-tune timing)           | 2        |
/// | $0028   | Set post-boot CPU registers          | 12       |
/// | $0034   | JP $00FE                             | 4        |
/// | $00FE   | LDH [$FF50], A (unmap boot ROM)      | 3        |
/// |         | **Total**                            | **1496** |
pub const DMG0_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: APU init ──────────────────────────────────────────────────────
    0x3E, 0x80, 0xE0, 0x26, // NR52 = $80  (enable APU, all channels off)
    0x3E, 0xF3, 0xE0, 0x12, // NR12 = $F3  (channel 1 envelope: initial vol 15, increase)
    0xE0, 0x25, // NR51 = $F3  (reuse A; all channels routed to both outputs)
    0x3E, 0x77, 0xE0, 0x24, // NR50 = $77  (master volume: both outputs at 7)
    // ── $0011: BGP = $FC ─────────────────────────────────────────────────────
    0x3E, 0xFC, 0xE0, 0x47,
    // ── $0015: Pre-LCD delay loop (187 iterations × 7 M-cycles + overhead) ──
    // LD HL, 187  ($00BB)
    0x21, 0xBB, 0x00,
    // Loop: DEC HL; LD A,H; OR L; JR NZ → $0018  (7 M taken / 6 M last)
    0x2B, 0x7C, 0xB5, 0x20, 0xFB,
    // ── $001D: Enable LCD ────────────────────────────────────────────────────
    // LD A, $91  (LCDC: LCD on, BG on, BG tile data $8000, BG map $9800)
    0x3E, 0x91, // LDH ($FF40), A  — write LCDC; PPU begins at M-cycle 1342
    0xE0, 0x40,
    // ── $0021: Post-LCD delay loop (33 iterations × 4 M-cycles + overhead) ──
    // LD B, 33  ($21)
    0x06, 0x21, // Loop: DEC B; JR NZ → $0023  (4 M taken / 3 M last)
    0x05, 0x20, 0xFD,
    // ── $0026: Fine-tune NOPs (2 × 1 M-cycle) ────────────────────────────────
    0x00, 0x00,
    // ── $0028: Set post-boot CPU registers ───────────────────────────────────
    0x01, 0x13, 0xFF, // LD BC, $FF13  →  B=$FF, C=$13
    0x11, 0xC1, 0x00, // LD DE, $00C1  →  D=$00, E=$C1
    0x21, 0x03, 0x84, // LD HL, $8403  →  H=$84, L=$03
    0x3E, 0x01, // LD A, $01
    0xB7, // OR A          →  F=$00 (Z=0, N=0, H=0, C=0)
    // ── $0034: Jump to boot exit at $00FE ─────────────────────────────────────
    0xC3, 0xFE, 0x00, // JP $00FE
    // ── $0037–$00FD: Unused (fill with $00) ──────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $0037–$003E
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // $003F–$0046
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
