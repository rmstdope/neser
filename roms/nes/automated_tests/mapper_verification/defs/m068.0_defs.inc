; m068.0_defs.inc — Sunsoft-4 (Mapper 68, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/INES_Mapper_068
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last 16KB fixed at $C000
;   - CHR banking: 4 × 2KB switchable banks
;   - Mirroring: V/H/1scA/1scB via $E000 bits 0-1
;   - PRG-RAM: 8KB at $6000-$7FFF, enable via $F000 bit 4
;   - NT-from-CHR: $E000 bit 4 enables mapping nametables from CHR-ROM
;     (not tested in this ROM; planned as a follow-up)
;   - No IRQ, no bus conflicts
;
; Register layout:
;   $8000-$8FFF: CHR bank 0 → PPU $0000-$07FF
;   $9000-$9FFF: CHR bank 1 → PPU $0800-$0FFF
;   $A000-$AFFF: CHR bank 2 → PPU $1000-$17FF
;   $B000-$BFFF: CHR bank 3 → PPU $1800-$1FFF
;   $C000-$CFFF: NT bank 0 (when $E000.4 = 1)
;   $D000-$DFFF: NT bank 1 (when $E000.4 = 1)
;   $E000-$EFFF: Nametable control [..N. ..MM]
;   $F000-$FFFF: PRG bank + PRG-RAM enable [...E PPPP]

.define MAPPER_NUM 68
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB banks
CHR_ROM_8K    = 4        ; 32KB = 16 × 2KB banks
PRG_RAM_SHIFT = 7        ; 8KB PRG-RAM
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; V/H/1scA/1scB

; Bank sizes
.define PRG_BANK_SIZE 16       ; 16KB switchable
.define CHR_BANK_SIZE 2        ; 2KB switchable
.define PRG_BANK_COUNT 2       ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4       ; 4 × 2KB slots

; ============================================================
; Register Addresses
; ============================================================

S4_CHR0     = $8000   ; CHR bank 0 → $0000-$07FF
S4_CHR1     = $9000   ; CHR bank 1 → $0800-$0FFF
S4_CHR2     = $A000   ; CHR bank 2 → $1000-$17FF
S4_CHR3     = $B000   ; CHR bank 3 → $1800-$1FFF
S4_NT0      = $C000   ; NT bank 0 (ROM mode)
S4_NT1      = $D000   ; NT bank 1 (ROM mode)
S4_MIRROR   = $E000   ; Nametable control [..N...MM]
S4_PRG      = $F000   ; PRG bank + PRG-RAM enable [...E PPPP]

; ============================================================
; Register Macros
; ============================================================

; Select 16KB PRG bank at $8000-$BFFF
.macro select_prg_bank slot, bank
    lda #($10 | bank)      ; Keep PRG-RAM enabled (bit 4 = 1)
    sta S4_PRG
.endmacro

; Select 2KB CHR bank
; slot 0 → $8000, slot 1 → $9000, slot 2 → $A000, slot 3 → $B000
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta S4_CHR0
    .elseif slot = 1
        lda #bank
        sta S4_CHR1
    .elseif slot = 2
        lda #bank
        sta S4_CHR2
    .elseif slot = 3
        lda #bank
        sta S4_CHR3
    .endif
.endmacro

; Set mirroring: 0=vertical, 1=horizontal, 2=1-screen A, 3=1-screen B
; Keep NT-ROM mode disabled (bit 4 = 0) for standard mirroring tests
.macro set_mirroring mode
    lda #mode
    sta S4_MIRROR
.endmacro

; Enable PRG-RAM at $6000-$7FFF for status byte protocol
.macro init_prg_ram
    lda #$10               ; Bit 4 = PRG-RAM enable, bank 0
    sta S4_PRG
.endmacro

; Map font CHR bank (4) to PPU $0000-$07FF
.macro init_chr_font
    lda #4                 ; CHR bank 4 contains font
    sta S4_CHR0            ; Map to $0000-$07FF
.endmacro

; Enable PRG-RAM (already done by init_prg_ram at startup)
.macro enable_prg_ram
    lda #$10               ; Bit 4 = PRG-RAM enable, bank 0
    sta S4_PRG
.endmacro
