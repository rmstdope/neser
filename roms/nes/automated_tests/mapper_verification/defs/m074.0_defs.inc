; m074.0_defs.inc — Waixing MMC3 with CHR-RAM (Mapper 74, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/INES_Mapper_074
;
; Features:
;   - Standard MMC3 PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - Standard MMC3 CHR banking: 1KB/2KB switchable
;     - CHR bank indices 8 and 9 redirect to 2KB CHR-RAM
;     - All other bank indices read from CHR-ROM
;   - Mirroring: H/V programmable (standard MMC3)
;   - Scanline IRQ (Sharp mode, standard MMC3)
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - No bus conflicts

.define MAPPER_NUM 74
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks (banks 8-9 redirect to CHR-RAM)
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
.define IRQ_MODE 0             ; 0 = Sharp (standard)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; Test only ROM banks 0-7 (8-9 are CHR-RAM)

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14, 64)
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR bank to $0000-$07FF via R0.
; Standard MMC3 uses banks 8+9, but on mapper 074 those are CHR-RAM.
; Use banks 10+11 (CHR-ROM) for font data instead.
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #10                 ; Font is in CHR banks 10+11
    sta MMC3_BANK_DATA
.endmacro
