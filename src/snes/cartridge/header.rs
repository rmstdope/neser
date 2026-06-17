use crate::snes::cartridge::mapping::Mapping;

pub(crate) const TITLE_LEN: usize = 21;
pub(crate) const HEADER_MODE_OFFSET: usize = 0xD5;
pub(crate) const HEADER_CHIPSET_OFFSET: usize = 0xD6;
pub(crate) const HEADER_ROM_SIZE_OFFSET: usize = 0xD7;
pub(crate) const HEADER_RAM_SIZE_OFFSET: usize = 0xD8;
pub(crate) const HEADER_COUNTRY_OFFSET: usize = 0xD9;
pub(crate) const HEADER_DEVELOPER_OFFSET: usize = 0xDA;
pub(crate) const HEADER_VERSION_OFFSET: usize = 0xDB;
pub(crate) const HEADER_CHECKSUM_COMPLEMENT_OFFSET: usize = 0xDC;
pub(crate) const HEADER_CHECKSUM_OFFSET: usize = 0xDE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnesHeader {
    pub title: String,
    pub map_mode: u8,
    pub chipset: u8,
    pub rom_size_field: u8,
    pub ram_size_field: u8,
    pub country: u8,
    pub developer_id: u8,
    pub version: u8,
    pub checksum_complement: u16,
    pub checksum: u16,
    pub mapping: Mapping,
}

pub(crate) fn parse_header_at(
    rom: &[u8],
    mapping: Mapping,
    header_offset: usize,
) -> Option<SnesHeader> {
    let header_end = header_offset.checked_add(0x100)?;
    if header_end > rom.len() {
        return None;
    }

    let title_bytes = &rom[header_offset..header_offset + TITLE_LEN];
    let title = decode_title(title_bytes);

    Some(SnesHeader {
        title,
        map_mode: rom[header_offset + HEADER_MODE_OFFSET],
        chipset: rom[header_offset + HEADER_CHIPSET_OFFSET],
        rom_size_field: rom[header_offset + HEADER_ROM_SIZE_OFFSET],
        ram_size_field: rom[header_offset + HEADER_RAM_SIZE_OFFSET],
        country: rom[header_offset + HEADER_COUNTRY_OFFSET],
        developer_id: rom[header_offset + HEADER_DEVELOPER_OFFSET],
        version: rom[header_offset + HEADER_VERSION_OFFSET],
        checksum_complement: read_u16_le(rom, header_offset + HEADER_CHECKSUM_COMPLEMENT_OFFSET),
        checksum: read_u16_le(rom, header_offset + HEADER_CHECKSUM_OFFSET),
        mapping,
    })
}

fn decode_title(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    raw.trim_end_matches(['\0', ' ']).to_string()
}

fn read_u16_le(rom: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([rom[offset], rom[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_trims_trailing_spaces_and_nuls_from_title() {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        let title = b"GAME TITLE   \0\0\0\0\0\0\0\0";
        rom[base..base + TITLE_LEN].copy_from_slice(&title[..TITLE_LEN]);

        let parsed = parse_header_at(&rom, Mapping::LoRom, base).expect("header");
        assert_eq!(parsed.title, "GAME TITLE");
    }

    #[test]
    fn parse_header_reads_checksum_fields() {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        rom[base + HEADER_CHECKSUM_COMPLEMENT_OFFSET] = 0x34;
        rom[base + HEADER_CHECKSUM_COMPLEMENT_OFFSET + 1] = 0x12;
        rom[base + HEADER_CHECKSUM_OFFSET] = 0xCD;
        rom[base + HEADER_CHECKSUM_OFFSET + 1] = 0xAB;

        let parsed = parse_header_at(&rom, Mapping::LoRom, base).expect("header");
        assert_eq!(parsed.checksum_complement, 0x1234);
        assert_eq!(parsed.checksum, 0xABCD);
    }
}
