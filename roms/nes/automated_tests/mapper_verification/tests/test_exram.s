; test_exram.s — MMC5 ExRAM Mode Verification
;
; Tests ExRAM mode register ($5104) behavior and Fill mode ($5106/$5107).
;
; ExRAM modes (NESdev spec):
;   Mode 0 (%00): ExRAM as nametable, CPU reads return open bus
;   Mode 1 (%01): Extended attribute mode, CPU reads return open bus
;   Mode 2 (%10): CPU RAM — full read/write via $5C00-$5FFF
;   Mode 3 (%11): Read Only — reads succeed, writes ignored
;
; Fill mode:
;   $5106 sets tile value for fill-mode nametable
;   $5107 bits 0-1 set palette index, replicated to all attribute quadrants
;
; Nametable mapping ($5105):
;   Value 0/1 = CIRAM page 0/1, 2 = ExRAM, 3 = Fill mode
;
; Tests use NT3 ($2C00) for PPU reads to avoid disturbing console output
; on NT0 ($2000). Rendering is disabled during PPU reads. $5105 is restored
; to vertical mirroring ($44) before any console output.

.include "test_macros.inc"
.include "mapper_config.inc"

.importzp ppumask_shadow

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "ExRAM m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

; Per NESdev spec note (4), CPU reads in modes 0/1 return open bus.
.define TEST_OPEN_BUS 1
EXRAM_BASE = $5C00
EXRAM_END  = $5FFF

; Nametable mapping values for $5105
; Format: DDCC_BBAA (each 2-bit field selects source for that NT quadrant)
; 0=CIRAM0, 1=CIRAM1, 2=ExRAM, 3=Fill
NT_VERTICAL    = $44        ; Standard vertical mirroring
NT_D_EXRAM     = $84        ; NT3($2C00)=ExRAM(2), rest=CIRAM vertical
NT_D_FILL      = $C4        ; NT3($2C00)=Fill(3), rest=CIRAM vertical

; PPU addresses for NT3 reads
NT3_BASE_HI    = $2C        ; NT3 at PPU $2C00
NT3_ATTR_HI    = $2F        ; NT3 attribute table at PPU $2FC0
NT3_TEST_OFF   = $0F        ; Offset within nametable for test data

.segment "ZEROPAGE"
ppu_read_result: .res 1

.segment "CODE"

; Read a byte from PPU at given address
; X = high byte, Y = low byte
; Returns value in A
.proc read_ppu
    bit PPUSTATUS
    stx PPUADDR
    sty PPUADDR
    lda PPUDATA             ; Dummy read (fills internal buffer)
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

; Set ExRAM mode ($5104)
; A = mode (0-3)
.proc set_exram_mode
    sta MMC5_EXRAM_MODE
    rts
.endproc

; Restore nametable mapping to vertical mirroring
; Must be called before any console output (start_test/pass_test)
.proc restore_nt_mapping
    lda #NT_VERTICAL
    sta MMC5_NT_MAP
    rts
.endproc

.proc run_tests
    jsr disable_rendering

    ; ========================================
    ; Group 1: Mode 2 — CPU RAM
    ; ========================================

    ; Set ExRAM to mode 2 (CPU read/write)
    lda #2
    jsr set_exram_mode

    ; --- Test 1: Write $AA to $5C00, read back ---
    start_test 1, "M2 $5C00 wr"
    lda #$AA
    sta EXRAM_BASE
    lda EXRAM_BASE
    assert_a_eq $AA
    pass_test

    ; --- Test 2: Write $55 to $5C01, read back ---
    start_test 2, "M2 $5C01 wr"
    lda #$55
    sta EXRAM_BASE+1
    lda EXRAM_BASE+1
    assert_a_eq $55
    pass_test

    ; --- Test 3: Write $33 to $5FFF (end of range), read back ---
    start_test 3, "M2 $5FFF wr"
    lda #$33
    sta EXRAM_END
    lda EXRAM_END
    assert_a_eq $33
    pass_test

    ; ========================================
    ; Group 2: Mode 3 — Read Only
    ; ========================================
    ; Per NESdev: Mode 3 is "Read Only" — reads succeed, writes ignored.

    ; Write test value in mode 2 first
    lda #2
    jsr set_exram_mode
    lda #$CC
    sta EXRAM_BASE

    ; Switch to mode 3
    lda #3
    jsr set_exram_mode

    ; --- Test 4: Data preserved from mode 2 ---
    start_test 4, "M3 read ok"
    lda EXRAM_BASE
    assert_a_eq $CC
    pass_test

    ; --- Test 5: Writes ignored in mode 3 ---
    start_test 5, "M3 wr ignor"
    lda #$DD
    sta EXRAM_BASE          ; Should be ignored
    lda EXRAM_BASE
    assert_a_eq $CC         ; Still $CC, not $DD
    pass_test

    ; ========================================
    ; Group 3: Mode 0 — Nametable
    ; ========================================

    ; Write test data to ExRAM in mode 2
    lda #2
    jsr set_exram_mode
    lda #$42
    sta EXRAM_BASE + NT3_TEST_OFF   ; $5C0F — corresponds to NT3 offset $0F

    ; Switch to mode 0 (ExRAM as nametable)
    lda #0
    jsr set_exram_mode

    ; Map NT3 ($2C00) to ExRAM
    lda #NT_D_EXRAM
    sta MMC5_NT_MAP

    ; --- Test 6: PPU reads ExRAM data via nametable ---
    ldx #NT3_BASE_HI        ; $2C
    ldy #NT3_TEST_OFF        ; $0F
    jsr read_ppu
    sta ppu_read_result

    ; Restore NT mapping before console output
    jsr restore_nt_mapping

    start_test 6, "M0 NT read"
    lda ppu_read_result
    assert_a_eq $42
    pass_test

    ; --- Test 7: CPU read in mode 0 returns open bus (not stored data) ---
    ; We wrote $42 to $5C0F in mode 2. In mode 0, CPU reads should NOT
    ; return the stored value (NESdev note 4: open bus in modes 0/1).
    .if TEST_OPEN_BUS
    start_test 7, "M0 CPU obus"
    lda #0
    jsr set_exram_mode
    lda EXRAM_BASE + NT3_TEST_OFF
    assert_a_neq $42        ; Should be open bus, not $42
    ; Restore to mode 2 for safety
    lda #2
    jsr set_exram_mode
    pass_test
    .endif

    ; ========================================
    ; Group 4: Mode 1 — Extended Attribute NT
    ; ========================================

    ; Write test data to ExRAM in mode 2
    lda #2
    jsr set_exram_mode
    lda #$A5
    sta EXRAM_BASE + NT3_TEST_OFF   ; $5C0F

    ; Switch to mode 1 (extended attributes)
    lda #1
    jsr set_exram_mode

    ; Map NT3 to ExRAM
    lda #NT_D_EXRAM
    sta MMC5_NT_MAP

    ; --- Test 8: PPU reads ExRAM data in mode 1 (nametable function) ---
    ldx #NT3_BASE_HI
    ldy #NT3_TEST_OFF
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 8, "M1 NT read"
    lda ppu_read_result
    assert_a_eq $A5
    ; Restore mode 2
    lda #2
    jsr set_exram_mode
    pass_test

    ; ========================================
    ; Group 5: Fill Mode
    ; ========================================

    ; --- Test 9: Fill tile ---
    ; Set fill tile to $42
    lda #$42
    sta MMC5_FILL_TILE

    ; Map NT3 to fill mode
    lda #NT_D_FILL
    sta MMC5_NT_MAP

    ldx #NT3_BASE_HI
    ldy #NT3_TEST_OFF
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 9, "Fill tile"
    lda ppu_read_result
    assert_a_eq $42
    pass_test

    ; --- Test 10: Fill attribute ---
    ; Set fill attribute to palette 1 ($01)
    ; Per spec: bits 0-1 replicated to all 4 quadrants
    ; %01 → %01_01_01_01 = $55
    lda #$01
    sta MMC5_FILL_ATTR

    ; Map NT3 to fill mode
    lda #NT_D_FILL
    sta MMC5_NT_MAP

    ; Read attribute table at $2FC0
    ldx #NT3_ATTR_HI        ; $2F
    ldy #$C0
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 10, "Fill attr"
    lda ppu_read_result
    assert_a_eq $55
    pass_test

    ; ========================================
    ; Group 6: Mode Switching
    ; ========================================

    ; Write known data to ExRAM offset $0F in mode 2
    lda #2
    jsr set_exram_mode
    lda #$77
    sta EXRAM_BASE + NT3_TEST_OFF

    ; Map NT3 to ExRAM
    lda #NT_D_EXRAM
    sta MMC5_NT_MAP

    ; --- Test 11: Mode 2 — NT mapped to ExRAM reads all zeros ---
    ; Per spec: "When $5104 is set to mode %10 or %11, the nametable
    ; will read as all zeros."
    ldx #NT3_BASE_HI
    ldy #NT3_TEST_OFF
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 11, "M2 NT zero"
    lda ppu_read_result
    assert_a_eq $00         ; Reads as zero in mode 2
    pass_test

    ; --- Test 12: Switch to mode 0 — same NT now reads ExRAM data ---
    lda #0
    jsr set_exram_mode

    lda #NT_D_EXRAM
    sta MMC5_NT_MAP

    ldx #NT3_BASE_HI
    ldy #NT3_TEST_OFF
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 12, "M0 NT data"
    lda ppu_read_result
    assert_a_eq $77         ; Now reads actual ExRAM data
    pass_test

    ; --- Test 13: Update data in mode 2, verify in mode 0 ---
    lda #2
    jsr set_exram_mode
    lda #$88
    sta EXRAM_BASE + NT3_TEST_OFF   ; Overwrite with new value

    lda #0
    jsr set_exram_mode

    lda #NT_D_EXRAM
    sta MMC5_NT_MAP

    ldx #NT3_BASE_HI
    ldy #NT3_TEST_OFF
    jsr read_ppu
    sta ppu_read_result

    jsr restore_nt_mapping

    start_test 13, "M0 upd dat"
    lda ppu_read_result
    assert_a_eq $88         ; Sees updated data
    ; Final cleanup
    lda #2
    jsr set_exram_mode
    pass_test

    ; ========================================
    jsr enable_rendering
    rts
.endproc

; Export unique name for combined ROM builds
run_exram = run_tests
.export run_exram

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
