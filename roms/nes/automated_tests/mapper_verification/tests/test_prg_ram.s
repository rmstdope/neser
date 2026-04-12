; test_prg_ram.s — PRG-RAM Read/Write Verification
;
; Tests PRG-RAM at $6000-$7FFF:
;   1. Basic read/write pattern
;   2. Write protection (for mappers that support it)
;
; Note: $6000-$6003 are reserved for the test status protocol.
; All PRG-RAM tests use $6004 and above.

.include "test_macros.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "PRG-RAM m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

; Test addresses in PRG-RAM (above status byte area)
PRG_RAM_TEST = $6004
PRG_RAM_END  = $6100         ; Test first 252 bytes

.segment "CODE"

.proc run_tests
    ; ========================================
    ; Test 1: Basic write/read
    ; ========================================
    start_test 1, "RAM write"

    ; Enable PRG-RAM if mapper supports it
    enable_prg_ram

    ; Write ascending pattern
    ldx #0
@write_loop:
    txa
    sta PRG_RAM_TEST, x
    inx
    cpx #(PRG_RAM_END - PRG_RAM_TEST)
    bne @write_loop
    pass_test

    ; ========================================
    ; Test 2: Read back and verify
    ; ========================================
    start_test 2, "RAM read"

    ldx #0
@read_loop:
    lda PRG_RAM_TEST, x
    stx TEST_TEMP2             ; Save X
    ; Compare A (read value) with X (expected)
    cmp TEST_TEMP2
    beq @read_ok
    ; Failed: A != X
    tax                     ; got = A (already in A via tax, X via read)
    lda TEST_TEMP2
    fail_test
@read_ok:
    ldx TEST_TEMP2
    inx
    cpx #(PRG_RAM_END - PRG_RAM_TEST)
    bne @read_loop
    pass_test

    ; ========================================
    ; Test 3: Complementary pattern
    ; ========================================
    start_test 3, "RAM pattern"

    ; Write complement pattern ($FF - x)
    ldx #0
@write2:
    txa
    eor #$FF
    sta PRG_RAM_TEST, x
    inx
    cpx #(PRG_RAM_END - PRG_RAM_TEST)
    bne @write2

    ; Read back
    ldx #0
@read2:
    txa
    eor #$FF
    sta TEST_TEMP              ; Expected value
    lda PRG_RAM_TEST, x
    cmp TEST_TEMP
    beq @read2_ok
    tax
    lda TEST_TEMP
    fail_test
@read2_ok:
    inx
    cpx #(PRG_RAM_END - PRG_RAM_TEST)
    bne @read2
    pass_test

    ; ========================================
    ; Test 4: Write protection (if supported)
    ; ========================================
.if HAS_PRG_RAM_PROTECT

    start_test 4, "Write prot"

    ; Write known value
    enable_prg_ram
    lda #$42
    sta PRG_RAM_TEST

    ; Enable write protection
    write_protect_prg_ram

    ; Attempt to write different value (should be ignored)
    lda #$99
    sta PRG_RAM_TEST

    ; Read back — should still be $42
    lda PRG_RAM_TEST
    assert_a_eq $42
    pass_test

    ; ========================================
    ; Test 5: Re-enable writes
    ; ========================================
    start_test 5, "Write enable"

    ; Re-enable writes
    enable_prg_ram

    ; Write should work now
    lda #$55
    sta PRG_RAM_TEST
    lda PRG_RAM_TEST
    assert_a_eq $55
    pass_test

.endif

    rts
.endproc

; Export unique name for combined ROM builds
run_prg_ram = run_tests
.export run_prg_ram

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
