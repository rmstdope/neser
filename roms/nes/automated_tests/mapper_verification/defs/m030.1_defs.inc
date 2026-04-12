; m030.1_defs.inc — UNROM 512 (Mapper 30, Submapper 1) — Flashable, no bus conflicts
;
; Spec: https://nesdev-wiki.nes.science/wikipages/UNROM_512.xhtml
;
; Identical to submapper 0 except:
;   - No bus conflicts (flashable board configuration)
;   - Battery flag set in header (signals flash capability)
;
; Single register at $C000-$FFFF (flashable boards use $C000 mask):
;   bits 4-0: 16KB PRG bank at $8000-$BFFF ($C000-$FFFF fixed to last)
;   bits 6-5: 8KB CHR-RAM bank (4 banks × 8KB = 32KB)
;   bit  7:   unused in SM1 (not mirroring control)

.define MAPPER_NUM 30
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 16       ; 256KB = 16 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 8        ; 32KB CHR-RAM (4 × 8KB)

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_CHR_RAM_BANKING 1    ; CHR-RAM bank switching
.define HAS_CHR_FIXED_LOWER 0
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0      ; No bus conflicts (flashable board)
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4      ; 4 × 8KB CHR-RAM banks

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank — no bus conflicts so can write to any address
.macro select_prg_bank slot, bank
    lda #(bank & $1F)       ; bits 4-0 = PRG bank; bits 6-5 = CHR (0)
    sta $C000               ; Flashable boards use $C000 mask
.endmacro

; Select 8KB CHR-RAM bank
.macro select_chr_bank slot, bank
    lda #((bank & $03) << 5)  ; bits 6-5 = CHR bank; bits 4-0 = PRG (0)
    sta $C000
.endmacro

; No mirroring control in SM1
.macro set_mirroring mode
.endmacro
