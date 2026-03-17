; m022.0_defs.inc — VRC2a (Mapper 22, Submapper 0) — Konami PCB 351618
;
; PCB: 351618, address wiring A0=NES_A1 (offset $02), A1=NES_A0 (offset $01)
;
; Features:
;   - PRG banking: 8KB switchable at $8000, $A000; 16KB fixed at $C000-$FFFF
;   - CHR banking: 8 × 1KB switchable (nibble-split; VRC2a right-shift quirk)
;   - Mirroring: H/V only (VRC2 does not support 1-screen)
;   - No PRG-RAM, No IRQ
;   - No bus conflicts
;
; VRC2a CHR quirk: hardware right-shifts bank value by 1 before storing.
; To select CHR bank N, write N*2 (shifted left by 1) to the register.

.define MAPPER_NUM 22
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB (8 test banks + 2KB font + 6 spare)
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
.define MAX_MIRRORING_MODES 2       ; H/V only (no 1-screen on VRC2)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4      ; 4 windows: 2 switchable + 2 fixed
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; VRC2a address wiring: A0=NES_A1 (bit 1 offset), A1=NES_A0 (bit 0 offset)
VRC2A_A0 = $02              ; NES address bit A1
VRC2A_A1 = $01              ; NES address bit A0

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000-$9FFF, slot 1 = $A000-$BFFF
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $1F)
        sta $8000
    .elseif slot = 1
        lda #(bank & $1F)
        sta $A000
    .endif
.endmacro

; Select 1KB CHR bank (VRC2a quirk: write bank*2 to the nibble registers)
; The hardware right-shifts the stored value by 1, so effective bank = written/2
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #((bank * 2) & $0F)
        sta $B000
        lda #(((bank * 2) >> 4) & $0F)
        sta $B000 | VRC2A_A0
    .elseif slot = 1
        lda #((bank * 2) & $0F)
        sta $B000 | VRC2A_A1
        lda #(((bank * 2) >> 4) & $0F)
        sta $B000 | VRC2A_A0 | VRC2A_A1
    .elseif slot = 2
        lda #((bank * 2) & $0F)
        sta $C000
        lda #(((bank * 2) >> 4) & $0F)
        sta $C000 | VRC2A_A0
    .elseif slot = 3
        lda #((bank * 2) & $0F)
        sta $C000 | VRC2A_A1
        lda #(((bank * 2) >> 4) & $0F)
        sta $C000 | VRC2A_A0 | VRC2A_A1
    .elseif slot = 4
        lda #((bank * 2) & $0F)
        sta $D000
        lda #(((bank * 2) >> 4) & $0F)
        sta $D000 | VRC2A_A0
    .elseif slot = 5
        lda #((bank * 2) & $0F)
        sta $D000 | VRC2A_A1
        lda #(((bank * 2) >> 4) & $0F)
        sta $D000 | VRC2A_A0 | VRC2A_A1
    .elseif slot = 6
        lda #((bank * 2) & $0F)
        sta $E000
        lda #(((bank * 2) >> 4) & $0F)
        sta $E000 | VRC2A_A0
    .elseif slot = 7
        lda #((bank * 2) & $0F)
        sta $E000 | VRC2A_A1
        lda #(((bank * 2) >> 4) & $0F)
        sta $E000 | VRC2A_A0 | VRC2A_A1
    .endif
.endmacro

; Set nametable mirroring: 0=vertical, 1=horizontal (VRC2 supports H/V only)
.macro set_mirroring mode
    lda #mode
    sta $9000
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
.macro init_chr_font
    select_chr_bank 0, 8
    select_chr_bank 1, 9
.endmacro
