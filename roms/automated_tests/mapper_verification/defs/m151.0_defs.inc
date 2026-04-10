; m151.0_defs.inc — VRC1 on VS System (Mapper 151, Submapper 0)
;
; Spec: https://nesdev-wiki.nes.science/wikipages/VRC1.xhtml
;       https://nesdev-wiki.nes.science/wikipages/INES_Mapper_151.xhtml
;
; Same Konami VRC1 ASIC as mapper 75, but on VS System boards:
;   - PRG banking: 3 × 8KB switchable + 8KB fixed last
;     $8000: slot 0, $A000: slot 1, $C000: slot 2
;   - CHR banking: 2 × 4KB switchable via full 8-bit registers
;     $E000: CHR bank at PPU $0000 (full 8-bit, no high bit from $9000)
;     $F000: CHR bank at PPU $1000 (full 8-bit, no high bit from $9000)
;   - Mirroring: Four-screen (hardwired on VS System board)
;   - $9000 writes are ignored (no mirroring control, no CHR high bits)
;   - No IRQ, no PRG-RAM, no bus conflicts

.define MAPPER_NUM 151
.define SUBMAPPER_NUM 0

PRG_ROM_16K   = 4        ; 64KB = 4 × 16KB = 8 × 8KB banks
CHR_ROM_8K    = 2        ; 16KB = 4 × 4KB banks
PRG_RAM_SHIFT = 7        ; 8KB for status byte protocol
CHR_RAM_SHIFT = 0

; NES 2.0 header flags
MIRRORING           = 0       ; Ignored: four-screen overrides
HAS_FOUR_SCREEN_VRAM = 1      ; Hardware four-screen VRAM

; Capability flags
.define HAS_PRG_BANKING 1
.define HAS_CHR_BANKING 1
.define HAS_FOUR_SCREEN 1
.define HAS_MIRRORING_CONTROL 0
.define HAS_IRQ 0
.define HAS_PRG_RAM 0
.define HAS_PRG_RAM_PROTECT 0
.define HAS_BUS_CONFLICTS 0
.define MAX_MIRRORING_MODES 0       ; No switchable mirroring

; Bank sizes
.define PRG_BANK_SIZE 8       ; 8KB switchable
.define CHR_BANK_SIZE 4       ; 4KB switchable
.define PRG_BANK_COUNT 4      ; 3 switchable + 1 fixed
.define CHR_BANK_COUNT 2      ; 2 × 4KB slots

; Skip the second PRG slot test (slot 1 at $A000) — VRC1 has 3 independent
; slots but the generic 8KB test only probes slot 0 and slot 1.
; We keep slot 1 enabled since VRC1 supports it.

; ============================================================
; Register Addresses
; ============================================================

VRC1_PRG0    = $8000   ; PRG Select 0 (bits 0-3) → 8KB at $8000
VRC1_PRG1    = $A000   ; PRG Select 1 (bits 0-3) → 8KB at $A000
VRC1_PRG2    = $C000   ; PRG Select 2 (bits 0-3) → 8KB at $C000
VRC1_CHR0    = $E000   ; CHR Select 0 (full 8-bit) → 4KB at PPU $0000
VRC1_CHR1    = $F000   ; CHR Select 1 (full 8-bit) → 4KB at PPU $1000

; ============================================================
; Register Macros
; ============================================================

; Select 8KB PRG bank (identical to VRC1 mapper 75)
; slot 0 = $8000, slot 1 = $A000, slot 2 = $C000
.macro select_prg_bank slot, bank
    .if slot = 0
        lda #(bank & $0F)
        sta VRC1_PRG0
    .elseif slot = 1
        lda #(bank & $0F)
        sta VRC1_PRG1
    .elseif slot = 2
        lda #(bank & $0F)
        sta VRC1_PRG2
    .endif
.endmacro

; Select 4KB CHR bank — full 8-bit register (unlike VRC1 which splits high bit via $9000)
; slot 0 → $E000 (PPU $0000), slot 1 → $F000 (PPU $1000)
.macro select_chr_bank slot, bank
    .if slot = 0
        lda #bank
        sta VRC1_CHR0
    .elseif slot = 1
        lda #bank
        sta VRC1_CHR1
    .endif
.endmacro

; Map font CHR bank (0) to PPU $0000-$0FFF
.macro init_chr_font
    lda #0
    sta VRC1_CHR0               ; Map bank 0 to $0000
.endmacro
