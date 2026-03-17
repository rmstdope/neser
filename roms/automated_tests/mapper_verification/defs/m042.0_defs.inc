; m042.0_defs.inc — Mario Baby / Ai Senshi Nicol (Mapper 42, Submapper 0)
;
; NESdev backup wiki notes:
; - CPU $6000-$7FFF: 8KB switchable PRG-ROM bank
; - CPU $8000-$FFFF: fixed to last 32KB of PRG-ROM
; - PPU $0000-$1FFF: 8KB switchable CHR-ROM bank
; - $8000 [.... CCCC] selects CHR bank
; - $E000 [.... PPPP] selects PRG bank for $6000-$7FFF
; - $E001 [.... M...] sets mirroring (0=vertical, 1=horizontal)
; - $E002 [......E.] controls a cycle IRQ, not covered by this first ROM slice
; - Register addresses are masked with $E003
;
; This mapper cannot use the shared $6000 status-byte protocol because
; $6000-$7FFF is PRG-ROM, not writable RAM. Use console verification instead.

.define MAPPER_NUM 42
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 16 × 8KB banks
CHR_ROM_8K    = 16       ; 128KB = 16 × 8KB banks
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4

; Bank sizes
.define PRG_BANK_SIZE 8
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; Mapper 42's variable 8KB PRG window is at $6000, not $8000.
BANK_WINDOW_OVERRIDE = $6000
SKIP_SECOND_PRG_8K_SLOT_TESTS = 1

; ============================================================
; Register macros
; ============================================================

.macro select_prg_bank slot, bank
    lda #(bank & $0F)
    sta $E000
.endmacro

.macro select_chr_bank slot, bank
    lda #(bank & $0F)
    sta $8000
.endmacro

.macro set_mirroring mode
    .if mode = 0
        lda #$00
        sta $E001
    .elseif mode = 1
        lda #$08
        sta $E001
    .endif
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
