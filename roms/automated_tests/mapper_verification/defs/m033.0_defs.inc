; m033.0_defs.inc — Taito TC0190 (Mapper 33, Submapper 0)
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - CHR banking: mixed 2K+1K (2×2K + 4×1K)
;   - Mirroring: H/V via bit 6 of $8000 (shared with PRG Reg 0)
;   - No IRQ, no PRG-RAM

.define MAPPER_NUM 33
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB (header field, not used)
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 6       ; 6 slots: 2×2K + 4×1K

; ============================================================
; Taito Core Macros (shared with mapper 48)
; ============================================================

.include "taito_core_macros.inc"

; ============================================================
; Mirroring
; ============================================================

; Set mirroring mode: 0=Vertical, 1=Horizontal
; On mapper 33, mirroring is bit 6 of $8000 (shared with PRG Reg 0).
; Writing here also sets PRG bank 0 to 0 as a side-effect.
.macro set_mirroring mode
    lda #(mode << 6)
    sta TAITO_PRG0
.endmacro

; PRG-RAM is always enabled (no protect register)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
