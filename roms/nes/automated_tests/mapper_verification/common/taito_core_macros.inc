; taito_core_macros.inc — Shared Taito TC0190/TC0690 register macros
;
; This file contains register definitions and macros shared by Taito mappers:
; - Mapper 33 (TC0190 — no IRQ, mirroring in $8000)
; - Mapper 48 (TC0690 — has IRQ, separate mirroring register)
;
; Each mapper-specific defs file includes this and adds its own extensions.

; ============================================================
; Register Addresses
; ============================================================

TAITO_PRG0     = $8000  ; PRG Reg 0 (8K @ $8000); mapper 33 also has mirroring bit 6
TAITO_PRG1     = $8001  ; PRG Reg 1 (8K @ $A000)
TAITO_CHR0_2K  = $8002  ; CHR Reg 0 (2K @ $0000); value = 2K page number
TAITO_CHR1_2K  = $8003  ; CHR Reg 1 (2K @ $0800); value = 2K page number
TAITO_CHR2_1K  = $A000  ; CHR Reg 2 (1K @ $1000)
TAITO_CHR3_1K  = $A001  ; CHR Reg 3 (1K @ $1400)
TAITO_CHR4_1K  = $A002  ; CHR Reg 4 (1K @ $1800)
TAITO_CHR5_1K  = $A003  ; CHR Reg 5 (1K @ $1C00)

; ============================================================
; PRG banking
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000-$9FFF, slot 1 = $A000-$BFFF
; slots 2,3 are fixed (second-to-last and last banks)
;
; NOTE: For mapper 33, writing to $8000 also sets mirroring bit 6.
; This macro writes with bit 6 = 0 (vertical mirroring).
; Use set_mirroring to change mirroring separately.
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $3F)
        sta TAITO_PRG0
    .elseif slot = 1
        lda #(bank & $3F)
        sta TAITO_PRG1
    .endif
.endmacro

; ============================================================
; CHR banking (mixed 2K + 1K layout)
; ============================================================

; Select CHR bank
; Slots 0-1 are 2K banks ($8002/$8003); bank value is in 1K units (divide by 2 for register)
; Slots 2-5 are 1K banks ($A000-$A003); bank value written directly
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #(bank / 2)
        sta TAITO_CHR0_2K
    .elseif slot = 1
        lda #(bank / 2)
        sta TAITO_CHR1_2K
    .elseif slot = 2
        lda #bank
        sta TAITO_CHR2_1K
    .elseif slot = 3
        lda #bank
        sta TAITO_CHR3_1K
    .elseif slot = 4
        lda #bank
        sta TAITO_CHR4_1K
    .elseif slot = 5
        lda #bank
        sta TAITO_CHR5_1K
    .endif
.endmacro

; ============================================================
; Font initialisation
; ============================================================

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
; For 2K registers: bank 8 in 1K units → 2K page 4, bank 9 → not needed (2K covers both)
; Slot 0 (2K @ $0000) = 1K banks 8+9 → write 2K page 4 to $8002
; Slot 1 (2K @ $0800) doesn't need font, leave as-is
.macro init_chr_font
    lda #4                  ; 2K page 4 = 1K banks 8+9
    sta TAITO_CHR0_2K
.endmacro
