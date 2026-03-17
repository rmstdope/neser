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
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp/MMC3A

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; Outer CHR A18 register (Mapper 12 specific)
CHR_EXT_REG      = $4132

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; ============================================================
; Mapper 12 Specific Macros
; ============================================================

; Set outer CHR A18 bits
; lo_bit: CHR A18 for PPU A12=0 ($0000-$0FFF)
; hi_bit: CHR A18 for PPU A12=1 ($1000-$1FFF)
.macro set_chr_ext lo_bit, hi_bit
    lda #((hi_bit << 4) | lo_bit)
    sta CHR_EXT_REG
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
