; vrc4_core_macros.inc — Shared VRC4/VRC2 register macros
;
; This file contains register definitions and macros shared by all VRC4/VRC2 mappers:
; - Mapper 21 (VRC4a, VRC4c)
; - Mapper 23 (VRC4f, VRC4e, VRC2b)
; - Mapper 25 (VRC4b, VRC4d, VRC2c)
;
; Each mapper-specific defs file must define VRC4_A0_OFFSET and VRC4_A1_OFFSET
; before including this file.  VRC2 variants additionally set VRC2_MODE = 1.

; ============================================================
; Derived register addresses (computed from A0/A1 offsets)
; ============================================================

VRC4_PRG0_REG       = $8000
VRC4_PRG1_REG       = $A000
VRC4_MIRROR_REG     = $9000
VRC4_WRAM_REG       = $9000 | VRC4_A1_OFFSET

VRC4_CHR0_LO        = $B000
VRC4_CHR0_HI        = $B000 | VRC4_A0_OFFSET
VRC4_CHR1_LO        = $B000 | VRC4_A1_OFFSET
VRC4_CHR1_HI        = $B000 | VRC4_A0_OFFSET | VRC4_A1_OFFSET

VRC4_CHR2_LO        = $C000
VRC4_CHR2_HI        = $C000 | VRC4_A0_OFFSET
VRC4_CHR3_LO        = $C000 | VRC4_A1_OFFSET
VRC4_CHR3_HI        = $C000 | VRC4_A0_OFFSET | VRC4_A1_OFFSET

VRC4_CHR4_LO        = $D000
VRC4_CHR4_HI        = $D000 | VRC4_A0_OFFSET
VRC4_CHR5_LO        = $D000 | VRC4_A1_OFFSET
VRC4_CHR5_HI        = $D000 | VRC4_A0_OFFSET | VRC4_A1_OFFSET

VRC4_CHR6_LO        = $E000
VRC4_CHR6_HI        = $E000 | VRC4_A0_OFFSET
VRC4_CHR7_LO        = $E000 | VRC4_A1_OFFSET
VRC4_CHR7_HI        = $E000 | VRC4_A0_OFFSET | VRC4_A1_OFFSET

VRC4_IRQ_LATCH_LO   = $F000
VRC4_IRQ_LATCH_HI   = $F000 | VRC4_A0_OFFSET
VRC4_IRQ_CTRL       = $F000 | VRC4_A1_OFFSET
VRC4_IRQ_ACK        = $F000 | VRC4_A0_OFFSET | VRC4_A1_OFFSET

; ============================================================
; PRG banking
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000-$9FFF (switchable), slot 1 = $A000-$BFFF (switchable)
; slots 2,3 are fixed (2nd-to-last and last banks)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #bank
        sta VRC4_PRG0_REG
    .elseif slot = 1
        lda #bank
        sta VRC4_PRG1_REG
    .endif
.endmacro

; ============================================================
; CHR banking (1KB per slot, nibble-split registers)
; ============================================================

; Select 1KB CHR bank
; slot 0-7 → PPU $0000-$1FFF in 1KB pages
; bank value is 8-bit (lo nibble bits 3-0, hi nibble bits 7-4)
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC4_CHR0_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR0_HI
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC4_CHR1_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR1_HI
    .elseif slot = 2
        lda #(bank & $0F)
        sta VRC4_CHR2_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR2_HI
    .elseif slot = 3
        lda #(bank & $0F)
        sta VRC4_CHR3_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR3_HI
    .elseif slot = 4
        lda #(bank & $0F)
        sta VRC4_CHR4_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR4_HI
    .elseif slot = 5
        lda #(bank & $0F)
        sta VRC4_CHR5_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR5_HI
    .elseif slot = 6
        lda #(bank & $0F)
        sta VRC4_CHR6_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR6_HI
    .elseif slot = 7
        lda #(bank & $0F)
        sta VRC4_CHR7_LO
        lda #((bank >> 4) & $1F)
        sta VRC4_CHR7_HI
    .endif
.endmacro

; ============================================================
; Nametable mirroring
; ============================================================

; Set nametable mirroring
; mode 0=Vertical, 1=Horizontal, 2=1-screen A, 3=1-screen B
; VRC2 variants only support modes 0 and 1
.macro set_mirroring mode
    lda #mode
    sta VRC4_MIRROR_REG
.endmacro

; ============================================================
; IRQ control (VRC4 only; guarded so VRC2 variants compile clean)
; ============================================================

.ifndef VRC2_MODE

; Load IRQ latch and arm counter
; value = number of CPU cycles before IRQ fires (approximately)
; 8-bit latch written as two nibbles; use M-cycle mode (bit 2 = 0) for simplicity
.macro set_irq_counter value
    lda #(value & $0F)
    sta VRC4_IRQ_LATCH_LO
    lda #((value >> 4) & $0F)
    sta VRC4_IRQ_LATCH_HI
.endmacro

; Enable IRQ: set E=1 and A=1 so counter auto-reloads after each acknowledge
; This allows multiple consecutive IRQs (needed for reload test in test_irq.s)
.macro enable_irq
    lda #%00000011          ; E=1, A=1 (auto-reload after ack), M-cycle mode=0
    sta VRC4_IRQ_CTRL
    cli
.endmacro

; Disable IRQ: clear all control bits, mask CPU interrupt
.macro disable_irq
    lda #0
    sta VRC4_IRQ_CTRL       ; Clear E and A bits — counter stops
    sei
.endmacro

; Acknowledge pending IRQ (clears IRQ line; reloads counter if A bit was set)
.macro ack_irq
    lda #0
    sta VRC4_IRQ_ACK
.endmacro

.endif  ; .ifndef VRC2_MODE

; ============================================================
; Font initialisation
; ============================================================

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
; Called during init and after CHR banking tests
.macro init_chr_font
    select_chr_bank 0, 8    ; slot 0 → bank 8 (font low 1KB)
    select_chr_bank 1, 9    ; slot 1 → bank 9 (font high 1KB)
.endmacro
