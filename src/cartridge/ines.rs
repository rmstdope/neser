// iNES / NES 2.0 header parsing helpers
// Purpose: centralize header parsing so multiple callers can reuse the logic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleType {
    NesFamicom,
    VsSystem,
    Playchoice10,
    Extended(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingMode {
    Ntsc,
    Pal,
    MultiRegion,
    Dendy,
    Unknown(u8),
}

/// Parsed iNES / NES 2.0 header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InesHeader {
    pub mapper: u16,
    pub submapper: u8,
    pub console_type: ConsoleType,
    pub mirroring: Mirroring,
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

// Helper: parse NES2 RAM size nibble into bytes
pub fn parse_nes2_ram_size(nibble: u8) -> Option<usize> {
    if nibble == 0 {
        None
    } else {
        Some(64usize << nibble)
    }
}

// Helper: parse NES2 ROM size (LSB + MSB nibble) into bytes
pub fn parse_nes2_rom_size_bytes(lsb: u8, msb_nibble: u8, unit_bytes: usize) -> usize {
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

/// Parse a 16-byte iNES header. Returns `None` if the header magic is invalid.
pub fn parse_header(header: &[u8; 16]) -> Option<InesHeader> {
    if &header[0..4] != b"NES\x1A" {
        return None;
    }

    let flags6 = header[6];
    let flags7 = header[7];
    let nes2 = (flags7 & 0x0C) == 0x08;

    let header_version = if nes2 { "2.0" } else { "1.0" };
    let has_trainer = (flags6 & 0x04) != 0;
    let battery_backed_prg_ram = (flags6 & 0x02) != 0;
    let mirroring = if (flags6 & 0x08) != 0 {
        Mirroring::FourScreen
    } else if (flags6 & 0x01) != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let (mapper, submapper) = if nes2 {
        let mapper =
            (flags6 as u16 >> 4) | ((flags7 as u16) & 0xF0) | (((header[8] & 0x0F) as u16) << 8);
        let submapper = header[8] >> 4;
        (mapper, submapper)
    } else {
        let mapper = (flags6 >> 4) | (flags7 & 0xF0);
        (mapper as u16, 0)
    };

    let console_type = if nes2 {
        match flags7 & 0x03 {
            0x00 => ConsoleType::NesFamicom,
            0x01 => ConsoleType::VsSystem,
            0x02 => ConsoleType::Playchoice10,
            _ => ConsoleType::Extended(header[13]),
        }
    } else if (flags7 & 0x01) != 0 {
        ConsoleType::VsSystem
    } else if (flags7 & 0x02) != 0 {
        ConsoleType::Playchoice10
    } else {
        ConsoleType::NesFamicom
    };

    let (prg_rom_size_bytes, chr_rom_size_bytes) = if nes2 {
        let prg_msb = header[9] & 0x0F;
        let chr_msb = header[9] >> 4;
        (
            parse_nes2_rom_size_bytes(header[4], prg_msb, 16 * 1024),
            parse_nes2_rom_size_bytes(header[5], chr_msb, 8 * 1024),
        )
    } else {
        (
            header[4] as usize * 16 * 1024,
            header[5] as usize * 8 * 1024,
        )
    };

    let (prg_ram_size_bytes, prg_nvram_size_bytes, chr_ram_size_bytes, chr_nvram_size_bytes) =
        if nes2 {
            let prg_ram = parse_nes2_ram_size(header[10] & 0x0F);
            let prg_nvram = parse_nes2_ram_size(header[10] >> 4);
            let chr_ram = parse_nes2_ram_size(header[11] & 0x0F);
            let chr_nvram = parse_nes2_ram_size(header[11] >> 4);
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

    let timing_mode = if nes2 {
        match header[12] & 0x03 {
            0x00 => TimingMode::Ntsc,
            0x01 => TimingMode::Pal,
            0x02 => TimingMode::MultiRegion,
            0x03 => TimingMode::Dendy,
            value => TimingMode::Unknown(value),
        }
    } else if (header[9] & 0x01) != 0 {
        TimingMode::Pal
    } else {
        TimingMode::Ntsc
    };

    let (vs_ppu_type, vs_hardware_type) = if matches!(console_type, ConsoleType::VsSystem) && nes2 {
        (Some(header[13] & 0x0F), Some(header[13] >> 4))
    } else {
        (None, None)
    };

    let misc_roms = if nes2 { header[14] } else { 0 };
    let default_expansion_device = if nes2 { header[15] } else { 0 };

    Some(InesHeader {
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

/// Parse a full iNES/NES2 ROM blob. Returns the parsed header, owned PRG ROM
/// bytes, owned CHR ROM bytes, and the combined CRC32 of PRG+CHR.
/// Errors that can occur while parsing a full ROM blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomParseError {
    InvalidHeader,
    FileTooSmall { expected: usize, actual: usize },
}

pub fn parse_rom(data: &[u8]) -> Result<(InesHeader, Vec<u8>, Vec<u8>, u32), RomParseError> {
    if data.len() < 16 {
        return Err(RomParseError::FileTooSmall {
            expected: 16,
            actual: data.len(),
        });
    }

    let header: [u8; 16] = data[0..16]
        .try_into()
        .map_err(|_| RomParseError::InvalidHeader)?;
    let info = parse_header(&header).ok_or(RomParseError::InvalidHeader)?;

    let trainer_offset = if info.has_trainer { 512 } else { 0 };
    let prg_rom_start = 16 + trainer_offset;

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

    Ok((header_for_return, prg_rom, chr_rom, crc))
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

        let info = parse_header(&header).expect("header parse");
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

        let info = parse_header(&header).expect("nes2");
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

        let info = parse_header(&header).expect("header parse");
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

        let info = parse_header(&header).expect("v1 defaults");
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

        let info = parse_header(&header).expect("console timing");
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
        rom.extend(vec![0xBBu8; 1 * 8 * 1024]);

        let (hdr, prg, chr, crc) = parse_rom(&rom).expect("parse_rom v1");
        assert_eq!(hdr.header_version, "1.0");
        assert_eq!(prg.len(), 2 * 16 * 1024);
        assert_eq!(chr.len(), 1 * 8 * 1024);
        assert_eq!(crc, crate::cartridge::calculate_rom_crc32(&prg, &chr));
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
        rom.extend(vec![0xAAu8; 1 * 16 * 1024]);
        rom.extend(vec![0xBBu8; 1 * 8 * 1024]);

        let (hdr, prg, chr, _crc) = parse_rom(&rom).expect("parse_rom fallback");
        // Fallback should update reported sizes to v1 values
        assert_eq!(hdr.header_version, "2.0");
        assert_eq!(hdr.prg_rom_size_bytes, 1 * 16 * 1024);
        assert_eq!(hdr.chr_rom_size_bytes, 1 * 8 * 1024);
        assert_eq!(prg.len(), 1 * 16 * 1024);
        assert_eq!(chr.len(), 1 * 8 * 1024);
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

        let err = parse_rom(&rom).expect_err("should be too small");
        match err {
            RomParseError::FileTooSmall { expected: _, actual } => assert!(actual < 2000),
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn parse_rom_invalid_header() {
        // Provide a 16-byte buffer with an invalid magic to trigger InvalidHeader
        let mut data = vec![0u8; 16];
        data[0..4].copy_from_slice(b"BAD!");
        let err = parse_rom(&data).expect_err("invalid header");
        match err {
            RomParseError::InvalidHeader => {}
            _ => panic!("expected InvalidHeader"),
        }
    }
}
