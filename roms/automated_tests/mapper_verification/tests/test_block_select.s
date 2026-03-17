; test_block_select.s — Outer Bank/Block Switching Verification (Mapper 44)
;
; Tests that the MMC3 multicart block select mechanism works.
; Mapper 44 uses $A001 bits 0-2 for block selection.
; Each block constrains MMC3 bank values to a window of PRG/CHR ROM.
;
; The test verifies that switching blocks changes the visible PRG content
; by reading bank signatures from different blocks.
;
; IMPORTANT: Switching blocks changes the ENTIRE PRG address space including
; $E000 where code lives. All block switching must be done from a RAM
; trampoline to avoid crashing when the code bank changes underneath us.

.include "test_macros.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Block Sel m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "BSS"
blk_result:  .res 4             ; Signature bytes read from target block

.segment "ZEROPAGE"
blk_target:  .res 1             ; Target block number for trampoline

.ifndef BLOCK_SELECT_WRITE_ADDR
    BLOCK_SELECT_WRITE_ADDR = $A001
.endif
.ifndef BLOCK_SELECT_WRITE_OR_MASK
    BLOCK_SELECT_WRITE_OR_MASK = $80
.endif
.ifndef BLOCK_SELECT_RESTORE_VALUE
    BLOCK_SELECT_RESTORE_VALUE = BLOCK_SELECT_WRITE_OR_MASK
.endif
.ifndef BLOCK_SELECT_TEST1_TARGET
    BLOCK_SELECT_TEST1_TARGET = 0
.endif
.ifndef BLOCK_SELECT_TEST1_EXPECTED_ID
    BLOCK_SELECT_TEST1_EXPECTED_ID = 0
.endif
.ifndef BLOCK_SELECT_TEST2_TARGET
    BLOCK_SELECT_TEST2_TARGET = 1
.endif
.ifndef BLOCK_SELECT_TEST2_EXPECTED_ID
    BLOCK_SELECT_TEST2_EXPECTED_ID = $10
.endif
.ifndef BLOCK_SELECT_TEST3_TARGET
    BLOCK_SELECT_TEST3_TARGET = BLOCK_SELECT_TEST1_TARGET
.endif
.ifndef BLOCK_SELECT_TEST3_EXPECTED_ID
    BLOCK_SELECT_TEST3_EXPECTED_ID = BLOCK_SELECT_TEST1_EXPECTED_ID
.endif
.ifndef BLOCK_SELECT_TEST4_TARGET
    BLOCK_SELECT_TEST4_TARGET = 2
.endif
.ifndef BLOCK_SELECT_TEST4_EXPECTED_ID
    BLOCK_SELECT_TEST4_EXPECTED_ID = $20
.endif
.ifndef BLOCK_SELECT_SIG16_ID
    BLOCK_SELECT_SIG16_ID = $10
.endif
.ifndef BLOCK_SELECT_SIG32_ID
    BLOCK_SELECT_SIG32_ID = $20
.endif
.ifndef BLOCK_SELECT_RESTORE_TARGET
    BLOCK_SELECT_RESTORE_TARGET = BLOCK_SELECT_TEST1_TARGET
.endif

; Trampoline lives at $0380 (above $0300 used by test_prg_banking)
BLK_TRAMP_ADDR = $0380
BLK_TRAMP_SIZE = blk_tramp_end - blk_tramp_code

.segment "CODE"

; Copy block-switch trampoline to RAM
.proc install_blk_trampoline
    ldx #0
@copy:
    lda blk_tramp_code, x
    sta BLK_TRAMP_ADDR, x
    inx
    cpx #BLK_TRAMP_SIZE
    bne @copy
    rts
.endproc

; Call the block-switch trampoline
; Input: A = target block number (0-7)
; Output: blk_result[0..3] = signature from $8000 in target block
.proc call_blk_trampoline
    sta blk_target
    jmp BLK_TRAMP_ADDR
.endproc

; This trampoline runs from RAM.
; It switches to the target block, reads the PRG signature at $8000,
; switches back to block 0, and returns.
blk_tramp_code:
    ; Switch to target block.
    .ifdef CUSTOM_BLOCK_SELECT_SEQUENCE
    lda blk_target
    program_block_select_from_a
    .else
    .ifdef BLOCK_SELECT_NEEDS_PRG_RAM_ENABLE
    enable_prg_ram
    .endif
    lda blk_target
    ora #BLOCK_SELECT_WRITE_OR_MASK
    sta BLOCK_SELECT_WRITE_ADDR
    .endif

    ; Select PRG R6 = bank 0 within this block
    lda #6
    sta $8000
    lda #0
    sta $8001

    ; Read 4 signature bytes
    lda $8000
    sta blk_result
    lda $8001
    sta blk_result+1
    lda $8002
    sta blk_result+2
    lda $8003
    sta blk_result+3

    ; Switch back to block 0
    .ifdef CUSTOM_BLOCK_SELECT_SEQUENCE
    lda #BLOCK_SELECT_RESTORE_TARGET
    program_block_select_from_a
    .else
    .ifdef BLOCK_SELECT_NEEDS_PRG_RAM_ENABLE
    enable_prg_ram
    .endif
    lda #BLOCK_SELECT_RESTORE_VALUE
    sta BLOCK_SELECT_WRITE_ADDR
    .endif

    ; Re-select PRG R6 = bank 0 in block 0 (restore state)
    lda #6
    sta $8000
    lda #0
    sta $8001

    rts
blk_tramp_end:

.proc run_tests
    ; Install the block-switch trampoline to RAM
    jsr install_blk_trampoline

    ; ========================================
    ; Test 1: Block 0 — read bank 0 signature
    ; ========================================
    start_test 1, "Block 0 sig"

    ; Use trampoline to read block 0 bank 0
    lda #BLOCK_SELECT_TEST1_TARGET
    jsr call_blk_trampoline

    ; Verify signature
    lda blk_result
    assert_a_eq $A5
    lda blk_result+1        ; Bank ID — should be 0 in block 0
    assert_a_eq BLOCK_SELECT_TEST1_EXPECTED_ID
    pass_test

    ; ========================================
    ; Test 2: Block 1 — different content
    ; ========================================
    start_test 2, "Block 1 sig"

    lda #BLOCK_SELECT_TEST2_TARGET
    jsr call_blk_trampoline

    lda blk_result
    assert_a_eq $A5
    lda blk_result+1
    assert_a_eq BLOCK_SELECT_TEST2_EXPECTED_ID
    pass_test

    ; ========================================
    ; Test 3: Return to block 0
    ; ========================================
    start_test 3, "Back to blk0"

    lda #BLOCK_SELECT_TEST3_TARGET
    jsr call_blk_trampoline

    lda blk_result
    assert_a_eq $A5
    lda blk_result+1
    assert_a_eq BLOCK_SELECT_TEST3_EXPECTED_ID
    pass_test

    ; ========================================
    ; Test 4: Block 2 — verify third block
    ; ========================================
    .ifndef SKIP_BLOCK_SELECT_TEST_4
    start_test 4, "Block 2 sig"

    lda #BLOCK_SELECT_TEST4_TARGET
    jsr call_blk_trampoline

    lda blk_result
    assert_a_eq $A5
    lda blk_result+1
    assert_a_eq BLOCK_SELECT_TEST4_EXPECTED_ID
    pass_test
    .endif

    rts
.endproc

; Export unique name for combined ROM builds
run_block_select = run_tests
.export run_block_select

; ============================================================
; Block PRG bank signatures
; Block 0 bank 0 needs PRG_SIG0 for standalone builds.
; Block 1 and block 2 need their own first-bank signatures.
; ============================================================
.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A            ; Block 0 bank 0 (physical bank 0)
.segment "PRG_SIG16"
    .byte $A5, BLOCK_SELECT_SIG16_ID, (BLOCK_SELECT_SIG16_ID ^ $FF), $5A
.ifndef SKIP_BLOCK_SELECT_TEST_4
.segment "PRG_SIG32"
    .byte $A5, BLOCK_SELECT_SIG32_ID, (BLOCK_SELECT_SIG32_ID ^ $FF), $5A
.endif

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
