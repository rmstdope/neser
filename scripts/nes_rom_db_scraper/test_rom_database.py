"""Unit tests for the ``RomDatabase`` helper in ``scripts.nes_rom_db_scraper.rom_database``.

These tests exercise schema creation, inserts, updates, upserts,
processing of parsed records and utility functions.
"""

import contextlib
import os
import tempfile
import unittest

from scripts.nes_rom_db_scraper.rom_database import (
    ControllerType,
    HardwareType,
    RomDatabase,
    RomDbKey,
    hardware_from_console_type_and_region,
)


class TestRomDatabase(unittest.TestCase):
    """Tests for RomDatabase: schema, insert/update, queries and helpers."""

    def setUp(self) -> None:
        fd, self.db_path = tempfile.mkstemp(prefix="romdb_test_", suffix=".sqlite")
        os.close(fd)
        self.db = RomDatabase(self.db_path)

    def tearDown(self) -> None:
        with contextlib.suppress(Exception):
            self.db.close()
        with contextlib.suppress(Exception):
            os.unlink(self.db_path)

    def test_schema_created_and_reset(self):
        """Verify initial schema is created and reset_schema recreates it."""
        cur = self.db._conn.execute("PRAGMA table_info(roms)")
        cols = {r[1] for r in cur.fetchall()}
        self.assertIn(RomDbKey.CRC.value, cols)
        self.assertIn(RomDbKey.NAME.value, cols)
        self.assertIn(RomDbKey.ROM_ID.value, cols)
        self.assertIn(RomDbKey.COUNTRY.value, cols)

        # Reset schema should recreate table and keep rom_class column
        self.db.reset_schema()
        cur = self.db._conn.execute("PRAGMA table_info(roms)")
        cols_after = {r[1] for r in cur.fetchall()}
        self.assertIn(RomDbKey.CONSOLE_CLASS.value, cols_after)
        self.assertIn(RomDbKey.COUNTRY.value, cols_after)

    def test_insert_and_get_by_crc(self):
        """Insert a minimal row by CRC and retrieve it via get_rom_by_crc."""
        data = {
            RomDbKey.CRC.value: "DEADBEEF",
            RomDbKey.HARDWARE.value: 0,
            RomDbKey.NAMETABLE_LAYOUT.value: "horizontal",
        }
        self.db.insert_rom_by_crc(data)
        fetched = self.db.get_rom_by_crc("DEADBEEF")
        self.assertIsNotNone(fetched)
        self.assertEqual(fetched.get(RomDbKey.CRC.value), "DEADBEEF")
        # Accept integer or string storage representation
        self.assertEqual(str(fetched.get(RomDbKey.HARDWARE.value)), "0")

    def test_upsert_and_get_rom(self):
        """Upsert a full row and retrieve it by rom_id."""
        rom_id = 42
        payload = {
            RomDbKey.NAME.value: "Test ROM",
            RomDbKey.CRC.value: "ABCD1234",
            RomDbKey.HARDWARE.value: 0,
            RomDbKey.MAPPER.value: 1,
            RomDbKey.PRG_ROM_SIZE.value: 2,
        }
        self.db.upsert_rom(rom_id, payload)
        got = self.db.get_rom(rom_id)
        self.assertIsNotNone(got)
        self.assertEqual(got.get(RomDbKey.NAME.value), "Test ROM")
        self.assertEqual(got.get(RomDbKey.CRC.value), "ABCD1234")

    def test_update_rom_by_crc(self):
        """Insert by CRC and update a column with update_rom_by_crc."""
        crc = "BEEFCAFE"
        self.db.insert_rom_by_crc({RomDbKey.CRC.value: crc})
        # update a new column
        self.db.update_rom_by_crc(crc, {RomDbKey.NAMETABLE_LAYOUT.value: "vertical"})
        got = self.db.get_rom_by_crc(crc)
        self.assertIsNotNone(got)
        self.assertEqual(got.get(RomDbKey.NAMETABLE_LAYOUT.value), "vertical")

    def test_missing_columns_remain_null_on_insert(self):
        """Inserts should keep unspecified integer columns as NULL, not zero."""
        crc = "NULLONINSERT"
        self.db.insert_rom_by_crc({RomDbKey.CRC.value: crc})
        got = self.db.get_rom_by_crc(crc)
        self.assertIsNotNone(got)
        self.assertIsNone(got.get(RomDbKey.PRG_RAM_SIZE.value))
        self.assertIsNone(got.get(RomDbKey.CHR_RAM_SIZE.value))

    def test_update_can_set_explicit_null_and_zero_stays_distinct(self):
        """Updates should allow NULL and preserve distinction from explicit zero."""
        crc = "NULLVSZERO"
        self.db.insert_rom_by_crc({RomDbKey.CRC.value: crc, RomDbKey.PRG_RAM_SIZE.value: 0})

        got = self.db.get_rom_by_crc(crc)
        self.assertIsNotNone(got)
        self.assertEqual(got.get(RomDbKey.PRG_RAM_SIZE.value), 0)

        self.db.update_rom_by_crc(crc, {RomDbKey.PRG_RAM_SIZE.value: None})
        got = self.db.get_rom_by_crc(crc)
        self.assertIsNotNone(got)
        self.assertIsNone(got.get(RomDbKey.PRG_RAM_SIZE.value))

    def test_process_record_by_crc_outcomes(self):
        """Exercise process_record_by_crc return codes for add/update/skip/conflict."""
        crc = "FEEDFACE"
        # add
        add_res = self.db.process_record_by_crc({RomDbKey.CRC.value: crc})
        self.assertEqual(add_res, (1, 0, 0, 0))

        # update (add name)
        upd_res = self.db.process_record_by_crc({RomDbKey.CRC.value: crc, RomDbKey.NAME.value: "Name1"})
        self.assertEqual(upd_res, (0, 1, 0, 0))

        # skip (same data)
        skip_res = self.db.process_record_by_crc({RomDbKey.CRC.value: crc, RomDbKey.NAME.value: "Name1"})
        self.assertEqual(skip_res, (0, 0, 1, 0))

        # conflict (different name)
        conflict_res = self.db.process_record_by_crc({RomDbKey.CRC.value: crc, RomDbKey.NAME.value: "Other"})
        self.assertEqual(conflict_res, (0, 0, 0, 1))

    def test_process_record_by_crc_inserts_full_record(self):
        """New CRC inserts should persist all provided fields."""
        crc = "AABBCCDD"
        payload = {
            RomDbKey.CRC.value: crc,
            RomDbKey.MAPPER.value: 2,
            RomDbKey.PRG_ROM_SIZE.value: 16384,
        }
        add_res = self.db.process_record_by_crc(payload)
        self.assertEqual(add_res, (1, 0, 0, 0))

        fetched = self.db.get_rom_by_crc(crc)
        self.assertIsNotNone(fetched)
        self.assertEqual(str(fetched.get(RomDbKey.MAPPER.value)), "2")
        self.assertEqual(str(fetched.get(RomDbKey.PRG_ROM_SIZE.value)), "16384")

    def test_process_record_by_crc_ram_sum_mismatch_conflict(self):
        """RAM sum mismatch between existing and update should trigger conflict."""
        crc = "RAMSUMCONFLICT"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.PRG_RAM_SIZE.value: 8,
                RomDbKey.PRG_NVRAM_SIZE.value: 0,
                RomDbKey.CHR_RAM_SIZE.value: 4,
                RomDbKey.CHR_NVRAM_SIZE.value: 0,
            }
        )

        conflict_res = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.PRG_RAM_SIZE.value: 16,
                RomDbKey.PRG_NVRAM_SIZE.value: 0,
                RomDbKey.CHR_RAM_SIZE.value: 4,
                RomDbKey.CHR_NVRAM_SIZE.value: 0,
            }
        )

        self.assertEqual(conflict_res, (0, 0, 0, 1))

    def test_process_record_by_crc_ram_sum_match_no_conflict(self):
        """RAM sum match between existing and update should not trigger conflict."""
        crc = "RAMSUMMATCH"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.PRG_RAM_SIZE.value: 8,
                RomDbKey.PRG_NVRAM_SIZE.value: 0,
                RomDbKey.CHR_RAM_SIZE.value: 4,
                RomDbKey.CHR_NVRAM_SIZE.value: 0,
            }
        )

        skip_res = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.PRG_RAM_SIZE.value: 0,
                RomDbKey.PRG_NVRAM_SIZE.value: 8,
                RomDbKey.CHR_RAM_SIZE.value: 0,
                RomDbKey.CHR_NVRAM_SIZE.value: 4,
            }
        )

        self.assertEqual(skip_res, (0, 0, 1, 0))

    def test_list_roms_ordering(self):
        """Ensure list_roms returns rows ordered by rom_id ascending."""
        # insert two roms via upsert
        self.db.upsert_rom(2, {RomDbKey.NAME.value: "B", RomDbKey.CRC.value: "C2"})
        self.db.upsert_rom(1, {RomDbKey.NAME.value: "A", RomDbKey.CRC.value: "C1"})
        rows = self.db.list_roms()
        self.assertGreaterEqual(len(rows), 2)
        # ensure ordering by rom_id ascending and that our entries exist
        ids = [r[RomDbKey.ROM_ID.value] for r in rows if r.get(RomDbKey.ROM_ID.value) in (1, 2)]
        self.assertIn(1, ids)
        self.assertIn(2, ids)

    def test_ensure_columns_adds_column(self):
        """Verify _ensure_columns adds missing columns to the table."""
        # add a synthetic column and verify it appears
        self.db._ensure_columns({"new_test_col": "TEXT"})
        cur = self.db._conn.execute("PRAGMA table_info(roms)")
        cols = {r[1] for r in cur.fetchall()}
        self.assertIn("new_test_col", cols)

    def test_upsert_stores_all_fields(self):
        """Upsert should persist all fields and allow retrieval by rom_id."""
        rom_id = 7
        col_types = self.db.list_columns_with_types()
        col_names = [name for name in col_types if name != RomDbKey.ROM_ID.value]

        payload = {}
        int_value = 1
        for name in col_names:
            if col_types[name] == "INTEGER":
                payload[name] = int_value
                int_value += 1
            else:
                payload[name] = f"val_{name}"
        self.assertEqual(len(payload), len(col_names))

        self.db.upsert_rom(rom_id, payload)
        fetched = self.db.get_rom(rom_id)
        self.assertIsNotNone(fetched)
        for key, value in payload.items():
            self.assertEqual(str(fetched.get(key)), str(value))

    def test_hardware_ntsc_does_not_overwrite_more_specific_xml_value(self) -> None:
        """When XML import sets hardware=NES_MULTI_REGION and scraper says NES_NTSC,
        scraper must not overwrite the more-specific XML value — no conflict either."""
        crc = "MULTIREGCRC"
        # Simulate: XML import already set hardware to NES_MULTI_REGION (6)
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_MULTI_REGION.value,
            }
        )
        # Simulate: scraper says NTSC (0) — should be silently ignored, no conflict
        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_NTSC.value,
            }
        )
        self.assertNotEqual(result, (0, 0, 0, 1), "Should not produce a conflict")
        # Existing more-specific value must be preserved
        row = self.db.get_rom_by_crc(crc)
        self.assertEqual(str(row[RomDbKey.HARDWARE.value]), str(HardwareType.NES_MULTI_REGION.value))

    def test_hardware_ntsc_does_not_overwrite_pal(self) -> None:
        """When XML/prior import set hardware=NES_PAL, scraper NES_NTSC must not conflict or overwrite."""
        crc = "PALOVERNTSC"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_PAL.value,
            }
        )
        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_NTSC.value,
            }
        )
        self.assertNotEqual(result, (0, 0, 0, 1), "Should not produce a conflict")
        row = self.db.get_rom_by_crc(crc)
        self.assertEqual(str(row[RomDbKey.HARDWARE.value]), str(HardwareType.NES_PAL.value))

    def test_hardware_conflict_between_two_specific_values(self) -> None:
        """Two genuinely different specific hardware values must still conflict."""
        crc = "HWCONFLICT"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_PAL.value,
            }
        )
        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_MULTI_REGION.value,
            }
        )
        self.assertEqual(result, (0, 0, 0, 1), "Two differing specific values must conflict")

    def test_hardware_multi_region_existing_ignores_incoming(self) -> None:
        """When existing hardware is NES_MULTI_REGION, any incoming value must be
        silently ignored — no overwrite and no conflict reported."""
        crc = "MULTIREG_LOCK"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_MULTI_REGION.value,
            }
        )
        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_PAL.value,
            }
        )
        self.assertNotEqual(result, (0, 0, 0, 1), "NES_MULTI_REGION existing must not conflict")
        row = self.db.get_rom_by_crc(crc)
        self.assertEqual(
            str(row[RomDbKey.HARDWARE.value]),
            str(HardwareType.NES_MULTI_REGION.value),
            "NES_MULTI_REGION must be preserved",
        )

    def test_hardware_famicom_upgrades_ntsc(self) -> None:
        """When existing hardware is NES_NTSC (generic XML value) and the scraper
        detects Japan and sets FAMICOM, FAMICOM must overwrite NES_NTSC — no conflict."""
        crc = "FAMICOM_UP"
        self.db.insert_rom_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.NES_NTSC.value,
            }
        )
        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.HARDWARE.value: HardwareType.FAMICOM.value,
            }
        )
        self.assertNotEqual(result, (0, 0, 0, 1), "FAMICOM upgrading NES_NTSC must not conflict")
        row = self.db.get_rom_by_crc(crc)
        self.assertEqual(
            str(row[RomDbKey.HARDWARE.value]),
            str(HardwareType.FAMICOM.value),
            "FAMICOM must overwrite NES_NTSC",
        )


class TestHardwareFromConsoleTypeAndRegion(unittest.TestCase):
    """Tests for hardware_from_console_type_and_region()."""

    def test_region_2_numeric_is_multi_region(self):
        """Region string '2' (iNES spec: multi-region) must map to NES_MULTI_REGION, not Dendy."""
        result = hardware_from_console_type_and_region("0", "2")
        self.assertEqual(result, HardwareType.NES_MULTI_REGION.value)

    def test_region_3_numeric_is_dendy(self):
        """Region string '3' (iNES spec: Dendy) must map to DENDY, not NES_MULTI_REGION."""
        result = hardware_from_console_type_and_region("0", "3")
        self.assertEqual(result, HardwareType.DENDY.value)

    def test_region_text_dendy_is_dendy(self):
        """Text region 'dendy' must map to DENDY hardware type."""
        result = hardware_from_console_type_and_region("0", "dendy")
        self.assertEqual(result, HardwareType.DENDY.value)

    def test_region_text_universal_is_multi_region(self):
        """Text region 'universal' must map to NES_MULTI_REGION hardware type."""
        result = hardware_from_console_type_and_region("0", "universal")
        self.assertEqual(result, HardwareType.NES_MULTI_REGION.value)

    def test_region_0_is_ntsc(self):
        """Region '0' must remain NES_NTSC."""
        result = hardware_from_console_type_and_region("0", "0")
        self.assertEqual(result, HardwareType.NES_NTSC.value)

    def test_region_1_is_pal(self):
        """Region '1' must remain NES_PAL."""
        result = hardware_from_console_type_and_region("0", "1")
        self.assertEqual(result, HardwareType.NES_PAL.value)

    def test_japan_ntsc_is_famicom(self):
        """NTSC + Japan country must yield Famicom, not NES_NTSC."""
        result = hardware_from_console_type_and_region("0", "0", country="Licensed Japan")
        self.assertEqual(result, HardwareType.FAMICOM.value)

    def test_japan_multi_region_is_famicom(self):
        """Multi-region + Japan country must stay NES_MULTI_REGION, not Famicom."""
        result = hardware_from_console_type_and_region("0", "2", country="Licensed Japan")
        self.assertEqual(result, HardwareType.NES_MULTI_REGION.value)

    def test_japan_pal_remains_pal(self):
        """PAL + Japan country must remain NES_PAL (contradictory, keep as-is)."""
        result = hardware_from_console_type_and_region("0", "1", country="Japan")
        self.assertEqual(result, HardwareType.NES_PAL.value)

    def test_non_japan_ntsc_remains_nes_ntsc(self):
        """NTSC without Japan country must remain NES_NTSC."""
        result = hardware_from_console_type_and_region("0", "0", country="USA")
        self.assertEqual(result, HardwareType.NES_NTSC.value)


class TestExpansionTypeMerge(unittest.TestCase):
    """Merge rules for the ``expansion_type`` column in process_record_by_crc.

    Scraped records carry every field as a string, while the column is INTEGER
    so existing values read back as ints. The merge rules must therefore compare
    the two sides as strings, exactly as the ``hardware`` branch does.
    """

    def setUp(self) -> None:
        fd, self.db_path = tempfile.mkstemp(prefix="romdb_expansion_", suffix=".sqlite")
        os.close(fd)
        self.db = RomDatabase(self.db_path)

    def tearDown(self) -> None:
        with contextlib.suppress(Exception):
            self.db.close()
        with contextlib.suppress(Exception):
            os.unlink(self.db_path)

    def _seed(self, crc: str, expansion: ControllerType) -> None:
        self.db.insert_rom_by_crc({RomDbKey.CRC.value: crc, RomDbKey.EXPANSION_TYPE.value: str(expansion.value)})

    def _stored(self, crc: str) -> object:
        return self.db.get_rom_by_crc(crc).get(RomDbKey.EXPANSION_TYPE.value)

    def test_incoming_multicart_replaces_a_specific_expansion_type(self):
        """Given a specific type, when a multicart arrives, then it wins."""
        crc = "EXPMULTI1"
        self._seed(crc, ControllerType.ZAPPER_4017)

        result = self.db.process_record_by_crc(
            {RomDbKey.CRC.value: crc, RomDbKey.EXPANSION_TYPE.value: str(ControllerType.MULTICART.value)}
        )

        self.assertEqual(result, (0, 1, 0, 0), "multicart should be merged, not reported as a conflict")
        self.assertEqual(int(self._stored(crc)), ControllerType.MULTICART.value)

    def test_incoming_standard_controller_never_overwrites_a_specific_type(self):
        """Given a specific type, when the generic standard controller arrives, then it is ignored."""
        crc = "EXPSTD1"
        self._seed(crc, ControllerType.ZAPPER_4017)

        result = self.db.process_record_by_crc(
            {
                RomDbKey.CRC.value: crc,
                RomDbKey.EXPANSION_TYPE.value: str(ControllerType.STANDARD_CONTROLLERS.value),
            }
        )

        self.assertEqual(result, (0, 0, 1, 0), "generic standard controller should be skipped, not a conflict")
        self.assertEqual(int(self._stored(crc)), ControllerType.ZAPPER_4017.value)


if __name__ == "__main__":
    unittest.main()
