; m030.0_defs.inc — UNROM 512 (Mapper 30, Submapper 0) — Fixed mirroring
;
; Single register at $8000-$FFFF:
;   bits 4-0: 16KB PRG bank at $8000-$BFFF ($C000-$FFFF fixed to last)
;   bits 6-5: 8KB CHR-RAM bank (4 banks × 8KB = 32KB)
;   bit  7:   unused in SM0 (not mirroring control — that is SM2)
;
; Features:
;   - PRG banking: 16KB switchable at $8000, last bank fixed at $C000
;   - CHR-RAM banking: 4 × 8KB banks
;   - Fixed mirroring (solder pads; vertical for this test ROM)
;   - No IRQ, No PRG-RAM, bus conflicts apply
;
; Note: PRG and CHR share one register. select_prg_bank sets CHR bits to 0;
; select_chr_bank sets PRG bits to 0. Safe because the fixed $C000 bank is
; always accessible and the tests run from there.

.define MAPPER_NUM 30
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 16       ; 256KB = 16 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 8        ; 32KB CHR-RAM (4 × 8KB)

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_CHR_RAM_BANKING 1    ; CHR-RAM bank switching
.define HAS_CHR_FIXED_LOWER 0   ; No fixed lower CHR page; entire 8KB window switches
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1
.define MAX_MIRRORING_MODES 0       ; SM0: no mirroring control

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4      ; 4 × 8KB CHR-RAM banks

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank using a write address that stays at linker fill ($FF)
; in the currently mapped bank, so real bus conflicts do not corrupt the value.
.macro select_prg_bank slot, bank
    lda #(bank & $1F)       ; bits 4-0 = PRG bank; bits 6-5 = CHR (0)
    sta $BFFF
.endmacro

; Select 8KB CHR-RAM bank using the same bus-conflict-safe write address.
.macro select_chr_bank slot, bank
    lda #((bank & $03) << 5)  ; bits 6-5 = CHR bank; bits 4-0 = PRG (0)
    sta $BFFF
.endmacro

; No mirroring control in SM0
.macro set_mirroring mode
.endmacro
