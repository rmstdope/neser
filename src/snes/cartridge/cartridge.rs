use crate::snes::cartridge::header::parse_header_at;
use crate::snes::cartridge::mapping::{Mapping, detect_mapping};
use std::fmt;
// A 32 KiB single-bank LoROM has its header flush against the end of the
// image (0x7FC0..0x8000), so the pre-detection length guard must only
// require these 0x40 bytes, matching detect_mapping's own per-candidate
// bounds check (mapping.rs).
const MIN_LOROM_HEADER_END: usize = 0x8000;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnhancementChip {
    Dsp,
    SuperFx,
    Obc1,
    Sa1,
    Sdd1,
    Srtc,
    Spc7110,
    St010St011,
    St018,
    Cx4,
    SuperGameBoy,
    Satellaview,
    UnknownCustom { chipset: u8, subtype: Option<u8> },
}

impl fmt::Display for EnhancementChip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dsp => write!(f, "DSP"),
            Self::SuperFx => write!(f, "Super FX / GSU"),
            Self::Obc1 => write!(f, "OBC1"),
            Self::Sa1 => write!(f, "SA-1"),
            Self::Sdd1 => write!(f, "S-DD1"),
            Self::Srtc => write!(f, "S-RTC"),
            Self::Spc7110 => write!(f, "SPC7110"),
            Self::St010St011 => write!(f, "ST010/ST011"),
            Self::St018 => write!(f, "ST018"),
            Self::Cx4 => write!(f, "CX4"),
            Self::SuperGameBoy => write!(f, "Super Game Boy"),
            Self::Satellaview => write!(f, "Satellaview"),
            Self::UnknownCustom { chipset, subtype } => {
                if let Some(subtype) = subtype {
                    write!(
                        f,
                        "unknown custom coprocessor (chipset ${chipset:02X}, subtype ${subtype:02X})"
                    )
                } else {
                    write!(f, "unknown custom coprocessor (chipset ${chipset:02X})")
                }
            }
        }
    }
}

pub struct Cartridge {
    rom: Vec<u8>,
    mapping: Mapping,
    sram_size: usize,
    has_battery: bool,
    speed: RomSpeed,
    title: String,
    country: u8,
    enhancement_chip: Option<EnhancementChip>,
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

        let enhancement_chip = detect_enhancement_chip(header.chipset, header.chipset_subtype);
        let sram_size = if enhancement_chip == Some(EnhancementChip::SuperFx) {
            super_fx_ram_size(header.expansion_ram_field)
        } else {
            decode_sram_size(header.ram_size_field)
        };

        Ok(Self {
            rom: stripped.to_vec(),
            mapping: candidate.mapping,
            sram_size,
            has_battery: has_battery(header.chipset),
            speed: if header.map_mode & 0x10 != 0 {
                RomSpeed::Fast
            } else {
                RomSpeed::Slow
            },
            title: header.title,
            country: header.country,
            enhancement_chip,
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

    pub fn enhancement_chip(&self) -> Option<EnhancementChip> {
        self.enhancement_chip
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

/// Size of a Super FX cartridge's Game Pak RAM. fullsnes ("SNES Cart GSU-n Memory Map", "GSU
/// Cartridge Header"): GSU carts leave the normal RAM-size byte at 0 and declare their RAM in the
/// extended header's expansion-RAM byte (32 KB and 64 KB exist). Star Fox has no extended header
/// and 32 KB, which is also used for any other cart that does not declare a size. Mesen2 picks
/// 64 KB in that case instead; fullsnes names the size, so it wins.
fn super_fx_ram_size(expansion_ram_field: Option<u8>) -> usize {
    const UNDECLARED: usize = 32 * 1024;
    match expansion_ram_field.map(decode_sram_size) {
        Some(size) if size > 0 => size,
        _ => UNDECLARED,
    }
}

fn has_battery(chipset: u8) -> bool {
    matches!(chipset & 0x0F, 0x2 | 0x5 | 0x6 | 0x9 | 0xA | 0xD | 0xE)
}

fn detect_enhancement_chip(chipset: u8, subtype: Option<u8>) -> Option<EnhancementChip> {
    match chipset {
        0x00..=0x02 => None,
        0x03..=0x06 => Some(EnhancementChip::Dsp),
        0x13..=0x1A => Some(EnhancementChip::SuperFx),
        0x25 => Some(EnhancementChip::Obc1),
        0x32 | 0x34 | 0x35 => Some(EnhancementChip::Sa1),
        0x43 | 0x45 => Some(EnhancementChip::Sdd1),
        0x55 => Some(EnhancementChip::Srtc),
        0xE3 => Some(EnhancementChip::SuperGameBoy),
        0xE5 => Some(EnhancementChip::Satellaview),
        value if value & 0xF0 == 0xF0 => match subtype {
            Some(0x00) => Some(EnhancementChip::Spc7110),
            Some(0x01) => Some(EnhancementChip::St010St011),
            Some(0x02) => Some(EnhancementChip::St018),
            Some(0x10) => Some(EnhancementChip::Cx4),
            other => Some(EnhancementChip::UnknownCustom {
                chipset,
                subtype: other,
            }),
        },
        _ => None,
    }
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
    fn from_bytes_detects_64kb_hirom_mapping() {
        // A 64 KiB HiROM has its header flush against the end of the image
        // (0xFFC0..0x10000). The full from_bytes path (detection + header
        // parse) must accept it instead of rejecting it as HeaderNotFound.
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0xFFC0,
            0x21,
            0x00,
            0x00,
            b"HIROM 64K TEST     \0\0",
        );
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0x80;

        let cart = Cartridge::from_bytes(&rom).expect("64 KiB HiROM should load");
        assert_eq!(cart.mapping(), Mapping::HiRom);
    }

    #[test]
    fn from_bytes_detects_minimal_32kib_lorom_mapping() {
        // A single-bank 32 KiB LoROM has its header flush against the end of
        // the image (0x7FC0..0x8000), same as the 64 KiB HiROM case above.
        // This is a valid, minimal SNES ROM size (used by e.g. the vendored
        // 93143 hvdma.sfc test ROM) and must not be rejected as TooShort.
        let mut rom = vec![0u8; 0x8000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x00,
            0x00,
            b"LOROM 32K TEST     \0\0",
        );

        let cart = Cartridge::from_bytes(&rom).expect("32 KiB LoROM should load");
        assert_eq!(cart.mapping(), Mapping::LoRom);
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
    fn from_bytes_detects_hirom_dsp_fixture() {
        let mut rom = vec![0u8; 0x20000];
        write_header(
            &mut rom,
            0xFFC0,
            0x21,
            0x03,
            0x00,
            b"DSP HIROM TEST      \0",
        );
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0x80;

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
        assert_eq!(cart.title(), "DSP HIROM TEST");
        assert_eq!(cart.enhancement_chip(), Some(EnhancementChip::Dsp));
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

    #[test]
    fn detects_cx4_from_custom_subtype() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0xF3,
            0x00,
            b"CX4 TEST           \0\0",
        );
        rom[0x7FBF] = 0x10;
        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.enhancement_chip(), Some(EnhancementChip::Cx4));
    }

    /// A Super FX cartridge's Game Pak RAM is declared by the extended header's expansion-RAM
    /// byte `$FFBD`, not the normal `$FFD8` field, which GSU carts leave at 0 (fullsnes "SNES
    /// Cart GSU-n Memory Map", "GSU Cartridge Header"). The extended header is present when the
    /// maker code `$FFDA` is `$33`.
    #[test]
    fn super_fx_cart_ram_size_comes_from_expansion_ram_field() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x15,
            0x00,
            b"GSU EXT HEADER     \0\0",
        );
        rom[0x7FC0 + 0x1A] = 0x33; // Maker code $33: extended header present.
        rom[0x7FBD] = 0x06; // Expansion RAM: 1 << 6 KB = 64 KB.
        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.enhancement_chip(), Some(EnhancementChip::SuperFx));
        assert_eq!(cart.sram_size(), 64 * 1024);
        assert!(
            cart.has_battery(),
            "chipset $15 = co-processor + RAM + battery"
        );
    }

    /// Star Fox has no extended header; fullsnes gives its RAM as 32 KB, and treats that as the
    /// size of every GSU cartridge that does not declare one.
    #[test]
    fn super_fx_cart_without_extended_header_has_32kb_ram() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x13,
            0x00,
            b"GSU NO EXT HEADER  \0\0",
        );
        rom[0x7FC0 + 0x1A] = 0x01; // Maker code other than $33: no extended header.
        rom[0x7FBD] = 0xFF; // Not an expansion-RAM field without the extended header.
        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.sram_size(), 32 * 1024);
        assert!(!cart.has_battery());
    }

    #[test]
    fn plain_rom_has_no_enhancement_chip() {
        let mut rom = vec![0u8; 0x10000];
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x02,
            0x00,
            b"PLAIN ROM TEST     \0\0",
        );
        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.enhancement_chip(), None);
    }

    #[test]
    fn from_bytes_exposes_country_code() {
        // Country lives at header+0x19 and is surfaced verbatim; NTSC codes are
        // 0x00/0x01 and PAL codes span 0x02..=0x0C (region derivation is the
        // console's job -- see console::snes::country_implies_pal).
        for country in [0x00u8, 0x01, 0x02, 0x0C] {
            let mut rom = vec![0u8; 0x10000];
            write_header(
                &mut rom,
                0x7FC0,
                0x20,
                0x00,
                0x00,
                b"COUNTRY TEST       \0\0",
            );
            rom[0x7FC0 + 0x19] = country;
            let cart = Cartridge::from_bytes(&rom).expect("cart");
            assert_eq!(cart.country(), country);
        }
    }

    #[test]
    fn from_bytes_reports_speed_from_map_mode_fast_bit() {
        let mut slow = vec![0u8; 0x10000];
        write_header(
            &mut slow,
            0x7FC0,
            0x20,
            0x00,
            0x00,
            b"SLOW TEST          \0\0",
        );
        assert_eq!(
            Cartridge::from_bytes(&slow).expect("cart").speed(),
            RomSpeed::Slow
        );

        let mut fast = vec![0u8; 0x10000];
        write_header(
            &mut fast,
            0x7FC0,
            0x30,
            0x00,
            0x00,
            b"FAST TEST          \0\0",
        );
        assert_eq!(
            Cartridge::from_bytes(&fast).expect("cart").speed(),
            RomSpeed::Fast
        );
    }

    #[test]
    fn from_bytes_strips_copier_header_for_hirom() {
        let mut rom = vec![0xAAu8; 0x200 + 0x10000];
        rom[0x200] = 0x55;
        write_header(
            &mut rom,
            0x200 + 0xFFC0,
            0x21,
            0x00,
            0x00,
            b"HIROM COPIER TEST  \0\0",
        );
        rom[0x200 + 0xFFFC] = 0x00;
        rom[0x200 + 0xFFFD] = 0x80;

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
        assert_eq!(cart.rom().len(), 0x10000);
        assert_eq!(cart.rom()[0], 0x55);
    }

    #[test]
    fn from_bytes_does_not_strip_when_length_not_copier_multiple() {
        // A clean 0x10000 image (len % 0x400 == 0, not 0x200) must be kept
        // whole -- the first byte is NOT a copier header to discard.
        let mut rom = vec![0u8; 0x10000];
        rom[0] = 0x99;
        write_header(
            &mut rom,
            0x7FC0,
            0x20,
            0x00,
            0x00,
            b"NO COPIER TEST     \0\0",
        );

        let cart = Cartridge::from_bytes(&rom).expect("cart");
        assert_eq!(cart.rom().len(), 0x10000);
        assert_eq!(cart.rom()[0], 0x99);
    }

    #[test]
    fn from_bytes_rejects_rom_shorter_than_min_header_end() {
        // One byte below the 0x8000 minimum: the header can't fully fit, so the
        // length precondition must reject it as TooShort before detection.
        let rom = vec![0u8; 0x8000 - 1];
        let err = Cartridge::from_bytes(&rom)
            .err()
            .expect("sub-minimum image should be rejected");
        assert_eq!(err, CartridgeError::TooShort);
    }
}
