; console.s — Simple text console for NES nametable output
;
; Provides a 30-column × 28-row text console using PPU nametable
; Requires ascii.chr loaded into CHR pattern table
;
; Public routines:
;   console_init    — Initialize console, load palette, clear screen
;   console_print   — Print null-terminated string (pointer in A=lo, Y=hi)
;   console_putc    — Print single character in A
;   console_newline — Move to next line
;   console_print_hex — Print A as 2 hex digits
;   console_print_dec — Print A as decimal (0-255)
;   console_flush   — Flush line buffer to PPU

.include "nes.inc"
.include "mapper_config.inc"

.ifndef CONSOLE_BG_PPUCTRL
    CONSOLE_BG_PPUCTRL = $08
.endif

.import wait_vbl

; Export public routines
.export console_init, console_print, console_putc, console_newline
.export console_flush, console_print_hex, console_print_dec
.export console_print_inline, console_show
.exportzp str_ptr, ppumask_shadow

.segment "ZEROPAGE"
console_x:     .res 1       ; Current column (0-31)
console_y:     .res 1       ; Current row (0-29)
str_ptr:       .res 2       ; String pointer for console_print
ppumask_shadow: .res 1      ; Shadow copy of last PPUMASK write

.segment "BSS"
; Line buffer: 32 chars per line
console_buf:   .res 32
; For CHR-RAM mappers, embed font in PRG-ROM
.if CHR_ROM_8K = 0
.segment "RODATA"
font_data:
    .incbin "ascii.chr"
FONT_SIZE = 1536            ; 96 tiles × 16 bytes
.endif

.segment "CODE"

; Initialize console: set palette, clear nametable
.proc console_init
    ; Wait for VBL before PPU access
    jsr wait_vbl

    ; For CHR-RAM mappers: copy font from PRG-ROM into CHR-RAM
    .if ::CHR_ROM_8K = 0
    ; Disable rendering for safe PPU access
    lda #0
    sta PPUMASK

    ; Set PPU address to $0200 (tile $20 = space, matching Blargg convention)
    bit PPUSTATUS
    lda #$02
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; Copy 1536 bytes (96 tiles × 16 bytes/tile) in 6 × 256-byte pages
    lda #<::font_data
    sta str_ptr
    lda #>::font_data
    sta str_ptr+1
    ldx #6                  ; 6 pages × 256 = 1536 bytes
    ldy #0
@copy_font:
    lda (str_ptr), y
    sta PPUDATA
    iny
    bne @copy_font
    inc str_ptr+1
    dex
    bne @copy_font

    jsr wait_vbl
    .endif

    ; Load palette — set all 4 BG palettes to black + white
    bit PPUSTATUS           ; Reset latch
    lda #$3F
    sta PPUADDR
    lda #$00
    sta PPUADDR
    ldx #4                  ; 4 palettes × 4 colors = 16 entries
@pal_loop:
    lda #$0F               ; Background: black
    sta PPUDATA
    lda #$30               ; Text: white
    sta PPUDATA
    sta PPUDATA
    sta PPUDATA
    dex
    bne @pal_loop

    ; Clear both nametables at $2000-$27FF (2 × 960 tiles + 2 × 64 attributes)
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR

    lda #$20               ; Space character (ASCII $20)
    ldx #0
    ldy #8                 ; 8 × 256 = 2048 bytes (both nametables)
@clear:
    sta PPUDATA
    inx
    bne @clear
    dey
    bne @clear

    ; Clear both attribute tables to palette 0
    bit PPUSTATUS
    lda #$23
    sta PPUADDR
    lda #$C0
    sta PPUADDR
    lda #$00
    ldx #64
@clear_attr:
    sta PPUDATA
    dex
    bne @clear_attr

    bit PPUSTATUS
    lda #$27
    sta PPUADDR
    lda #$C0
    sta PPUADDR
    lda #$00
    ldx #64
@clear_attr2:
    sta PPUDATA
    dex
    bne @clear_attr2

    ; Reset cursor position
    lda #1                  ; Start at column 1 (margin)
    sta console_x
    lda #1                  ; Start at row 1 (margin)
    sta console_y

    ; Clear line buffer
    jsr clear_line_buf

    ; Enable rendering: background on
    lda #CONSOLE_BG_PPUCTRL
    sta PPUCTRL
    lda #%00001010          ; Show background, no clipping
    sta PPUMASK
    sta ppumask_shadow

    ; Reset scroll
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL

    rts
.endproc

; Clear the line buffer with spaces
.proc clear_line_buf
    lda #$20               ; Space
    ldx #31
@loop:
    sta console_buf, x
    dex
    bpl @loop
    rts
.endproc

; Print null-terminated string
; Input: str_ptr set to string address (lo/hi)
.proc console_print
    ldy #0
@loop:
    lda (str_ptr), y
    beq @done               ; Null terminator
    cmp #10                 ; Newline?
    beq @newline
    jsr console_putc
    iny
    bne @loop               ; Max 256 chars per string
@done:
    rts
@newline:
    jsr console_flush
    jsr console_newline
    iny
    bne @loop
    rts
.endproc

; Print single character in A
.proc console_putc
    pha
    ldx console_x
    cpx #31                 ; End of line?
    bcs @skip
    pla
    sta console_buf, x
    inc console_x
    rts
@skip:
    pla
    rts
.endproc

; Move to next line, flush current line
; If we've reached the bottom of the screen, clear and wrap to top
.proc console_newline
    ; Reset column
    lda #1
    sta console_x

    ; Advance row
    inc console_y

    ; Check if we've gone past the visible area (30 rows, row 0-29)
    lda console_y
    cmp #28
    bcc @no_wrap

    ; Wrap: clear nametable and reset cursor
    jsr clear_screen
    lda #1
    sta console_x
    lda #1
    sta console_y

@no_wrap:
    ; Clear line buffer for next line
    jsr clear_line_buf
    rts
.endproc

; Clear nametable A and reset for continued output
.proc clear_screen
    ; Disable rendering so PPU address isn't corrupted mid-write
    lda #0
    sta PPUMASK

    jsr wait_vbl
    bit PPUSTATUS
    lda #$20
    sta PPUADDR
    lda #$00
    sta PPUADDR

    ; Clear 960 tile bytes (30 rows × 32 cols) with spaces
    lda #$20               ; Space character
    ldx #0
    ldy #4                 ; 4 × 256 = 1024 (covers tiles + attributes)
@clear:
    sta PPUDATA
    inx
    bne @clear
    dey
    bne @clear

    ; Restore previous rendering state (not hardcoded)
    lda ppumask_shadow
    sta PPUMASK

    ; Reset scroll
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc

; Flush line buffer to PPU nametable
; Writes current line buffer at the current row position
.proc console_flush
    ; Wait for VBL
    jsr wait_vbl

    ; Calculate PPU address: $2000 + (console_y * 32)
    bit PPUSTATUS
    lda console_y
    lsr a                   ; y/8 → high nibble adjustment
    lsr a
    lsr a
    clc
    adc #$20                ; Base at $2000
    sta PPUADDR
    lda console_y
    asl a                   ; y * 32 = y << 5
    asl a
    asl a
    asl a
    asl a
    sta PPUADDR

    ; Write 32 bytes from line buffer
    ldx #0
@loop:
    lda console_buf, x
    sta PPUDATA
    inx
    cpx #32
    bne @loop

    ; Reset scroll
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL

    rts
.endproc

; Print A as 2 hex digits
.proc console_print_hex
    pha
    ; High nibble
    lsr a
    lsr a
    lsr a
    lsr a
    jsr @print_nibble
    ; Low nibble
    pla
    and #$0F
    jsr @print_nibble
    rts

@print_nibble:
    cmp #10
    bcc @digit
    ; A-F
    clc
    adc #('A' - 10)
    jmp console_putc
@digit:
    clc
    adc #'0'
    jmp console_putc
.endproc

; Print A as unsigned decimal (0-255)
.proc console_print_dec
    ; Hundreds digit
    ldx #0
@hundreds:
    cmp #100
    bcc @tens
    sbc #100
    inx
    bne @hundreds
@tens:
    pha
    cpx #0
    beq @skip_hundreds
    txa
    clc
    adc #'0'
    jsr console_putc
@skip_hundreds:
    pla
    ; Tens digit
    ldx #0
@tens_loop:
    cmp #10
    bcc @ones
    sbc #10
    inx
    bne @tens_loop
@ones:
    pha
    txa
    clc
    adc #'0'
    jsr console_putc
    ; Ones digit
    pla
    clc
    adc #'0'
    jmp console_putc
.endproc

; Print inline string — call with JSR, string follows in code
; String must be null-terminated
; Usage:
;   jsr console_print_inline
;   .byte "Hello", 0
.proc console_print_inline
    ; Pull return address from stack (points to byte before string)
    pla
    sta str_ptr
    pla
    sta str_ptr+1

    ; Advance past the return address byte
    inc str_ptr
    bne :+
    inc str_ptr+1
:

    ; Print the string
    jsr console_print

    ; Find end of string to fix return address
    ldy #0
@find_end:
    lda (str_ptr), y
    beq @found
    iny
    bne @find_end
@found:
    ; str_ptr + y = address of null terminator
    ; We need to push (str_ptr + y) as return address
    tya
    clc
    adc str_ptr
    tax
    lda #0
    adc str_ptr+1
    pha
    txa
    pha
    rts
.endproc

; Restore display state: PPUCTRL + PPUMASK + scroll
; Call after test code that may have disabled or altered rendering
.proc console_show
    lda #CONSOLE_BG_PPUCTRL
    sta PPUCTRL
    lda #%00001010          ; Show background + leftmost pixels
    sta PPUMASK
    sta ppumask_shadow
    lda #0
    sta PPUSCROLL
    sta PPUSCROLL
    rts
.endproc
