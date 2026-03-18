; test_mode2.s — Mapper 15 Mode 2 (NROM-64) Banking Verification
;
; Tests that PRG banking mode 2 maps the same single 8KB page
; to all four windows ($8000-$FFFF).
;
; NESdev spec (mode 2, SS=10): PRG A13 = p (data bit 7)
;   For bank_select=N, sub_bank=p:
;     All windows → 8KB page (N*2 | p)
;
; Uses a RAM trampoline since mode 2 remaps all of $8000-$FFFF.

.include "test_macros.inc"
.include "mapper_config.inc"

TRAMPOLINE_ADDR = $0400

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Mode 2 m015.0", 0
.endif

.segment "BSS"
tramp_result: .res 4

.segment "CODE"

.proc run_tests
    jsr install_trampoline

    ; === Test bank_select=3, p=0: all windows → page 6 ===

    start_test 1, "M2 $8000"
    lda #3                  ; bank_select=3, M=0, p=0
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 6
    pass_test

    start_test 2, "M2 $A000"
    lda tramp_result + 1
    assert_a_eq 6
    pass_test

    start_test 3, "M2 $C000"
    lda tramp_result + 2
    assert_a_eq 6
    pass_test

    start_test 4, "M2 $E000"
    lda tramp_result + 3
    assert_a_eq 6
    pass_test

    ; === Test bank_select=5, p=0: all windows → page 10 ===

    start_test 5, "M2b5 $8000"
    lda #5                  ; bank_select=5, M=0, p=0
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 10
    pass_test

    start_test 6, "M2b5 all eq"
    lda tramp_result + 1
    cmp tramp_result
    beq :+
    tax
    lda tramp_result
    fail_test
:   lda tramp_result + 2
    cmp tramp_result
    beq :+
    tax
    lda tramp_result
    fail_test
:   lda tramp_result + 3
    cmp tramp_result
    beq :+
    tax
    lda tramp_result
    fail_test
:   pass_test

    ; === Test with p=1 (sub_bank): bank_select=3, p=1 → page 7 ===

    start_test 7, "M2 p=1"
    lda #$83                ; bank_select=3 (bits 6:0=03), p=1 (bit 7)
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 7
    pass_test

    rts
.endproc

run_mode2 = run_tests
.export run_mode2

; ============================================================
; Trampoline: switch to mode 2, read sigs, switch back
; ============================================================

.proc install_trampoline
    ldx #0
@copy:
    lda trampoline_code, x
    sta TRAMPOLINE_ADDR, x
    inx
    cpx #(trampoline_end - trampoline_code)
    bne @copy
    rts
.endproc

.proc call_trampoline
    jmp TRAMPOLINE_ADDR
.endproc

; Trampoline code — copied to RAM and executed from there.
; Input: A = data value (bank_select | mirroring | sub_bank bits)
; Writes to $8002 (SS=10 → mode 2), reads signatures, restores mode 1.
trampoline_code:
    sta $8002               ; Mode 2, data=A
    lda $8001               ; Bank ID at $8000 window
    sta tramp_result
    lda $A001               ; Bank ID at $A000 window
    sta tramp_result + 1
    lda $C001               ; Bank ID at $C000 window
    sta tramp_result + 2
    lda $E001               ; Bank ID at $E000 window
    sta tramp_result + 3
    lda #0
    sta $8001               ; Restore mode 1, bank 0, vertical
    rts
trampoline_end:

; ============================================================
; 8KB bank signatures — $A5, page_num, ~page_num, $5A
; ============================================================

.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A
.segment "PRG_SIG1"
    .byte $A5, 1, $FE, $5A
.segment "PRG_SIG4"
    .byte $A5, 4, $FB, $5A
.segment "PRG_SIG5"
    .byte $A5, 5, $FA, $5A
.segment "PRG_SIG6"
    .byte $A5, 6, $F9, $5A
.segment "PRG_SIG7"
    .byte $A5, 7, $F8, $5A
.segment "PRG_SIG8"
    .byte $A5, 8, $F7, $5A
.segment "PRG_SIG9"
    .byte $A5, 9, $F6, $5A
.segment "PRG_SIG10"
    .byte $A5, 10, $F5, $5A
.segment "PRG_SIG11"
    .byte $A5, 11, $F4, $5A
.segment "PRG_SIG12"
    .byte $A5, 12, $F3, $5A
.segment "PRG_SIG13"
    .byte $A5, 13, $F2, $5A

.ifndef COMBINED
.include "nes20_header.inc"
nes20_header
.endif
