; test_mmc5_split.s — MMC5 Vertical Split Screen Rendering Verification
;
; Verifies that the MMC5 vertical split screen ($5200-$5202) divides the
; display into a split region and a main region, where the split region:
;   - Reads nametable data from ExRAM (regardless of $5105)
;   - Uses the CHR bank from $5202
;   - Applies the vertical scroll from $5201
;
; Setup:
;   Main region (right): CIRAM nametable, tile $01 (solid), CHR bank 0
;   Split region (left 16 tiles): ExRAM nametable, tile $02 (h-stripes),
;       CHR bank 2 ($5202=2), vertical scroll 32 ($5201=$20)
;
; Verification is CRC-based.  run_tests never returns.

.include "nes.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "Split", 0
.endif

.segment "ZEROPAGE"
; (no extra ZP needed)

.segment "CODE"

.proc run_tests
    ; === Load palettes ===
    bit PPUSTATUS
    lda #$3F
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; BG palette 0: black, white, light blue, dark blue (main region)
    lda #$0F
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$21
    sta PPUDATA
    lda #$02
    sta PPUDATA

    ; BG palette 1: black, red, green, yellow (split region)
    lda #$0F
    sta PPUDATA
    lda #$16
    sta PPUDATA
    lda #$1A
    sta PPUDATA
    lda #$28
    sta PPUDATA

    ; BG palettes 2-3: filler
    lda #$0F
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$0F
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA

    ; === Fill CIRAM nametable 0 ($2000) with tile $01 ===
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; 960 tile bytes
    lda #$01
    ldx #0
    ldy #3
@fill_tiles:
    sta PPUDATA
    inx
    bne @fill_tiles
    dey
    bne @fill_tiles
    ldx #0
@fill_tiles2:
    sta PPUDATA
    inx
    cpx #192
    bne @fill_tiles2

    ; 64 attribute bytes = $00 (palette 0)
    lda #$00
    ldx #0
@fill_attr:
    sta PPUDATA
    inx
    cpx #64
    bne @fill_attr

    ; === Write ExRAM as split nametable ===
    ; ExRAM mode 2 (CPU R/W) for writing
    lda #$02
    sta MMC5_EXRAM_MODE

    ; Fill ExRAM tile area (960 bytes at $5C00-$5FBF) with tile $02
    ldy #0
    lda #$02
:   sta $5C00, y
    iny
    bne :-
    ldy #0
:   sta $5D00, y
    iny
    bne :-
    ldy #0
:   sta $5E00, y
    iny
    bne :-
    ldy #0
:   sta $5F00, y
    iny
    cpy #192
    bne :-

    ; Fill ExRAM attribute area ($5FC0-$5FFF, 64 bytes) with $55
    ; $55 = %01010101 → palette 1 for all quadrants
    ldy #0
    lda #$55
:   sta $5FC0, y
    iny
    cpy #64
    bne :-

    ; === Configure MMC5 ===
    ; ExRAM mode 0 (nametable mode) — required for split (mode < 2)
    lda #$00
    sta MMC5_EXRAM_MODE

    ; Nametable mapping: all CIRAM A
    lda #$00
    sta MMC5_NT_MAP

    ; CHR mode 0 (8KB), bank 0 for main region
    lda #$00
    sta MMC5_CHR_MODE
    sta MMC5_CHR_A7             ; 8KB bank 0

    ; Upper CHR bank bits = 0
    lda #$00
    sta MMC5_CHR_UPPER

    ; Split: enable, left side, threshold = 16 tiles
    ; $5200 = %1_0_0_10000 = $90  (E=1, S=0=left, T=16)
    lda #$90
    sta MMC5_SPLIT_MODE

    ; Split vertical scroll = 32 pixels
    lda #$20
    sta MMC5_SPLIT_SCROLL

    ; Split CHR bank = 4KB bank 2 (first half of CHR_B1)
    lda #$02
    sta MMC5_SPLIT_BANK

    ; === Enable rendering ===
    bit PPUSTATUS
    lda #$00
    sta PPUSCROLL
    sta PPUSCROLL
    lda #$00                    ; BG from $0000, base NT $2000
    sta PPUCTRL
    lda #$0A                    ; Show BG + leftmost 8px
    sta PPUMASK

@loop:
    jmp @loop
.endproc

run_mmc5_split = run_tests
.export run_mmc5_split

.ifndef COMBINED
.include "nes20_header.inc"
nes20_header
.endif

; ============================================================
; CHR tile data
; ============================================================

; CHR bank 0 (main region): tile $01 = solid block, tile $02 = solid block
.segment "CHR_SIG0"
    .res 16, $00                ; Tile $00: empty
    ; Tile $01: solid block
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    ; Tile $02: solid block (same, for fallback)
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF

; CHR bank 1 (split region, 4KB bank 2): tile $02 = horizontal stripes
.segment "CHR_SIG1"
    .res 16, $00                ; Tile $00: empty
    .res 16, $00                ; Tile $01: empty (not used in split)
    ; Tile $02: horizontal stripes
    .byte $FF,$00,$FF,$00,$FF,$00,$FF,$00
    .byte $FF,$00,$FF,$00,$FF,$00,$FF,$00

; CHR banks 2-3: unused
.segment "CHR_SIG2"
    .byte $00
.segment "CHR_SIG3"
    .byte $00
