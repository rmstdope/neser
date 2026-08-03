"""Unit tests for RomDbIndex."""

import tempfile
import unittest
from pathlib import Path

from .rom_db_index import RomDbIndex


class RomDbIndexTests(unittest.TestCase):
    """Behavior tests for ROM DB CRC lookup parsing."""

    def test_lookup_by_crc(self) -> None:
        """ROM DB loader builds lookup keys from CSV rows."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            csv_path = Path(temp_dir_str) / "rom_db.csv"
            csv_path.write_text(
                "# comment\n1,Test Game,,836C4FA7,0,Licensed Japan,4,2,H,0,0,0,0,0,0,0,0,0,,,1\n",
                encoding="utf-8",
            )

            index = RomDbIndex.from_csv(csv_path)
            entry = index.lookup("836c4fa7")

            self.assertIsNotNone(entry)
            assert entry is not None
            self.assertEqual(entry.name, "Test Game")
            self.assertEqual(entry.mapper, "4")
            self.assertEqual(entry.submapper, "2")

    def test_parses_unquoted_name_with_comma(self) -> None:
        """Parser keeps CRC/mapper columns correct when names contain commas."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            csv_path = Path(temp_dir_str) / "rom_db.csv"
            csv_path.write_text(
                "1,Name, With Comma,,1234ABCD,0,Licensed Japan,7,1,H,0,0,0,0,0,0,0,0,0,,,1\n",
                encoding="utf-8",
            )

            index = RomDbIndex.from_csv(csv_path)
            entry = index.lookup("1234ABCD")

            self.assertIsNotNone(entry)
            assert entry is not None
            self.assertEqual(entry.mapper, "7")
            self.assertEqual(entry.submapper, "1")
