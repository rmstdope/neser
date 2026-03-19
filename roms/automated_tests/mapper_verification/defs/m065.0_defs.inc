; m065.0_defs.inc — Irem H3001 (Mapper 65, Submapper 0)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_065.xhtml
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 1 fixed second-to-last + 1 fixed last)
;   - PRG mode bit ($9000 bit 7) swaps reg0 between $8000 and $C000
;   - CHR banking: 8 × 1KB switchable banks ($B000-$B007)
;   - Mirroring: V/H/1scA via $9001
;   - IRQ: 16-bit CPU cycle down-counter
;   - No PRG-RAM, no bus conflicts
;
; Power-up defaults: reg0=$00, reg1=$01

.define MAPPER_NUM 65
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB = 16 × 1KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; H/V (set >= 4 to enable V/H tests)

; IRQ behavior — CPU cycle down-counter
.define IRQ_MODE 0
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4      ; reg0, reg1, fixed $3E, fixed $3F
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots

; ============================================================
; Register Addresses
; ============================================================

H3001_PRG0     = $8000   ; PRG reg 0 (8K @ $8000 or $C000 depending on mode)
H3001_PRG_MODE = $9000   ; Bit 7: PRG layout (0=reg0@$8000, 1=reg0@$C000)
H3001_MIRROR   = $9001   ; Bits 6-7: mirroring (00=V, 10=H, 01/11=1scA)
H3001_IRQ_EN   = $9003   ; Bit 7: IRQ enable (0=disable, 1=enable); any write acks
H3001_IRQ_LOAD = $9004   ; Any write: copy reload value to counter + ack
H3001_IRQ_HI   = $9005   ; High 8 bits of IRQ reload value
H3001_IRQ_LO   = $9006   ; Low 8 bits of IRQ reload value
H3001_PRG1     = $A000   ; PRG reg 1 (8K @ $A000, always)

; ============================================================
; Register Macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000 (reg0), slot 1 = $A000 (reg1)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #bank
        sta H3001_PRG0
    .elseif slot = 1
        lda #bank
        sta H3001_PRG1
    .endif
.endmacro

; Select 1KB CHR bank (slot 0-7 → $B000-$B007)
.macro select_chr_bank slot, bank
    lda #bank
    sta $B000 + slot
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
; $9001 bits 6-7: 00=V, 10=H, 01/11=1scA
.macro set_mirroring mode
    .if mode = 0
        lda #%00000000          ; Vertical
    .elseif mode = 1
        lda #%10000000          ; Horizontal
    .else
        lda #%01000000          ; 1-screen A
    .endif
    sta H3001_MIRROR
.endmacro

; IRQ counter: 16-bit CPU cycle down-counter.
; Scale shared test values by ~114 (cycles per scanline) to avoid
; an interrupt storm from a counter value that's too small.
.macro set_irq_counter value
    lda #>(value * 114)
    sta H3001_IRQ_HI
    lda #<(value * 114)
    sta H3001_IRQ_LO
    lda #0
    sta H3001_IRQ_LOAD      ; Copy reload value to counter
.endmacro

.macro enable_irq
    lda #$80                ; Bit 7 = enable
    sta H3001_IRQ_EN
    cli
.endmacro

.macro disable_irq
    lda #$00                ; Bit 7 = disable (also acknowledges)
    sta H3001_IRQ_EN
    sei
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF
.macro init_chr_font
    lda #8
    sta $B000               ; CHR bank 8 → $0000-$03FF
    lda #9
    sta $B001               ; CHR bank 9 → $0400-$07FF
.endmacro
