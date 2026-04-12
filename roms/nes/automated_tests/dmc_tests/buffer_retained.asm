; buffer_retained.asm
; Disassembly + commentary for: roms/blargg/dmc_tests/buffer_retained.nes
; Mapper: NROM-128 (16KB PRG), PRG mapped at $C000-$FFFF
;
; Vectors (from $FFFA..$FFFF):
;   NMI   = $E11E
;   RESET = $E0C0
;   IRQ   = $E12D
;
; High-level intent (from code structure):
; - Configure DMC (IRQ disabled) with a tiny sample at $E040
; - After a delay, briefly enable DMC ($4015 bit 4) and then immediately disable all APU ($4015=0)
; - Then enter a terminal “beep forever” loop
;
; This ROM is typically used to probe whether the emulator/hardware “retains” some DMC state
; (e.g., output / sample buffer effects) across quick enable->disable sequences.
; There is no explicit pass/fail branch in the ROM; it relies on emulator-observable behavior
; (audio waveform, trace markers, etc.).
;
; APU registers used:
; - $4010 DMC control: bit 7 IRQ enable, bit 6 loop, bits 3..0 rate.
; - $4011 DMC DAC: sets output level.
; - $4012/$4013: sample address/length.
; - $4015 APU status: bit 4 enables DMC, bit 0 enables pulse 1.

        ; ----------------------------
        ; RESET entrypoint ($E0C0)
        ; ----------------------------
RESET:
E0C0:   SEI                 ; Disable IRQs during init

E0C1:   LDA #$00
E0C3:   STA $2000           ; PPUCTRL = 0 (disable NMI)

E0C6:   LDX #$FF
E0C8:   TXS                 ; Stack pointer = $FF

E0C9:   LDA #$40
E0CB:   STA $4017           ; APU frame counter: disable frame IRQ
E0CE:   LDA $4017           ; Read-back (often used as a small sync/delay)

E0D1:   LDA #$00
E0D3:   STA $4015           ; Disable all APU channels (including DMC)

        ; Configure DMC playback parameters from a small table.
        ; Here $4010 is written with $00 => DMC IRQ disabled, loop disabled, rate index 0.
E0D6:   LDA $E11F
E0D9:   STA $4010           ; DMC control
E0DC:   LDA $E120
E0DF:   STA $4012           ; DMC sample address
E0E2:   LDA $E121
E0E5:   STA $4013           ; DMC sample length

E0E8:   CLI                 ; Enable IRQs (though IRQ/NMI handlers do nothing)

        ; Initialize DAC level.
E0E9:   LDA #$20
E0EB:   STA $4011

        ; Delay a while.
E0EE:   LDA #$FA
E0F0:   JSR $E104

        ; Briefly enable DMC and then disable it immediately.
E0F3:   JSR $E122

        ; Delay again.
E0F6:   LDA #$FA
E0F8:   JSR $E104

        ; Enter terminal beep loop.
E0FB:   SEI
E0FC:   LDA #$20
E0FE:   STA $4011
E101:   JMP $E12E


        ; ----------------------------
        ; Delay routine ($E104)
        ; ----------------------------
        ; Called with A = outer loop count.
DELAY:
E104:   PHA
E105:   LDA #$12
E107:   SEC
E108:   NOP
E109:   NOP
E10A:   ADC #$FE            ; With SEC set: A := A - 1
E10C:   BNE $E108
E10E:   PLA
E10F:   CLC
E110:   ADC #$FF            ; A := A - 1
E112:   BNE $E104
E114:   RTS

        ; Tiny odd delay loop (not used by RESET path).
        ; NOTE: 6502 has no BIT #imm; bytes here are `24 00` => BIT $00.
E115:   CLC
E116:   BIT $00             ; If assembling, you may prefer: `.byte $24, $00`
E118:   NOP
E119:   ADC #$FE
E11B:   BNE $E116
E11D:   RTS


        ; ----------------------------
        ; NMI handler ($E11E)
        ; ----------------------------
NMI:
E11E:   RTI


        ; ----------------------------
        ; DMC config table ($E11F)
        ; ----------------------------
        ; $E11F -> $4010 = $00:
        ;   IRQ disabled, loop disabled, rate index 0
        ; $E120 -> $4012 = $81:
        ;   sample address = $C000 + ($81 * $40) = $E040
        ; $E121 -> $4013 = $01:
        ;   sample length bytes = $01 * $10 + 1 = $11 bytes (17 bytes)
DMC_TABLE:
E11F:   .byte $00
E120:   .byte $81
E121:   .byte $01


        ; ----------------------------
        ; DMC sample payload ($E040)
        ; ----------------------------
        ; With $4012=$81 and $4013=$01, the DMC reads $11 bytes (17 bytes)
        ; starting at $E040. In this ROM the payload is simply $55 repeated.
DMC_SAMPLE_E040:
E040:   .byte $55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55
E050:   .byte $55


        ; ----------------------------
        ; Brief DMC enable/disable ($E122)
        ; ----------------------------
        ; Enable DMC, then immediately disable all channels.
        ; This is the key behavior under test for “buffer retained”.
TRIGGER_DMC:
E122:   LDA #$10
E124:   STA $4015           ; Enable DMC
E127:   LDA #$00
E129:   STA $4015           ; Disable all channels (DMC off)
E12C:   RTS


        ; ----------------------------
        ; IRQ handler ($E12D)
        ; ----------------------------
IRQ:
E12D:   RTI


        ; ----------------------------
        ; Terminal beep loop ($E12E)
        ; ----------------------------
        ; Configures pulse 1, then enters a BRK/RTI loop.
        ; The ROM area from $E149 onward is filled with $00, so the CPU executes BRK repeatedly.
        ; BRK vectors through IRQ ($E12D), which just RTIs back to the next BRK.
BEEP_FOREVER:
E12E:   SEI
E12F:   LDA #$00
E131:   STA $2000           ; PPUCTRL = 0
E134:   STA $2001           ; PPUMASK = 0

E137:   LDA #$82
E139:   STA $4000

E13C:   LDA #$01
E13E:   STA $4002
E141:   STA $4015           ; Enable pulse 1

E144:   LDA #$09
E146:   STA $4003

E149:   BRK                 ; $00 (BRK) repeating forever in the padding region
