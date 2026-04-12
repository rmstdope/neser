; test_vrc2.s — VRC2-mode Verification for SL-1632 (Mapper 14)
;
; Tests VRC2-mode behavior when the supervisor register ($A131) selects
; VRC2 mode (bit 1 clear).  Shell.s initializes the mapper in MMC3 mode;
; these tests switch to VRC2, exercise PRG/CHR/mirroring, verify that
; MMC3 IRQ does not leak through, and switch back to MMC3.
;
; Tests:
;   1. Mode switch to VRC2 — VRC2 PRG register controls $8000
;   2. VRC2 PRG banking   — both switchable slots + fixed banks
;   3. VRC2 CHR banking   — nibble-split registers across $B000–$E003
;   4. VRC2 mirroring     — $9000 bit 0 selects V/H
;   5. VRC2 IRQ negative  — armed MMC3 IRQ does not fire in VRC2 mode
;   6. Mode switch to MMC3 — MMC3 banking resumes correctly

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow
.importzp irq_fired, irq_count

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "VRC2 m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
saved_val:   .res 1
frame_count: .res 1

.segment "CODE"

; Read CHR byte at PPU address A(hi):X(lo) → A
.proc read_chr_byte
    bit PPUSTATUS
    sta PPUADDR
    stx PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA
    rts
.endproc

; Write to nametable: A=value, X=hi byte, Y=lo byte
.proc write_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    sta PPUDATA
    rts
.endproc

; Read from nametable: X=hi byte, Y=lo byte → A
.proc read_nt
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    lda PPUDATA             ; Dummy read
    lda PPUDATA
    rts
.endproc

.proc disable_rendering
    lda #0
    sta PPUMASK
    sta ppumask_shadow
    rts
.endproc

.proc enable_rendering
    lda #%00001010
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

; Enable BG rendering (required for MMC3 A12 clocking)
.proc enable_bg
    lda #%00001000
    sta PPUCTRL
    lda #%00001000
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

.proc wait_vbl
    bit PPUSTATUS
:   bit PPUSTATUS
    bpl :-
    rts
.endproc

.proc wait_frames
    sta frame_count
@loop:
    jsr wait_vbl
    dec frame_count
    bne @loop
    rts
.endproc

; ============================================================

.proc run_tests

    ; ==========================================================
    ; Test 1: Mode switch to VRC2
    ; ==========================================================
    ; Shell.s initialised MMC3 mode.  Switch to VRC2 and verify
    ; that VRC2 PRG register controls the $8000 window.

    set_vrc2_mode
    init_vrc2_chr_font

    start_test 1, "VRC2 mode"
    select_vrc2_prg_bank 0, 3
    lda $8000
    assert_a_eq $A5             ; PRG signature marker
    lda $8001
    assert_a_eq 3               ; Bank ID
    pass_test

    ; ==========================================================
    ; Test 2: VRC2 PRG banking
    ; ==========================================================
    ; Verify both switchable slots and the two fixed slots.

    start_test 2, "VRC2 PRG"
    ; Slot 0 → bank 5
    select_vrc2_prg_bank 0, 5
    lda $8001
    assert_a_eq 5
    ; Slot 1 → bank 2
    select_vrc2_prg_bank 1, 2
    lda $A001
    assert_a_eq 2
    ; Fixed bank at $E000 should be the last bank (signature present)
    lda $E000
    assert_a_eq $A5
    pass_test

    ; ==========================================================
    ; Test 3: VRC2 CHR banking (nibble-split registers)
    ; ==========================================================
    ; Program slots 2, 4, 6, 7 with distinct banks and read back
    ; via PPUDATA.  Bank 26 ($1A) tests that both nibbles combine.

    start_test 3, "VRC2 CHR"
    jsr disable_rendering

    select_vrc2_chr_bank 2, 26  ; $C000/$C001 — lo=$0A, hi=$01
    select_vrc2_chr_bank 4, 6   ; $D000/$D001
    select_vrc2_chr_bank 6, 3   ; $E000/$E001
    select_vrc2_chr_bank 7, 7   ; $E002/$E003

    ; Slot 2 → PPU $0800
    lda #$08
    ldx #$00
    jsr read_chr_byte
    assert_a_eq $B6             ; CHR signature marker
    lda #$08
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 26              ; Bank 26

    ; Slot 4 → PPU $1000
    lda #$10
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 6

    ; Slot 6 → PPU $1800
    lda #$18
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 3

    ; Slot 7 → PPU $1C00
    lda #$1C
    ldx #$01
    jsr read_chr_byte
    assert_a_eq 7

    ; Restore font for console output
    init_vrc2_chr_font
    jsr enable_rendering
    pass_test

    ; ==========================================================
    ; Test 4: VRC2 mirroring
    ; ==========================================================
    ; $9000 bit 0: 0=Vertical, 1=Horizontal

    start_test 4, "VRC2 mirror"
    jsr disable_rendering

    ; Vertical: $2000 mirrors $2800
    set_vrc2_mirroring 0
    lda #$AA
    ldx #$20
    ldy #$0F
    jsr write_nt
    ldx #$28
    ldy #$0F
    jsr read_nt
    assert_a_eq $AA

    ; Horizontal: $2000 mirrors $2400
    set_vrc2_mirroring 1
    lda #$CC
    ldx #$20
    ldy #$10
    jsr write_nt
    ldx #$24
    ldy #$10
    jsr read_nt
    sta saved_val

    ; Restore vertical mirroring before console output
    set_vrc2_mirroring 0
    jsr enable_rendering
    lda saved_val
    assert_a_eq $CC
    pass_test

    ; ==========================================================
    ; Test 5: VRC2 IRQ negative test
    ; ==========================================================
    ; Arm MMC3 scanline IRQ, switch to VRC2 mode, wait several
    ; frames, and verify no IRQ fires (VRC2 has no IRQ).

    ; Temporarily return to MMC3 to arm the IRQ
    set_mmc3_mode
    init_chr_font
    lda #0
    sta irq_fired
    sta irq_count
    set_irq_counter 10
    enable_irq
    jsr enable_bg

    ; Switch to VRC2 — IRQ counter stops clocking
    set_vrc2_mode
    init_vrc2_chr_font
    ; Clear irq_fired AFTER mode switch to isolate VRC2 behavior
    lda #0
    sta irq_fired

    ; Wait several frames — if IRQ leaked through, irq_fired != 0
    lda #5
    jsr wait_frames

    start_test 5, "No VRC2 IRQ"
    lda irq_fired
    assert_a_eq 0

    ; Clean up: mask CPU interrupts, disable MMC3 IRQ
    sei
    set_mmc3_mode
    disable_irq
    ; Return to VRC2 for test 6 preamble
    set_vrc2_mode
    init_vrc2_chr_font
    pass_test

    ; ==========================================================
    ; Test 6: Mode switch back to MMC3
    ; ==========================================================
    ; Set VRC2 PRG state that must NOT leak into MMC3 banking.

    select_vrc2_prg_bank 0, 7      ; VRC2 slot 0 → bank 7

    ; Switch to MMC3 mode
    set_mmc3_mode
    init_chr_font

    start_test 6, "Back to MMC3"
    ; Program MMC3 R6 (PRG slot 0) to bank 2
    lda #$06
    sta MMC3_BANK_SELECT
    lda #2
    sta MMC3_BANK_DATA
    lda $8001
    assert_a_eq 2
    pass_test

    ; Leave mapper in MMC3 mode for shell cleanup
    rts
.endproc

; Export unique name for combined ROM builds
run_vrc2 = run_tests
.export run_vrc2

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
