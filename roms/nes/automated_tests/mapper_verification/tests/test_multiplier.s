; test_multiplier.s — Hardware Multiplier Verification
;
; Tests the MMC5 8×8 unsigned hardware multiplier.
; Write multiplicand to $5205, multiplier to $5206.
; Read result: low byte from $5205, high byte from $5206.
;
; Tests:
;   1. 0 × 0 = 0
;   2. 1 × 1 = 1
;   3. 10 × 20 = 200
;   4. 255 × 255 = 65025
;   5. 128 × 2 = 256

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Multiplier m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

.proc run_tests
    ; ========================================
    ; Test 1: 0 × 0 = 0
    ; ========================================
    start_test 1, "0x0=0"
    lda #0
    sta MMC5_MULT_A
    sta MMC5_MULT_B
    lda MMC5_MULT_A         ; Result low
    assert_a_eq 0
    pass_test

    ; ========================================
    ; Test 2: 1 × 1 = 1
    ; ========================================
    start_test 2, "1x1=1"
    lda #1
    sta MMC5_MULT_A
    sta MMC5_MULT_B
    lda MMC5_MULT_A         ; Result low = 1
    assert_a_eq 1
    pass_test

    ; ========================================
    ; Test 3: 10 × 20 = 200 ($00C8)
    ; ========================================
    start_test 3, "10x20 lo"
    lda #10
    sta MMC5_MULT_A
    lda #20
    sta MMC5_MULT_B
    lda MMC5_MULT_A         ; Result low = $C8
    assert_a_eq $C8
    pass_test

    start_test 4, "10x20 hi"
    lda MMC5_MULT_B         ; Result high = $00
    assert_a_eq $00
    pass_test

    ; ========================================
    ; Test 5: 255 × 255 = 65025 ($FE01)
    ; ========================================
    start_test 5, "FFxFF lo"
    lda #$FF
    sta MMC5_MULT_A
    sta MMC5_MULT_B
    lda MMC5_MULT_A         ; Result low = $01
    assert_a_eq $01
    pass_test

    start_test 6, "FFxFF hi"
    lda MMC5_MULT_B         ; Result high = $FE
    assert_a_eq $FE
    pass_test

    ; ========================================
    ; Test 7: 128 × 2 = 256 ($0100)
    ; ========================================
    start_test 7, "128x2 lo"
    lda #128
    sta MMC5_MULT_A
    lda #2
    sta MMC5_MULT_B
    lda MMC5_MULT_A         ; Result low = $00
    assert_a_eq $00
    pass_test

    start_test 8, "128x2 hi"
    lda MMC5_MULT_B         ; Result high = $01
    assert_a_eq $01
    pass_test

    rts
.endproc

; Export unique name for combined ROM builds
run_multiplier = run_tests
.export run_multiplier

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
