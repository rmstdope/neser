; test_nametable.s — Nametable Mirroring Verification
;
; Tests that nametable mirroring configuration works correctly.
; For mappers with configurable mirroring:
;   - Writes unique value to nametable A ($2000)
;   - Reads from nametables B ($2400), C ($2800), D ($2C00)
;   - Verifies which nametables mirror which based on the mode
;
; Mirroring modes:
;   Horizontal: $2000=$2400, $2800=$2C00
;   Vertical:   $2000=$2800, $2400=$2C00
;   1-Screen A: $2000=$2400=$2800=$2C00
;   1-Screen B: all mirror $2400

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Nametable m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
nt_read: .res 1

.segment "CODE"

; Write a value to a specific nametable offset
; A = value, X = high byte of NT address ($20/$24/$28/$2C), Y = low byte offset
.proc write_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    sta PPUDATA
    rts
.endproc

; Read a value from a specific nametable offset
; X = high byte of NT address, Y = low byte offset
; Returns value in A
.proc read_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Actual value
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
    jsr disable_rendering

    ; Use offset $0F in each nametable for test writes (avoids console area)
    ; Write unique values to distinguish nametables
    ;
    ; IMPORTANT: Mirroring must be restored to vertical before any
    ; console output (start_test/pass_test) to keep nametable A consistent.

    ; ========================================
    ; Test Vertical Mirroring
    ; ========================================
    .if MAPPER_NUM = 7
        ; AxROM only supports single-screen, skip V/H test
    .else
        start_test 1, "Vert mirror"
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8 .or MAPPER_NUM = 15
            set_mirroring 2     ; MMC1: 2 = vertical
        .else
            set_mirroring 0     ; MMC3/generic: 0 = vertical
        .endif

        ; Write $AA to NT A ($2000)
        lda #$AA
        ldx #$20
        ldy #$0F
        jsr write_nt

        ; Write $BB to NT B ($2400)
        lda #$BB
        ldx #$24
        ldy #$0F
        jsr write_nt

        ; In vertical: $2000 mirrors $2800, $2400 mirrors $2C00
        ; Read from $2800 — should equal $AA
        ldx #$28
        ldy #$0F
        jsr read_nt
        assert_a_eq $AA
        pass_test

        start_test 2, "Vert mirror2"
        ; Read from $2C00 — should equal $BB
        ldx #$2C
        ldy #$0F
        jsr read_nt
        assert_a_eq $BB
        pass_test

        ; ========================================
        ; Test Horizontal Mirroring
        ; ========================================
        start_test 3, "Horiz mirror"
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8 .or MAPPER_NUM = 15
            set_mirroring 3     ; MMC1: 3 = horizontal
        .else
            set_mirroring 1     ; MMC3/generic: 1 = horizontal
        .endif

        ; Write $CC to NT A ($2000)
        lda #$CC
        ldx #$20
        ldy #$0F
        jsr write_nt

        ; Write $DD to NT C ($2800)
        lda #$DD
        ldx #$28
        ldy #$0F
        jsr write_nt

        ; In horizontal: $2000 mirrors $2400, $2800 mirrors $2C00
        ; Read from $2400 — should equal $CC
        ldx #$24
        ldy #$0F
        jsr read_nt
        assert_a_eq $CC

        ; Restore vertical before console output
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8 .or MAPPER_NUM = 15
            set_mirroring 2
        .else
            set_mirroring 0
        .endif
        pass_test

        start_test 4, "Horiz mirror2"
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8 .or MAPPER_NUM = 15
            set_mirroring 3     ; Re-enter horizontal
        .else
            set_mirroring 1
        .endif
        ; Read from $2C00 — should equal $DD
        ldx #$2C
        ldy #$0F
        jsr read_nt
        assert_a_eq $DD
        ; Restore vertical before console output
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8 .or MAPPER_NUM = 15
            set_mirroring 2
        .else
            set_mirroring 0
        .endif
        pass_test
    .endif

    ; ========================================
    ; Test Single-Screen A (if supported)
    ; ========================================
    .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 7 .or MAPPER_NUM = 8
        ; Set 1-Screen A, do PPU writes/reads, then restore before console output
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 0     ; MMC1: 0 = single-screen A
        .else
            set_mirroring 0     ; AxROM: 0 = screen A
        .endif

        ; Write $EE to NT A ($2000)
        lda #$EE
        ldx #$20
        ldy #$0F
        jsr write_nt

        ; Read from $2400 — should mirror A
        ldx #$24
        ldy #$0F
        jsr read_nt
        sta nt_read             ; Save result

        ; Restore mirroring before console output
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 2
        .else
            set_mirroring 0
        .endif

        start_test 5, "1-Screen A"
        lda nt_read
        assert_a_eq $EE
        pass_test

        ; Test $2800 mirror
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 0
        .else
            set_mirroring 0
        .endif
        ldx #$28
        ldy #$0F
        jsr read_nt
        sta nt_read
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 2
        .else
            set_mirroring 0
        .endif

        start_test 6, "1-Screen A2"
        lda nt_read
        assert_a_eq $EE
        pass_test

        ; Test $2C00 mirror
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 0
        .else
            set_mirroring 0
        .endif
        ldx #$2C
        ldy #$0F
        jsr read_nt
        sta nt_read
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 2
        .else
            set_mirroring 0
        .endif

        start_test 7, "1-Screen A3"
        lda nt_read
        assert_a_eq $EE
        pass_test

        ; ========================================
        ; Test Single-Screen B
        ; ========================================
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 1     ; MMC1: 1 = single-screen B
        .else
            set_mirroring 1     ; AxROM: 1 = screen B
        .endif

        ; Write $77 to NT B ($2400)
        lda #$77
        ldx #$24
        ldy #$0F
        jsr write_nt

        ; Read from $2000 — should mirror B
        ldx #$20
        ldy #$0F
        jsr read_nt
        sta nt_read

        ; Restore mirroring before console output
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 2
        .else
            set_mirroring 0
        .endif

        start_test 8, "1-Screen B"
        lda nt_read
        assert_a_eq $77
        pass_test

        ; Test $2800 mirror
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 1
        .else
            set_mirroring 1
        .endif
        ldx #$28
        ldy #$0F
        jsr read_nt
        sta nt_read
        .if MAPPER_NUM = 1 .or MAPPER_NUM = 5 .or MAPPER_NUM = 6 .or MAPPER_NUM = 8
            set_mirroring 2
        .else
            set_mirroring 0
        .endif

        start_test 9, "1-Screen B2"
        lda nt_read
        assert_a_eq $77
        pass_test
    .endif

    jsr enable_rendering
    rts
.endproc

; Export unique name for combined ROM builds
run_nametable = run_tests
.export run_nametable

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
