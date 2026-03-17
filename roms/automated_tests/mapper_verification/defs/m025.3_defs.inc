; m025.3_defs.inc — VRC2c (Mapper 25, Submapper 3)
;
; Konami VRC2c chip.  Address pins: A0_OFFSET=$02, A1_OFFSET=$01.
; VRC2 variant: no IRQ, H/V mirroring only, no PRG swap mode.
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed 16KB)
;   - CHR banking: 8 × 1KB switchable (hi nibble 5th bit ignored; max 256KB CHR)
;   - Mirroring: V/H only
;   - No IRQ

.define MAPPER_NUM 25
.define SUBMAPPER_NUM 3

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (= 8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks
PRG_RAM_SHIFT = 0        ; No PRG-RAM
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 2       ; VRC2: V/H only

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots (0,1 switchable; 2,3 fixed)
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; ============================================================
; VRC2 address pin configuration
; ============================================================

VRC4_A0_OFFSET = $02
VRC4_A1_OFFSET = $01

; VRC2_MODE disables IRQ macros in vrc4_core_macros.inc
VRC2_MODE = 1

; ============================================================
; VRC4/VRC2 Core Macros (shared by all VRC4/VRC2 variants)
; ============================================================

.include "vrc4_core_macros.inc"
