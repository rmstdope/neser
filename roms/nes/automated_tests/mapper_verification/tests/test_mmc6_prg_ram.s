; test_mmc6_prg_ram.s — MMC6 internal PRG-RAM verification
;
; Specification: https://www.nesdev.org/wiki/MMC6
;   - "CPU $7000-$7FFF: 1 KB PRG RAM, mirrored"
;   - $8000 bit 5: "PRG RAM enable"
;   - $A001 = HhLl xxxx: H/h = read/write enable $7200-$73FF,
;                         L/l = read/write enable $7000-$71FF
;   - "If neither bank is enabled for reading, the $7000-$7FFF area is
;      open bus. If only one bank is enabled for reading, the other reads
;      back as zero. The write-enable bits only have effect if that bank is
;      enabled for reading, otherwise the bank is not writable."
;   - "When PRG RAM is disabled via $8000, the mapper continuously sets
;      $A001 to $00, and so all writes to $A001 are ignored."
;
; Open bus: an absolute-mode read of an undriven address returns the last
; value on the CPU data bus, which is the high byte of the operand
; (https://www.nesdev.org/wiki/Open_bus_behavior), e.g. LDA $7000 -> $70.
;
; Only $8000 is written with the bank-select value $20 (R0 selected, PRG
; and CHR mode 0, RAM enabled) or $00; $8001 is never written, so the font
; mapping in R0 and the PRG layout are left untouched.
;
; Console-verified: there is no RAM at $6000 on the MMC6.

.include "test_macros.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "MMC6 RAM m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

LO_ADDR = $7000                 ; first byte of the low 512-byte half
HI_ADDR = $7200                 ; first byte of the high 512-byte half
LO_VAL  = $A5
HI_VAL  = $5A
JUNK    = $EE

; Enable PRG-RAM in $8000 (bit 5), leaving R0 selected and modes at 0
.macro mmc6_ram_on
    lda #MMC6_RAM_ENABLE
    sta MMC3_BANK_SELECT
.endmacro

; Disable PRG-RAM in $8000 (bit 5 clear)
.macro mmc6_ram_off
    lda #0
    sta MMC3_BANK_SELECT
.endmacro

; Write the $A001 PRG-RAM protect register
.macro mmc6_protect value
    lda #value
    sta MMC3_PRG_RAM
.endmacro

MMC6_ALL = MMC6_RD_HI | MMC6_WR_HI | MMC6_RD_LO | MMC6_WR_LO

.segment "CODE"

run_mmc6_prg_ram = run_tests
.export run_mmc6_prg_ram

.proc run_tests
    ; ========================================
    ; Test 1: both halves readable and writable
    ; ========================================
    start_test 1, "RAM R/W"
    mmc6_ram_on
    mmc6_protect MMC6_ALL
    lda #LO_VAL
    sta LO_ADDR
    lda #HI_VAL
    sta HI_ADDR
    lda LO_ADDR
    assert_a_eq LO_VAL
    lda HI_ADDR
    assert_a_eq HI_VAL
    pass_test

    ; ========================================
    ; Test 2: 1 KB mirrored through $7000-$7FFF
    ; ========================================
    start_test 2, "1K mirror"
    lda #$11
    sta $7005
    lda $7405
    assert_a_eq $11
    lda $7805
    assert_a_eq $11
    lda $7C05
    assert_a_eq $11
    lda #$22
    sta $7F10                   ; mirror of $7310 (high half)
    lda $7310
    assert_a_eq $22
    lda $7B10
    assert_a_eq $22
    pass_test

    ; ========================================
    ; Test 3: no RAM at $6000-$6FFF (open bus)
    ; ========================================
    start_test 3, "No RAM $6000"
    lda #$C3
    sta $6000
    sta $6400
    lda $6000
    assert_a_eq $60             ; open bus: high byte of operand
    lda $6400
    assert_a_eq $64
    lda LO_ADDR                 ; the $6000 writes did not alias into RAM
    assert_a_eq LO_VAL
    lda $7400
    assert_a_eq LO_VAL
    pass_test

    ; ========================================
    ; Test 4: only low half readable (L+l), h set without H
    ;   low half readable and writable; high half reads 0, not writable
    ; ========================================
    start_test 4, "Low half only"
    mmc6_protect (MMC6_RD_LO | MMC6_WR_LO | MMC6_WR_HI)
    lda LO_ADDR
    assert_a_eq LO_VAL
    lda HI_ADDR
    assert_a_eq $00             ; unreadable half reads back as zero
    lda #$B6
    sta LO_ADDR
    lda LO_ADDR
    assert_a_eq $B6             ; low half writable
    lda #JUNK
    sta HI_ADDR                 ; h without H: must not be written
    lda #LO_VAL
    sta LO_ADDR                 ; restore low half
    pass_test

    ; ========================================
    ; Test 5: only high half readable (H+h), l set without L
    ; ========================================
    start_test 5, "High half only"
    mmc6_protect (MMC6_RD_HI | MMC6_WR_HI | MMC6_WR_LO)
    lda HI_ADDR
    assert_a_eq HI_VAL          ; test 4's write to $7200 was ignored
    lda LO_ADDR
    assert_a_eq $00             ; unreadable half reads back as zero
    lda #$6B
    sta HI_ADDR
    lda HI_ADDR
    assert_a_eq $6B             ; high half writable
    lda #JUNK
    sta LO_ADDR                 ; l without L: must not be written
    lda #HI_VAL
    sta HI_ADDR                 ; restore high half
    mmc6_protect MMC6_ALL
    lda LO_ADDR
    assert_a_eq LO_VAL          ; the write to $7000 above was ignored
    pass_test

    ; ========================================
    ; Test 6: both readable, neither writable
    ; ========================================
    start_test 6, "Read only"
    mmc6_protect (MMC6_RD_HI | MMC6_RD_LO)
    lda #JUNK
    sta LO_ADDR
    sta HI_ADDR
    lda LO_ADDR
    assert_a_eq LO_VAL
    lda HI_ADDR
    assert_a_eq HI_VAL
    pass_test

    ; ========================================
    ; Test 7: neither readable -> open bus, writes need read enable
    ; ========================================
    start_test 7, "Open bus"
    mmc6_protect (MMC6_WR_HI | MMC6_WR_LO)
    lda LO_ADDR
    assert_a_eq $70             ; open bus: high byte of operand
    lda HI_ADDR
    assert_a_eq $72
    lda $7C00
    assert_a_eq $7C
    lda #JUNK
    sta LO_ADDR                 ; write enables without read enables
    sta HI_ADDR
    mmc6_protect MMC6_ALL
    lda LO_ADDR
    assert_a_eq LO_VAL          ; writes above had no effect
    lda HI_ADDR
    assert_a_eq HI_VAL
    pass_test

    ; ========================================
    ; Test 8: $8000 bit 5 clear disables RAM and holds $A001 at $00
    ; ========================================
    start_test 8, "$8000 RAM off"
    mmc6_ram_off                ; $A001 was $F0; now forced to $00
    lda LO_ADDR
    assert_a_eq $70             ; open bus
    lda HI_ADDR
    assert_a_eq $72
    mmc6_protect MMC6_ALL       ; ignored while RAM is disabled
    lda #JUNK
    sta LO_ADDR                 ; not writable
    sta HI_ADDR
    lda LO_ADDR
    assert_a_eq $70             ; still open bus: the $A001 write was ignored
    pass_test

    ; ========================================
    ; Test 9: re-enabling via $8000 leaves $A001 at $00
    ; ========================================
    start_test 9, "$A001 held 0"
    mmc6_ram_on
    lda LO_ADDR
    assert_a_eq $70             ; $A001 is still $00: open bus
    lda HI_ADDR
    assert_a_eq $72
    mmc6_protect MMC6_ALL       ; now accepted
    lda LO_ADDR
    assert_a_eq LO_VAL          ; data retained, test 8 writes ignored
    lda HI_ADDR
    assert_a_eq HI_VAL
    pass_test

    ; Leave PRG-RAM enabled but protected, bank select back at R0
    mmc6_protect (MMC6_RD_HI | MMC6_RD_LO)
    rts
.endproc

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
