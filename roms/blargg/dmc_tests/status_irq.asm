; status_irq.asm
; Disassembly + commentary for: roms/blargg/dmc_tests/status_irq.nes
; Mapper: NROM-128 (16KB PRG), PRG mapped at $C000-$FFFF
;
; Vectors (from $FFFA..$FFFF):
;   NMI   = $E11E
;   RESET = $E0C0
;   IRQ   = $E12E
;
; High-level purpose of this ROM (as inferred from code):
; - Program the DMC with IRQ enabled ($4010 bit 7 = 1)
; - Start DMC playback
; - Wait long enough for "DMC IRQ at end-of-sample" to fire
; - If IRQ fires, IRQ handler disables DMC IRQ and silences DAC
; - Then enter an infinite "beep" loop (audible terminal state)
;
; Notes on registers used:
; - $2000 PPUCTRL: bit 7 is NMI enable; writing 0 disables PPU NMI.
; - $2001 PPUMASK: writing 0 disables rendering.
; - $4015 APU status: bit 4 enables DMC, bit 0 enables pulse 1.
; - $4010 DMC control: bit 7 = IRQ enable, bit 6 = loop, bits 3..0 = rate.
; - $4011 DMC DAC: sets the 7-bit output level.
; - $4012 DMC sample address (base $C000 + value*$40).
; - $4013 DMC sample length (bytes = value*$10 + 1).
;
; If you assemble this, you’ll probably want an assembler that supports
; `.org $E000` / `.org $FFFA`, or you can treat this as “annotated listing”.

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

        ; Program DMC registers from the small table at $E11F.
        ; In this ROM, $4010 is written with $80 => DMC IRQ enabled.
E0D6:   LDA $E11F
E0D9:   STA $4010           ; DMC control (IRQ enable / loop / rate)

E0DC:   LDA $E120
E0DF:   STA $4012           ; DMC sample address

E0E2:   LDA $E121
E0E5:   STA $4013           ; DMC sample length

E0E8:   CLI                 ; Enable IRQs (so DMC can interrupt)

        ; Set an initial DAC level (audible click / baseline).
E0E9:   LDA #$20
E0EB:   STA $4011

        ; Delay a bit before starting the DMC.
E0EE:   LDA #$FA
E0F0:   JSR $E104           ; Delay routine

        ; Start DMC playback with IRQ enabled, then wait a bit.
E0F3:   JSR $E122

        ; Delay some more.
E0F6:   LDA #$FA
E0F8:   JSR $E104

        ; Stop interrupts and reset DAC to a known level.
E0FB:   SEI
E0FC:   LDA #$20
E0FE:   STA $4011

        ; Jump to terminal beep loop.
E101:   JMP $E139


        ; ----------------------------
        ; Delay routine ($E104)
        ; ----------------------------
        ; Called with A = outer loop count (e.g. #$FA).
        ; Inner loop uses A = #$12.
        ; Uses ADC tricks to decrement A with predictable timing.
DELAY:
E104:   PHA                 ; Save outer counter
E105:   LDA #$12            ; Inner counter
E107:   SEC
E108:   NOP
E109:   NOP
E10A:   ADC #$FE            ; With SEC set: A := A + $FE + 1 = A - 1
E10C:   BNE $E108           ; Inner loop until A == 0

E10E:   PLA                 ; Restore outer counter
E10F:   CLC
E110:   ADC #$FF            ; A := A - 1
E112:   BNE $E104           ; Outer loop
E114:   RTS


        ; ----------------------------
        ; NMI handler ($E11E)
        ; ----------------------------
NMI:
E11E:   RTI                 ; NMI does nothing


        ; ----------------------------
        ; DMC config table ($E11F)
        ; ----------------------------
        ; The reset code copies these into $4010/$4012/$4013.
        ;
        ; $E11F -> $4010 = $80:
        ;   bit 7 = 1 => DMC IRQ enabled
        ;   bit 6 = 0 => loop disabled
        ;   bits 3..0 = 0 => rate index 0
        ;
        ; $E120 -> $4012 = $81:
        ;   sample address = $C000 + ($81 * $40) = $E040
        ;
        ; $E121 -> $4013 = $01:
        ;   sample length bytes = $01 * $10 + 1 = $11 bytes (17 bytes)
DMC_TABLE:
E11F:   .byte $80
E120:   .byte $81
E121:   .byte $01


        ; ----------------------------
        ; DMC sample payload ($E040)
        ; ----------------------------
        ; With $4012=$81 and $4013=$01, the DMC reads $11 bytes (17 bytes)
        ; starting at $E040. In this ROM that payload is simply $55 repeated.
DMC_SAMPLE_E040:
E040:   .byte $55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55,$55
E050:   .byte $55


        ; ----------------------------
        ; Start DMC and wait ($E122)
        ; ----------------------------
        ; This routine enables DMC and then burns time.
        ; If the emulator is correct, DMC should generate an IRQ at end-of-sample
        ; (because $4010 bit 7 was set).
START_DMC_AND_WAIT:
E122:   CLI                 ; Ensure IRQs are enabled (redundant but explicit)
E123:   LDA #$10
E125:   STA $4015           ; Enable DMC channel (bit 4)

        ; Give it time to reach end-of-sample and assert IRQ.
E128:   LDA #$FA
E12A:   JSR $E104
E12D:   RTS


        ; ----------------------------
        ; IRQ handler ($E12E)
        ; ----------------------------
        ; Expected to run on DMC end-of-sample IRQ.
        ; It disables DMC IRQ and silences DAC.
IRQ:
E12E:   PHA
E12F:   LDA #$00
E131:   STA $4010           ; Disable DMC IRQ (and rate=0, loop=0)
E134:   STA $4011           ; Silence DAC output
E137:   PLA
E138:   RTI


        ; ----------------------------
        ; Terminal beep loop ($E139)
        ; ----------------------------
        ; Sets up pulse channel 1 and loops forever.
BEEP_FOREVER:
E139:   SEI
E13A:   LDA #$00
E13C:   STA $2000           ; PPUCTRL = 0 (disable NMI)
E13F:   STA $2001           ; PPUMASK = 0 (disable rendering)

        ; Configure Pulse 1 (registers $4000-$4003)
E142:   LDA #$82
E144:   STA $4000           ; Duty/envelope/volume (constant volume-ish)

E147:   LDA #$01
E149:   STA $4002           ; Timer low
E14C:   STA $4015           ; Enable pulse 1 (bit 0)

E14F:   LDA #$09
E151:   STA $4003           ; Timer high + length reload => starts tone

E154:   JMP $E154           ; Infinite loop
