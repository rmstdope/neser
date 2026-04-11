; m154.0_defs.inc — NAMCOT-3453 (Mapper 154, Submapper 0)
;
; Mapper 154 is identical to Mapper 88 (Namcot 3443) with one addition:
;   - Bit 6 of any $8000–$FFFF write controls one-screen mirroring
;   - M=0 → Single Screen A (CIRAM A10=0)
;   - M=1 → Single Screen B (CIRAM A10=1)
;
; The only known game: Devil Man (Namco, 1990).
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_154.xhtml
;       https://nesdev-wiki.nes.science/wikipages/Namco_108.xhtml
;
; Register map (identical to Mapper 88):
;   $8000 [.Mxx xRRR]  M=mirroring bit, RRR=register select (0–5)
;   $8001 [DDDD DDDD]  D=register data (bit 6 here also updates M)
;
; PRG banking (mode 0, fixed):
;   R6 → 8KB bank at $8000–$9FFF
;   R7 → 8KB bank at $A000–$BFFF
;   Fixed second-to-last bank at $C000–$DFFF
;   Fixed last bank at $E000–$FFFF
;
; CHR banking (Namco 108 split layout):
;   R0, R1 → 2KB banks at $0000 and $0800 (lower 64KB, bit 6 = 0)
;   R2–R5  → 1KB banks at $1000–$1FFF  (upper 64KB, bit 6 forced = 1)
;   CHR mode bit (bit 7 of $8000) is always 0.
;   CHR banking is not covered by the standard CHR test (split layout
;   is not directly expressible via linear bank indices).
;
; NOTE: $8001 writes also update the M bit (bit 6 of data byte).
; select_prg_bank and select_chr_bank only use bank values < 64 (bit 6=0),
; keeping mirroring in Single Screen A during banking tests.
; set_mirroring writes only to $8000, not $8001, to avoid corrupting CHR regs.

.define MAPPER_NUM 154
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks (font in banks 8+9)
PRG_RAM_SHIFT = 7        ; 8KB PRG-RAM for $6000 status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0   ; Split lower/upper 64KB layout not compatible with standard test
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 2   ; 0 = Single Screen A, 1 = Single Screen B

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable slots
.define CHR_BANK_SIZE 1       ; 1KB granularity (unused — HAS_CHR_BANKING=0)
.define PRG_BANK_COUNT 2      ; R6 ($8000) and R7 ($A000)
.define CHR_BANK_COUNT 2      ; R0, R1 (for font init only)

; ============================================================
; Register Addresses
; ============================================================

N3453_BANK_SELECT = $8000  ; [.Mxx xRRR] M=mirroring, RRR=register 0–5
N3453_BANK_DATA   = $8001  ; [DDDD DDDD] bank data; bit 6 also updates M

; ============================================================
; PRG Banking
; ============================================================

; Select 8KB PRG bank for a given slot.
; slot 0 = R6 → $8000–$9FFF
; slot 1 = R7 → $A000–$BFFF
; Bank values 0–5 all have bit 6=0, so M stays at 0 (Single Screen A).
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #6                  ; R6: PRG at $8000–$9FFF
        sta N3453_BANK_SELECT
        lda #bank
        sta N3453_BANK_DATA
    .elseif slot = 1
        lda #7                  ; R7: PRG at $A000–$BFFF
        sta N3453_BANK_SELECT
        lda #bank
        sta N3453_BANK_DATA
    .endif
.endmacro

; ============================================================
; CHR Banking (not used — HAS_CHR_BANKING = 0)
; ============================================================

.macro select_chr_bank slot, bank
.endmacro

; Map font to PPU $0000–$07FF via R0.
; R0 = 8 (even, lower 64KB range, bit 6=0 → M stays Single Screen A).
.macro init_chr_font
    lda #0                  ; Select R0 (register 0), M=0
    sta N3453_BANK_SELECT
    lda #8                  ; R0 = banks 8+9 (2KB pair, lower range)
    sta N3453_BANK_DATA
.endmacro

; ============================================================
; Mirroring Control
; ============================================================

; set_mirroring mode:
;   mode 0 = Single Screen A (M=0): only writes $8000, no $8001
;   mode 1 = Single Screen B (M=1): only writes $8000, no $8001
;   mode 2 = default/restore → Single Screen A
;
; Writing only $8000 avoids disturbing the last-written CHR register value.
; Any subsequent select_prg_bank or select_chr_bank will issue its own
; $8000 + $8001 pair, overriding the bank select state left here.
.macro set_mirroring mode
    .if mode = 1
        lda #$40            ; R=0, M=1 → Single Screen B
    .else
        lda #$00            ; R=0, M=0 → Single Screen A (default/restore)
    .endif
    sta N3453_BANK_SELECT
.endmacro

; ============================================================
; Unused stubs
; ============================================================

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
