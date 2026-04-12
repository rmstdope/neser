; m075.0_defs.inc — VRC1 (Mapper 75, Submapper 0)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/VRC1.xhtml
;
; Features:
;   - PRG banking: 3 × 8KB switchable + 8KB fixed last
;     $8000: slot 0, $A000: slot 1, $C000: slot 2
;   - CHR banking: 2 × 4KB switchable
;     $E000: low 4 bits of CHR bank at $0000
;     $F000: low 4 bits of CHR bank at $1000
;     $9000 bit 1: high bit of CHR bank at $0000
;     $9000 bit 2: high bit of CHR bank at $1000
;   - Mirroring: H/V via $9000 bit 0
;   - No IRQ, no PRG-RAM, no bus conflicts

.define MAPPER_NUM 75
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 4 × 4KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; H/V (>= 4 enables V/H tests)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 4       ; 4KB switchable
.define PRG_BANK_COUNT 4      ; 3 switchable + 1 fixed
.define CHR_BANK_COUNT 2      ; 2 × 4KB slots

; ============================================================
; Register Addresses
; ============================================================

VRC1_PRG0    = $8000   ; PRG Select 0 (bits 0-3) → 8KB at $8000
VRC1_CTRL    = $9000   ; Bit 0: mirroring, Bit 1: CHR0 high, Bit 2: CHR1 high
VRC1_PRG1    = $A000   ; PRG Select 1 (bits 0-3) → 8KB at $A000
VRC1_PRG2    = $C000   ; PRG Select 2 (bits 0-3) → 8KB at $C000
VRC1_CHR0    = $E000   ; CHR Select 0 low 4 bits → 4KB at $0000
VRC1_CHR1    = $F000   ; CHR Select 1 low 4 bits → 4KB at $1000

; ============================================================
; Register Macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000, slot 1 = $A000, slot 2 = $C000
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC1_PRG0
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC1_PRG1
    .elseif slot = 2
        lda #(bank & $0F)
        sta VRC1_PRG2
    .endif
.endmacro

; Select 4KB CHR bank (low 4 bits only; high bit via $9000 is left at 0)
; slot 0 → $E000 (PPU $0000), slot 1 → $F000 (PPU $1000)
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC1_CHR0
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC1_CHR1
    .endif
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
; $9000 bit 0: 0=V, 1=H (CHR high bits cleared — fine during mirroring test)
.macro set_mirroring mode
    .if mode = 0
        lda #$00                ; Vertical
    .elseif mode = 1
        lda #$01                ; Horizontal
    .endif
    sta VRC1_CTRL
.endmacro

; Map font CHR bank (0) to PPU $0000-$0FFF
.macro init_chr_font
    lda #0
    sta VRC1_CTRL               ; Clear CHR high bits and set vertical mirroring
    sta VRC1_CHR0               ; Map bank 0 to $0000
.endmacro
