; m043.0_defs.inc — TONY-I / YS-612 (Mapper 43)
;
; NESdev backup wiki notes:
; - CPU $5000-$5FFF: repeated 2 KiB PRG-ROM chip
; - CPU $6000-$7FFF: fixed bank #2 from the 64 KiB main PRG area
; - CPU $8000-$9FFF: fixed bank #1
; - CPU $A000-$BFFF: fixed bank #0
; - CPU $C000-$DFFF: switchable 8 KiB PRG bank via $4022
; - CPU $E000-$FFFF: fixed 8 KiB PRG-ROM chip
; - $4022 low 3 bits select one of banks 3/4/5/6/7 via a lookup table
;
; This ROM reuses the generic 8KB PRG banking test by treating three distinct
; lookup-table outputs as logical banks 0, 1, and 2.

.define MAPPER_NUM 43
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 5        ; 80KB total PRG
CHR_ROM_8K    = 1        ; Fixed CHR for console output
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

BANK_WINDOW_OVERRIDE = $C000
SKIP_SECOND_PRG_8K_SLOT_TESTS = 1

.macro select_prg_bank slot, bank
    .if bank = 0
        ; $4022 value 0 -> physical bank 4
        lda #0
    .elseif bank = 1
        ; $4022 value 1 -> physical bank 3
        lda #1
    .elseif bank = 2
        ; $4022 value 6 -> physical bank 5
        lda #6
    .endif
    sta $4022
.endmacro

.macro select_chr_bank slot, bank
.endmacro

.macro set_mirroring mode
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
