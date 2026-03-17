; m032.1_defs.inc — Irem G-101 (Mapper 32, Submapper 1) — Major League
;
; Same as submapper 0 except:
;   - $9000 register entirely disabled
;   - Hardwired 1-screen mirroring (screen B, tied to +5V)
;   - PRG mode always 0
;
; Features:
;   - PRG banking: 8KB switchable via $8000 (Reg0) and $A000 (Reg1)
;   - CHR banking: 8 × 1KB switchable via $B000-$B007
;   - Mirroring: hardwired 1-screen (screen B)
;   - No PRG mode switching (always mode 0)
;   - No IRQ
;   - No bus conflicts
;
; PRG layout: Reg0@$8000, Reg1@$A000, fixed{-2}@$C000, fixed{-1}@$E000

.define MAPPER_NUM 32
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (8 × 8KB banks)
CHR_ROM_8K    = 2        ; 16KB = 2 × 8KB (16 × 1KB banks)
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 8        ; 8KB switchable
.define CHR_BANK_SIZE 1        ; 1KB switchable

; CHR slot layout: linear (slot 0=$0000, slot 1=$0400, slot 2=$0800, ...)
CHR_1K_LINEAR = 1
.define PRG_BANK_COUNT 4       ; 4 windows: 2 switchable + 2 fixed
.define CHR_BANK_COUNT 8       ; 8 × 1KB slots

; Hardwired 1-screen mirroring (screen B)
HARDWIRED_1SCREEN = 1

; ============================================================
; Register macros
; ============================================================

; Select 8KB PRG bank
; slot 0 = $8000-$9FFF (Reg0), slot 1 = $A000-$BFFF (Reg1)
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $1F)
        sta $8000
    .elseif slot = 1
        lda #(bank & $1F)
        sta $A000
    .endif
.endmacro

; Select 1KB CHR bank
; slots 0-7 → $B000+slot
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta $B000
    .elseif slot = 1
        lda #bank
        sta $B001
    .elseif slot = 2
        lda #bank
        sta $B002
    .elseif slot = 3
        lda #bank
        sta $B003
    .elseif slot = 4
        lda #bank
        sta $B004
    .elseif slot = 5
        lda #bank
        sta $B005
    .elseif slot = 6
        lda #bank
        sta $B006
    .elseif slot = 7
        lda #bank
        sta $B007
    .endif
.endmacro

; Mirroring is hardwired — no-op
.macro set_mirroring mode
.endmacro

; Map font CHR banks (8+9) to PPU $0000-$07FF via slots 0 and 1
.macro init_chr_font
    select_chr_bank 0, 8
    select_chr_bank 1, 9
.endmacro

; PRG-RAM is always enabled (no protect register)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
