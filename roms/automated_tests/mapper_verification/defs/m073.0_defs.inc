; m073.0_defs.inc — VRC3 (Mapper 73, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/VRC3
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last 16KB fixed at $C000
;   - CHR: 8KB CHR-RAM only (no banking)
;   - PRG-RAM: 8KB at $6000-$7FFF
;   - Fixed mirroring (solder pads)
;   - IRQ: 16-bit CPU cycle up-counter with 8-bit mode option
;     - Latch loaded in 4 nibbles ($8000/$9000/$A000/$B000)
;     - Control at $C000: bit 2 = 8-bit mode, bit 1 = enable, bit 0 = ack-enable
;     - Acknowledge at $D000 (also moves A→E)
;     - Counter increments each CPU cycle; fires on overflow ($FFFF→$0000)
;   - No bus conflicts
;
; Register layout:
;   $8000-$8FFF: IRQ latch bits 3-0
;   $9000-$9FFF: IRQ latch bits 7-4
;   $A000-$AFFF: IRQ latch bits 11-8
;   $B000-$BFFF: IRQ latch bits 15-12
;   $C000-$CFFF: IRQ control [.... .MEA]
;   $D000-$DFFF: IRQ acknowledge (+ moves A→E)
;   $F000-$FFFF: PRG bank select (bits 2-0)

.define MAPPER_NUM 73
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB banks
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB PRG-RAM
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; IRQ behavior — CPU cycle up-counter
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 16       ; 16KB switchable
.define CHR_BANK_SIZE 8        ; 8KB CHR-RAM (fixed)
.define PRG_BANK_COUNT 2       ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 0       ; No CHR banking

; ============================================================
; Register Addresses
; ============================================================

VRC3_IRQ_LATCH0 = $8000   ; IRQ latch bits 3-0
VRC3_IRQ_LATCH1 = $9000   ; IRQ latch bits 7-4
VRC3_IRQ_LATCH2 = $A000   ; IRQ latch bits 11-8
VRC3_IRQ_LATCH3 = $B000   ; IRQ latch bits 15-12
VRC3_IRQ_CTRL   = $C000   ; IRQ control [.... .MEA]
VRC3_IRQ_ACK    = $D000   ; IRQ acknowledge
VRC3_PRG_SELECT = $F000   ; PRG bank select (bits 2-0)

; ============================================================
; Register Macros
; ============================================================

; Select 16KB PRG bank at $8000-$BFFF
.macro select_prg_bank slot, bank
    lda #bank
    sta VRC3_PRG_SELECT
.endmacro

; No CHR banking
.macro select_chr_bank slot, bank
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro

; IRQ counter: CPU cycle up-counter.
; The shared test passes scanline-scale values; multiply by ~114 for CPU cycles.
; Load the 16-bit latch in 4 nibble writes, then compute reload value as
; ($10000 - cycles_wanted) so the counter overflows after that many cycles.
.macro set_irq_counter value
    .local reload_val
    reload_val = $10000 - (value * 114)
    lda #<(reload_val) & $0F
    sta VRC3_IRQ_LATCH0        ; Nibble 0
    lda #(<(reload_val) >> 4) & $0F
    sta VRC3_IRQ_LATCH1        ; Nibble 1
    lda #>(reload_val) & $0F
    sta VRC3_IRQ_LATCH2        ; Nibble 2
    lda #(>(reload_val) >> 4) & $0F
    sta VRC3_IRQ_LATCH3        ; Nibble 3
.endmacro

.macro enable_irq
    lda #$02                ; E=1 (enable), reload counter from latch
    sta VRC3_IRQ_CTRL
    cli
.endmacro

.macro disable_irq
    lda #$00                ; E=0 (disable)
    sta VRC3_IRQ_CTRL
    lda #$00
    sta VRC3_IRQ_ACK        ; Acknowledge pending IRQ
    sei
.endmacro

; VRC3: PRG-RAM is always enabled (no control register)
.macro enable_prg_ram
.endmacro
