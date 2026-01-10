        SEI             ; Disable IRQs during init (so timing/logic is deterministic)

        LDA #$00
        STA $2000        ; PPUCTRL = 0 (disable NMI, background pattern, etc.)

        LDX #$FF
        TXS             ; Set stack pointer to $FF (common reset/init pattern)

        LDA #$40
        STA $4017        ; APU frame counter: write $40 disables frame IRQ (and sets 5-step mode)
        LDA $4017        ; Read-back (often used as a small delay / to sync internal state)

        LDA #$00
        STA $4015        ; APU status: disable all channels (DMC, pulse, triangle, noise)

        ; Configure DMC playback parameters.
        ; $4010 = DMC control (irq enable, loop, rate)
        ; $4012 = sample address (in units of $40 bytes, base $C000)
        ; $4013 = sample length (in units of $10 bytes, +1)
        LDA $E11F
        STA $4010
        LDA $E120
        STA $4012
        LDA $E121
        STA $4013

        CLI             ; Enable IRQs again (though this ROM’s IRQ handler is just RTI)

        LDA #$20
        STA $4011        ; DMC DAC output level (sets initial DAC; audible "click" level)

        ; Delay a while (used to space out audible events / give hardware time).
        LDA #$FA
        JSR E104         ; If your assembler requires it, this should be `JSR $E104`

        ; Start DMC and wait for it to finish.
        JSR E122         ; If your assembler requires it, this should be `JSR $E122`

        ; Delay again.
        LDA #$FA
        JSR E104

        SEI             ; Stop interrupts before entering the terminal "beep" loop
        LDA #$20
        STA $4011        ; Reset DAC level again
        JMP $E133        ; Jump into the "beep forever" routine

E104:
        ; Delay routine.
        ; Outer loop count is passed in A by caller (e.g. #$FA).
        ; Inner loop always runs ~0x12 iterations.
        PHA             ; Save outer loop counter (caller-provided A)
        LDA #$12        ; Inner loop counter
        SEC
E108:
        NOP             ; Small, predictable delay
        NOP
        ADC #$FE        ; With SEC set, ADC #$FE adds $FF each time -> A decrements by 1
        BNE $E108       ; Loop until inner counter reaches 0

        PLA             ; Restore outer loop counter
        CLC
        ADC #$FF        ; A := A - 1
        BNE $E104       ; Repeat outer loop until it reaches 0
        RTS

        ; Another tiny delay routine (bytes here are a bit "weird" because of how it was encoded).
        ; NOTE: On the NES CPU (6502/2A03), there is *no* `BIT #imm` instruction.
        ; The original ROM bytes at $E116 are `24 00` which is `BIT $00` (zero-page).
        CLC
E116:
        BIT $00        ; If you want this to assemble as 6502, use: `.byte $24, $00`
        NOP
        ADC #$FE
        BNE $E116
        RTS

NMI:
        RTI             ; NMI vector points here: do nothing and return

E11F:
        ; Small table used above to program the DMC.
        ; $E11F -> $4010 (control): $00 = IRQ disabled, loop disabled, rate index 0
        ; $E120 -> $4012 (address): $81 means sample starts at $C000 + ($81 * $40) = $E040
        ; $E121 -> $4013 (length):  $01 means (1 * $10) + 1 bytes = $11 bytes (17 bytes)
.byte $00
.byte $81
.byte $01

        ; DMC sample payload ($E040)
        ; With $4012=$81 and $4013=$01, the DMC reads $11 bytes (17 bytes)
        ; starting at $E040. In this ROM that payload is simply $55 repeated.
DMC_SAMPLE_E040:
E040:
.byte $55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55
.byte $55

E122:
        ; Start DMC playback.
        LDA #$10
        STA $4015        ; APU status: enable DMC (bit 4)
E127:
        ; Wait for DMC to stop.
        ; BIT sets Z based on (A & M). Here A=$10, so Z=0 while $4015 bit4 is 1.
        BIT $4015
        BNE $E127        ; Loop while DMC active bit (status bit 4) is set

        LDA #$00
        STA $4011        ; Clear DAC level after playback finishes
        RTS

IRQ:
        RTI             ; IRQ vector points here: do nothing and return

E133:
        ; Terminal routine: set up a steady pulse tone and loop forever.
        SEI

        LDA #$00
        STA $2000        ; PPUCTRL = 0 (NMI off)
        STA $2001        ; PPUMASK = 0 (rendering off)

        ; Configure Pulse 1 for an audible beep.
        ; $4000: duty/envelope/volume. $82 sets duty + constant volume (approx).
        ; $4002/$4003: timer low/high; write $4003 also reloads length counter.
        LDA #$82
        STA $4000

        LDA #$01
        STA $4002
        STA $4015        ; Enable pulse 1 (bit 0)

        LDA #$09
        STA $4003        ; Timer high + length reload -> starts the tone
E14E:
        JMP $E14E        ; Infinite loop (this ROM signals end-of-test this way)