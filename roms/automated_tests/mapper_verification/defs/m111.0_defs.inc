; m111.0_defs.inc — GTROM / Cheapocabra (Mapper 111, Submapper 0) ; ; NESdev
spec: https://www.nesdev.org/wiki/GTROM ; ; Register ($5000-$5FFF, $7000-$7FFF):
; 7 bit 0 ; ---- ---- ; GRNC PPPP ; |||| |||| ; |||| ++++- Select 32KB PRG ROM
bank for CPU $8000-$FFFF ; |||+------ Select 8KB CHR RAM bank for PPU
$0000-$1FFF ; ||+------- Select 8KB nametable bank for PPU $2000-$3EFF ;
|+-------- Red LED (0=On; 1=Off) ; +--------- Green LED (0=On; 1=Off) ; ; No
PRG-RAM ($7000-$7FFF is register mirror, $6000-$6FFF unmapped) ; 4-screen
mirroring (2 switchable 8KB nametable banks) ; No IRQ ; No bus conflicts .define
MAPPER_NUM 111 .define SUBMAPPER_NUM 0 PRG_ROM_16K = 8 ; 128KB = 4 x 32KB banks
(uses bottom 4 bits) CHR_ROM_8K = 0 ; CHR-RAM PRG_RAM_SHIFT = 0 CHR_RAM_SHIFT =
8 ; 16KB CHR-RAM (2 x 8KB banks) ; $6000-$7FFF not available for status protocol
CONSOLE_VERIFICATION = 1 ; Capability flags .define HAS_PRG_BANKING 1 .define
HAS_CHR_BANKING 0 .define HAS_CHR_RAM_BANKING 1 .define HAS_CHR_FIXED_LOWER 0 ;
Entire 8KB window switches .define HAS_MIRRORING_CONTROL 0 ; 4-screen hardwired,
no V/H/1screen modes .define HAS_IRQ 0 .define HAS_PRG_RAM 0 .define
HAS_PRG_RAM_PROTECT 0 .define HAS_BUS_CONFLICTS 0 .define MAX_MIRRORING_MODES 0
; Bank sizes .define PRG_BANK_SIZE 32 ; 32KB switchable .define CHR_BANK_SIZE 8
; 8KB CHR-RAM .define PRG_BANK_COUNT 1 ; 1 x 32KB slot .define CHR_BANK_COUNT 1
; CHR-RAM has only 2 banks (bit 4 of register) HAS_ONLY_2_CHR_RAM_BANKS = 1 ;
============================================================ ; Register macros ;
============================================================ ; Shadow register
in BSS to support read-modify-write .segment "BSS" mapper_shadow: .res 1
.segment "CODE" ; Select 32KB PRG bank. ; PRG bits are 0-3 of the register.
Preserve upper bits. .macro select_prg_bank slot, bank lda mapper_shadow and
#%11110000 ; Keep upper bits (CHR, NT, LEDs) ora #(bank & $0F) ; Set PRG bits
0-3 sta mapper_shadow sta $5000 .endmacro ; Select 8KB CHR-RAM bank (bit 4).
.macro select_chr_bank slot, bank lda mapper_shadow and #%11101111 ; Clear bit 4
.if bank & 1 ora #%00010000 ; Set bit 4 if bank is odd .endif sta mapper_shadow
sta $5000 .endmacro ; No mirroring control (4-screen hardwired) .macro
set_mirroring mode .endmacro ;
============================================================ ; Custom trampoline
macros for 32KB PRG banking ;
============================================================ TRAMPOLINE_CUSTOM =
1 ; Called from trampoline_code with bank number in A. ; Write bank number to
$5000, preserving upper bits. .macro trampoline_select_bank sta $5000 ; PRG bits
are low 4, upper bits don't affect bank reading .endmacro ; Called from
trampoline_code to switch back to bank 0. ; Also resets mapper_shadow so
subsequent select_chr_bank / select_prg_bank ; macros start from a clean state
without stale PRG bank bits. .macro trampoline_select_bank_zero lda #$00 sta
mapper_shadow sta $5000 .endmacro
