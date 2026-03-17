; m018.0_defs.inc — Jaleco SS 88006 (Mapper 18, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_018
;
; Features:
;   - PRG banking: 8KB × 3 switchable ($8000, $A000, $C000) + fixed at $E000
;   - CHR banking: 8 × 1KB switchable
;   - Mirroring: H/V/1A/1B via $F002
;   - IRQ: 16-bit (or 12-bit) M2 down-counter, nibble-split latch
;   - PRG-RAM: 8KB at $6000-$7FFF, write-protect via $9002
;
; All bank and latch registers are nibble-split: write low nibble (bits 3-0)
; to first address, high nibble (bits 7-4) to second address.

.define MAPPER_NUM 18
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB (8 test banks + 2KB font + 6 spare)
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
.define MAX_MIRRORING_MODES 4       ; Supports V/H/1A/1B

; IRQ behavior (M2 cycle down-counter)
.define IRQ_MODE 0

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 4      ; 3 switchable + 1 fixed
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots

; ============================================================
; Register addresses
; ============================================================
; PRG bank registers (nibble-split pairs)
M18_PRG0_LO = $8000     ; PRG bank 0 at $8000 — low nibble
M18_PRG0_HI = $8001     ;                     — high nibble
M18_PRG1_LO = $8002     ; PRG bank 1 at $A000 — low nibble
M18_PRG1_HI = $8003     ;                     — high nibble
M18_PRG2_LO = $9000     ; PRG bank 2 at $C000 — low nibble
M18_PRG2_HI = $9001     ;                     — high nibble
M18_RAM_WP  = $9002     ; PRG-RAM write protect (bit 0: 1=enable writes)

; CHR bank registers (nibble-split pairs; slot N at $A000 + N/2*$1000 + (N&1)*$2)
M18_CHR0_LO = $A000
M18_CHR0_HI = $A001
M18_CHR1_LO = $A002
M18_CHR1_HI = $A003
M18_CHR2_LO = $B000
M18_CHR2_HI = $B001
M18_CHR3_LO = $B002
M18_CHR3_HI = $B003
M18_CHR4_LO = $C000
M18_CHR4_HI = $C001
M18_CHR5_LO = $C002
M18_CHR5_HI = $C003
M18_CHR6_LO = $D000
M18_CHR6_HI = $D001
M18_CHR7_LO = $D002
M18_CHR7_HI = $D003

; IRQ registers
M18_IRQ_LATCH0 = $E000  ; Reload value bits  3-0
M18_IRQ_LATCH1 = $E001  ; Reload value bits  7-4
M18_IRQ_LATCH2 = $E002  ; Reload value bits 11-8
M18_IRQ_LATCH3 = $E003  ; Reload value bits 15-12
M18_IRQ_RELOAD = $F000  ; Write any value → reload counter from latch + start
M18_IRQ_SIZE   = $F001  ; Counter size (bit 3: 0=12-bit, 1=16-bit)
M18_IRQ_MIRROR = $F002  ; Mirroring (bits 1-0: 0=H, 1=V, 2=1A, 3=1B)

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank (nibble-split write)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta M18_PRG0_LO
        lda #((bank >> 4) & $0F)
        sta M18_PRG0_HI
    .elseif slot = 1
        lda #(bank & $0F)
        sta M18_PRG1_LO
        lda #((bank >> 4) & $0F)
        sta M18_PRG1_HI
    .elseif slot = 2
        lda #(bank & $0F)
        sta M18_PRG2_LO
        lda #((bank >> 4) & $0F)
        sta M18_PRG2_HI
    .endif
.endmacro

; Select 1KB CHR bank (nibble-split write)
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta M18_CHR0_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR0_HI
    .elseif slot = 1
        lda #(bank & $0F)
        sta M18_CHR1_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR1_HI
    .elseif slot = 2
        lda #(bank & $0F)
        sta M18_CHR2_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR2_HI
    .elseif slot = 3
        lda #(bank & $0F)
        sta M18_CHR3_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR3_HI
    .elseif slot = 4
        lda #(bank & $0F)
        sta M18_CHR4_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR4_HI
    .elseif slot = 5
        lda #(bank & $0F)
        sta M18_CHR5_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR5_HI
    .elseif slot = 6
        lda #(bank & $0F)
        sta M18_CHR6_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR6_HI
    .elseif slot = 7
        lda #(bank & $0F)
        sta M18_CHR7_LO
        lda #((bank >> 4) & $0F)
        sta M18_CHR7_HI
    .endif
.endmacro

; Set mirroring: 0=horizontal, 1=vertical, 2=1-screen A, 3=1-screen B
.macro set_mirroring mode
    lda #mode
    sta M18_IRQ_MIRROR
.endmacro

; Set IRQ counter (fire after 'value' M2 cycles); use 16-bit mode
.macro set_irq_counter value
    lda #(value & $0F)
    sta M18_IRQ_LATCH0
    lda #((value >> 4) & $0F)
    sta M18_IRQ_LATCH1
    lda #((value >> 8) & $0F)
    sta M18_IRQ_LATCH2
    lda #((value >> 12) & $0F)
    sta M18_IRQ_LATCH3
    lda #$08                ; 16-bit counter mode
    sta M18_IRQ_SIZE
.endmacro

; Enable IRQ: reload counter from latch and start counting
.macro enable_irq
    lda #1
    sta M18_IRQ_RELOAD      ; Reload and start counter
    cli
.endmacro

; Disable IRQ: write 0 to size register stops counting
.macro disable_irq
    lda #0
    sta M18_IRQ_SIZE        ; Clear mode → counter halts
    sei
.endmacro

; PRG-RAM control via $9002 bit 0
.macro enable_prg_ram
    lda #$01
    sta M18_RAM_WP          ; Enable writes
.endmacro

.macro disable_prg_ram
    ; No disable on Jaleco — writes always visible
.endmacro

.macro write_protect_prg_ram
    lda #$00
    sta M18_RAM_WP          ; Write-protect
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
.macro init_chr_font
    select_chr_bank 0, 8
    select_chr_bank 1, 9
.endmacro
