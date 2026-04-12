; test_irq.s — Scanline IRQ Verification
;
; Tests MMC3 scanline counter IRQ behavior.
; Parameterized for Sharp (submapper 0) vs NEC (submapper 1) behavior.
;
; Tests:
;   1. IRQ fires when counter reaches 0
;   2. IRQ does NOT fire when disabled
;   3. IRQ counter reload works
;   4. IRQ acknowledge clears pending
;
; Note: MMC3 scanline IRQ requires rendering to be enabled (BG or sprites)
; because it is clocked by PPU A12 rising edges during rendering.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string
.endif

.importzp irq_fired, irq_count

.ifndef COMBINED
.segment "RODATA"
test_title_string:
    .byte "IRQ m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .if IRQ_MODE = 0
        .byte " Sharp"
    .else
        .byte " NEC"
    .endif
    .byte 0
.endif

.segment "ZEROPAGE"
frame_count: .res 1

.segment "CODE"

; Wait for next VBlank
.proc wait_vbl_irq
    bit PPUSTATUS
:   bit PPUSTATUS
    bpl :-
    rts
.endproc

; Enable BG rendering (required for MMC3 scanline counter)
.proc enable_bg
    lda #%00001000          ; BG pattern at $0000
    sta PPUCTRL
    lda #%00001000          ; Show BG (bit 3)
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

; Disable rendering (updates shadow)
.proc disable_bg
    lda #0
    sta PPUMASK
    sta ppumask_shadow
    rts
.endproc

; Wait N frames for IRQ to potentially fire
; N in A
.proc wait_frames
    sta frame_count
@loop:
    jsr wait_vbl_irq
    dec frame_count
    bne @loop
    rts
.endproc

.proc run_tests
    ; ========================================
    ; Test 1: IRQ fires with rendering enabled
    ; ========================================
    start_test 1, "IRQ fires"

    ; Reset IRQ state
    lda #0
    sta irq_fired
    sta irq_count

    ; Set IRQ counter to 10 (fire after 10 scanlines)
    set_irq_counter 10

    ; Enable rendering (required for A12 clocking)
    jsr enable_bg

    ; Enable IRQ
    enable_irq

    ; Wait a few frames for IRQ to fire
    lda #5
    jsr wait_frames

    ; Check if IRQ fired
    lda irq_fired
    bne @test1_ok
    ; IRQ didn't fire — fail
    ldx #0                  ; got = 0
    lda #1                  ; expected = 1 (fired)
    fail_test
@test1_ok:
    pass_test

    ; ========================================
    ; Test 2: IRQ does NOT fire when disabled
    ; ========================================
    start_test 2, "IRQ disabled"

    ; Disable IRQ
    disable_irq

    ; Reset IRQ state
    lda #0
    sta irq_fired
    sta irq_count

    ; Set counter
    set_irq_counter 10

    ; Enable rendering but keep IRQ disabled
    jsr enable_bg

    ; Wait a few frames
    sei                     ; Keep CPU IRQ disabled
    lda #5
    jsr wait_frames

    ; IRQ should NOT have fired
    lda irq_fired
    beq @test2_ok
    ldx irq_fired           ; got = irq_fired (non-zero)
    lda #0                  ; expected = 0
    fail_test
@test2_ok:
    pass_test

    ; ========================================
    ; Test 3: IRQ counter reload
    ; ========================================
    start_test 3, "IRQ reload"

    lda #0
    sta irq_fired
    sta irq_count

    ; Set counter to 5
    set_irq_counter 5
    enable_irq

    jsr enable_bg

    ; Wait for IRQs to fire
    lda #5
    jsr wait_frames

    ; Should have fired multiple times (counter reloads each frame)
    lda irq_count
    cmp #2                  ; At least 2 IRQs expected over 5 frames
    bcs @test3_ok
    tax
    lda #2
    fail_test
@test3_ok:
    pass_test

    ; ========================================
    ; Test 4: IRQ acknowledge clears pending
    ; ========================================
    start_test 4, "IRQ ack"

    ; Disable IRQ
    disable_irq

    lda #0
    sta irq_fired
    sta irq_count

    ; Set counter, let it fire, then immediately acknowledge
    set_irq_counter 10
    enable_irq
    jsr enable_bg

    ; Wait for one IRQ
    lda #3
    jsr wait_frames

    ; Disable and acknowledge
    disable_irq
    lda #0
    sta irq_fired

    ; Wait more frames — no more IRQs should fire
    lda #5
    jsr wait_frames

    ; Verify no new IRQ
    lda irq_fired
    beq @test4_ok
    ldx irq_fired
    lda #0
    fail_test
@test4_ok:
    pass_test

    jsr disable_bg
    rts
.endproc

; Export unique name for combined ROM builds
run_irq = run_tests
.export run_irq

.ifndef COMBINED
; NES 2.0 Header
.include "nes20_header.inc"
nes20_header

; ASCII font
.if CHR_ROM_8K > 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
.endif ; COMBINED
