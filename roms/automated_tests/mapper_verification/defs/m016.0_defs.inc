; m016.0_defs.inc — Bandai FCG (Mapper 16, Submapper 0 — unspecified)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_016
;
; Submapper 0 means the ASIC type is unspecified. Per the spec, emulators
; should handle both FCG-1/2 ($6000-based) and LZ93D50 ($8000-based)
; register ranges. This ROM exercises the $6000-based path, matching SM4.
;
; Features:
;   - PRG banking: 16KB switchable at $8000, last 16KB fixed at $C000
;   - CHR banking: 8 × 1KB switchable banks
;   - Mirroring: H/V/1A/1B via $6009
;   - IRQ: 16-bit M2 down-counter
;   - No PRG-RAM ($6000-$600C occupied by registers)

.define MAPPER_NUM 16
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB (8 test banks + 2KB font + 6 spare)
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
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior (M2 cycle down-counter)
.define IRQ_MODE 0
CONSOLE_BG_PPUCTRL = $18

; Bank sizes
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 2      ; 1 switchable + 1 fixed
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; Register base address (use $6000 path, matching SM4 / FCG-1/2)
FCG_BASE = $6000

; Register addresses
FCG_PRG     = FCG_BASE + $08    ; PRG bank select (bits 3-0)
FCG_MIRROR  = FCG_BASE + $09    ; Mirroring (bits 1-0: 0=V, 1=H, 2=1A, 3=1B)
FCG_IRQ_EN  = FCG_BASE + $0A    ; IRQ enable (bit 0: 1=enable)
FCG_IRQ_LO  = FCG_BASE + $0B    ; IRQ counter low 8 bits
FCG_IRQ_HI  = FCG_BASE + $0C    ; IRQ counter high 8 bits

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000
.macro select_prg_bank slot, bank
    lda #(bank & $0F)
    sta FCG_PRG
.endmacro

; Select 1KB CHR bank (slot 0-7 → CHR registers $6000-$6007)
.macro select_chr_bank slot, bank
    lda #bank
    sta FCG_BASE + slot
.endmacro

; Set mirroring: 0=vertical, 1=horizontal, 2=1-screen A, 3=1-screen B
.macro set_mirroring mode
    lda #mode
    sta FCG_MIRROR
.endmacro

; Mapper 16 IRQ counts raw M2 cycles. Scale the shared abstract test values
; so the IRQ tests do not end up in a tight interrupt storm.
.macro set_irq_counter value
    lda #<(value * 114)
    sta FCG_IRQ_LO
    lda #>(value * 114)
    sta FCG_IRQ_HI
.endmacro

; Enable IRQ (also serves as reload — write 1 enables counting)
.macro enable_irq
    lda #1
    sta FCG_IRQ_EN
    cli
.endmacro

; Disable IRQ
.macro disable_irq
    lda #0
    sta FCG_IRQ_EN
    sei
.endmacro

; Map font CHR banks (8+9) to PPU $1000-$17FF via slots 4 and 5.
.macro init_chr_font
    lda #8
    sta FCG_BASE + 4
    lda #9
    sta FCG_BASE + 5
.endmacro
