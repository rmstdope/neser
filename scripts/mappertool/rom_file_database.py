"""Persistent ROM file database and scanner."""

import csv
import sys
from collections.abc import Callable
from pathlib import Path
from typing import ClassVar

try:
    from scripts.sort_roms import calculate_rom_crc32, parse_ines_header
except ModuleNotFoundError:
    repo_root = Path(__file__).resolve().parents[2]
    if str(repo_root) not in sys.path:
        sys.path.insert(0, str(repo_root))
    from scripts.sort_roms import calculate_rom_crc32, parse_ines_header

from .rom_db_index import RomDbIndex
from .rom_file_record import RomFileRecord


class RomFileDatabase:
    """Persistent CSV database for scanned ROM files."""

    HEADER: ClassVar[list[str]] = [
        "rom_path",
        "crc",
        "header_mapper",
        "header_submapper",
        "mapper",
        "submapper",
        "mapper_source",
        "has_autorun",
        "autorun_status",
        "is_valid",
        "parse_error",
    ]

    def __init__(self, csv_path: Path) -> None:
        self.csv_path = csv_path

    def load(self) -> dict[str, RomFileRecord]:
        """Load existing ROM file records keyed by relative path."""

        if not self.csv_path.exists():
            return {}

        records: dict[str, RomFileRecord] = {}
        with self.csv_path.open("r", encoding="utf-8", newline="") as handle:
            reader = csv.DictReader(handle)
            for row in reader:
                rom_path = row.get("rom_path", "").strip()
                if not rom_path:
                    continue
                records[rom_path] = RomFileRecord(
                    rom_path=rom_path,
                    crc=row.get("crc", "").strip(),
                    header_mapper=row.get("header_mapper", "").strip(),
                    header_submapper=row.get("header_submapper", "").strip(),
                    mapper=row.get("mapper", "").strip(),
                    submapper=row.get("submapper", "").strip(),
                    mapper_source=row.get("mapper_source", "").strip() or "header",
                    has_autorun=(row.get("has_autorun", "0").strip() == "1"),
                    is_valid=(row.get("is_valid", "1").strip() != "0"),
                    parse_error=row.get("parse_error", "").strip(),
                    autorun_status=self._normalize_autorun_status(
                        row.get("autorun_status", ""),
                        has_autorun=(row.get("has_autorun", "0").strip() == "1"),
                    ),
                )

        return records

    def save(self, records: dict[str, RomFileRecord]) -> None:
        """Save all ROM file records to CSV."""

        self.csv_path.parent.mkdir(parents=True, exist_ok=True)
        with self.csv_path.open("w", encoding="utf-8", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=self.HEADER)
            writer.writeheader()
            for rom_path in sorted(records):
                record = records[rom_path]
                writer.writerow(
                    {
                        "rom_path": record.rom_path,
                        "crc": record.crc,
                        "header_mapper": record.header_mapper,
                        "header_submapper": record.header_submapper,
                        "mapper": record.mapper,
                        "submapper": record.submapper,
                        "mapper_source": record.mapper_source,
                        "has_autorun": "1" if record.has_autorun else "0",
                        "autorun_status": self._normalize_autorun_status(
                            record.autorun_status,
                            has_autorun=record.has_autorun,
                        ),
                        "is_valid": "1" if record.is_valid else "0",
                        "parse_error": record.parse_error,
                    }
                )

    @staticmethod
    def _normalize_autorun_status(raw_status: str, *, has_autorun: bool) -> str:
        """Normalize persisted autorun status and apply fallback defaults."""

        normalized = raw_status.strip().lower()
        if not has_autorun:
            return "na"
        if normalized in {"not_run", "passed", "failed"}:
            return normalized
        return "not_run"

    def scan_and_update(
        self,
        rom_root: Path,
        rom_db_index: RomDbIndex,
        should_cancel: Callable[[], bool] | None = None,
    ) -> tuple[dict[str, RomFileRecord], list[RomFileRecord], int, int, list[str], bool]:
        """Scan ROMs, append new ones, reconcile existing rows, return diagnostics."""

        records = self.load()
        new_records: list[RomFileRecord] = []
        updated_records = 0
        invalid_marked = 0
        warnings: list[str] = []
        was_cancelled = False

        if not rom_root.exists():
            return (
                records,
                new_records,
                updated_records,
                invalid_marked,
                [f"ROM root not found: {rom_root}"],
                was_cancelled,
            )

        for rom_path in sorted(path for path in rom_root.rglob("*") if path.suffix.lower() == ".nes"):
            if should_cancel is not None and should_cancel():
                was_cancelled = True
                break

            if not rom_path.is_file():
                continue

            relative_path = rom_path.relative_to(rom_root).as_posix()
            has_autorun = rom_path.with_suffix(".autorun").is_file()
            existing = records.get(relative_path)
            if existing is not None and not existing.is_valid:
                continue

            try:
                rom_bytes = rom_path.read_bytes()
                header = parse_ines_header(rom_bytes)
                prg_rom = rom_bytes[header.prg_offset : header.prg_offset + header.prg_size]
                chr_rom = rom_bytes[header.chr_offset : header.chr_offset + header.chr_size]
                crc = calculate_rom_crc32(prg_rom, chr_rom)
            except (OSError, ValueError) as error:
                invalid_record = RomFileRecord(
                    rom_path=relative_path,
                    crc="",
                    header_mapper="",
                    header_submapper="",
                    mapper="",
                    submapper="",
                    mapper_source="invalid",
                    has_autorun=False,
                    is_valid=False,
                    parse_error=str(error),
                    autorun_status="na",
                )
                if existing is None:
                    records[relative_path] = invalid_record
                    invalid_marked += 1
                elif existing != invalid_record:
                    records[relative_path] = invalid_record
                    updated_records += 1
                warnings.append(f"Marked invalid ROM {rom_path}: {error}")
                continue

            header_mapper = header.mapper
            header_submapper = header.submapper
            mapper = header_mapper
            submapper = header_submapper
            mapper_source = "header"

            override = rom_db_index.lookup(crc)
            if override is not None:
                if override.mapper:
                    mapper = int(override.mapper, 10)
                    mapper_source = "rom_db_override"
                if override.submapper:
                    submapper = int(override.submapper, 10)
                    mapper_source = "rom_db_override"

            record = RomFileRecord(
                rom_path=relative_path,
                crc=f"{crc:08X}",
                header_mapper=str(header_mapper),
                header_submapper="" if header_submapper is None else str(header_submapper),
                mapper=str(mapper),
                submapper="" if submapper is None else str(submapper),
                mapper_source=mapper_source,
                has_autorun=has_autorun,
                is_valid=True,
                parse_error="",
                autorun_status=self._normalize_autorun_status(
                    existing.autorun_status if existing is not None else "",
                    has_autorun=has_autorun,
                ),
            )
            if existing is None:
                records[relative_path] = record
                new_records.append(record)
            elif existing != record:
                records[relative_path] = record
                updated_records += 1

        if new_records or updated_records or invalid_marked:
            self.save(records)

        return records, new_records, updated_records, invalid_marked, warnings, was_cancelled
