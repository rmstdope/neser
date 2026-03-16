; m007.0_defs.inc — AxROM (Mapper 7, Submapper 0) — Bus Conflicts
;
; Features:
;   - PRG banking: 32KB switchable
;   - No CHR banking (8KB CHR-RAM)
;   - Nametable: single-screen A or B (bit 4)
;   - No PRG-RAM
;   - No IRQ
;   - Bus conflicts (AMROM/AOROM)

.define MAPPER_NUM 7
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB switchable
.define CHR_BANK_SIZE 8        ; 8KB CHR-RAM
.define PRG_BANK_COUNT 1        ; 1 × 32KB slot
.define CHR_BANK_COUNT 1

; AxROM register:
;   Write to $8000-$FFFF
;   Bits 0-2: 32KB PRG bank number
;   Bit 4: Nametable select (0=A, 1=B)

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank. Bus conflicts apply.
; The bank_table must contain bytes matching the bank+nametable value.
.macro select_prg_bank slot, bank
    lda #bank
    sta bank_table + bank   ; Bus-conflict-safe address
.endmacro

; Set nametable: 0=screen A, 1=screen B
; Must also re-specify current PRG bank (bit 4 and bits 0-2 share a register)
; Bus conflict note: writes target $8000 which is in the $FF fill area
; ($8000-$BFFF is unused fill in bank 0; CODE starts at $C000)
; So effective = written AND $FF = written. Safe as long as fillval=$FF.
.macro set_mirroring mode
    .if mode = 0
        ; Single-screen A
        lda #%00000000      ; Bit 4 = 0
        sta $8000
    .else
        ; Single-screen B
        lda #%00010000      ; Bit 4 = 1
        sta $8000
    .endif
.endmacro

.macro select_chr_bank slot, bank
.endmacro
