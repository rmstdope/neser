//! ROM database module: CRC32 calculation and cartridge quirks.
//!
//! This module isolates ROM identification and quirk detection from mapper construction logic.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::cartridge::{ConsoleType, TimingMode};

const ROM_DB_COLUMN_COUNT: usize = 22;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomDbEntry {
    pub rom_id: Option<u16>,
    pub name: Option<String>,
    pub country: Option<String>,
    pub crc: Option<u32>,
    pub console_type: Option<ConsoleType>,
    pub console_region: Option<TimingMode>,
    pub rom_class: Option<String>,
    pub mapper: Option<u16>,
    pub submapper: Option<u8>,
    pub nametable_layout: Option<NametableLayout>,
    pub prg_rom_size: Option<u32>,
    pub prg_rom_crc: Option<u32>,
    pub prg_nvram_size: Option<u32>,
    pub prg_ram_size: Option<u32>,
    pub chr_rom_size: Option<u32>,
    pub chr_rom_crc: Option<u32>,
    pub chr_nvram_size: Option<u32>,
    pub chr_ram_size: Option<u32>,
    pub battery: Option<bool>,
    pub vs_hardware_type: Option<VsHardwareType>,
    pub vs_ppu_type: Option<VsPpuType>,
    pub expansion_type: Option<ExpansionType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NametableLayout {
    Horizontal,
    Vertical,
    OneScreenLower,
    OneScreenUpper,
    FourScreen,
    MapperControlled,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsHardwareType {
    Unisystem,
    RbiBaseball,
    TkoBoxing,
    SuperXevious,
    IceClimberJapan,
    VsDualSystem,
    RaidOnBungelingBay,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsPpuType {
    Rp2c03b,
    Rp2c03g,
    Rp2c04_0001,
    Rp2c04_0002,
    Rp2c04_0003,
    Rp2c04_0004,
    Rc2c03b,
    Rc2c03c,
    Rc2c05_01,
    Rc2c05_02,
    Rc2c05_03,
    Rc2c05_04,
    Rc2c05_05,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionType {
    None,
    Device(u8),
}

impl RomDbEntry {
    fn from_columns(columns: &[String]) -> Self {
        Self {
            rom_id: parse_optional_u16_decimal(&columns[0]),
            name: parse_optional_field(&columns[1]),
            country: parse_optional_field(&columns[2]),
            crc: parse_optional_u32_hex(&columns[3]),
            console_type: parse_optional_console_type(&columns[4]),
            console_region: parse_optional_timing_mode(&columns[5]),
            rom_class: parse_optional_field(&columns[6]),
            mapper: parse_optional_u16_decimal(&columns[7]),
            submapper: parse_optional_u8_decimal(&columns[8]),
            nametable_layout: parse_optional_nametable_layout(&columns[9]),
            prg_rom_size: parse_optional_u32_decimal(&columns[10]),
            prg_rom_crc: parse_optional_u32_hex(&columns[11]),
            prg_nvram_size: parse_optional_u32_decimal(&columns[12]),
            prg_ram_size: parse_optional_u32_decimal(&columns[13]),
            chr_rom_size: parse_optional_u32_decimal(&columns[14]),
            chr_rom_crc: parse_optional_u32_hex(&columns[15]),
            chr_nvram_size: parse_optional_u32_decimal(&columns[16]),
            chr_ram_size: parse_optional_u32_decimal(&columns[17]),
            battery: parse_optional_bool(&columns[18]),
            vs_hardware_type: parse_optional_vs_hardware_type(&columns[19]),
            vs_ppu_type: parse_optional_vs_ppu_type(&columns[20]),
            expansion_type: parse_optional_expansion_type(&columns[21]),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RomDb {
    entries: HashMap<u32, RomDbEntry>,
}

impl RomDb {
    pub fn new() -> io::Result<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("cartridge")
            .join("rom_db.csv");
        Self::from_path(path)
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::from_csv_content(&content))
    }

    #[cfg(test)]
    pub fn entries(&self) -> &HashMap<u32, RomDbEntry> {
        &self.entries
    }

    pub fn get_by_crc(&self, crc: u32) -> Option<&RomDbEntry> {
        self.entries.get(&crc)
    }

    fn from_csv_content(content: &str) -> Self {
        let mut entries = HashMap::new();

        for line in content.lines() {
            if let Some(entry) = parse_row(line)
                && let Some(crc) = entry.crc
            {
                entries.insert(crc, entry);
            }
        }

        Self { entries }
    }
}

fn parse_optional_field(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_optional_u16_decimal(raw: &str) -> Option<u16> {
    parse_optional_field(raw)?.parse::<u16>().ok()
}

fn parse_optional_u8_decimal(raw: &str) -> Option<u8> {
    parse_optional_field(raw)?.parse::<u8>().ok()
}

fn parse_optional_u32_decimal(raw: &str) -> Option<u32> {
    parse_optional_field(raw)?.parse::<u32>().ok()
}

fn parse_optional_u32_hex(raw: &str) -> Option<u32> {
    u32::from_str_radix(parse_optional_field(raw)?.as_str(), 16).ok()
}

fn parse_optional_bool(raw: &str) -> Option<bool> {
    let value = parse_optional_field(raw)?;
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn parse_optional_console_type(raw: &str) -> Option<ConsoleType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(match value {
        0 => ConsoleType::NesFamicom,
        1 => ConsoleType::VsSystem,
        2 => ConsoleType::Playchoice10,
        other => ConsoleType::Extended(other),
    })
}

fn parse_optional_timing_mode(raw: &str) -> Option<TimingMode> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(match value {
        0 => TimingMode::Ntsc,
        1 => TimingMode::Pal,
        2 => TimingMode::MultiRegion,
        3 => TimingMode::Dendy,
        other => TimingMode::Unknown(other),
    })
}

fn parse_optional_nametable_layout(raw: &str) -> Option<NametableLayout> {
    let value = parse_optional_field(raw)?;
    Some(match value.as_str() {
        "H" | "h" => NametableLayout::Horizontal,
        "V" | "v" => NametableLayout::Vertical,
        "0" => NametableLayout::Horizontal,
        "1" => NametableLayout::Vertical,
        "2" => NametableLayout::OneScreenLower,
        "3" => NametableLayout::OneScreenUpper,
        "4" => NametableLayout::FourScreen,
        "5" => NametableLayout::MapperControlled,
        _ => value
            .parse::<u8>()
            .map(NametableLayout::Unknown)
            .unwrap_or(NametableLayout::Unknown(0xFF)),
    })
}

fn parse_optional_vs_hardware_type(raw: &str) -> Option<VsHardwareType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(match value {
        0 => VsHardwareType::Unisystem,
        1 => VsHardwareType::RbiBaseball,
        2 => VsHardwareType::TkoBoxing,
        3 => VsHardwareType::SuperXevious,
        4 => VsHardwareType::IceClimberJapan,
        5 => VsHardwareType::VsDualSystem,
        6 => VsHardwareType::RaidOnBungelingBay,
        other => VsHardwareType::Unknown(other),
    })
}

fn parse_optional_vs_ppu_type(raw: &str) -> Option<VsPpuType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(match value {
        0 => VsPpuType::Rp2c03b,
        1 => VsPpuType::Rp2c03g,
        2 => VsPpuType::Rp2c04_0001,
        3 => VsPpuType::Rp2c04_0002,
        4 => VsPpuType::Rp2c04_0003,
        5 => VsPpuType::Rp2c04_0004,
        6 => VsPpuType::Rc2c03b,
        7 => VsPpuType::Rc2c03c,
        8 => VsPpuType::Rc2c05_01,
        9 => VsPpuType::Rc2c05_02,
        10 => VsPpuType::Rc2c05_03,
        11 => VsPpuType::Rc2c05_04,
        12 => VsPpuType::Rc2c05_05,
        other => VsPpuType::Unknown(other),
    })
}

fn parse_optional_expansion_type(raw: &str) -> Option<ExpansionType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(if value == 0 {
        ExpansionType::None
    } else {
        ExpansionType::Device(value)
    })
}

fn parse_row(line: &str) -> Option<RomDbEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let mut columns = normalize_columns(trimmed);
    if columns.len() < ROM_DB_COLUMN_COUNT {
        columns.resize(ROM_DB_COLUMN_COUNT, String::new());
    }

    Some(RomDbEntry::from_columns(&columns))
}

fn normalize_columns(line: &str) -> Vec<String> {
    let raw_columns: Vec<String> = line.split(',').map(ToString::to_string).collect();

    if raw_columns.len() <= ROM_DB_COLUMN_COUNT {
        return raw_columns;
    }

    let tail_start = raw_columns.len() - 20;
    let mut normalized = Vec::with_capacity(ROM_DB_COLUMN_COUNT);

    normalized.push(raw_columns[0].clone());
    normalized.push(raw_columns[1..tail_start].join(","));
    normalized.extend(raw_columns[tail_start..].iter().cloned());

    normalized
}

/// Calculate CRC32 of ROM data (PRG + CHR combined).
/// Uses the standard CRC-32 (ISO 3309) polynomial.
pub fn calculate_rom_crc32(prg_rom: &[u8], chr_rom: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut j = 0;
            while j < 8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };

    let mut crc = 0xFFFFFFFFu32;
    for &byte in prg_rom.iter().chain(chr_rom.iter()) {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    // crate::debugging::log_info(format!("Calculated CRC32: {:08X}", crc ^ 0xFFFFFFFF));
    !crc
}

/// CRC32 values for ROMs that require alternate (NEC) MMC3 IRQ behavior.
const MMC3_ALTERNATE_IRQ_CRCS: &[u32] = &[
    0x633AFE6F, // 6-MMC3_alt.nes (blargg mmc3_test_2)
    0xF312D1DE, // 5.MMC3_rev_A.nes (blargg mmc3_irq_tests)
    0xA512BDF6, // 6-MMC6.nes
];

/// CRC32 values for ROMs that default to Arkanoid controller input on port 2.
const ARKANOID_PADDLE_PORT2_CRCS: &[u32] = &[
    0x32FB0583, // Arkanoid (NES, 1987)
];

/// CRC32 values for ROMs that default to Arkanoid controller input on port 1.
const ARKANOID_PADDLE_PORT1_CRCS: &[u32] = &[
    0x47F9F410, // PaddleTest3
];

/// CRC32 values for ROMs that default to Zapper input on port 2.
const ZAPPER_PORT2_CRCS: &[u32] = &[
    0x24598791, // Duck Hunt (World)
    0xFF24D794, // Hogan's Alley (World)
];

/// Check if a ROM CRC requires alternate MMC3 IRQ behavior.
pub fn requires_mmc3_alternate_irq(crc: u32) -> bool {
    MMC3_ALTERNATE_IRQ_CRCS.contains(&crc)
}

/// Return the default Arkanoid controller port for a ROM CRC.
///
/// Returns 0 for none, 1 for port 1, 2 for port 2.
pub fn default_arkanoid_on_port(crc: u32) -> u8 {
    if ARKANOID_PADDLE_PORT1_CRCS.contains(&crc) {
        1
    } else if ARKANOID_PADDLE_PORT2_CRCS.contains(&crc) {
        2
    } else {
        0
    }
}

/// Return the default Zapper controller port for a ROM CRC.
///
/// Returns 0 for none, 2 for port 2.
pub fn default_zapper_on_port(crc: u32) -> u8 {
    if ZAPPER_PORT2_CRCS.contains(&crc) {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_rom_crc32_empty() {
        let crc = calculate_rom_crc32(&[], &[]);
        assert_eq!(crc, 0);
    }

    #[test]
    fn test_calculate_rom_crc32_with_data() {
        // Test with some sample data
        let prg_rom = vec![0x4E, 0x45, 0x53, 0x1A]; // "NES" header start
        let chr_rom = vec![0xFF, 0x00, 0xFF, 0x00];
        let crc = calculate_rom_crc32(&prg_rom, &chr_rom);
        // Verify the exact CRC32 value for this specific input
        assert_eq!(crc, 0xA26D5B91);
    }

    #[test]
    fn test_mmc3_alternate_irq_known_crcs() {
        // Known CRCs that require alternate IRQ
        assert!(requires_mmc3_alternate_irq(0x633AFE6F));
        assert!(requires_mmc3_alternate_irq(0xF312D1DE));
    }

    #[test]
    fn test_mmc3_alternate_irq_unknown_crc() {
        // Random CRC should not require alternate IRQ
        assert!(!requires_mmc3_alternate_irq(0x12345678));
    }

    #[test]
    fn test_arkanoid_paddle_known_crcs() {
        assert_eq!(default_arkanoid_on_port(0x32FB0583), 2);
        assert_eq!(default_arkanoid_on_port(0x47F9F410), 1);
    }

    #[test]
    fn test_arkanoid_paddle_unknown_crc() {
        assert_eq!(default_arkanoid_on_port(0xDEADBEEF), 0);
    }

    #[test]
    fn test_zapper_known_crc() {
        assert_eq!(default_zapper_on_port(0x24598791), 2);
    }

    #[test]
    fn test_zapper_unknown_crc() {
        assert_eq!(default_zapper_on_port(0xDEADBEEF), 0);
    }

    #[test]
    fn test_rom_db_ignores_comments_and_empty_rows() {
        let csv = "\n# comment\n1,Demo,,ABCDEF01\n\n# another\n2,Demo 2,,ABCDEF02\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.entries().len(), 2);
        assert_eq!(db.get_by_crc(0xABCDEF01).and_then(|e| e.rom_id), Some(1));
        assert_eq!(db.get_by_crc(0xABCDEF02).and_then(|e| e.rom_id), Some(2));
    }

    #[test]
    fn test_rom_db_omitted_values_are_unknown() {
        let csv = "1,Demo,,ABCDEF01,,,\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0xABCDEF01)
            .expect("entry should be found by CRC");

        assert_eq!(entry.rom_id, Some(1));
        assert_eq!(entry.name.as_deref(), Some("Demo"));
        assert_eq!(entry.country, None);
        assert_eq!(entry.crc, Some(0xABCDEF01));
        assert_eq!(entry.expansion_type, None);
    }

    #[test]
    fn test_rom_db_handles_name_with_comma() {
        let csv = "844,F-1 Hero 2, Nakajima Satoru Kanshuu,,1C2A58FF,,,Licensed Japan,4,,H,131072,B2AB361E,,,131072,89AAD993,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0x1C2A58FF)
            .expect("entry should be found by CRC");

        assert_eq!(entry.rom_id, Some(844));
        assert_eq!(
            entry.name.as_deref(),
            Some("F-1 Hero 2, Nakajima Satoru Kanshuu")
        );
        assert_eq!(entry.crc, Some(0x1C2A58FF));
        assert_eq!(entry.mapper, Some(4));
    }

    #[test]
    fn test_rom_db_parses_nes2_typed_fields() {
        let csv = "63,Battletoads,,9806CB84,0,1,Licensed Japan,7,2,H,262144,9806CB84,,8192,, , ,8192,1,5,10,2\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0x9806CB84)
            .expect("entry should be found by CRC");

        assert_eq!(entry.console_type, Some(ConsoleType::NesFamicom));
        assert_eq!(entry.console_region, Some(TimingMode::Pal));
        assert_eq!(entry.submapper, Some(2));
        assert_eq!(entry.nametable_layout, Some(NametableLayout::Horizontal));
        assert_eq!(entry.prg_rom_size, Some(262144));
        assert_eq!(entry.prg_ram_size, Some(8192));
        assert_eq!(entry.chr_ram_size, Some(8192));
        assert_eq!(entry.battery, Some(true));
        assert_eq!(entry.vs_hardware_type, Some(VsHardwareType::VsDualSystem));
        assert_eq!(entry.vs_ppu_type, Some(VsPpuType::Rc2c05_03));
        assert_eq!(entry.expansion_type, Some(ExpansionType::Device(2)));
    }

    #[test]
    fn test_rom_db_skips_entries_without_crc_key() {
        let csv = "1,Demo,,,,\n";
        let db = RomDb::from_csv_content(csv);
        assert!(db.entries().is_empty());
    }

    #[test]
    fn test_rom_db_get_entry_by_crc() {
        let csv = "1,Demo,,ABCDEF01\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db.get_by_crc(0xABCDEF01).expect("entry should be found");
        assert_eq!(entry.rom_id, Some(1));
    }
}
