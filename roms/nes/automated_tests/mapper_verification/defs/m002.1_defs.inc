; m002.1_defs.inc — UxROM (Mapper 2, Submapper 1) — No Bus Conflicts
;
; NESdev spec: https://www.nesdev.org/wiki/UxROM
;              https://www.nesdev.org/wiki/NES_2.0_submappers (002: 0, 1, 2)
;
; Submapper 1: bus conflicts do not occur. The written value selects the
; bank unmasked, whatever PRG-ROM byte sits at the written address. The
; bus_conflicts aspect asserts this (writes whose value differs from the
; ROM byte take effect unmasked).

.define MAPPER_NUM 2
.define SUBMAPPER_NUM 1

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
