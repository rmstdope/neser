""" Stream parser for ROM XML files """
import xml.etree.ElementTree as ET
from typing import Dict, Optional
from rom_database import RomDbKey


class RomXml:
    """
    Stream parser for ROM XML files. Initialize with a filename and call
    `next_record()` to get the next parsed record as a dict. Returns `None`
    when the stream is exhausted.
    """

    def __init__(self, filename: str):
        """Initialize the stream parser for the given XML file.

        Args:
            filename: Path to the XML file containing <game> entries.
        """
        self._filename = filename
        # Use iterparse to stream <game> elements
        parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
        self._context = ET.iterparse(filename, events=("end",), parser=parser)
        self._iterator = iter(self._context)
        self._remaining = self._count_games()

    def _count_games(self) -> int:
        parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
        context = ET.iterparse(self._filename, events=("end",), parser=parser)
        count = 0
        for _event, elem in context:
            if elem.tag.lower() == "game":
                count += 1
                elem.clear()
        return count

    @staticmethod
    def _normalize_label(text: Optional[str]) -> str:
        """Normalize a label string by trimming whitespace and trailing colons.

        Returns an empty string for falsy input.
        """
        if not text:
            return ""
        return " ".join(text.strip().rstrip(":").split())

    def _parse_game_element(self, game_elem) -> Dict[str, str]:
        """Extract relevant fields from a single <game> element.

        Only present fields are added to the returned dict.
        """
        data: Dict[str, str] = {}

        for node in game_elem.iter():
            if node.tag is ET.Comment and node.text:
                data[RomDbKey.CONSOLE_CLASS.value] = node.text.split('\\')[0].strip()
                break

        prgrom = game_elem.find("prgrom")
        if prgrom is not None:
            prg_size = prgrom.get("size")
            if prg_size:
                data[RomDbKey.PRG_ROM_SIZE.value] = prg_size
            prg_crc = prgrom.get("crc32")
            if prg_crc:
                data[RomDbKey.PRG_ROM_CRC.value] = prg_crc.upper()
        else:
            data[RomDbKey.PRG_ROM_SIZE.value] = "0"

        chrrom = game_elem.find("chrrom")
        if chrrom is not None:
            chr_size = chrrom.get("size")
            if chr_size:
                data[RomDbKey.CHR_ROM_SIZE.value] = chr_size
            chr_crc = chrrom.get("crc32")
            if chr_crc:
                data[RomDbKey.CHR_ROM_CRC.value] = chr_crc.upper()
        else:
            data[RomDbKey.CHR_ROM_SIZE.value] = "0"

        prgnvram = game_elem.find("prgnvram")
        if prgnvram is not None:
            size = prgnvram.get("size")
            if size:
                data[RomDbKey.PRG_NVRAM_SIZE.value] = size
        else:
            data[RomDbKey.PRG_NVRAM_SIZE.value] = "0"

        prgram = game_elem.find("prgram")
        if prgram is not None:
            size = prgram.get("size")
            if size:
                data[RomDbKey.PRG_RAM_SIZE.value] = size
        else:
            data[RomDbKey.PRG_RAM_SIZE.value] = "0"

        chrnvram = game_elem.find("chrnvram")
        if chrnvram is not None:
            size = chrnvram.get("size")
            if size:
                data[RomDbKey.CHR_NVRAM_SIZE.value] = size
        else:
            data[RomDbKey.CHR_NVRAM_SIZE.value] = "0"

        chrram = game_elem.find("chrram")
        if chrram is not None:
            size = chrram.get("size")
            if size:
                data[RomDbKey.CHR_RAM_SIZE.value] = size
        else:
            data[RomDbKey.CHR_RAM_SIZE.value] = "0"

        rom = game_elem.find("rom")
        if rom is not None:
            crc = rom.get("crc32")
            if crc:
                data[RomDbKey.CRC.value] = crc.upper()

        pcb = game_elem.find("pcb")
        if pcb is not None:
            mapper = pcb.get("mapper")
            if mapper:
                data[RomDbKey.MAPPER.value] = mapper
            submapper = pcb.get("submapper")
            if submapper:
                data[RomDbKey.SUBMAPPER.value] = submapper
            mir = pcb.get("mirroring")
            if mir:
                data[RomDbKey.NAMETABLE_LAYOUT.value] = mir
            battery = pcb.get("battery")
            if battery:
                data[RomDbKey.BATTERY.value] = battery

        console = game_elem.find("console")
        if console is not None:
            console_type = console.get("type")
            if console_type:
                data[RomDbKey.CONSOLE_TYPE.value] = console_type
            region = console.get("region")
            if region:
                data[RomDbKey.CONSOLE_REGION.value] = region

        expansion = game_elem.find("expansion")
        if expansion is not None:
            expansion_type = expansion.get("type")
            if expansion_type:
                data[RomDbKey.EXPANSION_TYPE.value] = expansion_type

        vs = game_elem.find("vs")
        if vs is not None:
            vs_hardware = vs.get("hardware")
            if vs_hardware:
                data[RomDbKey.VS_HARDWARE_TYPE.value] = vs_hardware
            vs_ppu = vs.get("ppu")
            if vs_ppu:
                data[RomDbKey.VS_PPU_TYPE.value] = vs_ppu

        return data

    def num_left(self) -> int:
        """Return the number of records left to parse."""
        return self._remaining

    def _patch(self, record: Dict[str, str]) -> None:
        """Apply hardcoded patches for known bad/missing data."""
        crc = record.get(RomDbKey.CRC.value)
        if not crc:
            return
        # Gauntlet (USA) with CRCs EC968C51 and CD50A092 should have 2kB VRAM according to component
        # list on nescart
        # TODO Try the actual ROM
        if crc in ["EC968C51", "CD50A092"]:
            record[RomDbKey.CHR_RAM_SIZE.value] = 2048
        # Tetris (343C7BB0) is a mapper 3, not 148 according to component list
        # on nescart
        if crc == "343C7BB0":
            record[RomDbKey.MAPPER.value] = 3
        # The following titles have EEPROM and hence no battery
        # Dragon Ball Z III: Ressen Jinzou Ningen (DC52BF0C)
        # Dragon Ball Z Gaiden: Saiyajin Zetsumetsu Keikaku (136CA449)
        # Famicom Jump II: Saikyou no 7 Nin (E170404C)
        # Magical Taruruuto-kun: Fantastic World!! (Version 2.0) (DCB972CE, 0CF42E69)
        # Dragon Ball Z II: Gekishin Freeza!! (Version 2.0) (A9541452, 99240573)
        # SD Gundam Gaiden: Knight Gundam Monogatari 2: Hikari no Kishi (B049A8C4)
        # SD Gundam Gaiden: Knight Gundam Monogatari 3: Densetsu no Kishi Dan (C2840372)
        # Dragon Ball Z: Kyoushuu! Saiyajin (183859D2)
        # SD Gundam Gaiden: Knight Gundam Monogatari (Version 2.0) (276AC722)
        if crc in ["DC52BF0C", "136CA449", "E170404C", "DCB972CE", "0CF42E69",
                "A9541452", "99240573", "B049A8C4", "C2840372", "183859D2", "276AC722"]:
            record[RomDbKey.BATTERY.value] = 0
        # Dokuganryuu Masamune (10C8F2FA) as 8kB of PRGM NVRAM according to component list
        # on nescart
        if crc == "10C8F2FA":
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 8192
        # Kyuukyoku Harikiri Stadium: Heisei Gannen Ban (0BBF80CB) has a X1-017 with 1kB Save RAM
        if crc == "0BBF80CB":
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 1024
        # Bakushou!! Jinsei Gekijou 2 (BC7B1D0F) has a X1-005 chip which means mapper 80
        if crc == "BC7B1D0F":
            record[RomDbKey.MAPPER.value] = 80
        # Rad Racer II (404B2E8B) has a 8kB VRAM chip, but likely not all address lines connected
        if crc == "404B2E8B":
            record[RomDbKey.CHR_RAM_SIZE.value] = 4096

    def next_record(self) -> Optional[Dict[str, str]]:
        """Return the next parsed game record dict, or None if finished."""
        for _event, elem in self._iterator:
            # Looking for end events on <game>
            if elem.tag.lower() == "game":
                record = self._parse_game_element(elem)
                self._patch(record)
                # Clear element to free memory
                elem.clear()
                if self._remaining > 0:
                    self._remaining -= 1
                return record
        return None
