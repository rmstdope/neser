; test_macros.inc — Test framework macros
;
; Provides macros for test sequencing, assertions, and result reporting.
; All tests use the $6000 status byte protocol AND console output.

.include "nes.inc"

; Import cross-file symbols used by macros
.ifndef __SHELL_S__
.import do_fail_test
.endif
.import console_print_inline, console_flush, console_newline

; ============================================================
; Test lifecycle macros
; ============================================================

; Start a new sub-test with a given number and name
; Usage: start_test 1, "Bank 0 select"
.macro start_test num, name
    lda #num
    sta TEST_CODE
    jsr console_print_inline
    .byte "Test ", '0' + num, ": ", name, 0
.endmacro

; Report sub-test passed and move to next line
.macro pass_test
    jsr console_print_inline
    .byte "..OK", 0
    jsr console_flush
    jsr console_newline
.endmacro

; Report sub-test failed with expected/got values
; expected and got are in A (expected) and X (got)
; Halts test execution with error code
.macro fail_test
    jsr do_fail_test
.endmacro

; Report all tests passed and re-enable rendering
.macro all_passed
    jsr console_flush
    jsr console_newline
    jsr console_print_inline
    .byte "PASSED", 0
    jsr console_flush
    jsr console_show       ; Re-enable rendering to ensure console output is visible
    lda #STATUS_PASS
    sta TEST_STATUS
    ; Infinite loop
:   jmp :-
.endmacro

; ============================================================
; Assertion macros
; ============================================================

; Assert A equals immediate value. If not, fail with error.
; Usage: assert_a_eq $A5
.macro assert_a_eq expected
    cmp #expected
    beq :+
    ; A has the "got" value, save it
    tax
    lda #expected
    fail_test
:
.endmacro

; Assert zero-page location equals immediate value
; Usage: assert_mem_eq $80, $A5
.macro assert_mem_eq addr, expected
    lda addr
    cmp #expected
    beq :+
    tax
    lda #expected
    fail_test
:
.endmacro

; Assert A does NOT equal immediate value (negative test)
; Usage: assert_a_neq $00
.macro assert_a_neq unexpected
    cmp #unexpected
    bne :+
    ; Unexpected match — report fail
    tax                     ; got = A
    lda #unexpected
    fail_test
:
.endmacro

; Skip test with message (for unsupported features)
.macro skip_test name
    jsr console_print_inline
    .byte "SKIP: ", name, 0
    jsr console_flush
    jsr console_newline
.endmacro
