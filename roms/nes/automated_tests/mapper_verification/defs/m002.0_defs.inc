; m002.0_defs.inc — UxROM (Mapper 2, Submapper 0) — Bus conflicts unspecified
;
; NESdev spec: https://www.nesdev.org/wiki/UxROM
;              https://www.nesdev.org/wiki/NES_2.0_submappers (002: 0, 1, 2)
;
; Submapper 0: default iNES behaviour, bus-conflict behaviour unknown. This
; ROM therefore asserts neither the presence nor the absence of bus
; conflicts: it only tests PRG banking, and every bank-select write stores a
; value equal to the PRG-ROM byte at the written address, so the result is
; the same with or without AND-type bus conflicts.
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last bank fixed at $C000-$FFFF
;   - No CHR banking (8KB CHR-RAM)
;   - Fixed mirroring (solder pads)
;   - No PRG-RAM
;   - No IRQ
;   - Bus conflicts: unspecified (not asserted; all writes are conflict-safe)

.define MAPPER_NUM 2
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
.define HAS_BUS_CONFLICTS 0     ; Not asserted; writes are conflict-safe without bank_table
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
; Write to $8000-$FFFF selects bank. Conflict-safe: the value is stored to
; the address of the LDA's own immediate operand byte, which holds 'bank'.
; The code runs from the fixed bank at $C000-$FFFF, so that byte is always
; mapped. (No shared bank_table is used: it is only linked into combined
; ROMs together with the bus_conflicts aspect, which submapper 0 omits.)
.macro select_prg_bank slot, bank
    .local imm
imm:
    lda #bank
    sta imm + 1             ; ROM byte at imm+1 == bank: safe either way
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro
