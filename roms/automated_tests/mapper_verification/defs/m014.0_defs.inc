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

; IRQ behavior
.define IRQ_MODE 0             ; 0 = Sharp/MMC3

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; MMC3 register addresses
MMC3_BANK_SELECT = $8000
MMC3_BANK_DATA   = $8001
MMC3_MIRRORING   = $A000
MMC3_PRG_RAM     = $A001
MMC3_IRQ_LATCH   = $C000
MMC3_IRQ_RELOAD  = $C001
MMC3_IRQ_DISABLE = $E000
MMC3_IRQ_ENABLE  = $E001

; Supervisor register
SUPERVISOR_REG   = $A131

; ============================================================
; Register macros
; ============================================================

; Switch to MMC3 mode via supervisor register
; Also sets CHR outer bits to 0 (lower 256KB)
.macro set_mmc3_mode
    lda #$02                ; M=1 (MMC3 mode), all CHR A18=0
    sta SUPERVISOR_REG
.endmacro

; Select 8KB PRG bank (MMC3 mode)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #6
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .elseif slot = 1
        lda #7
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .endif
.endmacro

; Select 1KB CHR bank (MMC3 mode)
.macro select_chr_bank slot, bank
    lda #slot
    sta MMC3_BANK_SELECT
    lda #bank
    sta MMC3_BANK_DATA
.endmacro

; Set mirroring: 0=vertical, 1=horizontal (MMC3 mode)
.macro set_mirroring mode
    .if mode = 0
        lda #0
        sta MMC3_MIRRORING
    .elseif mode = 1
        lda #1
        sta MMC3_MIRRORING
    .else
        lda #0
        sta MMC3_MIRRORING
    .endif
.endmacro

; IRQ control (MMC3 mode)
.macro set_irq_counter value
    lda #value
    sta MMC3_IRQ_LATCH
    lda #0
    sta MMC3_IRQ_RELOAD
.endmacro

.macro enable_irq
    cli
    lda #0
    sta MMC3_IRQ_ENABLE
.endmacro

.macro disable_irq
    lda #0
    sta MMC3_IRQ_DISABLE
    sei
.endmacro

; PRG-RAM control (MMC3)
.macro enable_prg_ram
    lda #%10000000
    sta MMC3_PRG_RAM
.endmacro

.macro disable_prg_ram
    lda #%00000000
    sta MMC3_PRG_RAM
.endmacro

.macro write_protect_prg_ram
    lda #%11000000
    sta MMC3_PRG_RAM
.endmacro

; Map font CHR banks to $0000-$07FF via R0
.macro init_chr_font
    ; R0: 2KB at $0000-$07FF → banks 8+9
    lda #0
    sta MMC3_BANK_SELECT
    lda #8
    sta MMC3_BANK_DATA
.endmacro
