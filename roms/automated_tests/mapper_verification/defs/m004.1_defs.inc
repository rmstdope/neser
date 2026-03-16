; m004.1_defs.inc — MMC3/TxROM (Mapper 4, Submapper 1) — NEC IRQ
;
; Identical to m004.0 except:
;   - SUBMAPPER_NUM = 1
;   - IRQ_MODE = 1 (NEC: fires on 1→0 transition only)
;
; The NEC variant fires IRQ one scanline later than Sharp on the first
; IRQ after a reload, because it requires the actual 1→0 transition
; rather than just "counter == 0".

.define MAPPER_NUM 4
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 4
CHR_ROM_8K    = 2
PRG_RAM_SHIFT = 7
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
.define IRQ_MODE 1             ; 1 = NEC (alternate, fires on 1→0 transition)

; Bank sizes
.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 1
.define PRG_BANK_COUNT 4
.define CHR_BANK_COUNT 8

; Registers — same as submapper 0
MMC3_BANK_SELECT = $8000
MMC3_BANK_DATA   = $8001
MMC3_MIRRORING   = $A000  ; Mirroring: bit 0 (0=vert mirror/horiz arr/A10, 1=horiz mirror/vert arr/A11)
MMC3_PRG_RAM     = $A001
MMC3_IRQ_LATCH   = $C000
MMC3_IRQ_RELOAD  = $C001
MMC3_IRQ_DISABLE = $E000
MMC3_IRQ_ENABLE  = $E001

; All macros identical to submapper 0
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

.macro select_chr_bank slot, bank
    lda #slot
    sta MMC3_BANK_SELECT
    lda #bank
    sta MMC3_BANK_DATA
.endmacro

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

.macro enable_prg_ram
    lda #%10000000          ; Bit 7=chip enable, bit 6=0 (write enabled)
    sta MMC3_PRG_RAM
.endmacro

.macro disable_prg_ram
    lda #%00000000
    sta MMC3_PRG_RAM
.endmacro

.macro write_protect_prg_ram
    lda #%11000000          ; Chip enable + write protect
    sta MMC3_PRG_RAM
.endmacro

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
