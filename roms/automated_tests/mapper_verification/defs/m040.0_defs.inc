; m040.0_defs.inc — NTDEC 2722 (Mapper 40, Submapper 0)
;
; NESdev backup wiki notes:
; - PRG setup:
;     $6000 = fixed bank 6
;     $8000 = fixed bank 4
;     $A000 = fixed bank 5
;     $C000 = switchable 8KB bank selected by $E000
;     $E000 = fixed bank 7
; - $8000 disables/acknowledges IRQ, $A000 enables IRQ
; - IRQ fires after 4096 M2 cycles, not covered by this first ROM slice
; - No PRG-RAM; $6000-$7FFF is PRG-ROM

.define MAPPER_NUM 40
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 8 × 8KB banks
CHR_ROM_8K    = 1        ; 8KB fixed CHR-ROM
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; Mapper 40's switchable 8KB PRG window is at $C000.
BANK_WINDOW_OVERRIDE = $C000
SKIP_SECOND_PRG_8K_SLOT_TESTS = 1

; ============================================================
; Register macros
; ============================================================

.macro select_prg_bank slot, bank
    lda #(bank & $07)
    sta $E000
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
