; m048.0_defs.inc — Taito TC0690 (Mapper 48, Submapper 0)
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - CHR banking: mixed 2K+1K (2×2K + 4×1K)
;   - Mirroring: H/V via bit 6 of $E000 (separate register)
;   - Scanline IRQ counter
;   - No PRG-RAM

.define MAPPER_NUM 48
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB (header field, not used)
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp-like (fires when counter reaches 0)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 6       ; 6 slots: 2×2K + 4×1K

; ============================================================
; Taito Core Macros (shared with mapper 33)
; ============================================================

.include "taito_core_macros.inc"

; ============================================================
; Mirroring
; ============================================================

TAITO_MIRROR   = $E000  ; Mirroring register (bit 6: 0=V, 1=H)

; Set mirroring mode: 0=Vertical, 1=Horizontal
; On mapper 48, mirroring is bit 6 of $E000 (separate from PRG).
.macro set_mirroring mode
    lda #(mode << 6)
    sta TAITO_MIRROR
.endmacro

; ============================================================
; IRQ
; ============================================================

TAITO_IRQ_LATCH = $C000  ; IRQ reload value
TAITO_IRQ_RELOAD = $C001 ; IRQ clear/reload (write any value)
TAITO_IRQ_ENABLE = $C002 ; IRQ enable (write any value)
TAITO_IRQ_ACK    = $C003 ; IRQ acknowledge (write any value)

; Set IRQ counter reload value, then trigger reload
.macro set_irq_counter value
    lda #value
    sta TAITO_IRQ_LATCH
    lda #0
    sta TAITO_IRQ_RELOAD
.endmacro

; Enable IRQ (clear CLI + write enable register)
.macro enable_irq
    cli
    lda #0
    sta TAITO_IRQ_ENABLE
.endmacro

; Disable IRQ (acknowledge + set SEI)
.macro disable_irq
    lda #0
    sta TAITO_IRQ_ACK
    sei
.endmacro

; PRG-RAM is always enabled (no protect register)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
