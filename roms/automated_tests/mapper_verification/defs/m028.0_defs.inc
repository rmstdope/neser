; m028.0_defs.inc — Action 53 / InfiniteNESLives (Mapper 28, Submapper 0)
;
; Two-level register system: write index to $5000, then data to $8000-$FFFF.
;
; Register indices:
;   $00 = user reg 0: CHR bank (bits 1-0)
;   $01 = user reg 1: inner PRG bank
;   $80 = supervisor reg 0: mode (PRG mode, mirror, mirror-enable)
;   $81 = supervisor reg 1: outer PRG bank
;
; Configuration for this test ROM:
;   Action 53 mode register layout in this emulator:
;     bits 1:0 = mirroring selector, bit 2 = slot select, bit 3 = 16KB/32KB,
;     bits 5:4 = game size
;   We keep slot-select=1, prgSize=1, gameSize=1 and only vary bits 1:0.
;
; Init: write $80 → $5000, write PRG-mode-2+mirror-H+bit4 ($1E) → $8000
;
; Features:
;   - PRG banking: 16KB switchable at $8000, last bank fixed at $C000
;   - CHR-RAM only (8KB, no banking tested)
;   - Mirroring: 4-way (H/V/1A/1B) via supervisor register
;   - No IRQ, No PRG-RAM
;   - Power-on maps the second 16KB bank at $C000, so test code lives there.

.define MAPPER_NUM 28
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM (no banking)
.define PRG_BANK_COUNT 4      ; Emit bank signatures up through bank 3 for the PRG test
.define CHR_BANK_COUNT 1
SKIP_PRG_SIG1 = 1
TEST_BANK_B = 2
TEST_BANK_C = 3

; ============================================================
; Action 53 register macros
; ============================================================

; Select 16KB PRG bank at $8000 (supervisor reg selects PRG mode 2).
; Use $BFFF so the write sees linker fill ($FF) instead of signature bytes.
.macro select_prg_bank slot, bank
    lda #$01
    sta $5000               ; Select user register 1 (inner PRG bank)
    lda #(bank & $0F)
    sta $BFFF               ; Write inner bank number
.endmacro

; No CHR banking (CHR-RAM only)
.macro select_chr_bank slot, bank
.endmacro

; Set mirroring via supervisor mode register ($80), preserving:
; slot-select=1, prgSize=1, gameSize=1 (base value $1C).
.macro set_mirroring mode
    lda #$80
    sta $5000               ; Select supervisor register 0 (mode)
    .if mode = 0
        lda #$1E            ; Vertical
    .elseif mode = 1
        lda #$1F            ; Horizontal
    .elseif mode = 2
        lda #$1C            ; 1-screen lower
    .elseif mode = 3
        lda #$1D            ; 1-screen upper
    .endif
    sta $BFFF
.endmacro

; Mapper initialisation: configure PRG mode 2, horizontal mirror (mode 3)
; Called from shell.s during reset
.macro init_action53
    lda #$81
    sta $5000               ; Select outer PRG bank register
    lda #0
    sta $BFFF               ; Outer bank = 0
    lda #$80
    sta $5000               ; Select supervisor mode register
    lda #$1F                ; Horizontal mirroring, slot-select=1, prgSize=1, gameSize=1
    sta $BFFF
.endmacro
