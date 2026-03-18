; test_mode0.s — Mapper 15 Mode 0 (NROM-256) Banking Verification
;
; Tests that PRG banking mode 0 maps a contiguous 32KB block
; (four sequential 8KB pages) to $8000-$FFFF.
;
; NESdev spec (mode 0, SS=00): PRG A14 = CPU A14
;   For bank_select=N:
;     $8000-$9FFF → 8KB page N*2
;     $A000-$BFFF → 8KB page N*2+1
;     $C000-$DFFF → 8KB page N*2+2
;     $E000-$FFFF → 8KB page N*2+3
;
; Uses a RAM trampoline since mode 0 remaps all of $8000-$FFFF,
; displacing the running code. The trampoline switches mode, reads
; bank ID signatures from each window, switches back to mode 1,
; and returns.

.include "test_macros.inc"
.include "mapper_config.inc"

TRAMPOLINE_ADDR = $0400

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Mode 0 m015.0", 0
.endif

.segment "BSS"
tramp_result: .res 4

.segment "CODE"

.proc run_tests
    jsr install_trampoline

    ; === Test bank_select=2: pages 4,5,6,7 ===

    start_test 1, "M0 $8000"
    lda #2
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 4
    pass_test

    start_test 2, "M0 $A000"
    lda tramp_result + 1
    assert_a_eq 5
    pass_test

    start_test 3, "M0 $C000"
    lda tramp_result + 2
    assert_a_eq 6
    pass_test

    start_test 4, "M0 $E000"
    lda tramp_result + 3
    assert_a_eq 7
    pass_test

    ; === Test bank_select=4: pages 8,9,10,11 ===

    start_test 5, "M0b4 $8000"
    lda #4
    jsr call_trampoline
    lda tramp_result
    assert_a_eq 8
    pass_test

    start_test 6, "M0b4 $A000"
    lda tramp_result + 1
    assert_a_eq 9
    pass_test

    start_test 7, "M0b4 $C000"
    lda tramp_result + 2
    assert_a_eq 10
    pass_test

    rts
.endproc

run_mode0 = run_tests
.export run_mode0

; ============================================================
; Trampoline: switch to mode 0, read sigs, switch back
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
; Input: A = data value (bank_select, M=0, p=0)
; Writes to $8000 (SS=00 → mode 0), reads signatures, restores mode 1.
trampoline_code:
    sta $8000               ; Mode 0, bank_select=A, M=0, p=0
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
; Pages 2-3 (boot) and 14-15 (code) are omitted.
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
