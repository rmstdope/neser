// iNES / NES 2.0 header parsing helpers
// Purpose: centralize header parsing so multiple callers can reuse the logic.

use crate::cartridge::rom_db::RomDb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleType {
    NesFamicom,
    VsSystem,
    Playchoice10,
    Extended(u8),
}

impl ConsoleType {
    pub fn from_header(flags7: u8, nes2: bool, extended_value: u8) -> Self {
        if nes2 {
            return match flags7 & 0x03 {
                0x00 => Self::NesFamicom,
                0x01 => Self::VsSystem,
                0x02 => Self::Playchoice10,
                _ => Self::Extended(extended_value),
            };
        }

        if (flags7 & 0x01) != 0 {
            Self::VsSystem
        } else if (flags7 & 0x02) != 0 {
            Self::Playchoice10
        } else {
            Self::NesFamicom
        }
    }

    #[allow(dead_code)]
    pub fn header_value(self) -> u8 {
        match self {
            Self::NesFamicom => 0x00,
            Self::VsSystem => 0x01,
            Self::Playchoice10 => 0x02,
            Self::Extended(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NametableLayout {
    Horizontal,
    Vertical,
    FourScreen,
    SingleScreen,
    SingleScreenLower,
    SingleScreenUpper,
}

impl NametableLayout {
    pub fn from_flags6(flags6: u8) -> Self {
        if (flags6 & 0x08) != 0 {
            Self::FourScreen
        } else if (flags6 & 0x01) != 0 {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }

    #[allow(dead_code)]
    pub fn header_value(self) -> Option<u8> {
        match self {
            Self::Horizontal => Some(0x00),
            Self::Vertical => Some(0x01),
            Self::FourScreen => Some(0x08),
            Self::SingleScreen | Self::SingleScreenLower | Self::SingleScreenUpper => None,
        }
    }

    /// Encode this layout as a single byte for snapshot/save-state purposes.
    ///
    /// Standard encoding: Horizontal=0, Vertical=1, SingleScreenLower=2,
    /// SingleScreenUpper=3, FourScreen=4.
    /// `SingleScreen` is treated as equivalent to `SingleScreenLower` (both → 2).
    pub fn to_snapshot_byte(self) -> u8 {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
            Self::SingleScreen | Self::SingleScreenLower => 2,
            Self::SingleScreenUpper => 3,
            Self::FourScreen => 4,
        }
    }

    /// Decode a snapshot byte back into a `NametableLayout`.
    ///
    /// Standard encoding: 0=Horizontal, 1=Vertical, 2=SingleScreenLower,
    /// 3=SingleScreenUpper, 4=FourScreen.
    /// Unknown values default to `Horizontal`.
    pub fn from_snapshot_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Horizontal,
            1 => Self::Vertical,
            2 => Self::SingleScreenLower,
            3 => Self::SingleScreenUpper,
            4 => Self::FourScreen,
            _ => Self::Horizontal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingMode {
    Ntsc,
    Pal,
    MultiRegion,
    Dendy,
    Unknown(u8),
}

impl TimingMode {
    const CPU_CLOCK_NTSC: f32 = 1_789_773.0;
    const CPU_CLOCK_PAL: f32 = 1_662_607.0;
    const NTSC_SCANLINES: u16 = 262;
    const PAL_SCANLINES: u16 = 312;
    const DOTS_PER_SCANLINE: u16 = 341;

    pub fn from_header(header: &[u8; 16], nes2: bool) -> Self {
        if nes2 {
            return match header[12] & 0x03 {
                0x00 => Self::Ntsc,
                0x01 => Self::Pal,
                0x02 => Self::MultiRegion,
                0x03 => Self::Dendy,
                value => Self::Unknown(value),
            };
        }

        if (header[9] & 0x01) != 0 {
            Self::Pal
        } else {
            Self::Ntsc
        }
    }

    #[allow(dead_code)]
    pub fn header_value(self) -> u8 {
        match self {
            Self::Ntsc => 0x00,
            Self::Pal => 0x01,
            Self::MultiRegion => 0x02,
            Self::Dendy => 0x03,
            Self::Unknown(value) => value,
        }
    }

    pub fn normalize_rom_timing_mode(self) -> Self {
        match self {
            Self::Ntsc => Self::Ntsc,
            Self::Pal => Self::Pal,
            Self::MultiRegion | Self::Dendy | Self::Unknown(_) => Self::Unknown(0),
        }
    }

    pub fn is_ntsc_or_pal(self) -> bool {
        matches!(self, Self::Ntsc | Self::Pal)
    }

    pub fn cpu_clock_hz(self) -> f32 {
        if matches!(self, Self::Pal) {
            Self::CPU_CLOCK_PAL
        } else {
            Self::CPU_CLOCK_NTSC
        }
    }

    pub fn frame_rate_hz(self) -> f64 {
        let cpu_clock = f64::from(self.cpu_clock_hz());
        let ppu_cycles_per_frame = if matches!(self, Self::Pal) {
            f64::from(Self::PAL_SCANLINES) * f64::from(Self::DOTS_PER_SCANLINE)
        } else {
            let even_ppu_cycles =
                f64::from(Self::NTSC_SCANLINES) * f64::from(Self::DOTS_PER_SCANLINE);
            let odd_ppu_cycles = even_ppu_cycles - 1.0;
            (even_ppu_cycles + odd_ppu_cycles) / 2.0
        };
        let cpu_cycles_per_frame = ppu_cycles_per_frame / self.ppu_cycles_per_cpu_cycle();
        cpu_clock / cpu_cycles_per_frame
    }

    pub fn ppu_cycles_per_cpu_cycle(self) -> f64 {
        if matches!(self, Self::Pal) { 3.2 } else { 3.0 }
    }

    pub fn scanlines_per_frame(self) -> u16 {
        if matches!(self, Self::Pal) {
            Self::PAL_SCANLINES
        } else {
            Self::NTSC_SCANLINES
        }
    }
}

/// Parsed iNES / NES 2.0 header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InesHeader {
    pub mapper: u16,
    pub submapper: u8,
    pub console_type: ConsoleType,
    pub mirroring: NametableLayout,
    pub has_trainer: bool,
    pub header_version: &'static str,
    pub battery_backed_prg_ram: bool,
    pub prg_rom_size_bytes: usize,
    pub chr_rom_size_bytes: usize,
    pub prg_ram_size_bytes: Option<usize>,
    pub prg_nvram_size_bytes: Option<usize>,
    pub chr_ram_size_bytes: Option<usize>,
    pub chr_nvram_size_bytes: Option<usize>,
    pub timing_mode: TimingMode,
    pub vs_ppu_type: Option<u8>,
    pub vs_hardware_type: Option<u8>,
    pub misc_roms: u8,
    pub default_expansion_device: u8,
}

impl InesHeader {
    /// Parse a 16-byte iNES header. Returns `None` if the header magic is invalid.
    pub fn parse(header: &[u8; 16]) -> Option<Self> {
        if &header[0..4] != b"NES\x1A" {
            return None;
        }

        let flags6 = header[6];
        let flags7 = header[7];
        let nes2 = (flags7 & 0x0C) == 0x08;

        let header_version = if nes2 { "2.0" } else { "1.0" };
        let has_trainer = (flags6 & 0x04) != 0;
        let battery_backed_prg_ram = (flags6 & 0x02) != 0;
        let mirroring = NametableLayout::from_flags6(flags6);

        let (mapper, submapper) = if nes2 {
            let mapper = (flags6 as u16 >> 4)
                | ((flags7 as u16) & 0xF0)
                | (((header[8] & 0x0F) as u16) << 8);
            let submapper = header[8] >> 4;
            (mapper, submapper)
        } else {
            let mapper = (flags6 >> 4) | (flags7 & 0xF0);
            (mapper as u16, 0)
        };

        let console_type = ConsoleType::from_header(flags7, nes2, header[13]);

        let (prg_rom_size_bytes, chr_rom_size_bytes) = if nes2 {
            let prg_msb = header[9] & 0x0F;
            let chr_msb = header[9] >> 4;
            (
                Self::parse_nes2_rom_size_bytes(header[4], prg_msb, 16 * 1024),
                Self::parse_nes2_rom_size_bytes(header[5], chr_msb, 8 * 1024),
            )
        } else {
            (
                header[4] as usize * 16 * 1024,
                header[5] as usize * 8 * 1024,
            )
        };

        let (prg_ram_size_bytes, prg_nvram_size_bytes, chr_ram_size_bytes, chr_nvram_size_bytes) =
            if nes2 {
                let prg_ram = Self::parse_nes2_ram_size(header[10] & 0x0F);
                let prg_nvram = Self::parse_nes2_ram_size(header[10] >> 4);
                let chr_ram = Self::parse_nes2_ram_size(header[11] & 0x0F);
                let chr_nvram = Self::parse_nes2_ram_size(header[11] >> 4);
                (prg_ram, prg_nvram, chr_ram, chr_nvram)
            } else {
                let prg_ram = if header[8] == 0 {
                    Some(8 * 1024)
                } else {
                    Some(header[8] as usize * 8 * 1024)
                };
                let chr_ram = if chr_rom_size_bytes == 0 {
                    Some(8 * 1024)
                } else {
                    None
                };
                (prg_ram, None, chr_ram, None)
            };

        let timing_mode = TimingMode::from_header(header, nes2);

        let (vs_ppu_type, vs_hardware_type) =
            if matches!(console_type, ConsoleType::VsSystem) && nes2 {
                (Some(header[13] & 0x0F), Some(header[13] >> 4))
            } else {
                (None, None)
            };

        let misc_roms = if nes2 { header[14] } else { 0 };
        let default_expansion_device = if nes2 { header[15] } else { 0 };

        Some(Self {
            mapper,
            submapper,
            console_type,
            mirroring,
            has_trainer,
            header_version,
            battery_backed_prg_ram,
            prg_rom_size_bytes,
            chr_rom_size_bytes,
            prg_ram_size_bytes,
            prg_nvram_size_bytes,
            chr_ram_size_bytes,
            chr_nvram_size_bytes,
            timing_mode,
            vs_ppu_type,
            vs_hardware_type,
            misc_roms,
            default_expansion_device,
        })
    }

    // Helper: parse NES2 RAM size nibble into bytes
    fn parse_nes2_ram_size(nibble: u8) -> Option<usize> {
        if nibble == 0 {
            None
        } else {
            Some(64usize << nibble)
        }
    }

    // Helper: parse NES2 ROM size (LSB + MSB nibble) into bytes
    fn parse_nes2_rom_size_bytes(lsb: u8, msb_nibble: u8, unit_bytes: usize) -> usize {
        if msb_nibble == 0x0F {
            // Extended format
            let exponent = (lsb >> 4) as usize;
            let multiplier = (lsb & 0x0F) as usize;
            let base = 1usize << (exponent + 10);
            base * (multiplier * 2 + 1)
        } else {
            let size_units = ((msb_nibble as usize) << 8) | lsb as usize;
            size_units * unit_bytes
        }
    }
}

/// Errors that can occur while parsing a full ROM blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomParseError {
    InvalidHeader,
    FileTooSmall { expected: usize, actual: usize },
}

// iNES format layout constants
const HEADER_SIZE: usize = 16;
const TRAINER_SIZE: usize = 512;

/// Result of parsing a full iNES/NES2 ROM blob.
#[derive(Debug)]
pub struct ParsedRom {
    pub header: InesHeader,
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub trainer: Option<Vec<u8>>,
    /// CRC32 of the concatenated PRG + CHR ROM data.
    pub crc32: u32,
    /// CRC32 of the raw payload (everything after header + trainer).
    /// Useful as a fallback for ROM database lookup.
    pub payload_crc32: u32,
}

impl ParsedRom {
    /// Parse a full iNES/NES2 ROM blob, optionally applying ROM database overrides.
    ///
    /// When a `RomDb` is provided, the parsed CRC32 (and a fallback payload CRC32)
    /// are used to look up the ROM in the database. Any database overrides for
    /// mapper, submapper, mirroring, PRG/CHR sizes, and timing mode are applied
    /// to the returned `ParsedRom`.
    ///
    /// # Errors
    ///
    /// Returns `RomParseError::InvalidHeader` if the magic bytes are invalid.
    /// Returns `RomParseError::FileTooSmall` if the file is smaller than expected.
    pub fn parse(data: &[u8], rom_db: Option<&RomDb>) -> Result<Self, RomParseError> {
        if data.len() < HEADER_SIZE {
            return Err(RomParseError::FileTooSmall {
                expected: HEADER_SIZE,
                actual: data.len(),
            });
        }

        let header: [u8; HEADER_SIZE] = data[0..HEADER_SIZE]
            .try_into()
            .map_err(|_| RomParseError::InvalidHeader)?;
        let info = InesHeader::parse(&header).ok_or(RomParseError::InvalidHeader)?;

        let mut trainer_offset = if info.has_trainer { TRAINER_SIZE } else { 0 };
        let mut prg_rom_start = HEADER_SIZE + trainer_offset;

        // Extract trainer data if present
        let mut trainer = if info.has_trainer {
            if data.len() < HEADER_SIZE + TRAINER_SIZE {
                return Err(RomParseError::FileTooSmall {
                    expected: HEADER_SIZE + TRAINER_SIZE,
                    actual: data.len(),
                });
            }
            Some(data[HEADER_SIZE..HEADER_SIZE + TRAINER_SIZE].to_vec())
        } else {
            None
        };

        // Primary sizes come from parsed header (NES2-aware). If these sizes
        // don't fit in the provided data and the header is marked as NES2,
        // attempt a graceful fallback to iNES v1 sizing to be permissive with
        // headers that set NES2 bits but leave v1-compatible size fields.
        let mut prg_bytes = info.prg_rom_size_bytes;
        let mut chr_bytes = info.chr_rom_size_bytes;

        let prg_rom_end = prg_rom_start + prg_bytes;
        let chr_rom_start = prg_rom_end;
        let chr_rom_end = chr_rom_start + chr_bytes;

        let mut header_for_return = info.clone();

        if data.len() < chr_rom_end {
            // Try fallback to iNES v1 sizing when header indicated NES2 but data is small.
            if header_for_return.header_version == "2.0" {
                let prg_v1 = (header[4] as usize) * 16 * 1024;
                let chr_v1 = (header[5] as usize) * 8 * 1024;
                let prg_end_v1 = prg_rom_start + prg_v1;
                let chr_end_v1 = prg_end_v1 + chr_v1;
                if data.len() >= chr_end_v1 {
                    prg_bytes = prg_v1;
                    chr_bytes = chr_v1;
                    header_for_return.prg_rom_size_bytes = prg_bytes;
                    header_for_return.chr_rom_size_bytes = chr_bytes;
                } else {
                    return Err(RomParseError::FileTooSmall {
                        expected: chr_end_v1,
                        actual: data.len(),
                    });
                }
            } else if info.has_trainer {
                // Trainer bit mislabeled: check if the data fits without the trainer offset.
                let no_trainer_chr_end = HEADER_SIZE + prg_bytes + chr_bytes;
                if data.len() >= no_trainer_chr_end {
                    trainer_offset = 0;
                    prg_rom_start = HEADER_SIZE;
                    trainer = None;
                    header_for_return.has_trainer = false;
                } else {
                    return Err(RomParseError::FileTooSmall {
                        expected: chr_rom_end,
                        actual: data.len(),
                    });
                }
            } else {
                return Err(RomParseError::FileTooSmall {
                    expected: chr_rom_end,
                    actual: data.len(),
                });
            }
        }

        let prg_rom = data[prg_rom_start..(prg_rom_start + prg_bytes)].to_vec();
        let chr_rom =
            data[(prg_rom_start + prg_bytes)..(prg_rom_start + prg_bytes + chr_bytes)].to_vec();

        let crc = crate::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

        let payload = if prg_rom_start <= data.len() {
            &data[prg_rom_start..]
        } else {
            &[]
        };
        let payload_crc32 = crate::cartridge::calculate_rom_crc32(payload, &[]);

        let mut parsed = Self {
            header: header_for_return,
            prg_rom,
            chr_rom,
            trainer,
            crc32: crc,
            payload_crc32,
        };

        if let Some(db) = rom_db {
            parsed.apply_db_overrides(data, db)?;
        }

        Ok(parsed)
    }

    /// Look up the ROM in the database and apply any overrides.
    fn apply_db_overrides(&mut self, data: &[u8], db: &RomDb) -> Result<(), RomParseError> {
        let db_entry = db
            .get_by_crc(self.crc32)
            .or_else(|| db.get_by_crc(self.payload_crc32));

        let Some(entry) = db_entry else {
            return Ok(());
        };

        // Apply PRG/CHR size overrides — may require re-slicing the raw data.
        let resolved_prg =
            Self::resolve_with_db(self.header.prg_rom_size_bytes, entry.prg_rom_size);
        let resolved_chr =
            Self::resolve_with_db(self.header.chr_rom_size_bytes, entry.chr_rom_size);

        if resolved_prg != self.header.prg_rom_size_bytes
            || resolved_chr != self.header.chr_rom_size_bytes
        {
            let (new_prg, new_chr, new_crc) = Self::reparse_rom_with_sizes(
                data,
                self.header.has_trainer,
                resolved_prg,
                resolved_chr,
            )?;
            self.prg_rom = new_prg;
            self.chr_rom = new_chr;
            self.header.prg_rom_size_bytes = resolved_prg;
            self.header.chr_rom_size_bytes = resolved_chr;
            self.crc32 = new_crc;
        }

        // Apply scalar overrides.
        if let Some(mapper) = entry.mapper {
            self.header.mapper = mapper;
        }
        if let Some(submapper) = entry.submapper {
            self.header.submapper = submapper;
        }
        if let Some(mirroring) = entry.nametable_layout {
            self.header.mirroring = mirroring;
        }
        if let Some(timing) = entry.hardware.map(|h| h.timing_mode()) {
            self.header.timing_mode = timing;
        }

        Ok(())
    }

    /// Resolve a header value with an optional database override.
    fn resolve_with_db(header_value: usize, db_value: Option<u32>) -> usize {
        db_value.map(|v| v as usize).unwrap_or(header_value)
    }

    /// Re-parse the raw ROM data with overridden PRG and CHR sizes.
    ///
    /// This is used when a ROM database provides corrected sizes that differ
    /// from what the header declares. All knowledge of the iNES layout
    /// (header size, trainer offset) is encapsulated here.
    ///
    /// # Errors
    ///
    /// Returns `RomParseError::FileTooSmall` if the data is too small for the
    /// requested sizes.
    fn reparse_rom_with_sizes(
        data: &[u8],
        has_trainer: bool,
        prg_size: usize,
        chr_size: usize,
    ) -> Result<(Vec<u8>, Vec<u8>, u32), RomParseError> {
        let trainer_offset = if has_trainer { TRAINER_SIZE } else { 0 };
        let prg_start = HEADER_SIZE + trainer_offset;
        let prg_end = prg_start + prg_size;
        let chr_start = prg_end;
        let chr_end = chr_start + chr_size;

        if data.len() < chr_end {
            return Err(RomParseError::FileTooSmall {
                expected: chr_end,
                actual: data.len(),
            });
        }

        let prg_rom = data[prg_start..prg_end].to_vec();
        let chr_rom = data[chr_start..chr_end].to_vec();
        let crc32 = crate::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

        Ok((prg_rom, chr_rom, crc32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_ines_header_v1() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1; // 1 * 16KB PRG
        header[5] = 1; // 1 * 8KB CHR
        header[6] = 0; // flags6
        header[7] = 0; // flags7
        header[8] = 0; // prg ram size

        let info = InesHeader::parse(&header).expect("header parse");
        assert_eq!(info.prg_rom_size_bytes, 16 * 1024);
        assert_eq!(info.chr_rom_size_bytes, 8 * 1024);
        assert_eq!(info.header_version, "1.0");
        assert_eq!(info.mapper, 0);
    }

    #[test]
    fn parse_nes2_sizes() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[6] = 0;
        header[7] = 0x08; // NES2 identifier bits
        header[4] = 0x10; // lsb
        header[9] = 0x01; // prg msb nibble = 1

        let info = InesHeader::parse(&header).expect("nes2");
        // with prg lsb 0x10 (16) and msb 1, compute size > 0
        assert!(info.prg_rom_size_bytes > 0);
        assert_eq!(info.header_version, "2.0");
    }

    #[test]
    fn parse_trainer_and_flags() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[6] = 0x07; // trainer + battery + mirroring bit set
        header[7] = 0x10; // mapper high nibble
        header[4] = 2; // 2 * 16KB PRG
        header[5] = 1; // 1 * 8KB CHR

        let info = InesHeader::parse(&header).expect("header parse");
        assert!(info.has_trainer);
        assert!(info.battery_backed_prg_ram);
        assert_eq!(info.prg_rom_size_bytes, 2 * 16 * 1024);
        assert_eq!(info.chr_rom_size_bytes, 8 * 1024);
    }

    #[test]
    fn parse_prg_ram_default_and_chr_ram_default() {
        // v1 header with zero prg ram and zero chr -> defaults
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1; // prg
        header[5] = 0; // chr (implies CHR-RAM default)
        header[6] = 0; // flags
        header[8] = 0; // prg ram size byte

        let info = InesHeader::parse(&header).expect("v1 defaults");
        assert_eq!(info.prg_ram_size_bytes, Some(8 * 1024));
        assert_eq!(info.chr_ram_size_bytes, Some(8 * 1024));
    }

    #[test]
    fn parse_console_and_timing_modes() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[6] = 0;
        header[7] = 0x01; // vs system flag in v1
        header[9] = 0x01; // pal for v1

        let info = InesHeader::parse(&header).expect("console timing");
        match info.console_type {
            ConsoleType::VsSystem => {}
            _ => panic!("expected VsSystem"),
        }
        match info.timing_mode {
            TimingMode::Pal => {}
            _ => panic!("expected PAL"),
        }
    }

    #[test]
    fn parse_rom_v1_success() {
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 2; // 2 * 16KB PRG
        rom[5] = 1; // 1 * 8KB CHR
        rom[6] = 0; // flags6
        rom[7] = 0; // flags7

        // Append PRG (32KB) and CHR (8KB)
        rom.extend(vec![0xAAu8; 2 * 16 * 1024]);
        rom.extend(vec![0xBBu8; 8 * 1024]);

        let parsed = ParsedRom::parse(&rom, None).expect("parse_rom v1");
        assert_eq!(parsed.header.header_version, "1.0");
        assert_eq!(parsed.prg_rom.len(), 2 * 16 * 1024);
        assert_eq!(parsed.chr_rom.len(), 8 * 1024);
        assert!(parsed.trainer.is_none());
        assert_eq!(
            parsed.crc32,
            crate::cartridge::calculate_rom_crc32(&parsed.prg_rom, &parsed.chr_rom)
        );
    }

    #[test]
    fn parse_rom_nes2_fallback_to_v1() {
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1; // v1 PRG LSB 1
        rom[5] = 1; // v1 CHR LSB 1
        rom[6] = 0;
        rom[7] = 0x08; // mark NES2
        // Set NES2 prg MSB nibble to 0x01 to produce a large NES2 size that
        // does not fit the provided data, forcing the v1 fallback path.
        rom[9] = 0x01;

        // Only provide small v1-sized data (16KB + 8KB)
        rom.extend(vec![0xAAu8; 16 * 1024]);
        rom.extend(vec![0xBBu8; 8 * 1024]);

        let parsed = ParsedRom::parse(&rom, None).expect("parse_rom fallback");
        // Fallback should update reported sizes to v1 values
        assert_eq!(parsed.header.header_version, "2.0");
        assert_eq!(parsed.header.prg_rom_size_bytes, 16 * 1024);
        assert_eq!(parsed.header.chr_rom_size_bytes, 8 * 1024);
        assert_eq!(parsed.prg_rom.len(), 16 * 1024);
        assert_eq!(parsed.chr_rom.len(), 8 * 1024);
        assert!(parsed.trainer.is_none());
    }

    #[test]
    fn parse_rom_file_too_small() {
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 2; // 32KB PRG expected
        rom[5] = 0;
        rom[6] = 0;
        rom[7] = 0;

        // Only provide 100 bytes after header
        rom.extend(vec![0x00u8; 100]);

        let err = ParsedRom::parse(&rom, None).expect_err("should be too small");
        match err {
            RomParseError::FileTooSmall {
                expected: _,
                actual,
            } => assert!(actual < 2000),
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn parse_rom_invalid_header() {
        // Provide a 16-byte buffer with an invalid magic to trigger InvalidHeader
        let mut data = vec![0u8; 16];
        data[0..4].copy_from_slice(b"BAD!");
        let err = ParsedRom::parse(&data, None).expect_err("invalid header");
        match err {
            RomParseError::InvalidHeader => {}
            _ => panic!("expected InvalidHeader"),
        }
    }

    #[test]
    fn parse_rom_with_trainer_extracts_trainer_data() {
        // Test that when trainer flag is set, the 512-byte trainer is extracted
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1; // 1 * 16KB PRG
        rom[5] = 1; // 1 * 8KB CHR
        rom[6] = 0x04; // flags6 with trainer bit set (bit 2)
        rom[7] = 0; // flags7

        // Add 512 bytes of trainer data with a pattern
        let trainer_data: Vec<u8> = (0..512).map(|i| i as u8).collect();
        rom.extend(&trainer_data);

        // Add PRG (16KB) and CHR (8KB)
        rom.extend(vec![0xAAu8; 16 * 1024]);
        rom.extend(vec![0xBBu8; 8 * 1024]);

        let parsed = ParsedRom::parse(&rom, None).expect("parse_rom with trainer");

        // Verify header recognizes trainer
        assert!(parsed.header.has_trainer);

        // Verify trainer data is extracted
        assert!(parsed.trainer.is_some());
        let extracted_trainer = parsed.trainer.unwrap();
        assert_eq!(extracted_trainer.len(), 512);

        // Verify trainer data matches what we put in
        for (i, &byte) in extracted_trainer.iter().enumerate() {
            assert_eq!(byte, i as u8);
        }

        // Verify PRG and CHR are still correct
        assert_eq!(parsed.prg_rom.len(), 16 * 1024);
        assert_eq!(parsed.chr_rom.len(), 8 * 1024);
        assert_eq!(parsed.prg_rom[0], 0xAA);
        assert_eq!(parsed.chr_rom[0], 0xBB);
    }

    #[test]
    fn parse_rom_without_trainer_returns_none() {
        // Test that when trainer flag is not set, no trainer data is returned
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1; // 1 * 16KB PRG
        rom[5] = 1; // 1 * 8KB CHR
        rom[6] = 0x00; // flags6 without trainer bit
        rom[7] = 0; // flags7

        // Add PRG (16KB) and CHR (8KB)
        rom.extend(vec![0xAAu8; 16 * 1024]);
        rom.extend(vec![0xBBu8; 8 * 1024]);

        let parsed = ParsedRom::parse(&rom, None).expect("parse_rom without trainer");

        // Verify header doesn't have trainer
        assert!(!parsed.header.has_trainer);

        // Verify no trainer data is returned
        assert!(parsed.trainer.is_none());

        // Verify PRG and CHR are correct
        assert_eq!(parsed.prg_rom.len(), 16 * 1024);
        assert_eq!(parsed.chr_rom.len(), 8 * 1024);
    }

    #[test]
    fn parse_rom_with_trainer_file_too_small() {
        // Test that if trainer flag is set but file is too small, error is returned
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1; // 1 * 16KB PRG
        rom[5] = 1; // 1 * 8KB CHR
        rom[6] = 0x04; // flags6 with trainer bit set
        rom[7] = 0; // flags7

        // Only add 100 bytes (not enough for trainer)
        rom.extend(vec![0x00u8; 100]);

        let err = ParsedRom::parse(&rom, None).expect_err("should be too small");
        match err {
            RomParseError::FileTooSmall { expected, actual } => {
                // Expected: 16 (header) + 512 (trainer) = 528
                assert_eq!(expected, 528);
                assert_eq!(actual, 116);
            }
            _ => panic!("expected FileTooSmall error"),
        }
    }

    #[test]
    fn mirroring_mode_header_values_match_ines_flags6() {
        assert_eq!(NametableLayout::Horizontal.header_value(), Some(0x00));
        assert_eq!(NametableLayout::Vertical.header_value(), Some(0x01));
        assert_eq!(NametableLayout::FourScreen.header_value(), Some(0x08));
        assert_eq!(NametableLayout::SingleScreen.header_value(), None);
    }

    #[test]
    fn timing_mode_header_values_match_nes2() {
        assert_eq!(TimingMode::Ntsc.header_value(), 0x00);
        assert_eq!(TimingMode::Pal.header_value(), 0x01);
        assert_eq!(TimingMode::MultiRegion.header_value(), 0x02);
        assert_eq!(TimingMode::Dendy.header_value(), 0x03);
        assert_eq!(TimingMode::Unknown(0x0A).header_value(), 0x0A);
    }

    #[test]
    fn console_type_header_values_match_spec() {
        assert_eq!(ConsoleType::NesFamicom.header_value(), 0x00);
        assert_eq!(ConsoleType::VsSystem.header_value(), 0x01);
        assert_eq!(ConsoleType::Playchoice10.header_value(), 0x02);
        assert_eq!(ConsoleType::Extended(0x09).header_value(), 0x09);
    }

    /// Build a minimal valid iNES v1 ROM with the given PRG/CHR bank counts.
    fn build_test_rom(prg_banks: u8, chr_banks: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = prg_banks;
        rom[5] = chr_banks;
        rom.extend(vec![0xAAu8; prg_banks as usize * 16 * 1024]);
        rom.extend(vec![0xBBu8; chr_banks as usize * 8 * 1024]);
        rom
    }

    /// Create a `RomDb` with a single entry whose CRC matches the given ROM
    /// and whose CSV columns encode the specified overrides.
    fn db_with_override_for_rom(rom: &[u8], csv_fields: &str) -> RomDb {
        let parsed = ParsedRom::parse(rom, None).expect("test ROM should parse");
        let crc_hex = format!("{:08X}", parsed.crc32);
        // CSV: rom_id,name,country,crc,hardware,rom_class,
        //      mapper,submapper,nametable_layout,prg_rom_size,prg_rom_crc,
        //      prg_nvram_size,prg_ram_size,chr_rom_size,chr_rom_crc,
        //      chr_nvram_size,chr_ram_size,battery,vs_hardware,vs_ppu,expansion
        let csv = format!("1,Test,US,{crc_hex},{csv_fields}\n");
        RomDb::from_csv_content(&csv)
    }

    #[test]
    fn db_overrides_mapper() {
        let rom = build_test_rom(1, 1);
        // Override mapper to 4 (col 6 = csv_fields position 2)
        let db = db_with_override_for_rom(&rom, ",,4,,,,,,,,,,,,,,");
        let parsed = ParsedRom::parse(&rom, Some(&db)).expect("parse with db");
        assert_eq!(parsed.header.mapper, 4);
    }

    #[test]
    fn db_overrides_submapper() {
        let rom = build_test_rom(1, 1);
        // Override submapper to 2 (col 7 = csv_fields position 3)
        let db = db_with_override_for_rom(&rom, ",,,2,,,,,,,,,,,,,");
        let parsed = ParsedRom::parse(&rom, Some(&db)).expect("parse with db");
        assert_eq!(parsed.header.submapper, 2);
    }

    #[test]
    fn db_overrides_mirroring() {
        let rom = build_test_rom(1, 1);
        // Override nametable_layout to V (col 8 = csv_fields position 4)
        let db = db_with_override_for_rom(&rom, ",,,,V,,,,,,,,,,,,");
        let parsed = ParsedRom::parse(&rom, Some(&db)).expect("parse with db");
        assert_eq!(parsed.header.mirroring, NametableLayout::Vertical);
    }

    #[test]
    fn db_overrides_timing_mode() {
        let rom = build_test_rom(1, 1);
        // Override hardware to 1 (col 4) = NesPal -> timing_mode = PAL
        let db = db_with_override_for_rom(&rom, "1,,,,,,,,,,,,,,,,");
        let parsed = ParsedRom::parse(&rom, Some(&db)).expect("parse with db");
        assert_eq!(parsed.header.timing_mode, TimingMode::Pal);
    }

    #[test]
    fn db_no_match_leaves_header_unchanged() {
        let rom = build_test_rom(1, 1);
        // DB with a CRC that doesn't match
        let db = RomDb::from_csv_content("1,Other,,DEADBEEF,,,4,,,,,,,,,,,,,,\n");
        let without_db = ParsedRom::parse(&rom, None).expect("parse without db");
        let with_db = ParsedRom::parse(&rom, Some(&db)).expect("parse with db");
        assert_eq!(without_db.header.mapper, with_db.header.mapper);
        assert_eq!(without_db.header.mirroring, with_db.header.mirroring);
    }

    #[test]
    fn to_snapshot_byte_horizontal() {
        assert_eq!(NametableLayout::Horizontal.to_snapshot_byte(), 0);
    }

    #[test]
    fn to_snapshot_byte_vertical() {
        assert_eq!(NametableLayout::Vertical.to_snapshot_byte(), 1);
    }

    #[test]
    fn to_snapshot_byte_single_screen_lower() {
        assert_eq!(NametableLayout::SingleScreenLower.to_snapshot_byte(), 2);
    }

    #[test]
    fn to_snapshot_byte_single_screen_aliases_lower() {
        assert_eq!(NametableLayout::SingleScreen.to_snapshot_byte(), 2);
    }

    #[test]
    fn to_snapshot_byte_single_screen_upper() {
        assert_eq!(NametableLayout::SingleScreenUpper.to_snapshot_byte(), 3);
    }

    #[test]
    fn to_snapshot_byte_four_screen() {
        assert_eq!(NametableLayout::FourScreen.to_snapshot_byte(), 4);
    }

    #[test]
    fn from_snapshot_byte_horizontal() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(0),
            NametableLayout::Horizontal
        );
    }

    #[test]
    fn from_snapshot_byte_vertical() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(1),
            NametableLayout::Vertical
        );
    }

    #[test]
    fn from_snapshot_byte_single_screen_lower() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(2),
            NametableLayout::SingleScreenLower
        );
    }

    #[test]
    fn from_snapshot_byte_single_screen_upper() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(3),
            NametableLayout::SingleScreenUpper
        );
    }

    #[test]
    fn from_snapshot_byte_four_screen() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(4),
            NametableLayout::FourScreen
        );
    }

    #[test]
    fn from_snapshot_byte_unknown_defaults_to_horizontal() {
        assert_eq!(
            NametableLayout::from_snapshot_byte(255),
            NametableLayout::Horizontal
        );
    }

    #[test]
    fn snapshot_byte_roundtrip_all_variants() {
        let variants = [
            NametableLayout::Horizontal,
            NametableLayout::Vertical,
            NametableLayout::SingleScreenLower,
            NametableLayout::SingleScreenUpper,
            NametableLayout::FourScreen,
        ];
        for &layout in &variants {
            let byte = layout.to_snapshot_byte();
            let restored = NametableLayout::from_snapshot_byte(byte);
            assert_eq!(restored, layout, "roundtrip failed for {layout:?}");
        }
    }

    /// Reproduces the 110-in-1 [p1] ROM header bug: trainer bit set but the file
    /// only contains header + PRG + CHR data (no actual 512-byte trainer block).
    /// The parser should fall back gracefully and load PRG/CHR from offset 16.
    #[test]
    fn parse_rom_with_false_trainer_flag_falls_back_gracefully() {
        let prg_size = 2 * 16 * 1024; // 2 PRG banks
        let chr_size = 1 * 8 * 1024; // 1 CHR bank

        let mut rom = vec![0u8; 16];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 2; // 2 × 16KB PRG
        rom[5] = 1; // 1 × 8KB CHR
        rom[6] = 0x04; // trainer bit set (incorrectly)
        rom[7] = 0xF0; // mapper 15 in high nibble

        // Append PRG and CHR without any trainer block — file size = 16 + PRG + CHR.
        rom.extend(vec![0xAAu8; prg_size]);
        rom.extend(vec![0xBBu8; chr_size]);

        // Must succeed, not error with FileTooSmall.
        let parsed = ParsedRom::parse(&rom, None).expect("should load ROM with false trainer flag");

        // Trainer mislabeled — should be corrected by loader.
        assert!(parsed.trainer.is_none());
        assert!(!parsed.header.has_trainer);

        // PRG and CHR must be read from the start of the data area (offset 16).
        assert_eq!(parsed.prg_rom.len(), prg_size);
        assert_eq!(parsed.chr_rom.len(), chr_size);
        assert_eq!(parsed.prg_rom[0], 0xAA);
        assert_eq!(parsed.chr_rom[0], 0xBB);
    }
}
