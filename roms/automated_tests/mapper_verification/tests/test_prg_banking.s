; test_prg_banking.s — PRG Bank Switching Verification
;
; Tests that PRG bank switching works correctly by reading signature
; bytes embedded in each bank.
;
; Bank N contains signature: $A5, N, ~N, $5A at the start of the bank.
; The test selects each bank and verifies the signature.
;
; For mappers with a fixed bank, also verifies the fixed bank remains stable.
; For MMC1 submapper 5 (FIXED_PRG), verifies that bank writes are ignored.

.include "test_macros.inc"

; Include the mapper-specific definitions
.include "mapper_config.inc"

.ifndef SKIP_PRG_SIG1
    SKIP_PRG_SIG1 = 0
.endif

; Signature read address: start of banked window
.ifndef BANK_WINDOW_OVERRIDE
    .if PRG_BANK_SIZE = 4
        BANK_WINDOW = $8000         ; 4KB: $8000-$8FFF (slot 0)
        FIXED_WINDOW = $F000        ; Fixed bank at $F000-$FFFF (slot 7)
    .elseif PRG_BANK_SIZE = 8
        BANK_WINDOW = $8000         ; 8KB: $8000-$9FFF
        FIXED_WINDOW = $E000        ; Fixed bank at $E000-$FFFF
    .elseif PRG_BANK_SIZE = 16
        BANK_WINDOW = $8000         ; 16KB: $8000-$BFFF
        FIXED_WINDOW = $C000        ; Fixed bank at $C000-$FFFF
    .elseif PRG_BANK_SIZE = 32
        BANK_WINDOW = $8000         ; 32KB: $8000-$FFFF (whole space)
        FIXED_WINDOW = $8000        ; No separate fixed window
    .endif
.else
    BANK_WINDOW = BANK_WINDOW_OVERRIDE
    .ifndef FIXED_WINDOW_OVERRIDE
        FIXED_WINDOW = $E000
    .else
        FIXED_WINDOW = FIXED_WINDOW_OVERRIDE
    .endif
.endif

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "PRG Banking m"
    ; Mapper number as ASCII
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

.proc run_tests
.if .not HAS_PRG_BANKING
    ; === MMC1 Submapper 5: Negative test ===
    ; Verify that PRG banking writes are IGNORED
    start_test 1, "PRG fixed"

    ; Read initial signature at $8000
    lda BANK_WINDOW
    sta TEST_TEMP              ; Save initial value
    ; Try to switch PRG bank
    select_prg_bank 0, 1
    ; Read again — should be unchanged
    lda BANK_WINDOW
    cmp TEST_TEMP
    beq @fixed_ok
    ; Failed: bank changed when it shouldn't have
    tax
    lda TEST_TEMP
    fail_test
@fixed_ok:
    pass_test

    start_test 2, "PRG fixed 2"
    select_prg_bank 0, 2
    lda BANK_WINDOW
    cmp TEST_TEMP
    beq @fixed_ok2
    tax
    lda TEST_TEMP
    fail_test
@fixed_ok2:
    pass_test
.else
    ; === Normal PRG banking tests ===

    .if PRG_BANK_SIZE = 32
        ; --- 32KB banking (AxROM, GxROM) ---
        ; Code lives in bank 0. To test other banks, we use a
        ; RAM-based trampoline that switches bank, reads the signature,
        ; stores it, then switches back to bank 0.

        ; First, install the trampoline into RAM at TRAMPOLINE_ADDR
        jsr install_trampoline

        ; Test 1: Bank 1 signature
        start_test 1, "Bank 1 sig"
        lda #1
        jsr call_trampoline
        lda tramp_result
        assert_a_eq $A5
        pass_test

        start_test 2, "Bank 1 id"
        lda tramp_result+1
        assert_a_eq 1
        pass_test

        ; Test 3: Bank 2 signature
        .if ::PRG_ROM_16K >= 6
        start_test 3, "Bank 2 sig"
        lda #2
        jsr call_trampoline
        lda tramp_result
        assert_a_eq $A5
        pass_test

        start_test 4, "Bank 2 id"
        lda tramp_result+1
        assert_a_eq 2
        pass_test
        .endif

        ; Test 5: Bank 3 signature
        .if ::PRG_ROM_16K >= 8
        start_test 5, "Bank 3 sig"
        lda #3
        jsr call_trampoline
        lda tramp_result
        assert_a_eq $A5
        pass_test

        start_test 6, "Bank 3 id"
        lda tramp_result+1
        assert_a_eq 3
        pass_test
        .endif

    .elseif PRG_BANK_SIZE = 4
        ; --- 4KB banking (NSF mapper 31) ---
        ; Single 4KB slot at $8000-$8FFF; $F000-$FFFF fixed to last bank

        .ifndef TEST_BANK_B
            TEST_BANK_B = 1
        .endif
        .ifndef TEST_BANK_C
            TEST_BANK_C = 2
        .endif

        start_test 1, "Bank 0 sig"
        select_prg_bank 0, 0
        lda BANK_WINDOW
        assert_a_eq $A5
        pass_test

        start_test 2, "Bank 0 id"
        lda BANK_WINDOW + 1
        assert_a_eq 0
        pass_test

        start_test 3, "Bank B sig"
        select_prg_bank 0, TEST_BANK_B
        lda BANK_WINDOW
        assert_a_eq $A5
        pass_test

        start_test 4, "Bank B id"
        lda BANK_WINDOW + 1
        assert_a_eq TEST_BANK_B
        pass_test

        start_test 5, "Bank C"
        select_prg_bank 0, TEST_BANK_C
        lda BANK_WINDOW + 1
        assert_a_eq TEST_BANK_C
        pass_test

        ; Verify fixed slot 7 ($F000-$FFFF) is unaffected by slot 0 switching
        start_test 6, "Fixed slot7"
        lda $FFFC               ; Read reset vector low byte
        sta TEST_TEMP
        select_prg_bank 0, 0    ; Switch slot 0 — should not disturb slot 7
        lda $FFFC               ; Should be unchanged
        cmp TEST_TEMP
        beq :+
        tax
        lda TEST_TEMP
        fail_test
:       pass_test

        start_test 7, "Fixed stable"
        select_prg_bank 0, TEST_BANK_C
        lda $FFFC
        cmp TEST_TEMP
        beq :+
        tax
        lda TEST_TEMP
        fail_test
:       pass_test

    .elseif PRG_BANK_SIZE = 16
        ; --- 16KB banking (UxROM, MMC1 mode 3) ---
        ; Switchable bank at $8000-$BFFF, fixed bank at $C000-$FFFF

        ; Configurable test bank numbers (mapper 15 skips bank 1 = bootstrap)
        .ifndef TEST_BANK_B
            TEST_BANK_B = 1
        .endif
        .ifndef TEST_BANK_C
            TEST_BANK_C = 2
        .endif

        start_test 1, "Bank 0 sig"
        select_prg_bank 0, 0
        lda BANK_WINDOW
        assert_a_eq $A5
        pass_test

        start_test 2, "Bank 0 id"
        lda BANK_WINDOW + 1
        assert_a_eq 0
        pass_test

        start_test 3, "Bank B sig"
        select_prg_bank 0, TEST_BANK_B
        lda BANK_WINDOW
        assert_a_eq $A5
        pass_test

        start_test 4, "Bank B id"
        lda BANK_WINDOW + 1
        assert_a_eq TEST_BANK_B
        pass_test

        start_test 5, "Bank C"
        select_prg_bank 0, TEST_BANK_C
        lda BANK_WINDOW + 1
        assert_a_eq TEST_BANK_C
        pass_test

        ; Verify fixed bank stays stable while switching variable bank
        ; We check the reset vector at $FFFC-$FFFD (always in fixed bank)
        start_test 6, "Fixed bank"
        lda $FFFC               ; Read reset vector low byte
        sta TEST_TEMP            ; Save it
        select_prg_bank 0, 0    ; Switch variable bank
        lda $FFFC               ; Read reset vector again
        cmp TEST_TEMP
        beq :+
        tax
        lda TEST_TEMP
        fail_test
:       pass_test

        start_test 7, "Fixed stable"
        select_prg_bank 0, 2    ; Switch to different variable bank
        lda $FFFC               ; Reset vector should still be the same
        cmp TEST_TEMP
        beq :+
        tax
        lda TEST_TEMP
        fail_test
:       pass_test

        ; === MMC1 Shift Register Reset ===
        .if MAPPER_NUM = 1
        start_test 8, "Shift reset"
        ; Select bank 0 first
        select_prg_bank 0, 0
        lda BANK_WINDOW + 1
        assert_a_eq 0
        ; Start partial write (3 of 5 bits) — incomplete load
        lda #1
        sta MMC1_PRG
        sta MMC1_PRG
        sta MMC1_PRG
        ; Reset shift register with bit 7 write
        mmc1_reset
        ; Normal bank switch should work after reset
        select_prg_bank 0, 1
        lda BANK_WINDOW + 1
        assert_a_eq 1
        pass_test
        .endif

    .elseif PRG_BANK_SIZE = 8
        ; --- 8KB banking (MMC3) ---
        ; Switchable banks at $8000-$9FFF (R6) and $A000-$BFFF (R7)
        ; Fixed banks at $C000-$DFFF and $E000-$FFFF

        start_test 1, "R6 Bank 0"
        select_prg_bank 0, 0
        lda BANK_WINDOW
        assert_a_eq $A5
        pass_test

        start_test 2, "R6 Bank 0 id"
        lda BANK_WINDOW + 1
        assert_a_eq 0
        pass_test

        start_test 3, "R6 Bank 1"
        select_prg_bank 0, 1
        lda BANK_WINDOW + 1
        assert_a_eq 1
        pass_test

        start_test 4, "R6 Bank 2"
        select_prg_bank 0, 2
        lda BANK_WINDOW + 1
        assert_a_eq 2
        pass_test

        ; Optional second 8KB slot test for mappers with two switchable windows.
        .ifndef SKIP_SECOND_PRG_8K_SLOT_TESTS
        start_test 5, "R7 Bank 0"
        select_prg_bank 1, 0
        lda $A000               ; R7 window
        assert_a_eq $A5
        pass_test

        start_test 6, "R7 Bank 1"
        select_prg_bank 1, 1
        lda $A001
        assert_a_eq 1
        pass_test
        .endif

        ; Verify fixed bank stability via reset vector
        start_test 7, "Fixed $E000"
        lda $FFFC               ; Read reset vector low byte
        sta TEST_TEMP
        select_prg_bank 0, 3    ; Switch variable bank
        lda $FFFC               ; Reset vector should be unchanged
        cmp TEST_TEMP
        beq :+
        tax
        lda TEST_TEMP
        fail_test
:       pass_test

        ; === MMC3 PRG Mode Bit (bit 6 of $8000) ===
        .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14
        start_test 8, "PRG mode 1"
        ; Mode 1: bit 6 → R6 at $C000, $8000 = 2nd-to-last bank
        lda #(6 | $40)          ; R6 + PRG mode 1
        sta MMC3_BANK_SELECT
        lda #0                  ; R6 = bank 0
        sta MMC3_BANK_DATA
        ; $C000 should now be bank 0 (R6 in mode 1)
        lda $C001
        assert_a_eq 0
        pass_test

        start_test 9, "Mode1 $8000"
        ; $8000 should be second-to-last bank in mode 1
        ; For N 8KB banks, second-to-last = N-2 = PRG_ROM_16K*2-2
        lda $8001
        assert_a_eq (PRG_ROM_16K * 2 - 2)
        ; Restore mode 0
        lda #6                  ; R6, PRG mode 0
        sta MMC3_BANK_SELECT
        lda #0
        sta MMC3_BANK_DATA
        pass_test
        .endif

        ; === Irem H3001 PRG Mode Bit ($9000 bit 7) ===
        .if MAPPER_NUM = 65
        start_test 8, "PRG mode 1"
        ; Mode 1: $9000 bit 7 → reg0 at $C000, $8000 = second-to-last
        lda #$80                ; PRG mode 1
        sta H3001_PRG_MODE
        lda #0                  ; reg0 = bank 0
        sta H3001_PRG0
        ; $C000 should now be bank 0 (reg0 in mode 1)
        lda $C001
        assert_a_eq 0
        pass_test

        start_test 9, "Mode1 $8000"
        ; $8000 should be second-to-last bank (N-2 = PRG_ROM_16K*2-2)
        lda $8001
        assert_a_eq (PRG_ROM_16K * 2 - 2)
        ; Restore mode 0
        lda #$00                ; PRG mode 0
        sta H3001_PRG_MODE
        lda #0
        sta H3001_PRG0
        pass_test
        .endif

        ; === VRC1 Third PRG Slot ($C000) ===
        .if MAPPER_NUM = 75
        start_test 8, "Slot2 Bank 0"
        select_prg_bank 2, 0
        lda $C000
        assert_a_eq $A5
        pass_test

        start_test 9, "Slot2 Bank 1"
        select_prg_bank 2, 1
        lda $C001
        assert_a_eq 1
        pass_test
        .endif

        ; === Action 53 PRG Mode Tests ===
        .if MAPPER_NUM = 28

        ; Mode 0 (32KB banking): inner bank 0 → $8000=bank 0, $C000=bank 1 (code)
        start_test 8, "Mode0 32K"
        lda #$01
        sta $5000               ; Select inner bank register
        lda #0
        sta $BFFF               ; Inner bank = 0
        lda #$80
        sta $5000               ; Select mode register
        lda #$13                ; Mode 0 (32KB), H mirror, 64KB outer
        sta $BFFF
        ; $8000 should be bank 0 of outer bank
        lda $8000
        assert_a_eq $A5
        pass_test

        start_test 9, "Mode0 B0 id"
        lda $8001
        assert_a_eq 0
        pass_test

        ; Mode 2 (Fixed $8000): inner bank 1 → $8000=bank 0 (fixed), $C000=bank 1 (code)
        start_test 10, "Mode2 fix"
        lda #$01
        sta $5000               ; Select inner bank register
        lda #1
        sta $BFFF               ; Inner bank = 1 (keeps code at $C000)
        lda #$80
        sta $5000               ; Select mode register
        lda #$1B                ; Mode 2 (fixed $8000), H mirror, 64KB outer
        sta $BFFF
        ; $8000 should be fixed to bank 0 (first bank of outer bank)
        lda $8000
        assert_a_eq $A5
        pass_test

        start_test 11, "Mode2 B0 id"
        lda $8001
        assert_a_eq 0
        pass_test

        ; Restore mode 3 (fixed $C000, switchable $8000) for subsequent tests
        lda #$80
        sta $5000
        lda #$1F                ; Mode 3, H mirror, 64KB outer
        sta $BFFF
        lda #$01
        sta $5000               ; Select inner bank register
        lda #0
        sta $BFFF               ; Inner bank = 0

        .endif

    .endif
.endif
    rts
.endproc

; Export unique name for combined ROM builds
run_prg_banking = run_tests
.export run_prg_banking
; When we switch the entire 32KB PRG space, we lose our code.
; This routine copies a small bank-test snippet to RAM, calls it
; from RAM, and it switches back to bank 0 before returning.
; ============================================================
.if PRG_BANK_SIZE = 32

.segment "BSS"
tramp_result: .res 4         ; Signature bytes read from target bank

; We reserve space in BSS for the trampoline code
TRAMPOLINE_ADDR = $0300      ; Address in RAM for trampoline
TRAMPOLINE_SIZE = trampoline_end - trampoline_code

.segment "CODE"

; Copy trampoline to RAM
.proc install_trampoline
    ldx #0
@copy:
    lda trampoline_code, x
    sta TRAMPOLINE_ADDR, x
    inx
    cpx #TRAMPOLINE_SIZE
    bne @copy
    rts
.endproc

; Call the RAM trampoline with bank number in A
.proc call_trampoline
    ; Store bank number at a known location in RAM for the trampoline
    sta tramp_result        ; Reuse as temp: bank number
    jmp TRAMPOLINE_ADDR     ; Jump to RAM code (it will RTS back)
.endproc

; This code is copied to RAM and executed there.
; Input: tramp_result = bank number to read
; Output: tramp_result[0..3] = 4 signature bytes from target bank
; NOTE: Uses TRAMPOLINE_BANK_ADDR for bank select writes.
; Default $FFF0 is in the fill region ($FF) for bus-conflict-safe writes.
; Mappers with register at $7xxx (NINA-001, mapper 38) override this.
.ifndef TRAMPOLINE_BANK_SHIFT
    TRAMPOLINE_BANK_SHIFT = 0   ; Default: bank number in low bits
.endif
.ifndef TRAMPOLINE_BANK_ADDR
    TRAMPOLINE_BANK_ADDR = $FFF0 ; Default: bus-conflict-safe address
.endif
.ifndef TRAMPOLINE_BANK_BY_ADDRESS
    TRAMPOLINE_BANK_BY_ADDRESS = 0
.endif
.ifndef TRAMPOLINE_BANK_BY_SPLIT_OUTER_INNER
    TRAMPOLINE_BANK_BY_SPLIT_OUTER_INNER = 0
.endif
.ifndef TRAMPOLINE_OUTER_BANK_ADDR
    TRAMPOLINE_OUTER_BANK_ADDR = $6000
.endif
.ifndef TRAMPOLINE_INNER_BANK_ADDR
    TRAMPOLINE_INNER_BANK_ADDR = $FFF0
.endif
trampoline_code:
    lda tramp_result        ; Bank number
    .if TRAMPOLINE_BANK_BY_SPLIT_OUTER_INNER
    tay
    tya
    lsr a
    sta TRAMPOLINE_OUTER_BANK_ADDR ; Outer bank bits
    tya
    and #$01
    sta TRAMPOLINE_INNER_BANK_ADDR ; Inner bank bit
    .elseif TRAMPOLINE_BANK_BY_ADDRESS
    tay
    lda #0
    sta TRAMPOLINE_BANK_ADDR, y ; Select target bank via address bits
    .else
    .repeat TRAMPOLINE_BANK_SHIFT
    asl a
    .endrepeat
    sta TRAMPOLINE_BANK_ADDR ; Select target bank
    .endif
    ; Read 4 signature bytes from $8000
    lda $8000
    sta tramp_result
    lda $8001
    sta tramp_result+1
    lda $8002
    sta tramp_result+2
    lda $8003
    sta tramp_result+3
    ; Switch back to bank 0 (where code lives)
    .if TRAMPOLINE_BANK_BY_SPLIT_OUTER_INNER
    lda #0
    sta TRAMPOLINE_OUTER_BANK_ADDR
    sta TRAMPOLINE_INNER_BANK_ADDR
    .else
    lda #0
    sta TRAMPOLINE_BANK_ADDR
    .endif
    rts
trampoline_end:

.export tramp_result

.endif ; PRG_BANK_SIZE = 32

; ============================================================
; Bank signature data
; Each PRG bank gets a 4-byte signature: $A5, bank_num, ~bank_num, $5A
; Only switchable banks get signatures (not the code/fixed bank)
; Only emitted for mappers that have PRG banking
; ============================================================

.if HAS_PRG_BANKING

; For 32KB banking (AxROM): code is in bank 0, sigs in banks 1-3
; For other sizes: bank 0 is switchable and gets a signature
.if PRG_BANK_SIZE <> 32
.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A
.endif

; Bank 1 sig: skip for mapper 15 (bank 1 = bootstrap)
.if MAPPER_NUM <> 15
    .if .not SKIP_PRG_SIG1
.segment "PRG_SIG1"
    .byte $A5, 1, $FE, $5A
    .endif
.endif

.if PRG_BANK_SIZE = 32
    .if PRG_ROM_16K >= 6
.segment "PRG_SIG2"
    .byte $A5, 2, $FD, $5A
    .endif
.else
.segment "PRG_SIG2"
    .byte $A5, 2, $FD, $5A
.endif

; Banks 3-7: only emit if the mapper has enough PRG banks
; The code/fixed bank doesn't get a signature segment
.if PRG_BANK_SIZE = 32
    ; 32KB banking: code is in bank 0, sigs in banks 1-3 (if they exist)
    .if PRG_ROM_16K >= 8
    .segment "PRG_SIG3"
        .byte $A5, 3, $FC, $5A
    .endif
.elseif PRG_BANK_SIZE = 8
    ; MMC3/MMC3-clone: 8KB banks. Banks 0 to N-3 are switchable,
    ; N-2 is fixed second-to-last, N-1 is code bank.
    .segment "PRG_SIG3"
        .byte $A5, 3, $FC, $5A
    .segment "PRG_SIG4"
        .byte $A5, 4, $FB, $5A
    .segment "PRG_SIG5"
        .byte $A5, 5, $FA, $5A
    .segment "PRG_SIG6"
        .byte $A5, 6, $F9, $5A
    ; Banks 7+ only if more than 8 banks
    .if PRG_ROM_16K > 4
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
    ; PRG_SIG14 = fixed second-to-last bank (needs sig for mode 1 test)
    .segment "PRG_SIG14"
        .byte $A5, 14, $F1, $5A
    .endif
.elseif PRG_BANK_SIZE = 16
    .if PRG_BANK_COUNT > 3
        ; UxROM with 8 banks: emit sigs 3-6 (7 is code bank)
        .segment "PRG_SIG3"
            .byte $A5, 3, $FC, $5A
        .segment "PRG_SIG4"
            .byte $A5, 4, $FB, $5A
        .segment "PRG_SIG5"
            .byte $A5, 5, $FA, $5A
        .segment "PRG_SIG6"
            .byte $A5, 6, $F9, $5A
    .endif
    ; MMC1 with 4 banks: PRG_SIG3 is code bank — skip
.endif

.endif ; HAS_PRG_BANKING

; ============================================================
; Bus conflict lookup table (for mappers that need it)
; Each byte at offset N contains the value N, so writing N to
; address (bank_table + N) results in N AND N = N (safe write)
; ============================================================
.if HAS_BUS_CONFLICTS
.ifndef COMBINED
.segment "RODATA"
.export bank_table
bank_table:
    .repeat 16, i
        .byte i
    .endrepeat
.else
    .import bank_table
.endif
.endif

.ifndef COMBINED
; ============================================================
; NES 2.0 Header
; ============================================================
.include "nes20_header.inc"
nes20_header

; ============================================================
; CHR data: ASCII font for console output
; ============================================================
.if CHR_ROM_8K > 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
.endif ; COMBINED
