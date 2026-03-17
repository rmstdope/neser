; mmc3_core_macros.inc — Shared MMC3 register macros
;
; This file contains register definitions and macros shared by all MMC3-based mappers:
; - Mapper 4 (MMC3/TxROM)
; - Mapper 12 (SL-5020B / MMC3 + outer CHR)
; - Mapper 14 (SL-1632 / MMC3+VRC2 hybrid)
;
; Each mapper-specific defs file includes this and adds its own extensions.

; ============================================================
; MMC3 Register Addresses
; ============================================================

MMC3_BANK_SELECT = $8000  ; Bank select: bits 0-2=register, bit 6=PRG mode, bit 7=CHR mode
MMC3_BANK_DATA   = $8001  ; Bank data: value for selected register
MMC3_MIRRORING   = $A000  ; Mirroring: bit 0 (0=vert mirror/horiz arr/A10, 1=horiz mirror/vert arr/A11)
MMC3_PRG_RAM     = $A001  ; PRG-RAM protect: bit 6=write enable, bit 7=chip enable
MMC3_IRQ_LATCH   = $C000  ; IRQ latch value
MMC3_IRQ_RELOAD  = $C001  ; IRQ counter reload (write any value)
MMC3_IRQ_DISABLE = $E000  ; IRQ disable + acknowledge (write any value)
MMC3_IRQ_ENABLE  = $E001  ; IRQ enable (write any value)

; MMC3 bank select register numbers:
;   R0: CHR $0000-$07FF (2KB) or $1000-$17FF (if CHR mode bit set)
;   R1: CHR $0800-$0FFF (2KB) or $1800-$1FFF
;   R2: CHR $1000-$13FF (1KB) or $0000-$03FF
;   R3: CHR $1400-$17FF (1KB) or $0400-$07FF
;   R4: CHR $1800-$1BFF (1KB) or $0800-$0BFF
;   R5: CHR $1C00-$1FFF (1KB) or $0C00-$0FFF
;   R6: PRG $8000-$9FFF or $C000-$DFFF (mode bit swaps these)
;   R7: PRG $A000-$BFFF (always)

; ============================================================
; Register Macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000 (R6), slot 1 = $A000 (R7)
; Slots 2,3 are fixed (second-to-last and last banks)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #6              ; R6: PRG $8000
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .elseif slot = 1
        lda #7              ; R7: PRG $A000
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .endif
.endmacro

; Select 1KB CHR bank
; slots 0-5 map to R0-R5
; R0,R1 select 2KB regions; R2-R5 select 1KB regions
.macro select_chr_bank slot, bank
    lda #slot
    sta MMC3_BANK_SELECT
    lda #bank
    sta MMC3_BANK_DATA
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
; Note: MMC3 only supports H/V, not single-screen
.macro set_mirroring mode
    .if mode = 0
        ; Vertical
        lda #0
        sta MMC3_MIRRORING
    .elseif mode = 1
        ; Horizontal
        lda #1
        sta MMC3_MIRRORING
    .else
        ; MMC3 doesn't support single-screen — map to vertical
        lda #0
        sta MMC3_MIRRORING
    .endif
.endmacro

; IRQ control
.macro set_irq_counter value
    lda #value
    sta MMC3_IRQ_LATCH
    lda #0
    sta MMC3_IRQ_RELOAD     ; Reload counter on next scanline
.endmacro

.macro enable_irq
    cli                     ; Enable CPU interrupts
    lda #0
    sta MMC3_IRQ_ENABLE
.endmacro

.macro disable_irq
    lda #0
    sta MMC3_IRQ_DISABLE    ; Disable + acknowledge
    sei                     ; Disable CPU interrupts
.endmacro

; PRG-RAM control
; $A001 bit 7 = chip enable, bit 6 = write protect
.macro enable_prg_ram
    lda #%10000000          ; Bit 7=chip enable, bit 6=0 (write enabled)
    sta MMC3_PRG_RAM
.endmacro

.macro disable_prg_ram
    lda #%00000000          ; Disable chip
    sta MMC3_PRG_RAM
.endmacro

.macro write_protect_prg_ram
    lda #%11000000          ; Chip enable + write protect
    sta MMC3_PRG_RAM
.endmacro
