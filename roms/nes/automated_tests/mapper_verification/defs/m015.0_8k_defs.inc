; m015.0_8k_defs.inc — K-1029 Multicart (Mapper 15, Submapper 0)
;
; 8KB mode test variant — used by test_mode0/mode2/mode3.
; Same mapper parameters as m015.0_defs.inc, but paired with
; m015.0_8k.cfg which splits PRG-ROM into 16 × 8KB pages for
; per-page bank verification.
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_015
;
; Address and Data Latch ($8000-$FFFF, write):
;   A~1... .... .... ..SS    D~pMPP PPPP
;   SS = PRG banking mode (0-3)
;   PPPPPP = PRG A19..A14
;   M = Mirroring (0=V, 1=H)
;   p = PRG A13 (mode 2 only)

.define MAPPER_NUM 15
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8         ; 128KB = 8 × 16KB = 16 × 8KB
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

; Bank sizes — mode 1 values for shell.s/bootstrap compatibility
.define PRG_BANK_SIZE 16
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 4
.define CHR_BANK_COUNT 0

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000 (mode 1: write to $8001)
; Used by shell.s bootstrap — mode tests use their own trampoline.
.macro select_prg_bank slot, bank
    lda #bank
    sta $8001               ; SS=01 → UNROM mode
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; Set mirroring via data bit 6 of mode 1 write
.macro set_mirroring mode
    .if mode = 2
        lda #$00
        sta $8001
    .elseif mode = 3
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
