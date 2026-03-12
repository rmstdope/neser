"""Tracked ROM file record model."""

from dataclasses import dataclass


@dataclass(frozen=True)
class RomFileRecord:
    """Tracked ROM file information used by mappertool inventory."""

    rom_path: str
    crc: str
    header_mapper: str
    header_submapper: str
    mapper: str
    submapper: str
    mapper_source: str
    has_autorun: bool
    is_valid: bool
    parse_error: str
    autorun_status: str
