; m021.2_defs.inc — VRC4c (Mapper 21, Submapper 2)
;
; Konami VRC4c chip.  Address pins: A0_OFFSET=$40, A1_OFFSET=$80.
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - CHR banking: 8 × 1KB switchable
;   - Mirroring: V/H/1A/1B programmable
;   - Scanline IRQ (CPU cycle counter, M-cycle mode)

.define MAPPER_NUM 21
.define SUBMAPPER_NUM 2

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (= 8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks
PRG_RAM_SHIFT = 0        ; No PRG-RAM for test
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior (CPU cycle counter)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots (0,1 switchable; 2,3 fixed)
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; ============================================================
; VRC4 address pin configuration
; ============================================================

VRC4_A0_OFFSET = $40
VRC4_A1_OFFSET = $80

; ============================================================
; VRC4 Core Macros (shared by all VRC4/VRC2 variants)
; ============================================================

.include "vrc4_core_macros.inc"
