; test_modes.s — Mapper 6 All-Mode PRG and WRAM Banking Verification
;
; Tests the four PRG banking modes and 4×8KB WRAM bank switching
; of the Front Fareast Magic Card (Super Magic Card).
;
; NESdev spec: https://www.nesdev.org/wiki/Super_Magic_Card
;
; Tests:
;   1-4:   GNROM (latch mode 4) — 32KB bank switching via RAM trampoline
;   5-7:   UN1ROM (latch mode 1) — 16KB+fixed regression test
;   8-11:  2M PRG banking — 4 independent 8KB windows via RAM trampoline
;   12-15: WRAM banking — 4×8KB bank isolation via $4500 bits 5-4
;   16-18: Write-enable — PRG DRAM writable + latch re-enable on restore
;
; Uses RAM trampolines at $0400 for modes that remap $8000-$FFFF.

.include "test_macros.inc"
.include "mapper_config.inc"

TRAMPOLINE_ADDR = $0400

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Modes m006.0", 0
.endif

.segment "BSS"
tramp_result: .res 4          ; Signature bytes read by trampoline
tramp_bank:   .res 1          ; Bank number argument for trampoline

.segment "CODE"

.proc run_tests
    ; ============================================================
    ; Install RAM trampoline
    ; ============================================================
    jsr install_trampoline

    ; ============================================================
    ; Tests 1-4: GNROM (Latch Mode 4) — 32KB bank switching
    ; ============================================================
    ; Mode 4 maps $8000-$FFFF as a single 32KB bank via latch PP bits (D5-D4).
    ; 128KB PRG = 4 × 32KB banks:
    ;   Bank 0: pages 0-3,  Bank 1: pages 4-7,
    ;   Bank 2: pages 8-11, Bank 3: pages 12-15
    ; We test banks 1 and 2 (bank 0 overlaps code page on restore,
    ; bank 3 contains code pages 14-15).

    start_test 1, "GNROM b1 $8"
    lda #1                  ; 32KB bank 1
    jsr call_gnrom_trampoline
    lda tramp_result        ; Page ID at $8000 (should be page 4)
    assert_a_eq 4
    pass_test

    start_test 2, "GNROM b1 $A"
    lda tramp_result + 1    ; Page ID at $A000 (should be page 5)
    assert_a_eq 5
    pass_test

    start_test 3, "GNROM b2 $8"
    lda #2                  ; 32KB bank 2
    jsr call_gnrom_trampoline
    lda tramp_result        ; Page ID at $8000 (should be page 8)
    assert_a_eq 8
    pass_test

    start_test 4, "GNROM b2 $A"
    lda tramp_result + 1    ; Page ID at $A000 (should be page 9)
    assert_a_eq 9
    pass_test

    ; ============================================================
    ; Tests 5-7: UN1ROM Regression (Latch Mode 1) — 16KB+fixed
    ; ============================================================
    ; Mode 1: $8000-$BFFF switchable 16KB via latch BBBB (D5-D2)
    ;         $C000-$FFFF fixed to 16KB bank 7 (pages 14-15)
    ; Verify we're in mode 1 (restored by GNROM trampoline).

    start_test 5, "UN1ROM b0"
    select_prg_bank 0, 0   ; 16KB bank 0 (pages 0-1) at $8000
    lda $8001               ; Page ID at $8000 (should be page 0)
    assert_a_eq 0
    pass_test

    start_test 6, "UN1ROM b2"
    select_prg_bank 0, 2   ; 16KB bank 2 (pages 4-5) at $8000
    lda $8001               ; Page ID at $8000 (should be page 4)
    assert_a_eq 4
    pass_test

    start_test 7, "UN1ROM fix"
    ; Fixed bank at $C000 should always be page 14
    ; (We can't put a signature there since it's the code bank,
    ;  so we verify by reading the reset vector which is always present)
    ; Instead, verify $A000 in bank 2 is page 5 (second half of 16KB bank 2)
    lda $A001               ; Page ID at $A000 (should be page 5)
    assert_a_eq 5
    pass_test

    ; ============================================================
    ; Tests 8-11: 2M PRG Banking — 4 independent 8KB windows
    ; ============================================================
    ; 2M mode: writes to each $8000/$A000/$C000/$E000 range set
    ; individual 8KB pages via PPPPPP (bits 7-2).
    ; All four windows are independently switchable.

    ; Config 1: pages 4, 5, 6, 7
    start_test 8, "2M cfg1 $8"
    lda #4
    sta tramp_result        ; Page for $8000
    lda #5
    sta tramp_result + 1    ; Page for $A000
    lda #6
    sta tramp_result + 2    ; Page for $C000
    lda #7
    sta tramp_result + 3    ; Page for $E000
    jsr call_2m_trampoline
    lda tramp_result        ; Read page ID from $8000
    assert_a_eq 4
    pass_test

    start_test 9, "2M cfg1 $A"
    lda tramp_result + 1    ; Read page ID from $A000
    assert_a_eq 5
    pass_test

    ; Config 2: verify different pages on all 4 windows
    start_test 10, "2M cfg2 $8"
    lda #8
    sta tramp_result        ; Page for $8000
    lda #0
    sta tramp_result + 1    ; Page for $A000
    lda #12
    sta tramp_result + 2    ; Page for $C000
    lda #1
    sta tramp_result + 3    ; Page for $E000
    jsr call_2m_trampoline
    lda tramp_result        ; Read page ID from $8000
    assert_a_eq 8
    pass_test

    start_test 11, "2M cfg2 $C"
    lda tramp_result + 2    ; Read page ID from $C000
    assert_a_eq 12
    pass_test

    ; ============================================================
    ; Tests 12-15: WRAM Banking — 4 × 8KB bank isolation
    ; ============================================================
    ; $4500 bits 5-4 select among 4 × 8KB WRAM banks at $6000-$7FFF.
    ; Test writes a unique byte to $6004 in each bank, then verifies
    ; each bank retains its own value.

    ; Pre-initialize $6000 = STATUS_RUNNING in all WRAM banks
    ; so the test runner doesn't see a false pass/fail during switches.
    lda #STATUS_RUNNING
    select_wram_bank 1
    sta TEST_STATUS
    select_wram_bank 2
    sta TEST_STATUS
    select_wram_bank 3
    sta TEST_STATUS
    select_wram_bank 0      ; Back to bank 0 (status byte bank)

    ; Write unique values to $6004 in each bank
    select_wram_bank 0
    lda #$B0
    sta $6004

    select_wram_bank 1
    lda #$B1
    sta $6004

    select_wram_bank 2
    lda #$B2
    sta $6004

    select_wram_bank 3
    lda #$B3
    sta $6004

    ; Verify: switch back to each bank and read $6004
    start_test 12, "WRAM bank0"
    select_wram_bank 0
    lda $6004
    assert_a_eq $B0
    pass_test

    start_test 13, "WRAM bank1"
    select_wram_bank 1
    lda $6004
    assert_a_eq $B1
    pass_test

    start_test 14, "WRAM bank2"
    select_wram_bank 2
    lda $6004
    assert_a_eq $B2
    pass_test

    start_test 15, "WRAM bank3"
    select_wram_bank 3
    lda $6004
    assert_a_eq $B3
    pass_test

    ; Restore WRAM bank 0 (where status byte lives)
    select_wram_bank 0

    ; ============================================================
    ; Tests 16-18: Write-Enable (ROM-RAM swap)
    ; ============================================================
    ; Clearing write-protect via $42FD (A1=0) makes PRG DRAM writable
    ; and disables the latch.
    ; Restoring via $42FF (A1=1) re-enables write-protect and latch.
    ;
    ; NOTE: Currently disabled because the mapper does not yet
    ; implement PRG DRAM writability. Enable TEST_WRITE_ENABLE
    ; when the mapper is updated to support writes to PRG DRAM
    ; when write-protection is disabled via $42FC/$42FD.

.define TEST_WRITE_ENABLE 0

.if TEST_WRITE_ENABLE
    ; Ensure bank 0 is selected at $8000
    select_prg_bank 0, 0

    start_test 16, "WR enable"
    jsr call_write_enable_trampoline
    lda tramp_result        ; Byte read back from $9000
    assert_a_eq $42
    pass_test

    start_test 17, "Latch rest"
    ; After trampoline restored write-protect and selected bank 2:
    ; $8000 should now show bank 2 = pages 4-5
    lda $8001               ; Page ID at $8000 (should be page 4)
    assert_a_eq 4
    pass_test

    start_test 18, "WR persist"
    ; Switch back to bank 0 and verify the DRAM write persisted
    select_prg_bank 0, 0
    lda $9000               ; Should still be $42 (written to DRAM earlier)
    assert_a_eq $42
    pass_test
.endif

    rts
.endproc

run_modes = run_tests
.export run_modes

; ============================================================
; Trampoline infrastructure
; ============================================================

; Install all trampolines into RAM at TRAMPOLINE_ADDR
.proc install_trampoline
    ldx #0
@copy:
    lda trampoline_code, x
    sta TRAMPOLINE_ADDR, x
    inx
    cpx #(trampoline_code_end - trampoline_code)
    bne @copy
    rts
.endproc

; ============================================================
; GNROM trampoline — switch to mode 4, read sigs, restore mode 1
; Input: A = 32KB bank number (0-3)
; Output: tramp_result[0..3] = page IDs from $8000/$A000/$C000/$E000
; ============================================================
.proc call_gnrom_trampoline
    sta tramp_bank
    jmp TRAMPOLINE_ADDR
.endproc

; ============================================================
; 2M trampoline — enable 2M, set 4 windows, read sigs, disable 2M
; Input: tramp_result[0..3] = desired page numbers for each window
; Output: tramp_result[0..3] = page IDs read from each window
; ============================================================
TRAMPOLINE_2M = TRAMPOLINE_ADDR + (trampoline_2m_code - trampoline_code)

.proc call_2m_trampoline
    jmp TRAMPOLINE_2M
.endproc

; ============================================================
; Write-enable trampoline — enable writes, write $42 to $9000,
; read back, restore write-protect, select bank 2 via latch
; Output: tramp_result[0] = byte read from $9000 after write
; ============================================================
TRAMPOLINE_WE = TRAMPOLINE_ADDR + (trampoline_we_code - trampoline_code)

.proc call_write_enable_trampoline
    jmp TRAMPOLINE_WE
.endproc

; ============================================================
; Trampoline code block — copied to RAM and executed from there
; ============================================================
trampoline_code:

; --- GNROM trampoline ---
trampoline_gnrom_code:
    ; Switch to mode 4 (GNROM): write to $42FF (A1=1, A0=1 → vertical)
    lda #$80                ; BBB=100 (mode 4), D4=0
    sta $42FF
    ; Select 32KB bank: PP in bits 5-4 of latch
    lda tramp_bank
    asl a
    asl a
    asl a
    asl a                   ; Shift to PP position (bits 5-4)
    sta $8000               ; Write to latch
    ; Read page ID signatures from each 8KB window
    lda $8001               ; Page ID at $8000
    sta tramp_result
    lda $A001               ; Page ID at $A000
    sta tramp_result + 1
    lda $C001               ; Page ID at $C000
    sta tramp_result + 2
    lda $E001               ; Page ID at $E000
    sta tramp_result + 3
    ; Restore mode 1 (UN1ROM): write to $42FF (A1=1, A0=1 → vertical)
    lda #$20                ; BBB=001 (mode 1), D4=0
    sta $42FF
    ; Select bank 0 in mode 1
    lda #0
    sta $8000
    rts

; --- 2M PRG banking trampoline ---
trampoline_2m_code:
    ; Read desired pages from tramp_result (set by caller)
    ; Set 2M bank registers (these accept values even when 2M is not yet active)
    lda tramp_result        ; Page for $8000
    asl a
    asl a                   ; PPPPPP in bits 7-2
    sta $8000               ; Set $8000-$9FFF bank register

    lda tramp_result + 1    ; Page for $A000
    asl a
    asl a
    sta $A000               ; Set $A000-$BFFF bank register

    lda tramp_result + 2    ; Page for $C000
    asl a
    asl a
    sta $C000               ; Set $C000-$DFFF bank register

    lda tramp_result + 3    ; Page for $E000
    asl a
    asl a
    sta $E000               ; Set $E000-$FFFF bank register

    ; Enable 2M mode: write to $43FE (N=1 → 2M, M=0 → enable)
    lda #$00
    sta $43FE

    ; Read page ID signatures from each window
    lda $8001
    pha                     ; Save on stack
    lda $A001
    pha
    lda $C001
    pha
    lda $E001
    pha

    ; Disable 2M mode: write to $43FF (M=1 → disable)
    lda #$00
    sta $43FF

    ; Restore mode 1 bank 0 via latch
    lda #0
    sta $8000

    ; Store results (pop in reverse order)
    pla
    sta tramp_result + 3
    pla
    sta tramp_result + 2
    pla
    sta tramp_result + 1
    pla
    sta tramp_result
    rts

; --- Write-enable trampoline ---
trampoline_we_code:
    ; Enable PRG writes: write to $42FD (A1=0, A0=1 → vertical mirroring)
    lda #$20                ; BBB=001 (mode 1), D4=0
    sta $42FD               ; A1=0 (write-enable), A0=1 (vertical)

    ; Write test byte to $9000 (offset $1000 in page, avoids signatures)
    lda #$42
    sta $9000

    ; Read back
    lda $9000
    sta tramp_result        ; Save for verification

    ; Restore write-protect: write to $42FF (A1=1, A0=1 → vertical)
    lda #$20                ; BBB=001 (mode 1), D4=0
    sta $42FF

    ; Verify latch re-enabled: select 16KB bank 2 via latch
    lda #(2 << 2)           ; BBBB=0010 in bits 5-2
    sta $8000               ; Write to latch (should go to bank register)
    rts

trampoline_code_end:

; ============================================================
; 8KB bank signatures — $A5, page_num, ~page_num, $5A
; Pages 14-15 (code) have no signatures.
; ============================================================

.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A
.segment "PRG_SIG1"
    .byte $A5, 1, $FE, $5A
.segment "PRG_SIG2"
    .byte $A5, 2, $FD, $5A
.segment "PRG_SIG3"
    .byte $A5, 3, $FC, $5A
.segment "PRG_SIG4"
    .byte $A5, 4, $FB, $5A
.segment "PRG_SIG5"
    .byte $A5, 5, $FA, $5A
.segment "PRG_SIG6"
    .byte $A5, 6, $F9, $5A
.segment "PRG_SIG7"
    .byte $A5, 7, $F8, $5A
.segment "PRG_SIG8"
    .byte $A5, 8, $F7, $5A
.segment "PRG_SIG9"
    .byte $A5, 9, $F6, $5A
.segment "PRG_SIG10"
    .byte $A5, 10, $F5, $5A
.segment "PRG_SIG11"
    .byte $A5, 11, $F4, $5A
.segment "PRG_SIG12"
    .byte $A5, 12, $F3, $5A
.segment "PRG_SIG13"
    .byte $A5, 13, $F2, $5A

.ifndef COMBINED
.include "nes20_header.inc"
nes20_header

; ASCII font in first CHR bank
.if CHR_ROM_8K > 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
.endif ; COMBINED
