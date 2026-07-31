use crate::snes::cartridge::mapping::Mapping;

pub(crate) const TITLE_LEN: usize = 21;
pub(crate) const HEADER_MODE_OFFSET: usize = 0x15;
pub(crate) const HEADER_CHIPSET_OFFSET: usize = 0x16;
pub(crate) const HEADER_ROM_SIZE_OFFSET: usize = 0x17;
pub(crate) const HEADER_RAM_SIZE_OFFSET: usize = 0x18;
pub(crate) const HEADER_COUNTRY_OFFSET: usize = 0x19;
pub(crate) const HEADER_DEVELOPER_OFFSET: usize = 0x1A;
pub(crate) const HEADER_VERSION_OFFSET: usize = 0x1B;
pub(crate) const HEADER_CHECKSUM_COMPLEMENT_OFFSET: usize = 0x1C;
pub(crate) const HEADER_CHECKSUM_OFFSET: usize = 0x1E;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnesHeader {
    pub title: String,
    pub map_mode: u8,
    pub chipset: u8,
    pub chipset_subtype: Option<u8>,
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
    // The SNES header spans 0x40 bytes ending at the interrupt vectors. A
    // 64 KiB HiROM places it flush against the end of the image, so only these
    // 0x40 bytes need to be present.
    let header_end = header_offset.checked_add(0x40)?;
    if header_end > rom.len() {
        return None;
    }

    let title_bytes = &rom[header_offset..header_offset + TITLE_LEN];
    let title = decode_title(title_bytes);
    let map_mode = rom[header_offset + HEADER_MODE_OFFSET];
    let chipset = rom[header_offset + HEADER_CHIPSET_OFFSET];
    let chipset_subtype = header_offset.checked_sub(1).map(|idx| rom[idx]);
    let rom_size_field = rom[header_offset + HEADER_ROM_SIZE_OFFSET];
    let ram_size_field = rom[header_offset + HEADER_RAM_SIZE_OFFSET];
    let country = rom[header_offset + HEADER_COUNTRY_OFFSET];
    let developer_id = rom[header_offset + HEADER_DEVELOPER_OFFSET];
    let version = rom[header_offset + HEADER_VERSION_OFFSET];
    let checksum_complement = u16::from_le_bytes([
        rom[header_offset + HEADER_CHECKSUM_COMPLEMENT_OFFSET],
        rom[header_offset + HEADER_CHECKSUM_COMPLEMENT_OFFSET + 1],
    ]);
    let checksum = u16::from_le_bytes([
        rom[header_offset + HEADER_CHECKSUM_OFFSET],
        rom[header_offset + HEADER_CHECKSUM_OFFSET + 1],
    ]);

    Some(SnesHeader {
        title,
        map_mode,
        chipset,
        chipset_subtype,
        rom_size_field,
        ram_size_field,
        country,
        developer_id,
        version,
        checksum_complement,
        checksum,
        mapping,
    })
}

fn decode_title(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    raw.trim_end_matches(['\0', ' ']).to_string()
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

    /// Writes every SNES-header field with a distinct sentinel so a misread of
    /// any single field fails the assertion (see the mutation proof in the
    /// #2885 RED evidence). `base - 1` is the chipset-subtype byte that
    /// immediately precedes the header.
    fn write_all_fields(rom: &mut [u8], base: usize) {
        let title = b"FULL FIELD TEST      ";
        rom[base..base + TITLE_LEN].copy_from_slice(&title[..TITLE_LEN]);
        rom[base - 1] = 0x5A; // chipset subtype
        rom[base + HEADER_MODE_OFFSET] = 0x30;
        rom[base + HEADER_CHIPSET_OFFSET] = 0x02;
        rom[base + HEADER_ROM_SIZE_OFFSET] = 0x0A;
        rom[base + HEADER_RAM_SIZE_OFFSET] = 0x03;
        rom[base + HEADER_COUNTRY_OFFSET] = 0x0D;
        rom[base + HEADER_DEVELOPER_OFFSET] = 0xC3;
        rom[base + HEADER_VERSION_OFFSET] = 0x07;
        rom[base + HEADER_CHECKSUM_COMPLEMENT_OFFSET] = 0x34;
        rom[base + HEADER_CHECKSUM_COMPLEMENT_OFFSET + 1] = 0x12;
        rom[base + HEADER_CHECKSUM_OFFSET] = 0xCB;
        rom[base + HEADER_CHECKSUM_OFFSET + 1] = 0xED;
    }

    #[test]
    fn parse_header_reads_every_field_at_lorom_offset() {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        write_all_fields(&mut rom, base);

        let parsed = parse_header_at(&rom, Mapping::LoRom, base).expect("header");
        assert_eq!(parsed.title, "FULL FIELD TEST");
        assert_eq!(parsed.map_mode, 0x30);
        assert_eq!(parsed.chipset, 0x02);
        assert_eq!(parsed.chipset_subtype, Some(0x5A));
        assert_eq!(parsed.rom_size_field, 0x0A);
        assert_eq!(parsed.ram_size_field, 0x03);
        assert_eq!(parsed.country, 0x0D);
        assert_eq!(parsed.developer_id, 0xC3);
        assert_eq!(parsed.version, 0x07);
        assert_eq!(parsed.checksum_complement, 0x1234);
        assert_eq!(parsed.checksum, 0xEDCB);
    }

    #[test]
    fn parse_header_reads_every_field_at_hirom_offset() {
        let mut rom = vec![0u8; 0x10000];
        let base = 0xFFC0;
        write_all_fields(&mut rom, base);

        let parsed = parse_header_at(&rom, Mapping::HiRom, base).expect("header");
        assert_eq!(parsed.title, "FULL FIELD TEST");
        assert_eq!(parsed.map_mode, 0x30);
        assert_eq!(parsed.chipset, 0x02);
        assert_eq!(parsed.chipset_subtype, Some(0x5A));
        assert_eq!(parsed.rom_size_field, 0x0A);
        assert_eq!(parsed.ram_size_field, 0x03);
        assert_eq!(parsed.country, 0x0D);
        assert_eq!(parsed.developer_id, 0xC3);
        assert_eq!(parsed.version, 0x07);
        assert_eq!(parsed.checksum_complement, 0x1234);
        assert_eq!(parsed.checksum, 0xEDCB);
    }

    #[test]
    fn parse_header_returns_none_when_header_exceeds_rom_length() {
        // One byte short of the 0x40-byte header at the LoROM offset.
        let rom = vec![0u8; 0x7FC0 + 0x3F];
        assert!(parse_header_at(&rom, Mapping::LoRom, 0x7FC0).is_none());
    }
}
