/// Open-source DMG boot ROM replacement.
///
/// Mimics the visual and audio behavior of a real DMG boot sequence:
/// scrolling the Nintendo logo down from off-screen, playing the two-note
/// "ba-ding!" sound near the end of the scroll, then holding the logo
/// on screen before handing off to the cartridge.
///
/// Key behavioral properties matching real hardware:
///   - SCY starts at 100 ($64), scrolls 1 pixel per 2-VBlank iteration
///   - Sound triggers near end of scroll: NR13=$83 at iter 98, NR13=$C1 at iter 100
///   - 32-iteration hold phase (~64 frames) after scroll completes
///   - NR11=$80 (50% duty cycle) for authentic sound character
///   - VBlank detection via LY polling (LDH A,[$FF44]; CP 144)
///   - Logo is accepted without verification (custom ROM design choice)
///
/// ## ROM layout
///
/// | Address   | Content                                    | Size  |
/// |-----------|--------------------------------------------|-------|
/// | $0000     | SP init + VRAM clear                       | 12 B  |
/// | $000C     | APU init (no trigger) + NR11 duty          | 16 B  |
/// | $001C     | BGP = $FC                                  | 4 B   |
/// | $0020     | Logo tile load (CALL $00A7)                | 20 B  |
/// | $0034     | Tile map setup                             | 18 B  |
/// | $0046     | Pre-LCD delay (N=2148, 2 NOP)              | 10 B  |
/// | $0050     | LCD enable (LCDC=$91)                      | 4 B   |
/// | $0054     | SCY = $64 (100)                            | 4 B   |
/// | $0058     | Scroll loop setup (D=100, B=1, H=0)        | 4 B   |
/// | $005C     | Scroll loop (LY poll + sound + SCY update) | 56 B  |
/// | $0094     | Padding                                    | 19 B  |
/// | $00A7     | `DoubleBitsAndWriteRow`                    | 21 B  |
/// | $00BC     | Post-loop: IF=$E1, DIV fine-tune, regs     | 34 B  |
/// | $00DE     | Padding                                    | 32 B  |
/// | $00FE     | `BootGame` (LDH [$FF50])                   | 2 B   |
pub const DMG_BOOT_ROM: [u8; 256] = [
    // ── $0000: LD SP, $FFFE ──────────────────────────────────────────────────
    0x31, 0xFE, 0xFF,
    // ── $0003: Clear VRAM ($8000–$9FFF) ─────────────────────────────────────
    0x21, 0x00, 0x80, 0xAF, 0x22, 0xCB, 0x6C, 0x28, 0xFB,
    // ── $000C: APU init — envelope/panning/volume only, NO trigger ───────────
    0x3E, 0x80, 0xE0, 0x26, // LD A,$80; LDH [$FF26]  NR52 (APU on)
    0xE0, 0x11, // LDH [$FF11]             NR11=$80 (50% duty)
    0x3E, 0xF3, 0xE0, 0x12, // LD A,$F3; LDH [$FF12]  NR12 (envelope)
    0xE0, 0x25, // LDH [$FF25]             NR51=$F3 (panning)
    0x3E, 0x77, 0xE0, 0x24, // LD A,$77; LDH [$FF24]  NR50 (volume)
    // ── $001C: Init BG palette ───────────────────────────────────────────────
    0x3E, 0xFC, 0xE0, 0x47, // LD A,$FC; LDH [$FF47]  BGP
    // ── $0020: Load logo tiles from cartridge header → VRAM ──────────────────
    0x11, 0x04, 0x01, // LD DE, $0104
    0x21, 0x10, 0x80, // LD HL, $8010
    // .logoLoop ($0026):
    0x1A, 0x47, // LD A,[DE]; LD B,A
    0xCD, 0xA7, 0x00, // CALL DoubleBitsAndWriteRow ($00A7)
    0xCD, 0xA7, 0x00, // CALL DoubleBitsAndWriteRow ($00A7)
    0x13, // INC DE
    0x7B, 0xEE, 0x34, // LD A,E; XOR $34
    0x20, 0xF2, // JR NZ, .logoLoop (→ $0026)
    // ── $0034: Build BG tile map ─────────────────────────────────────────────
    0x3E, 0x19, // LD A, $19
    0x21, 0x2F, 0x99, // LD HL, $992F
    0x0E, 0x0C, // LD C, 12
    // .tmapLoop ($003B):
    0x3D, // DEC A
    0x28, 0x08, // JR Z, .tmapDone (+8 → $0046)
    0x32, // LD [HL-], A
    0x0D, // DEC C
    0x20, 0xF9, // JR NZ, .tmapLoop (→ $003B)
    0x2E, 0x0F, // LD L, $0F
    0x18, 0xF5, // JR .tmapLoop (→ $003B)
    // .tmapDone ($0046):
    // ── $0046: Pre-LCD delay (tuned for DIV=$AB alignment) ───────────────────
    0x21, 0x64, 0x08, // LD HL, $0864  (2148)
    0x00, 0x00, // 2 × NOP
    // .preLoop ($004B):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $0050: Enable LCD ────────────────────────────────────────────────────
    0x3E, 0x91, 0xE0, 0x40, // LD A,$91; LDH [$FF40]
    // ── $0054: SCY = $64 (100 pixels) ────────────────────────────────────────
    0x3E, 0x64, 0xE0, 0x42, // LD A,$64; LDH [$FF42]
    // ── $0058: Scroll loop register setup ────────────────────────────────────
    0x57, // LD D, A       D = 100 (iteration counter)
    0x04, // INC B         B = 1 (scroll active flag)
    0x26, 0x00, // LD H, 0      H = 0 (frame counter for sound)
    // ═══════════════════════════════════════════════════════════════════════
    // ── $005C: Main scroll/hold loop (LY-polling, VBlank-synced) ─────────
    // ═══════════════════════════════════════════════════════════════════════
    // .loop ($005C):
    0x1E, 0x02, // LD E, 2      E = 2 VBlanks per iteration
    // .waitNotVblank ($005E):  — wait until LY < 144
    0xF0, 0x44, // LDH A, [$FF44]
    0xFE, 0x90, // CP 144
    0x30, 0xFA, // JR NC, .waitNotVblank (→ $005E)
    // .waitVblank ($0064):  — wait until LY ≥ 144
    0xF0, 0x44, // LDH A, [$FF44]
    0xFE, 0x90, // CP 144
    0x38, 0xFA, // JR C, .waitVblank (→ $0064)
    // VBlank entered:
    0x1D, // DEC E
    0x20, 0xF1, // JR NZ, .waitNotVblank (→ $005E)
    // ── $006D: Sound trigger check ───────────────────────────────────────────
    0x24, // INC H         frame counter++
    0x7C, // LD A, H
    0x0E, 0x13, // LD C, $13     C = LOW(rNR13)
    0x1E, 0x83, // LD E, $83     first note frequency
    0xFE, 0x62, // CP $62        iteration 98?
    0x28, 0x06, // JR Z, .playSound (→ $007D)
    0x1E, 0xC1, // LD E, $C1     second note frequency
    0xFE, 0x64, // CP $64        iteration 100?
    0x20, 0x08, // JR NZ, .noSound (→ $0085)
    // .playSound ($007D):
    0x7B, // LD A, E
    0xE2, // LD ($FF00+C), A  → NR13
    0x0C, // INC C            → C = $14
    0x3E, 0x87, // LD A, $87
    0xE2, // LD ($FF00+C), A  → NR14 (trigger + freq high=$07)
    0x18, 0x00, // JR .noSound (→ $0085)
    // .noSound ($0085):
    0xF0, 0x42, // LDH A, [$FF42]   read SCY
    0x90, // SUB B             SCY -= B (1 if scroll, 0 if hold)
    0xE0, 0x42, // LDH [$FF42], A   write SCY
    // ── $008A: Loop control ──────────────────────────────────────────────────
    0x15, // DEC D
    0x20, 0xCF, // JR NZ, .loop (→ $005C)
    0x05, // DEC B        B: 1→0 (enter hold) or 0→$FF (exit)
    0x20, 0x2C, // JR NZ, .done (→ $00BC)
    0x16, 0x20, // LD D, 32     hold phase: 32 iterations
    0x18, 0xC8, // JR .loop (→ $005C)
    // ── $0094–$00A6: Padding ─────────────────────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00,
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
    0x20, 0xF5, // JR NZ, .dblLoop (→ $00AB)
    0x79, // LD A, C
    0x22, 0x23, // LD [HL+],A; INC HL
    0x22, 0x23, // LD [HL+],A; INC HL
    0xC9, // RET
    // ════════════════════════════════════════════════════════════════════════
    // ── $00BC: .done — post-loop continuation ────────────────────────────────
    0x3E, 0xE1, 0xE0, 0x0F, // LD A,$E1; LDH [$FF0F]   IF = $E1
    // ── $00C0: Fine-tune delay (calibrated for DIV=$AB + LY=$0A at exit) ────
    0x21, 0xB4, 0xEB, // LD HL, $EBB4 (N=60340)
    0x00, 0x00, 0x00, // 3 × NOP
    // .fineLoop ($00C6):
    0x2B, 0x7C, 0xB5, 0x20, 0xFB, // DEC HL; LD A,H; OR L; JR NZ
    // ── $00CB: Set post-boot register state ──────────────────────────────────
    0x21, 0xB0, 0x01, // LD HL, $01B0
    0xE5, 0xF1, // PUSH HL; POP AF  → AF=$01B0
    0x21, 0x4D, 0x01, // LD HL, $014D
    0x01, 0x13, 0x00, // LD BC, $0013
    0x11, 0xD8, 0x00, // LD DE, $00D8
    // ── $00D9: JP BootGame ($00FE) ───────────────────────────────────────────
    0xC3, 0xFE, 0x00,
    // ── $00DC–$00FD: Padding ─────────────────────────────────────────────────
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
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

/// Minimal CGB boot ROM (Production variant: CGB-A through CGB-E).
///
/// Sets up the post-boot hardware state for CGB-native mode without a boot
/// animation or intentional startup jingle. To match required post-boot APU
/// state, it may still perform minimal audio register initialization before
/// handing off to the cartridge at $0100.
///
/// **Production CGB vs CGB-0 difference**: This boot ROM initializes wave RAM
/// ($FF30-$FF3F) to an alternating 0x00/0xFF pattern. CGB-0 does NOT initialize
/// wave RAM (see `CGB0_BOOT_ROM`).
///
/// ## Post-boot CPU state (Mooneye-verified)
///
/// | Register | Value |
/// |----------|-------|
/// | A        | $11   |
/// | F        | $80 (Z=1, N=0, H=0, C=0) |
/// | B        | $00   |
/// | C        | $00   |
/// | D        | $00   |
/// | E        | $08   |
/// | H        | $00   |
/// | L        | $7C   |
/// | SP       | $FFFE |
/// | PC       | $0100 |
///
/// Note: Pan Docs lists different values (D=$FF, E=$56, L=$0D) but Mooneye's
/// `misc/boot_regs-cgb.s` test verifies the above values against real hardware.
///
/// ## Post-boot IO state
///
/// | Address | Register | Value |
/// |---------|----------|-------|
/// | $FF26   | NR52     | $F1 (APU on, channel 1 active) |
/// | $FF24   | NR50     | $77   |
/// | $FF25   | NR51     | $F3   |
/// | $FF40   | LCDC     | $91   |
/// | $FF47   | BGP      | $FC   |
///
/// ## Memory layout
///
/// The CGB boot ROM is 2048 bytes, split into two mapped regions:
/// - $0000-$00FF: Early initialization (256 bytes) → array indices [0..256]
/// - $0100-$01FF: Cartridge header (NOT in boot ROM; reads go to cartridge)
/// - $0200-$08FF: Main boot ROM code (1792 bytes) → array indices [256..2048]
///
/// This minimal implementation uses only the first 256 bytes.
pub const CGB_BOOT_ROM: [u8; 2048] = {
    let mut rom = [0u8; 2048];

    // ═══════════════════════════════════════════════════════════════════════
    // $0000-$00FF: First mapped region (256 bytes)
    // ═══════════════════════════════════════════════════════════════════════

    // ── $0000: Initialize stack pointer ────────────────────────────────────
    rom[0x00] = 0x31; // LD SP, $FFFE
    rom[0x01] = 0xFE;
    rom[0x02] = 0xFF;

    // ── $0003: APU initialization ──────────────────────────────────────────
    // Turn on APU
    rom[0x03] = 0x3E; // LD A, $80
    rom[0x04] = 0x80;
    rom[0x05] = 0xE0; // LDH [$26], A  ; NR52 = $80 (APU on)
    rom[0x06] = 0x26;

    // Set channel 1 duty (50%) - writes $80, reads back as $BF due to mask
    rom[0x07] = 0xE0; // LDH [$11], A  ; NR11 = $80 (reuse A=$80)
    rom[0x08] = 0x11;

    // Set channel 1 envelope
    rom[0x09] = 0x3E; // LD A, $F3
    rom[0x0A] = 0xF3;
    rom[0x0B] = 0xE0; // LDH [$12], A  ; NR12 = $F3
    rom[0x0C] = 0x12;

    // Set sound panning (reuse A=$F3)
    rom[0x0D] = 0xE0; // LDH [$25], A  ; NR51 = $F3
    rom[0x0E] = 0x25;

    // Set master volume
    rom[0x0F] = 0x3E; // LD A, $77
    rom[0x10] = 0x77;
    rom[0x11] = 0xE0; // LDH [$24], A  ; NR50 = $77
    rom[0x12] = 0x24;

    // Trigger channel 1 (makes NR52 read as $F1)
    rom[0x13] = 0x3E; // LD A, $87
    rom[0x14] = 0x87;
    rom[0x15] = 0xE0; // LDH [$14], A  ; NR14 = $87 (trigger, length=0, period=7)
    rom[0x16] = 0x14;

    // ── $0017: PPU initialization ──────────────────────────────────────────
    rom[0x17] = 0x3E; // LD A, $91
    rom[0x18] = 0x91;
    rom[0x19] = 0xE0; // LDH [$40], A  ; LCDC = $91
    rom[0x1A] = 0x40;

    rom[0x1B] = 0x3E; // LD A, $FC
    rom[0x1C] = 0xFC;
    rom[0x1D] = 0xE0; // LDH [$47], A  ; BGP = $FC
    rom[0x1E] = 0x47;

    // ── $001F: Wave RAM initialization (Production CGB only) ───────────────
    // Initialize $FF30-$FF3F to alternating 0x00/0xFF pattern.
    // SameBoy's CGB boot ROM does: ld a, 0; loop: ldi [hl], a; cpl; dec c; jr nz, loop
    // We do the same with HL=$FF30, C=16
    rom[0x1F] = 0x21; // LD HL, $FF30
    rom[0x20] = 0x30;
    rom[0x21] = 0xFF;
    rom[0x22] = 0x0E; // LD C, $10 (16 bytes)
    rom[0x23] = 0x10;
    rom[0x24] = 0xAF; // XOR A (A = 0)
    // Loop: write A to [HL++], complement A, decrement C, loop if not zero
    rom[0x25] = 0x22; // LDI [HL], A  (write A to [HL], HL++)
    rom[0x26] = 0x2F; // CPL         (A = ~A, toggles 0x00 <-> 0xFF)
    rom[0x27] = 0x0D; // DEC C
    rom[0x28] = 0x20; // JR NZ, -5   (back to LDI)
    rom[0x29] = 0xFB; // -5 in two's complement

    // ── $002A: Set CPU registers ───────────────────────────────────────────
    // Set AF = $1180 (A=$11, F=$80) using HL trick:
    // PUSH HL pushes H first, then L. POP AF pops to F first, then A.
    // So for A=$11, F=$80: set H=$11, L=$80, PUSH HL, POP AF.
    rom[0x2A] = 0x26; // LD H, $11
    rom[0x2B] = 0x11;
    rom[0x2C] = 0x2E; // LD L, $80
    rom[0x2D] = 0x80;
    rom[0x2E] = 0xE5; // PUSH HL
    rom[0x2F] = 0xF1; // POP AF  ; Now A=$11, F=$80

    // Set B, C, D, E
    rom[0x30] = 0x06; // LD B, $00
    rom[0x31] = 0x00;
    rom[0x32] = 0x0E; // LD C, $00
    rom[0x33] = 0x00;
    rom[0x34] = 0x16; // LD D, $00
    rom[0x35] = 0x00;
    rom[0x36] = 0x1E; // LD E, $08
    rom[0x37] = 0x08;

    // Set H, L to final values
    rom[0x38] = 0x26; // LD H, $00
    rom[0x39] = 0x00;
    rom[0x3A] = 0x2E; // LD L, $7C
    rom[0x3B] = 0x7C;

    // ── $003C: Jump to boot exit ───────────────────────────────────────────
    rom[0x3C] = 0xC3; // JP $00FE
    rom[0x3D] = 0xFE;
    rom[0x3E] = 0x00;

    // ── $003F-$00FD: Padding (zeros) ───────────────────────────────────────
    // (already zero-initialized)

    // ── $00FE: Boot exit ───────────────────────────────────────────────────
    // Write to $FF50 to unmap boot ROM. A=$11 from earlier.
    // After this instruction completes, PC=$0100 = cartridge entry point.
    rom[0xFE] = 0xE0; // LDH [$50], A
    rom[0xFF] = 0x50;

    // ═══════════════════════════════════════════════════════════════════════
    // $0200-$08FF: Second mapped region (1792 bytes) - unused in minimal ROM
    // ═══════════════════════════════════════════════════════════════════════
    // (already zero-initialized)

    rom
};

/// Minimal CGB boot ROM for CGB-0 (first hardware revision).
///
/// CGB-0 is a rare early CGB revision with a slightly different boot ROM.
/// The key difference: CGB-0 does NOT initialize wave RAM ($FF30-$FF3F),
/// leaving it unchanged in its power-on state rather than guaranteeing any
/// specific initial value. This matches Pan Docs and SameBoy's handling.
///
/// All other aspects are identical to the Production CGB boot ROM:
/// - Same post-boot CPU register values (A=$11, F=$80, B=$00, C=$00, D=$00, E=$08, H=$00, L=$7C)
/// - Same IO register values (NR52=$F1, NR50=$77, NR51=$F3, LCDC=$91, BGP=$FC)
/// - Same APU initialization (channel 1 triggered)
///
/// ## Post-boot CPU state (same as Production CGB)
///
/// | Register | Value |
/// |----------|-------|
/// | A        | $11   |
/// | F        | $80 (Z=1, N=0, H=0, C=0) |
/// | B        | $00   |
/// | C        | $00   |
/// | D        | $00   |
/// | E        | $08   |
/// | H        | $00   |
/// | L        | $7C   |
/// | SP       | $FFFE |
/// | PC       | $0100 |
///
/// Reference: SameBoy's CGB boot ROM uses identical register values for both
/// CGB-0 and Production CGB. The only difference is wave RAM initialization.
pub const CGB0_BOOT_ROM: [u8; 2048] = {
    let mut rom = [0u8; 2048];

    // ═══════════════════════════════════════════════════════════════════════
    // $0000-$00FF: First mapped region (256 bytes)
    // ═══════════════════════════════════════════════════════════════════════

    // ── $0000: Initialize stack pointer ────────────────────────────────────
    rom[0x00] = 0x31; // LD SP, $FFFE
    rom[0x01] = 0xFE;
    rom[0x02] = 0xFF;

    // ── $0003: APU initialization ──────────────────────────────────────────
    // Turn on APU
    rom[0x03] = 0x3E; // LD A, $80
    rom[0x04] = 0x80;
    rom[0x05] = 0xE0; // LDH [$26], A  ; NR52 = $80 (APU on)
    rom[0x06] = 0x26;

    // Set channel 1 duty (50%) - writes $80, reads back as $BF due to mask
    rom[0x07] = 0xE0; // LDH [$11], A  ; NR11 = $80 (reuse A=$80)
    rom[0x08] = 0x11;

    // Set channel 1 envelope
    rom[0x09] = 0x3E; // LD A, $F3
    rom[0x0A] = 0xF3;
    rom[0x0B] = 0xE0; // LDH [$12], A  ; NR12 = $F3
    rom[0x0C] = 0x12;

    // Set sound panning (reuse A=$F3)
    rom[0x0D] = 0xE0; // LDH [$25], A  ; NR51 = $F3
    rom[0x0E] = 0x25;

    // Set master volume
    rom[0x0F] = 0x3E; // LD A, $77
    rom[0x10] = 0x77;
    rom[0x11] = 0xE0; // LDH [$24], A  ; NR50 = $77
    rom[0x12] = 0x24;

    // Trigger channel 1 (makes NR52 read as $F1)
    rom[0x13] = 0x3E; // LD A, $87
    rom[0x14] = 0x87;
    rom[0x15] = 0xE0; // LDH [$14], A  ; NR14 = $87 (trigger, length=0, period=7)
    rom[0x16] = 0x14;

    // ── $0017: PPU initialization ──────────────────────────────────────────
    rom[0x17] = 0x3E; // LD A, $91
    rom[0x18] = 0x91;
    rom[0x19] = 0xE0; // LDH [$40], A  ; LCDC = $91
    rom[0x1A] = 0x40;

    rom[0x1B] = 0x3E; // LD A, $FC
    rom[0x1C] = 0xFC;
    rom[0x1D] = 0xE0; // LDH [$47], A  ; BGP = $FC
    rom[0x1E] = 0x47;

    // ── $001F: NO wave RAM initialization (CGB-0 specific) ─────────────────
    // Unlike Production CGB, CGB-0 does NOT initialize wave RAM.
    // Wave RAM ($FF30-$FF3F) remains at power-on state (all zeros).

    // ── $001F: Set CPU registers ───────────────────────────────────────────
    // Set AF = $1180 (A=$11, F=$80) using HL trick:
    // PUSH HL pushes H first, then L. POP AF pops to F first, then A.
    // So for A=$11, F=$80: set H=$11, L=$80, PUSH HL, POP AF.
    rom[0x1F] = 0x26; // LD H, $11
    rom[0x20] = 0x11;
    rom[0x21] = 0x2E; // LD L, $80
    rom[0x22] = 0x80;
    rom[0x23] = 0xE5; // PUSH HL
    rom[0x24] = 0xF1; // POP AF  ; Now A=$11, F=$80

    // Set B, C, D, E
    rom[0x25] = 0x06; // LD B, $00
    rom[0x26] = 0x00;
    rom[0x27] = 0x0E; // LD C, $00
    rom[0x28] = 0x00;
    rom[0x29] = 0x16; // LD D, $00
    rom[0x2A] = 0x00;
    rom[0x2B] = 0x1E; // LD E, $08
    rom[0x2C] = 0x08;

    // Set H, L to final values
    rom[0x2D] = 0x26; // LD H, $00
    rom[0x2E] = 0x00;
    rom[0x2F] = 0x2E; // LD L, $7C
    rom[0x30] = 0x7C;

    // ── $0031: Jump to boot exit ───────────────────────────────────────────
    rom[0x31] = 0xC3; // JP $00FE
    rom[0x32] = 0xFE;
    rom[0x33] = 0x00;

    // ── $0034-$00FD: Padding (zeros) ───────────────────────────────────────
    // (already zero-initialized)

    // ── $00FE: Boot exit ───────────────────────────────────────────────────
    // Write to $FF50 to unmap boot ROM. A=$11 from earlier.
    // After this instruction completes, PC=$0100 = cartridge entry point.
    rom[0xFE] = 0xE0; // LDH [$50], A
    rom[0xFF] = 0x50;

    // ═══════════════════════════════════════════════════════════════════════
    // $0200-$08FF: Second mapped region (1792 bytes) - unused in minimal ROM
    // ═══════════════════════════════════════════════════════════════════════
    // (already zero-initialized)

    rom
};

/// Placeholder CGB boot ROM for infrastructure testing.
///
/// This is a minimal boot ROM that immediately disables itself and jumps to
/// the cartridge entry point at $0100. It is used during infrastructure
/// development and will be replaced by a full IPR-free CGB boot ROM implementation.
///
/// The CGB boot ROM is 2048 bytes, split into two mapped regions:
/// - $0000-$00FF: Early initialization (256 bytes)
/// - $0100-$01FF: Cartridge header (not part of the boot ROM; reads come from the cartridge)
/// - $0200-$08FF: Main boot ROM code (1792 bytes)
///
/// In this array, the boot-ROM-backed regions are stored contiguously:
/// indices 0..256 correspond to $0000-$00FF, and indices 256..2048
/// correspond to $0200-$08FF. The $0100-$01FF header gap exists in the
/// memory map, not as a gap inside this array.
///
/// This placeholder contains code to disable the boot ROM by writing to $FF50,
/// then jumping to $0100, followed by zero padding.
pub const CGB_PLACEHOLDER_BOOT_ROM: [u8; 2048] = {
    let mut rom = [0u8; 2048];
    // $0000: LD A, $01
    rom[0] = 0x3E; // LD A, n8
    rom[1] = 0x01; // value $01
    // $0002: LDH [$FF50], A  (unmap boot ROM)
    rom[2] = 0xE0; // LDH [n8], A
    rom[3] = 0x50; // $FF50
    // $0004: JP $0100  (jump to cartridge entry point)
    rom[4] = 0xC3; // JP opcode
    rom[5] = 0x00; // low byte of $0100
    rom[6] = 0x01; // high byte of $0100
    // Rest is padding (zeros)
    rom
};

#[cfg(test)]
mod tests {
    use super::DMG_BOOT_ROM;

    const CANONICAL_NINTENDO_LOGO_CRC32: u32 = 0x4619_5417;

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg() & 0xEDB8_8320;
                crc = (crc >> 1) ^ mask;
            }
        }
        !crc
    }

    #[test]
    fn dmg_boot_rom_sets_nr11_duty_cycle() {
        // Real hardware sets NR11=$80 (50% duty) for correct boot sound
        // character. The boot ROM must write $80 to $FF11 before the sound
        // is triggered. Look for LDH [$FF11],A right after writing NR52=$80
        // (which leaves A=$80), so we expect E0 11 at $0010.
        assert_eq!(DMG_BOOT_ROM[0x10], 0xE0, "NR11 write: LDH opcode expected");
        assert_eq!(
            DMG_BOOT_ROM[0x11], 0x11,
            "NR11 write: address byte $11 expected"
        );
    }

    #[test]
    fn dmg_boot_rom_does_not_trigger_sound_at_boot_start() {
        // Real hardware triggers the "ba-ding!" near the END of the scroll,
        // not at boot start. The APU init section ($000C–$001D) must set up
        // envelope/panning/volume but NOT write NR13/NR14 trigger.
        // Scan $000C–$001D for NR14 trigger byte ($87):
        let apu_init = &DMG_BOOT_ROM[0x0C..=0x1D];
        assert!(
            !apu_init.windows(2).any(|w| w == [0xE0, 0x14]),
            "NR14 trigger (LDH [$FF14]) must not appear in APU init section"
        );
        assert!(
            !apu_init.windows(2).any(|w| w == [0xE0, 0x13]),
            "NR13 freq (LDH [$FF13]) must not appear in APU init section"
        );
    }

    #[test]
    fn dmg_boot_rom_scroll_starts_at_100() {
        // Real hardware scrolls 100 pixels (SCY starts at $64).
        // Find LD A,$64; LDH [$FF42] pattern (3E 64 E0 42) somewhere
        // after the LCD enable section.
        let rom = &DMG_BOOT_ROM[0x50..0xA7];
        assert!(
            rom.windows(4).any(|w| w == [0x3E, 0x64, 0xE0, 0x42]),
            "SCY must be initialized to $64 (100) for real-hardware scroll distance"
        );
    }

    #[test]
    fn dmg_boot_rom_has_two_note_bading_sound() {
        // Real hardware plays two notes: NR13=$83 at scroll iter 98 and
        // NR13=$C1 at scroll iter 100. The ROM must contain both frequency
        // values as immediates within the scroll section ($005A–$00FD).
        let scroll_section = &DMG_BOOT_ROM[0x5A..0xFE];

        // First note frequency: $83
        assert!(
            scroll_section.contains(&0x83),
            "first 'ba' note frequency $83 must be in scroll section"
        );
        // Second note frequency: $C1
        assert!(
            scroll_section.contains(&0xC1),
            "second 'ding' note frequency $C1 must be in scroll section"
        );
        // Both iteration thresholds: $62 and $64
        assert!(
            scroll_section.contains(&0x62),
            "first sound trigger threshold $62 (iter 98) must be in scroll section"
        );
        assert!(
            scroll_section.contains(&0x64),
            "second sound trigger threshold $64 (iter 100) must be in scroll section"
        );
    }

    #[test]
    fn dmg_boot_rom_hold_phase_uses_32_iterations() {
        // Real hardware holds the logo for 32 iterations (× 2 VBlanks each
        // = ~64 frames ≈ 1 second) before handing off to the cartridge.
        // The hold counter value $20 (32) must appear as an immediate load
        // in the scroll section.
        let scroll_section = &DMG_BOOT_ROM[0x5A..0xFE];
        // Look for LD D,$20 pattern (16 20)
        assert!(
            scroll_section.windows(2).any(|w| w == [0x16, 0x20]),
            "hold phase must load D with $20 (32 iterations)"
        );
    }

    #[test]
    fn dmg_boot_rom_uses_ly_polling_for_vblank() {
        // Real hardware uses LDH A,[$FF44] (F0 44) to poll LY for VBlank
        // detection instead of fixed delay loops. The scroll section must
        // contain this pattern.
        let scroll_section = &DMG_BOOT_ROM[0x5A..0xFE];
        assert!(
            scroll_section.windows(2).any(|w| w == [0xF0, 0x44]),
            "scroll loop must poll LY via LDH A,[$FF44] for VBlank sync"
        );
    }

    #[test]
    fn dmg_boot_rom_does_not_embed_canonical_nintendo_logo_bytes() {
        assert!(
            !DMG_BOOT_ROM
                .windows(48)
                .any(|window| crc32(window) == CANONICAL_NINTENDO_LOGO_CRC32),
            "DMG boot ROM must not embed the canonical 48-byte Nintendo logo"
        );
    }
}
