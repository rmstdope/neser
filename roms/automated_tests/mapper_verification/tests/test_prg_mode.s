; test_prg_mode.s — PRG Mode Swap Verification (Mapper 32 Irem G-101)
;
; Tests that PRG mode switching works correctly.
; Mapper 32 $9000 bit 1 controls PRG mode:
;   Mode 0: Reg0@$8000, Reg1@$A000, fixed {-2}@$C000, fixed {-1}@$E000
;   Mode 1: fixed {-2}@$8000, Reg1@$A000, Reg0@$C000, fixed {-1}@$E000
;
; This test runs from $E000-$FFFF which is always fixed to the last bank
; in both modes, making it safe to switch modes mid-execution.
;
; Uses bank signatures: $A5, bank_num, ~bank_num, $5A at bank start.

.include "test_macros.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "PRG Mode m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

.proc run_tests
    ; ========================================
    ; Test 1: Mode 0 — Reg0 controls $8000
    ; ========================================
    start_test 1, "Mode0 Reg0@8"

    ; Set mode 0 (bit 1 = 0) and vertical mirroring (bit 0 = 0)
    lda #$00
    sta $9000

    ; Select bank 0 in Reg0
    lda #0
    sta $8000

    ; Verify bank 0 signature at $8000
    lda $8000
    assert_a_eq $A5
    lda $8001
    assert_a_eq 0
    pass_test

    ; ========================================
    ; Test 2: Mode 0 — switch Reg0 to bank 1
    ; ========================================
    start_test 2, "Mode0 bank 1"

    lda #1
    sta $8000

    ; Verify bank 1 at $8000
    lda $8000
    assert_a_eq $A5
    lda $8001
    assert_a_eq 1
    pass_test

    ; ========================================
    ; Test 3: Mode 0 — $C000 is fixed to {-2}
    ; ========================================
    start_test 3, "Mode0 fix $C"

    ; {-2} = second-to-last bank = PRG_ROM_16K*2 - 2
    lda $C001               ; Read bank ID byte from $C000 area
    assert_a_eq (PRG_ROM_16K * 2 - 2)
    pass_test

    ; ========================================
    ; Test 4: Switch to Mode 1 — Reg0 now at $C000
    ; ========================================
    start_test 4, "Mode1 Reg0@C"

    ; Switch to mode 1 (bit 1 = 1)
    lda #$02
    sta $9000

    ; Reg0 still has bank 1 from Test 2
    ; In mode 1, Reg0 maps to $C000
    lda $C000
    assert_a_eq $A5
    lda $C001
    assert_a_eq 1
    pass_test

    ; ========================================
    ; Test 5: Mode 1 — $8000 is now fixed to {-2}
    ; ========================================
    start_test 5, "Mode1 fix $8"

    lda $8001
    assert_a_eq (PRG_ROM_16K * 2 - 2)
    pass_test

    ; ========================================
    ; Test 6: Mode 1 — Reg0 switch at $C000
    ; ========================================
    start_test 6, "Mode1 bank 2"

    lda #2
    sta $8000               ; Write still goes to Reg0 via $8000

    ; But in mode 1, Reg0 appears at $C000
    lda $C000
    assert_a_eq $A5
    lda $C001
    assert_a_eq 2
    pass_test

    ; ========================================
    ; Test 7: Restore mode 0 and verify
    ; ========================================
    start_test 7, "Restore mode0"

    ; Restore mode 0
    lda #$00
    sta $9000

    ; Reg0 (bank 2) should now be back at $8000
    lda $8000
    assert_a_eq $A5
    lda $8001
    assert_a_eq 2

    ; $C000 should be fixed {-2} again
    lda $C001
    assert_a_eq (PRG_ROM_16K * 2 - 2)

    ; Restore Reg0 to bank 0
    lda #0
    sta $8000
    pass_test

    rts
.endproc

; Export unique name for combined ROM builds
run_prg_mode = run_tests
.export run_prg_mode

; ============================================================
; Bank signature data (needed for standalone builds)
; Each PRG bank gets: $A5, bank_num, ~bank_num, $5A
; ============================================================

.if HAS_PRG_BANKING

.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A
.segment "PRG_SIG1"
    .byte $A5, 1, $FE, $5A
.segment "PRG_SIG2"
    .byte $A5, 2, $FD, $5A
.segment "PRG_SIG3"
    .byte $A5, 3, $FC, $5A
.segment "PRG_SIG4"
    .byte $A5, 4, $FB, $5A
.segment "PRG_SIG5"
    .byte $A5, 5, $FA, $5A
; PRG_SIG6 = fixed second-to-last bank (needs sig for mode test)
.segment "PRG_SIG6"
    .byte $A5, 6, $F9, $5A

.endif ; HAS_PRG_BANKING

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
