; m070.0_defs.inc — Bandai (Mapper 70, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/INES_Mapper_070
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last 16KB fixed at $C000
;   - CHR banking: 8KB switchable at $0000-$1FFF
;   - Fixed mirroring (solder pad)
;   - No PRG-RAM, no IRQ
;   - Bus conflicts (74HC161 latch)
;
; Register $8000-$FFFF:
;   7  bit  0
;   ---- ----
;   xPPP CCCC
;    |||  ||||
;    |||  ++++- Select 8KB CHR ROM bank for PPU $0000-$1FFF
;    +++------ Select 16KB PRG ROM bank for CPU $8000-$BFFF

.define MAPPER_NUM 70
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB banks
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0     ; Writes go to $FFF0 fill area ($FF); bus-conflict-safe
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 16       ; 16KB switchable
.define CHR_BANK_SIZE 8        ; 8KB switchable
.define PRG_BANK_COUNT 2       ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 1       ; 1 × 8KB slot

; ============================================================
; Register macros
; ============================================================

; Writes to $FFF0 (fill=$FF) for bus-conflict safety.

; Bank shift for trampoline: PRG bits at 4-6 need left shift of 4
TRAMPOLINE_BANK_SHIFT = 4

; Select 16KB PRG bank at $8000-$BFFF (bits 6-4).
; Also resets CHR bank to 0.
.macro select_prg_bank slot, bank
    lda #(bank << 4)
    sta $FFF0
.endmacro

; Select 8KB CHR bank at $0000-$1FFF (bits 3-0).
; Also resets PRG bank to 0.
.macro select_chr_bank slot, bank
    lda #bank
    sta $FFF0
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro

; Restore font: CHR bank 0 has the font at PPU $0200.
; Font is in the default CHR bank (0), so select CHR 0 without clobbering PRG.
.macro init_chr_font
    lda #$00               ; CHR bank 0 (font), PRG bank 0
    sta $FFF0
.endmacro
