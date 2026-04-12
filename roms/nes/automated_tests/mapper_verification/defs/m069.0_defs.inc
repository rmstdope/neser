; m069.0_defs.inc — Sunsoft FME-7 (Mapper 69, Submapper 0)
;
; Spec: https://www.nesdev.org/wiki/Sunsoft_FME-7
;
; Features:
;   - PRG banking: 4 × 8KB (3 switchable $8000/$A000/$C000 + 1 fixed $E000)
;   - $6000-$7FFF: bankable PRG-ROM or PRG-RAM (command $08)
;   - CHR banking: 8 × 1KB independently switchable
;   - Mirroring: V/H/1scA/1scB via command $0C
;   - IRQ: 16-bit CPU cycle countdown timer (underflow-triggered)
;     - Command $0D: IRQ control (bit 7 = counter enable, bit 0 = IRQ enable)
;     - Commands $0E/$0F: counter low/high bytes
;   - PRG-RAM: bankable at $6000, enable via command $08 bits 7-6
;   - No bus conflicts
;   - Audio (Sunsoft 5B) not implemented — skip
;
; Register access:
;   $8000-$9FFF: write command number (0-F)
;   $A000-$BFFF: write parameter for current command

.define MAPPER_NUM 69
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 4        ; 32KB = 32 × 1KB banks
PRG_RAM_SHIFT = 7        ; 8KB PRG-RAM
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 1
.define HAS_IRQ 1
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 4       ; V/H/1scA/1scB

; IRQ behavior — CPU cycle countdown timer
.define IRQ_MODE 0
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4      ; 3 switchable ($8000/$A000/$C000) + 1 fixed ($E000)
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots

; ============================================================
; Register Addresses
; ============================================================

FME7_CMD   = $8000    ; Command register (bits 3-0 select command)
FME7_PARAM = $A000    ; Parameter register (invokes command)

; ============================================================
; Helper: write command + parameter
; ============================================================

.macro fme7_write cmd, param
    lda #cmd
    sta FME7_CMD
    lda #param
    sta FME7_PARAM
.endmacro

; ============================================================
; Register Macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000 (PRG bank 1, command $09)
; slot 1 = $A000 (PRG bank 2, command $0A)
; slot 2 = $C000 (PRG bank 3, command $0B)
; ($6000 is used for status byte protocol, not testable via shared test)
.macro select_prg_bank slot, bank
    .if slot = 0
        fme7_write $09, bank
    .elseif slot = 1
        fme7_write $0A, bank
    .elseif slot = 2
        fme7_write $0B, bank
    .endif
.endmacro

; Select 1KB CHR bank (slot 0-7 → commands $00-$07)
.macro select_chr_bank slot, bank
    fme7_write slot, bank
.endmacro

; Set mirroring: 0=vertical, 1=horizontal, 2=1-screen A, 3=1-screen B
.macro set_mirroring mode
    fme7_write $0C, mode
.endmacro

; Enable PRG-RAM at $6000-$7FFF for status byte protocol
; Command $08: E=1 (bit 7), R=1 (bit 6), bank=0
.macro init_prg_ram
    fme7_write $08, $C0
.endmacro

; IRQ counter: 16-bit CPU cycle countdown timer.
; Counter decrements each CPU cycle; IRQ fires on underflow ($0000→$FFFF).
; Scale shared test values by ~114 (cycles per scanline).
.macro set_irq_counter value
    .local counter_val
    counter_val = value * 114
    lda #$0E
    sta FME7_CMD
    lda #<(counter_val)
    sta FME7_PARAM           ; Counter low byte
    lda #$0F
    sta FME7_CMD
    lda #>(counter_val)
    sta FME7_PARAM           ; Counter high byte
.endmacro

.macro enable_irq
    fme7_write $0D, $81      ; Bit 7 = counter enable, bit 0 = IRQ enable
    cli
.endmacro

.macro disable_irq
    fme7_write $0D, $00      ; Disable + acknowledge
    sei
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF
.macro init_chr_font
    fme7_write $00, 8         ; CHR bank 8 → $0000-$03FF
    fme7_write $01, 9         ; CHR bank 9 → $0400-$07FF
.endmacro

; Enable PRG-RAM (already done by init_prg_ram at startup)
.macro enable_prg_ram
    fme7_write $08, $C0       ; E=1, R=1, bank 0
.endmacro
