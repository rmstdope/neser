"""ROM database entry model."""

from dataclasses import dataclass


@dataclass(frozen=True)
class RomDbEntry:
    """Minimal ROM DB entry indexed by full-ROM CRC."""

    rom_id: str
    name: str
    crc: str
    mapper: str
    submapper: str
