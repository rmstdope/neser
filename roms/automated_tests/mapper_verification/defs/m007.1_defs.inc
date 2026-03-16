; m007.1_defs.inc — AxROM (Mapper 7, Submapper 1) — No Bus Conflicts (ANROM)
;
; Same as m007.0 but without bus conflicts.

.define MAPPER_NUM 7
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 8
CHR_ROM_8K    = 0
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank — no bus conflicts
.macro select_prg_bank slot, bank
    lda #bank
    sta $8000
.endmacro

; Set nametable: 0=screen A, 1=screen B
.macro set_mirroring mode
    .if mode = 0
        lda #%00000000
        sta $8000
    .else
        lda #%00010000
        sta $8000
    .endif
.endmacro

.macro select_chr_bank slot, bank
.endmacro
