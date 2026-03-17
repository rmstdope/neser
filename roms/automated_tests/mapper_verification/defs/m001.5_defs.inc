; m001.5_defs.inc — MMC1 Fixed PRG (Mapper 1, Submapper 5)
;
; Submapper 5: PRG banking is DISABLED — 32KB PRG is always fixed.
; CHR banking and mirroring still work normally.
;
; This def file is identical to m001.0 except:
;   - SUBMAPPER_NUM = 5
;   - HAS_PRG_BANKING = 0 (PRG is fixed)
;   - PRG_BANK_SIZE = 32 (always 32KB fixed)
;   - FIXED_PRG flag is set for negative testing

.define MAPPER_NUM 1
.define SUBMAPPER_NUM 5

PRG_ROM_16K   = 2        ; 32KB = 2 × 16KB
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 0    ; ** FIXED — no PRG switching **
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0    ; MMC1 only has enable/disable, not write-protect
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B
.define FIXED_PRG 1    ; Flag: PRG is intentionally fixed

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB fixed
.define CHR_BANK_SIZE 4        ; 4KB switchable
.define PRG_BANK_COUNT 1        ; 1 fixed bank
.define CHR_BANK_COUNT 2        ; 2 × 4KB slots

; Include the MMC1 register macros (shift register is the same)
; The select_prg_bank macro still exists but the test should verify
; that writing it has NO effect on PRG banking.

MMC1_CONTROL  = $8000
MMC1_CHR0     = $A000
MMC1_CHR1     = $C000
MMC1_PRG      = $E000

; Write 5-bit value to MMC1 register
.macro mmc1_write_reg addr, val
    lda #val
    .repeat 4
        sta addr
        lsr a
    .endrepeat
    sta addr
.endmacro

; PRG bank select — still writes the register (test should verify it's ignored)
.macro select_prg_bank slot, bank
    mmc1_write_reg MMC1_PRG, bank
.endmacro

.macro select_chr_bank slot, bank
    .if slot = 0
        mmc1_write_reg MMC1_CHR0, bank
    .else
        mmc1_write_reg MMC1_CHR1, bank
    .endif
.endmacro

.macro set_mirroring mode
    mmc1_write_reg MMC1_CONTROL, (mode | %01100)
.endmacro

.macro enable_prg_ram
    mmc1_write_reg MMC1_PRG, (0)
.endmacro

.macro disable_prg_ram
    mmc1_write_reg MMC1_PRG, (%10000)
.endmacro
