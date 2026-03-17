; shell.s — Test shell: entry point, $6000 protocol, failure handler
;
; This is the main entry point for all test ROMs.
; It initializes the NES, sets up the console, then calls `run_tests`
; which each test source file must provide.

__SHELL_S__ = 1
.include "nes.inc"
.include "test_macros.inc"
.include "mapper_config.inc"

; Import from other modules
.import init_nes, wait_vbl
.import console_init, console_flush, console_newline, console_show
.import console_print_inline, console_print_hex, console_print

; Import from the test-specific source file
.import run_tests

; Export reset/nmi/irq vectors
.export reset, nmi_handler, irq_handler

.segment "ZEROPAGE"
irq_fired:    .res 1       ; Flag: set to 1 when IRQ fires
irq_count:    .res 1       ; Number of IRQs received
expected_val: .res 1       ; Temp for fail handler
got_val:      .res 1       ; Temp for fail handler

.export irq_fired, irq_count

.segment "CODE"

; ============================================================
; Reset vector — main entry point
; ============================================================
.proc reset
    ; Initialize CPU state (must be done before any JSR)
    sei
    cld
    ldx #$FF
    txs

    ; Enable PRG-RAM early so we can write the status byte at $6000.
    ; For MMC3: $A001 bit 7 = chip enable. Only for mappers where $A001 is the RAM protect register.
    .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14
    lda #$80
    sta $A001
    .endif

    ; Mapper-specific early init
    .if MAPPER_NUM = 4
    init_chr_font
    .elseif MAPPER_NUM = 12
    ; SL-5020B: clear outer CHR register, map font
    lda #0
    sta $4132
    init_chr_font
    .elseif MAPPER_NUM = 14
    ; SL-1632: set MMC3 mode via supervisor, map font
    set_mmc3_mode
    init_chr_font
    .elseif MAPPER_NUM = 5
    ; MMC5: set PRG mode 3 (4×8KB) and enable PRG-RAM
    lda #3
    sta $5100
    ; Set PRG bank for $E000 to last bank (should be default, but be safe)
    lda #7
    sta $5117
    ; Enable PRG-RAM writes ($5102=$02, $5103=$01)
    lda #$02
    sta $5102
    lda #$01
    sta $5103
    ; Set CHR mode 0 (8KB) and select bank 0
    lda #0
    sta $5101
    sta $5127
    ; Set nametable to vertical mirroring
    lda #$44
    sta $5105
    .elseif MAPPER_NUM = 22
    ; VRC2a: map font banks 8+9 to PPU $0000-$07FF
    init_chr_font
    .elseif MAPPER_NUM = 16
    ; Bandai FCG/LZ93D50: map font banks 8+9 to PPU $0000-$07FF
    init_chr_font
    .elseif MAPPER_NUM = 18
    ; Jaleco SS 88006: enable PRG-RAM writes, map font
    enable_prg_ram
    init_chr_font
    .elseif MAPPER_NUM = 24 .or MAPPER_NUM = 26
    ; VRC6a/b: set standard PPU banking mode (mode 0, CIRAM, vertical), map font
    lda #$20                ; N=1 (CIRAM), mode 0, vertical mirroring
    sta $B003
    init_chr_font
    .elseif MAPPER_NUM = 28
    ; Action 53: configure PRG mode 2, horizontal mirroring
    init_action53
    .elseif MAPPER_NUM = 31
    ; NSF mapper: set slot 6 to second-to-last bank (bank 14 = $E000 code)
    lda #(PRG_ROM_16K * 4 - 2)
    sta $5FFE
    ; Slot 7 already has last bank at power-on
    .endif

    jsr init_nes

    ; Set status = running
    lda #STATUS_RUNNING
    sta TEST_STATUS

    ; Initialize console
    jsr console_init

    ; Print ROM title (provided by test file via TITLE_STRING)
    jsr print_title

    ; Clear IRQ state
    lda #0
    sta irq_fired
    sta irq_count

    ; Run the actual tests
    jsr run_tests

    ; If we return, all tests passed
    all_passed
.endproc

; Print test title from test-specific code
.proc print_title
    ; The test file exports test_title_string
    .import test_title_string
    lda #<test_title_string
    sta str_ptr
    lda #>test_title_string
    sta str_ptr+1
    jsr console_print
    jsr console_flush
    jsr console_newline
    rts
.endproc

.importzp str_ptr

; ============================================================
; Failure handler
; Called with: A = expected value, X = got value
; ============================================================
.export do_fail_test
.proc do_fail_test
    sta expected_val
    stx got_val

    ; Print "..FAIL"
    jsr console_print_inline
    .byte "..FAIL", 0
    jsr console_flush
    jsr console_newline

    ; Print "  Exp: $XX"
    jsr console_print_inline
    .byte "  Exp: $", 0
    lda expected_val
    jsr console_print_hex
    jsr console_flush
    jsr console_newline

    ; Print "  Got: $XX"
    jsr console_print_inline
    .byte "  Got: $", 0
    lda got_val
    jsr console_print_hex
    jsr console_flush
    jsr console_newline

    ; Print "FAILED"
    jsr console_print_inline
    .byte "FAILED", 0
    jsr console_flush

    ; Re-enable rendering to ensure console output is visible
    jsr console_show

    ; Set status byte to failing test number
    lda TEST_CODE
    sta TEST_STATUS

    ; Halt
:   jmp :-
.endproc

; ============================================================
; NMI handler — minimal, just returns
; ============================================================
.proc nmi_handler
    rti
.endproc

; ============================================================
; IRQ handler — sets flag and returns
; Mapper-specific acknowledge logic
; ============================================================
.proc irq_handler
    pha
    inc irq_fired
    inc irq_count
    .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14
    ; MMC3/MMC3-clone: acknowledge + re-enable
    lda #0
    sta $E000               ; IRQ acknowledge (disable)
    sta $E001               ; IRQ re-enable
    .elseif MAPPER_NUM = 5
    ; MMC5: read $5204 to clear pending flag
    lda $5204
    .elseif MAPPER_NUM = 6 .or MAPPER_NUM = 8
    ; Mapper 6/8: write $4502 to ack IRQ (also sets latch LSB as side-effect)
    ; ($4501 would ack + disable counting; $4502 acks + loads latch LSB)
    ; The latch side-effect is harmless — running counter is unaffected.
    lda #0
    sta $4502
    .elseif MAPPER_NUM = 16
    ; Bandai FCG/LZ93D50: acknowledge by writing 0 then re-enabling with small count
    ; Re-enable with same small counter for continuous firing in reload test
    lda #0
    sta FCG_IRQ_EN          ; Acknowledge + disable
    lda #10
    sta FCG_IRQ_LO          ; Reload with small count
    lda #0
    sta FCG_IRQ_HI
    lda #1
    sta FCG_IRQ_EN          ; Re-enable
    .elseif MAPPER_NUM = 18
    ; Jaleco SS 88006: write to reload register to ack and restart counter
    lda #1
    sta M18_IRQ_RELOAD
    .elseif MAPPER_NUM = 24 .or MAPPER_NUM = 26
    ; VRC6a/b: write to IRQ ACK register (auto-reloads if A bit was set)
    lda #0
    sta VRC6_IRQ_ACK
    .endif
    pla
    rti
.endproc

; ============================================================
; Vectors
; ============================================================
.segment "VECTORS"
    .word nmi_handler
    .word reset
    .word irq_handler

; ============================================================
; Mapper 15 power-on bootstrap
; On power-on, mode 0 (NROM-256) maps bank 1 to $C000.
; This code runs from bank 1, copies a mode-switch routine
; to RAM, switches to mode 1, then jumps to the real reset.
; ============================================================
.if MAPPER_NUM = 15
.segment "BOOT"
.proc boot_reset
    sei
    ldx #0
@copy:
    lda m15_switch, x
    sta $0300, x
    inx
    cpx #(m15_switch_end - m15_switch)
    bne @copy
    jmp $0300
m15_switch:
    lda #0
    sta $8001               ; Switch to mode 1 (UNROM), bank 0, vertical
    jmp reset               ; Bank 7 now at $C000
m15_switch_end:
.endproc

.segment "BOOT_VECS"
    .word $0000             ; NMI (won't fire during bootstrap)
    .word boot_reset        ; Reset → bootstrap
    .word $0000             ; IRQ (won't fire during bootstrap)
.endif
