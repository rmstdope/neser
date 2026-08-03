"""Unit tests for RomFileDatabase scanning and persistence."""

import tempfile
import unittest
from pathlib import Path

from .rom_db_index import RomDbIndex
from .rom_file_database import RomFileDatabase
from .test_helpers import crc_from_rom_bytes, make_ines_rom


class RomFileDatabaseTests(unittest.TestCase):
    """Behavior tests for scan/update and invalid handling."""

    def test_scan_persists_new_rom_records(self) -> None:
        """Scan appends discovered ROMs and writes rom_files.csv."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=1))

            rom_files_db_path = temp_root / "rom_files.csv"
            db = RomFileDatabase(rom_files_db_path)

            records, new_records, updated_records, invalid_marked, warnings, was_cancelled = db.scan_and_update(
                rom_root,
                RomDbIndex({}),
            )

            self.assertTrue(rom_files_db_path.exists())
            self.assertIn("sample.nes", records)
            self.assertEqual(len(new_records), 1)
            self.assertEqual(updated_records, 0)
            self.assertEqual(invalid_marked, 0)
            self.assertEqual(warnings, [])
            self.assertFalse(was_cancelled)

    def test_scan_applies_rom_db_override_to_new_record(self) -> None:
        """Scan uses rom_db.csv CRC override for mapper/submapper values."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            rom_bytes = make_ines_rom(mapper=1, submapper=None, fill_prg=0x12, fill_chr=0x34)
            (rom_root / "override-me.nes").write_bytes(rom_bytes)

            crc = crc_from_rom_bytes(rom_bytes)
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text(
                f"1,Override Test,,{crc},0,Licensed Japan,66,3,H,0,0,0,0,0,0,0,0,0,,,1\n",
                encoding="utf-8",
            )

            db = RomFileDatabase(temp_root / "rom_files.csv")
            index = RomDbIndex.from_csv(rom_db_path)
            records, *_ = db.scan_and_update(rom_root, index)

            record = records.get("override-me.nes")
            self.assertIsNotNone(record)
            assert record is not None
            self.assertEqual(record.mapper, "66")
            self.assertEqual(record.submapper, "3")
            self.assertEqual(record.mapper_source, "rom_db_override")

    def test_scan_marks_record_when_corresponding_autorun_file_exists(self) -> None:
        """Scan marks records with existing sibling .autorun files."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=1))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

            db = RomFileDatabase(temp_root / "rom_files.csv")
            records, *_ = db.scan_and_update(rom_root, RomDbIndex({}))

            record = records.get("sample.nes")
            self.assertIsNotNone(record)
            assert record is not None
            self.assertTrue(record.has_autorun)
            self.assertEqual(record.autorun_status, "not_run")

    def test_scan_reconciles_existing_row_to_override(self) -> None:
        """Existing rows are updated when rom_db overrides become available."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            rom_bytes = make_ines_rom(mapper=1, submapper=None, fill_prg=0x12, fill_chr=0x34)
            (rom_root / "existing.nes").write_bytes(rom_bytes)

            crc = crc_from_rom_bytes(rom_bytes)
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text(
                f"1,Override Existing,,{crc},0,Licensed Japan,9,4,H,0,0,0,0,0,0,0,0,0,,,1\n",
                encoding="utf-8",
            )

            rom_files_db_path = temp_root / "rom_files.csv"
            rom_files_db_path.write_text(
                "rom_path,crc,header_mapper,header_submapper,mapper,submapper,mapper_source\n"
                f"existing.nes,{crc},1,,1,,header\n",
                encoding="utf-8",
            )

            db = RomFileDatabase(rom_files_db_path)
            index = RomDbIndex.from_csv(rom_db_path)
            records, new_records, updated_records, invalid_marked, warnings, was_cancelled = db.scan_and_update(
                rom_root,
                index,
            )

            record = records.get("existing.nes")
            self.assertIsNotNone(record)
            assert record is not None
            self.assertEqual(record.mapper, "9")
            self.assertEqual(record.submapper, "4")
            self.assertEqual(record.mapper_source, "rom_db_override")
            self.assertEqual(record.autorun_status, "na")
            self.assertEqual(len(new_records), 0)
            self.assertEqual(updated_records, 1)
            self.assertEqual(invalid_marked, 0)
            self.assertEqual(warnings, [])
            self.assertFalse(was_cancelled)

    def test_load_legacy_rows_default_autorun_status_from_has_autorun(self) -> None:
        """Legacy CSV rows without autorun_status get sane defaults when loaded."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            db_path = temp_root / "rom_files.csv"
            db_path.write_text(
                "rom_path,crc,header_mapper,header_submapper,mapper,submapper,mapper_source,has_autorun,is_valid,parse_error\n"
                "with_autorun.nes,AAAA0001,1,,1,,header,1,1,\n"
                "without_autorun.nes,BBBB0002,1,,1,,header,0,1,\n",
                encoding="utf-8",
            )

            records = RomFileDatabase(db_path).load()

            self.assertEqual(records["with_autorun.nes"].autorun_status, "not_run")
            self.assertEqual(records["without_autorun.nes"].autorun_status, "na")

    def test_invalid_rom_is_persisted_and_skipped_on_next_scan(self) -> None:
        """Invalid ROMs are stored as invalid entries and not reparsed later."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "broken.nes").write_bytes(b"not-a-valid-ines")

            db = RomFileDatabase(temp_root / "rom_files.csv")
            records_first, _, _, invalid_marked_first, warnings_first, cancelled_first = db.scan_and_update(
                rom_root,
                RomDbIndex({}),
            )

            first_record = records_first.get("broken.nes")
            self.assertIsNotNone(first_record)
            assert first_record is not None
            self.assertFalse(first_record.is_valid)
            self.assertEqual(first_record.mapper_source, "invalid")
            self.assertEqual(invalid_marked_first, 1)
            self.assertTrue(any("Marked invalid ROM" in warning for warning in warnings_first))
            self.assertFalse(cancelled_first)

            (
                records_second,
                _,
                updated_second,
                invalid_marked_second,
                warnings_second,
                cancelled_second,
            ) = db.scan_and_update(
                rom_root,
                RomDbIndex({}),
            )
            second_record = records_second.get("broken.nes")
            self.assertIsNotNone(second_record)
            assert second_record is not None
            self.assertFalse(second_record.is_valid)
            self.assertEqual(updated_second, 0)
            self.assertEqual(invalid_marked_second, 0)
            self.assertEqual(warnings_second, [])
            self.assertFalse(cancelled_second)

    def test_scan_honors_cancellation_callback(self) -> None:
        """Scan stops early and reports cancellation when callback requests it."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "a.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "b.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            db = RomFileDatabase(temp_root / "rom_files.csv")

            call_count = 0

            def should_cancel() -> bool:
                nonlocal call_count
                call_count += 1
                return call_count >= 2

            records, _, _, _, _, was_cancelled = db.scan_and_update(
                rom_root,
                RomDbIndex({}),
                should_cancel=should_cancel,
            )

            self.assertTrue(was_cancelled)
            self.assertLessEqual(len(records), 1)
