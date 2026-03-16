; nes.inc — NES hardware register definitions
; Common to all test ROMs

.ifndef __NES_INC__
__NES_INC__ = 1

; PPU registers
PPUCTRL   = $2000
PPUMASK   = $2001
PPUSTATUS = $2002
OAMADDR   = $2003
OAMDATA   = $2004
PPUSCROLL = $2005
PPUADDR   = $2006
PPUDATA   = $2007

; APU registers
SQ1_VOL   = $4000
SQ1_SWEEP = $4001
SQ1_LO    = $4002
SQ1_HI    = $4003
SQ2_VOL   = $4004
SQ2_SWEEP = $4005
SQ2_LO    = $4006
SQ2_HI    = $4007
TRI_LINEAR = $4008
TRI_LO    = $4009
TRI_HI    = $400A
NOISE_VOL = $400C
NOISE_LO  = $400E
NOISE_HI  = $400F
DMC_FREQ  = $4010
DMC_RAW   = $4011
DMC_START = $4012
DMC_LEN   = $4013
OAMDMA    = $4014
SND_CHN   = $4015
JOY1      = $4016
JOY2      = $4017

; Test status byte address
TEST_STATUS = $6000
TEST_CODE   = $6001    ; Current sub-test number
TEST_TEMP   = $6002    ; Temp storage for test framework
TEST_TEMP2  = $6003    ; Temp storage for test framework

; Status byte values
STATUS_RUNNING = $80
STATUS_PASS    = $00
STATUS_RESET   = $81

.endif ; __NES_INC__
