@ Open-source GBA BIOS replacement
@ MIT License - Copyright (c) 2025 Henrik Kurelid
@
@ Implements: Reset, IRQ dispatch, SWI 0x00-0x10
@ Reference: GBATek BIOS Functions (https://problemkaputt.de/gbatek.htm#biosfunctions)

.arm
.cpu arm7tdmi
.text
.global _start

@ ============================================================================
@ Exception Vector Table (0x00000000 - 0x0000001F)
@ ============================================================================
_start:
    b       reset_handler       @ 0x00: Reset
    b       trap                @ 0x04: Undefined Instruction
    b       swi_handler         @ 0x08: Software Interrupt (SWI)
    b       trap                @ 0x0C: Prefetch Abort
    b       trap                @ 0x10: Data Abort
    b       trap                @ 0x14: Reserved
    b       irq_handler         @ 0x18: IRQ
    b       trap                @ 0x1C: FIQ

@ ============================================================================
@ Trap handler - infinite loop for unhandled exceptions
@ ============================================================================
trap:
    b       trap

@ ============================================================================
@ Reset handler - full boot sequence
@ Matches real GBA BIOS behavior: warm-boot check, stack setup, register
@ clearing, header validation, hardware init, and jump to cartridge.
@ ============================================================================
reset_handler:
    @ --- Warm-boot check ---
    @ If POSTFLG is already 1, this is a warm reset (SoftReset return).
    @ Redirect to the debug handler vector at 0x0000001C.
    ldr     r0, =0x04000300
    ldrb    r0, [r0]
    cmp     r0, #1
    beq     warm_boot

    @ --- Stack pointer setup ---
    @ Set up IRQ mode stack
    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x12      @ IRQ mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007FA0

    @ Set up Supervisor mode stack
    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x13      @ Supervisor mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007FE0

    @ Set up System mode stack (same as User mode SP)
    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x1F      @ System mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007F00

    @ --- Clear registers ---
    mov     r0, #0
    mov     r1, #0
    mov     r2, #0
    mov     r3, #0
    mov     r4, #0
    mov     r5, #0
    mov     r6, #0
    mov     r7, #0
    mov     r8, #0
    mov     r9, #0
    mov     r10, #0
    mov     r11, #0
    mov     r12, #0

    @ --- Header validation ---
    @ Check fixed byte at ROM offset 0xB2 (must be 0x96)
    ldr     r0, =0x080000B2
    ldrb    r0, [r0]
    cmp     r0, #0x96
    bne     header_fail

    @ Compute complement check: sum bytes 0xA0..0xBC, subtract 0x19,
    @ result ANDed with 0xFF must equal byte at 0xBD.
    mov     r0, #0              @ accumulator
    ldr     r1, =0x080000A0     @ start address
    ldr     r2, =0x080000BD     @ end address (exclusive for sum)
.Lheader_loop:
    ldrb    r3, [r1], #1
    sub     r0, r0, r3
    cmp     r1, r2
    blt     .Lheader_loop
    sub     r0, r0, #0x19
    and     r0, r0, #0xFF
    ldrb    r1, [r2]            @ read complement check byte at 0xBD
    cmp     r0, r1
    bne     header_fail

    @ --- Undocumented register write ---
    @ Real GBA BIOS writes 0xFF to 0x04000410 ("probably a bug in the BIOS").
    ldr     r0, =0x04000410
    mov     r1, #0xFF
    strb    r1, [r0]

    @ --- Set POSTFLG ---
    ldr     r0, =0x04000300
    mov     r1, #1
    strb    r1, [r0]

    @ --- Check skip-intro flag ---
    @ If byte at 0x03007FFC is non-zero, skip the intro (logo + jingle).
    @ This flag is set by the emulator when skip-bios-intro is configured.
    ldr     r0, =0x03007FFC
    ldrb    r0, [r0]
    cmp     r0, #0
    bne     boot_finish

    @ ===================================================================
    @ BOOT INTRO — Logo display + jingle + fade
    @ ===================================================================

    @ --- Enable APU ---
    ldr     r4, =0x04000080     @ SOUNDCNT_L base
    mov     r0, #0x80
    strb    r0, [r4, #4]        @ SOUNDCNT_X (0x04000084) = 0x80 (APU on)

    @ --- Configure Sound Channel 1 for jingle ---
    @ SOUNDCNT_L = 0xF377 (max volume both speakers, CH1+2 to R, CH1-4 to L)
    mov     r0, #0x77
    strb    r0, [r4]            @ 0x04000080 low byte = 0x77
    mov     r0, #0xF3
    strb    r0, [r4, #1]        @ 0x04000081 high byte = 0xF3

    @ SOUNDCNT_H = 0x0002 (PSG at 100% ratio)
    ldr     r5, =0x04000082
    mov     r0, #0x02
    strh    r0, [r5]

    @ SOUND1CNT_L = 0x0000 (no sweep)
    ldr     r5, =0x04000060
    mov     r0, #0
    strh    r0, [r5]

    @ SOUND1CNT_H = 0xF380 (vol 15, decay pace 3, duty 50%)
    @ Low byte = 0x80, high byte = 0xF3
    ldr     r5, =0x04000062
    mov     r0, #0x80
    strb    r0, [r5]
    mov     r0, #0xF3
    strb    r0, [r5, #1]

    @ --- Set up Mode 4 display ---
    @ DISPCNT = 0x0404 (Mode 4, BG2 enable)
    ldr     r5, =0x04000000
    ldr     r0, =0x0404
    strh    r0, [r5]

    @ --- Write logo palette ---
    @ Palette entry 1 = white (0x7FFF) for logo text
    ldr     r5, =0x05000002     @ Palette entry 1 (offset 2)
    ldr     r0, =0x7FFF
    strh    r0, [r5]
    @ Palette entry 0 = black (background, already 0)

    @ --- Draw "NESER" logo to VRAM ---
    @ Mode 4 VRAM starts at 0x06000000, 240 bytes per scanline.
    @ Draw centered text starting at approximately row 72, col 80.
    @ Use a simple 5×7 pixel font, each letter 8px wide with 2px spacing.
    ldr     r5, =0x06000000     @ VRAM base
    ldr     r6, =logo_data      @ pointer to compressed logo bitmap
    ldr     r7, =logo_data_end  @ end of logo data
    @ Logo data is stored as (offset_16, count_16) halfword pairs.
    @ Each pair is 4 bytes, maintaining halfword alignment throughout.
    @
    @ GBA VRAM does not support byte writes (STRB duplicates the byte
    @ to both bytes of the halfword).  Use read-modify-write with LDRH/STRH.
.Llogo_copy:
    cmp     r6, r7
    bge     .Llogo_done
    ldrh    r0, [r6], #2        @ offset into VRAM
    ldrh    r1, [r6], #2        @ count of pixels
    add     r2, r5, r0          @ destination in VRAM
.Llogo_pixel:
    subs    r1, r1, #1
    blt     .Llogo_copy
    @ Read-modify-write: set one byte of the halfword at [r2].
    bic     r4, r2, #1          @ halfword-aligned address
    ldrh    r3, [r4]            @ read existing halfword
    tst     r2, #1              @ odd or even byte?
    biceq   r3, r3, #0xFF      @ even: clear low byte
    orreq   r3, r3, #1         @ even: set low byte = palette 1
    bicne   r3, r3, #0xFF00    @ odd: clear high byte
    orrne   r3, r3, #0x100     @ odd: set high byte = palette 1
    strh    r3, [r4]            @ write back halfword
    add     r2, r2, #1          @ advance to next pixel
    b       .Llogo_pixel
.Llogo_done:

    @ --- SoundBias ramp (SWI 0x19) ---
    @ Ramp SOUNDBIAS from 0x000 to 0x200
    mov     r0, #1              @ r0 != 0 means ramp up to 0x200
    swi     0x190000            @ SWI 0x19

    @ --- Wait loop: display logo for ~4 seconds ---
    @ ~240 VBlanks at 59.7Hz ≈ 4.02 seconds
    @ Poll VCOUNT (0x04000006) for scanline 160 (VBlank start)
    mov     r8, #0              @ frame counter
    ldr     r9, =240            @ target frame count
    ldr     r10, =0x04000006    @ REG_VCOUNT
.Lwait_loop:
    @ Wait for VBlank (VCOUNT == 160)
.Lwait_vblank:
    ldrh    r0, [r10]
    cmp     r0, #160
    bne     .Lwait_vblank
    @ Wait for VBlank to end (VCOUNT != 160) to avoid counting same frame twice
.Lwait_vblank_end:
    ldrh    r0, [r10]
    cmp     r0, #160
    beq     .Lwait_vblank_end

    @ Play jingle notes at specific frames
    @ Note 1 "ba" at frame 40: SOUND1CNT_X = 0x8783
    cmp     r8, #40
    bne     .Lno_note1
    ldr     r5, =0x04000064     @ SOUND1CNT_X
    ldr     r0, =0x8783         @ trigger + period for C6
    strh    r0, [r5]
.Lno_note1:
    @ Note 2 "DING" at frame 44: SOUND1CNT_X = 0x87C1
    cmp     r8, #44
    bne     .Lno_note2
    ldr     r5, =0x04000064     @ SOUND1CNT_X
    ldr     r0, =0x87C1         @ trigger + period for C7
    strh    r0, [r5]
.Lno_note2:

    add     r8, r8, #1
    cmp     r8, r9
    blt     .Lwait_loop

    @ --- Fade to white ---
    @ Gradually increase palette entry 0 (background) brightness over 16 frames.
    @ Use GBA's 5-bit RGB: increment R, G, B by 2 each frame (16 steps × 2 = 31 = max).
    mov     r8, #0              @ fade step
    ldr     r5, =0x05000000     @ Palette entry 0
.Lfade_loop:
    @ Wait for next VBlank
.Lfade_vblank:
    ldrh    r0, [r10]
    cmp     r0, #160
    bne     .Lfade_vblank
.Lfade_vblank_end2:
    ldrh    r0, [r10]
    cmp     r0, #160
    beq     .Lfade_vblank_end2

    @ Compute brightness: step * 2 for each R, G, B channel
    add     r8, r8, #1
    mov     r0, r8, lsl #1      @ r0 = step * 2 (0..31)
    cmp     r0, #31
    movgt   r0, #31             @ clamp to 31
    @ Build RGB555: R | (G << 5) | (B << 10)
    orr     r1, r0, r0, lsl #5
    orr     r1, r1, r0, lsl #10
    strh    r1, [r5]            @ Write to palette entry 0

    cmp     r8, #16
    blt     .Lfade_loop

    @ --- Disable display before jumping to game ---
    ldr     r5, =0x04000000
    mov     r0, #0
    strh    r0, [r5]            @ DISPCNT = 0 (forced blank / all off)

    @ --- Clear VRAM logo region (Mode 4 bitmap: 240×160 = 0x9600 bytes) ---
    @ Games expect clean VRAM; the BIOS drew its logo into this region.
    ldr     r0, =0x06000000     @ VRAM start (Mode 4 bitmap base)
    ldr     r2, =0x06009600     @ end of bitmap (240*160 bytes)
    mov     r1, #0
.Lclear_vram_logo:
    str     r1, [r0], #4        @ write 4 bytes at a time (VRAM supports 32-bit)
    cmp     r0, r2
    blt     .Lclear_vram_logo

    @ --- Clear palette entries used by BIOS intro ---
    @ palette[0] was faded to 0x7FFF (white); palette[1] was set white for logo text.
    @ Leave them as 0 (black) so the game starts with a clean backdrop.
    ldr     r0, =0x05000000
    strh    r1, [r0]            @ palette[0] = 0 (black backdrop)
    strh    r1, [r0, #2]        @ palette[1] = 0

    @ --- Silence BIOS jingle before jumping to game ---
    @ Leave master sound enabled, but disable CH1 DAC so SOUNDCNT_X active
    @ channel bits don't leak into cartridge code.
    ldr     r0, =0x04000062     @ SOUND1CNT_H
    strh    r1, [r0]

boot_finish:
    @ --- Enable IRQ/FIQ at CPU level (clear I and F bits in CPSR) ---
    @ Real GBA BIOS enters the game with I=0, F=0 so interrupt service routines
    @ can fire once the game sets IME=1. Without this, games that wait for
    @ VBLANK/timer IRQs will hang indefinitely.
    mrs     r0, cpsr
    bic     r0, r0, #0xC0      @ clear I (bit 7) and F (bit 6)
    msr     cpsr_c, r0

    @ --- Clear registers before jump ---
    mov     r0, #0
    mov     r1, #0
    mov     r2, #0
    mov     r3, #0
    mov     r4, #0
    mov     r5, #0
    mov     r6, #0
    mov     r7, #0
    mov     r8, #0
    mov     r9, #0
    mov     r10, #0
    mov     r11, #0
    mov     r12, #0

    @ --- Jump to cartridge entry point ---
    ldr     pc, =0x08000000

@ ============================================================================
@ Warm-boot handler - redirects to debug vector on soft reset
@ ============================================================================
warm_boot:
    @ Branch to the FIQ/debug vector at 0x0000001C.
    @ On real hardware this would be a debug handler entry point.
    @ We re-use the existing trap there (infinite loop).
    mov     pc, #0x1C

@ ============================================================================
@ Header validation failure - lock up
@ ============================================================================
header_fail:
    b       header_fail

@ Literal pool for the boot sequence (must be within 4KB of ldr= instructions)
.pool

@ ============================================================================
@ SWI Handler
@ Dispatches based on SWI comment field (bits 23:16 of the SWI instruction).
@ Called in Supervisor mode with IRQs disabled.
@ ============================================================================
swi_handler:
    stmfd   sp!, {r11, r12, lr}
    @ Read the SWI instruction to get the comment field.
    @ LR points to instruction after SWI, so SWI is at LR-4 (ARM) or LR-2 (Thumb).
    @ Check SPSR.T (bit 5) to determine the originating instruction set.
    mrs     r12, spsr
    tst     r12, #0x20          @ T bit set → Thumb origin
    ldrneh  r12, [lr, #-2]      @ Thumb: load 16-bit SWI instruction
    andne   r12, r12, #0xFF     @ Thumb: SWI number in bits 7:0
    ldreq   r12, [lr, #-4]      @ ARM: load 32-bit SWI instruction
    moveq   r12, r12, lsr #16   @ ARM: SWI number in bits 23:16
    andeq   r12, r12, #0xFF

    @ Dispatch table
    cmp     r12, #0x00
    beq     swi_soft_reset
    cmp     r12, #0x01
    beq     swi_register_ram_reset
    cmp     r12, #0x02
    beq     swi_halt
    cmp     r12, #0x03
    beq     swi_stop
    cmp     r12, #0x04
    beq     swi_intr_wait
    cmp     r12, #0x05
    beq     swi_vblank_intr_wait
    cmp     r12, #0x06
    beq     swi_div
    cmp     r12, #0x07
    beq     swi_div_arm
    cmp     r12, #0x08
    beq     swi_sqrt
    cmp     r12, #0x09
    beq     swi_arctan
    cmp     r12, #0x0A
    beq     swi_arctan2
    cmp     r12, #0x0B
    beq     swi_cpu_set
    cmp     r12, #0x0C
    beq     swi_cpu_fast_set
    cmp     r12, #0x0D
    beq     swi_bios_checksum
    cmp     r12, #0x0E
    beq     swi_bg_affine_set
    cmp     r12, #0x0F
    beq     swi_obj_affine_set
    cmp     r12, #0x10
    beq     swi_bit_unpack
    cmp     r12, #0x11
    beq     swi_lz77_wram
    cmp     r12, #0x12
    beq     swi_lz77_vram
    cmp     r12, #0x13
    beq     swi_huffman
    cmp     r12, #0x14
    beq     swi_rle_wram
    cmp     r12, #0x15
    beq     swi_rle_vram
    cmp     r12, #0x16
    beq     swi_diff8_wram
    cmp     r12, #0x17
    beq     swi_diff8_vram
    cmp     r12, #0x18
    beq     swi_diff16
    cmp     r12, #0x19
    beq     swi_sound_bias
    cmp     r12, #0x1F
    beq     swi_midi_key2freq
    cmp     r12, #0x25
    beq     swi_multiboot

    @ SWIs 0x1A-0x1E, 0x20-0x24, 0x26-0x2A: stubs (just return)
    @ 0x1A: SoundDriverInit — no sound mixer implemented
    @ 0x1B: SoundDriverMode — no sound mixer implemented
    @ 0x1C: SoundDriverMain — no sound mixer implemented
    @ 0x1D: SoundDriverVSync — no sound mixer implemented
    @ 0x1E: SoundChannelClear — no sound mixer implemented
    @ 0x20-0x24: Undocumented — rarely/never used by commercial games
    @ 0x26: HardReset — would require full system reset, just returns
    @ 0x27: CustomHalt — low-power halt modes not emulated
    @ 0x28: SoundDriverVSyncOff — no sound mixer implemented
    @ 0x29: SoundDriverVSyncOn — no sound mixer implemented
    @ 0x2A: SoundGetJumpList — no sound mixer, returns no data

    @ Unknown/stubbed SWI: just return
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x00: SoftReset
@ Clears 0x03007E00-0x03007FFF, resets stack pointers, and jumps to
@ ROM or RAM entry based on flag at 0x03007FFA.
@ ============================================================================
swi_soft_reset:
    @ Clear 0x03007E00 - 0x03007FFF (0x200 bytes = 128 words)
    ldr     r0, =0x03007E00
    mov     r1, #0
    mov     r2, #128
.soft_reset_clear:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .soft_reset_clear

    @ Re-initialize stack pointers
    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x12      @ IRQ mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007FA0

    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x13      @ Supervisor mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007FE0

    mrs     r0, cpsr
    bic     r0, r0, #0x1F
    orr     r0, r0, #0x1F      @ System mode
    msr     cpsr_c, r0
    ldr     sp, =0x03007F00

    @ Read return address flag at 0x03007FFA
    @ 0x00 = return to ROM (0x08000000), non-zero = return to RAM (0x02000000)
    ldr     r0, =0x03007FFA
    ldrb    r0, [r0]
    cmp     r0, #0
    ldreq   pc, =0x08000000
    ldrne   pc, =0x02000000

@ ============================================================================
@ SWI 0x01: RegisterRamReset
@ Selectively clears memory regions based on flag bits in r0.
@ Bit 0: Clear 256K EWRAM (0x02000000-0x0203FFFF)
@ Bit 1: Clear 32K IWRAM (0x03000000-0x03007FFF)  (excl. last 0x200 bytes)
@ Bit 2: Clear Palette RAM (0x05000000-0x050003FF)
@ Bit 3: Clear VRAM (0x06000000-0x06017FFF)
@ Bit 4: Clear OAM (0x07000000-0x070003FF)
@ Bit 5: Reset SIO registers
@ Bit 6: Reset Sound registers
@ Bit 7: Reset other registers
@ ============================================================================
swi_register_ram_reset:
    @ Save the flags
    mov     r11, r0

    @ Bit 0: Clear EWRAM
    tst     r11, #0x01
    beq     .skip_ewram
    ldr     r0, =0x02000000
    mov     r1, #0
    ldr     r2, =0x10000       @ 256KB / 4 = 64K words
.clear_ewram:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .clear_ewram
.skip_ewram:

    @ Bit 1: Clear IWRAM (0x03000000-0x03007DFF, preserve last 0x200 bytes)
    tst     r11, #0x02
    beq     .skip_iwram
    ldr     r0, =0x03000000
    mov     r1, #0
    ldr     r2, =0x1F80        @ (32K - 0x200) / 4 = 0x1F80 words
.clear_iwram:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .clear_iwram
.skip_iwram:

    @ Bit 2: Clear Palette RAM
    tst     r11, #0x04
    beq     .skip_palette
    ldr     r0, =0x05000000
    mov     r1, #0
    mov     r2, #256            @ 1KB / 4 = 256 words
.clear_palette:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .clear_palette
.skip_palette:

    @ Bit 3: Clear VRAM
    tst     r11, #0x08
    beq     .skip_vram
    ldr     r0, =0x06000000
    mov     r1, #0
    ldr     r2, =0x6000        @ 96KB / 4 = 0x6000 words
.clear_vram:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .clear_vram
.skip_vram:

    @ Bit 4: Clear OAM
    tst     r11, #0x10
    beq     .skip_oam
    ldr     r0, =0x07000000
    mov     r1, #0
    mov     r2, #256            @ 1KB / 4 = 256 words
.clear_oam:
    str     r1, [r0], #4
    subs    r2, r2, #1
    bne     .clear_oam
.skip_oam:

    @ Bits 5-7: Register resets (stub - just acknowledge)
    @ TODO: Implement full register reset for SIO, Sound, other registers

    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x02: Halt
@ Halts the CPU until any enabled interrupt fires.
@ Writes 0x00 to HALTCNT (0x04000301).
@ ============================================================================
swi_halt:
    ldmfd   sp!, {r11, r12, lr}
    @ Write to HALTCNT to enter halt state
    mov     r0, #0x04000000
    mov     r1, #0
    strb    r1, [r0, #0x301]
    @ CPU halts here until interrupt
    movs    pc, lr

@ ============================================================================
@ SWI 0x03: Stop
@ Stops the CPU (deeper power-down mode).
@ Writes 0x80 to HALTCNT (0x04000301).
@ ============================================================================
swi_stop:
    ldmfd   sp!, {r11, r12, lr}
    mov     r0, #0x04000000
    mov     r1, #0x80
    strb    r1, [r0, #0x301]
    movs    pc, lr

@ ============================================================================
@ SWI 0x04: IntrWait
@ r0 = discard_old (if 1, clear existing flags first)
@ r1 = interrupt flag mask to wait for
@ Waits until the specified interrupt(s) fire.
@ Uses BIOS interrupt flags at 0x03007FF8 (IntrCheck / IF_BIOS).
@ ============================================================================
swi_intr_wait:
    ldmfd   sp!, {r11, r12, lr}
    @ Save the waiting flags
    stmfd   sp!, {r4, r5, lr}
    mov     r4, r1              @ r4 = flag mask to wait for
    ldr     r5, =0x03007FF8    @ IntrCheck address (IF_BIOS)

    @ If r0 != 0, clear the current flags
    cmp     r0, #0
    beq     .intr_wait_loop
    ldrh    r2, [r5]
    bic     r2, r2, r4
    strh    r2, [r5]

.intr_wait_loop:
    @ Set REG_IME=1 so the IRQ handler runs even if the ROM had IME=0
    mov     r0, #0x04000000
    mov     r1, #1
    str     r1, [r0, #0x208]    @ REG_IME = 1

    @ Enable IRQs in CPSR too
    mrs     r0, cpsr
    bic     r0, r0, #0x80       @ Clear I bit (enable IRQ)
    msr     cpsr_c, r0

    @ Halt CPU until next interrupt
    mov     r0, #0x04000000
    mov     r1, #0
    strb    r1, [r0, #0x301]

    @ Disable IRQs while we check BIOS IF
    mrs     r0, cpsr
    orr     r0, r0, #0x80       @ Set I bit (disable IRQ)
    msr     cpsr_c, r0

    @ Check if our desired interrupt(s) have fired
    ldrh    r2, [r5]
    tst     r2, r4
    beq     .intr_wait_loop

    @ Clear the flags we were waiting for
    bic     r2, r2, r4
    strh    r2, [r5]

    @ Match the observed BIOS IntrWait return latency after the awaited IRQ.
    @ Timer phase tests depend on this synchronization point.
    .rept   25
    nop
    .endr

    ldmfd   sp!, {r4, r5, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x05: VBlankIntrWait
@ Equivalent to IntrWait(1, 0x0001) - wait for VBlank interrupt.
@ ============================================================================
swi_vblank_intr_wait:
    ldmfd   sp!, {r11, r12, lr}
    mov     r0, #1              @ discard_old = 1
    mov     r1, #1              @ flag mask = VBlank (bit 0)
    stmfd   sp!, {r4, r5, lr}
    mov     r4, r1
    ldr     r5, =0x03007FF8

    @ Clear existing VBlank flag
    ldrh    r2, [r5]
    bic     r2, r2, r4
    strh    r2, [r5]

.vblank_wait_loop:
    @ Set REG_IME=1 so the IRQ handler runs even if the ROM had IME=0
    mov     r0, #0x04000000
    mov     r1, #1
    str     r1, [r0, #0x208]    @ REG_IME = 1

    @ Enable IRQs in CPSR too
    mrs     r0, cpsr
    bic     r0, r0, #0x80
    msr     cpsr_c, r0

    @ Halt CPU
    mov     r0, #0x04000000
    mov     r1, #0
    strb    r1, [r0, #0x301]

    @ Disable IRQs while checking BIOS IF
    mrs     r0, cpsr
    orr     r0, r0, #0x80
    msr     cpsr_c, r0

    ldrh    r2, [r5]
    tst     r2, r4
    beq     .vblank_wait_loop

    bic     r2, r2, r4
    strh    r2, [r5]

    ldmfd   sp!, {r4, r5, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x06: Div
@ r0 = numerator (signed), r1 = denominator (signed)
@ Returns: r0 = quotient, r1 = remainder, r3 = abs(quotient)
@ ============================================================================
swi_div:
    stmfd   sp!, {r4, r5}

    @ Save original numerator sign in r5
    mov     r5, r0

    @ Handle signs
    mov     r4, #0              @ r4 = sign flag (0=positive, 1=negative)
    cmp     r0, #0
    rsblt   r0, r0, #0         @ r0 = abs(numerator)
    eorlt   r4, r4, #1         @ flip sign

    cmp     r1, #0
    rsblt   r1, r1, #0         @ r1 = abs(denominator)
    eorlt   r4, r4, #1         @ flip sign

    @ Division by zero: GBA BIOS behavior - just returns large values
    cmp     r1, #0
    beq     .div_by_zero

    @ Unsigned division: r0 / r1
    mov     r2, #0              @ quotient
    mov     r3, #1              @ bit position

    @ Find highest bit where divisor <= dividend
    @ Guard: stop if divisor MSB is set (shifting would overflow to 0)
.div_shift:
    cmp     r1, r0
    bhi     .div_loop           @ divisor > dividend, done shifting
    tst     r1, #0x80000000     @ would next shift overflow?
    bne     .div_loop
    mov     r1, r1, lsl #1
    mov     r3, r3, lsl #1
    b       .div_shift

    @ Subtract and accumulate quotient
.div_loop:
    cmp     r3, #0
    beq     .div_done
    cmp     r0, r1
    subcs   r0, r0, r1
    addcs   r2, r2, r3
    mov     r1, r1, lsr #1
    mov     r3, r3, lsr #1
    b       .div_loop

.div_done:
    @ r2 = quotient, r0 = remainder (unsigned)
    mov     r1, r0              @ r1 = remainder
    mov     r0, r2              @ r0 = quotient (unsigned)
    mov     r3, r0              @ r3 = abs(quotient)

    @ Apply sign to quotient (negative if signs of operands differ)
    cmp     r4, #0
    rsbne   r0, r0, #0          @ negate quotient

    @ Apply sign to remainder (same sign as original numerator)
    cmp     r5, #0
    rsblt   r1, r1, #0          @ negate remainder if numerator was negative

    ldmfd   sp!, {r4, r5}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

.div_by_zero:
    @ GBA BIOS returns: r0 = ±1 depending on numerator sign, r1 = numerator, r3 = 1
    @ Actually the behavior is somewhat undefined; we follow common convention
    mov     r0, #0
    mov     r1, #0
    mov     r3, #0
    ldmfd   sp!, {r4, r5}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x07: DivArm
@ Same as Div but with r0 and r1 swapped.
@ r0 = denominator, r1 = numerator
@ Returns: r0 = quotient, r1 = remainder, r3 = abs(quotient)
@ ============================================================================
swi_div_arm:
    @ Swap r0 and r1, then fall through to Div
    mov     r12, r0
    mov     r0, r1
    mov     r1, r12
    b       swi_div

@ ============================================================================
@ SWI 0x08: Sqrt
@ r0 = value (unsigned 32-bit)
@ Returns: r0 = floor(sqrt(r0))
@ Uses iterative bit-by-bit method.
@ ============================================================================
swi_sqrt:
    @ Newton-like integer sqrt (bit-by-bit)
    mov     r1, r0              @ r1 = input value
    mov     r0, #0              @ r0 = result
    mov     r2, #0x40000000     @ r2 = bit (start from highest power of 4)

.sqrt_loop:
    cmp     r2, #0
    beq     .sqrt_done

    orr     r3, r0, r2          @ r3 = result | bit
    cmp     r1, r3
    subcs   r1, r1, r3          @ if input >= (result|bit): input -= (result|bit)
    movcs   r0, r0, lsr #1     @   result >>= 1
    orrcs   r0, r0, r2         @   result |= bit
    movcc   r0, r0, lsr #1     @ else: result >>= 1

    mov     r2, r2, lsr #2     @ bit >>= 2
    b       .sqrt_loop

.sqrt_done:
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x0D: BiosChecksum
@ Returns a checksum of the BIOS in r0.
@ The original GBA BIOS returns 0xBAAE187F.
@ ============================================================================
swi_bios_checksum:
    ldr     r0, =0xBAAE187F    @ Original GBA/GBA SP BIOS checksum
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x09: ArcTan
@ r0 = tan (signed, s1.14 fixed-point)
@ Returns: r0 = angle, r1 = -(tan^2 >> 14), r3 = polynomial result
@ Uses Horner's method with 8 coefficients from the real GBA BIOS.
@ ============================================================================
swi_arctan:
    stmfd   sp!, {r4, lr}

    @ a = -(r0 * r0) >> 14
    mov     r4, r0              @ r4 = original input (i)
    smull   r1, r3, r0, r0     @ r1:r3 = i * i (64-bit signed)
    mov     r1, r1, lsr #14
    orr     r1, r1, r3, lsl #18
    rsb     r1, r1, #0          @ r1 = a = -(i*i >> 14)

    @ Horner's evaluation: b = (((...)*a >> 14) + coeff) for each coefficient
    @ Coefficients (from innermost): 0xA9, 0x390, 0x91C, 0xFB6, 0x16AA, 0x2081, 0x3651, 0xA2F9
    ldr     r3, =0x00A9         @ b = 0xA9
    bl      .arctan_horner_step @ b = (b * a >> 14) + 0x390
    ldr     r0, =0x0390
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x091C
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x0FB6
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x16AA
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x2081
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x3651
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0xA2F9
    add     r3, r3, r0

    @ result = (i * b) >> 16
    smull   r0, r2, r4, r3     @ r0:r2 = i * b
    mov     r0, r0, lsr #16
    orr     r0, r0, r2, lsl #16

    ldmfd   sp!, {r4, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

.arctan_horner_step:
    @ r3 = (r3 * r1) >> 14, where r1 = a
    smull   r0, r2, r3, r1     @ r0:r2 = b * a (64-bit)
    mov     r3, r0, lsr #14
    orr     r3, r3, r2, lsl #18
    bx      lr

@ ============================================================================
@ SWI 0x0A: ArcTan2
@ r0 = X (signed s1.14), r1 = Y (signed s1.14)
@ Returns: r0 = angle (0x0000-0xFFFF, full circle), r3 = 0x170
@ ============================================================================
swi_arctan2:
    stmfd   sp!, {r4-r7, lr}

    mov     r4, r0              @ r4 = X
    mov     r5, r1              @ r5 = Y

    @ Handle Y == 0
    cmp     r5, #0
    bne     .at2_check_x_zero
    cmp     r4, #0
    movge   r0, #0              @ X >= 0: angle = 0
    ldrlt   r0, =0x8000         @ X < 0: angle = 0x8000 (180°)
    b       .at2_done

.at2_check_x_zero:
    @ Handle X == 0
    cmp     r4, #0
    bne     .at2_quadrant
    cmp     r5, #0
    ldrge   r0, =0x4000         @ Y >= 0: angle = 0x4000 (90°)
    ldrlt   r0, =0xC000         @ Y < 0: angle = 0xC000 (270°)
    b       .at2_done

.at2_quadrant:
    @ Determine quadrant and compute ratio for ArcTan
    @ Strategy: always pass |smaller/larger| to ArcTan (keeps ratio <= 1)
    @ then adjust result based on quadrant and octant

    @ Get absolute values
    cmp     r4, #0
    rsblt   r6, r4, #0          @ r6 = |X|
    movge   r6, r4
    cmp     r5, #0
    rsblt   r7, r5, #0          @ r7 = |Y|
    movge   r7, r5

    @ Compute ratio: if |X| >= |Y|, ratio = (Y << 14) / X, else = (X << 14) / Y
    cmp     r6, r7
    bge     .at2_x_dominant

    @ |Y| > |X|: ratio = X/Y (for octants 45-90)
    mov     r0, r4, lsl #14     @ numerator = X << 14
    mov     r1, r5              @ denominator = Y
    bl      .at2_divide
    @ r0 = (X << 14) / Y = ratio

    @ Call internal arctan
    bl      .at2_arctan_internal

    @ Adjust: result = 0x4000 - arctan_result for Y>0, 0xC000 - arctan_result for Y<0
    cmp     r5, #0
    ldrge   r1, =0x4000
    ldrlt   r1, =0xC000
    sub     r0, r1, r0
    b       .at2_done

.at2_x_dominant:
    @ |X| >= |Y|: ratio = (Y << 14) / X
    mov     r0, r5, lsl #14
    mov     r1, r4
    bl      .at2_divide
    bl      .at2_arctan_internal
    @ r4 (X) and r5 (Y) preserved by both calls

    cmp     r4, #0
    bge     .at2_x_pos
    ldr     r1, =0x8000
    add     r0, r0, r1
    b       .at2_done

.at2_x_pos:
    cmp     r5, #0
    bge     .at2_done
    ldr     r1, =0x10000
    add     r0, r0, r1
    b       .at2_done

.at2_done:
    @ Mask to 16-bit
    mov     r0, r0, lsl #16
    mov     r0, r0, lsr #16
    ldr     r3, =0x170          @ r3 = 0x170 (matches real BIOS clobber)

    ldmfd   sp!, {r4-r7, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ Internal signed division for ArcTan2: r0 = r0 / r1 (both signed)
.at2_divide:
    stmfd   sp!, {r4, lr}
    mov     r4, #0              @ sign flag
    cmp     r0, #0
    rsblt   r0, r0, #0
    eorlt   r4, r4, #1
    cmp     r1, #0
    rsblt   r1, r1, #0
    eorlt   r4, r4, #1

    cmp     r1, #0
    moveq   r0, #0
    beq     .at2_div_done

    @ Unsigned division
    mov     r2, #0              @ quotient
    mov     r3, #1
.at2_div_shift:
    cmp     r1, r0
    bhi     .at2_div_loop
    tst     r1, #0x80000000
    bne     .at2_div_loop
    mov     r1, r1, lsl #1
    mov     r3, r3, lsl #1
    b       .at2_div_shift
.at2_div_loop:
    cmp     r3, #0
    beq     .at2_div_end
    cmp     r0, r1
    subcs   r0, r0, r1
    addcs   r2, r2, r3
    mov     r1, r1, lsr #1
    mov     r3, r3, lsr #1
    b       .at2_div_loop
.at2_div_end:
    mov     r0, r2
.at2_div_done:
    cmp     r4, #0
    rsbne   r0, r0, #0
    ldmfd   sp!, {r4, lr}
    bx      lr

@ Internal ArcTan for ArcTan2 (same algorithm, uses r0 as input)
.at2_arctan_internal:
    stmfd   sp!, {r4, lr}
    mov     r4, r0              @ save input

    @ a = -(r0 * r0) >> 14
    smull   r1, r3, r0, r0
    mov     r1, r1, lsr #14
    orr     r1, r1, r3, lsl #18
    rsb     r1, r1, #0          @ r1 = a

    ldr     r3, =0x00A9
    bl      .arctan_horner_step
    ldr     r0, =0x0390
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x091C
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x0FB6
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x16AA
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x2081
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0x3651
    add     r3, r3, r0
    bl      .arctan_horner_step
    ldr     r0, =0xA2F9
    add     r3, r3, r0

    @ result = (input * b) >> 16
    smull   r0, r2, r4, r3
    mov     r0, r0, lsr #16
    orr     r0, r0, r2, lsl #16

    ldmfd   sp!, {r4, lr}
    bx      lr

@ ============================================================================
@ SWI 0x0B: CpuSet
@ r0 = source address, r1 = destination address
@ r2 = count + flags: bits 0-20 = count, bit 24 = fill, bit 26 = 32-bit
@ ============================================================================
swi_cpu_set:
    stmfd   sp!, {r4-r6, lr}

    @ Extract count (bits 0-20)
    bic     r3, r2, #0xFF000000
    bic     r3, r3, #0x00E00000  @ r3 = count (bits 0-20)

    @ Align addresses. 16-bit CpuSet preserves an odd source byte lane but
    @ still aligns the destination; 32-bit mode aligns both endpoints for
    @ normal memory. Cart RAM (SRAM 0x0E/0x0F) is an 8-bit bus, so its
    @ effective byte lane is preserved by skipping alignment for those regions.
    tst     r2, #(1 << 26)      @ 32-bit mode?
    bne     .cpuset_align32
    biceq   r1, r1, #1
    b       .cpuset_aligned
.cpuset_align32:
    mov     r4, r0, lsr #24
    and     r4, r4, #0xF
    cmp     r4, #0xE
    biclo   r0, r0, #3
    mov     r4, r1, lsr #24
    and     r4, r4, #0xF
    cmp     r4, #0xE
    biclo   r1, r1, #3
.cpuset_aligned:

    @ Check fill mode (bit 24)
    tst     r2, #(1 << 24)
    bne     .cpuset_fill

    @ Copy mode
    tst     r2, #(1 << 26)
    bne     .cpuset_copy32

    @ 16-bit copy
    tst     r0, #1
    bne     .cpuset_copy16_odd_source
.cpuset_copy16:
    cmp     r3, #0
    beq     .cpuset_done
    ldrh    r4, [r0], #2
    strh    r4, [r1], #2
    sub     r3, r3, #1
    b       .cpuset_copy16

.cpuset_copy16_odd_source:
    cmp     r3, #0
    beq     .cpuset_done
    ldrb    r4, [r0], #2
    strh    r4, [r1], #2
    sub     r3, r3, #1
    b       .cpuset_copy16_odd_source

    @ 32-bit copy
.cpuset_copy32:
    cmp     r3, #0
    beq     .cpuset_done
    ldr     r4, [r0], #4
    str     r4, [r1], #4
    sub     r3, r3, #1
    b       .cpuset_copy32

    @ Fill mode
.cpuset_fill:
    tst     r2, #(1 << 26)
    bne     .cpuset_fill32

    @ 16-bit fill
    tst     r0, #1
    ldrneb  r4, [r0]
    ldreqh  r4, [r0]
.cpuset_fill16:
    cmp     r3, #0
    beq     .cpuset_done
    strh    r4, [r1], #2
    sub     r3, r3, #1
    b       .cpuset_fill16

    @ 32-bit fill
.cpuset_fill32:
    ldr     r4, [r0]
.cpuset_fill32_loop:
    cmp     r3, #0
    beq     .cpuset_done
    str     r4, [r1], #4
    sub     r3, r3, #1
    b       .cpuset_fill32_loop

.cpuset_done:
    ldmfd   sp!, {r4-r6, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x0C: CpuFastSet
@ r0 = source address, r1 = destination address
@ r2 = count + flags: bits 0-20 = wordcount, bit 24 = fill
@ Always 32-bit. Count rounded up to multiple of 8.
@ ============================================================================
swi_cpu_fast_set:
    stmfd   sp!, {r4-r11, lr}

    @ Align normal-memory endpoints to 4 bytes. Cart RAM is an 8-bit bus, so
    @ SRAM and its mirror must preserve the effective byte lane.
    mov     r4, r0, lsr #24
    and     r4, r4, #0xF
    cmp     r4, #0xE
    biclo   r0, r0, #3
    mov     r4, r1, lsr #24
    and     r4, r4, #0xF
    cmp     r4, #0xE
    biclo   r1, r1, #3

    @ Extract count (bits 0-20) and round up to multiple of 8
    bic     r3, r2, #0xFF000000
    bic     r3, r3, #0x00E00000  @ r3 = raw count
    add     r3, r3, #7
    bic     r3, r3, #7          @ r3 = count rounded up to ×8

    @ Check fill mode (bit 24)
    tst     r2, #(1 << 24)
    bne     .cpufastset_fill

    @ Copy mode: 8 words at a time using LDMIA/STMIA
.cpufastset_copy:
    cmp     r3, #0
    beq     .cpufastset_done
    ldmia   r0!, {r4-r11}
    stmia   r1!, {r4-r11}
    sub     r3, r3, #8
    b       .cpufastset_copy

    @ Fill mode: read one word, replicate
.cpufastset_fill:
    ldr     r4, [r0]
    mov     r5, r4
    mov     r6, r4
    mov     r7, r4
    mov     r8, r4
    mov     r9, r4
    mov     r10, r4
    mov     r11, r4
.cpufastset_fill_loop:
    cmp     r3, #0
    beq     .cpufastset_done
    stmia   r1!, {r4-r11}
    sub     r3, r3, #8
    b       .cpufastset_fill_loop

.cpufastset_done:
    ldmfd   sp!, {r4-r11, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x0E: BgAffineSet
@ r0 = ptr to source data array (20 bytes per entry)
@ r1 = ptr to dest data array (16 bytes per entry)
@ r2 = number of calculations
@ Source: {s32 cx, s32 cy, s16 disp_cx, s16 disp_cy, s16 scale_x, s16 scale_y, u16 angle, u16 pad}
@ Dest:   {s16 pa, s16 pb, s16 pc, s16 pd, s32 x0, s32 y0}
@ ============================================================================
swi_bg_affine_set:
    stmfd   sp!, {r4-r10, lr}

.bgaff_loop:
    subs    r2, r2, #1
    blt     .bgaff_done

    @ Save src, dst, remaining count
    stmfd   sp!, {r0, r1, r2}

    @ Load source struct (20 bytes total, advances r0 by 20)
    ldr     r3, [r0], #4        @ cx (s32)
    ldr     r4, [r0], #4        @ cy (s32)
    ldrsh   r5, [r0], #2        @ disp_cx (s16)
    ldrsh   r6, [r0], #2        @ disp_cy (s16)
    ldrsh   r7, [r0], #2        @ scale_x (s16)
    ldrsh   r8, [r0], #2        @ scale_y (s16)
    ldrh    r9, [r0], #4        @ angle (u16), skip 2 pad bytes

    @ Update src_ptr on stack for next iteration
    str     r0, [sp, #0]

    @ Save cx, cy, disp_cx, disp_cy, scale_x, scale_y
    stmfd   sp!, {r3, r4, r5, r6, r7, r8}
    @ Stack: [sp+0]=cx [sp+4]=cy [sp+8]=disp_cx [sp+12]=disp_cy
    @        [sp+16]=scale_x [sp+20]=scale_y
    @        [sp+24]=src [sp+28]=dst [sp+32]=count

    @ Sin/cos lookup from upper 8 bits of angle
    mov     r9, r9, lsr #8      @ index = angle >> 8
    ldr     r0, =sine_lut
    add     r3, r9, #64
    and     r3, r3, #0xFF
    mov     r3, r3, lsl #1      @ byte offset for cos
    ldrsh   r10, [r0, r3]       @ r10 = cos (s1.14)
    mov     r9, r9, lsl #1      @ byte offset for sin
    ldrsh   r9, [r0, r9]        @ r9 = sin (s1.14)

    @ Load scale values from stack
    ldr     r5, [sp, #16]       @ scale_x
    ldr     r6, [sp, #20]       @ scale_y

    @ pa = (cos << 2) / scale_x  [s1.14 → s8.8: shift by 8-14+8 = 2]
    mov     r0, r10, lsl #2
    mov     r1, r5
    bl      .affine_divide
    mov     r7, r0              @ r7 = pa

    @ pb = (-sin << 2) / scale_y
    rsb     r0, r9, #0
    mov     r0, r0, lsl #2
    mov     r1, r6
    bl      .affine_divide
    mov     r8, r0              @ r8 = pb

    @ pc = (sin << 2) / scale_x
    mov     r0, r9, lsl #2
    mov     r1, r5
    bl      .affine_divide
    mov     r5, r0              @ r5 = pc (scale_x no longer needed)

    @ pd = (cos << 2) / scale_y
    mov     r0, r10, lsl #2
    mov     r1, r6
    bl      .affine_divide
    mov     r6, r0              @ r6 = pd (scale_y no longer needed)

    @ Store pa, pb, pc, pd to dest (as s16 halfwords)
    ldr     r1, [sp, #28]       @ dst ptr
    strh    r7, [r1], #2        @ pa
    strh    r8, [r1], #2        @ pb
    strh    r5, [r1], #2        @ pc
    strh    r6, [r1], #2        @ pd

    @ Load cx, cy, disp_cx, disp_cy from stack
    ldr     r0, [sp, #0]        @ cx
    ldr     r2, [sp, #4]        @ cy
    ldr     r3, [sp, #8]        @ disp_cx
    ldr     r4, [sp, #12]       @ disp_cy

    @ x0 = cx - pa*disp_cx - pb*disp_cy
    smull   r9, r10, r7, r3     @ pa * disp_cx (64-bit)
    sub     r0, r0, r9          @ cx - pa*disp_cx (low 32 bits suffice)
    smull   r9, r10, r8, r4     @ pb * disp_cy
    sub     r0, r0, r9          @ x0 = cx - pa*disp_cx - pb*disp_cy
    str     r0, [r1], #4        @ store x0

    @ y0 = cy - pc*disp_cx - pd*disp_cy
    smull   r9, r10, r5, r3     @ pc * disp_cx
    sub     r2, r2, r9
    smull   r9, r10, r6, r4     @ pd * disp_cy
    sub     r2, r2, r9          @ y0 = cy - pc*disp_cx - pd*disp_cy
    str     r2, [r1]            @ store y0

    @ Update dst ptr on stack (advanced by 16 bytes: 4*s16 + 2*s32)
    add     r1, r1, #4          @ past y0
    str     r1, [sp, #28]       @ update dst

    @ Pop saved source values (discard) and iteration state
    add     sp, sp, #24         @ discard cx/cy/disp_cx/disp_cy/scale_x/scale_y
    ldmfd   sp!, {r0, r1, r2}   @ restore src, dst, count

    b       .bgaff_loop

.bgaff_done:
    ldmfd   sp!, {r4-r10, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x0F: ObjAffineSet
@ r0 = ptr to source data array (8 bytes per entry: s16 sx, s16 sy, u16 angle, u16 pad)
@ r1 = ptr to dest (PA/PB/PC/PD as s16, with stride r3 between each)
@ r2 = number of calculations
@ r3 = stride (byte offset between consecutive PA/PB/PC/PD entries)
@ ============================================================================
swi_obj_affine_set:
    stmfd   sp!, {r4-r10, lr}

    mov     r10, r3             @ r10 = stride

.objaff_loop:
    subs    r2, r2, #1
    blt     .objaff_done

    stmfd   sp!, {r0, r1, r2}

    @ Load source struct (8 bytes, advances r0)
    ldrsh   r5, [r0], #2        @ scale_x (s16)
    ldrsh   r6, [r0], #2        @ scale_y (s16)
    ldrh    r7, [r0], #4        @ angle (u16), skip 2 pad bytes

    @ Update src_ptr
    str     r0, [sp, #0]

    @ Sin/cos lookup
    mov     r7, r7, lsr #8
    ldr     r0, =sine_lut
    add     r3, r7, #64
    and     r3, r3, #0xFF
    mov     r3, r3, lsl #1      @ byte offset for cos
    ldrsh   r9, [r0, r3]        @ r9 = cos (s1.14)
    mov     r7, r7, lsl #1      @ byte offset for sin
    ldrsh   r8, [r0, r7]        @ r8 = sin (s1.14)

    @ pa = (cos << 2) / scale_x  [s1.14 → s8.8: shift by 8-14+8 = 2]
    mov     r0, r9, lsl #2
    mov     r1, r5
    bl      .affine_divide
    mov     r7, r0              @ r7 = pa

    @ pb = (-sin << 2) / scale_y
    rsb     r0, r8, #0
    mov     r0, r0, lsl #2
    mov     r1, r6
    bl      .affine_divide
    mov     r4, r0              @ r4 = pb

    @ pc = (sin << 2) / scale_x
    mov     r0, r8, lsl #2
    mov     r1, r5
    bl      .affine_divide
    mov     r5, r0              @ r5 = pc

    @ pd = (cos << 2) / scale_y
    mov     r0, r9, lsl #2
    mov     r1, r6
    bl      .affine_divide
    mov     r6, r0              @ r6 = pd

    @ Store PA, PB, PC, PD with stride
    ldmfd   sp!, {r0, r1, r2}
    strh    r7, [r1]            @ PA
    add     r1, r1, r10
    strh    r4, [r1]            @ PB
    add     r1, r1, r10
    strh    r5, [r1]            @ PC
    add     r1, r1, r10
    strh    r6, [r1]            @ PD
    add     r1, r1, r10         @ advance past PD

    b       .objaff_loop

.objaff_done:
    ldmfd   sp!, {r4-r10, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x10: BitUnPack
@ r0 = source address
@ r1 = destination address (word-aligned)
@ r2 = pointer to info: {u16 src_len, u8 src_width, u8 dst_width, u32 data_offset}
@ data_offset bit 31 = zero flag (add offset to zeros too)
@ ============================================================================
swi_bit_unpack:
    stmfd   sp!, {r4-r10, lr}

    @ Load info struct
    ldrh    r3, [r2]            @ src_len (bytes)
    ldrb    r4, [r2, #2]        @ src_width (1, 2, 4, or 8 bits)
    ldrb    r5, [r2, #3]        @ dst_width (1, 2, 4, 8, 16, or 32 bits)
    ldr     r6, [r2, #4]        @ data_offset (bit 31 = zero flag)

    mov     r7, #0              @ output accumulator
    mov     r8, #0              @ bits accumulated in output word
    mov     r9, #1
    mov     r9, r9, lsl r4
    sub     r9, r9, #1          @ src_mask = (1 << src_width) - 1

.bitunp_byte_loop:
    cmp     r3, #0
    beq     .bitunp_flush
    sub     r3, r3, #1

    ldrb    r10, [r0], #1       @ read source byte
    mov     r2, #0              @ bits consumed from this byte

.bitunp_bit_loop:
    cmp     r2, #8
    bge     .bitunp_byte_loop

    @ Extract src_width bits
    and     lr, r10, r9         @ value = byte & src_mask
    mov     r10, r10, lsr r4    @ shift byte right by src_width
    add     r2, r2, r4          @ bits consumed += src_width

    @ Apply data offset
    cmp     lr, #0
    bne     .bitunp_nonzero
    @ Zero value: add offset only if zero flag (bit 31) set
    tst     r6, #0x80000000
    beq     .bitunp_store       @ zero flag clear: store 0
.bitunp_nonzero:
    @ Non-zero (or zero with flag): add offset (bits 0-30)
    bic     r14, r6, #0x80000000  @ clear zero flag bit
    add     lr, lr, r14

.bitunp_store:
    @ Place value at current bit position in output word
    orr     r7, r7, lr, lsl r8
    add     r8, r8, r5          @ advance by dst_width bits

    @ If we've filled 32 bits, write the word
    cmp     r8, #32
    blt     .bitunp_bit_loop
    str     r7, [r1], #4        @ write output word
    mov     r7, #0              @ reset accumulator
    mov     r8, #0
    b       .bitunp_bit_loop

.bitunp_flush:
    @ Write remaining partial word if any bits accumulated
    cmp     r8, #0
    strne   r7, [r1]

    ldmfd   sp!, {r4-r10, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ Signed fixed-point division for affine functions
@ r0 = numerator (signed), r1 = divisor (signed)
@ Returns: r0 = quotient (signed)
@ ============================================================================
.affine_divide:
    stmfd   sp!, {r4, lr}
    mov     r4, #0              @ sign flag
    cmp     r0, #0
    rsblt   r0, r0, #0
    eorlt   r4, r4, #1
    cmp     r1, #0
    rsblt   r1, r1, #0
    eorlt   r4, r4, #1

    cmp     r1, #0
    moveq   r0, #0
    beq     .affdiv_done

    @ Unsigned division
    mov     r2, #0              @ quotient
    mov     r3, #1
.affdiv_shift:
    cmp     r1, r0
    bhi     .affdiv_loop
    tst     r1, #0x80000000
    bne     .affdiv_loop
    mov     r1, r1, lsl #1
    mov     r3, r3, lsl #1
    b       .affdiv_shift
.affdiv_loop:
    cmp     r3, #0
    beq     .affdiv_end
    cmp     r0, r1
    subcs   r0, r0, r1
    addcs   r2, r2, r3
    mov     r1, r1, lsr #1
    mov     r3, r3, lsr #1
    b       .affdiv_loop
.affdiv_end:
    mov     r0, r2
.affdiv_done:
    cmp     r4, #0
    rsbne   r0, r0, #0
    ldmfd   sp!, {r4, lr}
    bx      lr

@ ============================================================================
@ SWI 0x11: LZ77UnCompWram
@ Decompresses LZ77-encoded data with byte writes (WRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ Header[4:7]=1, Header[8:31]=decompressed size
@ Flag byte per 8 blocks (MSB first): 0=literal, 1=compressed
@ Compressed: 2 bytes → (count-3)<<12 | displacement, copy from dest-disp-1
@ Reference: GBATek "SWI 11h"
@ ============================================================================
swi_lz77_wram:
    stmfd   sp!, {r4-r7, lr}
    ldr     r3, [r0], #4        @ header
    mov     r3, r3, lsr #8      @ decompressed size
    add     r3, r3, r1          @ r3 = dest end address
    mov     r4, r1              @ r4 = dest write pointer
.lz77w_flag:
    cmp     r4, r3
    bge     .lz77w_done
    ldrb    r5, [r0], #1        @ flag byte
    mov     r6, #0x80           @ bit mask (MSB = first block)
.lz77w_block:
    cmp     r6, #0
    beq     .lz77w_flag
    cmp     r4, r3
    bge     .lz77w_done
    tst     r5, r6
    bne     .lz77w_comp
    @ Literal byte
    ldrb    r7, [r0], #1
    strb    r7, [r4], #1
    mov     r6, r6, lsr #1
    b       .lz77w_block
.lz77w_comp:
    @ Compressed: 2-byte reference (count-3 in high nibble, 12-bit displacement)
    ldrb    r7, [r0], #1        @ byte1
    ldrb    r12, [r0], #1       @ byte2
    orr     r12, r12, r7, lsl #8
    mov     r7, r12, lsr #12
    add     r7, r7, #3          @ count
    bic     r12, r12, #0xF000
    add     r12, r12, #1        @ displacement + 1
.lz77w_copy:
    cmp     r7, #0
    ble     .lz77w_copy_end
    cmp     r4, r3
    bge     .lz77w_done
    ldrb    r11, [r4, -r12]     @ read from dest - (disp+1)
    strb    r11, [r4], #1
    sub     r7, r7, #1
    b       .lz77w_copy
.lz77w_copy_end:
    mov     r6, r6, lsr #1
    b       .lz77w_block
.lz77w_done:
    ldmfd   sp!, {r4-r7, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x12: LZ77UnCompVram
@ Same algorithm as 0x11 but buffers output for 16-bit writes (VRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ ============================================================================
swi_lz77_vram:
    stmfd   sp!, {r4-r9, lr}
    mov     r9, r1              @ r9 = dest write pointer (halfword)
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size
    mov     r4, #0              @ logical byte count
    mov     r8, #0              @ halfword buffer
.lz77v_flag:
    cmp     r4, r3
    bge     .lz77v_done
    ldrb    r5, [r0], #1
    mov     r6, #0x80
.lz77v_block:
    cmp     r6, #0
    beq     .lz77v_flag
    cmp     r4, r3
    bge     .lz77v_done
    tst     r5, r6
    bne     .lz77v_comp
    @ Literal
    ldrb    r7, [r0], #1
    tst     r4, #1
    moveq   r8, r7              @ even: store as low byte
    orrne   r8, r8, r7, lsl #8  @ odd: combine as high byte
    strneh  r8, [r9], #2        @ odd: write halfword
    add     r4, r4, #1
    mov     r6, r6, lsr #1
    b       .lz77v_block
.lz77v_comp:
    ldrb    r7, [r0], #1
    ldrb    r11, [r0], #1
    orr     r11, r11, r7, lsl #8
    mov     r7, r11, lsr #12
    add     r7, r7, #3          @ count
    bic     r11, r11, #0xF000
    add     r11, r11, #1        @ disp+1 (stable through copy loop)
.lz77v_copy:
    cmp     r7, #0
    ble     .lz77v_copy_end
    cmp     r4, r3
    bge     .lz77v_done
    sub     r12, r4, r11        @ offset for back-ref
    ldrb    r12, [r1, r12]      @ read from dest base
    tst     r4, #1
    moveq   r8, r12
    orrne   r8, r8, r12, lsl #8
    strneh  r8, [r9], #2
    add     r4, r4, #1
    sub     r7, r7, #1
    b       .lz77v_copy
.lz77v_copy_end:
    mov     r6, r6, lsr #1
    b       .lz77v_block
.lz77v_done:
    ldmfd   sp!, {r4-r9, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x13: HuffUnComp
@ Huffman decompression. r0 = source (32-bit aligned), r1 = destination.
@ Header[0:3] = bits per symbol (4 or 8)
@ Tree stored as byte nodes, bitstream as 32-bit words (MSB first).
@ Output accumulated into 32-bit words, written to destination.
@ Reference: GBATek "SWI 13h"
@ ============================================================================
swi_huffman:
    stmfd   sp!, {r4-r10, lr}
    mov     r9, r1              @ r9 = dest write pointer
    ldr     r3, [r0], #4        @ header
    and     r4, r3, #0x0F       @ r4 = bits per symbol (4 or 8)
    mov     r3, r3, lsr #8      @ r3 = decompressed size in bytes
    ldrb    r1, [r0], #1        @ tree_size_byte
    mov     r6, r0              @ r6 = tree root address
    add     r0, r0, r1, lsl #1
    add     r0, r0, #1          @ past tree table
    add     r0, r0, #3
    bic     r0, r0, #3          @ r0 = bitstream start (word-aligned)
    mov     r7, #0              @ output word accumulator
    mov     r8, #0              @ output bit shift
    mov     r10, #0             @ bytes written
    mov     r12, #0             @ bits remaining in current word
.huff_next:
    cmp     r10, r3
    bge     .huff_flush
    mov     r5, r6              @ r5 = current node (start at root)
.huff_trav:
    cmp     r12, #0
    bne     .huff_have_bit
    ldr     r11, [r0], #4       @ load next bitstream word
    mov     r12, #32
.huff_have_bit:
    sub     r12, r12, #1
    ldrb    r2, [r5]            @ current node byte
    and     r1, r2, #0x3F       @ offset field
    bic     r5, r5, #1          @ nodeAddr & ~1
    add     r5, r5, r1, lsl #1  @ + offset*2
    add     r5, r5, #2          @ r5 = child0 address
    @ Extract direction from bitstream (MSB first)
    movs    r11, r11, lsl #1    @ MSB → carry
    bcc     .huff_left
    @ Went right: child1 = child0 + 1
    add     r5, r5, #1
    tst     r2, #0x40           @ bit6: right child is leaf?
    beq     .huff_trav           @ not leaf, continue traversal
    b       .huff_leaf
.huff_left:
    tst     r2, #0x80           @ bit7: left child is leaf?
    beq     .huff_trav
.huff_leaf:
    ldrb    r1, [r5]            @ read data from leaf node
    orr     r7, r7, r1, lsl r8  @ accumulate into output word
    add     r8, r8, r4          @ advance by bits_per_symbol
    cmp     r8, #32
    blt     .huff_next
    @ Full word ready
    str     r7, [r9], #4
    add     r10, r10, #4
    mov     r7, #0
    mov     r8, #0
    b       .huff_next
.huff_flush:
    cmp     r8, #0
    strne   r7, [r9]            @ write partial word if any
    ldmfd   sp!, {r4-r10, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x14: RLUnCompWram
@ Run-length decompression with byte writes (WRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ Header[4:7]=3, Header[8:31]=decompressed size
@ Flag byte: bit7=compressed (repeat N+3), bit7=0 (copy N+1 literals)
@ Reference: GBATek "SWI 14h"
@ ============================================================================
swi_rle_wram:
    stmfd   sp!, {r4-r5, lr}
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size
    add     r3, r3, r1          @ r3 = dest end
    mov     r4, r1              @ r4 = dest write pointer
.rlew_loop:
    cmp     r4, r3
    bge     .rlew_done
    ldrb    r5, [r0], #1        @ flag byte
    tst     r5, #0x80
    bne     .rlew_comp
    @ Uncompressed: copy N+1 literal bytes
    and     r5, r5, #0x7F
    add     r5, r5, #1
.rlew_lit:
    cmp     r4, r3
    bge     .rlew_done
    ldrb    r12, [r0], #1
    strb    r12, [r4], #1
    subs    r5, r5, #1
    bgt     .rlew_lit
    b       .rlew_loop
.rlew_comp:
    @ Compressed: repeat byte N+3 times
    and     r5, r5, #0x7F
    add     r5, r5, #3
    ldrb    r12, [r0], #1
.rlew_fill:
    cmp     r4, r3
    bge     .rlew_done
    strb    r12, [r4], #1
    subs    r5, r5, #1
    bgt     .rlew_fill
    b       .rlew_loop
.rlew_done:
    ldmfd   sp!, {r4-r5, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x15: RLUnCompVram
@ Same as 0x14 but buffers output for 16-bit writes (VRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ ============================================================================
swi_rle_vram:
    stmfd   sp!, {r4-r8, lr}
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size
    mov     r4, #0              @ byte count
    mov     r7, #0              @ halfword buffer
    mov     r8, r1              @ dest write pointer
.rlev_loop:
    cmp     r4, r3
    bge     .rlev_done
    ldrb    r5, [r0], #1
    tst     r5, #0x80
    bne     .rlev_comp
    and     r5, r5, #0x7F
    add     r5, r5, #1
.rlev_lit:
    cmp     r4, r3
    bge     .rlev_done
    ldrb    r12, [r0], #1
    tst     r4, #1
    moveq   r7, r12
    orrne   r7, r7, r12, lsl #8
    strneh  r7, [r8], #2
    add     r4, r4, #1
    subs    r5, r5, #1
    bgt     .rlev_lit
    b       .rlev_loop
.rlev_comp:
    and     r5, r5, #0x7F
    add     r5, r5, #3
    ldrb    r12, [r0], #1
.rlev_fill:
    cmp     r4, r3
    bge     .rlev_done
    tst     r4, #1
    moveq   r7, r12
    orrne   r7, r7, r12, lsl #8
    strneh  r7, [r8], #2
    add     r4, r4, #1
    subs    r5, r5, #1
    bgt     .rlev_fill
    b       .rlev_loop
.rlev_done:
    ldmfd   sp!, {r4-r8, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x16: Diff8bitUnFilterWram
@ Cumulative 8-bit delta decoder with byte writes (WRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ Header[0:3]=1, Header[4:7]=8, Header[8:31]=decompressed size
@ First byte absolute, subsequent bytes are signed 8-bit deltas.
@ Reference: GBATek "SWI 16h"
@ ============================================================================
swi_diff8_wram:
    stmfd   sp!, {r4, lr}
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size
    mov     r4, #0              @ running sum
.diff8w_loop:
    cmp     r3, #0
    ble     .diff8w_done
    ldrb    r12, [r0], #1
    add     r4, r4, r12
    and     r4, r4, #0xFF       @ wrap to 8 bits
    strb    r4, [r1], #1
    subs    r3, r3, #1
    b       .diff8w_loop
.diff8w_done:
    ldmfd   sp!, {r4, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x17: Diff8bitUnFilterVram
@ Same as 0x16 but buffers output for 16-bit writes (VRAM safe).
@ r0 = source (32-bit aligned), r1 = destination
@ ============================================================================
swi_diff8_vram:
    stmfd   sp!, {r4-r6, lr}
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size
    mov     r4, #0              @ running sum
    mov     r5, #0              @ halfword buffer
    mov     r6, #0              @ byte count
.diff8v_loop:
    cmp     r6, r3
    bge     .diff8v_done
    ldrb    r12, [r0], #1
    add     r4, r4, r12
    and     r4, r4, #0xFF
    tst     r6, #1
    moveq   r5, r4              @ even: low byte
    orrne   r5, r5, r4, lsl #8  @ odd: high byte
    strneh  r5, [r1], #2        @ odd: write halfword
    add     r6, r6, #1
    b       .diff8v_loop
.diff8v_done:
    ldmfd   sp!, {r4-r6, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x18: Diff16bitUnFilter
@ Cumulative 16-bit delta decoder with halfword writes.
@ r0 = source (32-bit aligned), r1 = destination
@ Header[0:3]=2, Header[4:7]=8, Header[8:31]=decompressed size
@ First halfword absolute, subsequent halfwords are signed 16-bit deltas.
@ Reference: GBATek "SWI 18h"
@ ============================================================================
swi_diff16:
    stmfd   sp!, {r4, lr}
    ldr     r3, [r0], #4
    mov     r3, r3, lsr #8      @ decompressed size in bytes
    mov     r4, #0              @ running sum
.diff16_loop:
    cmp     r3, #0
    ble     .diff16_done
    ldrh    r12, [r0], #2
    add     r4, r4, r12
    mov     r4, r4, lsl #16
    mov     r4, r4, lsr #16     @ wrap to 16 bits
    strh    r4, [r1], #2
    sub     r3, r3, #2
    b       .diff16_loop
.diff16_done:
    ldmfd   sp!, {r4, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

.pool

@ ============================================================================
@ 256-entry sine lookup table (s1.14 fixed-point)
@ sin(i * 2π / 256) * 16384, for i = 0..255
@ Used by BgAffineSet and ObjAffineSet
@ ============================================================================
.align 2
sine_lut:
    .short 0x0000, 0x0192, 0x0324, 0x04b5, 0x0646, 0x07d6, 0x0964, 0x0af1
    .short 0x0c7c, 0x0e06, 0x0f8d, 0x1112, 0x1294, 0x1413, 0x1590, 0x1709
    .short 0x187e, 0x19ef, 0x1b5d, 0x1cc6, 0x1e2b, 0x1f8c, 0x20e7, 0x223d
    .short 0x238e, 0x24da, 0x2620, 0x2760, 0x289a, 0x29ce, 0x2afb, 0x2c21
    .short 0x2d41, 0x2e5a, 0x2f6c, 0x3076, 0x3179, 0x3274, 0x3368, 0x3453
    .short 0x3537, 0x3612, 0x36e5, 0x37b0, 0x3871, 0x392b, 0x39db, 0x3a82
    .short 0x3b21, 0x3bb6, 0x3c42, 0x3cc5, 0x3d3f, 0x3daf, 0x3e15, 0x3e72
    .short 0x3ec5, 0x3f0f, 0x3f4f, 0x3f85, 0x3fb1, 0x3fd4, 0x3fec, 0x3ffb
    .short 0x4000, 0x3ffb, 0x3fec, 0x3fd4, 0x3fb1, 0x3f85, 0x3f4f, 0x3f0f
    .short 0x3ec5, 0x3e72, 0x3e15, 0x3daf, 0x3d3f, 0x3cc5, 0x3c42, 0x3bb6
    .short 0x3b21, 0x3a82, 0x39db, 0x392b, 0x3871, 0x37b0, 0x36e5, 0x3612
    .short 0x3537, 0x3453, 0x3368, 0x3274, 0x3179, 0x3076, 0x2f6c, 0x2e5a
    .short 0x2d41, 0x2c21, 0x2afb, 0x29ce, 0x289a, 0x2760, 0x2620, 0x24da
    .short 0x238e, 0x223d, 0x20e7, 0x1f8c, 0x1e2b, 0x1cc6, 0x1b5d, 0x19ef
    .short 0x187e, 0x1709, 0x1590, 0x1413, 0x1294, 0x1112, 0x0f8d, 0x0e06
    .short 0x0c7c, 0x0af1, 0x0964, 0x07d6, 0x0646, 0x04b5, 0x0324, 0x0192
    .short 0x0000, 0xfe6e, 0xfcdc, 0xfb4b, 0xf9ba, 0xf82a, 0xf69c, 0xf50f
    .short 0xf384, 0xf1fa, 0xf073, 0xeeee, 0xed6c, 0xebed, 0xea70, 0xe8f7
    .short 0xe782, 0xe611, 0xe4a3, 0xe33a, 0xe1d5, 0xe074, 0xdf19, 0xddc3
    .short 0xdc72, 0xdb26, 0xd9e0, 0xd8a0, 0xd766, 0xd632, 0xd505, 0xd3df
    .short 0xd2bf, 0xd1a6, 0xd094, 0xcf8a, 0xce87, 0xcd8c, 0xcc98, 0xcbad
    .short 0xcac9, 0xc9ee, 0xc91b, 0xc850, 0xc78f, 0xc6d5, 0xc625, 0xc57e
    .short 0xc4df, 0xc44a, 0xc3be, 0xc33b, 0xc2c1, 0xc251, 0xc1eb, 0xc18e
    .short 0xc13b, 0xc0f1, 0xc0b1, 0xc07b, 0xc04f, 0xc02c, 0xc014, 0xc005
    .short 0xc000, 0xc005, 0xc014, 0xc02c, 0xc04f, 0xc07b, 0xc0b1, 0xc0f1
    .short 0xc13b, 0xc18e, 0xc1eb, 0xc251, 0xc2c1, 0xc33b, 0xc3be, 0xc44a
    .short 0xc4df, 0xc57e, 0xc625, 0xc6d5, 0xc78f, 0xc850, 0xc91b, 0xc9ee
    .short 0xcac9, 0xcbad, 0xcc98, 0xcd8c, 0xce87, 0xcf8a, 0xd094, 0xd1a6
    .short 0xd2bf, 0xd3df, 0xd505, 0xd632, 0xd766, 0xd8a0, 0xd9e0, 0xdb26
    .short 0xdc72, 0xddc3, 0xdf19, 0xe074, 0xe1d5, 0xe33a, 0xe4a3, 0xe611
    .short 0xe782, 0xe8f7, 0xea70, 0xebed, 0xed6c, 0xeeee, 0xf073, 0xf1fa
    .short 0xf384, 0xf50f, 0xf69c, 0xf82a, 0xf9ba, 0xfb4b, 0xfcdc, 0xfe6e

@ ============================================================================
@ SWI 0x19: SoundBias
@ Steps SOUNDBIAS (0x04000088) toward target value with delay.
@ r0 = 0 → target 0x000, r0 != 0 → target 0x200
@ Steps bias level by 1 per iteration with delay loop to avoid pops.
@ ============================================================================
swi_sound_bias:
    stmfd   sp!, {r0-r5, lr}

    @ Determine target: r0==0 → 0x000, else 0x200
    cmp     r0, #0
    moveq   r2, #0              @ target = 0x000
    movne   r2, #0x200          @ target = 0x200

    ldr     r3, =0x04000088     @ SOUNDBIAS address
    ldrh    r4, [r3]            @ current SOUNDBIAS value
    @ Isolate upper bits (10-15) by clearing bits 0-9
    @ Use two-step mask: clear with 0xFF, then clear bit 8-9
    mov     r5, r4, lsr #10     @ shift upper bits down
    mov     r5, r5, lsl #10     @ r5 = preserved upper bits
    sub     r4, r4, r5          @ r4 = current bias level (bits 0-9)

.sb_loop:
    cmp     r4, r2
    beq     .sb_done
    bgt     .sb_dec
    add     r4, r4, #1          @ step up
    b       .sb_write
.sb_dec:
    sub     r4, r4, #1          @ step down
.sb_write:
    orr     r0, r4, r5          @ merge bias level with preserved upper bits
    strh    r0, [r3]            @ write new SOUNDBIAS
    @ Small delay between steps
    mov     r5, #0x10
.sb_delay:
    subs    r5, r5, #1
    bne     .sb_delay
    b       .sb_loop

.sb_done:
    ldmfd   sp!, {r0-r5, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ SWI 0x1F: MidiKey2Freq
@ Converts MIDI key + fine-pitch to playback frequency.
@ r0 = pointer to WaveData struct (freq at offset +8)
@ r1 = MIDI key (mk), r2 = fine pitch (fp, 0-255)
@ Returns r0 = freq / 2^((180 - mk - fp/256) / 12)
@
@ Integer-only implementation using a 12-entry LUT for 2^(n/12) scaled
@ by 2^16. Minor pitch rounding compared to official BIOS floating-point.
@ ============================================================================
swi_midi_key2freq:
    stmfd   sp!, {r1-r8, lr}

    @ Load base frequency from WaveData struct (offset +8)
    ldr     r3, [r0, #8]        @ r3 = wa->freq

    @ Calculate total semitone offset: 180*256 - mk*256 - fp
    @ This gives us the offset in 1/256th semitone units
    mov     r4, #180
    sub     r4, r4, r1           @ r4 = 180 - mk
    mov     r4, r4, lsl #8      @ r4 = (180 - mk) * 256
    sub     r4, r4, r2           @ r4 = (180 - mk) * 256 - fp

    @ If offset <= 0, result = freq (no division needed)
    cmp     r4, #0
    ble     .mk2f_no_shift

    @ Divide offset by (12*256=3072) to get whole octaves
    @ r5 = whole octaves, r6 = remainder in 1/256th semitone units
    mov     r5, #0              @ octave counter
    ldr     r6, =3072           @ 12 * 256
.mk2f_oct_loop:
    cmp     r4, r6
    blt     .mk2f_oct_done
    sub     r4, r4, r6
    add     r5, r5, #1
    b       .mk2f_oct_loop
.mk2f_oct_done:
    @ r5 = whole octaves to shift down
    @ r4 = remaining offset in 1/256th semitone units (0..3071)

    @ Shift freq right by whole octaves
    mov     r3, r3, lsr r5      @ r3 = freq >> octaves

    @ For the fractional part, use LUT for 2^(n/12) scaled by 2^16
    @ r4 = remaining 1/256th semitone units
    @ Convert to semitone index: r4 / 256
    mov     r7, r4, lsr #8      @ r7 = whole semitones (0..11)

    @ Look up divisor from table: table[r7] is 2^(r7/12) * 65536
    adr     r8, .mk2f_lut
    ldr     r6, [r8, r7, lsl #2] @ r6 = lut[semitone]

    @ result = (freq << 16) / lut_value
    @ Since freq is already shifted down by octaves, freq<<16 should fit
    mov     r4, r3, lsl #16     @ r4 = freq << 16

    cmp     r6, #0
    moveq   r0, r3              @ avoid division by zero
    beq     .mk2f_done

    @ Unsigned division: r4 / r6 → r0
    mov     r0, #0
    mov     r8, #1
.mk2f_div_align:
    cmp     r6, r4
    bhs     .mk2f_div_loop
    cmp     r6, #0x80000000
    bhs     .mk2f_div_loop
    mov     r6, r6, lsl #1
    mov     r8, r8, lsl #1
    b       .mk2f_div_align
.mk2f_div_loop:
    cmp     r4, r6
    subhs   r4, r4, r6
    addhs   r0, r0, r8
    movs    r8, r8, lsr #1
    movne   r6, r6, lsr #1
    bne     .mk2f_div_loop

    b       .mk2f_done

.mk2f_no_shift:
    mov     r0, r3              @ result = freq unchanged

.mk2f_done:
    ldmfd   sp!, {r1-r8, lr}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ 2^(n/12) * 65536 lookup table for n = 0..11
.mk2f_lut:
    .word   65536               @ 2^(0/12)  = 1.0000 * 65536
    .word   69433               @ 2^(1/12)  = 1.0595 * 65536
    .word   73562               @ 2^(2/12)  = 1.1225 * 65536
    .word   77936               @ 2^(3/12)  = 1.1892 * 65536
    .word   82570               @ 2^(4/12)  = 1.2599 * 65536
    .word   87480               @ 2^(5/12)  = 1.3348 * 65536
    .word   92682               @ 2^(6/12)  = 1.4142 * 65536
    .word   98193               @ 2^(7/12)  = 1.4983 * 65536
    .word   104032              @ 2^(8/12)  = 1.5874 * 65536
    .word   110218              @ 2^(9/12)  = 1.6818 * 65536
    .word   116772              @ 2^(10/12) = 1.7818 * 65536
    .word   123715              @ 2^(11/12) = 1.8877 * 65536

@ ============================================================================
@ SWI 0x25: MultiBoot
@ Multiplayer boot transfer — not supported in this BIOS.
@ Returns r0 = 1 to indicate failure.
@ ============================================================================
swi_multiboot:
    mov     r0, #1              @ return failure
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ IRQ Handler
@ Reads the user IRQ handler address from 0x03FFFFFC (mirror of 0x03007FFC),
@ saves context, calls the handler, and returns.
@ ============================================================================
irq_handler:
    @ Match the official BIOS IRQ trampoline documented by GBATek.
    stmfd   sp!, {r0-r3, r12, lr}
    mov     r0, #0x04000000
    add     lr, pc, #0
    ldr     pc, [r0, #-4]
    ldmfd   sp!, {r0-r3, r12, lr}
    subs    pc, lr, #4

@ ============================================================================
@ Literal pool
@ ============================================================================
.pool

@ ============================================================================
@ Logo bitmap data for "NESER" text (Mode 4, palette index 1)
@ Format: repeated (halfword vram_offset, halfword pixel_count) pairs.
@ Each pair draws pixel_count pixels of palette entry 1 starting at
@ VRAM base + vram_offset. Generated from a 5x7 bitmap font at 3x scale.
@ Text is centered on the 240x160 display.
@ ============================================================================
.align 2
logo_data:
    .hword 16630, 3
    .hword 16642, 3
    .hword 16651, 15
    .hword 16675, 9
    .hword 16693, 15
    .hword 16714, 12
    .hword 16870, 3
    .hword 16882, 3
    .hword 16891, 15
    .hword 16915, 9
    .hword 16933, 15
    .hword 16954, 12
    .hword 17110, 3
    .hword 17122, 3
    .hword 17131, 15
    .hword 17155, 9
    .hword 17173, 15
    .hword 17194, 12
    .hword 17350, 3
    .hword 17362, 3
    .hword 17371, 3
    .hword 17392, 3
    .hword 17404, 3
    .hword 17413, 3
    .hword 17434, 3
    .hword 17446, 3
    .hword 17590, 3
    .hword 17602, 3
    .hword 17611, 3
    .hword 17632, 3
    .hword 17644, 3
    .hword 17653, 3
    .hword 17674, 3
    .hword 17686, 3
    .hword 17830, 3
    .hword 17842, 3
    .hword 17851, 3
    .hword 17872, 3
    .hword 17884, 3
    .hword 17893, 3
    .hword 17914, 3
    .hword 17926, 3
    .hword 18070, 6
    .hword 18082, 3
    .hword 18091, 3
    .hword 18112, 3
    .hword 18133, 3
    .hword 18154, 3
    .hword 18166, 3
    .hword 18310, 6
    .hword 18322, 3
    .hword 18331, 3
    .hword 18352, 3
    .hword 18373, 3
    .hword 18394, 3
    .hword 18406, 3
    .hword 18550, 6
    .hword 18562, 3
    .hword 18571, 3
    .hword 18592, 3
    .hword 18613, 3
    .hword 18634, 3
    .hword 18646, 3
    .hword 18790, 3
    .hword 18796, 3
    .hword 18802, 3
    .hword 18811, 12
    .hword 18835, 9
    .hword 18853, 12
    .hword 18874, 12
    .hword 19030, 3
    .hword 19036, 3
    .hword 19042, 3
    .hword 19051, 12
    .hword 19075, 9
    .hword 19093, 12
    .hword 19114, 12
    .hword 19270, 3
    .hword 19276, 3
    .hword 19282, 3
    .hword 19291, 12
    .hword 19315, 9
    .hword 19333, 12
    .hword 19354, 12
    .hword 19510, 3
    .hword 19519, 6
    .hword 19531, 3
    .hword 19564, 3
    .hword 19573, 3
    .hword 19594, 3
    .hword 19600, 3
    .hword 19750, 3
    .hword 19759, 6
    .hword 19771, 3
    .hword 19804, 3
    .hword 19813, 3
    .hword 19834, 3
    .hword 19840, 3
    .hword 19990, 3
    .hword 19999, 6
    .hword 20011, 3
    .hword 20044, 3
    .hword 20053, 3
    .hword 20074, 3
    .hword 20080, 3
    .hword 20230, 3
    .hword 20242, 3
    .hword 20251, 3
    .hword 20272, 3
    .hword 20284, 3
    .hword 20293, 3
    .hword 20314, 3
    .hword 20323, 3
    .hword 20470, 3
    .hword 20482, 3
    .hword 20491, 3
    .hword 20512, 3
    .hword 20524, 3
    .hword 20533, 3
    .hword 20554, 3
    .hword 20563, 3
    .hword 20710, 3
    .hword 20722, 3
    .hword 20731, 3
    .hword 20752, 3
    .hword 20764, 3
    .hword 20773, 3
    .hword 20794, 3
    .hword 20803, 3
    .hword 20950, 3
    .hword 20962, 3
    .hword 20971, 15
    .hword 20995, 9
    .hword 21013, 15
    .hword 21034, 3
    .hword 21046, 3
    .hword 21190, 3
    .hword 21202, 3
    .hword 21211, 15
    .hword 21235, 9
    .hword 21253, 15
    .hword 21274, 3
    .hword 21286, 3
    .hword 21430, 3
    .hword 21442, 3
    .hword 21451, 15
    .hword 21475, 9
    .hword 21493, 15
    .hword 21514, 3
    .hword 21526, 3
logo_data_end:
@ Total: 141 spans, 423 bytes
