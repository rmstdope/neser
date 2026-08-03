"""Sort NES ROM files into mapper/submapper directories.

The script scans a collection directory recursively for `.nes` files, parses
their headers, computes PRG+CHR CRC32, applies `rom_db.csv` mapper/submapper
overrides by CRC when available, and copies files into destination folders.
"""

from __future__ import annotations

import argparse
import shutil
import zlib
from dataclasses import dataclass
from pathlib import Path

__all__ = [
    "calculate_rom_crc32",
    "main",
    "parse_args",
    "parse_ines_header",
    "sort_collection",
]

ROM_DB_COLUMN_COUNT = 22


@dataclass(frozen=True)
class HeaderInfo:
    """Parsed iNES/NES 2.0 header information needed by this sorter."""

    mapper: int
    submapper: int | None
    has_trainer: bool
    prg_size: int
    chr_size: int
    prg_offset: int
    chr_offset: int


def calculate_rom_crc32(prg_rom: bytes, chr_rom: bytes) -> int:
    """Return CRC32 over concatenated PRG and CHR ROM bytes."""

    crc = zlib.crc32(prg_rom)
    return zlib.crc32(chr_rom, crc) & 0xFFFFFFFF


def parse_ines_header(rom_data: bytes) -> HeaderInfo:
    """Parse iNES/NES 2.0 header fields used by this script."""

    if len(rom_data) < 16:
        raise ValueError("File too small for iNES header")

    header = rom_data[:16]
    if header[0:4] != b"NES\x1a":
        raise ValueError("Invalid iNES header magic")

    flags6 = header[6]
    flags7 = header[7]
    nes2 = (flags7 & 0x0C) == 0x08

    if nes2:
        mapper = (flags6 >> 4) | (flags7 & 0xF0) | ((header[8] & 0x0F) << 8)
        submapper = header[8] >> 4
    else:
        mapper = (flags6 >> 4) | (flags7 & 0xF0)
        submapper = None

    has_trainer = (flags6 & 0x04) != 0
    trainer_size = 512 if has_trainer else 0

    prg_size = header[4] * 16 * 1024
    chr_size = header[5] * 8 * 1024
    prg_offset = 16 + trainer_size
    chr_offset = prg_offset + prg_size

    if len(rom_data) < chr_offset + chr_size:
        raise ValueError("File too small for PRG/CHR ROM data")

    return HeaderInfo(
        mapper=mapper,
        submapper=submapper,
        has_trainer=has_trainer,
        prg_size=prg_size,
        chr_size=chr_size,
        prg_offset=prg_offset,
        chr_offset=chr_offset,
    )


def _parse_optional_decimal(value: str) -> int | None:
    """Parse a decimal string or return None when empty."""

    stripped = value.strip()
    if not stripped:
        return None
    return int(stripped, 10)


def _parse_optional_hex(value: str) -> int | None:
    """Parse a hexadecimal string or return None when empty."""

    stripped = value.strip()
    if not stripped:
        return None
    return int(stripped, 16)


def _normalize_columns(line: str) -> list[str]:
    """Normalize rom_db CSV rows where the name field may contain commas."""

    columns = line.split(",")
    if len(columns) <= ROM_DB_COLUMN_COUNT:
        return columns

    tail_start = len(columns) - 20
    normalized = [columns[0], ",".join(columns[1:tail_start])]
    normalized.extend(columns[tail_start:])
    return normalized


def load_rom_db_overrides(csv_path: Path) -> dict[int, tuple[int | None, int | None]]:
    """Load CRC -> (mapper, submapper) overrides from `rom_db.csv`."""

    overrides: dict[int, tuple[int | None, int | None]] = {}
    for raw_line in csv_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue

        columns = _normalize_columns(line)
        if len(columns) < ROM_DB_COLUMN_COUNT:
            columns.extend([""] * (ROM_DB_COLUMN_COUNT - len(columns)))

        crc = _parse_optional_hex(columns[3])
        if crc is None:
            continue

        mapper = _parse_optional_decimal(columns[7])
        submapper = _parse_optional_decimal(columns[8])
        overrides[crc] = (mapper, submapper)

    return overrides


def _resolve_mapper_and_submapper(
    rom_path: Path,
    overrides: dict[int, tuple[int | None, int | None]],
) -> tuple[int, int | None]:
    """Resolve mapper/submapper from header and optional CRC override."""

    data = rom_path.read_bytes()
    header_info = parse_ines_header(data)

    prg_rom = data[header_info.prg_offset : header_info.prg_offset + header_info.prg_size]
    chr_rom = data[header_info.chr_offset : header_info.chr_offset + header_info.chr_size]
    crc = calculate_rom_crc32(prg_rom, chr_rom)

    mapper = header_info.mapper
    submapper = header_info.submapper

    override = overrides.get(crc)
    if override is not None:
        override_mapper, override_submapper = override
        if override_mapper is not None:
            mapper = override_mapper
        if override_submapper is not None:
            submapper = override_submapper

    return mapper, submapper


def _iter_nes_files(collection_root: Path) -> list[Path]:
    """Return all `.nes` files under collection root recursively."""

    return sorted(path for path in collection_root.rglob("*") if path.is_file() and path.suffix.lower() == ".nes")


def _collect_existing_destination_entries(
    destination_root: Path,
) -> tuple[set[Path], set[Path]]:
    """Collect existing destination directories and files as relative paths."""

    directories: set[Path] = set()
    files: set[Path] = set()

    if not destination_root.exists():
        return directories, files

    for entry in destination_root.rglob("*"):
        relative = entry.relative_to(destination_root)
        if entry.is_dir():
            directories.add(relative)
        elif entry.is_file():
            files.add(relative)

    return directories, files


def _print_projected_hierarchy(
    destination_root: Path,
    projected_files: set[Path],
) -> None:
    """Print the destination hierarchy as it would look after dry-run copy."""

    directories, files = _collect_existing_destination_entries(destination_root)

    files.update(projected_files)
    for file_path in files:
        parent = file_path.parent
        while parent != Path("."):
            directories.add(parent)
            parent = parent.parent

    print("Projected destination hierarchy:")
    print(f"{destination_root.name}/")

    for directory in sorted(directories, key=lambda path: (len(path.parts), str(path))):
        print(f"  {directory.as_posix()}/")

    for file_path in sorted(files, key=lambda path: (len(path.parts), str(path))):
        print(f"  {file_path.as_posix()}")


def sort_collection(
    collection_root: Path,
    destination_root: Path,
    rom_db_csv_path: Path,
    *,
    dry_run: bool = False,
) -> int:
    """Copy ROMs into mapper/submapper directories and return count."""

    overrides = load_rom_db_overrides(rom_db_csv_path)
    copied = 0
    projected_files: set[Path] = set()

    for rom_path in _iter_nes_files(collection_root):
        try:
            mapper, submapper = _resolve_mapper_and_submapper(rom_path, overrides)
        except (OSError, ValueError) as error:
            print(f"Skipping invalid ROM {rom_path}: {error}")
            continue

        if submapper is None:
            target_dir = destination_root / str(mapper)
        else:
            target_dir = destination_root / str(mapper) / str(submapper)

        target_path = target_dir / rom_path.name

        if not dry_run:
            target_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy2(rom_path, target_path)
        else:
            projected_files.add(target_path.relative_to(destination_root))
        copied += 1

    if dry_run:
        _print_projected_hierarchy(destination_root, projected_files)

    return copied


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse CLI arguments for source/destination paths and dry-run mode."""

    parser = argparse.ArgumentParser(description="Sort NES ROMs by mapper/submapper")
    parser.add_argument(
        "--collection-root",
        type=Path,
        default=Path("roms/games/collection"),
        help="Directory to recursively scan for .nes files",
    )
    parser.add_argument(
        "--destination-root",
        type=Path,
        default=Path("roms/games/mappers"),
        help="Directory where mapper/submapper folders are created",
    )
    parser.add_argument(
        "--rom-db-csv-path",
        type=Path,
        default=Path("src/cartridge/rom_db.csv"),
        help="Path to rom_db.csv used for CRC-based mapper/submapper overrides",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print summary only, do not copy any files",
    )
    return parser.parse_args(argv)


def _resolve_path(path: Path, repo_root: Path) -> Path:
    """Resolve relative paths against repository root."""

    return path if path.is_absolute() else repo_root / path


def main(argv: list[str] | None = None) -> None:
    """Run the ROM sorting command-line workflow."""

    args = parse_args(argv)
    repo_root = Path(__file__).resolve().parents[1]
    collection_root = _resolve_path(args.collection_root, repo_root)
    destination_root = _resolve_path(args.destination_root, repo_root)
    rom_db_csv_path = _resolve_path(args.rom_db_csv_path, repo_root)

    copied = sort_collection(
        collection_root,
        destination_root,
        rom_db_csv_path,
        dry_run=args.dry_run,
    )
    if args.dry_run:
        print(f"Dry run: would copy {copied} ROM(s)")
    else:
        print(f"Copied {copied} ROM(s)")


if __name__ == "__main__":
    main()
