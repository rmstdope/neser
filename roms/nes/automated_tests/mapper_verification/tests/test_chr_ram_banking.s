; test_chr_ram_banking.s — CHR-RAM Bank Switching Verification
;
; Tests that CHR-RAM bank switching works correctly by writing
; unique patterns to each switchable bank and verifying they persist.
;
; For CPROM (mapper 13):
;   PPU $0000-$0FFF: fixed CHR-RAM (always page 0)
;   PPU $1000-$1FFF: switchable CHR-RAM (4KB × 4 banks)
;
; When bank 0 is selected at $1000, it is the SAME physical
; memory as $0000-$0FFF (page 0).
;
; Defs flag CHR_RAM_BANK_COUNT controls how many banks are tested.
; Default is 4; set to 2 for mappers with only 2 CHR-RAM banks (e.g., GTROM).

.include "test_macros.inc"
.include "mapper_config.inc"

; Default: 4 CHR-RAM banks. Mapper defs can `.define HAS_ONLY_2_CHR_RAM_BANKS 1`
; to skip banks 2-3 tests (e.g., GTROM).

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "CHR RAM m"
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

.proc run_tests
    jsr disable_rendering

    ; === Write unique patterns to each CHR-RAM bank at $1000 ===

    start_test 1, "Write B0"
    select_chr_bank 0, 0
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda #$AA
    sta PPUDATA
    pass_test

    start_test 2, "Write B1"
    select_chr_bank 0, 1
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda #$BB
    sta PPUDATA
    pass_test

.ifndef HAS_ONLY_2_CHR_RAM_BANKS
    start_test 3, "Write B2"
    select_chr_bank 0, 2
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda #$CC
    sta PPUDATA
    pass_test
.endif

.ifndef HAS_ONLY_2_CHR_RAM_BANKS
    start_test 4, "Write B3"
    select_chr_bank 0, 3
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda #$DD
    sta PPUDATA
    pass_test
.endif

    ; === Verify each bank preserved its written pattern ===

    start_test 5, "Read B0"
    select_chr_bank 0, 0
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA             ; Dummy read (PPU read buffer)
    lda PPUDATA             ; Actual value
    assert_a_eq $AA
    pass_test

    start_test 6, "Read B1"
    select_chr_bank 0, 1
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA
    lda PPUDATA
    assert_a_eq $BB
    pass_test

.ifndef HAS_ONLY_2_CHR_RAM_BANKS
    start_test 7, "Read B2"
    select_chr_bank 0, 2
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA
    lda PPUDATA
    assert_a_eq $CC
    pass_test
.endif

.ifndef HAS_ONLY_2_CHR_RAM_BANKS
    start_test 8, "Read B3"
    select_chr_bank 0, 3
    bit PPUSTATUS
    lda #$10
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA
    lda PPUDATA
    assert_a_eq $DD
    pass_test
.endif

    ; === Verify fixed bank at $0000 is unaffected by bank switching ===
    ; $0000-$0FFF is always page 0 (same physical memory as bank 0 at $1000).
    ; Only for mappers with split 4KB/4KB CHR-RAM (CPROM style); controlled by HAS_CHR_FIXED_LOWER.

.if HAS_CHR_FIXED_LOWER
    start_test 9, "Fixed $0"
    select_chr_bank 0, 2        ; Switch to bank 2 at $1000
    bit PPUSTATUS
    lda #$00
    sta PPUADDR
    lda #$00
    sta PPUADDR
    lda PPUDATA                 ; Dummy read
    lda PPUDATA                 ; Read from $0000 (fixed page 0)
    assert_a_eq $AA             ; Should match bank 0 write from test 1
    pass_test
.endif

    ; Whole-window CHR-RAM banking leaves the last tested bank selected.
    ; Restore bank 0 so the console font copied during startup is visible again.
.if .not HAS_CHR_FIXED_LOWER
    select_chr_bank 0, 0
.endif
    jsr enable_rendering
    rts
.endproc

; Export unique name for combined ROM builds
run_chr_ram_banking = run_tests
.export run_chr_ram_banking

; Bus conflict lookup table (for mappers that need it)
; Always export since chr_ram_banking may be the only test in a combined build.
.if HAS_BUS_CONFLICTS
.segment "RODATA"
.export bank_table
bank_table:
    .repeat 16, i
        .byte i
    .endrepeat
.endif

.ifndef COMBINED
; NES 2.0 Header
.include "nes20_header.inc"
nes20_header
.endif
