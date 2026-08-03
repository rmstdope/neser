
"""
TestRomXml: Unit tests for the RomXml streaming XML parser.

This module provides a suite of tests for the RomXml class, verifying correct parsing
of NES ROM XML files, including handling of all relevant cartridge metadata fields.
"""
import os
import tempfile
import unittest
from scripts.nes_rom_db_scraper.rom_database import RomDbKey
from scripts.nes_rom_db_scraper.romxml import RomXml

class TestRomXml(unittest.TestCase):
    """
    Unit tests for the RomXml streaming XML parser.
    """
    def _write_xml(self, xml_text: str) -> str:
        """
        Write the given XML string to a temporary file.

        Args:
            xml_text: The XML content to write.

        Returns:
            str: Path to the temporary XML file.
        """
        fd, path = tempfile.mkstemp(suffix=".xml")
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(xml_text)
        return path
    def test_next_record_parses_fields_and_uppercases_crc(self) -> None:
        """
        Test that RomXml parses fields and uppercases CRC values.
        """
        xml_text = """
        <database>
            <game>
            <rom crc32="a1b2c3d4" />
            <pcb mirroring="horizontal" />
            <console region="PAL" />
            </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(
            record,
            {
                RomDbKey.CRC.value: "A1B2C3D4",
                RomDbKey.NAMETABLE_LAYOUT.value: "horizontal",
                RomDbKey.HARDWARE.value: "1",
                RomDbKey.PRG_ROM_SIZE.value: "0",
                RomDbKey.PRG_RAM_SIZE.value: "0",
                RomDbKey.PRG_NVRAM_SIZE.value: "0",
                RomDbKey.CHR_ROM_SIZE.value: "0",
                RomDbKey.CHR_RAM_SIZE.value: "0",
                RomDbKey.CHR_NVRAM_SIZE.value: "0"
            }
        )

    def test_next_record_handles_missing_fields(self) -> None:
        """
        Test that RomXml handles missing fields gracefully.
        """
        xml_text = """
        <database>
          <game>
            <rom />
            <pcb />
            <console />
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(record, {
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"
        })

    def test_next_record_multiple_games(self) -> None:
        """
        Test that RomXml parses multiple <game> elements in sequence.
        """
        xml_text = """
        <database>
          <game>
            <rom crc32="00000001" />
          </game>
          <game>
            <rom crc32="00000002" />
            <pcb mirroring="vertical" />
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            first = parser.next_record()
            second = parser.next_record()
            third = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(first, {
            RomDbKey.CRC.value: "00000001",
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"
        })
        self.assertEqual(second, {
            RomDbKey.CRC.value: "00000002", 
            RomDbKey.NAMETABLE_LAYOUT.value: "vertical",
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"
        })
        self.assertIsNone(third)

    def test_next_record_ignores_non_game_elements(self) -> None:
        """
        Test that RomXml ignores non-<game> elements in the XML.
        """
        xml_text = """
        <database>
          <header version="1" />
          <game>
            <rom crc32="deadbeef" />
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(record, {
            RomDbKey.CRC.value: "DEADBEEF",
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"
        })

    def test_next_record_missing_crc_is_omitted(self) -> None:
        """
        Test that RomXml omits records missing CRC values.
        """
        xml_text = """
        <database>
          <game>
            <rom size="16" />
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(record, {
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"            
        })

    def test_next_record_parses_extended_rom_info(self) -> None:
        """
        Test that RomXml parses all extended cartridge metadata fields from XML.
        """
        xml_text = """
        <database>
          <game>
            <!--  Licensed Japan 10-Yard Fight (rev0).nes  -->
            <prgrom size="16384" crc32="D3D248C9" sha1="64185EDC4FD64B5F5E565B90B0DDC241592D899C" sum16="0339"/>
            <chrrom size="8192" crc32="9C124A53" sha1="BEDDF6A65A1A72410CFB1208E8B6B6E0CF5B2E74" sum16="16E5"/>
            <rom size="24576" crc32="836C4FA7" sha1="55DC03A493150258E10166CF38ED76DFADE605D6"/>
            <chrnvram size="8192"/>
            <chrram size="8192"/>
            <prgnvram size="8192"/>
            <prgram size="8192"/>
            <pcb mapper="0" submapper="0" mirroring="H" battery="0"/>
            <console type="0" region="0"/>
            <expansion type="1"/>
            <vs hardware="0" ppu="5"/>
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(
            record,
            {
                RomDbKey.CRC.value: "836C4FA7",
                RomDbKey.PRG_ROM_SIZE.value: "16384",
                RomDbKey.PRG_ROM_CRC.value: "D3D248C9",
                RomDbKey.PRG_NVRAM_SIZE.value: "8192",
                RomDbKey.PRG_RAM_SIZE.value: "8192",
                RomDbKey.CHR_ROM_SIZE.value: "8192",
                RomDbKey.CHR_ROM_CRC.value: "9C124A53",
                RomDbKey.CHR_NVRAM_SIZE.value: "8192",
                RomDbKey.CHR_RAM_SIZE.value: "8192",
                RomDbKey.MAPPER.value: "0",
                RomDbKey.SUBMAPPER.value: "0",
                RomDbKey.NAMETABLE_LAYOUT.value: "H",
                RomDbKey.BATTERY.value: "0",
                RomDbKey.HARDWARE.value: "2",
                RomDbKey.EXPANSION_TYPE.value: "1",
                RomDbKey.VS_HARDWARE_TYPE.value: "0",
                RomDbKey.VS_PPU_TYPE.value: "5",
                RomDbKey.CONSOLE_CLASS.value: "Licensed Japan 10-Yard Fight (rev0).nes"
            },
        )

    def test_next_record_parses_extended_rom_info_missing_optional(self) -> None:
        """
        Test that RomXml handles missing optional fields in extended XML.
        """
        xml_text = """
        <database>
          <game>
            <rom crc32="1234abcd" />
            <pcb mapper="2" />
          </game>
        </database>
        """
        path = self._write_xml(xml_text)
        try:
            parser = RomXml(path)
            record = parser.next_record()
        finally:
            os.unlink(path)

        self.assertEqual(record, {
            RomDbKey.CRC.value: "1234ABCD",
            RomDbKey.MAPPER.value: "2",
            RomDbKey.PRG_ROM_SIZE.value: "0",
            RomDbKey.PRG_RAM_SIZE.value: "0",
            RomDbKey.PRG_NVRAM_SIZE.value: "0",
            RomDbKey.CHR_ROM_SIZE.value: "0",
            RomDbKey.CHR_RAM_SIZE.value: "0",
            RomDbKey.CHR_NVRAM_SIZE.value: "0"
        })

    def test_normalize_label(self) -> None:
        """
        Test the _normalize_label static method for label normalization.
        """
        self.assertEqual(RomXml._normalize_label(None), "")
        self.assertEqual(RomXml._normalize_label("  A  B  "), "A B")
        self.assertEqual(RomXml._normalize_label("Label:"), "Label")


if __name__ == "__main__":
    unittest.main()
