; m119.0_defs.inc — TQROM (Mapper 119, Submapper 0)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_119.xhtml
;
; TQROM uses MMC3 but adds CHR-ROM/CHR-RAM mixing via bit 6 of CHR bank values:
;   - Bit 6 = 0: select from CHR-ROM
;   - Bit 6 = 1: select from CHR-RAM (8KB)
;
; Features:
;   - PRG banking: standard MMC3 (2 switchable 8KB + 2 fixed)
;   - CHR banking: standard MMC3 1KB/2KB layout with bit 6 ROM/RAM select
;   - Mirroring: H/V via MMC3 $A000
;   - IRQ: MMC3 scanline counter (Sharp variant)
;   - No PRG-RAM on physical board (declared for test status byte protocol)
;   - No bus conflicts

.define MAPPER_NUM 119
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB CHR-ROM banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 7        ; 8KB CHR-RAM (TQROM unique: bit 6 selects CHR-RAM)

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0          ; No PRG-RAM on physical TQROM board
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4  ; H/V (>= 4 enables V/H tests)

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp (standard MMC3)

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots (2 switchable + 2 fixed)
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots (2×2KB + 4×1KB via MMC3)

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14, 64)
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
