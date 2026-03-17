; m029.0_defs.inc — Sealie Computing / RET-CUFROM revD (Mapper 29, Submapper 0)
;
; Single register at $8000-$FFFF:
;   bits 2-0: 16KB PRG bank at $8000-$BFFF (8 banks, $C000-$FFFF fixed to last)
;   bit  3:   CHR-RAM bank bit 0
;   bit  4:   CHR-RAM bank bit 1 (4 banks × 8KB = 32KB CHR-RAM)
;
; Features:
;   - PRG banking: 16KB switchable at $8000, last bank fixed at $C000
;   - CHR-RAM banking: 4 × 8KB banks (combined PRG+CHR register)
;   - Fixed vertical mirroring
;   - No IRQ, No PRG-RAM, No bus conflicts
;
; Note: PRG and CHR bank bits share one register. For test simplicity,
; select_prg_bank sets CHR bits to 0, and select_chr_bank sets PRG bits to 0.
; This is safe because the PRG banking test runs from the fixed $C000 bank,
; and the CHR banking test does not require a specific PRG bank.

.define MAPPER_NUM 29
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 8        ; 32KB CHR-RAM (4 × 8KB)

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0        ; No CHR-ROM banking
.define HAS_CHR_RAM_BANKING 1    ; CHR-RAM bank switching
.define HAS_CHR_FIXED_LOWER 0   ; No fixed lower CHR page; entire 8KB window switches
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; Fixed vertical mirroring

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4      ; 4 × 8KB CHR-RAM banks

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000 (CHR bank bits left as 0)
.macro select_prg_bank slot, bank
    lda #(bank & $07)       ; bits 2-0 = PRG bank; bits 4:3 = CHR bank (0)
    sta $8000
.endmacro

; Select 8KB CHR-RAM bank (PRG bank bits left as 0)
; slot is unused (only one CHR window)
.macro select_chr_bank slot, bank
    lda #((bank & $03) << 3)  ; bits 4:3 = CHR bank; bits 2-0 = PRG bank (0)
    sta $8000
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro
