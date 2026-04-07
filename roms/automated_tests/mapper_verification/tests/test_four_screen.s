; test_four_screen.s — Four-Screen Nametable Verification
;
; Tests that all four nametables ($2000, $2400, $2800, $2C00) are
; independent — no mirroring between any pair.  This is the expected
; behaviour for mappers that provide extra VRAM on the cartridge and
; wire all four nametable pages to separate physical RAM.
;
; Capability flag required in defs: HAS_FOUR_SCREEN = 1

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "4-Screen m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
fs_read: .res 1

.segment "CODE"

; Write a value to a specific nametable offset
; A = value, X = high byte of NT address ($20/$24/$28/$2C), Y = low byte offset
.proc write_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    sta PPUDATA
    rts
.endproc

; Read a value from a specific nametable offset
; X = high byte of NT address, Y = low byte offset
; Returns value in A
.proc read_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Actual value
    rts
.endproc

; Disable rendering for safe PPU access
.proc disable_rendering
    lda #0
    sta PPUMASK
    sta ppumask_shadow
    rts
.endproc

; Re-enable rendering
.proc enable_rendering
    lda #%00001010
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

.proc run_tests
    jsr disable_rendering

    ; ========================================
    ; Write unique values to all four nametables
    ; ========================================
    ; Use offset $0F to avoid the console text area

    lda #$AA
    ldx #$20
    ldy #$0F
    jsr write_nt            ; NT A ($2000) = $AA

    lda #$BB
    ldx #$24
    ldy #$0F
    jsr write_nt            ; NT B ($2400) = $BB

    lda #$CC
    ldx #$28
    ldy #$0F
    jsr write_nt            ; NT C ($2800) = $CC

    lda #$DD
    ldx #$2C
    ldy #$0F
    jsr write_nt            ; NT D ($2C00) = $DD

    ; ========================================
    ; Test 1: NT A retained its value
    ; ========================================
    ldx #$20
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 1, "NT A=$AA"
    lda fs_read
    assert_a_eq $AA
    pass_test

    ; ========================================
    ; Test 2: NT B is independent of NT A
    ; ========================================
    jsr disable_rendering
    ldx #$24
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 2, "NT B=$BB"
    lda fs_read
    assert_a_eq $BB
    pass_test

    ; ========================================
    ; Test 3: NT C is independent
    ; ========================================
    jsr disable_rendering
    ldx #$28
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 3, "NT C=$CC"
    lda fs_read
    assert_a_eq $CC
    pass_test

    ; ========================================
    ; Test 4: NT D is independent
    ; ========================================
    jsr disable_rendering
    ldx #$2C
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 4, "NT D=$DD"
    lda fs_read
    assert_a_eq $DD
    pass_test

    ; ========================================
    ; Test 5: Overwrite NT A, verify others unchanged
    ; ========================================
    jsr disable_rendering

    lda #$11
    ldx #$20
    ldy #$0F
    jsr write_nt            ; Overwrite NT A with $11

    ; NT B should still be $BB
    ldx #$24
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 5, "B after A"
    lda fs_read
    assert_a_eq $BB
    pass_test

    ; ========================================
    ; Test 6: NT C still independent after NT A change
    ; ========================================
    jsr disable_rendering
    ldx #$28
    ldy #$0F
    jsr read_nt
    sta fs_read

    jsr enable_rendering
    start_test 6, "C after A"
    lda fs_read
    assert_a_eq $CC
    pass_test

    rts
.endproc

; Export unique name for combined ROM builds
run_four_screen = run_tests
.export run_four_screen

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
