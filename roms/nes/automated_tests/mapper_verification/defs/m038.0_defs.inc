; m038.0_defs.inc — Bit Corp. PCI556 (Mapper 38, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_038
;
; Features:
;   - PRG banking: 32KB switchable (bits 0-1)
;   - CHR banking: 8KB switchable (bits 2-3)
;   - Fixed mirroring (hardwired vertical)
;   - No PRG-RAM (native)
;   - No IRQ
;   - No bus conflicts
;
; Register $7000-$7FFF:
;   7  bit  0
;   ---- ----
;   xxxx CCPP
;        ||||
;        ||++- Select 32 KB PRG ROM bank for CPU $8000-$FFFF
;        ++--- Select 8 KB CHR ROM bank for PPU $0000-$1FFF
;
; NOTE: Register at $7000 does NOT conflict with $6000 test protocol.

.define MAPPER_NUM 38
.define SUBMAPPER_NUM 0

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
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No mirroring control

; Bank sizes
.define PRG_BANK_SIZE 32       ; 32KB switchable
.define CHR_BANK_SIZE 8        ; 8KB switchable
.define PRG_BANK_COUNT 1        ; 1 × 32KB slot
.define CHR_BANK_COUNT 1        ; 1 × 8KB slot

; Trampoline bank address override for 32K banking test
TRAMPOLINE_BANK_ADDR = $7000

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank (bits 0-1 of xxxxCCPP).
; Register at $7000 (no bus conflicts).
; Also clears CHR bank to 0 (bits 2-3 = 0).
.macro select_prg_bank slot, bank
    lda #(bank & $03)
    sta $7000
.endmacro

; Select 8KB CHR bank (bits 2-3 of xxxxCCPP).
; Register at $7000 (no bus conflicts).
; Also clears PRG bank to 0 (bits 0-1 = 0).
.macro select_chr_bank slot, bank
    lda #((bank << 2) & $0C)
    sta $7000
.endmacro

.macro set_mirroring mode
.endmacro

; PRG-RAM is always enabled (no protect register)
.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro
