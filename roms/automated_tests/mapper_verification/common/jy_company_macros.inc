; jy_company_macros.inc — Shared J.Y. Company ASIC register macros
;
; This file contains register definitions and macros for the J.Y. Company ASIC.
; Used by mapper 35 (= mapper 209 + 8KB WRAM), and potentially mappers 90/209/211.
;
; Banking mode must be configured via $D000 before using these macros.
; Expected mode for test ROMs: $D000 = $1A
;   PP=10 (8K PRG), CC=11 (1K CHR), L=0 (last bank fixed),
;   R=0 (no ROM nametables), 6=0 (WRAM at $6000)

; ============================================================
; PRG banking (8K mode)
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000-$9FFF, slot 1 = $A000-$BFFF, slot 2 = $C000-$DFFF
; slot 3 = $E000-$FFFF (only switchable when $D000 bit 2 = 1)
; Address mask: $F803 (bit 11 must be clear)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $7F)
        sta $8000
    .elseif slot = 1
        lda #(bank & $7F)
        sta $8001
    .elseif slot = 2
        lda #(bank & $7F)
        sta $8002
    .endif
.endmacro

; ============================================================
; CHR banking (1K mode)
; ============================================================

; Select 1KB CHR bank
; slots 0-7 map to PPU $0000-$1FFF in 1K increments
; LSB: $9000+slot, MSB: $A000+slot
; Address mask: $F807
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #<(bank)
        sta $9000
        lda #>(bank)
        sta $A000
    .elseif slot = 1
        lda #<(bank)
        sta $9001
        lda #>(bank)
        sta $A001
    .elseif slot = 2
        lda #<(bank)
        sta $9002
        lda #>(bank)
        sta $A002
    .elseif slot = 3
        lda #<(bank)
        sta $9003
        lda #>(bank)
        sta $A003
    .elseif slot = 4
        lda #<(bank)
        sta $9004
        lda #>(bank)
        sta $A004
    .elseif slot = 5
        lda #<(bank)
        sta $9005
        lda #>(bank)
        sta $A005
    .elseif slot = 6
        lda #<(bank)
        sta $9006
        lda #>(bank)
        sta $A006
    .elseif slot = 7
        lda #<(bank)
        sta $9007
        lda #>(bank)
        sta $A007
    .endif
.endmacro

; ============================================================
; Mirroring
; ============================================================

; Set nametable mirroring
; 0 = vertical, 1 = horizontal, 2 = 1-screen A, 3 = 1-screen B
; Extended mirroring (bit 3) is not used.
.macro set_mirroring mode
    lda #(mode & $03)
    sta $D001
.endmacro

; ============================================================
; Font initialisation
; ============================================================

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
.macro init_chr_font
    lda #8
    sta $9000               ; Slot 0 LSB = 8
    lda #0
    sta $A000               ; Slot 0 MSB = 0
    lda #9
    sta $9001               ; Slot 1 LSB = 9
    lda #0
    sta $A001               ; Slot 1 MSB = 0
.endmacro

; ============================================================
; PRG-RAM control
; ============================================================

; PRG-RAM is always enabled when $D000 bit 7 = 0 (WRAM at $6000)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
