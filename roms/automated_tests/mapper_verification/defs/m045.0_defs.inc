; m045.0_defs.inc — GA23C multicart (Mapper 45)
;
; NESdev backup wiki notes:
; - MMC3 clone with four outer bank registers written in sequence at $6000
; - $6001 resets the outer-register write index and clears lock
; - PRG bank composition is controlled by PRG-OR LSB (#1), PRG-OR MSB (#2),
;   and PRG-AND (#3, inverted)
; - Outer registers overlay WRAM regardless of MMC3 WRAM bits
;
; This first ROM slice verifies that sequencing writes through the outer
; register ring changes the visible PRG bank at $8000.

.define MAPPER_NUM 45
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 16       ; 256KB = 32 × 8KB banks
CHR_ROM_8K    = 1        ; Fixed CHR for console output
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

CUSTOM_BLOCK_SELECT_SEQUENCE = 1
BLOCK_SELECT_TEST1_TARGET = 0
BLOCK_SELECT_TEST1_EXPECTED_ID = 0
BLOCK_SELECT_TEST2_TARGET = 1
BLOCK_SELECT_TEST2_EXPECTED_ID = 8
BLOCK_SELECT_TEST3_TARGET = 0
BLOCK_SELECT_TEST3_EXPECTED_ID = 0
BLOCK_SELECT_TEST4_TARGET = 2
BLOCK_SELECT_TEST4_EXPECTED_ID = $10
BLOCK_SELECT_SIG16_ID = 8
BLOCK_SELECT_SIG32_ID = $10
BLOCK_SELECT_RESTORE_TARGET = 0

.macro program_block_select_from_a
    .local not_target_one, not_target_two, have_prg_or
    tax
    lda #0
    sta $6001               ; Reset outer register index and unlock

    lda #0
    sta $6000               ; Reg #0: CHR-OR LSB = 0

    txa
    cmp #1
    bne not_target_one
    lda #8
    bne have_prg_or
not_target_one:
    cmp #2
    bne not_target_two
    lda #$10
    bne have_prg_or
not_target_two:
    lda #0
have_prg_or:
    sta $6000               ; Reg #1: PRG-OR LSB

    lda #0
    sta $6000               ; Reg #2: PRG-OR MSB = 0
    lda #0
    sta $6000               ; Reg #3: PRG-AND inverted mask = 0 (512KB/full range)
.endmacro
