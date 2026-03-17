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
; This test ROM places CHR signature data at the start of CHR bank 8 (the
; first half of the CHR_FONT region: $B6, 8, $F7, $6B). Mapping bank 8
; to NT $2000 allows PPU reads from $2000 to return that signature.

.ifdef HAS_NT_FROM_CHR

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

.proc run_tests
    jsr disable_rendering

    ; ========================================
    ; Test 1: NT $2000 mapped from CHR bank 8
    ; ========================================
    ; Write bank 8 to the NT select register for PPU $2000-$23FF (slot 8 at $C000)
    ; CHR bank 8 starts with signature: $B6, 8, $F7, $6B
    start_test 1, "NT from CHR"

    lda #8
    sta $C000               ; Slot 8: map CHR bank 8 to PPU $2000-$23FF

    ; Read from PPU $2000 (first byte of CHR bank 8 should be $B6)
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA             ; Dummy read (PPU read buffer)
    lda PPUDATA             ; Actual byte from CHR bank 8 at $2000
    sta nt_read_val

    ; Restore CIRAM before console output: write $E8 (chip RAM) to slot 8
    lda #$E8
    sta $C000

    lda nt_read_val
    assert_a_eq $B6         ; CHR bank 8 signature marker
    pass_test

    ; ========================================
    ; Test 2: NT bank ID byte (offset +1)
    ; ========================================
    start_test 2, "NT CHR id"

    lda #8
    sta $C000               ; Re-map CHR bank 8 to NT $2000

    ; Read offset +1 from $2001 (bank ID byte)
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$01
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Should be bank 8 ID
    sta nt_read_val

    lda #$E8
    sta $C000               ; Restore CIRAM

    lda nt_read_val
    assert_a_eq 8           ; CHR bank 8 ID
    pass_test

    ; ========================================
    ; Test 3: NT from CIRAM (internal RAM mode)
    ; ========================================
    ; Write $E8 to NT select → maps to internal chip RAM (CIRAM-like)
    ; Then write a pattern to PPU $2000 and verify read-back
    start_test 3, "NT CIRAM"

    lda #$E8
    sta $C000               ; Slot 8: map internal chip RAM to $2000-$23FF

    ; Write test pattern to PPU $200F
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$0F
    sta PPUADDR
    lda #$55
    sta PPUDATA

    ; Read back from PPU $200F
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$0F
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Should be $55

    assert_a_eq $55
    pass_test

    ; ========================================
    ; Test 4: NT $2400 mapped from CHR bank 9
    ; ========================================
    start_test 4, "NT CHR $2400"

    lda #9
    sta $C800               ; Slot 9: map CHR bank 9 to PPU $2400-$27FF

    ; CHR bank 9 signature: $B6, 9, $F6, $6B
    bit PPUSTATUS
    lda #$24
    sta PPUADDR
    lda #$01
    sta PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA             ; Bank 9 ID
    sta nt_read_val

    lda #$E8
    sta $C800               ; Restore slot 9 to chip RAM

    lda nt_read_val
    assert_a_eq 9           ; CHR bank 9 ID
    pass_test

    jsr enable_rendering
    rts
.endproc

; Export unique name for combined ROM builds
run_nt_from_chr = run_tests
.export run_nt_from_chr

.ifndef COMBINED
; NES 2.0 Header
.include "nes20_header.inc"
nes20_header
.endif ; COMBINED

.endif ; HAS_NT_FROM_CHR
