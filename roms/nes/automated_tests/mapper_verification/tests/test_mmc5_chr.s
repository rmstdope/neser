; test_mmc5_chr.s — MMC5 CHR Mode Switching and A/B Register Verification
;
; Tests MMC5-specific CHR features that are verifiable through PPUDATA reads:
;
; 1. CHR mode 1 ($5101=%01): 4KB×2 banking via $5123 and $5127
; 2. CHR mode 3 ($5101=%11): 1KB×8 banking via $5120-$5127
; 3. A/B register separation: last-written register set ($5120-$5127
;    or $5128-$512B) controls PPUDATA access (NESdev spec)
;
; The test uses the existing 32KB CHR-ROM (4×8KB banks) with signatures
; at 8KB boundaries.  In finer modes, signatures are readable at their
; corresponding absolute bank indices (e.g., 4KB bank 0 = first 4KB of
; 8KB bank 0, 4KB bank 2 = first 4KB of 8KB bank 1).
;
; Rendering is disabled for all PPU reads and restored before console output.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "MMC5 CHR m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "ZEROPAGE"
ppu_read_result: .res 1

.segment "CODE"

; Read a byte from PPU at given address
; A = high byte, X = low byte
; Returns value in A
.proc read_ppu_byte
    bit PPUSTATUS
    sta PPUADDR
    stx PPUADDR
    lda PPUDATA             ; Dummy read (fills buffer)
    lda PPUDATA             ; Actual value
    rts
.endproc

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

; Restore CHR to mode 0 (8KB) with bank 0 for font access
.proc restore_chr_state
    lda #0
    sta MMC5_CHR_MODE       ; Mode 0 = 8KB
    sta MMC5_CHR_A7         ; A register: bank 0
    rts
.endproc

.proc run_tests
    jsr disable_rendering

    ; ========================================
    ; Group 1: CHR Mode 1 — 4KB×2 banking
    ; ========================================
    ; In 4KB mode, $5123 controls $0000-$0FFF and $5127 controls $1000-$1FFF.
    ; With 32KB CHR-ROM: 4KB bank 0 = first 4KB of 8KB bank 0 (sig: $B6,0),
    ;                    4KB bank 2 = first 4KB of 8KB bank 1 (sig: $B6,1).

    ; Set CHR mode to 1 (4KB)
    lda #1
    sta MMC5_CHR_MODE

    ; --- Test 1: 4KB mode, slot 0, bank 0 → read 8KB bank 0 signature ---
    lda #0
    sta MMC5_CHR_5123       ; 4KB bank 0 at $0000-$0FFF
    start_test 1, "4K sl0 b0"
    lda #$00
    ldx #$00
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq $B6         ; Signature marker from 8KB bank 0
    pass_test

    ; --- Test 2: 4KB mode, slot 0, bank 2 → read 8KB bank 1 signature ---
    jsr disable_rendering
    lda #1
    sta MMC5_CHR_MODE       ; Back to 4KB mode
    lda #2
    sta MMC5_CHR_5123       ; 4KB bank 2 at $0000-$0FFF
    start_test 2, "4K sl0 b2"
    lda #$00
    ldx #$01                ; Read bank_num byte (offset 1)
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 1           ; Bank num from 8KB bank 1's signature
    pass_test

    ; --- Test 3: 4KB mode, slot 1 ($1000), bank 4 → 8KB bank 2 signature ---
    jsr disable_rendering
    lda #1
    sta MMC5_CHR_MODE
    lda #4
    sta MMC5_CHR_A7         ; $5127: 4KB bank 4 at $1000-$1FFF
    start_test 3, "4K sl1 b4"
    lda #$10
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 2           ; Bank num from 8KB bank 2's signature
    pass_test

    ; ========================================
    ; Group 2: CHR Mode 3 — 1KB×8 banking
    ; ========================================
    ; In 1KB mode, $5120-$5127 each control a 1KB slot.
    ; With 32KB CHR-ROM: 1KB bank 0 = first 1KB of 8KB bank 0 (sig: $B6,0),
    ;                    1KB bank 8 = first 1KB of 8KB bank 1 (sig: $B6,1).

    ; Set CHR mode to 3 (1KB)
    jsr disable_rendering
    lda #3
    sta MMC5_CHR_MODE

    ; --- Test 4: 1KB mode, register 0 ($5120), bank 0 ---
    lda #0
    sta MMC5_CHR_A0         ; $5120: 1KB bank 0 at $0000-$03FF
    start_test 4, "1K r0 b0"
    lda #$00
    ldx #$00
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq $B6         ; Signature marker
    pass_test

    ; --- Test 5: 1KB mode, register 0, bank 8 → 8KB bank 1 signature ---
    jsr disable_rendering
    lda #3
    sta MMC5_CHR_MODE
    lda #8
    sta MMC5_CHR_A0         ; $5120: 1KB bank 8 at $0000-$03FF
    start_test 5, "1K r0 b8"
    lda #$00
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 1           ; Bank num from 8KB bank 1's signature
    pass_test

    ; --- Test 6: 1KB mode, register 4 ($5124), bank 16 → 8KB bank 2 signature ---
    jsr disable_rendering
    lda #3
    sta MMC5_CHR_MODE
    lda #16
    sta MMC5_CHR_5124       ; $5124: 1KB bank 16 at $1000-$13FF
    start_test 6, "1K r4 b16"
    lda #$10
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 2           ; Bank num from 8KB bank 2's signature
    pass_test

    ; ========================================
    ; Group 3: A/B register separation
    ; ========================================
    ; Per NESdev: "the last set of registers written to (either $5120-$5127
    ; or $5128-$512B) will be used for I/O via PPUDATA ($2007)."
    ;
    ; In 8KB mode: $5127 = A set, $512B = B set.

    ; Set CHR mode to 0 (8KB) for simplicity
    jsr disable_rendering
    lda #0
    sta MMC5_CHR_MODE

    ; --- Test 7: Write A reg ($5127) = bank 0, read via PPUDATA ---
    lda #0
    sta MMC5_CHR_A7         ; A register: bank 0 (last written = A)
    start_test 7, "A/B A b0"
    lda #$00
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 0           ; Bank num 0 via A registers
    pass_test

    ; --- Test 8: Write B reg ($512B) = bank 1, PPUDATA should follow B ---
    jsr disable_rendering
    lda #0
    sta MMC5_CHR_MODE
    lda #0
    sta MMC5_CHR_A7         ; First set A to bank 0
    lda #1
    sta MMC5_CHR_B3         ; Then set B to bank 1 via $512B (last written = B)
    start_test 8, "A/B B b1"
    lda #$00
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 1           ; Bank num 1 via B registers
    pass_test

    ; --- Test 9: Write A reg again ($5127) = bank 2, PPUDATA follows A ---
    jsr disable_rendering
    lda #0
    sta MMC5_CHR_MODE
    lda #1
    sta MMC5_CHR_B3         ; Set B to bank 1 via $512B first
    lda #2
    sta MMC5_CHR_A7         ; Then set A to bank 2 (last written = A)
    start_test 9, "A/B A b2"
    lda #$00
    ldx #$01
    jsr read_ppu_byte
    sta ppu_read_result
    jsr restore_chr_state
    jsr enable_rendering
    lda ppu_read_result
    assert_a_eq 2           ; Bank num 2 via A registers
    pass_test

    ; ========================================
    rts
.endproc

; Export unique name for combined ROM builds
run_mmc5_chr = run_tests
.export run_mmc5_chr

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

; CHR bank signatures — shared with test_chr_banking.s
; (Only emitted when these segments exist in the linker config)
.segment "CHR_SIG0"
    .byte $B6, 0, $FF, $6B
.segment "CHR_SIG1"
    .byte $B6, 1, $FE, $6B
.segment "CHR_SIG2"
    .byte $B6, 2, $FD, $6B
.segment "CHR_SIG3"
    .byte $B6, 3, $FC, $6B
