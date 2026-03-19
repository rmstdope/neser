; m005.0_defs.inc — MMC5 (Mapper 5, Submapper 0)
;
; Features:
;   - PRG banking: 8KB×4 (mode 3), 16KB×2 (mode 1), 32KB (mode 0)
;   - CHR banking: 8KB (mode 0), 4KB×2 (mode 1), 1KB×8 (mode 3)
;   - Nametable: CIRAM A/B, ExRAM, Fill mode via $5105
;   - Scanline IRQ via $5203/$5204
;   - PRG-RAM at $6000-$7FFF with bank select + write protection
;   - Hardware 8×8 multiplier at $5205/$5206
;   - ExRAM at $5C00-$5FFF

.define MAPPER_NUM 5
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB
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
.define HAS_MULTIPLIER 1
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior (not Sharp/NEC — scanline compare)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable (mode 3)
.define CHR_BANK_SIZE 8       ; 8KB switchable (mode 0)
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 4       ; 4 × 8KB banks

; ============================================================
; MMC5 register addresses
; ============================================================
MMC5_PRG_MODE    = $5100  ; PRG banking mode (bits 1-0)
MMC5_CHR_MODE    = $5101  ; CHR banking mode (bits 1-0)
MMC5_RAM_PROT1   = $5102  ; PRG-RAM protect 1 (bits 1-0 must = %10)
MMC5_RAM_PROT2   = $5103  ; PRG-RAM protect 2 (bits 1-0 must = %01)
MMC5_EXRAM_MODE  = $5104  ; ExRAM mode (bits 1-0)
MMC5_NT_MAP      = $5105  ; Nametable mapping (4×2-bit quadrants)
MMC5_FILL_TILE   = $5106  ; Fill mode tile value
MMC5_FILL_ATTR   = $5107  ; Fill mode attribute (bits 1-0)
MMC5_PRG_RAM_BK  = $5113  ; PRG-RAM bank at $6000 (bits 2-0)
MMC5_PRG_5114    = $5114  ; PRG bank at $8000
MMC5_PRG_5115    = $5115  ; PRG bank at $A000
MMC5_PRG_5116    = $5116  ; PRG bank at $C000
MMC5_PRG_5117    = $5117  ; PRG bank at $E000 (ROM only)
MMC5_CHR_A0      = $5120  ; CHR A register 0
MMC5_CHR_A3      = $5123  ; CHR A register 3 (4KB mode: $0000-$0FFF)
MMC5_CHR_A4      = $5124  ; CHR A register 4 (1KB mode: $1000-$13FF)
MMC5_CHR_A7      = $5127  ; CHR A register 7 (8KB mode uses this)
MMC5_CHR_B0      = $5128  ; CHR B register 0
MMC5_CHR_B3      = $512B  ; CHR B register 3 (4KB mode: $0000-$0FFF for BG)
MMC5_CHR_UPPER   = $5130  ; Upper CHR bank bits (bits 1-0)
MMC5_IRQ_CMP     = $5203  ; Scanline IRQ compare value (0=disabled)
MMC5_IRQ_STATUS  = $5204  ; IRQ enable (W bit 7) / status (R bits 7-6)
MMC5_MULT_A      = $5205  ; Multiplicand (W) / result low (R)
MMC5_MULT_B      = $5206  ; Multiplier (W) / result high (R)

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank (mode 3: $5114-$5116)
; slot 0=$8000, slot 1=$A000, slot 2=$C000
; $5117 ($E000) is always ROM — not switchable by test
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #($80 | bank)       ; Bit 7 = ROM
        sta MMC5_PRG_5114
    .elseif slot = 1
        lda #($80 | bank)
        sta MMC5_PRG_5115
    .elseif slot = 2
        lda #($80 | bank)
        sta MMC5_PRG_5116
    .endif
.endmacro

; Select 8KB CHR bank (mode 0: $5127)
.macro select_chr_bank slot, bank
    lda #bank
    sta MMC5_CHR_A7
.endmacro

; Set mirroring via $5105 nametable mapping
; Uses MMC1-compatible convention (mirroring direction, not arrangement):
;   0 = single-screen A (all CIRAM A)        → $5105=$00
;   1 = single-screen B (all CIRAM B)        → $5105=$55
;   2 = vertical mirror (horiz arr, A10)     → $5105=$44
;   3 = horizontal mirror (vert arr, A11)    → $5105=$50
; NB: NESdev wiki calls $44 "Horizontal" and $50 "Vertical" (arrangement naming)
.macro set_mirroring mode
    .if mode = 0
        lda #$00                ; All CIRAM A
        sta MMC5_NT_MAP
    .elseif mode = 1
        lda #$55                ; All CIRAM B
        sta MMC5_NT_MAP
    .elseif mode = 2
        lda #$44                ; Vert mirror: Q0=A,Q1=B,Q2=A,Q3=B (spec: "Horizontal")
        sta MMC5_NT_MAP
    .elseif mode = 3
        lda #$50                ; Horiz mirror: Q0=A,Q1=A,Q2=B,Q3=B (spec: "Vertical")
        sta MMC5_NT_MAP
    .endif
.endmacro

; IRQ control — scanline compare
.macro set_irq_counter value
    lda #value
    sta MMC5_IRQ_CMP
.endmacro

.macro enable_irq
    lda #$80                    ; Bit 7 = enable
    sta MMC5_IRQ_STATUS
    cli
.endmacro

.macro disable_irq
    lda #$00
    sta MMC5_IRQ_STATUS
    sei
.endmacro

; PRG-RAM control
.macro enable_prg_ram
    lda #$02
    sta MMC5_RAM_PROT1
    lda #$01
    sta MMC5_RAM_PROT2
.endmacro

.macro disable_prg_ram
    lda #$00
    sta MMC5_RAM_PROT1
.endmacro

.macro write_protect_prg_ram
    lda #$00
    sta MMC5_RAM_PROT1
.endmacro
