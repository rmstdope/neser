; m026.0_defs.inc — VRC6b (Mapper 26, Submapper 0) — Konami PCB 351949A
;
; VRC6b address wiring: A0 and A1 lines are SWAPPED relative to VRC6a.
; VRC6a: $x001=A0-line, $x002=A1-line
; VRC6b: $x001=A1-line, $x002=A0-line  (odd and even addresses exchanged)
;
; Identical feature set to VRC6a (mapper 24) — only addresses differ.
;
; Features:
;   - PRG banking: 16KB switchable at $8000-$BFFF + 8KB at $C000-$DFFF
;   - CHR banking: 8 × 1KB (mode 0 via $B003=$20, using R0-R7 at $D000-$E003)
;   - Mirroring: H/V/1A/1B via $B003 bits 3-2 (with CIRAM enable bit 5)
;   - IRQ: 8-bit M2 down-counter
;   - No PRG-RAM on commercial cartridges

.define MAPPER_NUM 26
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
.define PRG_BANK_COUNT 2      ; 1 × 16KB switchable + code region
.define CHR_BANK_COUNT 8      ; 8 × 1KB slots

; VRC6b address offsets: A0 and A1 are swapped vs VRC6a
VRC6_OFFSET_0    = $0000
VRC6_OFFSET_A0   = $0002      ; A0 line is on address bit 1 in VRC6b
VRC6_OFFSET_A1   = $0001      ; A1 line is on address bit 0 in VRC6b
VRC6_OFFSET_BOTH = $0003

; ============================================================
; Register addresses
; ============================================================
VRC6_PRG16   = $8000
VRC6_PRG8    = $C000
VRC6_MODE    = $B000 | VRC6_OFFSET_BOTH       ; $B003: same for both variants

; CHR bank registers (mode 0: R0-R7 — note swapped A0/A1 vs VRC6a)
VRC6_CHR_R0  = $D000 | VRC6_OFFSET_0         ; R0: $D000
VRC6_CHR_R1  = $D000 | VRC6_OFFSET_A0        ; R1: $D002 (swapped)
VRC6_CHR_R2  = $D000 | VRC6_OFFSET_A1        ; R2: $D001 (swapped)
VRC6_CHR_R3  = $D000 | VRC6_OFFSET_BOTH      ; R3: $D003
VRC6_CHR_R4  = $E000 | VRC6_OFFSET_0         ; R4: $E000
VRC6_CHR_R5  = $E000 | VRC6_OFFSET_A0        ; R5: $E002 (swapped)
VRC6_CHR_R6  = $E000 | VRC6_OFFSET_A1        ; R6: $E001 (swapped)
VRC6_CHR_R7  = $E000 | VRC6_OFFSET_BOTH      ; R7: $E003

; IRQ registers (A0/A1 swapped → ctrl and ack addresses exchange vs VRC6a)
VRC6_IRQ_LATCH  = $F000 | VRC6_OFFSET_0      ; $F000: IRQ latch (unchanged)
VRC6_IRQ_CTRL   = $F000 | VRC6_OFFSET_A0     ; $F002: IRQ control (VRC6b: A0=bit1)
VRC6_IRQ_ACK    = $F000 | VRC6_OFFSET_A1     ; $F001: IRQ acknowledge (VRC6b: A1=bit0)

; ============================================================
; Register macros (identical logic to m024.0, different address constants)
; ============================================================

.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC6_PRG16
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC6_PRG8
    .endif
.endmacro

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

.macro set_mirroring mode
    .if mode = 0
        lda #$20            ; N=1 (CIRAM), vertical
    .elseif mode = 1
        lda #$24            ; N=1, horizontal
    .elseif mode = 2
        lda #$28            ; N=1, 1-screen A
    .elseif mode = 3
        lda #$2C            ; N=1, 1-screen B
    .endif
    sta VRC6_MODE
.endmacro

.macro set_irq_counter value
    lda #(value & $FF)
    sta VRC6_IRQ_LATCH
.endmacro

.macro enable_irq
    lda #$03                ; E=1, A=1 (auto-reload after ack)
    sta VRC6_IRQ_CTRL
    cli
.endmacro

.macro disable_irq
    lda #0
    sta VRC6_IRQ_CTRL
    sei
.endmacro

.macro init_chr_font
    select_chr_bank 0, 8
    select_chr_bank 1, 9
.endmacro
