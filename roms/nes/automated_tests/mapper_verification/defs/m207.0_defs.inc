; m207.0_defs.inc — Taito X1-005 Alternate Mirroring (Mapper 207, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_207
;
; Mapper 207 is identical to Mapper 80 (Taito X1-005) except:
;   - CHR regs $7EF0 and $7EF1 each have bit 7 = nametable select (not a CHR bank bit).
;     $7EF0 bit 7: selects VRAM bank for nametables NT0/NT1 ($2000/$2400).
;     $7EF1 bit 7: selects VRAM bank for nametables NT2/NT3 ($2800/$2C00).
;   - Registers $7EF6–$7EF7 are ignored (no H/V mirroring control).
;
; CHR layout (mixed 2K + 1K):
;   $7EF0 [.CCC CCCC] — 2KB pair at PPU $0000–$07FF (bits 0-6 = first 1KB bank of pair)
;   $7EF1 [.CCC CCCC] — 2KB pair at PPU $0800–$0FFF (bits 0-6 = first 1KB bank of pair)
;   $7EF2 [CCCC CCCC] — 1KB bank at PPU $1000–$13FF
;   $7EF3 [CCCC CCCC] — 1KB bank at PPU $1400–$17FF
;   $7EF4 [CCCC CCCC] — 1KB bank at PPU $1800–$1BFF
;   $7EF5 [CCCC CCCC] — 1KB bank at PPU $1C00–$1FFF
;
; PRG layout (3 switchable 8KB slots + 1 fixed):
;   $7EFA [CCCC CCCC] — PRG bank at $8000–$9FFF
;   $7EFC [CCCC CCCC] — PRG bank at $A000–$BFFF
;   $7EFE [CCCC CCCC] — PRG bank at $C000–$DFFF
;                       $E000–$FFFF is fixed to the last 8KB bank
;
; RAM permission: write $A3 to $7EF8 and $7EF9 to enable 128-byte WRAM at $7F00–$7FFF
;
; Registers live in $7EF0–$7EFF — cannot use $6000 status protocol.

.define MAPPER_NUM 207
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 16 × 1KB banks (banks 0-7 test, 8-9 font)
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

; Registers occupy $7EF0–$7EFF — use console verification instead of $6000 status.
CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0    ; Nametable routing is via CHR reg bit 7, not a separate register
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0

; Bank sizes
.define PRG_BANK_SIZE 8            ; 8KB switchable slots
.define CHR_BANK_SIZE 1            ; 1KB granularity (mixed 2K+1K layout)
.define PRG_BANK_COUNT 3           ; 3 switchable 8KB slots ($8000, $A000, $C000)
.define CHR_BANK_COUNT 6           ; 6 CHR register slots (2×2K pair + 4×1K)

; ============================================================
; Register addresses
; ============================================================

X1005_CHR0     = $7EF0   ; 2KB pair for PPU $0000–$07FF (bit 7 = NT0/NT1 select on mapper 207)
X1005_CHR1     = $7EF1   ; 2KB pair for PPU $0800–$0FFF (bit 7 = NT2/NT3 select on mapper 207)
X1005_CHR2     = $7EF2   ; 1KB for PPU $1000–$13FF
X1005_CHR3     = $7EF3   ; 1KB for PPU $1400–$17FF
X1005_CHR4     = $7EF4   ; 1KB for PPU $1800–$1BFF
X1005_CHR5     = $7EF5   ; 1KB for PPU $1C00–$1FFF
X1005_PRG0     = $7EFA   ; 8KB PRG bank at $8000–$9FFF
X1005_PRG1     = $7EFC   ; 8KB PRG bank at $A000–$BFFF
X1005_PRG2     = $7EFE   ; 8KB PRG bank at $C000–$DFFF

; ============================================================
; PRG banking
; ============================================================

; Select 8KB PRG bank for a given slot.
; Slot 0 → $8000, slot 1 → $A000, slot 2 → $C000.
.macro select_prg_bank slot, bank
    lda #(bank & $3F)
    .if slot = 0
        sta X1005_PRG0
    .elseif slot = 1
        sta X1005_PRG1
    .elseif slot = 2
        sta X1005_PRG2
    .endif
.endmacro

; ============================================================
; CHR banking (mixed 2K + 1K layout)
; ============================================================

; Slots 0 and 1 are 2KB pair registers: bank value is the first 1KB bank of the pair.
; The mapper maps (reg_val, reg_val+1) as the 1KB pair.
; Slots 2–5 are individual 1KB registers: bank value written directly.
;
; For the shared CHR banking test, bank arguments use 1KB granularity:
;   slot 0 or 1: register value = bank (first 1KB bank of the 2KB pair)
;   slots 2–5:   register value = bank
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta X1005_CHR0
    .elseif slot = 1
        lda #bank
        sta X1005_CHR1
    .elseif slot = 2
        lda #bank
        sta X1005_CHR2
    .elseif slot = 3
        lda #bank
        sta X1005_CHR3
    .elseif slot = 4
        lda #bank
        sta X1005_CHR4
    .elseif slot = 5
        lda #bank
        sta X1005_CHR5
    .endif
.endmacro

; ============================================================
; Font initialisation
; ============================================================

; ASCII font lives in 1KB banks 8 and 9 (first 1KB bank = 8).
; Map to PPU $0000–$07FF via slot 0 (X1005_CHR0).
; Bit 7 is cleared (NT select bit = 0), selecting VRAM bank A for NT0/NT1.
.macro init_chr_font
    lda #8                  ; first 1KB bank of the pair (banks 8+9); bit 7 = 0
    sta X1005_CHR0
.endmacro

; ============================================================
; Unused capability stubs
; ============================================================

.macro set_mirroring mode
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro
