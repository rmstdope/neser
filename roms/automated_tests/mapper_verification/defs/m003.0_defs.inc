; m003.0_defs.inc — CNROM (Mapper 3, Submapper 0) — Bus Conflicts
;
; Features:
;   - No PRG banking (32KB fixed)
;   - CHR banking: 8KB switchable
;   - Fixed mirroring
;   - No PRG-RAM
;   - No IRQ
;   - Bus conflicts: effective value = written AND ROM value

.define MAPPER_NUM 3
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 2        ; 32KB
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8       ; 8KB CHR banks
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1       ; 1 selectable slot

; ============================================================
; Register macros
; ============================================================

.macro select_prg_bank slot, bank
.endmacro

; Select 8KB CHR bank. Bus conflicts apply.
.macro select_chr_bank slot, bank
    lda #bank
    sta bank_table + bank   ; Safe bus-conflict-aware address
.endmacro

.macro set_mirroring mode
.endmacro
