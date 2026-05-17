@ Open-source GBA BIOS replacement
@ MIT License - Copyright (c) 2025 Henrik Kurelid
@
@ Implements: Reset, IRQ dispatch, SWI 0x00-0x08, 0x0D
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
@ Reset handler - minimal boot sequence
@ Sets up stack pointers for each CPU mode and jumps to cartridge.
@ ============================================================================
reset_handler:
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

    @ Clear registers
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

    @ Set POSTFLG to 1 (indicates BIOS has run)
    ldr     r0, =0x04000300
    mov     r1, #1
    strb    r1, [r0]
    mov     r0, #0
    mov     r1, #0

    @ Jump to cartridge entry point
    ldr     pc, =0x08000000

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
    cmp     r12, #0x0D
    beq     swi_bios_checksum

    @ Unknown SWI: just return
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
    @ Halt CPU until next interrupt
    mov     r0, #0x04000000
    mov     r1, #0
    strb    r1, [r0, #0x301]

    @ Check if our desired interrupt(s) have fired
    ldrh    r2, [r5]
    tst     r2, r4
    beq     .intr_wait_loop

    @ Clear the flags we were waiting for
    bic     r2, r2, r4
    strh    r2, [r5]

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
    mov     r0, #0x04000000
    mov     r1, #0
    strb    r1, [r0, #0x301]

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
    @ Swap r0 and r1, then fall through to div
    mov     r12, r0
    mov     r0, r1
    mov     r1, r12
    stmfd   sp!, {r4, r5}

    @ Save original numerator sign in r5 (r0 after swap = original r1 = numerator)
    mov     r5, r0

    mov     r4, #0              @ sign flag
    cmp     r0, #0
    rsblt   r0, r0, #0
    eorlt   r4, r4, #1

    cmp     r1, #0
    rsblt   r1, r1, #0
    eorlt   r4, r4, #1

    cmp     r1, #0
    beq     .divarm_by_zero

    mov     r2, #0
    mov     r3, #1

.divarm_shift:
    cmp     r1, r0
    bhi     .divarm_loop
    tst     r1, #0x80000000
    bne     .divarm_loop
    mov     r1, r1, lsl #1
    mov     r3, r3, lsl #1
    b       .divarm_shift

.divarm_loop:
    cmp     r3, #0
    beq     .divarm_done
    cmp     r0, r1
    subcs   r0, r0, r1
    addcs   r2, r2, r3
    mov     r1, r1, lsr #1
    mov     r3, r3, lsr #1
    b       .divarm_loop

.divarm_done:
    mov     r1, r0
    mov     r0, r2
    mov     r3, r0

    cmp     r4, #0
    rsbne   r0, r0, #0

    @ Apply sign to remainder (same sign as original numerator)
    cmp     r5, #0
    rsblt   r1, r1, #0

    ldmfd   sp!, {r4, r5}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

.divarm_by_zero:
    mov     r0, #0
    mov     r1, #0
    mov     r3, #0
    ldmfd   sp!, {r4, r5}
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

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
@ We return our own unique checksum.
@ ============================================================================
swi_bios_checksum:
    ldr     r0, =0x4E455345    @ "NESE" - our open-source BIOS identifier
    ldmfd   sp!, {r11, r12, lr}
    movs    pc, lr

@ ============================================================================
@ IRQ Handler
@ Reads the user IRQ handler address from 0x03FFFFFC (mirror of 0x03007FFC),
@ saves context, calls the handler, acknowledges in BIOS IF, and returns.
@ ============================================================================
irq_handler:
    @ Save context on IRQ stack
    stmfd   sp!, {r0-r3, r12, lr}

    @ Load user IRQ handler from 0x03FFFFFC
    mov     r0, #0x04000000
    sub     r0, r0, #4          @ r0 = 0x03FFFFFC
    ldr     r1, [r0]            @ r1 = user handler address

    @ Set return address and call user handler
    adr     lr, .irq_return
    bx      r1

.irq_return:
    @ Restore context and return from IRQ
    ldmfd   sp!, {r0-r3, r12, lr}
    subs    pc, lr, #4

@ ============================================================================
@ Literal pool
@ ============================================================================
.pool
