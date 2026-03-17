; m031.0_defs.inc — NSF Subset Mapper (Mapper 31, Submapper 0)
;
; 8 × 4KB PRG banks mapped to $8000-$FFFF via registers $5FF8-$5FFF.
; Address bits 2-0 select the 4KB slot (0=$8000, 1=$9000, ..., 7=$F000).
; Data = 4KB bank number to map into that slot.
;
; Power-on: only slot 7 ($F000-$FFFF) is guaranteed to hold the last bank.
; This test ROM therefore keeps all startup code in slot 7 and only exercises slot 0.
;
; Features:
;   - PRG banking: 4KB switchable per slot ($5FF8-$5FFD for slots 0-5)
;   - CHR-RAM: 8KB (no banking)
;   - Fixed mirroring
;   - No IRQ, No PRG-RAM

.define MAPPER_NUM 31
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 16 × 4KB banks
CHR_ROM_8K    = 0        ; CHR-RAM only
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
.define MAX_MIRRORING_MODES 0       ; Fixed mirroring

; Bank sizes
.define PRG_BANK_SIZE 4       ; 4KB per slot
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM (no banking)
.define PRG_BANK_COUNT 8      ; 8 addressable 4KB slots
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros
; ============================================================

; Select 4KB PRG bank for a given slot
; Slot 0 = $8000-$8FFF (register $5FF8), ..., slot 7 = $F000-$FFFF (register $5FFF)
; This test only exercises slot 0 (BANK_WINDOW = $8000)
.macro select_prg_bank slot, bank
    lda #bank
    sta $5FF8 + slot
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro
