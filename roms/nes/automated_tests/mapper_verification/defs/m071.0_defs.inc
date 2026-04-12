; m071.0_defs.inc — Camerica (Mapper 71, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_071
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last bank fixed at $C000-$FFFF
;   - No CHR banking (8KB CHR-RAM)
;   - Fixed mirroring (solder pads)
;   - No PRG-RAM
;   - No IRQ
;   - No bus conflicts
;
; Bank select ($C000-$FFFF):
;   7  bit  0
;   ---- ----
;   xxxx PPPP
;        ||||
;        ++++- Select 16 KB PRG ROM bank for CPU $8000-$BFFF

.define MAPPER_NUM 71
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
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
.define PRG_BANK_SIZE 16       ; 16KB switchable
.define CHR_BANK_SIZE 8        ; 8KB CHR-RAM (fixed)
.define PRG_BANK_COUNT 2       ; slot 0 = switchable, slot 1 = fixed last
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

CAMERICA_BANK_SELECT = $C000

; Select 16KB PRG bank at $8000-$BFFF
.macro select_prg_bank slot, bank
    lda #bank
    sta CAMERICA_BANK_SELECT
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro
