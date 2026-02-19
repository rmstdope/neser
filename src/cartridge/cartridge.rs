use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::{error, fmt};

use crate::app_context::AppContext;
use crate::cartridge::mapper::MapperContext;
use crate::cartridge::{Mapper, NametableLayout, TimingMode};

#[derive(Debug)]
pub enum CartridgeError {
    InvalidHeader,
    FileTooSmall { expected: usize, actual: usize },
    UnsupportedMapper(u16),
    Io(io::Error),
}

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const DEFAULT_PRG_RAM_BANKS_8K: u8 = 1;
const SAVE_FILE_EXTENSION: &str = "sav";
const STATE_FILE_EXTENSION: &str = "state";

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader => {
                write!(f, "Invalid iNES header: expected 'NES\\x1A' magic bytes")
            }
            Self::FileTooSmall { expected, actual } => {
                write!(
                    f,
                    "File too small: expected at least {expected} bytes, got {actual}"
                )
            }
            Self::UnsupportedMapper(mapper) => write!(f, "Unsupported mapper: {mapper}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl error::Error for CartridgeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for CartridgeError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}
/// Represents an NES cartridge containing PRG ROM and CHR ROM
pub struct Cartridge {
    /// Mapper instance that handles banking and memory access
    mapper: Box<dyn Mapper>,

    crc32: u32,

    rom_timing_mode: TimingMode,

    rom_path: Option<PathBuf>,
    save_path: Option<PathBuf>,
    battery_backed_prg_ram: bool,

    /// Optional 512-byte trainer data loaded from iNES header.
    /// If present, should be loaded into CPU memory at $7000-$71FF.
    trainer: Option<Vec<u8>>,
}

impl Cartridge {
    fn map_parse_error(err: crate::cartridge::ines::RomParseError) -> CartridgeError {
        match err {
            crate::cartridge::ines::RomParseError::InvalidHeader => CartridgeError::InvalidHeader,
            crate::cartridge::ines::RomParseError::FileTooSmall { expected, actual } => {
                CartridgeError::FileTooSmall { expected, actual }
            }
        }
    }

    fn prg_ram_banks_8k(prg_ram_size_bytes: Option<usize>) -> u8 {
        prg_ram_size_bytes
            .map(|size| {
                if size == 0 {
                    return 0;
                }

                size.div_ceil(PRG_RAM_BANK_SIZE).clamp(1, u8::MAX as usize) as u8
            })
            .unwrap_or(DEFAULT_PRG_RAM_BANKS_8K)
    }

    fn create_mapper(
        info: &crate::cartridge::ines::InesHeader,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Result<Box<dyn Mapper>, CartridgeError> {
        let mut context = MapperContext::new(info.mapper, prg_rom, chr_rom, mirroring)
            .with_prg_ram_banks(Self::prg_ram_banks_8k(info.prg_ram_size_bytes))
            .with_battery_backed_prg_ram(info.battery_backed_prg_ram);

        if info.submapper != 0 {
            context = context.with_submapper(info.submapper);
        }

        crate::cartridge::mapper::create_mapper(context).map_err(|err| {
            if err.kind() == io::ErrorKind::Unsupported {
                CartridgeError::UnsupportedMapper(info.mapper)
            } else {
                CartridgeError::Io(err)
            }
        })
    }

    fn resolve_mirroring_mode_with_db_override(
        header_mirroring: NametableLayout,
        db_entry: Option<&crate::cartridge::RomDbEntry>,
    ) -> NametableLayout {
        db_entry
            .and_then(|entry| entry.nametable_layout)
            .unwrap_or(header_mirroring)
    }

    fn configure_paths_from_rom(&mut self, rom_path: PathBuf) {
        self.rom_path = Some(rom_path.clone());
        self.save_path = Some(rom_path.with_extension(SAVE_FILE_EXTENSION));
    }

    fn apply_db_timing_mode_override_from_entry(
        &mut self,
        db_entry: Option<&crate::cartridge::RomDbEntry>,
    ) {
        if let Some(db_timing_mode) = db_entry.and_then(|entry| entry.console_region) {
            self.override_rom_timing_mode(db_timing_mode);
        }
    }

    pub fn override_rom_timing_mode(&mut self, timing_mode: TimingMode) {
        self.rom_timing_mode = timing_mode.normalize_rom_timing_mode();
    }

    fn can_persist_save_ram(&self) -> bool {
        self.battery_backed_prg_ram && self.save_path.is_some()
    }

    fn save_temp_path(save_path: &Path) -> PathBuf {
        let mut temp_path = save_path.to_path_buf();
        temp_path.set_extension(format!("sav.tmp.{}", std::process::id()));
        temp_path
    }

    fn write_save_data(save_path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = save_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = Self::save_temp_path(save_path);
        fs::write(&temp_path, data)?;

        if save_path.exists() {
            let _ = fs::remove_file(save_path);
        }

        fs::rename(temp_path, save_path)
    }

    fn load_save_data(&mut self, save_path: &Path) -> io::Result<()> {
        if !save_path.exists() {
            return Ok(());
        }

        let save_data = fs::read(save_path)?;
        self.mapper_mut().load_wram_snapshot(&save_data);
        Ok(())
    }

    /// Create a new cartridge from an iNES ROM byte stream and its source path.
    pub fn load_from_file<P: AsRef<Path>>(
        data: &[u8],
        path: P,
        app_context: &AppContext,
    ) -> Result<Self, CartridgeError> {
        let rom_path = path.as_ref().to_path_buf();
        let (info, prg_rom, chr_rom, trainer, crc32) =
            crate::cartridge::parse_rom(data).map_err(Self::map_parse_error)?;

        let db_entry = app_context.get_db_entry_by_crc(crc32);
        let mirroring =
            Self::resolve_mirroring_mode_with_db_override(info.mirroring, db_entry.as_ref());

        let mut cart = Self {
            mapper: Self::create_mapper(&info, prg_rom, chr_rom, mirroring)?,
            crc32,
            rom_timing_mode: info.timing_mode.normalize_rom_timing_mode(),
            rom_path: None,
            save_path: None,
            battery_backed_prg_ram: info.battery_backed_prg_ram,
            trainer,
        };

        cart.apply_db_timing_mode_override_from_entry(db_entry.as_ref());
        cart.configure_paths_from_rom(rom_path);
        cart.load_save_ram_from_disk()?;

        Ok(cart)
    }

    pub fn state_path(&self) -> Option<PathBuf> {
        self.rom_path
            .as_ref()
            .map(|path| path.with_extension(STATE_FILE_EXTENSION))
    }

    pub fn debug_path(&self) -> Option<PathBuf> {
        self.rom_path
            .as_ref()
            .map(|path| path.with_extension("debug"))
    }

    pub fn save_ram(&self) -> io::Result<()> {
        if !self.can_persist_save_ram() {
            return Ok(());
        }

        let Some(save_path) = self.save_path.as_ref() else {
            return Ok(());
        };

        // Use mapper's wram_snapshot to capture all WRAM, bypassing enable/protect and banking.
        let prg_ram = self.mapper().wram_snapshot();
        Self::write_save_data(save_path, &prg_ram)
    }

    fn load_save_ram_from_disk(&mut self) -> io::Result<()> {
        if !self.can_persist_save_ram() {
            return Ok(());
        }

        let Some(save_path) = self.save_path.as_ref() else {
            return Ok(());
        };

        let save_path = save_path.clone();
        self.load_save_data(&save_path)
    }

    /// Get a reference to the mapper
    pub fn mapper(&self) -> &dyn Mapper {
        &*self.mapper
    }

    /// Get a mutable reference to the mapper
    pub fn mapper_mut(&mut self) -> &mut dyn Mapper {
        &mut *self.mapper
    }

    /// Reset the cartridge to its power-on state.
    ///
    /// This delegates to the mapper's reset method to reset bank registers,
    /// IRQ counters, and other mapper-specific state. PRG-RAM is typically preserved.
    pub fn reset(&mut self) {
        self.mapper.reset();
    }

    /// Initialize cartridge RAM (PRG-RAM and CHR-RAM) based on the given mode.
    ///
    /// This should be called when the cartridge is inserted or on hard reset.
    /// Soft resets should NOT call this (RAM contents persist).
    pub fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mapper.initialize_ram(mode);
    }

    pub fn crc32(&self) -> u32 {
        self.crc32
    }

    pub fn rom_timing_mode(&self) -> TimingMode {
        self.rom_timing_mode
    }

    /// Returns a reference to the trainer data if present.
    /// The trainer is a 512-byte block that should be loaded into CPU memory at $7000-$71FF.
    pub fn trainer(&self) -> Option<&[u8]> {
        self.trainer.as_deref()
    }

    /// Create a cartridge directly from components (for testing)
    #[cfg(test)]
    pub fn from_parts(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        use crate::cartridge::nrom::NROMMapper;
        let crc32 = crate::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);
        let mapper = Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring));
        Self {
            mapper,
            crc32,
            rom_timing_mode: TimingMode::Ntsc,
            rom_path: None,
            save_path: None,
            battery_backed_prg_ram: false,
            trainer: None,
        }
    }

    #[cfg(test)]
    pub fn from_mapper_for_test(mapper: Box<dyn Mapper>) -> Self {
        Self {
            mapper,
            crc32: 0,
            rom_timing_mode: TimingMode::Ntsc,
            rom_path: None,
            save_path: None,
            battery_backed_prg_ram: false,
            trainer: None,
        }
    }

    #[cfg(test)]
    pub fn set_crc32_for_test(&mut self, crc32: u32) {
        self.crc32 = crc32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INES_HEADER_SIZE: usize = 16;
    const TRAINER_SIZE: usize = 512;
    const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
    const CHR_ROM_BANK_SIZE: usize = 8 * 1024;
    const PRG_FILL_BYTE: u8 = 0xAA;
    const CHR_FILL_BYTE: u8 = 0xBB;

    fn build_ines_header(
        prg_rom_banks: u8,
        chr_rom_banks: u8,
        flags6: u8,
        flags7: u8,
        flags8: u8,
        flags9: u8,
        flags10: u8,
    ) -> Vec<u8> {
        let mut header = vec![
            b'N',
            b'E',
            b'S',
            0x1A,
            prg_rom_banks,
            chr_rom_banks,
            flags6,
            flags7,
            flags8,
            flags9,
            flags10,
        ];
        header.resize(INES_HEADER_SIZE, 0);
        header
    }

    fn append_prg_and_chr_pattern(rom: &mut Vec<u8>, prg_rom_banks: u8, chr_rom_banks: u8) {
        let prg_size = prg_rom_banks as usize * PRG_ROM_BANK_SIZE;
        rom.extend(vec![PRG_FILL_BYTE; prg_size]);

        let chr_size = chr_rom_banks as usize * CHR_ROM_BANK_SIZE;
        rom.extend(vec![CHR_FILL_BYTE; chr_size]);
    }

    fn remove_file_if_exists(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    fn remove_files_if_exist(paths: &[&Path]) {
        for path in paths {
            remove_file_if_exists(path);
        }
    }

    fn create_test_rom(
        prg_rom_banks: u8,
        chr_rom_banks: u8,
        flags6: u8,
        include_trainer: bool,
    ) -> Vec<u8> {
        let mut rom = build_ines_header(prg_rom_banks, chr_rom_banks, flags6, 0, 0, 0, 0);

        // Add trainer if requested
        if include_trainer {
            rom.extend(vec![0x00; TRAINER_SIZE]);
        }

        append_prg_and_chr_pattern(&mut rom, prg_rom_banks, chr_rom_banks);
        rom
    }

    fn create_test_rom_with_flags9(
        prg_rom_banks: u8,
        chr_rom_banks: u8,
        flags6: u8,
        flags7: u8,
        flags9: u8,
        include_trainer: bool,
    ) -> Vec<u8> {
        let mut rom = build_ines_header(prg_rom_banks, chr_rom_banks, flags6, flags7, 0, flags9, 0);

        if include_trainer {
            rom.extend(vec![0x00; TRAINER_SIZE]);
        }

        append_prg_and_chr_pattern(&mut rom, prg_rom_banks, chr_rom_banks);

        rom
    }

    fn create_test_rom_with_mapper(
        prg_rom_banks: u8,
        chr_rom_banks: u8,
        mapper_number: u8,
        battery_backed: bool,
        prg_ram_banks_8k: u8,
    ) -> Vec<u8> {
        // Mapper number is split: lower nibble in flags6 bits 4-7, upper nibble in flags7 bits 4-7
        let mut flags6 = (mapper_number & 0x0F) << 4;
        if battery_backed {
            flags6 |= 0x02; // Bit 1 = battery-backed PRG-RAM
        }
        let flags7 = mapper_number & 0xF0;

        let mut rom = build_ines_header(
            prg_rom_banks,
            chr_rom_banks,
            flags6,
            flags7,
            prg_ram_banks_8k,
            0,
            0,
        );

        append_prg_and_chr_pattern(&mut rom, prg_rom_banks, chr_rom_banks);

        rom
    }

    fn unique_temp_path(filename: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("neser-{nonce}-{filename}"));
        path
    }

    fn load_cartridge_from_disk(path: &Path, app_context: &AppContext) -> Cartridge {
        let data = std::fs::read(path).expect("reading ROM from disk should succeed");
        Cartridge::load_from_file(&data, path, app_context).expect("load_from_file should succeed")
    }

    fn load_cartridge_from_bytes(data: &[u8]) -> Result<Cartridge, CartridgeError> {
        let app_context = AppContext::new();
        Cartridge::load_from_file(data, unique_temp_path("in_memory_test.nes"), &app_context)
    }

    #[test]
    fn test_load_from_file_parses_ines_rom() {
        // iNES Flags 6 bit 0 set => vertical mirroring.
        let rom_data = create_test_rom(1, 1, 0x01, false);
        let path = unique_temp_path("test_load_from_file.nes");
        std::fs::write(&path, &rom_data).expect("writing temp ROM should succeed");

        let app_context = AppContext::new();
        let cart = load_cartridge_from_disk(&path, &app_context);
        assert_eq!(cart.mapper().get_mirroring(), NametableLayout::Vertical);

        remove_file_if_exists(&path);
    }

    #[test]
    fn test_load_from_file_loads_save_ram_from_sav_with_same_basename() {
        // Arrange a ROM on disk plus a matching .sav file.
        // The cartridge should load PRG-RAM from <rom>.sav automatically.
        let rom_data = create_test_rom(1, 1, 0x02, false); // flags6 bit 1 = battery-backed PRG-RAM
        let rom_path = unique_temp_path("save_ram_load_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let sav_path = rom_path.with_extension("sav");
        let mut sav = vec![0u8; 0x2000]; // 8KB PRG-RAM
        sav[0] = 0x42;
        sav[0x1FFF] = 0x99;
        std::fs::write(&sav_path, &sav).expect("writing temp SAV should succeed");

        // Act
        let app_context = AppContext::new();
        let cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Assert: PRG-RAM reads reflect loaded save.
        assert_eq!(cart.mapper().read_prg(0x6000), 0x42);
        assert_eq!(cart.mapper().read_prg(0x7FFF), 0x99);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_save_ram_writes_to_sav_with_same_basename() {
        // Arrange a ROM loaded from disk; the cartridge should know where to save.
        let rom_data = create_test_rom(1, 1, 0x02, false);
        let rom_path = unique_temp_path("save_ram_save_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);
        cart.mapper_mut().write_prg(0x6000, 0xAB);
        cart.mapper_mut().write_prg(0x7FFF, 0xCD);

        // Act
        cart.save_ram().expect("save_ram should succeed");

        // Assert
        let sav_path = rom_path.with_extension("sav");
        let sav = std::fs::read(&sav_path).expect("SAV should be written");
        assert_eq!(sav.len(), 0x2000);
        assert_eq!(sav[0], 0xAB);
        assert_eq!(sav[0x1FFF], 0xCD);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_load_simple_rom() {
        let rom_data = create_test_rom(1, 1, 0, false);

        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        // Verify mapper can read PRG ROM (16KB at $8000-$BFFF)
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        // Verify mapper can read CHR ROM (8KB at $0000-$1FFF)
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
        // Verify no trainer data
        assert!(cartridge.trainer().is_none());
    }

    #[test]
    fn test_load_rom_with_trainer_stores_trainer_data() {
        // Create a ROM with trainer bit set and custom trainer data
        let mut rom = vec![
            b'N', b'E', b'S', 0x1A, // iNES header
            1,    // PRG ROM size (16KB units)
            1,    // CHR ROM size (8KB units)
            0x04, // Flags 6 with trainer bit set (bit 2)
            0,    // Flags 7
            0,    // Flags 8 (PRG RAM size)
            0,    // Flags 9
            0,    // Flags 10
            0, 0, 0, 0, 0, // Reserved (unused)
        ];

        // Add 512 bytes of trainer data with a pattern
        let trainer_pattern: Vec<u8> = (0..512).map(|i| (i + 100) as u8).collect();
        rom.extend(&trainer_pattern);

        // Add PRG ROM data
        rom.extend(vec![0xAA; 16 * 1024]);

        // Add CHR ROM data
        rom.extend(vec![0xBB; 8 * 1024]);

        let cartridge = load_cartridge_from_bytes(&rom).unwrap();

        // Verify trainer data is accessible
        let trainer = cartridge.trainer().expect("Trainer should be present");
        assert_eq!(trainer.len(), 512);

        // Verify trainer data matches what we put in
        for (i, &byte) in trainer.iter().enumerate() {
            assert_eq!(byte, (i + 100) as u8);
        }

        // Verify mapper can still read PRG and CHR correctly
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
    }

    #[test]
    fn test_load_rom_without_trainer_has_none() {
        let rom_data = create_test_rom(1, 1, 0, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(cartridge.trainer().is_none());
    }

    #[test]
    fn test_load_rom_with_trainer() {
        let rom_data = create_test_rom(1, 1, 0x04, true);

        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        // Verify mapper can read PRG ROM after skipping trainer
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        // Verify mapper can read CHR ROM
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
    }

    #[test]
    fn test_load_rom_multiple_banks() {
        let rom_data = create_test_rom(2, 4, 0, false);

        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        // Verify 32KB PRG ROM can be read
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        assert_eq!(cartridge.mapper().read_prg(0xFFFF), 0xAA);
        // Verify CHR ROM can be read (only first 8KB used by NROM)
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
        assert_eq!(cartridge.mapper().read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_rom_timing_mode_defaults_to_ntsc() {
        let rom_data = create_test_rom_with_flags9(1, 1, 0, 0, 0, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert_eq!(cartridge.rom_timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn test_rom_timing_mode_parses_pal_flag() {
        let rom_data = create_test_rom_with_flags9(1, 1, 0, 0, 0x01, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert_eq!(cartridge.rom_timing_mode(), TimingMode::Pal);
    }

    #[test]
    fn test_rom_timing_mode_nes2_timing_mode_zero_is_ntsc() {
        let rom_data = create_test_rom_with_flags9(1, 1, 0, 0x08, 0x01, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert_eq!(cartridge.rom_timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn test_rom_timing_mode_nes2_uses_header_timing_mode() {
        let mut rom_data = create_test_rom_with_flags9(1, 1, 0, 0x08, 0x00, false);
        rom_data[12] = 0x01; // NES2 timing mode PAL
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert_eq!(cartridge.rom_timing_mode(), TimingMode::Pal);
    }

    #[test]
    fn test_db_timing_mode_override_applies_known_crc_entry() {
        let rom_data = create_test_rom_with_flags9(1, 1, 0, 0, 0, false);
        let mut cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert_eq!(cartridge.rom_timing_mode(), TimingMode::Ntsc);

        let rom_db = crate::cartridge::RomDb::new().expect("rom db should load");
        let (crc, expected_timing_mode) = rom_db
            .entries()
            .iter()
            .find_map(|(crc, entry)| {
                entry
                    .console_region
                    .map(|timing_mode| (*crc, timing_mode.normalize_rom_timing_mode()))
            })
            .expect("rom db should contain at least one entry with console region");

        cartridge.set_crc32_for_test(crc);
        let db_entry = rom_db.get_by_crc(crc);
        cartridge.apply_db_timing_mode_override_from_entry(db_entry);

        assert_eq!(cartridge.rom_timing_mode(), expected_timing_mode);
    }

    #[test]
    fn test_db_nametable_layout_overrides_header_mirroring_mode() {
        let db_entry = crate::cartridge::RomDbEntry {
            rom_id: None,
            name: None,
            country: None,
            crc: None,
            console_type: None,
            console_region: None,
            rom_class: None,
            mapper: None,
            submapper: None,
            nametable_layout: Some(NametableLayout::Vertical),
            prg_rom_size: None,
            prg_rom_crc: None,
            prg_nvram_size: None,
            prg_ram_size: None,
            chr_rom_size: None,
            chr_rom_crc: None,
            chr_nvram_size: None,
            chr_ram_size: None,
            battery: None,
            vs_hardware_type: None,
            vs_ppu_type: None,
            expansion_type: None,
        };

        let resolved = Cartridge::resolve_mirroring_mode_with_db_override(
            NametableLayout::Horizontal,
            Some(&db_entry),
        );

        assert_eq!(resolved, NametableLayout::Vertical);
    }

    #[test]
    fn test_timing_mode_to_rom_timing_mode_maps_non_ntsc_pal_to_unknown() {
        let tv = crate::cartridge::TimingMode::MultiRegion.normalize_rom_timing_mode();
        assert!(matches!(tv, TimingMode::Unknown(_)));
    }

    #[test]
    fn test_invalid_header() {
        let mut rom_data = vec![b'X', b'Y', b'Z', 0x1A];
        rom_data.extend(vec![0; 12]); // Rest of header

        let result = load_cartridge_from_bytes(&rom_data);
        assert!(matches!(result, Err(CartridgeError::InvalidHeader)));
    }

    #[test]
    fn test_file_too_small() {
        let rom_data = create_test_rom(2, 1, 0, false);
        let truncated = &rom_data[0..100]; // Truncate to only 100 bytes

        let result = load_cartridge_from_bytes(truncated);
        if let Err(CartridgeError::FileTooSmall { expected, actual }) = result {
            assert_eq!(expected, 40_976);
            assert_eq!(actual, 100);
        } else {
            panic!("Expected FileTooSmall error");
        }
    }

    #[test]
    fn test_empty_data() {
        let result = load_cartridge_from_bytes(&[]);
        if let Err(CartridgeError::FileTooSmall { expected, actual }) = result {
            assert_eq!(expected, 16);
            assert_eq!(actual, 0);
        } else {
            panic!("Expected FileTooSmall error");
        }
    }

    #[test]
    fn test_unsupported_mapper() {
        let rom_data = create_test_rom_with_mapper(1, 1, 0xFF, false, 1);

        let result = load_cartridge_from_bytes(&rom_data);
        assert!(matches!(
            result,
            Err(CartridgeError::UnsupportedMapper(0xFF))
        ));
    }

    #[test]
    fn test_unsupported_nes2_mapper_does_not_truncate() {
        let mut rom_data = vec![
            b'N', b'E', b'S', 0x1A, // iNES header
            1,    // PRG ROM size (16KB units)
            1,    // CHR ROM size (8KB units)
            0x00, // Flags 6 (mapper low nibble = 0)
            0x08, // Flags 7 (NES2.0 identifier)
            0x01, // Flags 8 (mapper MSB nibble = 1 => mapper 0x100)
            0x00, // Flags 9
            0x00, // Flags 10
            0, 0, 0, 0, 0, // Reserved
        ];

        rom_data.extend(vec![0xAA; 16 * 1024]);
        rom_data.extend(vec![0xBB; 8 * 1024]);

        let result = load_cartridge_from_bytes(&rom_data);
        assert!(matches!(
            result,
            Err(CartridgeError::UnsupportedMapper(0x100))
        ));
    }

    #[test]
    fn test_horizontal_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x00, false); // Bit 0 = 0 = Horizontal
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Horizontal
        ));
    }

    #[test]
    fn test_vertical_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x01, false); // Bit 0 = 1 = Vertical
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Vertical
        ));
    }

    #[test]
    fn test_four_screen_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x08, false); // Bit 3 = 1 = Four-screen
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::FourScreen
        ));
    }

    #[test]
    fn test_four_screen_overrides_vertical() {
        let rom_data = create_test_rom(1, 1, 0x09, false); // Bit 3 and Bit 0 set
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        // Four-screen should take precedence
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::FourScreen
        ));
    }

    #[test]
    fn test_mirroring_bit_0_horizontal() {
        // Flags6 = 0b0000_0000: Bit 0 clear = Horizontal
        let rom_data = create_test_rom(1, 1, 0b0000_0000, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Horizontal
        ));
    }

    #[test]
    fn test_mirroring_bit_0_vertical() {
        // Flags6 = 0b0000_0001: Bit 0 set = Vertical
        let rom_data = create_test_rom(1, 1, 0b0000_0001, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Vertical
        ));
    }

    #[test]
    fn test_mirroring_bit_3_four_screen() {
        // Flags6 = 0b0000_1000: Bit 3 set = Four-screen
        let rom_data = create_test_rom(1, 1, 0b0000_1000, false);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::FourScreen
        ));
    }

    #[test]
    fn test_mirroring_with_other_flags_set() {
        // Flags6 = 0b0000_0110: Bit 2 (trainer) and bit 1 set, but bit 0 clear = Horizontal
        // Lower nibble (bits 4-7) is 0, so mapper number is 0
        let rom_data = create_test_rom(1, 1, 0b0000_0110, true);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Horizontal
        ));
    }

    #[test]
    fn test_mirroring_with_trainer_and_vertical() {
        // Flags6 = 0b0000_0101: Bit 2 (trainer) and Bit 0 (vertical) set
        let rom_data = create_test_rom(1, 1, 0b0000_0101, true);
        let cartridge = load_cartridge_from_bytes(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            NametableLayout::Vertical
        ));
    }

    #[test]
    fn test_mmc3_load_save_ram_with_prg_ram_disabled() {
        // Test that loading save RAM works even when MMC3 has PRG-RAM disabled.
        // This validates the snapshot API bypasses mapper enable/protect state.

        // Create an MMC3 ROM (mapper 4) with battery-backed PRG-RAM
        let rom_data = create_test_rom_with_mapper(2, 2, 4, true, 1);

        let rom_path = unique_temp_path("mmc3_disabled_ram_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        // Create a save file with known data
        let sav_path = rom_path.with_extension("sav");
        let mut sav = vec![0u8; 0x2000]; // 8KB PRG-RAM
        sav[0] = 0x42;
        sav[0x1FFF] = 0x99;
        std::fs::write(&sav_path, &sav).expect("writing temp SAV should succeed");

        // Load the cartridge
        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Disable PRG-RAM via MMC3 control register
        cart.mapper_mut().write_prg(0xA001, 0b0000_0000); // bit 7 = 0 disables PRG-RAM

        // Verify that reads through normal path return 0 (disabled)
        assert_eq!(cart.mapper().read_prg(0x6000), 0x00);
        assert_eq!(cart.mapper().read_prg(0x7FFF), 0x00);

        // But snapshot API should still capture the loaded data
        let snapshot = cart.mapper().wram_snapshot();
        assert_eq!(snapshot[0], 0x42);
        assert_eq!(snapshot[0x1FFF], 0x99);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_mmc3_save_ram_with_prg_ram_disabled() {
        // Test that saving RAM works even when MMC3 has PRG-RAM disabled.
        // This validates the snapshot API bypasses mapper enable/protect state.

        // Create an MMC3 ROM (mapper 4) with battery-backed PRG-RAM
        let rom_data = create_test_rom_with_mapper(2, 2, 4, true, 1);

        let rom_path = unique_temp_path("mmc3_disabled_save_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Write some data while PRG-RAM is enabled
        cart.mapper_mut().write_prg(0x6000, 0xAB);
        cart.mapper_mut().write_prg(0x7FFF, 0xCD);

        // Disable PRG-RAM
        cart.mapper_mut().write_prg(0xA001, 0b0000_0000);

        // Verify reads return 0 (disabled)
        assert_eq!(cart.mapper().read_prg(0x6000), 0x00);

        // Save should still capture the underlying data
        cart.save_ram().expect("save_ram should succeed");

        let sav_path = rom_path.with_extension("sav");
        let sav = std::fs::read(&sav_path).expect("SAV should be written");
        assert_eq!(sav.len(), 0x2000);
        assert_eq!(sav[0], 0xAB);
        assert_eq!(sav[0x1FFF], 0xCD);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_mmc3_save_ram_with_write_protect() {
        // Test that saving RAM works even when MMC3 has PRG-RAM write-protected.

        let rom_data = create_test_rom_with_mapper(2, 2, 4, true, 1);

        let rom_path = unique_temp_path("mmc3_write_protect_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Write some data while PRG-RAM is writable
        cart.mapper_mut().write_prg(0x6000, 0xAB);

        // Enable write-protect (bit 7 = enable, bit 6 = write-protect)
        cart.mapper_mut().write_prg(0xA001, 0b1100_0000);

        // Verify writes are ignored (still reads old value)
        cart.mapper_mut().write_prg(0x6000, 0xDD);
        assert_eq!(cart.mapper().read_prg(0x6000), 0xAB);

        // Save should still work and capture the data
        cart.save_ram().expect("save_ram should succeed");

        let sav_path = rom_path.with_extension("sav");
        let sav = std::fs::read(&sav_path).expect("SAV should be written");
        assert_eq!(sav[0], 0xAB);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_mmc5_save_ram_with_banked_wram() {
        // Test that MMC5 with >8KB of WRAM saves all banks correctly.
        // MMC5 can have up to 64KB of banked WRAM.

        // Create an MMC5 ROM (mapper 5) with battery-backed PRG-RAM and 16KB WRAM (2 banks)
        let rom_data = create_test_rom_with_mapper(2, 2, 5, true, 2);

        let rom_path = unique_temp_path("mmc5_banked_wram_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Write to bank 0 at $6000-$7FFF
        cart.mapper_mut().write_prg(0x5113, 0); // Select bank 0
        cart.mapper_mut().write_prg(0x6000, 0xAA);
        cart.mapper_mut().write_prg(0x7FFF, 0xBB);

        // Write to bank 1 at $6000-$7FFF
        cart.mapper_mut().write_prg(0x5113, 1); // Select bank 1
        cart.mapper_mut().write_prg(0x6000, 0xCC);
        cart.mapper_mut().write_prg(0x7FFF, 0xDD);

        // Save should capture both banks
        cart.save_ram().expect("save_ram should succeed");

        let sav_path = rom_path.with_extension("sav");
        let sav = std::fs::read(&sav_path).expect("SAV should be written");

        // Verify we saved 16KB (2 banks)
        assert_eq!(sav.len(), 0x4000);

        // Verify bank 0 data
        assert_eq!(sav[0x0000], 0xAA);
        assert_eq!(sav[0x1FFF], 0xBB);

        // Verify bank 1 data
        assert_eq!(sav[0x2000], 0xCC);
        assert_eq!(sav[0x3FFF], 0xDD);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }

    #[test]
    fn test_mmc5_load_save_ram_with_banked_wram() {
        // Test that MMC5 with >8KB of WRAM loads all banks correctly.

        let rom_data = create_test_rom_with_mapper(2, 2, 5, true, 2);

        let rom_path = unique_temp_path("mmc5_load_banked_wram_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        // Create a 16KB save file with known data in both banks
        let sav_path = rom_path.with_extension("sav");
        let mut sav = vec![0u8; 0x4000]; // 16KB
        sav[0x0000] = 0xAA; // Bank 0 start
        sav[0x1FFF] = 0xBB; // Bank 0 end
        sav[0x2000] = 0xCC; // Bank 1 start
        sav[0x3FFF] = 0xDD; // Bank 1 end
        std::fs::write(&sav_path, &sav).expect("writing temp SAV should succeed");

        let app_context = AppContext::new();
        let mut cart = load_cartridge_from_disk(&rom_path, &app_context);

        // Verify bank 0 data
        cart.mapper_mut().write_prg(0x5113, 0); // Select bank 0
        assert_eq!(cart.mapper().read_prg(0x6000), 0xAA);
        assert_eq!(cart.mapper().read_prg(0x7FFF), 0xBB);

        // Verify bank 1 data
        cart.mapper_mut().write_prg(0x5113, 1); // Select bank 1
        assert_eq!(cart.mapper().read_prg(0x6000), 0xCC);
        assert_eq!(cart.mapper().read_prg(0x7FFF), 0xDD);

        remove_files_if_exist(&[&rom_path, &sav_path]);
    }
}
