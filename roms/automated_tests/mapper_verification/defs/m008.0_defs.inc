; m008.0_defs.inc — Super Magic Card GNROM mode (Mapper 8 = Mapper 6 Submapper 4)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_008
;              https://www.nesdev.org/wiki/Super_Magic_Card
;
; Mode 4 (GNROM):
;   D~[..PP ..CC]
;   PP = CPU $8000-$FFFF: switchable 32 KiB PRG bank #0-#3
;   CC = PPU $0000-$1FFF: switchable 8 KiB CHR bank #0-#3, write-protected
;
; Features:
;   - PRG banking: 32KB switchable via latch bits 5-4
;   - CHR banking: 8KB switchable via latch bits 1-0
;   - Mirroring: via $42FC-$42FF (4-way: 1A/1B/V/H)
;   - WRAM: 32KB banked at $6000 (4 × 8KB via $4500 bits 5-4)
;   - IRQ: 16-bit M2 up-counter ($4502/$4503)
;   - Bus-conflict-free latch at $8000-$FFFF

.define MAPPER_NUM 8
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8         ; 128KB = 8 × 16KB (= 4 × 32KB banks)
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

; IRQ behavior (M2 cycle counter, not scanline)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 32      ; 32KB switchable
.define CHR_BANK_SIZE 8       ; 8KB switchable
.define PRG_BANK_COUNT 1      ; 1 × 32KB slot
.define CHR_BANK_COUNT 1      ; 1 × 8KB slot

; 32KB trampoline: bank number needs to be shifted to bits 5-4
TRAMPOLINE_BANK_SHIFT = 4

; ============================================================
; Register addresses (same as mapper 6)
; ============================================================
SMC_MODE_REG  = $42FE         ; Mode register ($42FC-$42FF)
SMC_CTRL      = $4500         ; Control: WRAM bank, CHR mode, IRQ mode
SMC_IRQ_ACK   = $4501         ; IRQ acknowledge + disable
SMC_IRQ_LO    = $4502         ; IRQ counter latch LSB
SMC_IRQ_HI    = $4503         ; IRQ counter MSB + enable
SMC_LATCH     = $8000         ; Latch register ($8000-$FFFF)

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank at $8000 (mode 4: bits 5-4 = PP)
; Preserves CHR bank 0 (bits 1-0 = 0)
.macro select_prg_bank slot, bank
    lda #(bank << 4)
    sta SMC_LATCH
.endmacro

; Select 8KB CHR bank (mode 4: bits 1-0 = CC)
; Sets PRG bank to 0 (bits 5-4 = 0)
.macro select_chr_bank slot, bank
    lda #bank
    sta SMC_LATCH
.endmacro

; Set mirroring via $42FE/$42FF
; mirroring_type = (D4 << 1) | A0
; Keep latch enabled: use addr with A1=1 ($42FE or $42FF)
; Mode bits D7-D5 = 100 (keep mode 4 for GNROM)
.macro set_mirroring mode
    .if mode = 0
        ; mirroring_type 0 (1A): A0=0, D4=0 → addr=$42FE, data=$80
        lda #$80
        sta $42FE
    .elseif mode = 1
        ; mirroring_type 1 (1B): A0=0, D4=1 → addr=$42FE, data=$90
        lda #$90
        sta $42FE
    .elseif mode = 2
        ; mirroring_type 2 (V): A0=1, D4=0 → addr=$42FF, data=$80
        lda #$80
        sta $42FF
    .elseif mode = 3
        ; mirroring_type 3 (H): A0=1, D4=1 → addr=$42FF, data=$90
        lda #$90
        sta $42FF
    .endif
.endmacro

; IRQ control — 16-bit up-counter (M2 mode), same as mapper 6
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
    sta SMC_IRQ_ACK         ; Acknowledge + disable counting
    sei
.endmacro

; PRG-RAM control (WRAM at $6000, always enabled)
SMC_PLAY_MODE = $42       ; Play mode + CIRAM nametables

.macro enable_prg_ram
    lda #SMC_PLAY_MODE
    sta SMC_CTRL
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
