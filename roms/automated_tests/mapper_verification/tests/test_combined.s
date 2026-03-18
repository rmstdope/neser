; test_combined.s — Combined test runner
;
; Runs all applicable tests for the current mapper/submapper in sequence.
; Stops on first failure (via fail_test which halts execution).
; Uses HAS_TEST_xxx defines (passed via -D flags) to determine which tests
; are linked into this combined ROM.

.include "test_macros.inc"
.include "mapper_config.inc"

.export run_tests
.export test_title_string

.import console_show

; Import test-specific entry points (only those included in this build)
.ifdef HAS_TEST_PRG_BANKING
    .import run_prg_banking
.endif
.ifdef HAS_TEST_CHR_BANKING
    .import run_chr_banking
.endif
.ifdef HAS_TEST_NAMETABLE
    .import run_nametable
.endif
.ifdef HAS_TEST_IRQ
    .import run_irq
.endif
.ifdef HAS_TEST_PRG_RAM
    .import run_prg_ram
.endif
.ifdef HAS_TEST_BUS_CONFLICTS
    .import run_bus_conflicts
.endif
.ifdef HAS_TEST_MULTIPLIER
    .import run_multiplier
.endif
.ifdef HAS_TEST_CHR_RAM_BANKING
    .import run_chr_ram_banking
.endif
.ifdef HAS_TEST_CHR_LATCH
    .import run_chr_latch
.endif
.ifdef HAS_TEST_NT_FROM_CHR
    .import run_nt_from_chr
.endif
.ifdef HAS_TEST_PRG_MODE
    .import run_prg_mode
.endif
.ifdef HAS_TEST_BLOCK_SELECT
    .import run_block_select
.endif
.ifdef HAS_TEST_MODE0
    .import run_mode0
.endif
.ifdef HAS_TEST_MODE2
    .import run_mode2
.endif
.ifdef HAS_TEST_MODE3
    .import run_mode3
.endif
.ifdef HAS_TEST_EXRAM
    .import run_exram
.endif

.segment "RODATA"
test_title_string:
    .byte "All m"
    .byte '0' + (MAPPER_NUM / 100)
    .byte '0' + ((MAPPER_NUM / 10) .mod 10)
    .byte '0' + (MAPPER_NUM .mod 10)
    .byte ".", '0' + SUBMAPPER_NUM
    .byte 0

.segment "CODE"

.proc run_tests
.ifdef HAS_TEST_PRG_BANKING
    jsr console_print_inline
    .byte "-- PRG Banking --", 0
    jsr console_flush
    jsr console_newline
    jsr run_prg_banking
    jsr console_show
.endif

.ifdef HAS_TEST_CHR_BANKING
    jsr console_print_inline
    .byte "-- CHR Banking --", 0
    jsr console_flush
    jsr console_newline
    jsr run_chr_banking
    jsr console_show
.endif

.ifdef HAS_TEST_NAMETABLE
    jsr console_print_inline
    .byte "-- Nametable --", 0
    jsr console_flush
    jsr console_newline
    jsr run_nametable
    jsr console_show
.endif

.ifdef HAS_TEST_IRQ
    jsr console_print_inline
    .byte "-- IRQ --", 0
    jsr console_flush
    jsr console_newline
    jsr run_irq
    jsr console_show
.endif

.ifdef HAS_TEST_PRG_RAM
    jsr console_print_inline
    .byte "-- PRG-RAM --", 0
    jsr console_flush
    jsr console_newline
    jsr run_prg_ram
    jsr console_show
.endif

.ifdef HAS_TEST_BUS_CONFLICTS
    jsr console_print_inline
    .byte "-- Bus Conflicts --", 0
    jsr console_flush
    jsr console_newline
    jsr run_bus_conflicts
    jsr console_show
.endif

.ifdef HAS_TEST_MULTIPLIER
    jsr console_print_inline
    .byte "-- Multiplier --", 0
    jsr console_flush
    jsr console_newline
    jsr run_multiplier
    jsr console_show
.endif

.ifdef HAS_TEST_CHR_RAM_BANKING
    jsr console_print_inline
    .byte "-- CHR RAM Banking --", 0
    jsr console_flush
    jsr console_newline
    jsr run_chr_ram_banking
    jsr console_show
.endif

.ifdef HAS_TEST_CHR_LATCH
    jsr console_print_inline
    .byte "-- CHR Latch --", 0
    jsr console_flush
    jsr console_newline
    jsr run_chr_latch
    jsr console_show
.endif

.ifdef HAS_TEST_NT_FROM_CHR
    jsr console_print_inline
    .byte "-- NT from CHR --", 0
    jsr console_flush
    jsr console_newline
    jsr run_nt_from_chr
    jsr console_show
.endif

.ifdef HAS_TEST_PRG_MODE
    jsr console_print_inline
    .byte "-- PRG Mode --", 0
    jsr console_flush
    jsr console_newline
    jsr run_prg_mode
    jsr console_show
.endif

.ifdef HAS_TEST_BLOCK_SELECT
    jsr console_print_inline
    .byte "-- Block Select --", 0
    jsr console_flush
    jsr console_newline
    jsr run_block_select
    jsr console_show
.endif

.ifdef HAS_TEST_MODE0
    jsr console_print_inline
    .byte "-- Mode 0 --", 0
    jsr console_flush
    jsr console_newline
    jsr run_mode0
    jsr console_show
.endif

.ifdef HAS_TEST_MODE2
    jsr console_print_inline
    .byte "-- Mode 2 --", 0
    jsr console_flush
    jsr console_newline
    jsr run_mode2
    jsr console_show
.endif

.ifdef HAS_TEST_MODE3
    jsr console_print_inline
    .byte "-- Mode 3 --", 0
    jsr console_flush
    jsr console_newline
    jsr run_mode3
    jsr console_show
.endif

.ifdef HAS_TEST_EXRAM
    jsr console_print_inline
    .byte "-- ExRAM --", 0
    jsr console_flush
    jsr console_newline
    jsr run_exram
    jsr console_show
.endif

    rts
.endproc

; NES 2.0 Header
.include "nes20_header.inc"
nes20_header

; ASCII font
.if CHR_ROM_8K > 0
.segment "CHARS"
    .incbin "ascii.chr"
.endif
