; m006.0_defs.inc — Front Fareast Magic Card (Mapper 6, Submapper 0 → effective 1)
;
; Power-on state simulates $42FF = $20 | (H ? $10 : $00):
;   Latch mode 1 (UN1ROM+CHRSW)
;   PRG: bits 5-2 of latch → 16KB bank at $8000, bank #7 fixed at $C000
;   CHR: bits 1-0 of latch → 8KB bank
;
; Features:
;   - PRG banking: 16KB switchable at $8000 + fixed at $C000
;   - CHR banking: 8KB via latch bits 1-0
;   - Mirroring: via $42FC-$42FF address/data bits
;   - WRAM: 32KB banked (4 × 8KB via $4500 bits 5-4)
;   - IRQ: 16-bit up-counter, fires on $FFFF→$0000 wrap

.define MAPPER_NUM 6
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8         ; 128KB = 8 × 16KB
CHR_ROM_8K    = 4         ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 9         ; 32KB WRAM (64 << 9 = 32KB NES 2.0 non-battery)
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0

; IRQ behavior (M2 cycle counter, not scanline)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB switchable
.define PRG_BANK_COUNT 4      ; 4 switchable 16KB banks (0-3 visible, 4-7 also exist)
.define CHR_BANK_COUNT 4      ; 4 × 8KB banks

; ============================================================
; Register addresses
; ============================================================
SMC_MODE_REG  = $42FE         ; Mode register ($42FC-$42FF)
SMC_CTRL      = $4500         ; Control: WRAM bank, CHR mode, IRQ mode
SMC_IRQ_ACK   = $4501         ; IRQ acknowledge + disable
SMC_IRQ_LO    = $4502         ; IRQ counter latch LSB
SMC_IRQ_HI    = $4503         ; IRQ counter MSB + enable
SMC_LATCH     = $8000         ; Latch register ($8000-$FFFF)

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000 (mode 1: bits 5-2 = bank)
; Preserves CHR bank 0 (bits 1-0 = 0)
.macro select_prg_bank slot, bank
    lda #(bank << 2)
    sta SMC_LATCH
.endmacro

; Select 8KB CHR bank (mode 1: bits 1-0 = bank)
; Sets PRG bank to 0 (bits 5-2 = 0)
.macro select_chr_bank slot, bank
    lda #bank
    sta SMC_LATCH
.endmacro

; Set mirroring via $42FE
; Uses MMC1-compatible convention:
;   0 = single-screen lower  (mirroring_type = 0)
;   1 = single-screen upper  (mirroring_type = 1)
;   2 = vertical              (mirroring_type = 2)
;   3 = horizontal            (mirroring_type = 3)
;
; mirroring_type = (A0 << 1) | D4
; To keep latch enabled: use addr with A1=1 ($42FE or $42FF)
; Mode bits D7-D5 = 001 (keep mode 1 for UN1ROM)
.macro set_mirroring mode
    .if mode = 0
        ; mirroring_type = 0 = (0<<1)|0 → A0=0, D4=0 → addr=$42FE, data=$20
        lda #$20
        sta $42FE
    .elseif mode = 1
        ; mirroring_type = 1 = (0<<1)|1 → A0=0, D4=1 → addr=$42FE, data=$30
        lda #$30
        sta $42FE
    .elseif mode = 2
        ; mirroring_type = 2 = (1<<1)|0 → A0=1, D4=0 → addr=$42FF, data=$20
        lda #$20
        sta $42FF
    .elseif mode = 3
        ; mirroring_type = 3 = (1<<1)|1 → A0=1, D4=1 → addr=$42FF, data=$30
        lda #$30
        sta $42FF
    .endif
.endmacro

; IRQ control — 16-bit up-counter (M2 mode)
; Counter wraps $FFFF→$0000 and fires IRQ
.macro set_irq_counter value
    ; Load counter to fire after 'value' CPU cycles
    ; Counter counts UP from loaded value to $FFFF then wraps
    ; To fire after N cycles, load counter = $10000 - N
    lda #<($10000 - value)
    sta SMC_IRQ_LO
    lda #>($10000 - value)
    sta SMC_IRQ_HI
.endmacro

.macro enable_irq
    ; Counter is already enabled by writing $4503
    ; Just need to clear interrupt mask
    cli
.endmacro

.macro disable_irq
    lda #0
    sta SMC_IRQ_ACK         ; Acknowledge + disable counting
    sei
.endmacro

; PRG-RAM control
; WRAM is always enabled on mapper 6 (no write protection)
; Must keep play mode (bit 6) and CIRAM nametables (bit 1) set
; Power-on $4500 = $42 = %01000010
SMC_PLAY_MODE = $42       ; Play mode + CIRAM nametables

.macro enable_prg_ram
    ; Select WRAM bank 0 (keep play mode + CIRAM nametables)
    lda #SMC_PLAY_MODE
    sta SMC_CTRL
.endmacro

.macro disable_prg_ram
    ; No-op: WRAM cannot be disabled on mapper 6
.endmacro

.macro write_protect_prg_ram
    ; No-op: no write protection on mapper 6
.endmacro
