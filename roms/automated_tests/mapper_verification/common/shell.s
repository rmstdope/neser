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
    .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14 .or MAPPER_NUM = 74 .or MAPPER_NUM = 119 .or MAPPER_NUM = 37 .or MAPPER_NUM = 45 .or MAPPER_NUM = 47
    lda #$80
    sta $A001
    .endif
    ; Namco 163: $F800 write-protect register, $40 = enable all writes
    .if MAPPER_NUM = 19
    lda #$40
    sta $F800
    .endif

    ; Mapper-specific early init
    .if MAPPER_NUM = 4
    init_chr_font
    .elseif MAPPER_NUM = 119
    ; TQROM: same init as MMC3 (font in CHR-ROM banks 8+9)
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
    .elseif MAPPER_NUM = 21 .or MAPPER_NUM = 23 .or MAPPER_NUM = 25
    ; VRC4a/b/c/d/e/f / VRC2b/c: map font CHR banks 8+9 to PPU $0000-$07FF.
    ; VRC4 submappers (SM != 3) also need PRG-RAM explicitly enabled via WRAM reg.
    .if SUBMAPPER_NUM <> 3
    lda #$01                ; bit 0 = WRAM enable (PRG-RAM on), bit 1 = 0 (no swap)
    sta VRC4_WRAM_REG
    .endif
    init_chr_font
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
    .elseif MAPPER_NUM = 19
    ; Namco 163/129: disable IRQ (bit 7=1), enable PRG-RAM writes (wram_protect=$40),
    ; then map font CHR banks 8+9 to PPU $0000-$07FF.
    ; init_chr_font also sets CIRAM nametables (vertical mirroring default).
    lda #$80
    sta $5800                   ; disable IRQ (bit 7 = 1) before init
    lda #$40
    sta $F800                   ; set wram_protect=$40 → all windows writable
    init_chr_font
    .elseif MAPPER_NUM = 24 .or MAPPER_NUM = 26
    ; VRC6a/b: set standard PPU banking mode (mode 0, CIRAM, vertical),
    ; and keep PRG-RAM enabled so the $6000 test-status byte stays readable.
    lda #$A0                ; W=1, N=1 (CIRAM), mode 0, vertical mirroring
    sta $B003
    init_chr_font
    .elseif MAPPER_NUM = 28
    ; Action 53: configure PRG mode 2, horizontal mirroring
    init_action53
    .elseif MAPPER_NUM = 32
    ; Irem G-101: map font CHR banks 8+9 to PPU $0000-$07FF
    ; CHR bank 8 → 1K slot 0 at $B000, CHR bank 9 → 1K slot 1 at $B001
    lda #8
    sta $B000
    lda #9
    sta $B001
    .elseif MAPPER_NUM = 33
    ; Taito TC0190: map font via 2K register ($8002)
    init_chr_font
    .elseif MAPPER_NUM = 35
    ; J.Y. Company ASIC: set mode register, mirroring, and map font
    ; $D000 = $1A: 8K PRG mode, 1K CHR mode, last bank fixed, WRAM at $6000
    lda #$1A
    sta $D000
    ; Vertical mirroring
    lda #$00
    sta $D001
    ; Map font CHR banks 8+9 to PPU $0000-$07FF
    init_chr_font
    .elseif MAPPER_NUM = 44
    ; Super Big 7-in-1: MMC3 multicart — enable PRG-RAM, select block 0, map font
    lda #$80                ; E=1, W=0, block=0
    sta $A001
    init_chr_font
    .elseif MAPPER_NUM = 37 .or MAPPER_NUM = 47
    ; MMC3 multicarts (mapper 37/47): map font (block 0 active at power-on)
    init_chr_font
    .elseif MAPPER_NUM = 45
    ; GA23C multicart: map font (outer regs default at power-on/reset)
    init_chr_font
    .elseif MAPPER_NUM = 48
    ; Taito TC0690: map font via 2K register ($8002)
    init_chr_font
    .elseif MAPPER_NUM = 64
    ; RAMBO-1: map font CHR banks (same register layout as MMC3)
    init_chr_font
    .elseif MAPPER_NUM = 65
    ; Irem H3001: map font CHR banks via $B000/$B001
    init_chr_font
    .elseif MAPPER_NUM = 67
    ; Sunsoft 3: map font CHR bank via $8800
    init_chr_font
    .elseif MAPPER_NUM = 68
    ; Sunsoft 4: enable PRG-RAM, map font CHR bank via $8000
    init_prg_ram
    init_chr_font
    .elseif MAPPER_NUM = 69
    ; Sunsoft FME-7: enable PRG-RAM at $6000, map font CHR banks via commands $00/$01
    init_prg_ram
    init_chr_font
    .elseif MAPPER_NUM = 72
    ; Jaleco JF-17: no init needed (font in CHR bank 0, mapped at power-on)
    .elseif MAPPER_NUM = 73
    ; VRC3: no special init needed (CHR-RAM, font not in CHR-ROM)
    .elseif MAPPER_NUM = 74
    ; MMC3 variant (CHR-RAM at banks 8-9): enable PRG-RAM, map font via R0=10
    lda #$80
    sta $A001
    init_chr_font
    .elseif MAPPER_NUM = 75
    ; VRC1: map font CHR bank via $E000
    init_chr_font
    .elseif MAPPER_NUM = 36
    ; TXC ASIC: map font CHR bank via $4200
    init_chr_font
    .endif

    jsr init_nes

    ; Set status = running for mappers using the $6000 status-byte protocol
    .ifndef CONSOLE_VERIFICATION
    lda #STATUS_RUNNING
    sta TEST_STATUS
    .endif

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

    ; Set status byte to failing test number for status-byte verified mappers
    .ifndef CONSOLE_VERIFICATION
    lda TEST_CODE
    sta TEST_STATUS
    .endif

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
    .if MAPPER_NUM = 4 .or MAPPER_NUM = 12 .or MAPPER_NUM = 14 .or MAPPER_NUM = 64 .or MAPPER_NUM = 119 .or MAPPER_NUM = 37 .or MAPPER_NUM = 45 .or MAPPER_NUM = 47 .or MAPPER_NUM = 74
    ; MMC3/MMC3-clone/RAMBO-1: acknowledge + re-enable
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
    lda #<(10 * 114)
    sta FCG_IRQ_LO          ; Reload with small count
    lda #>(10 * 114)
    sta FCG_IRQ_HI
    lda #1
    sta FCG_IRQ_EN          ; Re-enable
    .elseif MAPPER_NUM = 18
    ; Jaleco SS 88006: write to reload register to ack and restart counter
    lda #1
    sta M18_IRQ_RELOAD
    .elseif MAPPER_NUM = 19
    ; Namco 163: reload a small test interval and re-enable counting.
    lda #<($8000 - (10 * 114))
    sta $5000
    lda #(>($8000 - (10 * 114)) & $7F)
    sta $5800
    .elseif MAPPER_NUM = 21 .or MAPPER_NUM = 23 .or MAPPER_NUM = 25
    ; VRC4a/b/c/d/e/f: acknowledge IRQ by writing to the VRC4 IRQ ACK register
    lda #0
    sta VRC4_IRQ_ACK
    .elseif MAPPER_NUM = 24 .or MAPPER_NUM = 26
    ; VRC6a/b: write to IRQ ACK register (auto-reloads if A bit was set)
    lda #0
    sta VRC6_IRQ_ACK
    .elseif MAPPER_NUM = 44
    ; Mapper 44 (MMC3 multicart): acknowledge + re-enable
    lda #0
    sta $E000               ; IRQ acknowledge (disable)
    sta $E001               ; IRQ re-enable
    .elseif MAPPER_NUM = 35
    ; J.Y. Company ASIC: acknowledge IRQ by disabling then re-enabling
    lda #0
    sta $C002               ; IRQ disable (acknowledges pending IRQ)
    sta $C003               ; IRQ re-enable
    .elseif MAPPER_NUM = 48
    ; Taito TC0690: acknowledge IRQ by writing to $C003
    lda #0
    sta $C003
    sta $C002               ; Re-enable
    .elseif MAPPER_NUM = 65
    ; Irem H3001: acknowledge + reload counter, then re-enable
    lda #0
    sta H3001_IRQ_LOAD      ; Acknowledge + reload counter from reload value
    lda #$80
    sta H3001_IRQ_EN        ; Re-enable (bit 7 = enable)
    .elseif MAPPER_NUM = 67
    ; Sunsoft 3: acknowledge + reload counter + re-enable
    lda #0
    sta S3_IRQ_ACK          ; Acknowledge IRQ
    sta S3_IRQ_EN           ; Reset write toggle + pause
    lda #>(10 * 114)
    sta S3_IRQ_CTR          ; Reload high byte
    lda #<(10 * 114)
    sta S3_IRQ_CTR          ; Reload low byte
    lda #$10
    sta S3_IRQ_EN           ; Re-enable counting (bit 4)

    .elseif MAPPER_NUM = 42
    ; Mapper 42: acknowledge + reset counter, then re-enable
    lda #$00
    sta $E002               ; Disable + acknowledge + reset counter
    lda #$02
    sta $E002               ; Re-enable IRQ (bit 1)
    .elseif MAPPER_NUM = 69
    ; FME-7: acknowledge IRQ by writing to command $0D, then re-enable
    lda #$0D
    sta $8000               ; Select IRQ control command
    lda #$81
    sta $A000               ; Counter enable + IRQ enable (also acknowledges)
    .elseif MAPPER_NUM = 73
    ; VRC3: acknowledge + reload + re-enable
    lda #$00
    sta $D000               ; Acknowledge IRQ
    lda #$02
    sta $C000               ; E=1: reload from latch + re-enable
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

; ============================================================
; Mapper 28 power-on bootstrap
; On power-on, the last bank (7) is at $C000 (NESdev spec).
; This code runs from bank 7, copies init_action53 to RAM,
; executes it to switch $C000 to bank 1, then jumps to reset.
; ============================================================
.if MAPPER_NUM = 28
.segment "BOOT"
.proc boot_reset
    sei
    ldx #0
@copy:
    lda m28_switch, x
    sta $0300, x
    inx
    cpx #(m28_switch_end - m28_switch)
    bne @copy
    jmp $0300
m28_switch:
    init_action53                   ; Switches $C000 from bank 7 → bank 1
    jmp reset                       ; Bank 1 now at $C000
m28_switch_end:
.assert (m28_switch_end - m28_switch) <= $100, error, "m28_switch stub exceeds 256 bytes ($0300-$03FF buffer)"
.endproc

.segment "BOOT_VECS"
    .word $0000             ; NMI (won't fire during bootstrap)
    .word boot_reset        ; Reset → bootstrap
    .word $0000             ; IRQ (won't fire during bootstrap)
.endif
