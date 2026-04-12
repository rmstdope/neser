; m099.1_defs.inc — VS System (Mapper 99, Submapper 1) — 40KB PRG variant
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_099.xhtml
;
; 40KB PRG variant (Vs. Gumshoe):
;   - PRG: 40KB = 5 × 8KB banks
;     $8000: switchable (bank 0 or bank 4, via $4016 bit 2)
;     $A000: bank 1 (fixed)
;     $C000: bank 2 (fixed)
;     $E000: bank 3 (fixed — code runs here)
;   - CHR: 16KB = 2 × 8KB banks, selected by $4016 bit 2
;   - Mirroring: Four-screen (hardwired on VS System board)
;   - PRG-RAM: 2KB at $6000-$67FF
;   - No bus conflicts, no IRQ
;
; $4016 bit 2 controls BOTH PRG (slot 0 at $8000) and CHR simultaneously.
; The verification ROM uses logical bank IDs 0 and 1 for the two states:
;   - Logical bank 0 → $4016 bit 2 = 0 → hardware PRG bank 0 at $8000
;   - Logical bank 1 → $4016 bit 2 = 1 → hardware PRG bank 4 at $8000

.define MAPPER_NUM 99
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 3        ; 40KB ≈ 3 × 16KB (rounded up; actual 5 × 8KB)
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; NES 2.0 header flags
MIRRORING           = 0       ; Ignored: four-screen overrides
HAS_FOUR_SCREEN_VRAM = 1      ; Hardware four-screen VRAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_FOUR_SCREEN 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No switchable mirroring

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable (slot 0 only)
.define CHR_BANK_SIZE 8       ; 8KB switchable
.define PRG_BANK_COUNT 2      ; 2 logical banks selectable at slot 0
.define CHR_BANK_COUNT 2      ; 2 × 8KB CHR banks

; Only 2 banks in PRG slot 0 — skip the shared test's "bank 2" check
PRG_8K_SLOT0_BANKS_2 = 1
; Only 1 switchable PRG slot (slot 0 at $8000)
SKIP_SECOND_PRG_8K_SLOT_TESTS = 1
; Only 2 CHR banks — skip tests for banks 2 and 3
SKIP_CHR_BANK_2_3_TESTS = 1

; ============================================================
; Register Macros
; ============================================================

; $4016 I/O register used for bank switching
CONTROLLER_PORT = $4016

; Select 8KB PRG bank at slot 0 ($8000) via $4016 bit 2
; Only slot 0 is switchable. bnk 0 → bit 2 = 0, bnk 1 → bit 2 = 1.
.macro select_prg_bank slot, bnk
    .if slot = 0
        .if bnk = 0
            lda #$00            ; bit 2 = 0 → PRG bank 0 at $8000
        .else
            lda #$04            ; bit 2 = 1 → PRG bank 4 at $8000
        .endif
        sta CONTROLLER_PORT
    .endif
.endmacro

; Select 8KB CHR bank via $4016 bit 2
; bnk 0 → bit 2 = 0, bnk 1 → bit 2 = 1
.macro select_chr_bank slot, bnk
    .if bnk = 0
        lda #$00                ; bit 2 = 0 → CHR bank 0
    .else
        lda #$04                ; bit 2 = 1 → CHR bank 1
    .endif
    sta CONTROLLER_PORT
.endmacro

; Map font CHR bank (0) to PPU $0000
.macro init_chr_font
    lda #$00
    sta CONTROLLER_PORT         ; bit 2 = 0 → CHR bank 0, PRG bank 0 at $8000
.endmacro
