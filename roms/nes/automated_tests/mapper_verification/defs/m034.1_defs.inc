; m034.1_defs.inc — NINA-001 (Mapper 34, Submapper 1)
;
; Explicit NES 2.0 submapper for NINA-001.
; Identical behavior to submapper 0.
;
; Features:
;   - PRG banking: 32KB switchable via $7FFD [.... ...P]
;   - CHR banking: 2 × 4KB switchable via $7FFE/$7FFF [.... CCCC]
;   - PRG-RAM: 8KB at $6000-$7FFC (NINA-001 SRAM)
;   - Mirroring: hardwired (no control)
;   - No bus conflicts
;   - No IRQ
;
; Note: NINA-001 registers at $7FFD-$7FFF overlap the PRG-RAM address space
; ($6000-$7FFF), but test_prg_ram.s only tests $6004-$6100 which is safe.

.define MAPPER_NUM 34
.define SUBMAPPER_NUM 1

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB (= 2 × 32KB banks)
CHR_ROM_8K    = 8        ; 64KB = 8 × 8KB (= 16 × 4KB banks)
PRG_RAM_SHIFT = 7        ; 8KB
CHR_RAM_SHIFT = 0

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 1
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB switchable
.define CHR_BANK_SIZE 4        ; 4KB switchable
.define PRG_BANK_COUNT 1        ; 1 × 32KB slot
.define CHR_BANK_COUNT 2        ; 2 × 4KB slots

; NINA-001 register addresses
NINA001_PRG  = $7FFD     ; PRG bank select [.... ...P]
NINA001_CHR0 = $7FFE     ; CHR bank 0 select [.... CCCC] → $0000-$0FFF
NINA001_CHR1 = $7FFF     ; CHR bank 1 select [.... CCCC] → $1000-$1FFF

; Trampoline bank address override for 32K banking test
TRAMPOLINE_BANK_ADDR = NINA001_PRG

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank via NINA-001 register at $7FFD
.macro select_prg_bank slot, bank
    lda #bank
    sta NINA001_PRG
.endmacro

; Select 4KB CHR bank
; slot 0 = $0000-$0FFF via $7FFE, slot 1 = $1000-$1FFF via $7FFF
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta NINA001_CHR0
    .else
        lda #bank
        sta NINA001_CHR1
    .endif
.endmacro

.macro set_mirroring mode
.endmacro

; Map font: select CHR bank 0 for slot 0 ($0000-$0FFF)
.macro init_chr_font
    lda #0
    sta NINA001_CHR0
.endmacro

; PRG-RAM is always enabled (NINA-001 SRAM has no protect)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
