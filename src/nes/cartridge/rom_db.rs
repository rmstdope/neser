//! ROM database module: CRC32 calculation and cartridge quirks.
//!
//! This module isolates ROM identification and quirk detection from mapper construction logic.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::nes::cartridge::{HardwareType, NametableLayout};
use num_enum::TryFromPrimitive;

const ROM_DB_COLUMN_COUNT: usize = 21;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomDbEntry {
    pub rom_id: Option<u16>,
    pub name: Option<String>,
    pub country: Option<String>,
    pub crc: Option<u32>,
    pub hardware: Option<HardwareType>,
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

impl VsHardwareType {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Unisystem,
            1 => Self::RbiBaseball,
            2 => Self::TkoBoxing,
            3 => Self::SuperXevious,
            4 => Self::IceClimberJapan,
            5 => Self::VsDualSystem,
            6 => Self::RaidOnBungelingBay,
            other => Self::Unknown(other),
        }
    }

    pub fn to_raw(self) -> u8 {
        match self {
            Self::Unisystem => 0,
            Self::RbiBaseball => 1,
            Self::TkoBoxing => 2,
            Self::SuperXevious => 3,
            Self::IceClimberJapan => 4,
            Self::VsDualSystem => 5,
            Self::RaidOnBungelingBay => 6,
            Self::Unknown(v) => v,
        }
    }
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

impl VsPpuType {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Rp2c03b,
            1 => Self::Rp2c03g,
            2 => Self::Rp2c04_0001,
            3 => Self::Rp2c04_0002,
            4 => Self::Rp2c04_0003,
            5 => Self::Rp2c04_0004,
            6 => Self::Rc2c03b,
            7 => Self::Rc2c03c,
            8 => Self::Rc2c05_01,
            9 => Self::Rc2c05_02,
            10 => Self::Rc2c05_03,
            11 => Self::Rc2c05_04,
            12 => Self::Rc2c05_05,
            other => Self::Unknown(other),
        }
    }

    pub fn to_raw(self) -> u8 {
        match self {
            Self::Rp2c03b => 0,
            Self::Rp2c03g => 1,
            Self::Rp2c04_0001 => 2,
            Self::Rp2c04_0002 => 3,
            Self::Rp2c04_0003 => 4,
            Self::Rp2c04_0004 => 5,
            Self::Rc2c03b => 6,
            Self::Rc2c03c => 7,
            Self::Rc2c05_01 => 8,
            Self::Rc2c05_02 => 9,
            Self::Rc2c05_03 => 10,
            Self::Rc2c05_04 => 11,
            Self::Rc2c05_05 => 12,
            Self::Unknown(v) => v,
        }
    }
}

/// iNES 2.0 Controller types
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum ExpansionType {
    Unspecified = 0x00,
    StandardControllers = 0x01,
    NesFourScore = 0x02,
    FamicomFourPlayersSimple = 0x03,
    VsSystem4016 = 0x04,
    VsSystem4017 = 0x05,
    Reserved = 0x06,
    VsZapper = 0x07,
    Zapper4017 = 0x08,
    TwoZappers = 0x09,
    BandaiHyperShot = 0x0A,
    PowerPadSideA = 0x0B,
    PowerPadSideB = 0x0C,
    FamilyTrainerSideA = 0x0D,
    FamilyTrainerSideB = 0x0E,
    ArkanoidVausNes = 0x0F,
    ArkanoidVausFamicom = 0x10,
    TwoVausDataRecorder = 0x11,
    KonamiHyperShot = 0x12,
    CoconutsPachinko = 0x13,
    ExcitingBoxingBag = 0x14,
    JissenMahjong = 0x15,
    YonezawaPartyTap = 0x16,
    OekaKidsTablet = 0x17,
    SunsoftBarcodeBattler = 0x18,
    MiraclePiano = 0x19,
    PokkunMoguraaTapMat = 0x1A,
    TopRider = 0x1B,
    DoubleFisted = 0x1C,
    Famicom3dSystem = 0x1D,
    DoremikkoKeyboard = 0x1E,
    RobGyromite = 0x1F,
    FamicomDataRecorder = 0x20,
    AsciiTurboFile = 0x21,
    IgsStorageBattleBox = 0x22,
    FamilyBasicKeyboardRecorder = 0x23,
    DongdaPecKeyboard = 0x24,
    PuzeBit79Keyboard = 0x25,
    SuborKeyboard = 0x26,
    SuborKeyboardMacroMouse = 0x27,
    SuborKeyboardSuborMouse4016 = 0x28,
    SnesMouse4016 = 0x29,
    Multicart = 0x2A,
    TwoSnesControllers = 0x2B,
    RacermateBicycle = 0x2C,
    UForce = 0x2D,
    RobStackUp = 0x2E,
    CityPatrolmanLightgun = 0x2F,
    SharpC1Cassette = 0x30,
    StandardSwapDpadBa = 0x31,
    ExcaliburSudokuPad = 0x32,
    AblPinball = 0x33,
    GoldenNuggetButtons = 0x34,
    KedaKeyboard = 0x35,
    SuborKeyboardSuborMouse4017 = 0x36,
    PortTestController = 0x37,
    BandaiMultiGamePlayer = 0x38,
    VenomTvDanceMat = 0x39,
    LgTvRemote = 0x3A,
    FamicomNetworkController = 0x3B,
    KingFishingController = 0x3C,
    CroakyKaraoke = 0x3D,
    KewangKingwonKeyboard = 0x3E,
    ZechengKeyboard = 0x3F,
    SuborKeyboardPs2Mouse4017L90 = 0x40,
    Um6578Ps2KeyboardMouse4017 = 0x41,
    Um6578Ps2Mouse = 0x42,
    YuxingMouse4016 = 0x43,
    SuborKeyboardYuxingMouse4016 = 0x44,
    GigggleTvPump = 0x45,
    BbkKeyboardPs2Mouse4017R90 = 0x46,
    MagicalCooking = 0x47,
    SnesMouse4017 = 0x48,
    Zapper4016 = 0x49,
    ArkanoidVausProto = 0x4A,
    TvMahjongGame = 0x4B,
    MahjongGekitouDensetu = 0x4C,
    SuborKeyboardPs2Mouse4017Xinv = 0x4D,
    IbmPcXtKeyboard = 0x4E,
    SuborKeyboardMegaBookMouse = 0x4F,
    // Values not in iNES 2.0 spec:
    PowerGlove = 0x51,
    AladdinDeckEnhancer = 0x52,
}

impl RomDbEntry {
    fn from_columns(columns: &[String]) -> Self {
        Self {
            rom_id: parse_optional_u16_decimal(&columns[0]),
            name: parse_optional_field(&columns[1]),
            country: parse_optional_field(&columns[2]),
            crc: parse_optional_u32_hex(&columns[3]),
            hardware: parse_optional_hardware_type(&columns[4]),
            rom_class: parse_optional_field(&columns[5]),
            mapper: parse_optional_u16_decimal(&columns[6]),
            submapper: parse_optional_u8_decimal(&columns[7]),
            nametable_layout: parse_optional_nametable_layout(&columns[8]),
            prg_rom_size: parse_optional_u32_decimal(&columns[9]),
            prg_rom_crc: parse_optional_u32_hex(&columns[10]),
            prg_nvram_size: parse_optional_u32_decimal(&columns[11]),
            prg_ram_size: parse_optional_u32_decimal(&columns[12]),
            chr_rom_size: parse_optional_u32_decimal(&columns[13]),
            chr_rom_crc: parse_optional_u32_hex(&columns[14]),
            chr_nvram_size: parse_optional_u32_decimal(&columns[15]),
            chr_ram_size: parse_optional_u32_decimal(&columns[16]),
            battery: parse_optional_bool(&columns[17]),
            vs_hardware_type: parse_optional_vs_hardware_type(&columns[18]),
            vs_ppu_type: parse_optional_vs_ppu_type(&columns[19]),
            expansion_type: parse_optional_expansion_type(&columns[20]),
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
            .join("nes")
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

    /// Return the default Zapper controller port for a ROM CRC.
    ///
    /// Returns 2 when the ROM DB entry exists and expansion type is ZAPPER_4017,
    /// otherwise returns 0.
    pub fn default_zapper_on_port(&self, crc32: u32) -> u8 {
        match self
            .get_by_crc(crc32)
            .and_then(|entry| entry.expansion_type)
        {
            Some(ExpansionType::Zapper4017) => 2,
            Some(ExpansionType::VsZapper | ExpansionType::TwoZappers) => 2,
            Some(ExpansionType::Zapper4016) => 1,
            _ => 0,
        }
    }

    /// Return whether ROM DB expansion type implies Famicom four-player expansion wiring.
    pub fn has_famicom_four_players_expansion(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::FamicomFourPlayersSimple)
        )
    }

    /// Return whether ROM DB expansion type implies Famicom Arkanoid controller on expansion port.
    pub fn has_arkanoid_famicom_expansion(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::ArkanoidVausFamicom)
        )
    }

    /// Return whether ROM DB expansion type implies Famicom Zapper on expansion port.
    pub fn has_zapper_famicom_expansion(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::Zapper4016 | ExpansionType::Zapper4017)
        )
    }

    /// Return whether ROM DB expansion type is VsSystem4017 (swapped controller wiring).
    /// These VS games read P1 from $4017 (left stick) instead of $4016.
    pub fn has_vs_swapped_controllers(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::VsSystem4017)
        )
    }

    /// Return the default Power Pad controller port for a ROM CRC.
    ///
    /// Returns 2 for NES Power Pad games (PowerPadSideA/B), since the
    /// Power Pad connects to controller port 2 on the NES.
    /// Returns 0 when no NES Power Pad is detected.
    pub fn default_power_pad_on_port(&self, crc32: u32) -> u8 {
        match self
            .get_by_crc(crc32)
            .and_then(|entry| entry.expansion_type)
        {
            Some(ExpansionType::PowerPadSideA | ExpansionType::PowerPadSideB) => 2,
            _ => 0,
        }
    }

    /// Return whether ROM DB expansion type implies a Famicom Family Trainer
    /// (the Japanese equivalent of the Power Pad) on the expansion port.
    pub fn has_power_pad_famicom_expansion(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::FamilyTrainerSideA | ExpansionType::FamilyTrainerSideB)
        )
    }

    /// Return whether ROM DB expansion type implies NES Four Score adapter.
    pub fn has_nes_four_score_expansion(&self, crc32: u32) -> bool {
        matches!(
            self.get_by_crc(crc32)
                .and_then(|entry| entry.expansion_type),
            Some(ExpansionType::NesFourScore)
        )
    }

    // NOTE: Not every ExpansionType variant needs a dedicated auto-detection helper here.
    // Some supported peripherals are handled by other logic, for example:
    //   - ArkanoidVausNes (0x0F): handled by Arkanoid/NES-specific controller logic
    //   - VsZapper (0x07) / TwoZappers (0x09): handled by default_zapper_on_port()
    //   - PowerPadSideA/B (0x0B/0x0C): handled by default_power_pad_on_port()
    // Others still have no helper in this module because the corresponding peripherals or
    // wiring are not currently supported here, e.g. BandaiHyperShot, FamicomFourPlayersLong,
    // and keyboard / MIDI / Mahjong / Excitebike-style devices. Additional helpers can be
    // added in the future if ROM DB metadata becomes useful for those cases.

    /// Return whether ROM DB rom_class indicates a Japan-region (Famicom) ROM.
    pub fn is_japan_region(&self, crc32: u32) -> bool {
        self.get_by_crc(crc32)
            .and_then(|entry| entry.rom_class.as_ref())
            .is_some_and(|rc| rc.contains("Japan"))
    }

    pub(crate) fn from_csv_content(content: &str) -> Self {
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

fn parse_optional_hardware_type(raw: &str) -> Option<HardwareType> {
    let value = parse_optional_u8_decimal(raw)?;
    HardwareType::from_db_value(value)
}

fn parse_optional_nametable_layout(raw: &str) -> Option<NametableLayout> {
    let value = parse_optional_field(raw)?;
    match value.as_str() {
        "H" | "h" => Some(NametableLayout::Horizontal),
        "V" | "v" => Some(NametableLayout::Vertical),
        "0" => Some(NametableLayout::Horizontal),
        "1" => Some(NametableLayout::Vertical),
        "2" => Some(NametableLayout::SingleScreenLower),
        "3" => Some(NametableLayout::SingleScreenUpper),
        "4" => Some(NametableLayout::FourScreen),
        "5" => None,
        _ => None,
    }
}

fn parse_optional_vs_hardware_type(raw: &str) -> Option<VsHardwareType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(VsHardwareType::from_raw(value))
}

fn parse_optional_vs_ppu_type(raw: &str) -> Option<VsPpuType> {
    let value = parse_optional_u8_decimal(raw)?;
    Some(VsPpuType::from_raw(value))
}

fn parse_optional_expansion_type(raw: &str) -> Option<ExpansionType> {
    let value = parse_optional_u8_decimal(raw)?;
    ExpansionType::try_from(value).ok()
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

    let tail_start = raw_columns.len() - (ROM_DB_COLUMN_COUNT - 2);
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
    fn test_rom_db_default_zapper_on_port_when_expansion_is_zapper_4017() {
        let csv = "1,Zapper Demo,,24598791,,,,,,,,,,,,,,,,,8\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0x24598791), 2);
    }

    #[test]
    fn test_rom_db_default_zapper_on_port_when_expansion_is_zapper_4016() {
        let csv = "1,Zapper Demo,,24598791,,,,,,,,,,,,,,,,,73\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0x24598791), 1);
    }

    #[test]
    fn test_rom_db_default_zapper_on_port_when_expansion_is_two_zappers() {
        let csv = "1,Zapper Demo,,24598791,,,,,,,,,,,,,,,,,9\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0x24598791), 2);
    }

    #[test]
    fn test_rom_db_default_zapper_on_port_when_expansion_is_vs_zapper() {
        let csv = "1,Zapper Demo,,24598791,,,,,,,,,,,,,,,,,7\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0x24598791), 2);
    }

    #[test]
    fn test_rom_db_default_zapper_on_port_when_expansion_is_not_zapper_4017() {
        let csv = "1,No Zapper,,24598791,,,,,,,,,,,,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0x24598791), 0);
    }

    #[test]
    fn test_rom_db_default_zapper_on_port_when_crc_is_missing() {
        let csv = "1,Zapper Demo,,24598791,,,,,,,,,,,,,,,,,8\n";
        let db = RomDb::from_csv_content(csv);

        assert_eq!(db.default_zapper_on_port(0xDEADBEEF), 0);
    }

    #[test]
    fn test_rom_db_has_famicom_four_players_expansion_when_expansion_is_famicom_four_players() {
        let csv = "1,4P Demo,,24598791,,,,,,,,,,,,,,,,,3\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.has_famicom_four_players_expansion(0x24598791));
    }

    #[test]
    fn test_rom_db_has_famicom_four_players_expansion_is_false_for_other_expansion() {
        let csv = "1,2P Demo,,24598791,,,,,,,,,,,,,,,,,2\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_famicom_four_players_expansion(0x24598791));
        assert!(!db.has_famicom_four_players_expansion(0xDEADBEEF));
    }

    #[test]
    fn test_rom_db_has_arkanoid_famicom_expansion_when_expansion_is_arkanoid_vaus_famicom() {
        // ExpansionType::ArkanoidVausFamicom = 0x10 = 16
        let csv = "1,Arkanoid FC,,24598791,,,,,,,,,,,,,,,,,16\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.has_arkanoid_famicom_expansion(0x24598791));
    }

    #[test]
    fn test_rom_db_has_arkanoid_famicom_expansion_is_false_for_nes_arkanoid() {
        // ExpansionType::ArkanoidVausNes = 0x0F = 15
        let csv = "1,Arkanoid NES,,24598791,,,,,,,,,,,,,,,,,15\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_arkanoid_famicom_expansion(0x24598791));
    }

    #[test]
    fn test_rom_db_has_arkanoid_famicom_expansion_is_false_for_unknown_crc() {
        let csv = "1,Arkanoid FC,,24598791,,,,,,,,,,,,,,,,,16\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_arkanoid_famicom_expansion(0xDEADBEEF));
    }

    #[test]
    fn test_rom_db_is_japan_region_for_licensed_japan() {
        let csv = "1,Japanese Game,,24598791,,Licensed Japan,,,,,,,,,,,,,,\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.is_japan_region(0x24598791));
    }

    #[test]
    fn test_rom_db_is_japan_region_for_unlicensed_japan() {
        let csv = "1,Unlicensed Game,,24598791,,Unlicensed Japan,,,,,,,,,,,,,,\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.is_japan_region(0x24598791));
    }

    #[test]
    fn test_rom_db_is_japan_region_is_false_for_north_america() {
        let csv = "1,NA Game,,24598791,,Licensed North America,,,,,,,,,,,,,,\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.is_japan_region(0x24598791));
    }

    #[test]
    fn test_rom_db_is_japan_region_is_false_for_unknown_crc() {
        let csv = "1,Japanese Game,,24598791,,Licensed Japan,,,,,,,,,,,,,,\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.is_japan_region(0xDEADBEEF));
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
        let csv = "1,Demo,,ABCDEF01,,\n";
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
        let csv = "844,F-1 Hero 2, Nakajima Satoru Kanshuu,,1C2A58FF,,Licensed Japan,4,,H,131072,B2AB361E,,,131072,89AAD993,,,,,,1\n";
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
        // hardware=1 (NesPal), rom_class=Licensed Japan, mapper=7, submapper=2, etc.
        let csv = "63,Battletoads,,9806CB84,1,Licensed Japan,7,2,H,262144,9806CB84,,8192,, , ,8192,1,5,10,2\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0x9806CB84)
            .expect("entry should be found by CRC");

        assert_eq!(entry.hardware, Some(HardwareType::NesPal));
        assert_eq!(entry.submapper, Some(2));
        assert_eq!(entry.nametable_layout, Some(NametableLayout::Horizontal));
        assert_eq!(entry.prg_rom_size, Some(262144));
        assert_eq!(entry.prg_ram_size, Some(8192));
        assert_eq!(entry.chr_ram_size, Some(8192));
        assert_eq!(entry.battery, Some(true));
        assert_eq!(entry.vs_hardware_type, Some(VsHardwareType::VsDualSystem));
        assert_eq!(entry.vs_ppu_type, Some(VsPpuType::Rc2c05_03));
        assert_eq!(entry.expansion_type, Some(ExpansionType::NesFourScore));
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

    #[test]
    fn test_rom_db_nametable_layout_uses_mirroring_mode_enum() {
        let csv = "63,Battletoads,,9806CB84,1,Licensed Japan,7,2,H,262144,9806CB84,,8192,, , ,8192,1,5,10,2\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0x9806CB84)
            .expect("entry should be found by CRC");

        assert_eq!(
            entry.nametable_layout,
            Some(crate::nes::cartridge::NametableLayout::Horizontal)
        );
    }

    #[test]
    fn test_rom_db_nametable_layout_mapper_controlled_is_none() {
        let csv = "1,Demo,,ABCDEF01,0,Class,0,0,5,16384,ABCDEF01,0,0,0,0,0,0,0,0,0,0\n";
        let db = RomDb::from_csv_content(csv);
        let entry = db
            .get_by_crc(0xABCDEF01)
            .expect("entry should be found by CRC");

        assert_eq!(entry.nametable_layout, None);
    }

    #[test]
    fn test_rom_db_contains_mapper_override_for_sweet_home_translation_crc() {
        let db = RomDb::from_csv_content(include_str!("rom_db.csv"));
        let entry = db
            .get_by_crc(0x3AAF_F278)
            .expect("patched Sweet Home CRC should be present in ROM DB");

        assert_eq!(entry.mapper, Some(1));
    }

    #[test]
    fn test_has_zapper_famicom_expansion_for_zapper_4017() {
        let csv = "1,Duck Hunt Famicom,,24598791,,,,,,,,,,,,,,,,,8\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.has_zapper_famicom_expansion(0x24598791));
    }

    #[test]
    fn test_has_zapper_famicom_expansion_for_zapper_4016() {
        let csv = "1,Zapper Game,,24598791,,,,,,,,,,,,,,,,,73\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.has_zapper_famicom_expansion(0x24598791));
    }

    #[test]
    fn test_has_zapper_famicom_expansion_false_for_standard_controllers() {
        let csv = "1,Normal Game,,24598791,,,,,,,,,,,,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_zapper_famicom_expansion(0x24598791));
    }

    #[test]
    fn test_has_zapper_famicom_expansion_false_for_unknown_crc() {
        let csv = "1,Duck Hunt Famicom,,24598791,,,,,,,,,,,,,,,,,8\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_zapper_famicom_expansion(0xDEADBEEF));
    }

    #[test]
    fn vs_hardware_type_from_raw_maps_known_values() {
        assert_eq!(VsHardwareType::from_raw(0), VsHardwareType::Unisystem);
        assert_eq!(VsHardwareType::from_raw(1), VsHardwareType::RbiBaseball);
        assert_eq!(VsHardwareType::from_raw(2), VsHardwareType::TkoBoxing);
        assert_eq!(VsHardwareType::from_raw(3), VsHardwareType::SuperXevious);
        assert_eq!(VsHardwareType::from_raw(4), VsHardwareType::IceClimberJapan);
        assert_eq!(VsHardwareType::from_raw(5), VsHardwareType::VsDualSystem);
        assert_eq!(
            VsHardwareType::from_raw(6),
            VsHardwareType::RaidOnBungelingBay
        );
        assert_eq!(VsHardwareType::from_raw(7), VsHardwareType::Unknown(7));
    }

    #[test]
    fn vs_hardware_type_to_raw_roundtrips() {
        for raw in 0..=6 {
            assert_eq!(VsHardwareType::from_raw(raw).to_raw(), raw);
        }
    }

    #[test]
    fn vs_ppu_type_from_raw_maps_known_values() {
        assert_eq!(VsPpuType::from_raw(0), VsPpuType::Rp2c03b);
        assert_eq!(VsPpuType::from_raw(2), VsPpuType::Rp2c04_0001);
        assert_eq!(VsPpuType::from_raw(5), VsPpuType::Rp2c04_0004);
        assert_eq!(VsPpuType::from_raw(8), VsPpuType::Rc2c05_01);
        assert_eq!(VsPpuType::from_raw(12), VsPpuType::Rc2c05_05);
        assert_eq!(VsPpuType::from_raw(13), VsPpuType::Unknown(13));
    }

    #[test]
    fn vs_ppu_type_to_raw_roundtrips() {
        for raw in 0..=12 {
            assert_eq!(VsPpuType::from_raw(raw).to_raw(), raw);
        }
    }

    // --- Power Pad ROM DB detection tests ---

    #[test]
    fn test_default_power_pad_on_port_returns_2_for_power_pad_side_b() {
        // expansion_type 12 = PowerPadSideB
        let csv = "1,World Class Track Meet,,ABCD1234,,,,,,,,,,,,,,,,,12\n";
        let db = RomDb::from_csv_content(csv);
        assert_eq!(db.default_power_pad_on_port(0xABCD1234), 2);
    }

    #[test]
    fn test_default_power_pad_on_port_returns_2_for_power_pad_side_a() {
        // expansion_type 11 = PowerPadSideA
        let csv = "1,Power Pad Game,,ABCD1234,,,,,,,,,,,,,,,,,11\n";
        let db = RomDb::from_csv_content(csv);
        assert_eq!(db.default_power_pad_on_port(0xABCD1234), 2);
    }

    #[test]
    fn test_default_power_pad_on_port_returns_0_for_family_trainer() {
        // expansion_type 13 = FamilyTrainerSideA (Famicom expansion, not NES port)
        let csv = "1,Family Trainer,,ABCD1234,,,,,,,,,,,,,,,,,13\n";
        let db = RomDb::from_csv_content(csv);
        assert_eq!(db.default_power_pad_on_port(0xABCD1234), 0);
    }

    #[test]
    fn test_default_power_pad_on_port_returns_0_for_standard_controllers() {
        let csv = "1,Normal Game,,ABCD1234,,,,,,,,,,,,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);
        assert_eq!(db.default_power_pad_on_port(0xABCD1234), 0);
    }

    #[test]
    fn test_has_power_pad_famicom_expansion_true_for_family_trainer_side_a() {
        // expansion_type 13 = FamilyTrainerSideA
        let csv = "1,Athletic World,,ABCD1234,,,,,,,,,,,,,,,,,13\n";
        let db = RomDb::from_csv_content(csv);
        assert!(db.has_power_pad_famicom_expansion(0xABCD1234));
    }

    #[test]
    fn test_has_power_pad_famicom_expansion_true_for_family_trainer_side_b() {
        // expansion_type 14 = FamilyTrainerSideB
        let csv = "1,Running Stadium,,ABCD1234,,,,,,,,,,,,,,,,,14\n";
        let db = RomDb::from_csv_content(csv);
        assert!(db.has_power_pad_famicom_expansion(0xABCD1234));
    }

    #[test]
    fn test_has_power_pad_famicom_expansion_false_for_nes_power_pad() {
        // expansion_type 12 = PowerPadSideB (NES port, not Famicom expansion)
        let csv = "1,World Class Track Meet,,ABCD1234,,,,,,,,,,,,,,,,,12\n";
        let db = RomDb::from_csv_content(csv);
        assert!(!db.has_power_pad_famicom_expansion(0xABCD1234));
    }

    #[test]
    fn test_has_power_pad_famicom_expansion_false_for_standard() {
        let csv = "1,Normal,,ABCD1234,,,,,,,,,,,,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);
        assert!(!db.has_power_pad_famicom_expansion(0xABCD1234));
    }

    // ExpansionType::NesFourScore = 0x02
    #[test]
    fn test_rom_db_has_nes_four_score_expansion_when_expansion_is_nes_four_score() {
        let csv = "1,Bomberman II,,DEADC0DE,,,,,,,,,,,,,,,,,2\n";
        let db = RomDb::from_csv_content(csv);

        assert!(db.has_nes_four_score_expansion(0xDEADC0DE));
    }

    #[test]
    fn test_rom_db_has_nes_four_score_expansion_is_false_for_other_expansion() {
        // ExpansionType::StandardControllers = 0x01
        let csv = "1,Standard Game,,DEADC0DE,,,,,,,,,,,,,,,,,1\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_nes_four_score_expansion(0xDEADC0DE));
    }

    #[test]
    fn test_rom_db_has_nes_four_score_expansion_is_false_for_unknown_crc() {
        let csv = "1,Bomberman II,,DEADC0DE,,,,,,,,,,,,,,,,,2\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_nes_four_score_expansion(0xDEADBEEF));
    }

    #[test]
    fn test_rom_db_has_nes_four_score_expansion_is_false_for_famicom_four_players() {
        // ExpansionType::FamicomFourPlayersSimple = 0x03 — should NOT trigger NES Four Score
        let csv = "1,Famicom 4P,,DEADC0DE,,,,,,,,,,,,,,,,,3\n";
        let db = RomDb::from_csv_content(csv);

        assert!(!db.has_nes_four_score_expansion(0xDEADC0DE));
    }
}
