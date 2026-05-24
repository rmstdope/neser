//! Top-level GBA cartridge: ROM image + auto-detected save backend.
//!
//! [`load_cartridge`] is the entry point: it validates ROM size, parses the
//! header, runs the save-type heuristic, and returns a [`GbaCartridge`]
//! bundling the ROM bytes with the appropriate save backend.

use crate::gba::cartridge::eeprom::{Eeprom, EepromState};
use crate::gba::cartridge::flash::{Flash, FlashStateSnapshot};
use crate::gba::cartridge::header::{GbaHeader, HEADER_SIZE, HeaderError, parse_header};
use crate::gba::cartridge::save_type::{SaveType, detect_save_type};
use crate::gba::cartridge::sram::{Sram, SramState};
use serde::{Deserialize, Serialize};

/// Maximum cartridge ROM size addressable on the GBA (32 MB).
pub const ROM_MAX_SIZE: usize = 32 * 1024 * 1024;

/// Errors returned by [`load_cartridge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    /// ROM is shorter than the GBA header (192 bytes).
    TooShort,
    /// ROM is larger than [`ROM_MAX_SIZE`].
    TooLarge {
        actual: usize,
        max: usize,
    },
    HeaderParse(HeaderError),
}

impl From<HeaderError> for CartridgeError {
    fn from(err: HeaderError) -> Self {
        CartridgeError::HeaderParse(err)
    }
}

/// Persistent save backend selected for this cartridge.
#[derive(Debug)]
pub enum SaveBackend {
    /// No save hardware.
    None,
    /// 32 KB SRAM.
    Sram(Sram),
    /// EEPROM (512 B or 8 KB).
    Eeprom(Eeprom),
    /// Flash (64 KB single-bank or 128 KB dual-bank).
    Flash(Flash),
}

/// Serializable cartridge save-backend snapshot for GBA save states.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SaveBackendState {
    #[default]
    None,
    Sram(SramState),
    Eeprom(EepromState),
    Flash(FlashStateSnapshot),
}

impl SaveBackend {
    /// Build a fresh save backend for the given save type.
    fn for_save_type(save_type: SaveType) -> Self {
        match save_type {
            SaveType::None => SaveBackend::None,
            SaveType::Sram32K => SaveBackend::Sram(Sram::new()),
            SaveType::Eeprom512 | SaveType::Eeprom8K => SaveBackend::Eeprom(Eeprom::new(save_type)),
            SaveType::Flash64K | SaveType::Flash128K => SaveBackend::Flash(Flash::new(save_type)),
        }
    }

    /// Borrow the byte buffer that should be written to a `.sav` file.
    /// Returns `&[]` for `None`.
    pub fn snapshot(&self) -> &[u8] {
        match self {
            SaveBackend::None => &[],
            SaveBackend::Sram(s) => s.snapshot(),
            SaveBackend::Eeprom(e) => e.snapshot(),
            SaveBackend::Flash(f) => f.snapshot(),
        }
    }

    /// Restore the save buffer from raw bytes (e.g. a previously written
    /// `.sav` file). Bytes past the chip capacity are silently dropped.
    pub fn restore(&mut self, data: &[u8]) {
        match self {
            SaveBackend::None => {}
            SaveBackend::Sram(s) => s.restore(data),
            SaveBackend::Eeprom(e) => e.restore(data),
            SaveBackend::Flash(f) => f.restore(data),
        }
    }

    /// Capture save-backend state for save-state serialization.
    pub fn capture_state(&self) -> SaveBackendState {
        match self {
            SaveBackend::None => SaveBackendState::None,
            SaveBackend::Sram(sram) => SaveBackendState::Sram(sram.capture_state()),
            SaveBackend::Eeprom(eeprom) => SaveBackendState::Eeprom(eeprom.capture_state()),
            SaveBackend::Flash(flash) => SaveBackendState::Flash(flash.capture_state()),
        }
    }

    /// Restore save-backend state from a save-state snapshot.
    pub fn restore_state(&mut self, state: &SaveBackendState) -> Result<(), String> {
        match (self, state) {
            (SaveBackend::None, SaveBackendState::None) => Ok(()),
            (SaveBackend::Sram(sram), SaveBackendState::Sram(sram_state)) => {
                sram.restore_state(sram_state)
            }
            (SaveBackend::Eeprom(eeprom), SaveBackendState::Eeprom(eeprom_state)) => {
                eeprom.restore_state(eeprom_state)
            }
            (SaveBackend::Flash(flash), SaveBackendState::Flash(flash_state)) => {
                flash.restore_state(flash_state)
            }
            (backend, state) => Err(format!(
                "cartridge save-backend type mismatch: live={}, state={}",
                backend.kind_name(),
                state.kind_name()
            )),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            SaveBackend::None => "none",
            SaveBackend::Sram(_) => "sram",
            SaveBackend::Eeprom(_) => "eeprom",
            SaveBackend::Flash(_) => "flash",
        }
    }
}

impl SaveBackendState {
    fn kind_name(&self) -> &'static str {
        match self {
            SaveBackendState::None => "none",
            SaveBackendState::Sram(_) => "sram",
            SaveBackendState::Eeprom(_) => "eeprom",
            SaveBackendState::Flash(_) => "flash",
        }
    }
}

/// A loaded GBA cartridge.
pub struct GbaCartridge {
    rom: Vec<u8>,
    header: GbaHeader,
    save_type: SaveType,
    save: SaveBackend,
}

impl GbaCartridge {
    /// Borrow the ROM bytes (mirrored access is the bus' responsibility).
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// Parsed header.
    pub fn header(&self) -> &GbaHeader {
        &self.header
    }

    /// Auto-detected save type.
    pub fn save_type(&self) -> SaveType {
        self.save_type
    }

    /// Mutable access to the save backend (for bus integration).
    pub fn save_mut(&mut self) -> &mut SaveBackend {
        &mut self.save
    }

    /// Borrow the save backend (for `.sav` flush).
    pub fn save(&self) -> &SaveBackend {
        &self.save
    }

    /// Consume the cartridge and return owned ROM + save backend parts.
    pub fn into_rom_and_save(self) -> (Vec<u8>, SaveBackend) {
        (self.rom, self.save)
    }
}

/// Parse and load a GBA cartridge ROM.
///
/// Validations:
///
/// 1. Length must be at least [`HEADER_SIZE`] (192 bytes) — returns
///    [`CartridgeError::TooShort`].
/// 2. Length must be at most [`ROM_MAX_SIZE`] (32 MB) — returns
///    [`CartridgeError::TooLarge`].
/// 3. Header is parsed (lenient — bad fixed byte / complement check is
///    surfaced via [`GbaHeader::is_valid`] but does not fail the load).
///
/// The save type is auto-detected via [`detect_save_type`] and the
/// matching backend is constructed empty (callers should restore the
/// `.sav` contents afterwards via [`SaveBackend::restore`]).
pub fn load_cartridge(bytes: &[u8]) -> Result<GbaCartridge, CartridgeError> {
    if bytes.len() < HEADER_SIZE {
        return Err(CartridgeError::TooShort);
    }
    if bytes.len() > ROM_MAX_SIZE {
        return Err(CartridgeError::TooLarge {
            actual: bytes.len(),
            max: ROM_MAX_SIZE,
        });
    }
    let header = parse_header(bytes)?;
    let save_type = detect_save_type(bytes);
    let save = SaveBackend::for_save_type(save_type);
    Ok(GbaCartridge {
        rom: bytes.to_vec(),
        header,
        save_type,
        save,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, GAME_CODE_OFFSET,
        MAKER_CODE_OFFSET, TITLE_OFFSET, compute_complement_check,
    };

    /// Build a minimal valid 192-byte ROM with the given save-type marker
    /// embedded just past the header.
    fn build_rom(marker: &[u8], extra_size: usize) -> Vec<u8> {
        let total = (HEADER_SIZE + extra_size).max(HEADER_SIZE + marker.len());
        let mut rom = vec![0u8; total];
        rom[TITLE_OFFSET..TITLE_OFFSET + 4].copy_from_slice(b"TEST");
        rom[GAME_CODE_OFFSET..GAME_CODE_OFFSET + 4].copy_from_slice(b"AGBE");
        rom[MAKER_CODE_OFFSET..MAKER_CODE_OFFSET + 2].copy_from_slice(b"01");
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        // Place marker on a 4-byte boundary right after the header.
        if !marker.is_empty() {
            rom[HEADER_SIZE..HEADER_SIZE + marker.len()].copy_from_slice(marker);
        }
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    #[test]
    fn rejects_rom_smaller_than_header() {
        let rom = vec![0u8; HEADER_SIZE - 1];
        assert!(matches!(
            load_cartridge(&rom),
            Err(CartridgeError::TooShort)
        ));
    }

    #[test]
    fn rejects_rom_larger_than_32mb() {
        let rom = vec![0u8; ROM_MAX_SIZE + 1];
        assert!(matches!(
            load_cartridge(&rom),
            Err(CartridgeError::TooLarge { .. })
        ));
    }

    #[test]
    fn accepts_rom_exactly_32mb() {
        let mut rom = vec![0u8; ROM_MAX_SIZE];
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        let cart = load_cartridge(&rom).unwrap();
        assert_eq!(cart.rom().len(), ROM_MAX_SIZE);
    }

    #[test]
    fn accepts_minimal_192_byte_rom() {
        let rom = build_rom(&[], 0);
        let cart = load_cartridge(&rom).unwrap();
        assert_eq!(cart.header().game_code, "AGBE");
        assert!(cart.header().is_valid());
        assert_eq!(cart.save_type(), SaveType::None);
        assert!(cart.save().snapshot().is_empty());
    }

    #[test]
    fn detects_sram_save_type_and_creates_backend() {
        let rom = build_rom(b"SRAM_Vxxx\0", 256);
        let cart = load_cartridge(&rom).unwrap();
        assert_eq!(cart.save_type(), SaveType::Sram32K);
        assert!(matches!(cart.save(), SaveBackend::Sram(_)));
        assert_eq!(cart.save().snapshot().len(), 32 * 1024);
    }

    #[test]
    fn detects_eeprom_save_type_512b_for_small_rom() {
        let rom = build_rom(b"EEPROM_V124", 1024);
        let cart = load_cartridge(&rom).unwrap();
        assert_eq!(cart.save_type(), SaveType::Eeprom512);
        assert!(matches!(cart.save(), SaveBackend::Eeprom(_)));
        assert_eq!(cart.save().snapshot().len(), 512);
    }

    #[test]
    fn detects_flash_save_type_and_creates_backend() {
        let rom = build_rom(b"FLASH1M_Vxxx", 256);
        let cart = load_cartridge(&rom).unwrap();
        assert_eq!(cart.save_type(), SaveType::Flash128K);
        assert!(matches!(cart.save(), SaveBackend::Flash(_)));
        assert_eq!(cart.save().snapshot().len(), 128 * 1024);
    }

    #[test]
    fn snapshot_restore_round_trips_through_backend() {
        let rom = build_rom(b"SRAM_Vxxx\0", 256);
        let mut cart = load_cartridge(&rom).unwrap();
        // Inject some save data via the SRAM directly.
        if let SaveBackend::Sram(sram) = cart.save_mut() {
            sram.write(0x1000, 0x5A);
        } else {
            panic!("expected SRAM backend");
        }
        let snap = cart.save().snapshot().to_vec();
        // Build a fresh cart, restore from snapshot.
        let mut other = load_cartridge(&rom).unwrap();
        other.save_mut().restore(&snap);
        if let SaveBackend::Sram(sram) = other.save() {
            assert_eq!(sram.read(0x1000), 0x5A);
        } else {
            panic!("expected SRAM backend");
        }
    }

    #[test]
    fn save_backend_state_restores_sram_variant() {
        let mut backend = SaveBackend::Sram(Sram::new());
        if let SaveBackend::Sram(sram) = &mut backend {
            sram.write(0x42, 0xA5);
        }

        let state = backend.capture_state();
        if let SaveBackend::Sram(sram) = &mut backend {
            sram.write(0x42, 0x00);
        }
        backend
            .restore_state(&state)
            .expect("restore backend state");

        assert_eq!(backend.snapshot()[0x42], 0xA5);
    }

    #[test]
    fn save_backend_state_rejects_mismatched_variant_without_mutating() {
        let mut backend = SaveBackend::Sram(Sram::new());
        if let SaveBackend::Sram(sram) = &mut backend {
            sram.write(0x42, 0xA5);
        }

        let result = backend.restore_state(&SaveBackendState::None);

        assert!(result.is_err());
        assert_eq!(backend.snapshot()[0x42], 0xA5);
    }
}
