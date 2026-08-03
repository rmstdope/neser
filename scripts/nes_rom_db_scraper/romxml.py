""" Stream parser for ROM XML files """
import xml.etree.ElementTree as ET
from typing import Dict, Optional


from scripts.nes_rom_db_scraper.rom_database import (
    ControllerType,
    HardwareType,
    RomDbKey,
    hardware_from_console_type_and_region,
)


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
            region = console.get("region")
            rom_class = data.get(RomDbKey.CONSOLE_CLASS.value)
            hw = hardware_from_console_type_and_region(console_type, region, country=rom_class)
            if hw is not None:
                data[RomDbKey.HARDWARE.value] = str(hw)

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
        if crc in ["EC968C51", "CD50A092"]:
            record[RomDbKey.CHR_RAM_SIZE.value] = 2048
        # Tetris (343C7BB0) is a mapper 3, not 148 according to component list
        # on nescart
        if crc == "343C7BB0":
            record[RomDbKey.MAPPER.value] = 3
        # Volley Ball (A23CB659) is Mapper 79 (discrete 74xx‑based unlicensed board), not Mapper 36
        if crc == "A23CB659":
            record[RomDbKey.MAPPER.value] = 79
        # Dokuganryuu Masamune (10C8F2FA) as 8kB of PRG NVRAM according to component list
        # on nescart
        if crc == "10C8F2FA":
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 8192
        # Superman Prototype also have 8kB of PRG RAM, but the battery slot is not populated
        # according to PCB images, so we won't set the battery flag
        if crc == "47F7F860":
            record[RomDbKey.PRG_RAM_SIZE.value] = 8192
        # Kyuukyoku Harikiri Stadium: Heisei Gannen Ban (0BBF80CB) has a X1-017 with 1kB Save RAM
        # Kyuukyoku Harikiri Stadium III (2BB3DABE) too
        # Kyuukyoku Harikiri Koushien (8CA72D80) too
        # SD Keiji: Blader (05F04EAC) too
        if crc in ["0BBF80CB", "2BB3DABE", "8CA72D80", "05F04EAC"]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 1024
        # Rad Racer II (404B2E8B) has a 8kB VRAM chip, but likely not all address lines connected
        if crc == "404B2E8B":
            record[RomDbKey.CHR_RAM_SIZE.value] = 4096
        # These games used side A of the Power PAd
        # Super Team Games (D74B2719)
        # World Class Track Meet (5734EB9E, AF4010EA)
        # Dance Aerobics (9E382EBF)
        # Stadium Events (FCE71311, 0DA28A50)
        if crc in ["D74B2719", "5734EB9E", "AF4010EA", "9E382EBF", "FCE71311", "0DA28A50"]:
            record[RomDbKey.EXPANSION_TYPE.value] = ControllerType.POWER_PAD_SIDE_A
        # No Japanese titles ever used side B of the Family Trainer Mat
        if record.get(RomDbKey.EXPANSION_TYPE.value) == \
            str(ControllerType.FAMILY_TRAINER_SIDE_B.value):
            record[RomDbKey.EXPANSION_TYPE.value] = ControllerType.FAMILY_TRAINER_SIDE_A.value
        # Quattro Sports (CCCAF368) did not use four score, but needed an Aladdin Deck enhancer
        if crc == "CCCAF368":
            record[RomDbKey.EXPANSION_TYPE.value] = ControllerType.ALADDIN_DECK_ENHANCER
        # Star Wars Proptotype (B30599A1) had a battery (PCB image)
        # Thomas The Tank Engine & Friends Prototype (E46AEE21) too
        if crc in ["B30599A1", "E46AEE21"]:
            record[RomDbKey.BATTERY.value] = 1
        # Thomas The Tank Engine & Friends Prototype (E46AEE21) also had 8kB PRG NVRAM
        if crc in ["E46AEE21"]:
            record[RomDbKey.PRG_NVRAM_SIZE.value] = 8192
        # Same CRC but different regions. Align on multi-region
        if crc in ["638DBC52", "C4C3949A"]:
            record[RomDbKey.HARDWARE.value] = HardwareType.NES_MULTI_REGION.value

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
