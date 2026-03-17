; m013.0_defs.inc — CPROM (Mapper 13, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/CPROM
;
; Features:
;   - No PRG banking (32KB fixed)
;   - CHR-RAM banking: 4KB switchable at $1000-$1FFF (bits 0-1)
;   - Fixed CHR-RAM at $0000-$0FFF (always page 0)
;   - Vertical mirroring (fixed)
;   - No PRG-RAM
;   - No IRQ
;   - Bus conflicts (74HC161 latch + 74HC08 AND gate)
;
; Register $8000-$FFFF:
;   7  bit  0
;   ---- ----
;   xxxx xxCC
;          ||
;          ++- Select 4 KiB CHR RAM bank for PPU $1000-$1FFF

.define MAPPER_NUM 13
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 2        ; 32KB = 2 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 8        ; 16KB CHR-RAM (64 << 8 = 16384)

; Capability flags
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 0        ; No CHR-ROM banking
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1
.define HAS_CHR_RAM_BANKING 1    ; CHR-RAM bank switching
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 4          ; 4KB switchable at $1000
.define PRG_BANK_COUNT 0
.define CHR_BANK_COUNT 1         ; 1 switchable slot

; ============================================================
; Register macros
; ============================================================

.macro select_prg_bank slot, bank
.endmacro

; Select 4KB CHR-RAM bank at $1000-$1FFF (bits 0-1)
; Bus conflicts apply — use bank_table for safe writes.
.macro select_chr_bank slot, bank
    lda #bank
    sta bank_table + bank   ; Bus-conflict-safe address
.endmacro

.macro set_mirroring mode
.endmacro
