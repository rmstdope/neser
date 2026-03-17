; m024.0_defs.inc — VRC6a (Mapper 24, Submapper 0) — Konami PCB 351951
;
; VRC6a address wiring: A0=NES_A0 (bit 0), A1=NES_A1 (bit 1)
; Registers at $x000, $x001, $x002, $x003 for offsets 0, A0, A1, both.
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF + 8KB at $C000-$DFFF
;   - CHR banking: 8 × 1KB (mode 0 via $B003=$20, using R0-R7 at $D000-$E003)
;   - Mirroring: H/V/1A/1B via $B003 bits 3-2 (with CIRAM enable bit 5)
;   - IRQ: 8-bit M2 down-counter (not nibble-split, unlike VRC4)
;   - No PRG-RAM on commercial cartridges
;
; PRG layout: $8000-$BFFF = 16KB switchable, $C000-$DFFF = 8KB switchable,
;             $E000-$FFFF = 8KB fixed to last bank

.define MAPPER_NUM 24
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB
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

; IRQ behavior (M2 cycle down-counter, 8-bit latch)
.define IRQ_MODE 0

; Bank sizes — test 16KB banking at $8000
.define PRG_BANK_SIZE 16      ; 16KB switchable
.define CHR_BANK_SIZE 1       ; 1KB switchable
.define PRG_BANK_COUNT 2      ; 1 × 16KB switchable + code region (fixed $E000)
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots
CHR_1K_LINEAR = 1      ; Linear 1KB slots from $0000 to $1FFF

; VRC6a address offsets (A0=bit0, A1=bit1)
VRC6_OFFSET_0    = $0000
VRC6_OFFSET_A0   = $0001
VRC6_OFFSET_A1   = $0002
VRC6_OFFSET_BOTH = $0003

; ============================================================
; Register addresses
; ============================================================
VRC6_PRG16   = $8000                          ; 16KB PRG select at $8000-$BFFF
VRC6_PRG8    = $C000                          ; 8KB PRG select at $C000-$DFFF
VRC6_MODE    = $B000 | VRC6_OFFSET_BOTH       ; $B003: PPU banking mode + mirroring

; CHR bank registers (mode 0: R0-R7 at $D000-$E003)
VRC6_CHR_R0  = $D000 | VRC6_OFFSET_0         ; R0: PPU $0000-$03FF
VRC6_CHR_R1  = $D000 | VRC6_OFFSET_A0        ; R1: PPU $0400-$07FF
VRC6_CHR_R2  = $D000 | VRC6_OFFSET_A1        ; R2: PPU $0800-$0BFF
VRC6_CHR_R3  = $D000 | VRC6_OFFSET_BOTH      ; R3: PPU $0C00-$0FFF
VRC6_CHR_R4  = $E000 | VRC6_OFFSET_0         ; R4: PPU $1000-$13FF
VRC6_CHR_R5  = $E000 | VRC6_OFFSET_A0        ; R5: PPU $1400-$17FF
VRC6_CHR_R6  = $E000 | VRC6_OFFSET_A1        ; R6: PPU $1800-$1BFF
VRC6_CHR_R7  = $E000 | VRC6_OFFSET_BOTH      ; R7: PPU $1C00-$1FFF

; IRQ registers
VRC6_IRQ_LATCH  = $F000 | VRC6_OFFSET_0      ; IRQ latch (8-bit, not nibble-split)
VRC6_IRQ_CTRL   = $F000 | VRC6_OFFSET_A0     ; IRQ control (M/E/A bits)
VRC6_IRQ_ACK    = $F000 | VRC6_OFFSET_A1     ; IRQ acknowledge

; ============================================================
; Register macros
; ============================================================

; Select 16KB PRG bank at $8000-$BFFF (testing only this slot)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC6_PRG16
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC6_PRG8
    .endif
.endmacro

; Select 1KB CHR bank (mode 0 — 8 × 1KB registers R0-R7)
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta VRC6_CHR_R0
    .elseif slot = 1
        lda #bank
        sta VRC6_CHR_R1
    .elseif slot = 2
        lda #bank
        sta VRC6_CHR_R2
    .elseif slot = 3
        lda #bank
        sta VRC6_CHR_R3
    .elseif slot = 4
        lda #bank
        sta VRC6_CHR_R4
    .elseif slot = 5
        lda #bank
        sta VRC6_CHR_R5
    .elseif slot = 6
        lda #bank
        sta VRC6_CHR_R6
    .elseif slot = 7
        lda #bank
        sta VRC6_CHR_R7
    .endif
.endmacro

; Set mirroring via $B003 while keeping PRG-RAM enabled for the test-status byte.
; bit 7 = W (PRG-RAM enable), bit 5 = N (CIRAM), mirror bits 3-2 = V/H/1A/1B,
; and PPU banking mode bits 1-0 stay in mode 0.
.macro set_mirroring mode
    .if mode = 0
        lda #$A0            ; W=1, N=1, mirror=vertical
    .elseif mode = 1
        lda #$A4            ; W=1, N=1, mirror=horizontal
    .elseif mode = 2
        lda #$A8            ; W=1, N=1, mirror=1-screen A
    .elseif mode = 3
        lda #$AC            ; W=1, N=1, mirror=1-screen B
    .endif
    sta VRC6_MODE
.endmacro

; Set IRQ counter (fire after 'value' M2 cycles; 8-bit latch)
.macro set_irq_counter value
    lda #(value & $FF)
    sta VRC6_IRQ_LATCH
.endmacro

; Enable IRQ with auto-reload: E=1, A=1, M=0 (prescaler mode)
.macro enable_irq
    lda #$03                ; E=1, A=1 (auto-reload after ack)
    sta VRC6_IRQ_CTRL
    cli
.endmacro

; Disable IRQ
.macro disable_irq
    lda #0
    sta VRC6_IRQ_CTRL
    sei
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
.macro init_chr_font
    select_chr_bank 0, 8
    select_chr_bank 1, 9
.endmacro
