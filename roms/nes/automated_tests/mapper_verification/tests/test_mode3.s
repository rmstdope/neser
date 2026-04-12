; test_mode3.s — Mapper 15 Mode 3 (NROM-128) Banking Verification
;
; Tests that PRG banking mode 3 maps a 16KB page mirrored into
; both halves of the $8000-$FFFF address space.
;
; NESdev spec (mode 3, SS=11): NROM-128
;   For bank_select=N, sub_bank=p:
;     lower = N*2 | p
;     $8000-$9FFF → 8KB page lower
;     $A000-$BFFF → 8KB page lower+1
;     $C000-$DFFF → 8KB page lower      (mirror)
;     $E000-$FFFF → 8KB page lower+1    (mirror)
;
; Uses a RAM trampoline since mode 3 remaps all of $8000-$FFFF.

.include "test_macros.inc"
.include "mapper_config.inc"

TRAMPOLINE_ADDR = $0400

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Mode 3 m015.0", 0
.endif

.segment "BSS"
tramp_result: .res 4

.segment "CODE"

.proc run_tests
    jsr install_trampoline

    ; === Test bank_select=3, p=0: lower=6, pages 6,7,6,7 ===

    start_test 1, "M3 $8000"
    lda #3                  ; bank_select=3, M=0, p=0
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 6
    pass_test

    start_test 2, "M3 $A000"
    lda tramp_result + 1
    assert_a_eq 7
    pass_test

    start_test 3, "M3 $C000"
    lda tramp_result + 2
    assert_a_eq 6
    pass_test

    start_test 4, "M3 $E000"
    lda tramp_result + 3
    assert_a_eq 7
    pass_test

    ; === Test bank_select=4, p=0: lower=8, pages 8,9,8,9 ===

    start_test 5, "M3b4 $8000"
    lda #4                  ; bank_select=4, M=0, p=0
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 8
    pass_test

    start_test 6, "M3b4 $A000"
    lda tramp_result + 1
    assert_a_eq 9
    pass_test

    ; Verify mirroring: $C000 == $8000, $E000 == $A000
    start_test 7, "M3 mirror"
    lda tramp_result + 2
    cmp tramp_result
    beq :+
    tax
    lda tramp_result
    fail_test
:   lda tramp_result + 3
    cmp tramp_result + 1
    beq :+
    tax
    lda tramp_result + 1
    fail_test
:   pass_test

    rts
.endproc

run_mode3 = run_tests
.export run_mode3

; ============================================================
; Trampoline: switch to mode 3, read sigs, switch back
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
; Writes to $8003 (SS=11 → mode 3), reads signatures, restores mode 1.
trampoline_code:
    sta $8003               ; Mode 3, data=A
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
