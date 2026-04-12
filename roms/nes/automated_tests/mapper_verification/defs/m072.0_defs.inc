; m072.0_defs.inc — Jaleco JF-17 (Mapper 72, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/INES_Mapper_072
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last 16KB fixed at $C000
;   - CHR banking: 8KB switchable at $0000-$1FFF
;   - Fixed mirroring (cuttable trace)
;   - No PRG-RAM, no IRQ
;   - Bus conflicts (74HC161 latch)
;   - Edge-detection banking: bank switches trigger on 0→1 rising edge
;     - Bit 7 (P): rising edge loads PRG bank from D[2:0]
;     - Bit 6 (C): rising edge loads CHR bank from D[3:0]
;
; Register $8000-$FFFF:
;   7  bit  0
;   ---- ----
;   PCxx DDDD
;   ||   ||||
;   ||   ++++- Bank number data (D[2:0] for PRG, D[3:0] for CHR)
;   |+-------- CHR bank select trigger (rising edge 0→1)
;   +--------- PRG bank select trigger (rising edge 0→1)

.define MAPPER_NUM 72
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
.define HAS_BUS_CONFLICTS 0     ; Edge-detection banking is incompatible with shared bus conflict test
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
; Edge detection: write 0 first to clear latch bits, then write with P/C set.

; Bank shift for trampoline: no shift needed; but P-bit is bit 7
TRAMPOLINE_BANK_SHIFT = 7

; Select 16KB PRG bank at $8000-$BFFF via rising edge on P-bit.
; Two writes: first clear the P-bit (write 0), then set P-bit with bank.
.macro select_prg_bank slot, bank
    lda #$00               ; Clear latch (P=0, C=0)
    sta $FFF0
    lda #($80 | bank)      ; P=1 + bank in D[2:0]
    sta $FFF0
.endmacro

; Select 8KB CHR bank at $0000-$1FFF via rising edge on C-bit.
; Two writes: first clear the C-bit (write 0), then set C-bit with bank.
.macro select_chr_bank slot, bank
    lda #$00               ; Clear latch (P=0, C=0)
    sta $FFF0
    lda #($40 | bank)      ; C=1 + bank in D[3:0]
    sta $FFF0
.endmacro

; No mirroring control
.macro set_mirroring mode
.endmacro

; Restore font: CHR bank 0 has the font at PPU $0200.
; Use edge-detection: clear latch then set C-bit to select CHR bank 0.
.macro init_chr_font
    lda #$00               ; Clear latch
    sta $FFF0
    lda #$40               ; C=1, bank 0
    sta $FFF0
.endmacro
