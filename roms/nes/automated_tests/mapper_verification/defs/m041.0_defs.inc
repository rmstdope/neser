; m041.0_defs.inc — Caltron 6-in-1 (Mapper 41, Submapper 0)
;
; NESdev spec:
; - Outer bank select at $6000-$67FF, encoded in address lines
; - 32KB PRG banking via address bits PP
; - Mirroring via address bit M (0=vertical, 1=horizontal)
; - No PRG-RAM ("PRG RAM: Impossible")
; - Optional inner CHR banking only when E bit is set; not covered in this first ROM
;
; This mapper cannot use the shared $6000 status-byte protocol because writes in
; that range are mapper control writes. Use console verification instead.

.define MAPPER_NUM 41
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 4 × 32KB banks addressable via PP
CHR_ROM_8K    = 1        ; 8KB CHR-ROM (font only; no CHR banking tests yet)
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

; Disable $6000 status-byte writes; use console verification in Rust tests.
CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

TRAMPOLINE_BANK_ADDR = $6000
TRAMPOLINE_BANK_BY_ADDRESS = 1

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank via outer-bank-select address bits PP.
; Address format: $6000 + PP with M=0, CC=0, E=0.
.macro select_prg_bank slot, bank
    lda #0
    sta $6000 + (bank & $03)
.endmacro

.macro select_chr_bank slot, bank
.endmacro

; Set mirroring while preserving PRG bank 0.
; Generic nametable tests use mode 0=vertical, 1=horizontal.
.macro set_mirroring mode
    .if mode = 0
        lda #0
        sta $6000
    .elseif mode = 1
        lda #0
        sta $6020
    .endif
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
