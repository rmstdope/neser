; m039.0_defs.inc — Study & Game 32-in-1 (Mapper 39, Submapper 0)
;
; BNROM clone. Write to $8000-$FFFF selects 32KB PRG bank.
;
; Features:
;   - PRG banking: 32KB switchable via write to $8000-$FFFF (8-bit register)
;   - CHR: 8KB RAM (not bankswitched)
;   - Mirroring: hardwired vertical
;   - Bus conflicts: none
;   - No native PRG-RAM (8KB allocated via NES 2.0 header for test protocol)
;   - No IRQ

.define MAPPER_NUM 39
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB (= 4 × 32KB banks)
CHR_ROM_8K    = 0        ; CHR-RAM
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB switchable
.define CHR_BANK_SIZE 8        ; 8KB CHR-RAM
.define PRG_BANK_COUNT 1        ; 1 × 32KB slot
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank. No bus conflicts.
.macro select_prg_bank slot, bank
    lda #bank
    sta $8000
.endmacro

.macro select_chr_bank slot, bank
.endmacro

.macro set_mirroring mode
.endmacro
