use crate::snes::cartridge::header::parse_header_at;
use crate::snes::cartridge::mapping::{Mapping, detect_mapping};
const MIN_LOROM_HEADER_END: usize = 0x80C0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomSpeed {
    Slow,
    Fast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    TooShort,
    HeaderNotFound,
}

pub struct Cartridge {
    rom: Vec<u8>,
    mapping: Mapping,
    sram_size: usize,
    has_battery: bool,
    speed: RomSpeed,
    title: String,
    country: u8,
}

impl Cartridge {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CartridgeError> {
        if bytes.is_empty() {
            return Err(CartridgeError::TooShort);
        }

        let stripped = strip_copier_header(bytes);
        if stripped.len() < MIN_LOROM_HEADER_END {
            return Err(CartridgeError::TooShort);
        }

        let candidate = detect_mapping(stripped).ok_or(CartridgeError::HeaderNotFound)?;
        let header = parse_header_at(stripped, candidate.mapping, candidate.header_offset)
            .ok_or(CartridgeError::HeaderNotFound)?;

        Ok(Self {
            rom: stripped.to_vec(),
            mapping: candidate.mapping,
            sram_size: decode_sram_size(header.ram_size_field),
            has_battery: has_battery(header.chipset),
            speed: if header.map_mode & 0x10 != 0 {
                RomSpeed::Fast
            } else {
                RomSpeed::Slow
            },
            title: header.title,
            country: header.country,
        })
    }

    pub fn mapping(&self) -> Mapping {
        self.mapping
    }

    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    pub fn sram_size(&self) -> usize {
        self.sram_size
    }

    pub fn has_battery(&self) -> bool {
        self.has_battery
    }

    pub fn speed(&self) -> RomSpeed {
        self.speed
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn country(&self) -> u8 {
        self.country
    }
}

fn strip_copier_header(bytes: &[u8]) -> &[u8] {
    if bytes.len() > 0x200 && bytes.len() % 0x400 == 0x200 {
        &bytes[0x200..]
    } else {
        bytes
    }
}

fn decode_sram_size(ram_size_field: u8) -> usize {
    if ram_size_field == 0 {
        return 0;
    }

    let exp = usize::from(ram_size_field);
    let Some(kib) = 1usize.checked_shl(exp as u32) else {
        return 0;
    };
    let size = kib.saturating_mul(1024);
    if size > 1024 * 1024 { 0 } else { size }
}

fn has_battery(chipset: u8) -> bool {
    matches!(chipset & 0x0F, 0x2 | 0x5 | 0x6 | 0x9 | 0xA | 0xD | 0xE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_header(
        rom: &mut [u8],
        base: usize,
        mode: u8,
        chipset: u8,
        ram_size_field: u8,
        title: &[u8],
    ) {
        rom[base..base + title.len()].copy_from_slice(title);
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = mode;
        rom[base + 0x16] = chipset;
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = ram_size_field;
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
    }

    #[test]
    fn from_bytes_strips_512_byte_copier_header() {
        let mut rom = vec![0xAA; 0x200 + 0x10000];
        rom[0x200] = 0x55;
        write_header(
            &mut rom,
            0x200 + 0x7FC0,
            0x20,
            0x02,
            0x03,
            b"LOROM TEST         \0\0",
        );

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.rom().len(), 0x10000);
        assert_eq!(cart.rom()[0], 0x55);
    }

    #[test]
    fn from_bytes_detects_hirom_mapping() {
        let mut rom = vec![0u8; 0x20000];
        write_header(
            &mut rom,
            0xFFC0,
            0x21,
            0x00,
            0x00,
            b"HIROM TEST         \0\0",
        );
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0x80;

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
    }

    #[test]
    fn from_bytes_detects_exhirom_mapping() {
        let mut rom = vec![0u8; 0x500000];
        write_header(
            &mut rom,
            0x40FFC0,
            0x35,
            0x00,
            0x00,
            b"EXHIROM TEST       \0\0",
        );
        rom[0x40FFFC] = 0x00;
        rom[0x40FFFD] = 0x80;

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::ExHiRom);
    }

    #[test]
    fn from_bytes_accepts_bad_checksum_complement_when_header_present() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x00,
            0x00,
            b"BAD SUM TEST       \0\0",
        );
        rom[0x7FC0 + 0x1C] = 0x00;
        rom[0x7FC0 + 0x1D] = 0x00;
        rom[0x7FC0 + 0x1E] = 0x00;
        rom[0x7FC0 + 0x1F] = 0x00;

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::LoRom);
    }

    #[test]
    fn from_bytes_decodes_sram_size_and_battery_flag() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x02,
            0x05,
            b"SRAM TEST          \0\0",
        );

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.sram_size(), 32 * 1024);
        assert!(cart.has_battery());
    }

    #[test]
    fn from_bytes_title_is_trimmed() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x00,
            0x00,
            b"GAME TITLE   \0\0\0\0\0\0\0\0",
        );

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.title(), "GAME TITLE");
    }

    #[test]
    fn from_bytes_rejects_large_garbage_without_plausible_header() {
        let rom = vec![0u8; 0x500000];
        let err = Cartridge::from_bytes(&rom)
            .err()
            .expect("should reject garbage");
        assert_eq!(err, CartridgeError::HeaderNotFound);
    }

    #[test]
    fn from_bytes_loads_super_mario_kart_rom() {
        let rom = include_bytes!("../../../roms/games/snes/Super Mario Kart (USA).sfc");

        let cart = Cartridge::from_bytes(rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
        assert_eq!(cart.title(), "SUPER MARIO KART");
    }

    #[test]
    fn has_battery_uses_chipset_low_nibble_mapping() {
        assert!(has_battery(0x02));
        assert!(!has_battery(0x03));
    }

    #[test]
    fn decode_sram_size_returns_zero_for_out_of_range_field() {
        assert_eq!(decode_sram_size(32), 0);
    }
}
