; m002.0_defs.inc — UxROM (Mapper 2, Submapper 0) — Bus Conflicts
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last bank fixed at $C000-$FFFF
;   - No CHR banking (8KB CHR-RAM)
;   - Fixed mirroring (solder pads)
;   - No PRG-RAM
;   - No IRQ
;   - Bus conflicts: effective value = written AND ROM value at write address

.define MAPPER_NUM 2
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol        ; No PRG-RAM
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 16       ; 16KB switchable
.define CHR_BANK_SIZE 8        ; 8KB CHR-RAM (fixed)
.define PRG_BANK_COUNT 2        ; slot 0 = switchable, slot 1 = fixed last
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000-$BFFF
; Write to $8000-$FFFF selects bank. Bus conflicts apply.
; For safe writes, use addresses where ROM byte matches desired bank.
.macro select_prg_bank slot, bank
    lda #bank
    sta bank_table + bank   ; Write to ROM address that contains 'bank' value
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro
