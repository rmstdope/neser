; m011.1_defs.inc — Color Dreams (Mapper 11, Submapper 1) — No Bus Conflicts
;
; NESdev spec: https://www.nesdev.org/wiki/Color_Dreams
;
; Submapper 1: No bus conflicts (e.g., Free Fall prototype)
;   - Same banking as submapper 0
;   - Writes can go to any address without bus conflict avoidance
;
; Features:
;   - PRG banking: 32KB switchable (bits 0-1)
;   - CHR banking: 8KB switchable (bits 4-7)
;   - Fixed mirroring (solder pad)
;   - No PRG-RAM
;   - No IRQ
;   - NO bus conflicts (submapper 1 difference)
;
; Register $8000-$FFFF:
;   7  bit  0
;   ---- ----
;   CCCC LLPP
;   |||| ||||
;   |||| ||++- Select 32 KB PRG ROM bank for CPU $8000-$FFFF
;   |||| ++--- Lockout defeat (ignored by emulators)
;   ++++------ Select 8 KB CHR ROM bank for PPU $0000-$1FFF

.define MAPPER_NUM 11
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 8        ; 128KB = 8 × 16KB (= 4 × 32KB banks)
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0     ; Submapper 1: NO bus conflicts
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB switchable
.define CHR_BANK_SIZE 8        ; 8KB switchable
.define PRG_BANK_COUNT 1        ; 1 × 32KB slot
.define CHR_BANK_COUNT 1        ; 1 × 8KB slot

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank (bits 0-1 of CCCCLLPP).
; No bus conflicts - can write to any PRG address.
; Also resets CHR bank to 0 (bits 4-7 = 0).
.macro select_prg_bank slot, bank
    lda #bank
    sta $8000
.endmacro

; Select 8KB CHR bank (bits 4-7 of CCCCLLPP).
; No bus conflicts - can write to any PRG address.
; Also resets PRG bank to 0 (bits 0-1 = 0).
.macro select_chr_bank slot, bank
    lda #(bank << 4)
    sta $8000
.endmacro

.macro set_mirroring mode
.endmacro
