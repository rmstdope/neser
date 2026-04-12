; m030.2_defs.inc — UNROM 512 (Mapper 30, Submapper 2) — 1-Screen mirroring control
;
; Single register at $8000-$FFFF:
;   bits 4-0: 16KB PRG bank at $8000-$BFFF ($C000-$FFFF fixed to last)
;   bits 6-5: 8KB CHR-RAM bank (4 banks × 8KB = 32KB)
;   bit  7:   mirroring (0=1-screen lower bank A, 1=1-screen upper bank B)
;
; Features:
;   - PRG banking: 16KB switchable at $8000, last bank fixed at $C000
;   - CHR-RAM banking: 4 × 8KB banks
;   - Mirroring: 1-screen A (mode 0) or 1-screen B (mode 1) via bit 7
;   - No IRQ, No PRG-RAM, bus conflicts apply
;
; set_mirroring sets PRG=0 and CHR=0 as side effects (safe from fixed bank).

.define MAPPER_NUM 30
.define SUBMAPPER_NUM 2

PRG_ROM_16K   = 16       ; 256KB = 16 × 16KB
CHR_ROM_8K    = 0        ; CHR-RAM only
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 8        ; 32KB CHR-RAM (4 × 8KB)

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0
.define HAS_CHR_RAM_BANKING 1    ; CHR-RAM bank switching
.define HAS_CHR_FIXED_LOWER 0   ; No fixed lower CHR page; entire 8KB window switches
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 1
.define MAX_MIRRORING_MODES 2       ; 1-screen A and 1-screen B only

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 8       ; 8KB CHR-RAM
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4      ; 4 × 8KB CHR-RAM banks

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank via a bus-conflict-safe address in the switchable bank.
.macro select_prg_bank slot, bank
    lda #(bank & $1F)
    sta $BFFF
.endmacro

; Select 8KB CHR-RAM bank via a bus-conflict-safe address.
.macro select_chr_bank slot, bank
    lda #((bank & $03) << 5)
    sta $BFFF
.endmacro

; Set 1-screen mirroring: mode 0=1-screen A (lower), mode 1=1-screen B (upper)
; PRG and CHR bits are set to 0 as side effect (safe when running from fixed bank)
.macro set_mirroring mode
    .if mode = 0
        lda #$00            ; bit 7=0: 1-screen A
    .else
        lda #$80            ; bit 7=1: 1-screen B
    .endif
    sta $BFFF
.endmacro
