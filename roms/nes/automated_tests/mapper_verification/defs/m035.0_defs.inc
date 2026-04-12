; m035.0_defs.inc — J.Y. Company ASIC (Mapper 35, Submapper 0)
;
; Mapper 35 = mapper 209 + explicit 8KB WRAM.
; J.Y. Company proprietary ASIC with flexible banking, multiplier, and IRQ.
;
; Features:
;   - PRG banking: 4 × 8KB switchable via $8000-$8003 (8K mode)
;     $E000-$FFFF fixed to last bank when $D000 bit 2 = 0
;   - CHR banking: 8 × 1KB via $9000-$9007 (LSB) + $A000-$A007 (MSB)
;   - Mirroring: H/V via $D001 bits 0-1 (0=V, 1=H, 2=1A, 3=1B)
;   - PRG-RAM: 8KB WRAM at $6000-$7FFF (when $D000 bit 7 = 0)
;   - Hardware multiplier at $5800/$5801
;   - No bus conflicts
;
; Mode register ($D000) = $1A for test ROM:
;   PP=10 (8K PRG mode), L=0 (last bank fixed), CC=11 (1K CHR mode),
;   R=0 (ROM nametables disabled), G=0, 6=0 (WRAM at $6000)
;
; Address masks:
;   PRG banks: $F803 (bit 11 must be clear)
;   CHR banks: $F807 (bit 11 must be clear)
;   Mode/mirroring: $F803 (bit 11 must be clear)

.define MAPPER_NUM 35
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB (16 × 1KB banks)
PRG_RAM_SHIFT = 7        ; 8KB WRAM
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable

; CHR slot layout: linear (slot 0=$0000, slot 1=$0400, slot 2=$0800, ...)
CHR_1K_LINEAR = 1
.define PRG_BANK_COUNT 4       ; 4 windows: 3 switchable + 1 fixed
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; Hardware multiplier registers (reuse test_multiplier.s symbol names)
MMC5_MULT_A = $5800            ; Operand 1 (W) / result low (R)
MMC5_MULT_B = $5801            ; Operand 2 (W) / result high (R)

; ============================================================
; J.Y. Company ASIC Core Macros
; ============================================================

.include "jy_company_macros.inc"
