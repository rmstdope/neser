; test_chr_latch.s — PPU-triggered CHR Latch Verification (MMC2/MMC4)
;
; Tests that the PPU-triggered CHR bank latch works correctly.
;
; Each CHR 4KB window has TWO bank register slots:
;   - FD bank: used when latch state = $FD
;   - FE bank: used when latch state = $FE
;
; The latch switches when the PPU reads specific addresses:
;   Latch 0 ($0000-$0FFF): $0FD8 → FD, $0FE8 → FE
;   Latch 1 ($1000-$1FFF): $1FD8-$1FDF → FD, $1FE8-$1FEF → FE
;
; Initial latch state on power-on is $FE.
;
; CHR bank signatures: $B6, bank_num, ~bank_num, $6B at offset 0
; Read via PPUDATA with rendering disabled.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "CHR Latch m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

; Disable rendering for safe PPU access
.proc disable_rendering
    lda #0
    sta PPUMASK
    sta ppumask_shadow
    rts
.endproc

; Re-enable rendering
.proc enable_rendering
    lda #%00001010
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

; Read byte from CHR at PPU address hi=A, lo=X
; Returns value in A. Must call during VBlank/rendering disabled.
.proc read_chr_byte
    bit PPUSTATUS
    sta PPUADDR
    stx PPUADDR
    lda PPUDATA             ; Dummy read (fills buffer)
    lda PPUDATA             ; Actual value
    rts
.endproc

; Trigger latch by reading from a trigger address.
; PPU address hi=A, lo=X. Only the PPU fetch matters (not the data).
.proc trigger_latch
    bit PPUSTATUS
    sta PPUADDR
    stx PPUADDR
    lda PPUDATA             ; Dummy read: PPU fetches from trigger address
    rts
.endproc

.proc run_tests
    jsr disable_rendering

    ; ============================================================
    ; Setup: Configure latch banks
    ; Latch 0 (lower, $0000-$0FFF):  FD=bank 1, FE=bank 0
    ; Latch 1 (upper, $1000-$1FFF):  FD=bank 2, FE=bank 3
    ; ============================================================
    set_chr_latch_fd 0, 1       ; Latch 0 FD → bank 1
    set_chr_latch_fe 0, 0       ; Latch 0 FE → bank 0
    set_chr_latch_fd 1, 2       ; Latch 1 FD → bank 2
    set_chr_latch_fe 1, 3       ; Latch 1 FE → bank 3

    ; ============================================================
    ; Test 1: Initial FE state (power-on default)
    ; Latch 0 should be $FE → bank 0
    ; ============================================================
    start_test 1, "FE init"
    ; Read signature byte 1 (bank ID) from $0001
    lda #$00
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 0               ; Bank 0 (FE bank for latch 0)
    pass_test

    ; ============================================================
    ; Test 2: Trigger latch 0 → $FD
    ; Reading from $0FD8 switches latch 0 to FD state
    ; ============================================================
    start_test 2, "FD trigger"
    ; Trigger: read from $0FD8
    lda #$0F
    ldx #$D8
    jsr trigger_latch
    ; Verify: latch 0 now shows FD bank (bank 1)
    lda #$00
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 1               ; Bank 1 (FD bank for latch 0)
    pass_test

    ; ============================================================
    ; Test 3: Trigger latch 0 → $FE
    ; Reading from $0FE8 switches latch 0 back to FE state
    ; ============================================================
    start_test 3, "FE trigger"
    lda #$0F
    ldx #$E8
    jsr trigger_latch
    ; Verify: latch 0 back to FE bank (bank 0)
    lda #$00
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 0               ; Bank 0 (FE bank for latch 0)
    pass_test

    ; ============================================================
    ; Test 4: Latch 1 FE state (upper pattern table)
    ; Latch 1 should be $FE → bank 3
    ; ============================================================
    start_test 4, "Up FE"
    ; Read from $1001 (upper pattern table, no trigger)
    lda #$10
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 3               ; Bank 3 (FE bank for latch 1)
    pass_test

    ; ============================================================
    ; Test 5: Trigger latch 1 → $FD
    ; Reading from $1FD8 switches latch 1 to FD state
    ; ============================================================
    start_test 5, "Up FD"
    lda #$1F
    ldx #$D8
    jsr trigger_latch
    lda #$10
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 2               ; Bank 2 (FD bank for latch 1)
    pass_test

    ; ============================================================
    ; Test 6: Latches are independent
    ; After changing latch 1 (test 5), latch 0 should still be FE
    ; ============================================================
    start_test 6, "Indep"
    lda #$00
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 0               ; Bank 0 (FE, unchanged from test 3)
    pass_test

    ; Restore FE banks for console font display
    ; Trigger latch 0 → $FE (read $0FE8)
    lda #$0F
    ldx #$E8
    jsr trigger_latch
    ; Trigger latch 1 → $FE (read $1FE8)
    lda #$1F
    ldx #$E8
    jsr trigger_latch

    jsr enable_rendering
    rts
.endproc

; Export unique name for combined ROM builds
run_chr_latch = run_tests
.export run_chr_latch

; ============================================================
; CHR Bank signature data (4KB banks)
; ============================================================

.segment "CHR_SIG0"
    .byte $B6, 0, $FF, $6B

.segment "CHR_SIG1"
    .byte $B6, 1, $FE, $6B

.segment "CHR_SIG2"
    .byte $B6, 2, $FD, $6B

.segment "CHR_SIG3"
    .byte $B6, 3, $FC, $6B

.segment "CHR_SIG4"
    .byte $B6, 4, $FB, $6B

.segment "CHR_SIG5"
    .byte $B6, 5, $FA, $6B

.segment "CHR_SIG6"
    .byte $B6, 6, $F9, $6B

.segment "CHR_SIG7"
    .byte $B6, 7, $F8, $6B

.ifndef COMBINED
; NES 2.0 Header
.include "nes20_header.inc"
nes20_header

; ASCII font in CHR bank 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
