"""Unit tests for scraper CLI helpers in main.py."""

import unittest

from scripts.nes_rom_db_scraper.main import _csv_cell, _filter_present_fields


class TestMainHelpers(unittest.TestCase):
    """Tests for row filtering used by list output."""

    def test_filter_present_fields_keeps_zero_and_drops_none(self):
        row = {
            "prg_ram_size": 0,
            "chr_ram_size": None,
            "name": "Example",
        }
        filtered = _filter_present_fields(row)
        self.assertEqual(filtered["prg_ram_size"], 0)
        self.assertNotIn("chr_ram_size", filtered)
        self.assertEqual(filtered["name"], "Example")

    def test_csv_cell_keeps_zero(self):
        self.assertEqual(_csv_cell(0), "0")

    def test_csv_cell_blanks_only_none(self):
        self.assertEqual(_csv_cell(None), "")


if __name__ == "__main__":
    unittest.main()
