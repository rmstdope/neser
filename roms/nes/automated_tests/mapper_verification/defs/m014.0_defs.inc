; m014.0_defs.inc — SL-1632 (Mapper 14, Submapper 0) — MMC3/VRC2 hybrid
;
; Features:
;   - Supervisor register at $A131 selects MMC3 vs VRC2 mode
;   - PRG banking: 8KB switchable (MMC3 mode, same as mapper 4)
;   - CHR banking: 1KB switchable (MMC3 R0-R5) + outer CHR A18 bits
;   - Mirroring: H/V programmable (MMC3 $A000)
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (MMC3, only in MMC3 mode)
;
; Supervisor register ($A131):
;   D~A.B. C.M.
;     |  | | +-- Mapper mode (0=VRC2, 1=MMC3)
;     |  | +---- CHR A18 for PPU $0000-$0FFF
;     |  +------ CHR A18 for PPU $1000-$17FF
;     +--------- CHR A18 for PPU $1800-$1FFF
;
; We test in MMC3 mode (M=1) since that's the primary use case.

.define MAPPER_NUM 14
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 64       ; 512KB = 64 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 1
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp/MMC3

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; Supervisor register (Mapper 14 specific)
SUPERVISOR_REG   = $A131

; ============================================================
; MMC3 Core Macros (shared with mappers 4, 12, 14)
; ============================================================

.include "mmc3_core_macros.inc"

; ============================================================
; Mapper 14 Specific Macros
; ============================================================

; Switch to MMC3 mode via supervisor register
; Also sets CHR outer bits to 0 (lower 256KB)
.macro set_mmc3_mode
    lda #$02                ; M=1 (MMC3 mode), all CHR A18=0
    sta SUPERVISOR_REG
.endmacro

; Map font CHR banks to $0000-$07FF via R0
.macro init_chr_font
    ; R0: 2KB at $0000-$07FF → banks 8+9
    lda #0
    sta MMC3_BANK_SELECT
    lda #8
    sta MMC3_BANK_DATA
.endmacro

; ============================================================
; VRC2-mode registers and macros (SL-1632 specific)
; ============================================================
;
; When the supervisor register bit 1 ($A131 & $02) is clear, the mapper
; operates in VRC2 mode with the following register layout:
;   $8000       — PRG bank 0 (8KB at $8000-$9FFF)
;   $9000       — Mirroring (bit 0: 0=V, 1=H)
;   $A000       — PRG bank 1 (8KB at $A000-$BFFF)
;   $B000-$E003 — CHR banks 0-7 (nibble-split, 1KB each)
;
; CHR register addressing: low nibble at even address, high nibble at odd.
;   Slot 0: $B000 (lo) / $B001 (hi)
;   Slot 1: $B002 (lo) / $B003 (hi)
;   Slot 2: $C000 (lo) / $C001 (hi)
;   Slot 3: $C002 (lo) / $C003 (hi)
;   Slot 4: $D000 (lo) / $D001 (hi)
;   Slot 5: $D002 (lo) / $D003 (hi)
;   Slot 6: $E000 (lo) / $E001 (hi)
;   Slot 7: $E002 (lo) / $E003 (hi)

SL1632_VRC2_PRG0      = $8000
SL1632_VRC2_PRG1      = $A000
SL1632_VRC2_MIRROR    = $9000

SL1632_VRC2_CHR0_LO   = $B000
SL1632_VRC2_CHR0_HI   = $B001
SL1632_VRC2_CHR1_LO   = $B002
SL1632_VRC2_CHR1_HI   = $B003
SL1632_VRC2_CHR2_LO   = $C000
SL1632_VRC2_CHR2_HI   = $C001
SL1632_VRC2_CHR3_LO   = $C002
SL1632_VRC2_CHR3_HI   = $C003
SL1632_VRC2_CHR4_LO   = $D000
SL1632_VRC2_CHR4_HI   = $D001
SL1632_VRC2_CHR5_LO   = $D002
SL1632_VRC2_CHR5_HI   = $D003
SL1632_VRC2_CHR6_LO   = $E000
SL1632_VRC2_CHR6_HI   = $E001
SL1632_VRC2_CHR7_LO   = $E002
SL1632_VRC2_CHR7_HI   = $E003

; Switch to VRC2 mode via supervisor register
.macro set_vrc2_mode
    lda #$00                ; M=0 (VRC2 mode), all CHR A18=0
    sta SUPERVISOR_REG
.endmacro

; Select 8KB VRC2 PRG bank
; slot 0 = $8000-$9FFF, slot 1 = $A000-$BFFF
.macro select_vrc2_prg_bank slot, bank
    .if slot = 0
        lda #bank
        sta SL1632_VRC2_PRG0
    .elseif slot = 1
        lda #bank
        sta SL1632_VRC2_PRG1
    .endif
.endmacro

; Select 1KB VRC2 CHR bank (nibble-split write)
.macro select_vrc2_chr_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR0_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR0_HI
    .elseif slot = 1
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR1_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR1_HI
    .elseif slot = 2
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR2_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR2_HI
    .elseif slot = 3
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR3_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR3_HI
    .elseif slot = 4
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR4_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR4_HI
    .elseif slot = 5
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR5_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR5_HI
    .elseif slot = 6
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR6_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR6_HI
    .elseif slot = 7
        lda #(bank & $0F)
        sta SL1632_VRC2_CHR7_LO
        lda #((bank >> 4) & $0F)
        sta SL1632_VRC2_CHR7_HI
    .endif
.endmacro

; Set VRC2 nametable mirroring
; mode 0 = Vertical, mode 1 = Horizontal
.macro set_vrc2_mirroring mode
    lda #mode
    sta SL1632_VRC2_MIRROR
.endmacro

; Map font CHR banks in VRC2 mode
; Puts banks 8 and 9 into slots 0 and 1 ($0000-$07FF)
.macro init_vrc2_chr_font
    select_vrc2_chr_bank 0, 8
    select_vrc2_chr_bank 1, 9
.endmacro
