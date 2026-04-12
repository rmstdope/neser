; m009.0_defs.inc — MMC2 (Mapper 9, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/MMC2
;
; Features:
;   - PRG banking: 8KB switchable at $8000-$9FFF
;   - 24KB fixed at $A000-$FFFF (last 3 banks)
;   - CHR: 4KB × 2 windows with PPU-triggered FD/FE latch
;   - Mirroring: H/V via $F000 bit 0
;   - No PRG-RAM (PlayChoice only; we skip)
;   - No IRQ
;   - No bus conflicts
;
; Registers:
;   $A000-$AFFF: PRG bank select (bits 0-3)
;   $B000-$BFFF: CHR FD/lower bank select (bits 0-4)
;   $C000-$CFFF: CHR FE/lower bank select (bits 0-4)
;   $D000-$DFFF: CHR FD/upper bank select (bits 0-4)
;   $E000-$EFFF: CHR FE/upper bank select (bits 0-4)
;   $F000-$FFFF: Mirroring (bit 0: 0=V, 1=H)

.define MAPPER_NUM 9
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB = 8 × 4KB
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 0       ; CHR latch, not standard banking
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define HAS_CHR_LATCH 1
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 4        ; 4KB CHR latch windows
.define PRG_BANK_COUNT 1        ; 1 switchable slot (R6 only, no R7)
.define CHR_BANK_COUNT 2        ; 2 latch windows

; ============================================================
; Register addresses
; ============================================================
MMC2_PRG_SELECT = $A000
MMC2_CHR_FD_LO  = $B000
MMC2_CHR_FE_LO  = $C000
MMC2_CHR_FD_HI  = $D000
MMC2_CHR_FE_HI  = $E000
MMC2_MIRROR     = $F000

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank at $8000-$9FFF
.macro select_prg_bank slot, bank
    lda #bank
    sta MMC2_PRG_SELECT
.endmacro

; Standard CHR bank select not used (latch handles switching)
.macro select_chr_bank slot, bank
.endmacro

; Set CHR bank for FD latch state
; slot 0 = lower ($0000-$0FFF), slot 1 = upper ($1000-$1FFF)
.macro set_chr_latch_fd slot, bank
    .if slot = 0
        lda #bank
        sta MMC2_CHR_FD_LO
    .else
        lda #bank
        sta MMC2_CHR_FD_HI
    .endif
.endmacro

; Set CHR bank for FE latch state
.macro set_chr_latch_fe slot, bank
    .if slot = 0
        lda #bank
        sta MMC2_CHR_FE_LO
    .else
        lda #bank
        sta MMC2_CHR_FE_HI
    .endif
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
; (direct encoding, no single-screen support)
.macro set_mirroring mode
    lda #mode
    sta MMC2_MIRROR
.endmacro
