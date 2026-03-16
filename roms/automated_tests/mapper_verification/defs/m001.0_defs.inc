; m001.0_defs.inc — MMC1/SxROM (Mapper 1, Submapper 0)
;
; Features:
;   - PRG banking: 16KB or 32KB switchable
;   - CHR banking: 4KB or 8KB switchable
;   - Mirroring: H/V/1A/1B programmable
;   - PRG-RAM at $6000-$7FFF with enable/disable
;   - No IRQ
;   - Serial shift register interface (5 writes to load a register)

.define MAPPER_NUM 1
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable (in mode 2/3)
.define CHR_BANK_SIZE 4       ; 4KB switchable (in mode 1)
.define PRG_BANK_COUNT 2       ; 2 × 16KB slots
.define CHR_BANK_COUNT 2       ; 2 × 4KB slots

; MMC1 register addresses
MMC1_CONTROL  = $8000     ; $8000-$9FFF: Control register
MMC1_CHR0     = $A000     ; $A000-$BFFF: CHR bank 0
MMC1_CHR1     = $C000     ; $C000-$DFFF: CHR bank 1
MMC1_PRG      = $E000     ; $E000-$FFFF: PRG bank + RAM enable

; MMC1 control register bits:
;   bits 0-1: mirroring (0=1A, 1=1B, 2=V, 3=H)
;   bits 2-3: PRG mode (0,1=32KB, 2=fix first/switch second, 3=switch first/fix last)
;   bit 4: CHR mode (0=8KB, 1=4KB)

; ============================================================
; Register macros
; ============================================================

; Reset MMC1 shift register
.macro mmc1_reset
    lda #$80
    sta $8000
.endmacro

; Write 5-bit value to MMC1 register
; addr = register address ($8000/$A000/$C000/$E000)
; val  = 5-bit value
.macro mmc1_write_reg addr, val
    lda #val
    .repeat 4
        sta addr
        lsr a
    .endrepeat
    sta addr
.endmacro

; Select PRG bank (16KB mode: mode 2 or 3)
; Assumes control register already set to PRG mode 3
.macro select_prg_bank slot, bank
    mmc1_write_reg MMC1_PRG, bank
.endmacro

; Select CHR bank
; slot 0 = $0000-$0FFF, slot 1 = $1000-$1FFF (in 4KB mode)
.macro select_chr_bank slot, bank
    .if slot = 0
        mmc1_write_reg MMC1_CHR0, bank
    .else
        mmc1_write_reg MMC1_CHR1, bank
    .endif
.endmacro

; Set mirroring mode
; 0=single-screen A, 1=single-screen B, 2=vertical, 3=horizontal
; Preserves other control register bits (PRG mode 3, CHR mode 4KB)
.macro set_mirroring mode
    ; Control = mirroring | (PRG mode 3 << 2) | (CHR 4KB << 4)
    mmc1_write_reg MMC1_CONTROL, (mode | %01100)
.endmacro

; Enable/disable PRG-RAM
; MMC1 PRG register bit 4: 0=enable, 1=disable (active low on MMC1B/C)
.macro enable_prg_ram
    mmc1_write_reg MMC1_PRG, (0)
.endmacro

.macro disable_prg_ram
    mmc1_write_reg MMC1_PRG, (%10000)
.endmacro

; Write-protect PRG-RAM (read-only, same as disable for MMC1)
.macro write_protect_prg_ram
    mmc1_write_reg MMC1_PRG, (%10000)
.endmacro
