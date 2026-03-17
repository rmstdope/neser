; m037.0_defs.inc — SMB + Tetris + Nintendo World Cup (Mapper 37)
;
; NESdev backup wiki notes:
; - MMC3-compatible multicart with outer register at $6000-$7FFF
; - No PRG-RAM; outer register replaces it
; - Outer values 0/1/2 select PRG window $00000-$0FFFF
; - Outer value 3 selects PRG window $10000-$1FFFF
; - Outer values 4/5/6 select PRG window $20000-$3FFFF
; - Outer value 7 selects PRG window $30000-$3FFFF
; - Writes require MMC3 PRG-RAM to be enabled/writable first
;
; This first ROM slice verifies the unique outer block/jail-cell selection.

.define MAPPER_NUM 37
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 16       ; 256KB = 32 × 8KB banks
CHR_ROM_8K    = 1        ; Fixed CHR for console output in this slice
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

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

; Exercise both the 64KB and 128KB outer windows.
BLOCK_SELECT_TEST1_TARGET = 0
BLOCK_SELECT_TEST1_EXPECTED_ID = 0
BLOCK_SELECT_TEST2_TARGET = 3
BLOCK_SELECT_TEST2_EXPECTED_ID = 8
BLOCK_SELECT_TEST3_TARGET = 0
BLOCK_SELECT_TEST3_EXPECTED_ID = 0
BLOCK_SELECT_TEST4_TARGET = 4
BLOCK_SELECT_TEST4_EXPECTED_ID = $10
BLOCK_SELECT_SIG16_ID = 8
BLOCK_SELECT_SIG32_ID = $10
