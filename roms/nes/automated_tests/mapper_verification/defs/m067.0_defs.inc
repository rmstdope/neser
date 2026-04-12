; m067.0_defs.inc — Sunsoft-3 (Mapper 67, Submapper 0)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/INES_Mapper_067.xhtml
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF, last 16KB fixed at $C000
;   - CHR banking: 4 × 2KB switchable banks ($8800/$9800/$A800/$B800)
;   - Mirroring: V/H/1scA/1scB via $E800
;   - IRQ: 16-bit CPU cycle down-counter (write-twice register at $C800)
;   - Counter wraps $0000→$FFFF to fire IRQ and pause
;   - $8000: IRQ acknowledge
;   - $D800: IRQ enable (bit 4) + reset write toggle
;   - No PRG-RAM, no bus conflicts

.define MAPPER_NUM 67
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB banks
CHR_ROM_8K    = 2        ; 16KB = 8 × 2KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; V/H/1scA/1scB

; IRQ behavior — CPU cycle down-counter
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 2       ; 2KB switchable
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 4      ; 4 × 2KB slots

; ============================================================
; Register Addresses
; ============================================================

S3_IRQ_ACK  = $8000   ; Any write acknowledges pending IRQ (addr & $F800)
S3_CHR0     = $8800   ; CHR bank 0 → $0000-$07FF
S3_CHR1     = $9800   ; CHR bank 1 → $0800-$0FFF
S3_CHR2     = $A800   ; CHR bank 2 → $1000-$17FF
S3_CHR3     = $B800   ; CHR bank 3 → $1800-$1FFF
S3_IRQ_CTR  = $C800   ; IRQ counter (write-twice: high byte first, then low)
S3_IRQ_EN   = $D800   ; IRQ enable (bit 4 = count) + resets write toggle
S3_MIRROR   = $E800   ; Mirroring (bits 0-1: 0=V, 1=H, 2=1scA, 3=1scB)
S3_PRG      = $F800   ; PRG bank select (bits 0-3)

; ============================================================
; Register Macros
; ============================================================

; Select 16KB PRG bank at $8000-$BFFF
.macro select_prg_bank slot, bank
    lda #bank
    sta S3_PRG
.endmacro

; Select 2KB CHR bank
; slot 0 → $8800, slot 1 → $9800, slot 2 → $A800, slot 3 → $B800
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta S3_CHR0
    .elseif slot = 1
        lda #bank
        sta S3_CHR1
    .elseif slot = 2
        lda #bank
        sta S3_CHR2
    .elseif slot = 3
        lda #bank
        sta S3_CHR3
    .endif
.endmacro

; Set mirroring: 0=vertical, 1=horizontal, 2=1-screen A, 3=1-screen B
.macro set_mirroring mode
    lda #mode
    sta S3_MIRROR
.endmacro

; IRQ counter: 16-bit CPU cycle down-counter.
; Scale shared test values by ~114 (cycles per scanline).
; $C800 is write-twice (high first, then low); $D800 resets toggle.
.macro set_irq_counter value
    lda #$00
    sta S3_IRQ_EN           ; Reset write toggle + pause
    lda #>(value * 114)
    sta S3_IRQ_CTR          ; High byte
    lda #<(value * 114)
    sta S3_IRQ_CTR          ; Low byte
.endmacro

.macro enable_irq
    lda #$10                ; Bit 4 = enable counting
    sta S3_IRQ_EN
    cli
.endmacro

.macro disable_irq
    lda #$00                ; Bit 4 = 0, pause counting
    sta S3_IRQ_EN
    lda #0
    sta S3_IRQ_ACK          ; Acknowledge any pending IRQ
    sei
.endmacro

; Map font CHR bank (4) to PPU $0000-$07FF
.macro init_chr_font
    lda #4                  ; CHR bank 4 contains font
    sta S3_CHR0             ; Map to $0000-$07FF
.endmacro
