; m015.0_defs.inc — K-1029 Multicart (Mapper 15, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_015
;
; Address and Data Latch ($8000-$FFFF, write):
;   A~1... .... .... ..SS    D~pMPP PPPP
;   SS = PRG banking mode (0-3)
;   PPPPPP = PRG A19..A14
;   M = Mirroring (0=V, 1=H)
;   p = PRG A13 (mode 2 only)
;
; Mode 1 (SS=01, write to $8001): UNROM-style
;   CPU $8000-$BFFF: switchable 16KB (PPPPPP)
;   CPU $C000-$FFFF: fixed (PRG A14..16 = 111)
;
; Features:
;   - PRG banking: 16KB switchable at $8000, fixed at $C000 (mode 1)
;   - CHR: 8KB CHR-RAM (no banking)
;   - Mirroring: data bit 6 (0=V, 1=H)
;   - No IRQ
;   - No PRG-RAM on actual hardware

.define MAPPER_NUM 15
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8         ; 128KB = 8 × 16KB
CHR_ROM_8K    = 0         ; CHR-RAM (no CHR-ROM)
PRG_RAM_SHIFT = 7         ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7         ; 8KB CHR-RAM (64 << 7 = 8KB)

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
.define PRG_BANK_SIZE 16      ; 16KB switchable (mode 1)
.define CHR_BANK_SIZE 8       ; N/A (CHR-RAM, no banking)
.define PRG_BANK_COUNT 4      ; Test 4 switchable banks (0-3) + fixed bank 7
.define CHR_BANK_COUNT 0

; Bank 1 is used for power-on bootstrap, skip it in PRG banking tests
TEST_BANK_B = 2
TEST_BANK_C = 3

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000 (mode 1: write to $8001)
; Data bits 5-0 = PRG bank, bit 6 = mirroring (keep V=0)
.macro select_prg_bank slot, bank
    lda #bank
    sta $8001               ; SS=01 → UNROM mode
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; Set mirroring via data bit 6 of mode 1 write
; 0=V, 1=H (only 2-state)
; Writes to $8001 (mode 1) to avoid changing PRG mode
; Sets PRG bank to 0 as side effect
.macro set_mirroring mode
    .if mode = 2
        ; Vertical: M=0
        lda #$00
        sta $8001
    .elseif mode = 3
        ; Horizontal: M=1
        lda #$40
        sta $8001
    .endif
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
