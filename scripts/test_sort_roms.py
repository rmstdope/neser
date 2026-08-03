"""Unit tests for scripts.sort_roms sorting and CLI parsing behavior."""

import io
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from scripts.sort_roms import (
    calculate_rom_crc32,
    parse_args,
    parse_ines_header,
    sort_collection,
)


def make_ines_rom(
    mapper: int,
    *,
    submapper: int | None = None,
    prg_size: int = 16 * 1024,
    chr_size: int = 8 * 1024,
    trainer: bool = False,
    fill_prg: int = 0xAA,
    fill_chr: int = 0x55,
) -> bytes:
    """Build a minimal iNES/NES 2.0 ROM image for tests."""

    header = bytearray(16)
    header[0:4] = b"NES\x1a"
    header[4] = prg_size // (16 * 1024)
    header[5] = chr_size // (8 * 1024)

    if submapper is None:
        header[6] = ((mapper & 0x0F) << 4) | (0x04 if trainer else 0)
        header[7] = mapper & 0xF0
    else:
        header[6] = ((mapper & 0x0F) << 4) | (0x04 if trainer else 0)
        header[7] = (mapper & 0xF0) | 0x08
        header[8] = ((submapper & 0x0F) << 4) | ((mapper >> 8) & 0x0F)

    body = bytearray()
    if trainer:
        body.extend(b"\x00" * 512)
    body.extend(bytes([fill_prg]) * prg_size)
    body.extend(bytes([fill_chr]) * chr_size)
    return bytes(header) + bytes(body)


def make_rom_db_row(*, crc_hex: str, mapper: str = "", submapper: str = "", name: str = "") -> str:
    """Create a rom_db.csv row with optional mapper/submapper overrides."""

    columns = [
        "1",
        name,
        "",
        crc_hex,
        "",
        "",
        "Licensed Japan",
        mapper,
        submapper,
        "H",
        "16384",
        "00000000",
        "",
        "",
        "8192",
        "00000000",
        "",
        "",
        "",
        "",
        "",
        "1",
    ]
    return ",".join(columns)


class TestSortRoms(unittest.TestCase):
    """Behavioral tests for ROM sorting and command-line argument parsing."""

    def test_parse_args_uses_default_paths(self) -> None:
        """Defaults match repository-relative script conventions."""

        args = parse_args([])
        self.assertEqual(args.collection_root, Path("roms/games/collection"))
        self.assertEqual(args.destination_root, Path("roms/games/mappers"))
        self.assertEqual(args.rom_db_csv_path, Path("src/cartridge/rom_db.csv"))
        self.assertFalse(args.dry_run)

    def test_parse_args_supports_custom_paths_and_dry_run(self) -> None:
        """Custom path flags and dry-run flag are parsed correctly."""

        args = parse_args(
            [
                "--collection-root",
                "custom/collection",
                "--destination-root",
                "custom/mappers",
                "--rom-db-csv-path",
                "custom/rom_db.csv",
                "--dry-run",
            ]
        )
        self.assertEqual(args.collection_root, Path("custom/collection"))
        self.assertEqual(args.destination_root, Path("custom/mappers"))
        self.assertEqual(args.rom_db_csv_path, Path("custom/rom_db.csv"))
        self.assertTrue(args.dry_run)

    def test_parse_ines1_header_mapper_without_submapper(self) -> None:
        """iNES 1.0 mapper is parsed and submapper remains unset."""

        rom_bytes = make_ines_rom(mapper=33, submapper=None)
        info = parse_ines_header(rom_bytes)

        self.assertEqual(info.mapper, 33)
        self.assertIsNone(info.submapper)

    def test_parse_nes2_header_includes_submapper(self) -> None:
        """NES 2.0 headers expose both mapper and submapper values."""

        rom_bytes = make_ines_rom(mapper=0x3A5, submapper=2)
        info = parse_ines_header(rom_bytes)

        self.assertEqual(info.mapper, 0x3A5)
        self.assertEqual(info.submapper, 2)

    def test_sort_collection_overrides_header_mapper_and_submapper_from_rom_db(self) -> None:
        """CRC match in DB overrides mapper and submapper destination path."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            collection_root = temp_dir / "roms" / "games" / "collection"
            destination_root = temp_dir / "roms" / "games" / "mappers"
            collection_root.mkdir(parents=True)

            rom_path = collection_root / "Test Game.nes"
            rom_bytes = make_ines_rom(mapper=1, submapper=None, fill_prg=0x12, fill_chr=0x34)
            rom_path.write_bytes(rom_bytes)

            info = parse_ines_header(rom_bytes)
            crc = calculate_rom_crc32(
                rom_bytes[info.prg_offset : info.prg_offset + info.prg_size],
                rom_bytes[info.chr_offset : info.chr_offset + info.chr_size],
            )

            rom_db_path = temp_dir / "rom_db.csv"
            rom_db_path.write_text(
                "# header\n"
                + make_rom_db_row(
                    crc_hex=f"{crc:08X}",
                    mapper="4",
                    submapper="2",
                    name="Name, With Comma",
                )
                + "\n",
                encoding="utf-8",
            )

            copied = sort_collection(collection_root, destination_root, rom_db_path)

            self.assertEqual(copied, 1)
            expected = destination_root / "4" / "2" / rom_path.name
            self.assertTrue(expected.exists())

    def test_sort_collection_without_submapper_copies_to_mapper_folder(self) -> None:
        """Missing submapper copies ROM directly under mapper directory."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            collection_root = temp_dir / "roms" / "games" / "collection"
            destination_root = temp_dir / "roms" / "games" / "mappers"
            collection_root.mkdir(parents=True)

            rom_path = collection_root / "nested" / "No Submapper.nes"
            rom_path.parent.mkdir(parents=True)
            rom_path.write_bytes(make_ines_rom(mapper=7, submapper=None))

            rom_db_path = temp_dir / "rom_db.csv"
            rom_db_path.write_text("# header\n", encoding="utf-8")

            copied = sort_collection(collection_root, destination_root, rom_db_path)

            self.assertEqual(copied, 1)
            expected = destination_root / "7" / rom_path.name
            self.assertTrue(expected.exists())

    def test_sort_collection_dry_run_does_not_copy_files(self) -> None:
        """Dry-run mode reports matches without writing destination files."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            collection_root = temp_dir / "roms" / "games" / "collection"
            destination_root = temp_dir / "roms" / "games" / "mappers"
            collection_root.mkdir(parents=True)

            rom_path = collection_root / "Dry Run.nes"
            rom_path.write_bytes(make_ines_rom(mapper=2, submapper=1))

            rom_db_path = temp_dir / "rom_db.csv"
            rom_db_path.write_text("# header\n", encoding="utf-8")

            copied = sort_collection(
                collection_root,
                destination_root,
                rom_db_path,
                dry_run=True,
            )

            self.assertEqual(copied, 1)
            expected = destination_root / "2" / "1" / rom_path.name
            self.assertFalse(expected.exists())

    def test_sort_collection_dry_run_prints_projected_hierarchy(self) -> None:
        """Dry-run prints the projected destination hierarchy after copy."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            collection_root = temp_dir / "roms" / "games" / "collection"
            destination_root = temp_dir / "roms" / "games" / "mappers"
            collection_root.mkdir(parents=True)

            (collection_root / "A.nes").write_bytes(make_ines_rom(mapper=2, submapper=1))
            nested = collection_root / "nested"
            nested.mkdir(parents=True)
            (nested / "B.nes").write_bytes(make_ines_rom(mapper=7, submapper=None))

            rom_db_path = temp_dir / "rom_db.csv"
            rom_db_path.write_text("# header\n", encoding="utf-8")

            captured = io.StringIO()
            with redirect_stdout(captured):
                copied = sort_collection(
                    collection_root,
                    destination_root,
                    rom_db_path,
                    dry_run=True,
                )

            output = captured.getvalue()
            self.assertEqual(copied, 2)
            self.assertIn("Projected destination hierarchy:", output)
            self.assertIn("mappers/", output)
            self.assertIn("2/", output)
            self.assertIn("1/", output)
            self.assertIn("A.nes", output)
            self.assertIn("7/", output)
            self.assertIn("B.nes", output)

    def test_sort_collection_skips_invalid_rom_and_continues(self) -> None:
        """Malformed ROM files are skipped while valid ROMs are still processed."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            collection_root = temp_dir / "roms" / "games" / "collection"
            destination_root = temp_dir / "roms" / "games" / "mappers"
            collection_root.mkdir(parents=True)

            valid_rom_path = collection_root / "Valid.nes"
            valid_rom_path.write_bytes(make_ines_rom(mapper=3, submapper=None))

            invalid_rom_path = collection_root / "Broken.nes"
            invalid_rom_path.write_bytes(b"NES\x1a")

            rom_db_path = temp_dir / "rom_db.csv"
            rom_db_path.write_text("# header\n", encoding="utf-8")

            copied = sort_collection(collection_root, destination_root, rom_db_path)

            self.assertEqual(copied, 1)
            expected = destination_root / "3" / valid_rom_path.name
            self.assertTrue(expected.exists())


if __name__ == "__main__":
    unittest.main()
