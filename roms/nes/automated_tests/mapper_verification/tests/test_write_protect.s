; test_write_protect.s — PRG-RAM Write Protection Verification
;
; Tests that write-protection register correctly prevents writes to PRG-RAM.
;
; Test sequence:
;   1. Enable PRG-RAM with write enabled, verify writes work
;   2. Enable write-protection, verify writes are ignored
;   3. Disable write-protection, verify writes work again
;
; Only built for mappers with HAS_PRG_RAM_PROTECT = 1

.include "test_macros.inc"
.include "mapper_config.inc"
.include "nes20_header.inc"

.ifndef COMBINED
nes20_header

.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "WP m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

; Export run_write_protect alias for combined-mode runner
run_write_protect = run_tests
.export run_write_protect

.proc run_tests
    ; ========================================
    ; Test 1: PRG-RAM writable when enabled
    ; ========================================
    start_test 1, "RAM write OK"

    enable_prg_ram              ; Enable chip with writes allowed
    
    ; Write test value to PRG-RAM
    lda #$42
    sta $6000
    
    ; Read back and verify
    lda $6000
    assert_a_eq $42
    
    pass_test

    ; ========================================
    ; Test 2: Write-protect blocks writes
    ; ========================================
    start_test 2, "WP blocks wr"

    write_protect_prg_ram       ; Enable chip but block writes
    
    ; Try to write different value
    lda #$99
    sta $6000
    
    ; Should still read old value ($42)
    lda $6000
    assert_a_eq $42             ; Write should have been blocked
    
    pass_test

    ; ========================================
    ; Test 3: Writes work after re-enabling
    ; ========================================
    start_test 3, "WP re-enable"

    enable_prg_ram              ; Re-enable writes
    
    ; Write new value
    lda #$AA
    sta $6000
    
    ; Should read new value
    lda $6000
    assert_a_eq $AA
    
    pass_test

    ; ========================================
    ; Test 4: Write-protect preserves data
    ; ========================================
    start_test 4, "WP preserves"

    enable_prg_ram
    
    ; Write pattern
    ldx #0
@loop:
    txa
    sta $6000, x
    inx
    cpx #16
    bne @loop
    
    ; Enable write-protect
    write_protect_prg_ram
    
    ; Try to corrupt pattern
    ldx #0
@corrupt_loop:
    lda #$FF
    sta $6000, x
    inx
    cpx #16
    bne @corrupt_loop
    
    ; Verify pattern preserved
    ldx #0
@verify_loop:
    lda $6000, x
    stx $00
    lda $00
    cmp $6000, x
    beq @match                  ; Should still have X value
    fail_test
@match:
    inx
    cpx #16
    bne @verify_loop
    
    pass_test

    ; All tests passed
    rts
.endproc

; Export PRG-RAM accessor only when not in combined mode
; (in combined mode, test_prg_ram.s provides this symbol)
.ifndef COMBINED
.export run_prg_ram
.proc run_prg_ram
    lda #$FF
    sta $6000
    lda $6000
    rts
.endproc
.endif

.ifndef COMBINED
; Provide CHR data (minimal — not tested by this ROM)
.segment "CHARS"
    .incbin "ascii.chr"
.endif
