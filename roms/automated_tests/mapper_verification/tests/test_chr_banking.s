; test_chr_banking.s — CHR Bank Switching Verification
;
; Tests that CHR bank switching works correctly by reading signature
; bytes from each CHR bank via PPUDATA.
;
; Each CHR bank has a unique signature: $B6, bank_num, ~bank_num, $6B
; at the start of the bank (tile 0, byte 0-3).
;
; The test selects each bank, reads CHR data via $2006/$2007,
; and verifies the signature.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef CHR_1K_LINEAR
    CHR_1K_LINEAR = 0
.endif

.if CHR_1K_LINEAR
    CHR_R2_HI = $08
    CHR_R3_HI = $0C
    CHR_R4_HI = $10
.else
    CHR_R2_HI = $10
    CHR_R3_HI = $14
    CHR_R4_HI = $18
.endif

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "CHR Banking m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
chr_read_val: .res 1

.segment "CODE"

; Read a byte from CHR at PPU address (A=hi, X=lo)
; Returns value in A
; Must be called during VBlank or with rendering disabled
.proc read_chr_byte
    bit PPUSTATUS           ; Reset latch
    sta PPUADDR
    stx PPUADDR
    lda PPUDATA             ; Dummy read (buffered)
    lda PPUDATA             ; Actual data
    rts
.endproc

; Disable rendering for safe PPU access (updates shadow)
.proc disable_rendering
    lda #0
    sta PPUMASK
    sta ppumask_shadow
    rts
.endproc

; Re-enable rendering (updates shadow)
.proc enable_rendering
    lda #%00001010
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

.proc run_tests
    .if CHR_BANK_SIZE = 8
        ; --- 8KB CHR banking (CNROM) ---
        ; Whole pattern table switches at once

        start_test 1, "CHR Bank 0"
        jsr disable_rendering
        select_chr_bank 0, 0
        ; Read first byte of CHR bank 0 at PPU $0000
        lda #$00
        ldx #$00
        jsr read_chr_byte
        assert_a_eq $B6         ; Signature marker
        pass_test

        start_test 2, "CHR B0 id"
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 0           ; Bank 0
        pass_test

        start_test 3, "CHR Bank 1"
        select_chr_bank 0, 1
        lda #$00
        ldx #$00
        jsr read_chr_byte
        assert_a_eq $B6
        pass_test

        start_test 4, "CHR B1 id"
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 1
        pass_test

        start_test 5, "CHR Bank 2"
        select_chr_bank 0, 2
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 2
        pass_test

        start_test 6, "CHR Bank 3"
        select_chr_bank 0, 3
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 3
        pass_test

        ; Switch back to bank 0 for console (has ASCII font)
        select_chr_bank 0, 0
        jsr enable_rendering

    .elseif CHR_BANK_SIZE = 4
        ; --- 4KB CHR banking (MMC1) ---
        ; Two 4KB slots: $0000-$0FFF and $1000-$1FFF

        ; Initialize MMC1 to 4KB CHR mode + PRG mode 3
        .if MAPPER_NUM = 1
            mmc1_reset
            ; Control = mirroring V(2) | PRG mode 3 | CHR 4KB = %11110
            mmc1_write_reg MMC1_CONTROL, %11110
        .endif

        start_test 1, "CHR0 Bank 0"
        jsr disable_rendering
        select_chr_bank 0, 0
        lda #$00
        ldx #$00
        jsr read_chr_byte
        assert_a_eq $B6
        pass_test

        start_test 2, "CHR0 B0 id"
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 0
        pass_test

        start_test 3, "CHR0 Bank 1"
        select_chr_bank 0, 1
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 1
        pass_test

        start_test 4, "CHR1 Bank 2"
        select_chr_bank 1, 2
        lda #$10               ; $1000 for second 4KB slot
        ldx #$00
        jsr read_chr_byte
        assert_a_eq $B6
        pass_test

        start_test 5, "CHR1 B2 id"
        lda #$10
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 2
        pass_test

        start_test 6, "CHR1 Bank 3"
        select_chr_bank 1, 3
        lda #$10
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 3
        pass_test

        ; Restore bank 0 for console
        select_chr_bank 0, 0

        ; === MMC1 CHR 8KB Mode ===
        .if MAPPER_NUM = 1
        start_test 7, "CHR 8KB"
        ; Switch to 8KB CHR mode: control bit 4 = 0
        mmc1_write_reg MMC1_CONTROL, %01110
        ; Select 8KB bank 1 (value 2 → 4KB banks 2+3)
        mmc1_write_reg MMC1_CHR0, 2
        ; $0000 should now show 4KB bank 2 (lower half of 8KB bank 1)
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 2
        pass_test

        start_test 8, "CHR 8KB hi"
        ; $1000 should show 4KB bank 3 (upper half of 8KB bank 1)
        lda #$10
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 3
        ; Restore 4KB CHR mode + bank 0
        mmc1_write_reg MMC1_CONTROL, %11110
        mmc1_write_reg MMC1_CHR0, 0
        pass_test
        .endif

        jsr enable_rendering

    .elseif CHR_BANK_SIZE = 1
        ; --- 1KB CHR banking (MMC3) ---
        ; 8 slots: R0-R5 in bank select register

        start_test 1, "CHR R2 B0"
        jsr disable_rendering
        select_chr_bank 2, 0
        lda #CHR_R2_HI
        ldx #$00
        jsr read_chr_byte
        assert_a_eq $B6
        pass_test

        start_test 2, "CHR R2 id"
        lda #CHR_R2_HI
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 0
        pass_test

        start_test 3, "CHR R2 B1"
        select_chr_bank 2, 1
        lda #CHR_R2_HI
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 1
        pass_test

        start_test 4, "CHR R3 B2"
        select_chr_bank 3, 2
        lda #CHR_R3_HI
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 2
        pass_test

        start_test 5, "CHR R4 B3"
        select_chr_bank 4, 3
        lda #CHR_R4_HI
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 3
        pass_test

        start_test 6, "CHR R0 B4"
        select_chr_bank 0, 4    ; R0: 2KB at $0000
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 4
        pass_test

        ; === MMC3 CHR A12 Inversion ===
        .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14 .or MAPPER_NUM = 64
        start_test 7, "CHR invert"
        ; Set CHR A12 inversion: bit 7 of bank select
        ; Inverted: R2→$0000, R3→$0400, R4→$0800, R5→$0C00
        ;           R0→$1000, R1→$1800
        lda #(2 | $80)          ; R2 + inversion bit
        sta MMC3_BANK_SELECT
        lda #0                  ; R2 = bank 0
        sta MMC3_BANK_DATA
        ; $0000 = R2 = bank 0
        lda #$00
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 0
        pass_test

        start_test 8, "Invert R0"
        ; R0 now maps to $1000 (instead of $0000)
        lda #(0 | $80)          ; R0 + inversion bit
        sta MMC3_BANK_SELECT
        lda #4                  ; R0 = bank 4
        sta MMC3_BANK_DATA
        ; $1000 = R0 = bank 4
        lda #$10
        ldx #$01
        jsr read_chr_byte
        assert_a_eq 4
        pass_test
        .endif

        ; Restore font bank before enabling rendering
        init_chr_font
        jsr enable_rendering
    .endif

    rts
.endproc

; Export unique name for combined ROM builds
run_chr_banking = run_tests
.export run_chr_banking

; ============================================================
; CHR Bank signature data
; Each CHR bank gets: $B6, bank_num, ~bank_num, $6B
; at offset 0 of each CHR bank
; ============================================================

; Number of CHR banks at configured granularity
.if CHR_BANK_SIZE > 0
    CHR_NUM_BANKS = CHR_ROM_8K * (8 / CHR_BANK_SIZE)
.else
    CHR_NUM_BANKS = 0
.endif

.segment "CHR_SIG0"
    .byte $B6, 0, $FF, $6B

.segment "CHR_SIG1"
    .byte $B6, 1, $FE, $6B

.segment "CHR_SIG2"
    .byte $B6, 2, $FD, $6B

.segment "CHR_SIG3"
    .byte $B6, 3, $FC, $6B

.if CHR_NUM_BANKS > 4
.segment "CHR_SIG4"
    .byte $B6, 4, $FB, $6B

.segment "CHR_SIG5"
    .byte $B6, 5, $FA, $6B

.segment "CHR_SIG6"
    .byte $B6, 6, $F9, $6B

.segment "CHR_SIG7"
    .byte $B6, 7, $F8, $6B
.endif

.if CHR_NUM_BANKS > 8
.segment "CHR_SIG8"
    .byte $B6, 8, $F7, $6B

.segment "CHR_SIG9"
    .byte $B6, 9, $F6, $6B

.segment "CHR_SIG10"
    .byte $B6, 10, $F5, $6B

.segment "CHR_SIG11"
    .byte $B6, 11, $F4, $6B

.segment "CHR_SIG12"
    .byte $B6, 12, $F3, $6B

.segment "CHR_SIG13"
    .byte $B6, 13, $F2, $6B

.segment "CHR_SIG14"
    .byte $B6, 14, $F1, $6B

.segment "CHR_SIG15"
    .byte $B6, 15, $F0, $6B
.endif

; Bus conflict lookup table (for mappers that need it)
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
; NES 2.0 Header
.include "nes20_header.inc"
nes20_header

; ASCII font
.if CHR_ROM_8K > 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
.endif ; COMBINED
