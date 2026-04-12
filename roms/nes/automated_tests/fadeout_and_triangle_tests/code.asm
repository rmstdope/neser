; Disassembly of PRG-ROM for fadeout_and_triangle_test.nes
; PRG ROM banks: 2 (size=32768 bytes)
; CHR ROM banks: 0
; Mapper: 0
; Mirroring: horizontal
; Vectors: NMI=$FFB5 RESET=$FF80 IRQ=$FFB8
;
; Triangle channel notes:
; - $4015 bit 2 enables triangle output
; - $4008: bit7=control (length counter halt/linear control), bits6-0=linear counter reload
; - $400A/$400B: 11-bit timer; frequency = CPU/(32*(timer+1))
; - $400B bits7-3 set length counter load
;
RESET:
	SEI              ; Disable IRQs during init to avoid audio timing skew.
	CLD              ; Clear decimal mode (safety on 6502).
VBL:
	LDA $2002        ; Read PPUSTATUS (clears vblank flag).
	BPL VBL          ; Wait for vblank to become set.
	LDA $2002        ; Read PPUSTATUS again to sync a second vblank.
	BPL $ff87        ; Wait for second vblank (ensures PPU ready).
	LDX #$00         ; X=0 for RAM clear loop.
	TXA              ; A=0 to write zeros.
LOOP:
	STA $0200,X      ; Clear page $0200-$02FF (OAM shadow/zeroed state).
	INX              ; Next byte.
	BNE LOOP         ; Loop until X wraps to 0.
	LDA #$0f         ; APU enable register value.
	STA $4015        ; Disable all APU channels (bits 0-4 clear).
	LDA #$0a         ; DMC control value (IRQ off, rate set).
	STA $4010        ; DMC control (prevents DMC IRQ/noise).
	LDA #$00         ; Prepare A=0 for state init.
	LDX #$00         ; X=0 init.
	LDY #$00         ; Y=0 init.
	JSR $8008        ; Jump to main test setup (APU triangle config).
	LDA #$80         ; Enable NMI (bit 7), nametable select 0.
	STA $2000        ; PPUCTRL: NMI on to drive per-frame updates.
	LDA #$00         ; Disable rendering.
	STA $2001        ; PPUMASK: rendering off (audio-only test).
INF:
	JMP INF          ; Idle loop; audio work happens in NMI.
NMI:
	JSR $800b        ; Per-frame audio step (triangle fade/phase logic).
IRQ:
	RTI              ; No IRQ handling needed.




