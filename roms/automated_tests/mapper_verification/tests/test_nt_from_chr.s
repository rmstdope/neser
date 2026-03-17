; test_nt_from_chr.s — Namco 163 Nametable-from-CHR Verification
;
; Tests the Namco 163 (Mapper 19) ability to map nametables from CHR ROM
; instead of CIRAM. NT bank values $00-$DF select CHR ROM banks; $E0-$FF
; select internal chip RAM (which acts like CIRAM).
;
; NT select registers: $8000-$DFFF, address bits 15-12 select the 1KB PPU slot
;   Slot  8 ($C000): PPU $2000-$23FF (nametable A)
;   Slot  9 ($C800): PPU $2400-$27FF (nametable B)
;   Slot 10 ($D000): PPU $2800-$2BFF (nametable C)
;   Slot 11 ($D800): PPU $2C00-$2FFF (nametable D)
;
; This test ROM uses a dedicated CHR signature bank rather than the font bank.
; Bank 10 contains the standard signature header ($B6, 10, $F7, $6B), and bank
; 11 lets us verify a second nametable quadrant without overlapping the font.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "NT-CHR m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
nt_read_val: .res 1

.segment "CODE"

.proc run_tests
.if HAS_NT_FROM_CHR
    ; Disable rendering for safe PPU access
    lda #0
    sta PPUMASK
    sta ppumask_shadow

    ; Test 1: NT $2000 mapped from CHR bank 10
    start_test 1, "NT from CHR"

    lda #10
    sta $C000               ; Slot 8: map CHR bank 10 to PPU $2000-$23FF

    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; First byte from CHR bank 10 at $2000
    sta nt_read_val

    lda #$E8
    sta $C000               ; Restore CIRAM: write $E8 (chip RAM) to slot 8

    lda nt_read_val
    assert_a_eq $B6
    pass_test

    ; Test 2: NT bank ID byte (offset +1)
    start_test 2, "NT CHR id"

    lda #10
    sta $C000               ; Re-map CHR bank 10 to NT $2000

    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$01
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Bank ID byte at offset +1
    sta nt_read_val

    lda #$E8
    sta $C000               ; Restore CIRAM

    lda nt_read_val
    assert_a_eq 10
    pass_test

    ; Test 3: NT from CIRAM (internal RAM mode)
    start_test 3, "NT CIRAM"

    lda #$E8
    sta $C000               ; Slot 8: map internal chip RAM to $2000-$23FF

    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$0F
    sta PPUADDR
    lda #$55
    sta PPUDATA

    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$0F
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Should be $55

    assert_a_eq $55
    pass_test

    ; Test 4: NT $2400 mapped from CHR bank 11
    start_test 4, "NT CHR $2400"

    lda #11
    sta $C800               ; Slot 9: map CHR bank 11 to PPU $2400-$27FF

    bit PPUSTATUS
    lda #$24
    sta PPUADDR
    lda #$01
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Bank 11 ID
    sta nt_read_val

    lda #$E8
    sta $C800               ; Restore slot 9 to chip RAM

    lda nt_read_val
    assert_a_eq 11
    pass_test

    ; Re-enable rendering
    lda #%00001010
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
.endif
    rts
.endproc

; Export unique name for combined ROM builds
run_nt_from_chr = run_tests
.export run_nt_from_chr

.segment "CHR_SIG10"
    .byte $B6, 10, $F7, $6B

.segment "CHR_SIG11"
    .byte $B6, 11, $F7, $6B

.ifndef COMBINED
.include "nes20_header.inc"
nes20_header
.endif
