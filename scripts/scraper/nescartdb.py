from typing import Dict, Optional, List, Iterator, Union
from urllib.request import Request, urlopen
from rom_database import RomDbKey, ConsoleType, ControllerType
import re

try:
    from bs4 import BeautifulSoup
except ImportError:  # pragma: no cover - dependency handled by main
    BeautifulSoup = None


BASE_URL = "https://nescartdb.com/profile/view/{}"


class NesCartDb:
    """Iterator-like scraper for nescartdb.com profiles.

    Initialize with a list of integer IDs (or a single ID). Call
    `next_record()` repeatedly to fetch and parse each profile; returns a
    dict of string values (keys only present when available). Returns None
    when exhausted.
    """

    def __init__(self, ids: Union[str, List[int], int], base_url: Optional[str] = None):
        """Create a scraper backed by an iterable of numeric ids.

        ids may be:
        - A string 'all' to iterate 0..4800
        - A string range '123-456' to iterate that inclusive range
        - A string containing a single number '123'
        - An int or a list of ints (kept for backward compatibility)
        """
        # Normalize input into an iterator of ints
        MAX_ID = 4800
        ids_list: List[int]
        if isinstance(ids, str):
            s = ids.strip().lower()
            if s == "all":
                ids_list = list(range(0, MAX_ID + 1))
            else:
                tokens = [token.strip() for token in s.split(",") if token.strip()]
                ids_list = []
                for token in tokens:
                    if token == "all":
                        raise ValueError("'all' cannot be combined with other ids")
                    m = re.match(r"^(\d+)-(\d+)$", token)
                    if m:
                        start, end = int(m.group(1)), int(m.group(2))
                        if start > end:
                            start, end = end, start
                        start = max(0, start)
                        end = min(MAX_ID, end)
                        ids_list.extend(range(start, end + 1))
                    elif token.isdigit():
                        ids_list.append(int(token))
                    else:
                        raise ValueError(f"invalid ids string: {ids}")
        elif isinstance(ids, int):
            ids_list = [ids]
        else:
            # Assume it's already an iterable/list of ints
            ids_list = list(ids)

        self._remaining = len(ids_list)
        self._ids_iter: Iterator[int] = iter(ids_list)
        self._base_url = base_url or BASE_URL

    def _fetch_html(self, url: str) -> str:
        req = Request(url, headers={"User-Agent": "neser-rom-scraper/1.0"})
        with urlopen(req, timeout=30) as resp:
            return resp.read().decode("utf-8", errors="replace")

    @staticmethod
    def _normalize_label(text: Optional[str]) -> str:
        if not text:
            return ""
        return re.sub(r"\s+", " ", text.strip().rstrip(":")).lower()

    @staticmethod
    def _extract_key_value_pairs(soup: "BeautifulSoup") -> Dict[str, str]:
        values: Dict[str, str] = {}
        for table in soup.find_all("table"):
            for row in table.find_all("tr"):
                cells = row.find_all(["th", "td"])
                if len(cells) != 2:
                    continue
                label = NesCartDb._normalize_label(cells[0].get_text(" ", strip=True))
                value = cells[1].get_text(" ", strip=True)
                if not label or not value:
                    continue
                values.setdefault(label, value)
        return values

    @staticmethod
    def _table_title(table) -> Optional[str]:
        first_row = table.find("tr")
        if not first_row:
            return None
        cells = first_row.find_all(["th", "td"])
        if len(cells) != 1:
            return None
        return NesCartDb._normalize_label(cells[0].get_text(" ", strip=True))

    @staticmethod
    def _find_titled_table(soup: "BeautifulSoup", title: str):
        normalized = NesCartDb._normalize_label(title)
        for table in soup.find_all("table"):
            if NesCartDb._table_title(table) == normalized:
                return table
        return None

    @staticmethod
    def _parse_eeprom_size_from_chip_info(table) -> Optional[int]:
        if table is None:
            return None
        for row in table.find_all("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.find_all(["th", "td"]) ]
            if len(cells) < 4:
                continue
            designation = cells[0].lower()
            chip_type = cells[3].lower()
            if "eeprom" not in designation and "eeprom" not in chip_type:
                continue
            m = re.search(r"(\d+)\s*(kb|mb)", chip_type)
            if not m:
                return None
            size = int(m.group(1))
            unit = m.group(2)
            if unit == "kb":
                return (size * 1024) // 8
            if unit == "mb":
                return (size * 1024 * 1024) // 8
        return None

    @staticmethod
    def _parse_rom_details(table) -> Dict[str, str]:
        results: Dict[str, str] = {}
        if table is None:
            return results
        for row in table.find_all("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.find_all(["th", "td"]) ]
            if len(cells) < 4:
                continue
            kind = NesCartDb._normalize_label(cells[0])
            size = cells[2].strip()
            crc = cells[3].strip()
            if kind.startswith("prg") or kind.startswith("chr"):
                results["crc"] = crc
            if kind == "roms combined":
                results["crc"] = size
            elif kind.startswith("prg"):
                old = results.get(RomDbKey.PRG_ROM_SIZE) or 0
                results[RomDbKey.PRG_ROM_SIZE] = old + NesCartDb._parse_size(size)
            elif kind.startswith("chr"):
                old = results.get(RomDbKey.CHR_ROM_SIZE) or 0
                results[RomDbKey.CHR_ROM_SIZE] = old + NesCartDb._parse_size(size)
        return results

    @staticmethod
    def _first_value(values: Dict[str, str], labels) -> Optional[str]:
        for label in labels:
            key = NesCartDb._normalize_label(label)
            if key in values:
                return values[key]
        return None

    @staticmethod
    def _parse_video_system(region_value: Optional[str]) -> Optional[str]:
        if not region_value:
            return None
        m = re.search(r"\b(NTSC|PAL)\b", region_value, re.IGNORECASE)
        if m:
            return m.group(1).upper()
        return None

    @staticmethod
    def _parse_int(value: Optional[str]) -> Optional[int]:
        if not value:
            return None
        m = re.search(r"(\d+)", value.replace(",", ""))
        if not m:
            return None
        return int(m.group(1))

    @staticmethod
    def _parse_size(value: Optional[str]) -> Optional[int]:
        if not value:
            return None
        cleaned = value.replace(",", "").strip().lower()
        m = re.search(r"(\d+)(?:\s*(kb|mb))?", cleaned)
        if not m:
            return None
        size = int(m.group(1))
        unit = m.group(2)
        if unit == "kb":
            return size * 1024
        if unit == "mb":
            return size * 1024 * 1024
        return size

    @staticmethod
    def _parse_yes_no(value: Optional[str]) -> Optional[int]:
        if not value:
            return None
        normalized = value.strip().lower()
        if normalized in {"yes", "y", "true"}:
            return 1
        if normalized in {"no", "n", "false"}:
            return 0
        return None

    @staticmethod
    def _match_non_standard_controller(rom_id: int, value: str,) -> Optional[int]:
        normalized = value.strip().lower()
        if "4-player adapter" in normalized:
            # Smash T.V. is double fisted, which implies a four score
            if rom_id == 927:
                return ControllerType.DOUBLE_FISTED
            return ControllerType.NES_FOUR_SCORE
        elif "zapper light gun" in normalized:
            return ControllerType.ZAPPER_4017
        # 525, 600, 780, 910, 928 should be B not A
        elif "power pad" in normalized or "family fun fitness mat" in normalized:
            # Only Athletic world (525) and Street Cop (928) used B side
            if rom_id in [525, 928]:
                return ControllerType.POWER_PAD_SIDE_B
            return ControllerType.POWER_PAD_SIDE_A
        # All Japaneese titles used the A side of the mat
        elif "family trainer mat" in normalized:
            return ControllerType.FAMILY_TRAINER_SIDE_A
        # ROB is one and the same hardware but need to patch as they differ in nes20db
        elif "r. o. b." in normalized:
            # Gyromite ROB
            if rom_id in [266, 584, 785]:
                return ControllerType.ROB_GYROMITE
            # Stack Up ROB
            else:
                return ControllerType.ROB_STACK_UP
        elif "3-d glasses" in normalized:
            return ControllerType.THREE_D_GLASSES
        elif "power glove" in normalized:
            return ControllerType.POWER_GLOVE
        elif "vaus controller" in normalized:
            return ControllerType.ARKANOID_VAUS_NES
        elif "miracle piano" in normalized:
            return ControllerType.MIRACLE_PIANO
        # 550 says FOUR SCORE
        elif "aladdin deck enhancer" in normalized:
            return ControllerType.ALADDIN_DECK_ENHANCER
        elif "barcode battler" in normalized:
            return ControllerType.SUNSOFT_BARCODE_BATTLER
        elif "top rider bike" in normalized:
            return ControllerType.TOP_RIDER
        elif "konami hypershot" in normalized:
            return ControllerType.KONAMI_HYPER_SHOT
        elif "mahjong controller" in normalized:
            return ControllerType.JISSEN_MAHJONG
        elif "battle box" in normalized:
            return ControllerType.IGS_STORAGE_BATTLE_BOX
        elif "racermate bike" in normalized:
            return ControllerType.RACERMATE_BICYCLE
        return None

    @staticmethod
    def _parse_periphereals(rom_id: int, value: str) -> Optional[int]:
        # If there are more than one value, one is bound to be Famicom/NES controller, so ignore that
        # Accommodate for an empty value bug on the web page
        value = ",".join(v.strip() for v in value.split(",") if v.strip())
        if ',' in value:
            for val in value.split(','):
                matched = NesCartDb._match_non_standard_controller(rom_id, val)
                if matched is not None:
                    return matched
            print(f"\nUnrecognized peripherals value: '{value}'")
            exit(1)
        matched = NesCartDb._match_non_standard_controller(rom_id, value)
        if matched is not None:
            return matched
        if "nes controller" in value.lower() or "famicom controller" in value.lower():
            return ControllerType.STANDARD_CONTROLLERS
        print(f"\nUnrecognized peripherals value: '{value}'")
        exit(1)

    def _build_result(self, rom_id: int, html: str) -> Optional[Dict[str, str]]:
        soup = BeautifulSoup(html, "html.parser")
        invalid_header = soup.find("h3")
        if invalid_header and invalid_header.get_text(strip=True) == "Invalid profile specified!":
            return None
        kv = self._extract_key_value_pairs(soup)
        rom_details = self._parse_rom_details(self._find_titled_table(soup, "ROM Details"))
        chip_info = self._find_titled_table(soup, "Detailed Chip Info")

        game_name = None
        title_tag = soup.find("title")
        if title_tag and title_tag.text:
            game_name = title_tag.text.split("-")[0].strip()
        h1_tag = soup.find("h1")
        if h1_tag and h1_tag.text:
            game_name = h1_tag.text.strip()

        result: Dict[str, str] = {}
        # Insert defaults for ROM and RAM
        result[RomDbKey.PRG_ROM_SIZE.value] = 0
        result[RomDbKey.CHR_ROM_SIZE.value] = 0
        result[RomDbKey.PRG_RAM_SIZE.value] = 0
        result[RomDbKey.CHR_RAM_SIZE.value] = 0
        result[RomDbKey.PRG_NVRAM_SIZE.value] = 0
        result[RomDbKey.CHR_NVRAM_SIZE.value] = 0
        if game_name:
            result[RomDbKey.NAME.value] = game_name
        if rom_details.get("crc"):
            result[RomDbKey.CRC.value] = rom_details.get("crc")
        result[RomDbKey.CONSOLE_TYPE.value] = ConsoleType.NES_FAMICOM
        mapper = self._first_value(kv, ["iNES Mapper", "Mapper"])
        submapper = self._first_value(kv, ["Submapper", "SubMapper"])
        chr_ram = self._first_value(kv, ["CHR RAM", "CHR-RAM", "VRAM"])
        work_ram = self._first_value(kv, ["WRAM", "Work RAM"])
        eeprom_size = self._parse_eeprom_size_from_chip_info(chip_info) or 0
        batt = self._first_value(kv, ["Battery present", "Battery"])
        # In iNES 2.0, battery means "battery or other non-volatile memory"
        if eeprom_size > 0:
            batt = '1'
        peri = self._first_value(kv, ["Peripherals", "Controllers"])
        if mapper:
            result[RomDbKey.MAPPER.value] = self._parse_int(mapper)
        if submapper:
            result[RomDbKey.SUBMAPPER.value] = self._parse_int(submapper)
        if rom_details.get(RomDbKey.PRG_ROM_SIZE):
            result[RomDbKey.PRG_ROM_SIZE.value] = rom_details.get(RomDbKey.PRG_ROM_SIZE)
        if rom_details.get(RomDbKey.CHR_ROM_SIZE):
            result[RomDbKey.CHR_ROM_SIZE.value] = rom_details.get(RomDbKey.CHR_ROM_SIZE)
        if chr_ram:
            result[RomDbKey.CHR_RAM_SIZE.value] = self._parse_size(chr_ram)
        if work_ram:
            if self._parse_yes_no(batt):
                result[RomDbKey.PRG_NVRAM_SIZE.value] = self._parse_size(work_ram)
            else:
                result[RomDbKey.PRG_RAM_SIZE.value] = self._parse_size(work_ram)
                result[RomDbKey.PRG_NVRAM_SIZE.value] = eeprom_size
        if batt:
            result[RomDbKey.BATTERY.value] = self._parse_yes_no(batt)
        if peri:
            peri_value = self._parse_periphereals(rom_id, peri)
            if peri_value:
                result[RomDbKey.EXPANSION_TYPE.value] = peri_value
        # Peripherals/Controller fields are text and have no matching DB column.

        return result

    def num_left(self) -> int:
        """Return the number of records left to parse."""
        return self._remaining

    def _patch(self, rom_id: int, record: Optional[Dict[str, str]]) -> None:
        """Apply hardcoded patches for known bad/missing data."""
        # Startropics I and II (41, 814, 1896, 2449, 2769, 2780, 4171, 4365) has a 1kB PRG RAM in the MMC6 chip
        # As it also has a battery, it will be converted to NVRAM later in the parsing
        if rom_id in [41, 814, 1896, 2449, 2769, 2780, 4171, 4365]:
            record[RomDbKey.PRG_RAM_SIZE.value] = 1024
        # Pyramid with ROM_IDs 219 and 315: nes20db and nescart say different mappers.
        # 0 vs 79. However, these should be interchangable without any banking (8kB CHR)
        if rom_id in [219, 315]:
            record[RomDbKey.MAPPER.value] = 0
        # Gauntlet (ROM_ID 473), came in both Mapper 4 and 206 variants (same CRC)
        # nes20db says no 4 so let's go with that
        if rom_id in [473, 1316]:
            record[RomDbKey.MAPPER.value] = 4
        # Kyonshiizu 2 (ROM_ID 1559) has a X1-005 (mapper 80) with internal Save RAM
        # Taito Grand Prix: Eikou e no License (ROM_ID 1758) too
        # Fudou Myouou Den (ROM_ID 1762) too
        # Mirai Shinwa Jarvas (ROM_ID 1763) too
        # Kyuukyoku Harikiri Stadium (ROM_IDs 1765, 1766, 3071, 3147, 3148, 3149, 3150, 3303) too
        if rom_id in [1559, 1758, 1762, 1763, 1765, 1766, 3071, 3147, 3148, 3149, 3150, 3303]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 128
        # Kyuukyoku Harikiri Stadium: Heisei Gannen Ban (1767, 2254) has a Taito X1-017 with Save RAM
        if rom_id in [1767, 2254]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 1024
        # Famicom Jump II: Saikyou no 7 Nin (ROM_ID 1734) is mapper 153 (not 16) as PRG-ROM
        # is larger than 128 kB
        if rom_id == 1734:
            record[RomDbKey.MAPPER.value] = 153
        # # Dragon Ball Z: Kyoushuu! Saiyajin (2248) has a 128 byte EEPROM
        # # Magical Taruruuto-kun: Fantastic World!! (Version 2.0) (ROM_ID 1747, 2244) too
        # # SD Gundam Gaiden: Knight Gundam Monogatari (Version 2.0) (1746, 2249, 3079, 3080, 3081,
        # # 3082, 3083) too
        # if rom_id in [1747, 2244, 2248, 1746, 2249, 3079, 3080, 3081, 3082, 3083]:
        #     record[RomDbKey.PRG_NVRAM_SIZE.value] = 128
        # Bubble Bath Babes (ROM_ID 1838) is mapper 148 as this is an unlicensed AVE&NINA board
        if rom_id == 1838:
            record[RomDbKey.MAPPER.value] = 148
        # Skate Boy (2639) has no CHR ROM switching, so it should be mapper 0, not 4
        # El Monstruo de los Globos (2640) too
        # Booky Man (2647) too
        if rom_id in [2639, 2640, 2647]:
            record[RomDbKey.MAPPER.value] = 0
        # Galactic Crusader (2949) could be either mapper 79 or 146. Both will work. Chosing 79 as 146
        # for a non-switching 8kB ROMs seems odd
        if rom_id == 2949:
            record[RomDbKey.MAPPER.value] = 79


    def next_record(self) -> Optional[Dict[str, str]]:
        """Fetch and return the next parsed profile record, or None if done."""
        record  = None
        while record is None:
            try:
                rom_id = next(self._ids_iter)
            except StopIteration:
                return None

            url = self._base_url.format(rom_id)
            html = self._fetch_html(url)
            record = self._build_result(rom_id, html)
            self._patch(rom_id, record)
            self._remaining -= 1
        return record
