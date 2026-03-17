; m044.0_defs.inc — Super Big 7-in-1 (Mapper 44, Submapper 0) — MMC3 Multicart
;
; Features:
;   - PRG banking: 8KB switchable (MMC3 R6/R7 + 2 fixed)
;   - CHR banking: 1KB/2KB switchable (MMC3 R0-R5)
;   - Mirroring: H/V programmable
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (Sharp: fires when counter reaches 0)
;   - Block select: $A001 bits 0-2 select outer bank block
;
; Block select register $A001:
;   7  bit  0
;   ---- ----
;   EW.. .BBB
;   ||    |||
;   ||    +++- Block number (0-7; block 7 = block 6)
;   |+-------- WRAM write enable
;   +--------- WRAM chip enable
;
; Block sizes:
;   Blocks 0-5: PRG AND $0F (128KB), CHR AND $7F (128KB)
;   Block 6-7:  PRG AND $1F (256KB), CHR AND $FF (256KB)
;
; NOTE: $A001 is shared between WRAM control AND block select.
;       enable_prg_ram writes $80 to $A001 (WRAM enable + block 0).
;
; Test config uses 3 blocks (384KB PRG, 16KB CHR) to exercise block
; switching without an impractically large ROM.

.define MAPPER_NUM 44
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 24       ; 384KB = 24 × 16KB (= 48 × 8KB, 3 blocks of 16)
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB (= 16 × 1KB for block 0)
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
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots (R6, R7, fixed, fixed)
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots (R0-R5)

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14, 44)
; ============================================================

.include "mmc3_core_macros.inc"

; Map font CHR banks (8+9) to $0000-$07FF via R0
; Called during init and after CHR banking tests
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
