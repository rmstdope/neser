; m064.0_defs.inc — RAMBO-1 (Mapper 64, Submapper 0)
;
; Tengen's MMC3 variant with extra features:
;   - PRG banking: 8KB switchable (3 variable R6/R7/RF + fixed last)
;   - CHR banking: 1KB switchable (R0-R5 + optional R8/R9 via K bit)
;   - CHR A12 inversion (bit 7 of bank select)
;   - Mirroring: H/V programmable
;   - No PRG-RAM
;   - Scanline IRQ (PPU A12 mode) and CPU cycle IRQ mode
;   - No bus conflicts
;
; Spec: https://nesdev-wiki.nes.science/wikipages/RAMBO_1.xhtml

.define MAPPER_NUM 64
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB = 16 × 1KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; H/V (set to 4 to enable V/H tests)

; IRQ behavior — scanline mode for shared test
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; R6, R7, RF, fixed
.define CHR_BANK_COUNT 8       ; R0-R5 (R8/R9 via K bit)

; ============================================================
; RAMBO-1 uses the same register addresses as MMC3
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
