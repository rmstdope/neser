; nes20_header.inc — NES 2.0 header generation
;
; Required defines (set in mapper defs or via -D):
;   MAPPER_NUM      — Mapper number (0-4095)
;   SUBMAPPER_NUM   — Submapper number (0-15)
;   PRG_ROM_16K     — PRG ROM size in 16KB units
;   CHR_ROM_8K      — CHR ROM size in 8KB units (0 for CHR-RAM)
;   MIRRORING       — 0=horizontal, 1=vertical
;   HAS_BATTERY     — 0 or 1
;   PRG_RAM_SHIFT   — PRG-RAM size shift count (0=none, 7=8KB, 8=16KB, 10=64KB)
;   CHR_RAM_SHIFT   — CHR-RAM size shift count (0=none, 7=8KB)
;   TV_SYSTEM       — 0=NTSC, 1=PAL, 2=Multi, 3=Dendy

.ifndef MIRRORING
    MIRRORING = 0
.endif
.ifndef HAS_BATTERY
    HAS_BATTERY = 0
.endif
.ifndef HAS_FOUR_SCREEN_VRAM
    HAS_FOUR_SCREEN_VRAM = 0
.endif
.ifndef PRG_RAM_SHIFT
    PRG_RAM_SHIFT = 0
.endif
.ifndef CHR_RAM_SHIFT
    CHR_RAM_SHIFT = 0
.endif
.ifndef TV_SYSTEM
    TV_SYSTEM = 0
.endif

.macro nes20_header
    .segment "HEADER"

    ; Bytes 0-3: Magic number
    .byte "NES", $1A

    ; Byte 4: PRG ROM size LSB (in 16KB units)
    .byte PRG_ROM_16K

    ; Byte 5: CHR ROM size LSB (in 8KB units)
    .byte CHR_ROM_8K

    ; Byte 6: Flags 6
    ;   bit 0: mirroring (0=H, 1=V)
    ;   bit 1: battery-backed PRG-RAM
    ;   bit 2: trainer (0=no)
    ;   bit 3: four-screen VRAM (0=no)
    ;   bits 4-7: mapper low nibble
    .byte ((MAPPER_NUM & $0F) << 4) | (HAS_FOUR_SCREEN_VRAM << 3) | (HAS_BATTERY << 1) | MIRRORING

    ; Byte 7: Flags 7 — NES 2.0 identifier
    ;   bits 0-1: console type (0=NES)
    ;   bits 2-3: NES 2.0 identifier ($08 = NES 2.0)
    ;   bits 4-7: mapper bits 4-7
    .byte (MAPPER_NUM & $F0) | $08

    ; Byte 8: Mapper MSB / Submapper
    ;   bits 0-3: mapper bits 8-11
    ;   bits 4-7: submapper
    .byte ((SUBMAPPER_NUM & $0F) << 4) | ((MAPPER_NUM >> 8) & $0F)

    ; Byte 9: PRG/CHR ROM size MSB
    .byte 0

    ; Byte 10: PRG-RAM / PRG-NVRAM size
    ;   bits 0-3: PRG-RAM (volatile) shift count
    ;   bits 4-7: PRG-NVRAM (battery) shift count
    .if HAS_BATTERY
        .byte (PRG_RAM_SHIFT << 4)
    .else
        .byte PRG_RAM_SHIFT
    .endif

    ; Byte 11: CHR-RAM / CHR-NVRAM size
    .byte CHR_RAM_SHIFT

    ; Byte 12: CPU/PPU timing mode
    .byte TV_SYSTEM

    ; Bytes 13-15: Reserved / unused
    .byte 0, 0, 0
.endmacro
