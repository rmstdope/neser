; latency.asm
; Disassembly + commentary for: roms/blargg/dmc_tests/latency.nes
; Mapper: NROM-128 (16KB PRG), PRG mapped at $C000-$FFFF
;
; Vectors (from $FFFA..$FFFF):
;   NMI   = $E11E
;   RESET = $E0C0
;   IRQ   = $E146
;
; High-level intent (from code structure):
; - Configure DMC (IRQ disabled) and a tiny sample at $E040
; - Run a small loop that toggles DMC on/off and changes DAC ($4011)
; - This is likely a timing/latency probe (e.g., when DAC changes / DMC startup becomes audible)
; - Then enter an infinite “beep forever” terminal loop
;
; Important registers used:
; - $2000 PPUCTRL: writing 0 disables PPU NMI.
; - $2001 PPUMASK: writing 0 disables rendering.
; - $4015 APU status: bit 4 enables DMC, bit 0 enables pulse 1.
; - $4010 DMC control: bit 7 IRQ enable, bit 6 loop, bits 3..0 rate.
; - $4011 DMC DAC: sets output level.
; - $4012/$4013: sample address/length.

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
E0CE:   LDA $4017           ; Read-back (tiny delay / sync)

E0D1:   LDA #$00
E0D3:   STA $4015           ; Disable all APU channels (including DMC)

        ; Configure DMC from table.
        ; In this ROM, $4010 is 00 => IRQ off, loop off, rate index 0.
E0D6:   LDA $E11F
E0D9:   STA $4010
E0DC:   LDA $E120
E0DF:   STA $4012
E0E2:   LDA $E121
E0E5:   STA $4013

E0E8:   CLI                 ; Enable IRQs (though IRQ handler is RTI)

        ; Initialize DAC level.
E0E9:   LDA #$20
E0EB:   STA $4011

        ; Initial delay.
E0EE:   LDA #$FA
E0F0:   JSR $E104

        ; Run the latency/toggling routine.
E0F3:   JSR $E122

        ; Delay again.
E0F6:   LDA #$FA
E0F8:   JSR $E104

        ; Enter terminal beep loop.
E0FB:   SEI
E0FC:   LDA #$20
E0FE:   STA $4011
E101:   JMP $E147


        ; ----------------------------
        ; Delay routine ($E104)
        ; ----------------------------
        ; Called with A = outer loop count.
        ; Inner loop is fixed at A = #$12.
DELAY:
E104:   PHA
E105:   LDA #$12
E107:   SEC
E108:   NOP
E109:   NOP
E10A:   ADC #$FE            ; With SEC: A := A - 1
E10C:   BNE $E108
E10E:   PLA
E10F:   CLC
E110:   ADC #$FF            ; A := A - 1
E112:   BNE $E104
E114:   RTS


        ; ----------------------------
        ; NMI handler ($E11E)
        ; ----------------------------
NMI:
E11E:   RTI


        ; ----------------------------
        ; DMC table ($E11F)
        ; ----------------------------
        ; $E11F -> $4010 = $00:
        ;   IRQ disabled, loop disabled, rate index 0
        ; $E120 -> $4012 = $81:
        ;   sample address = $C000 + ($81 * $40) = $E040
        ; $E121 -> $4013 = $01:
        ;   sample length = $01 * $10 + 1 = $11 bytes (17 bytes)
DMC_TABLE:
E11F:   .byte $00
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
        ; Latency/toggling routine ($E122)
        ; ----------------------------
        ; This loops 4 times. Each iteration:
        ; - sets DAC to $24
        ; - enables DMC ($4015 bit 4)
        ; - waits a short time (A=$16)
        ; - sets DAC back to $20
        ; - disables all APU channels ($4015=$00) (important: DMC off)
        ; - waits a longer time (A=$2B)
        ;
        ; The audible/trace-visible behavior from this is likely used to judge latency.
LATENCY_LOOP:
E122:   LDY #$04            ; Repeat count

E124:   LDA #$24
E126:   STA $4011           ; Set DAC to $24

E129:   LDA #$10
E12B:   STA $4015           ; Enable DMC

E12E:   LDA #$16
E130:   JSR $E104           ; Short delay

E133:   LDA #$20
E135:   STA $4011           ; Set DAC back to $20

E138:   LDA #$00
E13A:   STA $4015           ; Disable all channels (DMC off)

E13D:   LDA #$2B
E13F:   JSR $E104           ; Longer delay

E142:   DEY
E143:   BNE $E124
E145:   RTS


        ; ----------------------------
        ; IRQ handler ($E146)
        ; ----------------------------
IRQ:
E146:   RTI                 ; No IRQ logic here


        ; ----------------------------
        ; Terminal beep loop ($E147)
        ; ----------------------------
BEEP_FOREVER:
E147:   SEI
E148:   LDA #$00
E14A:   STA $2000           ; PPUCTRL = 0
E14D:   STA $2001           ; PPUMASK = 0

        ; Configure pulse 1 for a steady tone and loop forever.
E150:   LDA #$82
E152:   STA $4000
E155:   LDA #$01
E157:   STA $4002
E15A:   STA $4015           ; Enable pulse 1 (bit 0)
E15D:   LDA #$09
E15F:   STA $4003
E162:   JMP $E162
