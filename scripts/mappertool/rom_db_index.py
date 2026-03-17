"""Lookup index for rom_db.csv."""

from pathlib import Path

from .rom_db_entry import RomDbEntry


class RomDbIndex:
    """Lookup index for src/cartridge/rom_db.csv."""

    def __init__(self, by_crc: dict[str, RomDbEntry]) -> None:
        self._by_crc = by_crc

    @classmethod
    def from_csv(cls, csv_path: Path) -> "RomDbIndex":
        """Load ROM DB rows, skipping comment lines."""

        column_count = 21

        def normalize_columns(line: str) -> list[str]:
            columns = line.split(",")
            if len(columns) <= column_count:
                return columns
            tail_start = len(columns) - (column_count - 2)
            normalized = [columns[0], ",".join(columns[1:tail_start])]
            normalized.extend(columns[tail_start:])
            return normalized

        by_crc: dict[str, RomDbEntry] = {}
        with csv_path.open("r", encoding="utf-8", newline="") as handle:
            for raw_line in handle:
                stripped = raw_line.strip()
                if not stripped or stripped.startswith("#"):
                    continue

                row = normalize_columns(stripped)
                if len(row) < 9:
                    continue

                crc_key = row[3].strip().upper()
                if not crc_key:
                    continue

                by_crc[crc_key] = RomDbEntry(
                    rom_id=row[0].strip(),
                    name=row[1].strip(),
                    crc=crc_key,
                    mapper=row[6].strip(),
                    submapper=row[7].strip(),
                )

        return cls(by_crc)

    @staticmethod
    def _crc_key(crc: int | str) -> str:
        if isinstance(crc, int):
            return f"{crc:08X}"
        return crc.strip().upper()

    def lookup(self, crc: int | str) -> RomDbEntry | None:
        """Find a ROM DB entry by CRC value."""

        return self._by_crc.get(self._crc_key(crc))

    @property
    def size(self) -> int:
        """Return number of indexed rows."""

        return len(self._by_crc)
