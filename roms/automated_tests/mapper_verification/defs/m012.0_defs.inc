; m012.0_defs.inc — SL-5020B (Mapper 12, Submapper 0) — MMC3 + outer CHR A18
;
; Features:
;   - PRG banking: 8KB switchable (same as MMC3)
;   - CHR banking: 1KB switchable + outer CHR A18 register at $4132
;   - Mirroring: H/V programmable (MMC3 $A000)
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (MMC3A behavior)
;
; Outer CHR register ($4132, mask $E100):
;   D~...B ...A
;      |    +- CHR A18 when PPU A12=0 ($0000-$0FFF)
;      +------ CHR A18 when PPU A12=1 ($1000-$1FFF)

.define MAPPER_NUM 12
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 64       ; 512KB = 64 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 1
.define HAS_BUS_CONFLICTS 0

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp/MMC3A

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; MMC3 register addresses (same as mapper 4)
MMC3_BANK_SELECT = $8000
MMC3_BANK_DATA   = $8001
MMC3_MIRRORING   = $A000
MMC3_PRG_RAM     = $A001
MMC3_IRQ_LATCH   = $C000
MMC3_IRQ_RELOAD  = $C001
MMC3_IRQ_DISABLE = $E000
MMC3_IRQ_ENABLE  = $E001

; Outer CHR A18 register
CHR_EXT_REG      = $4132

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank (identical to MMC3)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #6
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .elseif slot = 1
        lda #7
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .endif
.endmacro

; Select 1KB CHR bank (identical to MMC3)
; The outer CHR A18 bit is set separately via set_chr_ext
.macro select_chr_bank slot, bank
    lda #slot
    sta MMC3_BANK_SELECT
    lda #bank
    sta MMC3_BANK_DATA
.endmacro

; Set outer CHR A18 bits
; lo_bit: CHR A18 for PPU A12=0 ($0000-$0FFF)
; hi_bit: CHR A18 for PPU A12=1 ($1000-$1FFF)
.macro set_chr_ext lo_bit, hi_bit
    lda #((hi_bit << 4) | lo_bit)
    sta CHR_EXT_REG
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
.macro set_mirroring mode
    .if mode = 0
        lda #0
        sta MMC3_MIRRORING
    .elseif mode = 1
        lda #1
        sta MMC3_MIRRORING
    .else
        lda #0
        sta MMC3_MIRRORING
    .endif
.endmacro

; IRQ control (identical to MMC3)
.macro set_irq_counter value
    lda #value
    sta MMC3_IRQ_LATCH
    lda #0
    sta MMC3_IRQ_RELOAD
.endmacro

.macro enable_irq
    cli
    lda #0
    sta MMC3_IRQ_ENABLE
.endmacro

.macro disable_irq
    lda #0
    sta MMC3_IRQ_DISABLE
    sei
.endmacro

; PRG-RAM control (identical to MMC3)
.macro enable_prg_ram
    lda #%10000000
    sta MMC3_PRG_RAM
.endmacro

.macro disable_prg_ram
    lda #%00000000
    sta MMC3_PRG_RAM
.endmacro

.macro write_protect_prg_ram
    lda #%11000000
    sta MMC3_PRG_RAM
.endmacro

; Map font CHR banks to $0000-$07FF via R0
; Font is in the first 2KB of the lower 256KB (A18=0)
.macro init_chr_font
    ; Ensure outer CHR A18 = 0 for font access
    lda #0
    sta CHR_EXT_REG
    ; R0: 2KB at $0000-$07FF → banks 8+9
    lda #0
    sta MMC3_BANK_SELECT
    lda #8
    sta MMC3_BANK_DATA
.endmacro
