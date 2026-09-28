; test_bus_conflicts.s — Bus Conflict Verification
;
; The test uses a lookup table where byte at offset N contains value N.
; It then tests:
;   1-2. Writing matching value (N to offset N) → selects bank N either way
;   3-4. Writing mismatched value:
;        HAS_BUS_CONFLICTS = 1 (AND-type bus conflicts, e.g. NES 2.0
;          submapper 2 of mappers 2/3/7):
;            effective_value = written_value AND ROM_value_at_write_address
;        HAS_BUS_CONFLICTS = 0 (bus conflicts do not occur, e.g. NES 2.0
;          submapper 1 of mappers 2/3/7):
;            effective_value = written_value (unmasked)
;
; Only build this aspect for a mapper whose bus-conflict behaviour is
; specified. Never build it for a submapper where the behaviour is unknown
; (e.g. submapper 0 of mappers 2/3/7).

.include "test_macros.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Bus Conf m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0
.endif

.segment "CODE"

.proc run_tests
    .if PRG_BANK_SIZE = 32
        jsr install_bus_conflict_trampoline
    .endif

    ; ========================================
    ; Test 1: Safe write (value matches ROM)
    ; ========================================
    start_test 1, "Safe write"

    ; bank_table[0] = 0, so writing 0 to bank_table+0 → 0 AND 0 = 0
    .if HAS_PRG_BANKING
        ; For PRG banking mappers (UxROM, AxROM)
        ; 32KB mappers place executable code in bank 0, so select bank 1
        ; where the test's PRG signature is available.
        .if PRG_BANK_SIZE = 32
        lda #1
        ldx #1
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result
        assert_a_eq $A5
        .else
        ; Select bank 0 via safe write
        lda #0
        sta bank_table          ; Write 0 to address containing 0
        ; Verify bank 0 is selected by reading signature
        lda $8000
        assert_a_eq $A5         ; Bank 0 signature
        .endif
    .else
        ; For CHR banking mappers (CNROM)
        ; Select CHR bank 0 via safe write
        lda #0
        sta bank_table
        ; Read CHR to verify (need PPU access)
        lda #0
        sta PPUMASK             ; Disable rendering
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$00
        sta PPUADDR
        lda PPUDATA             ; Dummy
        lda PPUDATA
        assert_a_eq $B6         ; CHR bank 0 signature
    .endif
    pass_test

    ; ========================================
    ; Test 2: Another safe write
    ; ========================================
    start_test 2, "Safe write 2"

    .if HAS_PRG_BANKING
        ; Select bank 1 for 16KB mappers, bank 2 for 32KB mappers.
        .if PRG_BANK_SIZE = 32
        lda #2
        ldx #2
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result + 1
        assert_a_eq 2
        .else
        lda #1
        sta bank_table + 1
        lda $8000 + 1           ; Bank ID byte
        assert_a_eq 1
        .endif
    .else
        lda #1
        sta bank_table + 1
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$01
        sta PPUADDR
        lda PPUDATA
        lda PPUDATA
        assert_a_eq 1           ; CHR bank 1 ID
    .endif
    pass_test

.if HAS_BUS_CONFLICTS
    ; ========================================
    ; Test 3: Conflict write (AND behavior)
    ; ========================================
    start_test 3, "AND behavior"

    ; bank_table[2] = 2 = %00000010
    ; Write 3 = %00000011 to bank_table+2
    ; Effective = 3 AND 2 = 2
    .if HAS_PRG_BANKING
        lda #3
        .if PRG_BANK_SIZE = 32
        ldx #2
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result + 1
        assert_a_eq 2
        .else
        sta bank_table + 2      ; 3 AND 2 = 2
        lda $8000 + 1           ; Should be bank 2
        assert_a_eq 2
        .endif
    .else
        lda #3
        sta bank_table + 2
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$01
        sta PPUADDR
        lda PPUDATA
        lda PPUDATA
        assert_a_eq 2
    .endif
    pass_test

    ; ========================================
    ; Test 4: Conflict zeroing
    ; ========================================
    start_test 4, "AND zero"

    ; bank_table[0] = 0 = %00000000
    ; Write any non-zero value → effective = X AND 0 = 0
    .if HAS_PRG_BANKING
        .if PRG_BANK_SIZE = 32
        ; Bank 0 carries executable code rather than a signature. Use bank
        ; table byte 2 to verify that extra written bits are masked away.
        lda #7                  ; 7 AND 2 = 2
        ldx #2
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result + 1
        assert_a_eq 2
        .else
        lda #7                  ; Write 7 to address containing 0
        sta bank_table          ; 7 AND 0 = 0
        lda $8000 + 1           ; Should be bank 0
        assert_a_eq 0
        .endif
    .else
        lda #7
        sta bank_table
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$01
        sta PPUADDR
        lda PPUDATA
        lda PPUDATA
        assert_a_eq 0
    .endif
    pass_test
.else
    ; ========================================
    ; Test 3: Mismatched write is not masked
    ; ========================================
    start_test 3, "No AND mask"

    ; bank_table[2] = 2 = %00000010
    ; Write 3 = %00000011 to bank_table+2
    ; No bus conflicts → effective = 3 (AND would give 2)
    .if HAS_PRG_BANKING
        lda #3
        .if PRG_BANK_SIZE = 32
        ldx #2
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result + 1
        assert_a_eq 3
        .else
        sta bank_table + 2
        lda $8000 + 1           ; Should be bank 3
        assert_a_eq 3
        .endif
    .else
        lda #3
        sta bank_table + 2
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$01
        sta PPUADDR
        lda PPUDATA
        lda PPUDATA
        assert_a_eq 3
    .endif
    pass_test

    ; ========================================
    ; Test 4: Write over a zero byte is not zeroed
    ; ========================================
    start_test 4, "No AND zero"

    ; bank_table[0] = 0 = %00000000
    ; Write 1 to bank_table+0
    ; No bus conflicts → effective = 1 (AND would give 0)
    .if HAS_PRG_BANKING
        lda #1
        .if PRG_BANK_SIZE = 32
        ldx #0
        jsr call_bus_conflict_trampoline
        lda bus_conflict_result + 1
        assert_a_eq 1
        .else
        sta bank_table
        lda $8000 + 1           ; Should be bank 1
        assert_a_eq 1
        .endif
    .else
        lda #1
        sta bank_table
        bit PPUSTATUS
        lda #$00
        sta PPUADDR
        lda #$01
        sta PPUADDR
        lda PPUDATA
        lda PPUDATA
        assert_a_eq 1
    .endif
    pass_test
.endif

    .if HAS_PRG_BANKING
        ; Restore to a sensible bank
        lda #0
        sta bank_table
    .else
        lda #0
        sta bank_table
        lda #%00001010
        sta PPUMASK
        lda #0
        sta PPUSCROLL
        sta PPUSCROLL
    .endif

    rts
.endproc

; Export unique name for combined ROM builds
run_bus_conflicts = run_tests
.export run_bus_conflicts

.if PRG_BANK_SIZE = 32

.segment "BSS"
bus_conflict_result: .res 2
bus_conflict_value: .res 1
bus_conflict_offset: .res 1

BUS_CONFLICT_TRAMPOLINE_ADDR = $0300
BUS_CONFLICT_TRAMPOLINE_SIZE = bus_conflict_trampoline_end - bus_conflict_trampoline

.segment "CODE"

.proc install_bus_conflict_trampoline
    ldx #0
@copy:
    lda bus_conflict_trampoline, x
    sta BUS_CONFLICT_TRAMPOLINE_ADDR, x
    inx
    cpx #BUS_CONFLICT_TRAMPOLINE_SIZE
    bne @copy
    rts
.endproc

.proc call_bus_conflict_trampoline
    sta bus_conflict_value
    txa
    sta bus_conflict_offset
    lda #<bank_table
    clc
    adc bus_conflict_offset
    sta TEST_TEMP
    lda #>bank_table
    adc #0
    sta TEST_TEMP2
    jmp BUS_CONFLICT_TRAMPOLINE_ADDR
.endproc

bus_conflict_trampoline:
    ldy #0
    lda bus_conflict_value
    sta (TEST_TEMP), y
    lda $8000
    sta bus_conflict_result
    lda $8001
    sta bus_conflict_result + 1
    lda #0
    sta $FFF0
    rts
bus_conflict_trampoline_end:

.endif

; ============================================================
; Bus conflict lookup table
; Byte at offset N = N, so writing N to (bank_table+N) is safe
; ============================================================
.segment "RODATA"
.export bank_table
bank_table:
    .repeat 16, i
        .byte i
    .endrepeat

; PRG bank signatures (for PRG banking mappers)
.if HAS_PRG_BANKING
.if PRG_BANK_SIZE <> 32
.segment "PRG_SIG0"
    .byte $A5, 0, $FF, $5A
.endif
.segment "PRG_SIG1"
    .byte $A5, 1, $FE, $5A
.if PRG_BANK_SIZE <> 32 .or PRG_ROM_16K >= 6
.segment "PRG_SIG2"
    .byte $A5, 2, $FD, $5A
.endif
.if PRG_BANK_SIZE <> 32 .or PRG_ROM_16K >= 8
.segment "PRG_SIG3"
    .byte $A5, 3, $FC, $5A
.endif
.endif

; CHR bank signatures (for CHR banking mappers)
.if HAS_CHR_BANKING
.segment "CHR_SIG0"
    .byte $B6, 0, $FF, $6B
.segment "CHR_SIG1"
    .byte $B6, 1, $FE, $6B
.segment "CHR_SIG2"
    .byte $B6, 2, $FD, $6B
.segment "CHR_SIG3"
    .byte $B6, 3, $FC, $6B
.endif

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
