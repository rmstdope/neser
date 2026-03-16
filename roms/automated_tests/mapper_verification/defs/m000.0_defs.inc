; m000.0_defs.inc — NROM (Mapper 0, Submapper 0)
;
; Features:
;   - No PRG banking (16KB or 32KB fixed)
;   - No CHR banking (8KB fixed CHR-ROM or CHR-RAM)
;   - Fixed mirroring (solder pads)
;   - Optional PRG-RAM at $6000-$7FFF
;   - No IRQ
;   - No bus conflicts

.define MAPPER_NUM 0
.define SUBMAPPER_NUM 0

; ROM sizes for NES 2.0 header
PRG_ROM_16K   = 2        ; 32KB = 2 × 16KB
CHR_ROM_8K    = 1        ; 8KB = 1 × 8KB
PRG_RAM_SHIFT = 7        ; 2^7 = 128 × 64 = 8KB
CHR_RAM_SHIFT = 0        ; No CHR-RAM (has CHR-ROM)

; Capability flags
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0

; Bank sizes (not applicable, but defined for consistency)
.define PRG_BANK_SIZE 32      ; 32KB fixed
.define CHR_BANK_SIZE 8       ; 8KB fixed
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; ============================================================
; Register macros — NROM has no mapper registers
; ============================================================

; No-op macros for compatibility
.macro select_prg_bank slot, bank
    ; NROM: no PRG banking
.endmacro

.macro select_chr_bank slot, bank
    ; NROM: no CHR banking
.endmacro

.macro set_mirroring mode
    ; NROM: mirroring is fixed
.endmacro

.macro enable_prg_ram
    ; NROM: PRG-RAM is always enabled (if present)
.endmacro

.macro disable_prg_ram
    ; NROM: cannot disable PRG-RAM
.endmacro
