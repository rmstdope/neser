; m007.2_defs.inc — AxROM (Mapper 7, Submapper 2) — AND-type Bus Conflicts
;
; NESdev specifies that submapper 2 bus conflicts produce the bitwise AND of
; the written value and the PRG-ROM byte at the write address.

.define MAPPER_NUM 7
.define SUBMAPPER_NUM 2

PRG_ROM_16K   = 8
CHR_ROM_8K    = 0
PRG_RAM_SHIFT = 7
CHR_RAM_SHIFT = 7

.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1
.define MAX_MIRRORING_MODES 2

.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

.macro select_prg_bank slot, bank
    lda #bank
    sta bank_table + bank
.endmacro

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
