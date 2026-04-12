; m003.1_defs.inc — CNROM (Mapper 3, Submapper 1) — No Bus Conflicts
;
; Same as m003.0 but without bus conflicts.

.define MAPPER_NUM 3
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 2
CHR_ROM_8K    = 4
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

.macro select_prg_bank slot, bank
.endmacro

; Select 8KB CHR bank — no bus conflicts
.macro select_chr_bank slot, bank
    lda #bank
    sta $8000
.endmacro

.macro set_mirroring mode
.endmacro
