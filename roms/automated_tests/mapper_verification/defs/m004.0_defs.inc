; m004.0_defs.inc — MMC3/TxROM (Mapper 4, Submapper 0) — Sharp IRQ
;
; Features:
;   - PRG banking: 8KB switchable (2 variable + 2 fixed)
;   - CHR banking: 1KB/2KB switchable (2×2KB + 4×1KB or inverted)
;   - Mirroring: H/V programmable
;   - PRG-RAM at $6000-$7FFF with enable/write-protect
;   - Scanline IRQ (Sharp: fires when counter reaches 0)

.define MAPPER_NUM 4
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB
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
.define IRQ_MODE 0             ; 0 = Sharp (standard)

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4       ; 4 × 8KB slots
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; MMC3 register addresses
MMC3_BANK_SELECT = $8000  ; Bank select: bits 0-2=register, bit 6=PRG mode, bit 7=CHR mode
MMC3_BANK_DATA   = $8001  ; Bank data: value for selected register
MMC3_MIRRORING   = $A000  ; Mirroring: bit 0 (0=vert mirror/horiz arr/A10, 1=horiz mirror/vert arr/A11)
MMC3_PRG_RAM     = $A001  ; PRG-RAM protect: bit 6=write enable, bit 7=chip enable
MMC3_IRQ_LATCH   = $C000  ; IRQ latch value
MMC3_IRQ_RELOAD  = $C001  ; IRQ counter reload (write any value)
MMC3_IRQ_DISABLE = $E000  ; IRQ disable + acknowledge (write any value)
MMC3_IRQ_ENABLE  = $E001  ; IRQ enable (write any value)

; MMC3 bank select register numbers:
;   R0: CHR $0000-$07FF (2KB) or $1000-$17FF
;   R1: CHR $0800-$0FFF (2KB) or $1800-$1FFF
;   R2: CHR $1000-$13FF (1KB) or $0000-$03FF
;   R3: CHR $1400-$17FF (1KB) or $0400-$07FF
;   R4: CHR $1800-$1BFF (1KB) or $0800-$0BFF
;   R5: CHR $1C00-$1FFF (1KB) or $0C00-$0FFF
;   R6: PRG $8000-$9FFF or $C000-$DFFF (mode bit)
;   R7: PRG $A000-$BFFF (always)

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000 (R6), slot 1 = $A000 (R7)
; Slots 2,3 are fixed (second-to-last and last banks)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #6              ; R6: PRG $8000
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .elseif slot = 1
        lda #7              ; R7: PRG $A000
        sta MMC3_BANK_SELECT
        lda #bank
        sta MMC3_BANK_DATA
    .endif
.endmacro

; Select 1KB CHR bank
; slots 0-5 map to R0-R5
; R0,R1 select 2KB regions; R2-R5 select 1KB regions
.macro select_chr_bank slot, bank
    lda #slot
    sta MMC3_BANK_SELECT
    lda #bank
    sta MMC3_BANK_DATA
.endmacro

; Set mirroring: 0=vertical, 1=horizontal
; Note: MMC3 only supports H/V, not single-screen
.macro set_mirroring mode
    .if mode = 0
        ; Vertical
        lda #0
        sta MMC3_MIRRORING
    .elseif mode = 1
        ; Horizontal
        lda #1
        sta MMC3_MIRRORING
    .else
        ; MMC3 doesn't support single-screen — map to vertical
        lda #0
        sta MMC3_MIRRORING
    .endif
.endmacro

; IRQ control
.macro set_irq_counter value
    lda #value
    sta MMC3_IRQ_LATCH
    lda #0
    sta MMC3_IRQ_RELOAD     ; Reload counter on next scanline
.endmacro

.macro enable_irq
    cli                     ; Enable CPU interrupts
    lda #0
    sta MMC3_IRQ_ENABLE
.endmacro

.macro disable_irq
    lda #0
    sta MMC3_IRQ_DISABLE    ; Disable + acknowledge
    sei                     ; Disable CPU interrupts
.endmacro

; PRG-RAM control
; $A001 bit 7 = chip enable, bit 6 = write protect
.macro enable_prg_ram
    lda #%10000000          ; Bit 7=chip enable, bit 6=0 (write enabled)
    sta MMC3_PRG_RAM
.endmacro

.macro disable_prg_ram
    lda #%00000000          ; Disable chip
    sta MMC3_PRG_RAM
.endmacro

.macro write_protect_prg_ram
    lda #%11000000          ; Chip enable + write protect
    sta MMC3_PRG_RAM
.endmacro

; Map font CHR banks (8+9) to $0000-$07FF via R0
; Called during init and after CHR banking tests
.macro init_chr_font
    lda #0                  ; R0: 2KB at $0000-$07FF
    sta MMC3_BANK_SELECT
    lda #8                  ; Font is in CHR banks 8+9
    sta MMC3_BANK_DATA
.endmacro
