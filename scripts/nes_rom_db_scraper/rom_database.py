"""SQLite-backed ROM database for scraped NES Cart Database entries."""

from __future__ import annotations

import sqlite3
from collections.abc import Mapping
from enum import IntEnum, StrEnum


class HardwareType(IntEnum):
    """ROM database hardware types (merged console type + region)"""

    NES_NTSC = 0
    NES_PAL = 1
    FAMICOM = 2
    VS_SYSTEM = 3
    DENDY = 4
    PLAYCHOICE_10 = 5
    NES_MULTI_REGION = 6
    VT01_MONOCHROME = 7
    VT01_STN = 8
    VT02 = 9
    VT03 = 10
    VT09 = 11
    VT32 = 12
    VT369 = 13
    UMC_UM6578 = 14
    FAMICOM_NETWORK_SYSTEM = 15


# Mapping from iNES 2.0 extended console type to HardwareType
_EXTENDED_CONSOLE_MAP = {
    3: HardwareType.VT01_MONOCHROME,
    4: HardwareType.VT01_STN,
    5: HardwareType.VT02,
    6: HardwareType.VT03,
    7: HardwareType.VT09,
    8: HardwareType.VT32,
    9: HardwareType.VT369,
    10: HardwareType.UMC_UM6578,
    11: HardwareType.FAMICOM_NETWORK_SYSTEM,
}

# Mapping from iNES 2.0 region to HardwareType (for console type 0 = NES/Famicom)
# iNES 2.0 spec: 0=NTSC, 1=PAL, 2=Multi-region, 3=Dendy
_REGION_TEXT_MAP = {
    "ntsc": 0,
    "pal": 1,
    "universal": 2,
    "multi": 2,
    "dendy": 3,
}


def hardware_from_console_type_and_region(
    console_type_str: str | None,
    region_str: str | None,
    country: str | None = None,
) -> int | None:
    """Compute a HardwareType integer from iNES 2.0 console type and region strings.

    For console type 0 (NES/Famicom), the region normally selects between
    NES_NTSC, NES_PAL, NES_MULTI_REGION, and DENDY. If ``country`` contains
    "japan" (case-insensitive), NES_NTSC is upgraded to FAMICOM, but other
    region-derived types (including NES_PAL and NES_MULTI_REGION) are left
    unchanged.

    Returns None when the inputs cannot be mapped.
    """
    if console_type_str is None and region_str is None:
        return None

    try:
        ct = int(console_type_str) if console_type_str is not None else 0
    except (ValueError, TypeError):
        return None

    if ct == 1:
        return HardwareType.VS_SYSTEM.value
    if ct == 2:
        return HardwareType.PLAYCHOICE_10.value
    if ct >= 3:
        hw = _EXTENDED_CONSOLE_MAP.get(ct)
        return hw.value if hw is not None else None

    # ct == 0: NES/Famicom — determine from region
    if region_str is None:
        is_japan = country and "japan" in country.lower()
        return HardwareType.FAMICOM.value if is_japan else HardwareType.NES_NTSC.value
    try:
        cr = int(region_str)
    except (ValueError, TypeError):
        mapped = _REGION_TEXT_MAP.get(str(region_str).strip().lower())
        if mapped is None:
            return None
        cr = mapped

    # iNES 2.0 spec: region 2=Multi-region, 3=Dendy
    region_map = {
        0: HardwareType.NES_NTSC,
        1: HardwareType.NES_PAL,
        2: HardwareType.NES_MULTI_REGION,
        3: HardwareType.DENDY,
    }
    hw = region_map.get(cr)
    if hw is None:
        return None
    # NES_NTSC is upgraded to FAMICOM for Japan releases; NES_MULTI_REGION stays as-is
    is_japan = country and "japan" in country.lower()
    if is_japan and hw == HardwareType.NES_NTSC:
        return HardwareType.FAMICOM.value
    return hw.value


class NametableLayout(IntEnum):
    """iNES 2.0 Hardwired nametable layouts"""

    HORIZONTAL = 0
    VERTICAL = 1


class Battery(IntEnum):
    """iNES 2.0 Battery presence"""

    NO = 0
    YES = 1


class ControllerType(IntEnum):
    """iNES 2.0 Controller types"""

    UNSPECIFIED = 0x00
    STANDARD_CONTROLLERS = 0x01
    NES_FOUR_SCORE = 0x02
    FAMICOM_FOUR_PLAYERS_SIMPLE = 0x03
    VS_SYSTEM_4016 = 0x04
    VS_SYSTEM_4017 = 0x05
    RESERVED = 0x06
    VS_ZAPPER = 0x07
    ZAPPER_4017 = 0x08
    TWO_ZAPPERS = 0x09
    BANDAI_HYPER_SHOT = 0x0A
    POWER_PAD_SIDE_A = 0x0B
    POWER_PAD_SIDE_B = 0x0C
    FAMILY_TRAINER_SIDE_A = 0x0D
    FAMILY_TRAINER_SIDE_B = 0x0E
    ARKANOID_VAUS_NES = 0x0F
    ARKANOID_VAUS_FAMICOM = 0x10
    TWO_VAUS_DATA_RECORDER = 0x11
    KONAMI_HYPER_SHOT = 0x12
    COCONUTS_PACHINKO = 0x13
    EXCITING_BOXING_BAG = 0x14
    JISSEN_MAHJONG = 0x15
    YONEZAWA_PARTY_TAP = 0x16
    OEKA_KIDS_TABLET = 0x17
    SUNSOFT_BARCODE_BATTLER = 0x18
    MIRACLE_PIANO = 0x19
    POKKUN_MOGURAA_TAP_MAT = 0x1A
    TOP_RIDER = 0x1B
    DOUBLE_FISTED = 0x1C
    FAMICOM_3D_SYSTEM = 0x1D
    DOREMIKKO_KEYBOARD = 0x1E
    ROB_GYROMITE = 0x1F
    FAMICOM_DATA_RECORDER = 0x20
    ASCII_TURBO_FILE = 0x21
    IGS_STORAGE_BATTLE_BOX = 0x22
    FAMILY_BASIC_KEYBOARD_RECORDER = 0x23
    DONGDA_PEC_KEYBOARD = 0x24
    PUZE_BIT79_KEYBOARD = 0x25
    SUBOR_KEYBOARD = 0x26
    SUBOR_KEYBOARD_MACRO_MOUSE = 0x27
    SUBOR_KEYBOARD_SUBOR_MOUSE_4016 = 0x28
    SNES_MOUSE_4016 = 0x29
    MULTICART = 0x2A
    TWO_SNES_CONTROLLERS = 0x2B
    RACERMATE_BICYCLE = 0x2C
    U_FORCE = 0x2D
    ROB_STACK_UP = 0x2E
    CITY_PATROLMAN_LIGHTGUN = 0x2F
    SHARP_C1_CASSETTE = 0x30
    STANDARD_SWAP_DPAD_BA = 0x31
    EXCALIBUR_SUDOKU_PAD = 0x32
    ABL_PINBALL = 0x33
    GOLDEN_NUGGET_BUTTONS = 0x34
    KEDA_KEYBOARD = 0x35
    SUBOR_KEYBOARD_SUBOR_MOUSE_4017 = 0x36
    PORT_TEST_CONTROLLER = 0x37
    BANDAI_MULTI_GAME_PLAYER = 0x38
    VENOM_TV_DANCE_MAT = 0x39
    LG_TV_REMOTE = 0x3A
    FAMICOM_NETWORK_CONTROLLER = 0x3B
    KING_FISHING_CONTROLLER = 0x3C
    CROAKY_KARAOKE = 0x3D
    KEWANG_KINGWON_KEYBOARD = 0x3E
    ZECHENG_KEYBOARD = 0x3F
    SUBOR_KEYBOARD_PS2_MOUSE_4017_L90 = 0x40
    UM6578_PS2_KEYBOARD_MOUSE_4017 = 0x41
    UM6578_PS2_MOUSE = 0x42
    YUXING_MOUSE_4016 = 0x43
    SUBOR_KEYBOARD_YUXING_MOUSE_4016 = 0x44
    GIGGGLE_TV_PUMP = 0x45
    BBK_KEYBOARD_PS2_MOUSE_4017_R90 = 0x46
    MAGICAL_COOKING = 0x47
    SNES_MOUSE_4017 = 0x48
    ZAPPER_4016 = 0x49
    ARKANOID_VAUS_PROTO = 0x4A
    TV_MAHJONG_GAME = 0x4B
    MAHJONG_GEKITOU_DENSETU = 0x4C
    SUBOR_KEYBOARD_PS2_MOUSE_4017_XINV = 0x4D
    IBM_PC_XT_KEYBOARD = 0x4E
    SUBOR_KEYBOARD_MEGA_BOOK_MOUSE = 0x4F
    # Values not in iNES 2.0 spec:
    POWER_GLOVE = 0x51
    ALADDIN_DECK_ENHANCER = 0x52


class VsHardwareType(IntEnum):
    """iNES 2.0 Vs Hardware types"""

    VS_UNISYSTEM = 0x00
    VS_UNISYSTEM_RBI_BASEBALL = 0x01
    VS_UNISYSTEM_TKO_BOXING = 0x02
    VS_UNISYSTEM_SUPER_XEVIOUS = 0x03
    VS_UNISYSTEM_ICE_CLIMBER_JP = 0x04
    VS_DUALSYSTEM = 0x05
    VS_DUALSYSTEM_BUNGELING_BAY = 0x06


class VsPpuType(IntEnum):
    """iNES 2.0 Vs PPU types"""

    RP2C03_ANY = 0x00
    RP2C04_0001 = 0x02
    RP2C04_0002 = 0x03
    RP2C04_0003 = 0x04
    RP2C04_0004 = 0x05
    RC2C05_01 = 0x08
    RC2C05_02 = 0x09
    RC2C05_03 = 0x0A
    RC2C05_04 = 0x0B


class RomDbKey(StrEnum):
    """ROM database field keys."""

    ROM_ID = "rom_id"
    CRC = "crc"
    NAME = "name"
    COUNTRY = "country"
    HARDWARE = "hardware"
    CONSOLE_CLASS = "rom_class"
    MAPPER = "mapper"
    SUBMAPPER = "submapper"
    NAMETABLE_LAYOUT = "nametable_layout"
    PRG_ROM_SIZE = "prg_rom_size"
    CHR_ROM_SIZE = "chr_rom_size"
    PRG_NVRAM_SIZE = "prg_nvram_size"
    PRG_RAM_SIZE = "prg_ram_size"
    CHR_NVRAM_SIZE = "chr_nvram_size"
    CHR_RAM_SIZE = "chr_ram_size"
    PRG_ROM_CRC = "prg_rom_crc"
    CHR_ROM_CRC = "chr_rom_crc"
    BATTERY = "battery"
    VS_HARDWARE_TYPE = "vs_hardware_type"
    VS_PPU_TYPE = "vs_ppu_type"
    EXPANSION_TYPE = "expansion_type"


def _print_column_conflict(crc: str, key: str, old_value: object, new_value: object) -> None:
    """Report a per-column merge conflict for a ROM identified by CRC."""
    print(f"\nConflict on CRC {crc}: column '{key}' has existing value '{old_value}', new value '{new_value}'")


class RomDatabase:
    # Several queries below interpolate column names taken from the table
    # schema itself (PRAGMA table_info) or from RomDbKey constants. SQL cannot
    # bind identifiers as parameters, so those f-strings carry an S608
    # suppression; every *value* is still bound via ? or :name placeholders.
    """Store and update ROM metadata in a SQLite database."""

    @staticmethod
    def _expected_columns() -> dict[str, str]:
        return {
            RomDbKey.NAME.value: "TEXT",
            RomDbKey.COUNTRY.value: "TEXT",
            RomDbKey.CRC.value: "TEXT",
            RomDbKey.HARDWARE.value: "INTEGER",
            RomDbKey.CONSOLE_CLASS.value: "TEXT",
            RomDbKey.MAPPER.value: "INTEGER",
            RomDbKey.SUBMAPPER.value: "INTEGER",
            RomDbKey.NAMETABLE_LAYOUT.value: "TEXT",
            RomDbKey.PRG_ROM_SIZE.value: "INTEGER",
            RomDbKey.PRG_ROM_CRC.value: "TEXT",
            RomDbKey.PRG_NVRAM_SIZE.value: "INTEGER",
            RomDbKey.PRG_RAM_SIZE.value: "INTEGER",
            RomDbKey.CHR_ROM_SIZE.value: "INTEGER",
            RomDbKey.CHR_ROM_CRC.value: "TEXT",
            RomDbKey.CHR_NVRAM_SIZE.value: "INTEGER",
            RomDbKey.CHR_RAM_SIZE.value: "INTEGER",
            RomDbKey.BATTERY.value: "INTEGER",
            RomDbKey.VS_HARDWARE_TYPE.value: "INTEGER",
            RomDbKey.VS_PPU_TYPE.value: "INTEGER",
            RomDbKey.EXPANSION_TYPE.value: "INTEGER",
        }

    def __init__(self, db_path: str) -> None:
        """Open or create the SQLite database at ``db_path`` and ensure the
        primary `roms` table and expected columns exist.

        The connection is stored on ``self._conn`` for use by other methods.
        """
        self._db_path = db_path
        self._conn = sqlite3.connect(db_path)
        expected = self._expected_columns()
        schema_cols = ["rom_id INTEGER PRIMARY KEY"]
        schema_cols.extend([f"{name} {col_type}" for name, col_type in expected.items()])
        self._conn.execute(
            f"""
            CREATE TABLE IF NOT EXISTS roms (
                {", ".join(schema_cols)}
            ) STRICT
            """
        )
        self._ensure_columns(expected)
        self._conn.commit()

    def _ensure_columns(self, columns: dict[str, str]) -> None:
        """Ensure the named columns exist on the `roms` table.

        `columns` is a mapping of column name to SQLite column type
        (e.g. ``{"foo": "TEXT"}``). Missing columns will be added.
        """
        cursor = self._conn.execute("PRAGMA table_info(roms)")
        existing = {row[1] for row in cursor.fetchall()}
        for name, col_type in columns.items():
            if name not in existing:
                self._conn.execute(f"ALTER TABLE roms ADD COLUMN {name} {col_type}")

    def close(self) -> None:
        """Commit any pending changes and close the DB connection."""
        self._conn.commit()
        self._conn.close()

    def list_columns_with_types(self) -> dict[str, str]:
        """Return current column names mapped to their declared types."""
        cursor = self._conn.execute("PRAGMA table_info(roms)")
        return {row[1]: row[2].upper() for row in cursor.fetchall()}

    def list_columns(self) -> list[str]:
        """Return current column names for the ``roms`` table."""
        return list(self.list_columns_with_types().keys())

    @staticmethod
    def _coerce_value(value: object | None, col_type: str) -> object | None:
        if value is None or value == "":
            return None
        if col_type == "INTEGER":
            if isinstance(value, bool):
                return int(value)
            if isinstance(value, int):
                return value
            try:
                return int(str(value), 10)
            except (TypeError, ValueError):
                return None
        return value

    def _coerce_params(self, params: Mapping[str, object | None], columns: list[str]) -> dict[str, object | None]:
        types = self.list_columns_with_types()
        return {col: self._coerce_value(params.get(col), types.get(col, "")) for col in columns}

    def get_rom(self, rom_id: int) -> dict[str, str | None] | None:
        """Return a single ROM record by numeric ``rom_id``.

        The returned mapping uses the names defined in ``RomDbKey`` as keys.
        Returns ``None`` if no matching row is found.
        """
        cursor = self._conn.execute("PRAGMA table_info(roms)")
        cols = [row[1] for row in cursor.fetchall()]
        if not cols:
            return None
        cursor = self._conn.execute(
            f"SELECT {', '.join(cols)} FROM roms WHERE rom_id = ?",  # noqa: S608
            (rom_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return {col: row[idx] for idx, col in enumerate(cols)}

    def get_rom_by_crc(self, crc: str) -> dict[str, str | None] | None:
        """Return a single ROM record matching the given ``crc`` string.

        The returned mapping uses the names defined in ``RomDbKey`` as keys.
        Returns ``None`` if no matching row is found.
        """
        cols = self.list_columns()
        if not cols:
            return None
        cursor = self._conn.execute(
            f"SELECT {', '.join(cols)} FROM roms WHERE crc = ?",  # noqa: S608
            (crc,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return {col: row[idx] for idx, col in enumerate(cols)}

    def upsert_rom(self, rom_id: int, data: dict[str, str | None]) -> None:
        """Insert or update a ROM identified by ``rom_id``.

        ``data`` should be a mapping keyed by the string values in
        ``RomDbKey``. Existing rows are updated on of ``rom_id``.
        """
        cursor = self._conn.execute("PRAGMA table_info(roms)")
        cols = [row[1] for row in cursor.fetchall()]
        cols = [col for col in cols if col != RomDbKey.ROM_ID.value]
        unknown_keys = sorted(set(data.keys()) - set(cols))
        if unknown_keys:
            print(
                f"upsert_rom: ignoring unknown fields {unknown_keys}",
                file=__import__("sys").stderr,
            )
        col_list = ", ".join([RomDbKey.ROM_ID.value, *cols])
        placeholder_list = ", ".join([f":{RomDbKey.ROM_ID.value}"] + [f":{c}" for c in cols])
        update_list = ", \n                ".join([f"{c}=excluded.{c}" for c in cols])
        sql = f"""
            INSERT INTO roms (
                {col_list}
            ) VALUES (
                {placeholder_list}
            )
            ON CONFLICT(rom_id) DO UPDATE SET
                {update_list}
            """  # noqa: S608
        params = {RomDbKey.ROM_ID.value: rom_id}
        params.update({c: data.get(c) for c in cols})
        coerced = self._coerce_params(params, [RomDbKey.ROM_ID.value, *cols])
        self._conn.execute(sql, coerced)
        self._conn.commit()

    def insert_rom_by_crc(self, data: Mapping[str, str | None]) -> None:
        """Insert a minimal row using the CRC and any minimal columns provided.

        This creates a row with the supplied columns; other columns remain
        NULL until updated.
        """
        cursor = self._conn.execute("PRAGMA table_info(roms)")
        existing = {row[1] for row in cursor.fetchall()}
        columns = [k for k in data if k in existing]
        if RomDbKey.CRC.value not in columns:
            columns.insert(0, RomDbKey.CRC.value)
        if not columns:
            return
        placeholders = ", ".join([":" + c for c in columns])
        params = {c: data.get(c) for c in columns}
        coerced = self._coerce_params(params, columns)
        self._conn.execute(
            f"""
            INSERT INTO roms ({", ".join(columns)})
            VALUES ({placeholders})
            """,  # noqa: S608
            coerced,
        )
        self._conn.commit()

    def update_rom_by_crc(self, crc: str, updates: Mapping[str, str | None]) -> None:
        """Apply in-place `updates` (column -> value) to the row matching ``crc``.

        No-op if ``updates`` is empty.
        """
        if not updates:
            return
        set_clause = ", ".join([f"{k} = :{k}" for k in updates])
        params = {**updates, "crc": crc}
        coerced = self._coerce_params(params, list(updates.keys()))
        coerced["crc"] = crc
        self._conn.execute(
            f"UPDATE roms SET {set_clause} WHERE crc = :crc",  # noqa: S608
            coerced,
        )
        self._conn.commit()

    def process_record_by_crc(self, data: dict[str, str]) -> tuple[int, int, int, int]:
        """Process a single parsed record (identified by CRC) and update DB.

        Returns a tuple (added_count, updated_count, skipped_count, conflict_count)
        where exactly one element is 1 and the others are 0 describing the
        outcome for this single record.
        """
        crc = data.get(RomDbKey.CRC.value)
        if not crc:
            # No CRC => nothing done
            return (0, 0, 0, 1)

        existing = self.get_rom_by_crc(crc)
        if existing is None:
            # Insert minimal columns present in data
            self.insert_rom_by_crc(data)
            return (1, 0, 0, 0)

        updates: dict[str, str] = {}
        has_conflict = False
        for key, value in data.items():
            if key == RomDbKey.CRC.value or value is None or value == "":
                continue
            old_value = existing.get(key)
            if old_value is None or (old_value == "" and value != ""):
                updates[key] = value
            elif str(old_value) != str(value):
                # Extra merge of controller types
                if key == RomDbKey.HARDWARE.value:
                    # NES_NTSC (0) is the least-specific fallback produced by the
                    # nescartdb.com scraper — treat it as a no-op so it never
                    # overwrites a more-specific value set by an XML import.
                    # NES_MULTI_REGION is authoritative from XML — any incoming
                    # value is silently ignored.
                    # FAMICOM (2) is a Japan-specific NES_NTSC — allow it to
                    # upgrade an existing generic NES_NTSC entry.
                    if str(value) == str(HardwareType.NES_NTSC.value):
                        pass  # keep existing value, no conflict
                    elif str(old_value) == str(HardwareType.NES_MULTI_REGION.value):
                        pass  # NES_MULTI_REGION is authoritative, keep it
                    elif str(old_value) == str(HardwareType.NES_NTSC.value) and str(value) == str(
                        HardwareType.FAMICOM.value
                    ):
                        updates[key] = value  # FAMICOM upgrades generic NES_NTSC
                    else:
                        _print_column_conflict(crc, key, old_value, value)
                        has_conflict = True
                # Extra merge of controller types. Scraped records carry every
                # field as a string while the column is INTEGER, so both sides
                # are compared as strings (as the hardware branch above does).
                elif key == RomDbKey.EXPANSION_TYPE.value:
                    # if new value is multicart, update to that
                    if str(value) == str(ControllerType.MULTICART.value):
                        updates[key] = value
                    # If old is multicaart, do nothing
                    elif str(old_value) == str(ControllerType.MULTICART.value):
                        pass
                    # If old value says standard controller, select the other one
                    elif str(old_value) == str(ControllerType.STANDARD_CONTROLLERS.value):
                        updates[key] = value
                    # If new value says standard controller, do nothing
                    elif str(value) == str(ControllerType.STANDARD_CONTROLLERS.value):
                        pass
                    else:
                        _print_column_conflict(crc, key, old_value, value)
                        has_conflict = True
                elif key in (
                    RomDbKey.PRG_RAM_SIZE.value,
                    RomDbKey.CHR_RAM_SIZE.value,
                    RomDbKey.PRG_NVRAM_SIZE.value,
                    RomDbKey.CHR_NVRAM_SIZE.value,
                ):
                    # RAM checks are handled below in one go
                    pass
                else:
                    _print_column_conflict(crc, key, old_value, value)
                    # print(f"Old={existing}, New={data}")
                    has_conflict = True

        # Special handling for RAM sizes
        def to_int(value: object | None) -> int | None:
            if value is None or value == "":
                return None
            if isinstance(value, int):
                return value
            try:
                return int(str(value), 10)
            except (TypeError, ValueError):
                return None

        def ram_sum(source: Mapping[str, object | None], ram_key: RomDbKey, nvram_key: RomDbKey) -> int | None:
            ram = to_int(source.get(ram_key.value))
            nvram = to_int(source.get(nvram_key.value))
            if ram is None and nvram is None:
                return None
            return (ram or 0) + (nvram or 0)

        existing_prg_sum = ram_sum(existing, RomDbKey.PRG_RAM_SIZE, RomDbKey.PRG_NVRAM_SIZE)
        update_prg_sum = ram_sum(data, RomDbKey.PRG_RAM_SIZE, RomDbKey.PRG_NVRAM_SIZE)
        if existing_prg_sum is not None and update_prg_sum is not None and existing_prg_sum != update_prg_sum:
            print(
                f"\nConflict on CRC {crc}: PRG RAM+NVRAM size sum mismatch "
                f"existing={existing_prg_sum}, new={update_prg_sum}"
            )
            has_conflict = True

        existing_chr_sum = ram_sum(existing, RomDbKey.CHR_RAM_SIZE, RomDbKey.CHR_NVRAM_SIZE)
        update_chr_sum = ram_sum(data, RomDbKey.CHR_RAM_SIZE, RomDbKey.CHR_NVRAM_SIZE)
        if existing_chr_sum is not None and update_chr_sum is not None and existing_chr_sum != update_chr_sum:
            print(
                f"\nConflict on CRC {crc}: CHR RAM+NVRAM size sum mismatch "
                f"existing={existing_chr_sum}, new={update_chr_sum}"
            )
            has_conflict = True

        if len(updates) > 0 and not has_conflict:
            self.update_rom_by_crc(crc, updates)
            return (0, 1, 0, 0)
        if not has_conflict:
            return (0, 0, 1, 0)
        return (0, 0, 0, 1)

    def list_roms(self) -> list[dict[str, str | None]]:
        """Return all ROM rows ordered by ``rom_id`` ascending.

        Each row is returned as a dict keyed by the string values from
        ``RomDbKey``.
        """
        cols = self.list_columns()
        if not cols:
            return []
        cursor = self._conn.execute(
            f"SELECT {', '.join(cols)} FROM roms ORDER BY rom_id ASC"  # noqa: S608
        )
        rows = []
        for row in cursor.fetchall():
            rows.append({col: row[idx] for idx, col in enumerate(cols)})
        return rows

    def reset_schema(self) -> None:
        """Drop existing tables and recreate the schema for a fresh database.

        This removes the `roms` table if present and recreates it with the
        same columns used in `__init__`.
        """
        self._conn.execute("DROP TABLE IF EXISTS roms")
        expected = self._expected_columns()
        schema_cols = ["rom_id INTEGER PRIMARY KEY"]
        schema_cols.extend([f"{name} {col_type}" for name, col_type in expected.items()])
        self._conn.execute(
            f"""
            CREATE TABLE IF NOT EXISTS roms (
                {", ".join(schema_cols)}
            ) STRICT
            """,
        )
        self._ensure_columns(expected)
        self._conn.commit()
