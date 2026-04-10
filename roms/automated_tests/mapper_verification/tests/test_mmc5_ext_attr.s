; test_mmc5_ext_attr.s — MMC5 Extended Attribute Mode Rendering Verification
;
; Verifies that ExRAM mode 1 ($5104=%01) applies per-tile CHR bank selection
; (bits 5-0) and per-tile palette override (bits 7-6) during background
; rendering.
;
; The test fills the nametable with tile $01 and programs ExRAM to select
; three different 4KB CHR banks and palettes across the screen:
;
;   Rows  0- 9: 4KB bank 0 (solid tile),      palette 0
;   Rows 10-19: 4KB bank 2 (h-stripe tile),   palette 1
;   Rows 20-29: 4KB bank 4 (v-stripe tile),   palette 2
;
; Verification is CRC-based: the Rust test captures the rendered framebuffer
; and checks its CRC-32.  run_tests never returns — it enables rendering and
; loops forever.

.include "nes.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "ExtAttr", 0
.endif

.segment "ZEROPAGE"
; (no extra ZP needed)

.segment "CODE"

.proc run_tests
    ; Ensure PPU is in a known state: rendering off, increment by 1.
    ; Console/print_title may have changed PPUCTRL (e.g. increment by 32).
    lda #$00
    sta PPUCTRL                 ; Increment by 1, BG from $0000
    sta PPUMASK                 ; Rendering off

    ; === Load palettes ===
    ; Tile patterns use color 3 (both bitplanes set), so color 3 is the
    ; visually dominant entry.  Color 0 = background (black).
    bit PPUSTATUS               ; Reset PPU latch
    lda #$3F
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; BG palette 0: black bg, then white as color 3
    lda #$0F
    sta PPUDATA                 ; Color 0: black (bg)
    lda #$02
    sta PPUDATA                 ; Color 1: dark blue
    lda #$21
    sta PPUDATA                 ; Color 2: light blue
    lda #$30
    sta PPUDATA                 ; Color 3: white  ← solid tiles

    ; BG palette 1: black bg, then red as color 3
    lda #$0F
    sta PPUDATA
    lda #$28
    sta PPUDATA                 ; Color 1: yellow
    lda #$1A
    sta PPUDATA                 ; Color 2: green
    lda #$16
    sta PPUDATA                 ; Color 3: red    ← h-stripe tiles

    ; BG palette 2: black bg, then cyan as color 3
    lda #$0F
    sta PPUDATA
    lda #$27
    sta PPUDATA                 ; Color 1: orange
    lda #$24
    sta PPUDATA                 ; Color 2: magenta
    lda #$2C
    sta PPUDATA                 ; Color 3: cyan   ← v-stripe tiles

    ; BG palette 3: unused filler
    lda #$0F
    sta PPUDATA
    lda #$0A
    sta PPUDATA
    lda #$06
    sta PPUDATA
    lda #$14
    sta PPUDATA

    ; === Fill nametable 0 ($2000) ===
    ; 960 tile bytes = tile $01, then 64 attribute bytes = $00
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; Tiles: 3 full pages (768) + 192 remaining = 960
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

    ; Attributes: 64 bytes of $00 (palette 0 — overridden by ext attr anyway)
    lda #$00
    ldx #0
@fill_attr:
    sta PPUDATA
    inx
    cpx #64
    bne @fill_attr

    ; === Write ExRAM pattern ===
    ; Set ExRAM mode 2 (CPU R/W) for writing
    lda #$02
    sta MMC5_EXRAM_MODE

    ; Block 0: rows 0-9, offsets 0-319 ($5C00-$5D3F)
    ; ExRAM = $00: 4KB bank 0, palette 0
    ldy #0
    lda #$00
:   sta $5C00, y
    iny
    bne :-
    ; $5C00-$5CFF done (256 bytes)
    ldy #0
:   sta $5D00, y
    iny
    cpy #64
    bne :-
    ; $5D00-$5D3F done (64 bytes).  Total: 320.

    ; Block 1: rows 10-19, offsets 320-639 ($5D40-$5E7F)
    ; ExRAM = $42: 4KB bank 2, palette 1  (%01_000010)
    ldy #0
    lda #$42
:   sta $5D40, y
    iny
    cpy #192
    bne :-
    ; $5D40-$5DFF done (192 bytes)
    ldy #0
:   sta $5E00, y
    iny
    cpy #128
    bne :-
    ; $5E00-$5E7F done (128 bytes).  Total: 320.

    ; Block 2: rows 20-29, offsets 640-959 ($5E80-$5FBF)
    ; ExRAM = $84: 4KB bank 4, palette 2  (%10_000100)
    ldy #0
    lda #$84
:   sta $5E80, y
    iny
    cpy #128
    bne :-
    ; $5E80-$5EFF done (128 bytes)
    ldy #0
:   sta $5F00, y
    iny
    cpy #192
    bne :-
    ; $5F00-$5FBF done (192 bytes).  Total: 320.

    ; === Configure MMC5 for extended attribute rendering ===
    ; CHR mode 0 (8KB) — extended attributes override this to 4KB banks,
    ; but set it explicitly so the test is self-contained.
    lda #$00
    sta MMC5_CHR_MODE
    sta MMC5_CHR_A7             ; 8KB bank 0 (overridden by ext attr per-tile)

    ; Upper CHR bank bits = 0
    lda #$00
    sta MMC5_CHR_UPPER

    ; Nametable mapping: all CIRAM A (single screen)
    lda #$00
    sta MMC5_NT_MAP

    ; Switch ExRAM to mode 1 (extended attributes)
    lda #$01
    sta MMC5_EXRAM_MODE

    ; === Enable rendering ===
    bit PPUSTATUS
    lda #$00
    sta PPUSCROLL               ; X scroll = 0
    sta PPUSCROLL               ; Y scroll = 0
    lda #$00                    ; BG pattern table $0000, base NT $2000
    sta PPUCTRL
    lda #$0A                    ; Show BG + show BG in leftmost 8 px
    sta PPUMASK

    ; Loop forever — CRC verification captures the screen
@loop:
    jmp @loop
.endproc

; Export for combined builds (though rendering tests are standalone only)
run_mmc5_ext_attr = run_tests
.export run_mmc5_ext_attr

.ifndef COMBINED
; NES 2.0 header
.include "nes20_header.inc"
nes20_header
.endif

; ============================================================
; CHR tile data — distinct patterns per 8KB bank
; ============================================================

; CHR bank 0 (8KB bank 0, first half = 4KB bank 0):
; Tile $01 at offset $0010 = solid block (all pixels color 3)
.segment "CHR_SIG0"
    .res 16, $00                ; Tile $00: empty
    ; Tile $01: solid 8×8 block
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF    ; Plane 0
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF    ; Plane 1

; CHR bank 1 (8KB bank 1, first half = 4KB bank 2):
; Tile $01 at offset $0010 = horizontal stripes
.segment "CHR_SIG1"
    .res 16, $00
    ; Tile $01: horizontal stripes (rows alternate color 3 / color 0)
    .byte $FF,$00,$FF,$00,$FF,$00,$FF,$00    ; Plane 0
    .byte $FF,$00,$FF,$00,$FF,$00,$FF,$00    ; Plane 1

; CHR bank 2 (8KB bank 2, first half = 4KB bank 4):
; Tile $01 at offset $0010 = vertical stripes
.segment "CHR_SIG2"
    .res 16, $00
    ; Tile $01: vertical stripes (columns alternate color 3 / color 0)
    .byte $AA,$AA,$AA,$AA,$AA,$AA,$AA,$AA    ; Plane 0
    .byte $AA,$AA,$AA,$AA,$AA,$AA,$AA,$AA    ; Plane 1

; CHR bank 3 unused — leave empty
.segment "CHR_SIG3"
    .byte $00
