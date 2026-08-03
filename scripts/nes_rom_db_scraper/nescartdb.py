"""Scraper for nescartdb.com"""

import re
from collections.abc import Iterator
from pathlib import Path
from urllib.request import Request, urlopen

from bs4 import BeautifulSoup

from scripts.nes_rom_db_scraper.rom_database import (
    ControllerType,
    HardwareType,
    RomDbKey,
    hardware_from_console_type_and_region,
)

BASE_URL = "https://nescartdb.com/profile/view/{}"


class NesCartDb:
    """Iterator-like scraper for nescartdb.com profiles.

    Initialize with a list of integer IDs (or a single ID). Call
    `next_record()` repeatedly to fetch and parse each profile; returns a
    dict of string values (keys only present when available). Returns None
    when exhausted.
    """

    def __init__(self, ids: str | list[int] | int, base_url: str | None = None):
        """Create a scraper backed by an iterable of numeric ids.

        ids may be:
        - A string 'all' to iterate 0..4800
        - A string range '123-456' to iterate that inclusive range
        - A string containing a single number '123'
        - An int or a list of ints (kept for backward compatibility)
        """
        # Normalize input into an iterator of ints
        max_id = 4800
        ids_list: list[int]
        if isinstance(ids, str):
            s = ids.strip().lower()
            if s == "all":
                ids_list = list(range(1, max_id + 1))
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
                        end = min(max_id, end)
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
        self._cache_dir = Path(".html_cache")

    def _fetch_html(self, url: str) -> str:
        req = Request(url, headers={"User-Agent": "neser-rom-scraper/1.0"})
        with urlopen(req, timeout=30) as resp:
            return resp.read().decode("utf-8", errors="replace")

    def _fetch_html_with_cache(self, rom_id: int, url: str) -> str:
        cache_path = self._cache_dir / f"{rom_id}.html"
        if cache_path.exists():
            return cache_path.read_text(encoding="utf-8", errors="replace")

        html = self._fetch_html(url)
        self._cache_dir.mkdir(parents=True, exist_ok=True)
        cache_path.write_text(html, encoding="utf-8")
        return html

    @staticmethod
    def _normalize_label(text: str | None) -> str:
        if not text:
            return ""
        return re.sub(r"\s+", " ", text.strip().rstrip(":")).lower()

    @staticmethod
    def _extract_key_value_pairs(soup: BeautifulSoup) -> dict[str, str]:
        values: dict[str, str] = {}
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
    def _table_title(table) -> str | None:
        first_row = table.find("tr")
        if not first_row:
            return None
        cells = first_row.find_all(["th", "td"])
        if len(cells) != 1:
            return None
        return NesCartDb._normalize_label(cells[0].get_text(" ", strip=True))

    @staticmethod
    def _find_titled_table(soup: BeautifulSoup, title: str):
        normalized = NesCartDb._normalize_label(title)
        for table in soup.find_all("table"):
            if NesCartDb._table_title(table) == normalized:
                return table
        return None

    @staticmethod
    def _parse_eeprom_size_from_chip_info(table) -> int | None:
        if table is None:
            return None
        for row in table.find_all("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.find_all(["th", "td"])]
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
    def _parse_rom_details(table) -> dict[str, str]:
        results: dict[str, str] = {}
        if table is None:
            return results
        for row in table.find_all("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.find_all(["th", "td"])]
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
    def _first_value(values: dict[str, str], labels) -> str | None:
        for label in labels:
            key = NesCartDb._normalize_label(label)
            if key in values:
                return values[key]
        return None

    @staticmethod
    def _parse_video_system(region_value: str | None) -> str | None:
        if not region_value:
            return None
        m = re.search(r"\b(NTSC|PAL)\b", region_value, re.IGNORECASE)
        if m:
            return m.group(1).upper()
        return None

    @staticmethod
    def _parse_int(value: str | None) -> int | None:
        if not value:
            return None
        m = re.search(r"(\d+)", value.replace(",", ""))
        if not m:
            return None
        return int(m.group(1))

    @staticmethod
    def _parse_size(value: str | None) -> int | None:
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
    def _parse_yes_no(value: str | None) -> int | None:
        if not value:
            return None
        normalized = value.strip().lower()
        if normalized in {"yes", "y", "true"}:
            return 1
        if normalized in {"no", "n", "false"}:
            return 0
        return None

    @staticmethod
    def _match_non_standard_controller(
        rom_id: int,
        value: str,
    ) -> int | None:
        normalized = value.strip().lower()
        if "4-player adapter" in normalized:
            # Smash T.V. is double fisted, which implies a four score
            if rom_id in [927, 1306, 2025]:
                return ControllerType.DOUBLE_FISTED.value
            # Famicom variants of four score
            elif rom_id in [2163, 2236, 3601]:
                return ControllerType.FAMICOM_FOUR_PLAYERS_SIMPLE.value
            return ControllerType.NES_FOUR_SCORE.value
        elif "zapper light gun" in normalized:
            return ControllerType.ZAPPER_4017.value
        # 525, 600, 780, 910, 928 should be B not A
        elif "power pad" in normalized or "family fun fitness mat" in normalized:
            # Only Athletic world (525) and Street Cop (928) used B side
            if rom_id in [525, 928]:
                return ControllerType.POWER_PAD_SIDE_B.value
            return ControllerType.POWER_PAD_SIDE_A.value
        # All Japaneese titles used the A side of the mat
        elif "family trainer mat" in normalized:
            # Special case with the Pokkun Moguraa whack-a-mole mat and mole mallet
            if rom_id == 4686:
                return ControllerType.POKKUN_MOGURAA_TAP_MAT.value
            return ControllerType.FAMILY_TRAINER_SIDE_A.value
        # ROB is one and the same hardware but need to patch as they differ in nes20db
        elif "r. o. b." in normalized:
            # Gyromite ROB
            if rom_id in [266, 584, 785, 1286, 2789, 2788, 2354, 4063, 4423, 4524, 4542, 4543]:
                return ControllerType.ROB_GYROMITE.value
            # Stack Up ROB
            else:
                return ControllerType.ROB_STACK_UP.value
        elif "3-d glasses" in normalized:
            return ControllerType.FAMICOM_3D_SYSTEM.value
        elif "power glove" in normalized:
            return ControllerType.POWER_GLOVE.value
        elif "vaus controller" in normalized:
            # Famicom version
            if rom_id == 1757:
                return ControllerType.ARKANOID_VAUS_FAMICOM.value
            return ControllerType.ARKANOID_VAUS_NES.value
        elif "miracle piano" in normalized:
            return ControllerType.MIRACLE_PIANO.value
        elif "aladdin deck enhancer" in normalized:
            return ControllerType.ALADDIN_DECK_ENHANCER.value
        elif "barcode battler" in normalized:
            return ControllerType.SUNSOFT_BARCODE_BATTLER.value
        elif "top rider bike" in normalized:
            return ControllerType.TOP_RIDER.value
        elif "konami hypershot" in normalized:
            return ControllerType.KONAMI_HYPER_SHOT.value
        elif "mahjong controller" in normalized:
            return ControllerType.JISSEN_MAHJONG.value
        elif "battle box" in normalized:
            return ControllerType.IGS_STORAGE_BATTLE_BOX.value
        elif "racermate bike" in normalized:
            return ControllerType.RACERMATE_BICYCLE.value
        elif "family keyboard" in normalized:
            return ControllerType.FAMILY_BASIC_KEYBOARD_RECORDER.value
        elif "party tap" in normalized:
            return ControllerType.YONEZAWA_PARTY_TAP.value
        elif "oeka kids tablet" in normalized:
            return ControllerType.OEKA_KIDS_TABLET.value
        elif "pachinko controller" in normalized:
            return ControllerType.COCONUTS_PACHINKO.value
        elif "u-force" in normalized:
            return ControllerType.U_FORCE.value
        return None

    @staticmethod
    def _parse_periphereals(rom_id: int, value: str) -> int | None:
        # If there are more than one value, one is bound to be Famicom/NES controller,
        # so ignore that. Accommodate for an empty value bug on the web page
        value = ",".join(v.strip() for v in value.split(",") if v.strip())
        if "," in value:
            for val in value.split(","):
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

    def _build_result(self, rom_id: int, html: str) -> dict[str, str] | None:
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
            game_name = title_tag.text.split(" - ")[0].strip()
        h1_tag = soup.find("h1")
        if h1_tag and h1_tag.text:
            game_name = h1_tag.text.strip()

        result: dict[str, str] = {}
        if game_name:
            result[RomDbKey.NAME.value] = game_name
        if rom_details.get("crc"):
            result[RomDbKey.CRC.value] = rom_details.get("crc")
        region_text = self._first_value(kv, ["Region"])
        video_system = self._parse_video_system(region_text)
        if video_system == "PAL":
            result[RomDbKey.HARDWARE.value] = HardwareType.NES_PAL.value
        else:
            hw = hardware_from_console_type_and_region("0", "0", country=region_text)
            result[RomDbKey.HARDWARE.value] = hw if hw is not None else HardwareType.NES_NTSC.value
        mapper = self._first_value(kv, ["iNES Mapper", "Mapper"])
        submapper = self._first_value(kv, ["Submapper", "SubMapper"])
        chr_ram = self._first_value(kv, ["CHR RAM", "CHR-RAM", "VRAM"])
        work_ram = self._first_value(kv, ["WRAM", "Work RAM"])
        eeprom_size = self._parse_eeprom_size_from_chip_info(chip_info)
        batt = self._first_value(kv, ["Battery present", "Battery"])
        # In iNES 2.0, battery means "battery or other non-volatile memory"
        if eeprom_size is not None and eeprom_size > 0:
            batt = "1"
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
                if eeprom_size is not None:
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

    def _patch(self, rom_id: int, record: dict[str, str] | None) -> None:
        """Apply hardcoded patches for known bad/missing data."""
        # Startropics I and II (41, 814, 1896, 2449, 2769, 2780, 4171, 4365) has a 1kB PRG RAM in
        # the MMC6 chip.
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
        # Kyonshiizu 2 (ROM_ID 1559, 3151, 3152, 3153, 3300) has a X1-005 with internal Save RAM
        # Taito Grand Prix: Eikou e no License (ROM_ID 1758) too
        # Fudou Myouou Den (ROM_ID 1762) too
        # Mirai Shinwa Jarvas (ROM_ID 1763, 3163) too
        # Kyuukyoku Harikiri Stadium (ROM_IDs 1765, 1766, 3071, 3147, 3148, 3149, 3150, 3303) too
        # Yamamura Misa Suspense: Kyouto Ryuu no Tera Satsujin Jiken (3954) too
        # Minelvaton Saga: Ragon no Fukkatsu (3955) too
        if rom_id in [
            1559,
            3151,
            3152,
            3153,
            3300,
            1758,
            1762,
            1763,
            3163,
            1765,
            1766,
            3071,
            3147,
            3148,
            3149,
            3150,
            3303,
            3954,
            3955,
        ]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 128
        # Kyuukyoku Harikiri Stadium: Heisei Gannen Ban (1767, 2254, 3161) has a Taito X1-017 with
        # Save RAM
        # Kyuukyoku Harikiri Koushien (3956) too
        # Kyuukyoku Harikiri Stadium III (3961) too
        # SD Keiji: Blader (3989) too
        if rom_id in [1767, 2254, 3161, 3956, 3961, 3989]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 1024
        # Famicom Jump II: Saikyou no 7 Nin (ROM_ID 1734) is mapper 153 (not 16) as PRG-ROM
        # is larger than 128 kB
        if rom_id in [1734, 3280]:
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
        # Galactic Crusader (2949) could be either mapper 79 or 146. Both will work.
        # Choosing 79 as 146 for a non-switching 8kB ROMs seems odd
        if rom_id == 2949:
            record[RomDbKey.MAPPER.value] = 79
        # Solstics have different names in different languages, but with same CRC.
        # Align on English.
        if rom_id in [1617, 1998, 2018]:
            record[RomDbKey.NAME.value] = "Solstice: The Quest for the Staff of Demnos"
        # The same for Solar Jetman
        if rom_id in [2153, 2653]:
            record[RomDbKey.NAME.value] = "Solar Jetman: Hunt for the Golden Warpship"
        # The same for Die Schlümpfe (The Smurfs)
        if rom_id in [1615, 1933, 2121]:
            record[RomDbKey.NAME.value] = "The Smurfs"
        # The same for Arch Rivals: A Basketbrawl!
        if rom_id in [1301, 2403, 4435, 4492]:
            record[RomDbKey.NAME.value] = "Arch Rivals: A Basketbrawl!"
        # And The Lion King
        if rom_id in [2037, 2704, 2886, 4500]:
            record[RomDbKey.NAME.value] = "Disney's The Lion King"
        # and The Jungle Book
        if rom_id in [1931, 4465]:
            record[RomDbKey.NAME.value] = "Disney's The Jungle Book"
        # and War in the Gulf
        if rom_id == 4247:
            record[RomDbKey.NAME.value] = "War In The Gulf"
        # and The Hunt for Red October
        if rom_id in [4416, 4444, 4445]:
            record[RomDbKey.NAME.value] = "The Hunt for Red October"
        # Stack-up and Block Set are the same game, but with different names in different regions.
        # Align on Stack-Up as it's more common
        if RomDbKey.NAME.value in record and record[RomDbKey.NAME.value] == "Block Set":
            record[RomDbKey.NAME.value] = "Stack-Up"
        # Bakushou!! Jinsei Gekijou 2 (1764) has a X1-005 chip which means mapper 33
        if rom_id == 1764:
            record[RomDbKey.MAPPER.value] = 33
        # The Money Game (3458) Uses MMC1A, which is mapper 155, not 1
        # Tatakae!! Ramen Man: Sakuretsu Choujin 102 Gei (3736) too
        if rom_id in [3458, 3736]:
            record[RomDbKey.MAPPER.value] = 155
        # Advanced Dungeons & Dragons Heroes of the Lance Prototype (4625) has a battery (PCB image)
        # Taro's Quest Prototype (4639) too
        # Thomas The Tank Engine & Friends Prototype (4640) too
        if rom_id in [4625, 4639, 4640]:
            record[RomDbKey.BATTERY.value] = 1
        # Days of Thunder and its Protoype have the same CRC. Align on non-prototype.
        if rom_id in [4729]:
            record[RomDbKey.BATTERY.value] = 0
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 0
        # Same for Where in Time is Carmen Sandiego?
        if rom_id in [4642]:
            record[RomDbKey.BATTERY.value] = 0
        # Japaneese version of Gyromite is called Gyro, but has the same CRC. Align on Gyromite.
        if rom_id == 4063:
            record[RomDbKey.NAME.value] = "Gyromite"
        # One of The Simpsons:  Bart vs. The Space Mutants is spelled with a small t in 'the'
        if rom_id == 4394:
            record[RomDbKey.NAME.value] = "The Simpsons:  Bart vs. The Space Mutants"
        # Same game different names in different regions. Align on the more common name,
        # which is the English one.
        if rom_id in [4424, 4461]:
            record[RomDbKey.NAME.value] = "Goal! 2"

    def next_record(self) -> dict[str, str] | None:
        """Fetch and return the next parsed profile record, or None if done."""
        record = None
        while record is None:
            try:
                rom_id = next(self._ids_iter)
            except StopIteration:
                return None

            url = self._base_url.format(rom_id)
            html = self._fetch_html_with_cache(rom_id, url)
            record = self._build_result(rom_id, html)
            if record is not None:
                self._patch(rom_id, record)
            self._remaining -= 1
        return record
