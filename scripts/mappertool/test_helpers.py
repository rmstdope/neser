"""Shared test helpers for mappertool tests."""

import zlib


def make_ines_rom(
    mapper: int,
    *,
    submapper: int | None = None,
    prg_size: int = 16 * 1024,
    chr_size: int = 8 * 1024,
    fill_prg: int = 0xAA,
    fill_chr: int = 0x55,
) -> bytes:
    """Build a minimal iNES/NES 2.0 ROM image for tests."""

    header = bytearray(16)
    header[0:4] = b"NES\x1a"
    header[4] = prg_size // (16 * 1024)
    header[5] = chr_size // (8 * 1024)

    if submapper is None:
        header[6] = (mapper & 0x0F) << 4
        header[7] = mapper & 0xF0
    else:
        header[6] = (mapper & 0x0F) << 4
        header[7] = (mapper & 0xF0) | 0x08
        header[8] = ((submapper & 0x0F) << 4) | ((mapper >> 8) & 0x0F)

    return bytes(header) + (bytes([fill_prg]) * prg_size) + (bytes([fill_chr]) * chr_size)


def crc_from_rom_bytes(rom_bytes: bytes) -> str:
    """Compute PRG+CHR CRC from a minimal iNES ROM for test fixtures."""

    prg_size = rom_bytes[4] * 16 * 1024
    chr_size = rom_bytes[5] * 8 * 1024
    prg_offset = 16
    chr_offset = prg_offset + prg_size
    crc = zlib.crc32(rom_bytes[prg_offset : prg_offset + prg_size])
    crc = zlib.crc32(rom_bytes[chr_offset : chr_offset + chr_size], crc) & 0xFFFFFFFF
    return f"{crc:08X}"
