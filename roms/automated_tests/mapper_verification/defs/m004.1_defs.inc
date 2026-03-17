; m004.1_defs.inc — MMC3/TxROM (Mapper 4, Submapper 1) — NEC IRQ
;
; Identical to m004.0 except:
;   - SUBMAPPER_NUM = 1
;   - IRQ_MODE = 1 (NEC: fires on 1→0 transition only)
;
; The NEC variant fires IRQ one scanline later than Sharp on the first
; IRQ after a reload, because it requires the actual 1→0 transition
; rather than just "counter == 0".

.define MAPPER_NUM 4
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 4
CHR_ROM_8K    = 2
PRG_RAM_SHIFT = 7
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
.define IRQ_MODE 1             ; 1 = NEC (alternate, fires on 1→0 transition)

; Bank sizes
.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 1
.define PRG_BANK_COUNT 4
.define CHR_BANK_COUNT 8

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
