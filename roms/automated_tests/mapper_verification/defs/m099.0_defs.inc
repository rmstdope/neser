; m099.0_defs.inc — VS System (Mapper 99, Submapper 0) — Standard 32KB
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_099.xhtml
;
; VS System mapper for most VS arcade games (Vs. Super Mario Bros., etc.):
;   - PRG: 32KB fixed (no PRG banking)
;   - CHR: 16KB = 2 × 8KB banks, selected by $4016 bit 2 (2A03 OUT2)
;   - Mirroring: Four-screen (hardwired on VS System board)
;   - PRG-RAM: 2KB at $6000-$67FF (mirrored in $6000-$7FFF)
;   - No bus conflicts, no IRQ
;
; Bank switching is via I/O register $4016 bit 2, NOT mapper address space.
; The select_chr_bank macro writes to $4016 with bit 0 = 0 (strobe off)
; and bit 2 set to select the CHR bank.

.define MAPPER_NUM 99
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 2        ; 32KB = 2 × 16KB (fixed, not banked)
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; NES 2.0 header flags
MIRRORING           = 0       ; Ignored: four-screen overrides
HAS_FOUR_SCREEN_VRAM = 1      ; Hardware four-screen VRAM

; Capability flags
.define HAS_PRG_BANKING 0
.define HAS_CHR_BANKING 1
.define HAS_FOUR_SCREEN 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No switchable mirroring

; Bank sizes
.define PRG_BANK_SIZE 32      ; Treated as 32KB fixed for test framework
.define CHR_BANK_SIZE 8       ; 8KB switchable
.define PRG_BANK_COUNT 1      ; All PRG is fixed
.define CHR_BANK_COUNT 2      ; 2 × 8KB CHR banks

; Only 2 CHR banks — skip tests for banks 2 and 3
SKIP_CHR_BANK_2_3_TESTS = 1

; ============================================================
; Register Macros
; ============================================================

; $4016 I/O register used for bank switching
CONTROLLER_PORT = $4016

; Select 8KB CHR bank via $4016 bit 2
; slot is always 0 (single 8KB window at PPU $0000)
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
    sta CONTROLLER_PORT         ; bit 2 = 0 → CHR bank 0
.endmacro
