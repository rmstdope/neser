; test_mmc5_sprite_chr.s — MMC5 8×16 Sprite CHR A/B Separation Rendering
;
; Verifies that when 8×16 sprites are enabled (PPUCTRL bit 5) and CHR mode
; is 3 (1KB), the MMC5 uses A registers ($5120-$5127) for sprite tile
; fetches and B registers ($5128-$512B) for background tile fetches during
; rendering.
;
; Setup:
;   CHR mode 3 (1KB banking)
;   B registers → 1KB banks 0-3 (CHR_B0: solid block tiles)
;   A registers → 1KB banks 16-23 (CHR_B2: checkerboard tiles)
;   Background: nametable filled with tile $01 → solid via B registers
;   Sprites: 8×16 sprites using tile $02 → checkerboard via A registers
;
; The visual result shows solid background with checkerboard sprites,
; proving A/B separation during rendering.
;
; Verification is CRC-based.  run_tests never returns.

.include "nes.inc"
.include "mapper_config.inc"

.ifndef COMBINED
.export run_tests
.export test_title_string

.segment "RODATA"
test_title_string:
    .byte "SprCHR", 0
.endif

.segment "ZEROPAGE"
; (no extra ZP needed)

.segment "CODE"

.proc run_tests
    ; Ensure PPU is in a known state: rendering off, increment by 1.
    lda #$00
    sta PPUCTRL
    sta PPUMASK

    ; === Load palettes ===
    ; Color 3 is the visible color (both bitplanes set in tile data).
    bit PPUSTATUS
    lda #$3F
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; BG palette 0: white solid tiles (background)
    lda #$0F
    sta PPUDATA                 ; Color 0: black (bg)
    lda #$02
    sta PPUDATA                 ; Color 1: dark blue
    lda #$21
    sta PPUDATA                 ; Color 2: light blue
    lda #$30
    sta PPUDATA                 ; Color 3: white  ← BG tiles

    ; BG palettes 1-3: filler (white)
    ldx #0
@bg_pal_fill:
    lda #$0F
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    inx
    cpx #3
    bne @bg_pal_fill

    ; Sprite palette 0: red checkerboard sprites
    lda #$0F
    sta PPUDATA                 ; Color 0: transparent
    lda #$28
    sta PPUDATA                 ; Color 1: yellow
    lda #$1A
    sta PPUDATA                 ; Color 2: green
    lda #$16
    sta PPUDATA                 ; Color 3: red    ← sprite tiles

    ; Sprite palettes 1-3: filler
    ldx #0
@spr_pal_fill:
    lda #$0F
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    lda #$30
    sta PPUDATA
    inx
    cpx #3
    bne @spr_pal_fill

    ; === Fill nametable 0 with tile $01 ===
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR

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

    ; Attributes: 64 bytes of $00
    lda #$00
    ldx #0
@fill_attr:
    sta PPUDATA
    inx
    cpx #64
    bne @fill_attr

    ; === Set up OAM for 8×16 sprites at $0200 ===
    ; First clear all 64 sprites (Y=$EF = off screen)
    ldx #0
@clear_oam:
    lda #$EF
    sta $0200, x            ; Y = $EF (hidden)
    inx
    lda #$00
    sta $0200, x            ; Tile = 0
    inx
    sta $0200, x            ; Attr = 0
    inx
    sta $0200, x            ; X = 0
    inx
    bne @clear_oam          ; Branch on X wrapping to 0

    ; Place 8 visible 8×16 sprites in a 4×2 grid
    ; Tile byte for 8×16: bit 0 = pattern table (0=$0000),
    ;                     bits 7-1 = top tile (tile $01 → byte $02)
    ;
    ; Sprite 0: Y=48, Tile=$02, Attr=$00, X=80
    lda #48
    sta $0200
    lda #$02
    sta $0201
    lda #$00
    sta $0202
    lda #80
    sta $0203

    ; Sprite 1: Y=48, Tile=$02, Attr=$00, X=96
    lda #48
    sta $0204
    lda #$02
    sta $0205
    lda #$00
    sta $0206
    lda #96
    sta $0207

    ; Sprite 2: Y=48, Tile=$02, Attr=$00, X=112
    lda #48
    sta $0208
    lda #$02
    sta $0209
    lda #$00
    sta $020A
    lda #112
    sta $020B

    ; Sprite 3: Y=48, Tile=$02, Attr=$00, X=128
    lda #48
    sta $020C
    lda #$02
    sta $020D
    lda #$00
    sta $020E
    lda #128
    sta $020F

    ; Sprite 4: Y=80, Tile=$02, Attr=$00, X=80
    lda #80
    sta $0210
    lda #$02
    sta $0211
    lda #$00
    sta $0212
    lda #80
    sta $0213

    ; Sprite 5: Y=80, Tile=$02, Attr=$00, X=96
    lda #80
    sta $0214
    lda #$02
    sta $0215
    lda #$00
    sta $0216
    lda #96
    sta $0217

    ; Sprite 6: Y=80, Tile=$02, Attr=$00, X=112
    lda #80
    sta $0218
    lda #$02
    sta $0219
    lda #$00
    sta $021A
    lda #112
    sta $021B

    ; Sprite 7: Y=80, Tile=$02, Attr=$00, X=128
    lda #80
    sta $021C
    lda #$02
    sta $021D
    lda #$00
    sta $021E
    lda #128
    sta $021F

    ; === Configure MMC5 CHR banking ===
    ; CHR mode 3 (1KB)
    lda #3
    sta MMC5_CHR_MODE

    ; B registers ($5128-$512B): 1KB banks 0-3 (from CHR_B0 — solid tiles)
    lda #0
    sta $5128               ; B0 = bank 0
    lda #1
    sta $5129               ; B1 = bank 1
    lda #2
    sta $512A               ; B2 = bank 2
    lda #3
    sta $512B               ; B3 = bank 3

    ; A registers ($5120-$5127): 1KB banks 16-23 (from CHR_B2 — checkerboard tiles)
    lda #16
    sta $5120               ; A0 = bank 16
    lda #17
    sta $5121               ; A1 = bank 17
    lda #18
    sta $5122               ; A2 = bank 18
    lda #19
    sta $5123               ; A3 = bank 19
    lda #20
    sta $5124               ; A4 = bank 20
    lda #21
    sta $5125               ; A5 = bank 21
    lda #22
    sta $5126               ; A6 = bank 22
    lda #23
    sta $5127               ; A7 = bank 23

    ; Upper CHR bank bits = 0
    lda #$00
    sta MMC5_CHR_UPPER

    ; Nametable mapping: all CIRAM A
    lda #$00
    sta MMC5_NT_MAP

    ; ExRAM mode 0 (nametable — unused, but keep it neutral)
    lda #$00
    sta MMC5_EXRAM_MODE

    ; === OAM DMA ===
    lda #0
    sta OAMADDR
    lda #$02
    sta OAMDMA              ; DMA from $0200

    ; === Enable rendering ===
    bit PPUSTATUS
    lda #$00
    sta PPUSCROLL
    sta PPUSCROLL
    lda #$20                ; BG from $0000, 8×16 sprites (bit 5)
    sta PPUCTRL
    lda #$1E                ; Show BG + sprites + leftmost 8px
    sta PPUMASK

@loop:
    jmp @loop
.endproc

run_mmc5_sprite_chr = run_tests
.export run_mmc5_sprite_chr

.ifndef COMBINED
.include "nes20_header.inc"
nes20_header
.endif

; ============================================================
; CHR tile data
; ============================================================

; CHR bank 0 (B registers — background):
; Tile $01 = solid block, Tile $02 = solid block
.segment "CHR_SIG0"
    .res 16, $00                ; Tile $00: empty
    ; Tile $01: solid block (color 3)
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    ; Tile $02: solid block (color 3)
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF
    .byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF

; CHR bank 1: unused (not referenced by A or B registers in this test)
.segment "CHR_SIG1"
    .byte $00

; CHR bank 2 (A registers — sprites):
; Tile $01 = checkerboard, Tile $02 = checkerboard, Tile $03 = checkerboard
; (8×16 sprites use tile pairs: $02=top, $03=bottom)
.segment "CHR_SIG2"
    .res 16, $00                ; Tile $00: empty
    ; Tile $01: checkerboard (color 3 / color 0)
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55
    ; Tile $02: checkerboard (top half of 8×16 sprite)
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55
    ; Tile $03: checkerboard (bottom half of 8×16 sprite)
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55
    .byte $AA,$55,$AA,$55,$AA,$55,$AA,$55

; CHR bank 3: unused
.segment "CHR_SIG3"
    .byte $00
