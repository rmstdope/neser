; m158.0_defs.inc — Tengen 800037 / RAMBO-1 with CHR-bit7 CIRAM A10 (Mapper 158, Submapper 0)
;
; Mapper 158 is RAMBO-1 (mapper 64) with one modification:
;   - CHR A17 output is connected to CIRAM A10 instead of the $A000 mirroring register
;   - Bit 7 of CHR bank data ($8001) selects the CIRAM page (0 or 1)
;   - $A000 mirroring register is ignored
;   - All other RAMBO-1 behavior (PRG banking, CHR banking, IRQ) is unchanged
;
; The only known game: Alien Syndrome (Tengen, board 800037)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_158.xhtml
;       https://nesdev-wiki.nes.science/wikipages/RAMBO_1.xhtml

.define MAPPER_NUM 158
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB = 16 × 1KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0   ; No $A000 mirroring register (CHR-bit7 controls CIRAM)
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0     ; No H/V mirroring control via register

; IRQ behavior — scanline mode (same as mapper 64)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; R6, R7, RF, fixed-last
.define CHR_BANK_COUNT 8       ; R0-R5 (R8/R9 via K bit)

; ============================================================
; RAMBO-1 uses the same register addresses as MMC3
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF (C=0, K=0 mode)
    sta MMC3_BANK_SELECT
    lda #8                  ; Font in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
