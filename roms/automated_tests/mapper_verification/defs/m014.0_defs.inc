; m014.0_defs.inc — SL-1632 (Mapper 14, Submapper 0) — MMC3/VRC2 hybrid
;
; Features:
;   - Supervisor register at $A131 selects MMC3 vs VRC2 mode
;   - PRG banking: 8KB switchable (MMC3 mode, same as mapper 4)
;   - CHR banking: 1KB switchable (MMC3 R0-R5) + outer CHR A18 bits
;   - Mirroring: H/V programmable (MMC3 $A000)
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (MMC3, only in MMC3 mode)
;
; Supervisor register ($A131):
;   D~A.B. C.M.
;     |  | | +-- Mapper mode (0=VRC2, 1=MMC3)
;     |  | +---- CHR A18 for PPU $0000-$0FFF
;     |  +------ CHR A18 for PPU $1000-$17FF
;     +--------- CHR A18 for PPU $1800-$1FFF
;
; We test in MMC3 mode (M=1) since that's the primary use case.

.define MAPPER_NUM 14
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
.define IRQ_MODE 0             ; 0 = Sharp/MMC3

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; Supervisor register (Mapper 14 specific)
SUPERVISOR_REG   = $A131

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; ============================================================
; Mapper 14 Specific Macros
; ============================================================

; Switch to MMC3 mode via supervisor register
; Also sets CHR outer bits to 0 (lower 256KB)
.macro set_mmc3_mode
    lda #$02                ; M=1 (MMC3 mode), all CHR A18=0
    sta SUPERVISOR_REG
.endmacro

; Map font CHR banks to $0000-$07FF via R0
.macro init_chr_font
    ; R0: 2KB at $0000-$07FF → banks 8+9
    lda #0
    sta MMC3_BANK_SELECT
    lda #8
    sta MMC3_BANK_DATA
.endmacro
