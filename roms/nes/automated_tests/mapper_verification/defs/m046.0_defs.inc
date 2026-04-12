; m046.0_defs.inc — Rumble Station (Mapper 46, Submapper 0)
;
; NESdev notes:
; - Outer register at $6000-$7FFF: [CCCC PPPP]
; - Inner register at $8000-$FFFF: [.CCC ...P]
; - No PRG-RAM
; - 32KB PRG banking, 8KB CHR banking
; - Bus conflicts are not well documented; use a fill-$FF write for the $8000+
;   inner register path to stay conservative.

.define MAPPER_NUM 46
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 4 × 32KB banks for this test ROM
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB banks
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

TRAMPOLINE_BANK_BY_SPLIT_OUTER_INNER = 1
TRAMPOLINE_OUTER_BANK_ADDR = $6000
TRAMPOLINE_INNER_BANK_ADDR = $FFF0

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank.
; Outer nibble PPPP provides high PRG bits, inner bit P provides low PRG bit.
.macro select_prg_bank slot, bank
    lda #((bank >> 1) & $0F)
    sta $6000
    lda #(bank & $01)
    sta $FFF0
.endmacro

; Select 8KB CHR bank.
; Outer nibble CCCC provides high CHR bits, inner bits CCC provide low CHR bits.
.macro select_chr_bank slot, bank
    lda #(((bank >> 3) & $0F) << 4)
    sta $6000
    lda #((bank & $07) << 4)
    sta $FFF0
.endmacro

.macro set_mirroring mode
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
