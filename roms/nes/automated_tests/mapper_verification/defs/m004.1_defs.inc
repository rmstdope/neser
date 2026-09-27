; m004.1_defs.inc — MMC6/HKROM (Mapper 4, Submapper 1)
;
; NES 2.0 submapper 004:1 is the Nintendo MMC6
; (https://www.nesdev.org/wiki/NES_2.0_submappers#004:_1_MMC6):
;   "MMC6 has an alternative PRG-RAM enable and write protection scheme
;    designed for its internal 1k PRG RAM. Uses Sharp MMC3 IRQ behavior."
;
; Differences from m004.0 (https://www.nesdev.org/wiki/MMC6):
;   - 1 KB internal PRG-RAM at $7000-$7FFF, mirrored; nothing at $6000-$6FFF
;   - $8000 bit 5 = PRG-RAM enable; while clear, $A001 is held at $00
;   - $A001 = HhLl xxxx: per-512-byte-half read (H/L) and write (h/l) enables
;   - IRQ: Sharp MMC3 behaviour (IRQ_MODE 0)
;
; Because there is no RAM at $6000, the $6000 status-byte protocol cannot be
; used: these ROMs are console-verified.

.define MAPPER_NUM 4
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 4
CHR_ROM_8K    = 2
PRG_RAM_SHIFT = 4        ; 64 << 4 = 1 KB
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

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
.define IRQ_MODE 0             ; 0 = Sharp (MMC6 uses Sharp MMC3 IRQ behaviour)

; Bank sizes
.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 1
.define PRG_BANK_COUNT 4
.define CHR_BANK_COUNT 8

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; ============================================================
; MMC6 PRG-RAM control
; ============================================================

MMC6_RAM_ENABLE = %00100000     ; $8000 bit 5: PRG-RAM enable

; $A001 bits: H h L l x x x x
MMC6_RD_HI = %10000000          ; read enable  $7200-$73FF
MMC6_WR_HI = %01000000          ; write enable $7200-$73FF
MMC6_RD_LO = %00100000          ; read enable  $7000-$71FF
MMC6_WR_LO = %00010000          ; write enable $7000-$71FF

; Map font CHR banks (8+9) to $0000-$07FF via R0
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
