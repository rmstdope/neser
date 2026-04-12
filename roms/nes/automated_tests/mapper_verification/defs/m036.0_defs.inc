; m036.0_defs.inc — TXC 01-22000-400 (Mapper 36, Submapper 0)
;
; NESdev spec: https://www.nesdev.org/wiki/INES_Mapper_036
;
; CHR banking:
;   Mask $E200 — write $4200: [.... CCCC] — Select 8KB CHR bank
;
; PRG banking via TXC ASIC (05-00002-010):
;   Mask $E103 — $4100 read:  [xxRR xxxx] — internal register RR (bits 5-4)
;   Mask $E103 — $4100 write: if Increment set, RR <- RR+1
;                              else if Invert clear, RR <- PP
;                              else RR <- ~PP
;   Mask $E103 — $4101 write: [...V ....] — Invert Mode flag (bit 4)
;   Mask $E103 — $4102 write: [..PP ....] — Copy to internal register PP (bits 5-4)
;   Mask $E103 — $4103 write: [...C ....] — Increment Mode flag (bit 4)
;   Mask $8000 — $8000 write: copy RR to PRG banking pins
;
; No PRG-RAM ("PRG RAM: Impossible" — $6000-$7FFF is unmapped)
; No mirroring control (hardwired from header)
; No IRQ

.define MAPPER_NUM 36
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 8        ; 128KB = 4 × 32KB banks (RR = 2 bits)
CHR_ROM_8K    = 4        ; 32KB = 4 × 8KB banks (test subset of 16 possible)
PRG_RAM_SHIFT = 0
CHR_RAM_SHIFT = 0

; No PRG-RAM at $6000 — use console verification
CONSOLE_VERIFICATION = 1

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 2

; Bank sizes
.define PRG_BANK_SIZE 32
.define CHR_BANK_SIZE 8
.define PRG_BANK_COUNT 1
.define CHR_BANK_COUNT 1

; Custom trampoline for multi-step TXC ASIC bank select
TRAMPOLINE_CUSTOM = 1

; TXC ASIC register addresses
TXC_OUTPUT   = $4100
TXC_INVERT   = $4101
TXC_STAGING  = $4102
TXC_INCREMENT = $4103
TXC_CHR      = $4200
TXC_PRG_COMMIT = $8000

; ============================================================
; Register macros
; ============================================================

; Select 32KB PRG bank via TXC ASIC state machine.
; Steps: set PP, clear modes, copy PP→RR, commit RR to PRG pins.
.macro select_prg_bank slot, bank
    lda #((bank & $03) << 4)
    sta TXC_STAGING          ; PP = bank (bits 5-4)
    lda #$00
    sta TXC_INCREMENT        ; Clear increment mode
    sta TXC_INVERT           ; Clear invert mode
    sta TXC_OUTPUT           ; Copy PP to RR (inc=0, inv=0)
    sta TXC_PRG_COMMIT       ; Commit RR to PRG banking pins
.endmacro

; Select 8KB CHR bank.
.macro select_chr_bank slot, bank
    lda #(bank & $0F)
    sta TXC_CHR
.endmacro

; Mirroring is hardwired — no control register.
.macro set_mirroring mode
.endmacro

.macro enable_prg_ram
.endmacro

.macro disable_prg_ram
.endmacro

.macro write_protect_prg_ram
.endmacro

; ============================================================
; Custom trampoline macros for 32KB PRG banking
; ============================================================

; Called from trampoline_code with bank number in A.
; Must select that bank without destroying the bank number.
.macro trampoline_select_bank
    asl a
    asl a
    asl a
    asl a                    ; Bank << 4 into bits 5-4
    sta TXC_STAGING          ; PP = bank
    lda #$00
    sta TXC_INCREMENT        ; Clear increment mode
    sta TXC_INVERT           ; Clear invert mode
    sta TXC_OUTPUT           ; Copy PP to RR
    sta TXC_PRG_COMMIT       ; Commit RR to PRG banking
.endmacro

; Called from trampoline_code to switch back to bank 0.
.macro trampoline_select_bank_zero
    lda #$00
    sta TXC_STAGING          ; PP = 0
    sta TXC_INCREMENT        ; Clear increment mode
    sta TXC_INVERT           ; Clear invert mode
    sta TXC_OUTPUT           ; Copy PP to RR
    sta TXC_PRG_COMMIT       ; Commit RR to PRG banking
.endmacro

; CHR bank 0 contains the ASCII font.
.macro init_chr_font
    lda #0
    sta TXC_CHR
.endmacro
