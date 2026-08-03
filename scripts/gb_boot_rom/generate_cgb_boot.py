#!/usr/bin/env python3
"""
Generate CGB Boot ROM as Rust byte array.

This script generates the CGB boot ROM implementation in a more maintainable
format than hand-coding each byte. The output can be copied to boot_rom.rs.

Based on SameBoy's open-source CGB boot ROM implementation.
"""


class RomBuilder:
    """
    Helper class to build a boot ROM with address tracking.

    The CGB boot ROM uses split memory mapping:
    - Array indices 0x000-0x0FF → Memory $0000-$00FF
    - Array indices 0x100-0x7FF → Memory $0200-$08FF (offset by $100)

    We track both:
    - `array_idx`: position in the 2048-byte array
    - `mem_addr`: the CPU memory address that maps to that position
    """

    def __init__(self, size: int = 2048):
        self.rom = bytearray(size)
        self.array_idx = 0
        self.mem_addr = 0
        self.labels: dict[str, int] = {}  # Maps label name to memory address

    def org(self, mem_addr: int):
        """Set current memory address and compute array index."""
        self.mem_addr = mem_addr
        if mem_addr < 0x100:
            self.array_idx = mem_addr
        elif mem_addr >= 0x200:
            # $0200 → array[0x100], $0201 → array[0x101], etc.
            self.array_idx = mem_addr - 0x100
        else:
            raise ValueError(f"Invalid boot ROM address: ${mem_addr:04X} (cartridge header region)")

    def label(self, name: str):
        """Define a label at current memory address."""
        self.labels[name] = self.mem_addr

    def emit(self, data: bytes):
        """Emit bytes at current position."""
        for b in data:
            if self.array_idx < len(self.rom):
                self.rom[self.array_idx] = b
            self.array_idx += 1
            self.mem_addr += 1

    def jr_to(self, target_mem_addr: int) -> bytes:
        """Calculate JR offset to target memory address.

        This should be called AFTER emitting the JR opcode byte.
        The offset is relative to PC after the full 2-byte instruction.
        Since we've already emitted the opcode, mem_addr is at the offset byte,
        so PC after instruction = mem_addr + 1.
        """
        pc_after_jr = self.mem_addr + 1  # PC after the 2-byte JR instruction
        offset = target_mem_addr - pc_after_jr
        if offset < -128 or offset > 127:
            raise ValueError(f"JR target out of range: {offset} (from ${self.mem_addr:04X} to ${target_mem_addr:04X})")
        return bytes([offset & 0xFF])


def build_cgb_boot_rom(is_cgb0: bool = False) -> bytes:
    """
    Build the CGB boot ROM.

    Args:
        is_cgb0: If True, skip wave RAM initialization (CGB-0 variant)

    Returns:
        2048-byte boot ROM
    """
    rom = RomBuilder(2048)

    # ═══════════════════════════════════════════════════════════════════════
    # $0000-$00FF: First mapped region (256 bytes)
    # ═══════════════════════════════════════════════════════════════════════

    rom.org(0x0000)

    # ── $0000: Initialize stack pointer ────────────────────────────────────
    rom.emit(bytes([0x31, 0xFE, 0xFF]))  # LD SP, $FFFE

    # ── Clear VRAM ($8000–$9FFF) ───────────────────────────────────────────
    rom.emit(bytes([0x21, 0x00, 0x80]))  # LD HL, $8000
    rom.label("ClearVRAM")
    rom.emit(bytes([0xAF]))  # XOR A
    rom.emit(bytes([0x22]))  # LDI [HL], A
    rom.emit(bytes([0xCB, 0x6C]))  # BIT 5, H (check H >= $A0)
    rom.emit(bytes([0x28]))  # JR Z, ClearVRAM
    rom.emit(rom.jr_to(rom.labels["ClearVRAM"]))

    # ── Clear OAM ($FE00-$FE9F) ────────────────────────────────────────────
    rom.emit(bytes([0x26, 0xFE]))  # LD H, $FE
    rom.emit(bytes([0x0E, 0xA0]))  # LD C, $A0 (160 bytes)
    rom.label("ClearOAM")
    rom.emit(bytes([0x22]))  # LDI [HL], A
    rom.emit(bytes([0x0D]))  # DEC C
    rom.emit(bytes([0x20]))  # JR NZ, ClearOAM
    rom.emit(rom.jr_to(rom.labels["ClearOAM"]))

    if not is_cgb0:
        # ── Initialize wave RAM (Production CGB only) ─────────────────────
        rom.emit(bytes([0x0E, 0x10]))  # LD C, $10 (16 bytes)
        rom.emit(bytes([0x21, 0x30, 0xFF]))  # LD HL, $FF30
        rom.label("InitWaveRAM")
        rom.emit(bytes([0x22]))  # LDI [HL], A
        rom.emit(bytes([0x2F]))  # CPL (toggle 0x00 <-> 0xFF)
        rom.emit(bytes([0x0D]))  # DEC C
        rom.emit(bytes([0x20]))  # JR NZ, InitWaveRAM
        rom.emit(rom.jr_to(rom.labels["InitWaveRAM"]))

    # ── Clear HRAM state ───────────────────────────────────────────────────
    rom.emit(bytes([0xAF]))  # XOR A
    rom.emit(bytes([0xE0, 0x80]))  # LDH [$80], A (hInputPalette)
    rom.emit(bytes([0xE0, 0x81]))  # LDH [$81], A (hTitleChecksum)

    # ── Initialize Audio ───────────────────────────────────────────────────
    rom.emit(bytes([0x3E, 0x80]))  # LD A, $80
    rom.emit(bytes([0xE0, 0x26]))  # LDH [$26], A (NR52 = APU on)
    rom.emit(bytes([0xE0, 0x11]))  # LDH [$11], A (NR11 = 50% duty)
    rom.emit(bytes([0x3E, 0xF3]))  # LD A, $F3
    rom.emit(bytes([0xE0, 0x12]))  # LDH [$12], A (NR12 = envelope)
    rom.emit(bytes([0xE0, 0x25]))  # LDH [$25], A (NR51 = panning)
    rom.emit(bytes([0x3E, 0x77]))  # LD A, $77
    rom.emit(bytes([0xE0, 0x24]))  # LDH [$24], A (NR50 = volume)

    # ── Initialize BG palette ──────────────────────────────────────────────
    rom.emit(bytes([0x3E, 0xFC]))  # LD A, $FC
    rom.emit(bytes([0xE0, 0x47]))  # LDH [$47], A (BGP)

    # ── Load logo from cartridge header ────────────────────────────────────
    rom.emit(bytes([0x11, 0x04, 0x01]))  # LD DE, $0104 (Nintendo logo)
    rom.emit(bytes([0x21, 0x10, 0x80]))  # LD HL, $8010 (VRAM tile 1)
    rom.label("LoadLogoLoop")
    rom.emit(bytes([0x1A]))  # LD A, [DE]
    rom.emit(bytes([0x47]))  # LD B, A
    rom.emit(bytes([0xCD, 0x00, 0x02]))  # CALL DoubleBitsAndWriteRowTwice
    rom.emit(bytes([0x13]))  # INC DE
    rom.emit(bytes([0x7B]))  # LD A, E
    rom.emit(bytes([0xFE, 0x34]))  # CP $34
    rom.emit(bytes([0x20]))  # JR NZ, LoadLogoLoop
    rom.emit(rom.jr_to(rom.labels["LoadLogoLoop"]))

    # ── Initialize CGB BG palettes ─────────────────────────────────────────
    rom.emit(bytes([0x3E, 0x80]))  # LD A, $80 (auto-increment)
    rom.emit(bytes([0xE0, 0x68]))  # LDH [$68], A (BCPS)
    rom.emit(bytes([0x0E, 0x40]))  # LD C, $40 (64 bytes)
    rom.emit(bytes([0x21, 0x80, 0x02]))  # LD HL, $0280 (palette data)
    rom.label("PaletteLoop")
    rom.emit(bytes([0x2A]))  # LDI A, [HL]
    rom.emit(bytes([0xE0, 0x69]))  # LDH [$69], A (BCPD)
    rom.emit(bytes([0x0D]))  # DEC C
    rom.emit(bytes([0x20]))  # JR NZ, PaletteLoop
    rom.emit(rom.jr_to(rom.labels["PaletteLoop"]))

    # ── Enable LCD ─────────────────────────────────────────────────────────
    rom.emit(bytes([0x3E, 0x91]))  # LD A, $91
    rom.emit(bytes([0xE0, 0x40]))  # LDH [$40], A (LCDC)

    # ── Wait and play boot chime ───────────────────────────────────────────
    rom.emit(bytes([0x06, 0x3C]))  # LD B, 60 (frames)
    rom.label("WaitLoop1")
    rom.emit(bytes([0xCD, 0x40, 0x02]))  # CALL WaitFrame
    rom.emit(bytes([0x05]))  # DEC B
    rom.emit(bytes([0x20]))  # JR NZ, WaitLoop1
    rom.emit(rom.jr_to(rom.labels["WaitLoop1"]))

    # Play first tone ($83 = ~988 Hz)
    rom.emit(bytes([0x3E, 0x83]))  # LD A, $83
    rom.emit(bytes([0xE0, 0x13]))  # LDH [$13], A (NR13)
    rom.emit(bytes([0x3E, 0x87]))  # LD A, $87
    rom.emit(bytes([0xE0, 0x14]))  # LDH [$14], A (NR14 trigger)

    rom.emit(bytes([0x06, 0x05]))  # LD B, 5 (frames)
    rom.label("WaitLoop2")
    rom.emit(bytes([0xCD, 0x40, 0x02]))  # CALL WaitFrame
    rom.emit(bytes([0x05]))  # DEC B
    rom.emit(bytes([0x20]))  # JR NZ, WaitLoop2
    rom.emit(rom.jr_to(rom.labels["WaitLoop2"]))

    # Play second tone ($C1 = ~1319 Hz)
    rom.emit(bytes([0x3E, 0xC1]))  # LD A, $C1
    rom.emit(bytes([0xE0, 0x13]))  # LDH [$13], A (NR13)
    rom.emit(bytes([0x3E, 0x87]))  # LD A, $87
    rom.emit(bytes([0xE0, 0x14]))  # LDH [$14], A (NR14 trigger)

    rom.emit(bytes([0x06, 0x1E]))  # LD B, 30 (frames)
    rom.label("WaitLoop3")
    rom.emit(bytes([0xCD, 0x40, 0x02]))  # CALL WaitFrame
    rom.emit(bytes([0x05]))  # DEC B
    rom.emit(bytes([0x20]))  # JR NZ, WaitLoop3
    rom.emit(rom.jr_to(rom.labels["WaitLoop3"]))

    # ── Set final CPU registers ────────────────────────────────────────────
    # A=$11, F=$80, BC=$0000, DE=$0008, HL=$007C
    rom.emit(bytes([0x26, 0x11]))  # LD H, $11
    rom.emit(bytes([0x2E, 0x80]))  # LD L, $80
    rom.emit(bytes([0xE5]))  # PUSH HL
    rom.emit(bytes([0xF1]))  # POP AF (AF = $1180)
    rom.emit(bytes([0x01, 0x00, 0x00]))  # LD BC, $0000
    rom.emit(bytes([0x11, 0x08, 0x00]))  # LD DE, $0008
    rom.emit(bytes([0x21, 0x7C, 0x00]))  # LD HL, $007C

    # Jump to boot exit
    rom.emit(bytes([0xC3, 0xFE, 0x00]))  # JP $00FE

    # ── $00FE: Boot exit ───────────────────────────────────────────────────
    rom.org(0x00FE)
    rom.emit(bytes([0xE0, 0x50]))  # LDH [$50], A (unmap boot ROM)

    # ═══════════════════════════════════════════════════════════════════════
    # $0200-$08FF: Second mapped region (array indices 0x100-0x7FF)
    # ═══════════════════════════════════════════════════════════════════════

    # ── $0200: DoubleBitsAndWriteRowTwice ──────────────────────────────────
    # Memory $0200 → array[0x100]
    # Uses SameBoy's clever trick: CALL .twice, then fall through to .twice
    # This runs the doubling code twice (once via CALL, once via fallthrough)
    rom.org(0x0200)
    rom.emit(bytes([0xCD, 0x03, 0x02]))  # CALL .twice ($0203)
    # .twice at $0203 - IMMEDIATELY after the CALL so we fall through into it:
    rom.emit(bytes([0x3E, 0x04]))  # LD A, 4
    rom.emit(bytes([0x0E, 0x00]))  # LD C, 0
    rom.label("DoubleCurrentBit")
    rom.emit(bytes([0xCB, 0x20]))  # SLA B
    rom.emit(bytes([0xF5]))  # PUSH AF
    rom.emit(bytes([0xCB, 0x11]))  # RL C
    rom.emit(bytes([0xF1]))  # POP AF
    rom.emit(bytes([0xCB, 0x11]))  # RL C
    rom.emit(bytes([0x3D]))  # DEC A
    rom.emit(bytes([0x20]))  # JR NZ, DoubleCurrentBit
    rom.emit(rom.jr_to(rom.labels["DoubleCurrentBit"]))
    rom.emit(bytes([0x79]))  # LD A, C
    rom.emit(bytes([0x22]))  # LDI [HL], A
    rom.emit(bytes([0x23]))  # INC HL
    rom.emit(bytes([0x22]))  # LDI [HL], A
    rom.emit(bytes([0x23]))  # INC HL
    rom.emit(bytes([0xC9]))  # RET

    # ── $0240: WaitFrame ───────────────────────────────────────────────────
    # Memory $0240 → array[0x140]
    rom.org(0x0240)
    rom.emit(bytes([0xE5]))  # PUSH HL
    rom.emit(bytes([0x21, 0x0F, 0xFF]))  # LD HL, $FF0F (IF)
    rom.emit(bytes([0xCB, 0x86]))  # RES 0, [HL] (clear VBlank)
    rom.label("WaitVBlank")
    rom.emit(bytes([0xCB, 0x46]))  # BIT 0, [HL]
    rom.emit(bytes([0x28]))  # JR Z, WaitVBlank
    rom.emit(rom.jr_to(rom.labels["WaitVBlank"]))
    rom.emit(bytes([0xE1]))  # POP HL
    rom.emit(bytes([0xC9]))  # RET

    # ── $0280: Palette data ────────────────────────────────────────────────
    # Memory $0280 → array[0x180]
    # 8 palettes × 4 colors × 2 bytes = 64 bytes
    rom.org(0x0280)
    # Classic grayscale palette for all 8 BG palettes
    palette = bytes(
        [
            0xFF,
            0x7F,  # White ($7FFF)
            0x94,
            0x52,  # Light gray ($5294)
            0x4A,
            0x29,  # Dark gray ($294A)
            0x00,
            0x00,  # Black ($0000)
        ]
    )
    for _ in range(8):
        rom.emit(palette)

    return bytes(rom.rom)


def to_rust_const(data: bytes, name: str, doc: str) -> str:
    """Convert bytes to Rust const array."""
    lines = [doc, f"pub const {name}: [u8; 2048] = ["]

    for i in range(0, len(data), 16):
        chunk = data[i : i + 16]
        hex_bytes = ", ".join(f"0x{b:02X}" for b in chunk)
        # Map array index to memory address
        # First region is $0000-$00FF; the second starts at $0200, so index
        # 0x100 maps to address $0200.
        addr = i if i < 0x100 else i + 0x100
        lines.append(f"    {hex_bytes},  // ${addr:04X}")

    lines.append("];")
    return "\n".join(lines)


CGB_DOC = """/// Full CGB boot ROM with animation and sound.
///
/// This IPR-free implementation provides:
/// - VRAM/OAM clearing
/// - Wave RAM initialization (alternating 0x00/0xFF pattern)
/// - Logo decompression from cartridge header ($0104-$0133)
/// - Boot chime (two tones at ~988 Hz and ~1319 Hz)
/// - CGB palette initialization
/// - Proper CPU register setup for cartridge handoff
///
/// ## Post-boot CPU state
///
/// | Register | Value |
/// |----------|-------|
/// | A        | $11   |
/// | F        | $80 (Z=1, N=0, H=0, C=0) |
/// | B        | $00   |
/// | C        | $00   |
/// | D        | $00   |
/// | E        | $08   |
/// | H        | $00   |
/// | L        | $7C   |
/// | SP       | $FFFE |
/// | PC       | $0100 |
///
/// ## Memory layout
///
/// The CGB boot ROM is 2048 bytes, split into two mapped regions:
/// - $0000-$00FF: Early initialization (256 bytes) → array indices [0..256]
/// - $0100-$01FF: Cartridge header (NOT in boot ROM; reads go to cartridge)
/// - $0200-$08FF: Subroutines and data (1792 bytes) → array indices [256..2048]"""

CGB0_DOC = """/// CGB boot ROM for CGB-0 (first hardware revision).
///
/// CGB-0 is a rare early CGB revision with a slightly different boot ROM.
/// The key difference: CGB-0 does NOT initialize wave RAM ($FF30-$FF3F),
/// leaving it unchanged in its power-on state.
///
/// All other aspects are identical to the Production CGB boot ROM.
///
/// ## Post-boot CPU state (same as Production CGB)
///
/// | Register | Value |
/// |----------|-------|
/// | A        | $11   |
/// | F        | $80 (Z=1, N=0, H=0, C=0) |
/// | B        | $00   |
/// | C        | $00   |
/// | D        | $00   |
/// | E        | $08   |
/// | H        | $00   |
/// | L        | $7C   |
/// | SP       | $FFFE |
/// | PC       | $0100 |"""


if __name__ == "__main__":
    # Generate Production CGB boot ROM
    cgb_rom = build_cgb_boot_rom(is_cgb0=False)
    print(to_rust_const(cgb_rom, "CGB_BOOT_ROM", CGB_DOC))
    print()

    # Generate CGB-0 boot ROM
    cgb0_rom = build_cgb_boot_rom(is_cgb0=True)
    print(to_rust_const(cgb0_rom, "CGB0_BOOT_ROM", CGB0_DOC))
