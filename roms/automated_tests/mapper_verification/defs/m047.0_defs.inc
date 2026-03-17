; m047.0_defs.inc — Super Spike V'Ball + Nintendo World Cup (Mapper 47)
;
; NESdev backup wiki notes:
; - MMC3 multicart
; - No PRG-RAM
; - $6000-$7FFF: [.... ...B] block select
; - Block register is only writable when MMC3 PRG-RAM is enabled and writable
; - $8000-$FFFF: normal MMC3 registers within the selected block
; - Each block has 128KB PRG and 128KB CHR
;
; This first ROM slice verifies the unique outer block-select behavior only.

.define MAPPER_NUM 47
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 16       ; 256KB = 32 × 8KB banks (2 × 128KB blocks)
CHR_ROM_8K    = 1        ; Fixed CHR for console output in this slice
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

; Capability flags for this slice
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 0
.define CHR_BANK_COUNT 0

.include "mmc3_core_macros.inc"

BLOCK_SELECT_WRITE_ADDR = $6000
BLOCK_SELECT_WRITE_OR_MASK = 0
BLOCK_SELECT_RESTORE_VALUE = 0
BLOCK_SELECT_NEEDS_PRG_RAM_ENABLE = 1
SKIP_BLOCK_SELECT_TEST_4 = 1
