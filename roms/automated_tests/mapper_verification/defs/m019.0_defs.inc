; m019.0_defs.inc — Namco 129/163 (Mapper 19, Submapper 0)
;
; Namco 163 ASIC with expansion audio (audio mixing unspecified for SM0).
;
; Features:
;   - PRG banking: 8KB switchable (3 variable + 1 fixed last)
;   - CHR banking: 8 × 1KB switchable at PPU $0000-$1FFF
;   - Nametable control: CHR/CIRAM selectable per nametable slot
;   - Mirroring: via nametable select registers (V/H/1A/1B)
;   - IRQ: 15-bit CPU cycle counter; fires on $7FFF→$0000 wrap

.define MAPPER_NUM 19
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (= 8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks
PRG_RAM_SHIFT = 0        ; No WRAM for test
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define HAS_NT_FROM_CHR 1           ; SM0 tests NT-from-CHR feature
.define MAX_MIRRORING_MODES 4       ; V/H/1A/1B via NT select regs

; IRQ behavior (15-bit CPU cycle counter)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots (0,1,2 switchable; 3 fixed)
.define CHR_BANK_COUNT 8       ; 8 × 1KB PPU $0000-$1FFF slots

; ============================================================
; N163 Register Addresses
; ============================================================

N163_IRQ_CTR_LO   = $5000       ; IRQ counter low 8 bits (r/w)
N163_IRQ_CTR_HI   = $5800       ; IRQ counter high 7 bits + enable (r/w)
                                 ; bit 7 = IRQ disable (1=off, 0=running)
N163_PRG0_REG     = $E000       ; PRG bank at $8000 — bits 5-0
N163_PRG1_REG     = $E800       ; PRG bank at $A000 — bits 5-0
                                 ; bit 6 = CHR-RAM enable (ignored in test)
N163_PRG2_REG     = $F000       ; PRG bank at $C000 — bits 5-0

; ============================================================
; PRG banking
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000, slot 1 = $A000, slot 2 = $C000
; slot 3 = fixed last bank (no-op)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #bank
        sta N163_PRG0_REG
    .elseif slot = 1
        lda #bank
        sta N163_PRG1_REG
    .elseif slot = 2
        lda #bank
        sta N163_PRG2_REG
    .endif
.endmacro

; ============================================================
; CHR banking (1KB per PPU slot, $8000-$B800 register space)
; ============================================================

; Select 1KB CHR bank for PPU slots 0-7 ($0000-$1FFF)
; slot N → address $8000 + N*$0800
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta $8000
    .elseif slot = 1
        lda #bank
        sta $8800
    .elseif slot = 2
        lda #bank
        sta $9000
    .elseif slot = 3
        lda #bank
        sta $9800
    .elseif slot = 4
        lda #bank
        sta $A000
    .elseif slot = 5
        lda #bank
        sta $A800
    .elseif slot = 6
        lda #bank
        sta $B000
    .elseif slot = 7
        lda #bank
        sta $B800
    .endif
.endmacro

; ============================================================
; Nametable mirroring (via NT slot registers $C000-$D800)
; Internal CIRAM: $E8 (page 0), $E9 (page 1)
; ============================================================

; Set nametable mirroring
; mode 0=Vertical, 1=Horizontal, 2=1-screen A, 3=1-screen B
.macro set_mirroring mode
    .if mode = 0
        ; Vertical: slots 8+10 = CIRAM pg0, slots 9+11 = CIRAM pg1
        lda #$E8
        sta $C000               ; slot 8 (NT $2000)
        sta $D000               ; slot 10 (NT $2800)
        lda #$E9
        sta $C800               ; slot 9 (NT $2400)
        sta $D800               ; slot 11 (NT $2C00)
    .elseif mode = 1
        ; Horizontal: slots 8+9 = CIRAM pg0, slots 10+11 = CIRAM pg1
        lda #$E8
        sta $C000               ; slot 8 (NT $2000)
        sta $C800               ; slot 9 (NT $2400)
        lda #$E9
        sta $D000               ; slot 10 (NT $2800)
        sta $D800               ; slot 11 (NT $2C00)
    .elseif mode = 2
        ; 1-screen A: all NT slots → CIRAM page 0
        lda #$E8
        sta $C000
        sta $C800
        sta $D000
        sta $D800
    .elseif mode = 3
        ; 1-screen B: all NT slots → CIRAM page 1
        lda #$E9
        sta $C000
        sta $C800
        sta $D000
        sta $D800
    .endif
.endmacro

; ============================================================
; IRQ control (15-bit CPU cycle counter)
; ============================================================

; Set IRQ counter to fire after approximately 'value' CPU cycles
; 15-bit counter fires on $7FFF→$0000 wrap
; To fire after N cycles: load ($8000 - N)
.macro set_irq_counter value
    lda #<($8000 - value)
    sta N163_IRQ_CTR_LO
    lda #(>($8000 - value) & $7F)   ; bit 7 = 0 = IRQ enabled
    sta N163_IRQ_CTR_HI
.endmacro

.macro enable_irq
    ; IRQ is already enabled by writing N163_IRQ_CTR_HI with bit 7 = 0
    cli
.endmacro

.macro disable_irq
    lda N163_IRQ_CTR_HI
    ora #$80                    ; Set bit 7 = IRQ disable
    sta N163_IRQ_CTR_HI
    sei
.endmacro

; ============================================================
; Font initialisation
; ============================================================

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
; Also initialise CIRAM nametables (vertical mirroring as default)
.macro init_chr_font
    select_chr_bank 0, 8    ; slot 0 → bank 8 (font low 1KB)
    select_chr_bank 1, 9    ; slot 1 → bank 9 (font high 1KB)
    ; Set up CIRAM nametables (vertical mirroring as default)
    lda #$E8
    sta $C000               ; slot 8  = CIRAM page 0
    sta $D000               ; slot 10 = CIRAM page 0
    lda #$E9
    sta $C800               ; slot 9  = CIRAM page 1
    sta $D800               ; slot 11 = CIRAM page 1
.endmacro
