; m004.0_defs.inc — MMC3/TxROM (Mapper 4, Submapper 0) — Sharp IRQ
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - CHR banking: 1KB/2KB switchable (2×2KB + 4×1KB or inverted)
;   - Mirroring: H/V programmable
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (Sharp: fires when counter reaches 0)

.define MAPPER_NUM 4
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 1
.define HAS_BUS_CONFLICTS 0

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp (standard)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
; Called during init and after CHR banking tests
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
