; nes_init.s — NES initialization routines
;
; Provides: init_nes, wait_vbl

.include "nes.inc"

.segment "ZEROPAGE"
temp_ptr: .res 2

.segment "CODE"

; Export public routines
.export init_nes, wait_vbl

; Wait for vertical blank
.proc wait_vbl
    bit PPUSTATUS
:   bit PPUSTATUS
    bpl :-
    rts
.endproc

; Full NES initialization
; NOTE: sei/cld/txs must be done by the caller (reset handler)
; before calling this routine, because JSR pushes the return address
; and we must not clobber SP after that.
.proc init_nes
    ; Disable PPU
    lda #0
    sta PPUCTRL
    sta PPUMASK

    ; Disable APU IRQ, set frame counter mode
    lda #$40
    sta JOY2               ; Frame counter: mode 1 (5-step), disable IRQ
    lda #0
    sta SND_CHN             ; Disable all audio channels

    ; First VBL wait
    jsr wait_vbl

    ; Clear RAM ($0000-$07FF), skip stack page ($0100-$01FF)
    ; Stack page is not cleared because we were called via JSR
    ; and the return address is on the stack.
    lda #0
    tax
@clear_ram:
    sta $0000, x
    sta $0200, x
    sta $0300, x
    sta $0400, x
    sta $0500, x
    sta $0600, x
    sta $0700, x
    inx
    bne @clear_ram

    ; Second VBL wait — PPU is now ready
    jsr wait_vbl

    rts
.endproc
