//! GBA cartridge save-type detection.
//!
//! Most GBA cartridges do not advertise their save hardware via the ROM
//! header. Both Nintendo's first-party games and most homebrew embed a
//! plain-ASCII marker string somewhere in the ROM that the SDK linker put
//! there to guide flash-cart firmware (and, by convention, emulators).
//!
//! The markers we look for, in priority order, are:
//!
//! | Marker (ASCII)   | Meaning                                                  |
//! |------------------|----------------------------------------------------------|
//! | `EEPROM_V`       | EEPROM — variant (512 B vs 8 KB) chosen by ROM size      |
//! | `FLASH1M_V`      | 128 KB flash (two banks)                                 |
//! | `FLASH512_V`     | 64 KB flash, explicit                                    |
//! | `FLASH_V`        | 64 KB flash (Atmel/Macronix/etc., Panasonic MN63F805…)   |
//! | `SRAM_F_V`       | 32 KB SRAM with FRAM technology — same external behavior |
//! | `SRAM_V`         | 32 KB battery-backed SRAM                                |
//!
//! The ROM is searched in 4-byte aligned chunks because Nintendo's linker
//! always 4-byte-aligns these strings; that's also how mGBA's heuristic
//! works. If no marker is found we report [`SaveType::None`] and the caller
//! is free to fall back to a ROM database hint.

/// Save hardware variants supported by the cartridge layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SaveType {
    /// No persistent save hardware detected.
    None,
    /// 32 KB battery-backed SRAM (or FRAM) at `0x0E000000`.
    Sram32K,
    /// 512-byte EEPROM (6-bit address bus).
    Eeprom512,
    /// 8 KB EEPROM (14-bit address bus).
    Eeprom8K,
    /// 64 KB Flash, single bank.
    Flash64K,
    /// 128 KB Flash, two 64 KB banks switched at `0x0E005555`.
    Flash128K,
}

impl SaveType {
    /// Persistent storage size in bytes (`0` for [`SaveType::None`]).
    pub fn size_bytes(self) -> usize {
        match self {
            SaveType::None => 0,
            SaveType::Sram32K => 32 * 1024,
            SaveType::Eeprom512 => 512,
            SaveType::Eeprom8K => 8 * 1024,
            SaveType::Flash64K => 64 * 1024,
            SaveType::Flash128K => 128 * 1024,
        }
    }

    /// Human-readable name used in logs and the `.sav` flush path.
    pub fn label(self) -> &'static str {
        match self {
            SaveType::None => "None",
            SaveType::Sram32K => "SRAM",
            SaveType::Eeprom512 => "EEPROM 512B",
            SaveType::Eeprom8K => "EEPROM 8KB",
            SaveType::Flash64K => "Flash 64KB",
            SaveType::Flash128K => "Flash 128KB",
        }
    }
}

/// EEPROM size split: ROMs larger than 16 MB use the 8 KB variant, smaller
/// ROMs use the 512 B variant. This is the heuristic mGBA uses and matches
/// the way the GBA SDK chose addressing widths.
const EEPROM_LARGE_ROM_THRESHOLD: usize = 16 * 1024 * 1024;

/// Auto-detect the save type by scanning `rom` for SDK marker strings.
///
/// The search is 4-byte aligned and short-circuits on the first match.
/// `EEPROM_V` is checked first because some ROMs ship more than one marker
/// (e.g. a leftover `SRAM_V` from a porting kit) — EEPROM is typically the
/// one that matters for those titles, again matching mGBA's behavior.
pub fn detect_save_type(rom: &[u8]) -> SaveType {
    // EEPROM variant depends on overall ROM size.
    if find_marker(rom, b"EEPROM_V") {
        return if rom.len() > EEPROM_LARGE_ROM_THRESHOLD {
            SaveType::Eeprom8K
        } else {
            SaveType::Eeprom512
        };
    }
    if find_marker(rom, b"FLASH1M_V") {
        return SaveType::Flash128K;
    }
    if find_marker(rom, b"FLASH512_V") || find_marker(rom, b"FLASH_V") {
        return SaveType::Flash64K;
    }
    if find_marker(rom, b"SRAM_F_V") || find_marker(rom, b"SRAM_V") {
        return SaveType::Sram32K;
    }
    SaveType::None
}

/// Check whether `needle` appears at any 4-byte aligned offset in `rom`.
fn find_marker(rom: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || rom.len() < needle.len() {
        return false;
    }
    (0..=rom.len() - needle.len())
        .step_by(4)
        .any(|i| rom[i..].starts_with(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a ROM of `len` bytes with `marker` embedded at a 4-byte-aligned
    /// offset.
    fn rom_with_marker(len: usize, marker: &[u8], at: usize) -> Vec<u8> {
        assert!(at.is_multiple_of(4));
        assert!(at + marker.len() <= len);
        let mut rom = vec![0u8; len];
        rom[at..at + marker.len()].copy_from_slice(marker);
        rom
    }

    #[test]
    fn no_marker_returns_none() {
        let rom = vec![0u8; 1024];
        assert_eq!(detect_save_type(&rom), SaveType::None);
    }

    #[test]
    fn sram_marker_detected() {
        let rom = rom_with_marker(2048, b"SRAM_Vnnn\0", 256);
        assert_eq!(detect_save_type(&rom), SaveType::Sram32K);
    }

    #[test]
    fn sram_f_marker_also_detected_as_sram() {
        let rom = rom_with_marker(2048, b"SRAM_F_Vnnn\0", 512);
        assert_eq!(detect_save_type(&rom), SaveType::Sram32K);
    }

    #[test]
    fn flash_v_marker_detected_as_64k() {
        let rom = rom_with_marker(4096, b"FLASH_Vxxx", 64);
        assert_eq!(detect_save_type(&rom), SaveType::Flash64K);
    }

    #[test]
    fn flash512_marker_detected_as_64k() {
        let rom = rom_with_marker(4096, b"FLASH512_Vxxx", 64);
        assert_eq!(detect_save_type(&rom), SaveType::Flash64K);
    }

    #[test]
    fn flash1m_marker_detected_as_128k() {
        let rom = rom_with_marker(4096, b"FLASH1M_Vxxx", 64);
        assert_eq!(detect_save_type(&rom), SaveType::Flash128K);
    }

    #[test]
    fn eeprom_marker_small_rom_is_512b() {
        let rom = rom_with_marker(8 * 1024 * 1024, b"EEPROM_V124", 1024);
        assert_eq!(detect_save_type(&rom), SaveType::Eeprom512);
    }

    #[test]
    fn eeprom_marker_large_rom_is_8kb() {
        let rom = rom_with_marker(20 * 1024 * 1024, b"EEPROM_V124", 1024);
        assert_eq!(detect_save_type(&rom), SaveType::Eeprom8K);
    }

    #[test]
    fn eeprom_takes_priority_over_other_markers() {
        // ROM that has both SRAM_V and EEPROM_V: EEPROM wins.
        let mut rom = vec![0u8; 8192];
        rom[64..64 + 6].copy_from_slice(b"SRAM_V");
        rom[256..256 + 8].copy_from_slice(b"EEPROM_V");
        assert_eq!(detect_save_type(&rom), SaveType::Eeprom512);
    }

    #[test]
    fn marker_at_unaligned_offset_is_ignored() {
        // Marker placed at offset 1 (not 4-byte aligned) — should not match.
        let mut rom = vec![0u8; 1024];
        rom[1..1 + 6].copy_from_slice(b"SRAM_V");
        assert_eq!(detect_save_type(&rom), SaveType::None);
    }

    #[test]
    fn save_type_size_bytes_matches_label() {
        assert_eq!(SaveType::None.size_bytes(), 0);
        assert_eq!(SaveType::Sram32K.size_bytes(), 32 * 1024);
        assert_eq!(SaveType::Eeprom512.size_bytes(), 512);
        assert_eq!(SaveType::Eeprom8K.size_bytes(), 8 * 1024);
        assert_eq!(SaveType::Flash64K.size_bytes(), 64 * 1024);
        assert_eq!(SaveType::Flash128K.size_bytes(), 128 * 1024);
    }
}
