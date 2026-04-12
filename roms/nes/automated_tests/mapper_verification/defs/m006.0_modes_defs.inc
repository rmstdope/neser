; m006.0_modes_defs.inc — Front Fareast Magic Card (Mapper 6, Submapper 0)
;
; Mode-test variant — used by test_modes.s.
; Same mapper parameters as m006.0_defs.inc, but paired with
; m006.0_modes.cfg which splits PRG-ROM into 16 × 8KB pages for
; per-page bank verification.
;
; NESdev spec: https://www.nesdev.org/wiki/Super_Magic_Card
;              https://www.nesdev.org/wiki/INES_Mapper_006

.define MAPPER_NUM 6
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8         ; 128KB = 8 × 16KB = 16 × 8KB
CHR_ROM_8K    = 4         ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 9         ; 32KB WRAM (64 << 9 = 32KB NES 2.0 non-battery)
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior (M2 cycle counter, not scanline)
.define IRQ_MODE 0

; Bank sizes — mode 1 values for shell.s/bootstrap compatibility
.define PRG_BANK_SIZE 16
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 4
.define CHR_BANK_COUNT 4

; ============================================================
; Register addresses
; ============================================================
SMC_MODE_REG  = $42FE         ; Mode register ($42FC-$42FF)
SMC_CTRL      = $4500         ; Control: WRAM bank, CHR mode, IRQ mode
SMC_IRQ_ACK   = $4501         ; IRQ acknowledge + disable
SMC_IRQ_LO    = $4502         ; IRQ counter latch LSB
SMC_IRQ_HI    = $4503         ; IRQ counter MSB + enable
SMC_LATCH     = $8000         ; Latch register ($8000-$FFFF)

; Play mode base value for $4500: play mode + CIRAM nametables
SMC_PLAY_MODE = $42           ; %01000010

; ============================================================
; Standard macros (for shell.s / bootstrap compatibility)
; ============================================================

; Select 16KB PRG bank at $8000 (mode 1: bits 5-2 = bank)
.macro select_prg_bank slot, bank
    lda #(bank << 2)
    sta SMC_LATCH
.endmacro

; Select 8KB CHR bank (mode 1: bits 1-0 = bank)
.macro select_chr_bank slot, bank
    lda #bank
    sta SMC_LATCH
.endmacro

; Set mirroring via $42FE/$42FF
; mirroring_type = (A0 << 1) | D4
; Keep mode 1 (BBB=001) and write-protect ON (A1=1)
.macro set_mirroring mode
    .if mode = 0
        lda #$20
        sta $42FE
    .elseif mode = 1
        lda #$30
        sta $42FE
    .elseif mode = 2
        lda #$20
        sta $42FF
    .elseif mode = 3
        lda #$30
        sta $42FF
    .endif
.endmacro

; IRQ control
.macro set_irq_counter value
    lda #<($10000 - value)
    sta SMC_IRQ_LO
    lda #>($10000 - value)
    sta SMC_IRQ_HI
.endmacro

.macro enable_irq
    cli
.endmacro

.macro disable_irq
    lda #0
    sta SMC_IRQ_ACK
    sei
.endmacro

; PRG-RAM control
.macro enable_prg_ram
    lda #SMC_PLAY_MODE
    sta SMC_CTRL
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro

; ============================================================
; Mode-switching macros (for test_modes.s)
; ============================================================

; Switch latch-based PRG banking mode.
; Keeps write-protect ON and vertical mirroring (A1=1, A0=1).
; mode: 0-7 (NESdev latch mode number)
;   D7-D5 = mode, D4 = 0 (vertical mirroring via A0=1, D4=0)
;   Address = $42FF (A1=1 write-protect, A0=1 vertical)
.macro switch_latch_mode mode
    lda #(mode << 5)
    sta $42FF
.endmacro

; Enable 2M PRG banking mode.
; Write to $43FE: A1=1 (N=1 → 2M mode), A0=0 (M=0 → enable)
.macro enable_2m_mode
    lda #$00
    sta $43FE
.endmacro

; Disable 2M/4M PRG banking mode.
; Write to $43FF: A0=1 (M=1 → disable)
.macro disable_2m_mode
    lda #$00
    sta $43FF
.endmacro

; Enable PRG memory writes (disables latch).
; Write to $42FD: A1=0 (write-enable), A0=1 (vertical mirroring)
; D7-D5 = 001 (mode 1), D4 = 0
.macro enable_prg_writes
    lda #$20
    sta $42FD
.endmacro

; Restore write-protect (re-enables latch).
; Write to $42FF: A1=1 (write-protect), A0=1 (vertical mirroring)
; D7-D5 = 001 (mode 1), D4 = 0
.macro restore_write_protect
    lda #$20
    sta $42FF
.endmacro

; Select WRAM bank at $6000-$7FFF via $4500 bits 5-4.
; Keeps play mode (D6=1) and CIRAM nametables (D1=1).
; bank: 0-3
.macro select_wram_bank bank
    lda #(SMC_PLAY_MODE | (bank << 4))
    sta SMC_CTRL
.endmacro
