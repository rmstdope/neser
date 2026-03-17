; m002.2_defs.inc — UxROM (Mapper 2, Submapper 2) — No Bus Conflicts
;
; Same as m002.0 but without bus conflicts.
; Bank selection uses written value directly.

.define MAPPER_NUM 2
.define SUBMAPPER_NUM 2

PRG_ROM_16K   = 8
CHR_ROM_8K    = 0
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 16
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 2
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank — no bus conflicts, any address works
.macro select_prg_bank slot, bank
    lda #bank
    sta $8000
.endmacro

.macro select_chr_bank slot, bank
.endmacro

.macro set_mirroring mode
.endmacro
