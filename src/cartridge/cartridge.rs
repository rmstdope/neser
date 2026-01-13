use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cartridge::Mapper;

// Mirroring types for nametables
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirroringMode {
    Vertical,
    Horizontal,
    FourScreen,
    SingleScreen,
    SingleScreenLower,
    SingleScreenUpper,
}
/// Represents an NES cartridge containing PRG ROM and CHR ROM
pub struct Cartridge {
    /// Mapper instance that handles banking and memory access
    mapper: Box<dyn Mapper>,

    save_path: Option<PathBuf>,
    battery_backed_prg_ram: bool,
}

impl Cartridge {
    /// Create a new cartridge by parsing iNES v1 file data
    pub fn new(data: &[u8]) -> io::Result<Self> {
        // Validate iNES header (first 4 bytes should be "NES\x1A")
        if data.len() < 16 || &data[0..4] != b"NES\x1A" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid iNES file format",
            ));
        }

        // Parse iNES header
        let prg_rom_size = data[4] as usize * 16384; // 16 KB units
        let chr_rom_size = data[5] as usize * 8192; // 8 KB units
        let flags6 = data[6];
        let flags7 = data[7];
        // iNES v1 header byte 8 encodes PRG-RAM size in 8KB units.
        // A value of 0 commonly means 8KB.
        let prg_ram_banks_8k = data[8].max(1);

        // Battery-backed PRG-RAM present (iNES v1): bit 1 of flags6.
        let battery_backed_prg_ram = (flags6 & 0x02) != 0;

        // Parse mapper number from flags6 and flags7
        // Lower nibble: bits 4-7 of flags6
        // Upper nibble: bits 4-7 of flags7
        let mapper_number = (flags6 >> 4) | (flags7 & 0xF0);

        // Parse mirroring from flags6
        // Bit 0: Mirroring (0 = horizontal, 1 = vertical)
        // Bit 3: Four-screen mode
        let mirroring = if (flags6 & 0x08) != 0 {
            MirroringMode::FourScreen
        } else if (flags6 & 0x01) != 0 {
            MirroringMode::Vertical
        } else {
            MirroringMode::Horizontal
        };

        // Check if trainer is present (bit 2 of flags6)
        let has_trainer = (flags6 & 0x04) != 0;
        let trainer_offset = if has_trainer { 512 } else { 0 };

        // Calculate ROM positions
        let prg_rom_start = 16 + trainer_offset;
        let prg_rom_end = prg_rom_start + prg_rom_size;
        let chr_rom_start = prg_rom_end;
        let chr_rom_end = chr_rom_start + chr_rom_size;

        // Validate buffer size
        if data.len() < chr_rom_end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small for specified ROM sizes",
            ));
        }

        // Extract PRG ROM and CHR ROM
        let prg_rom = data[prg_rom_start..prg_rom_end].to_vec();
        let chr_rom = data[chr_rom_start..chr_rom_end].to_vec();

        // Create mapper instance
        let mapper = crate::cartridge::mapper::create_mapper_with_prg_ram_size(
            mapper_number,
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k,
        )?;

        Ok(Self {
            mapper,
            save_path: None,
            battery_backed_prg_ram,
        })
    }

    /// Create a new cartridge by loading an iNES ROM from disk.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let rom_path = path.as_ref().to_path_buf();
        let data = fs::read(&rom_path)?;
        let mut cart = Self::new(&data)?;

        cart.save_path = Some(rom_path.with_extension("sav"));
        cart.load_save_ram_from_disk()?;

        Ok(cart)
    }

    pub fn save_ram(&self) -> io::Result<()> {
        if !self.battery_backed_prg_ram {
            return Ok(());
        }

        let Some(save_path) = self.save_path.as_ref() else {
            return Ok(());
        };

        if let Some(parent) = save_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut prg_ram = vec![0u8; 0x2000];
        for (i, byte) in prg_ram.iter_mut().enumerate() {
            *byte = self.mapper().read_prg(0x6000 + i as u16);
        }

        let mut tmp_path = save_path.to_path_buf();
        tmp_path.set_extension(format!("sav.tmp.{}", std::process::id()));

        fs::write(&tmp_path, prg_ram)?;

        // Best-effort: on Unix this replaces atomically; on other platforms it may fail if exists.
        // Keep the behavior simple; callers can surface the error.
        if save_path.exists() {
            let _ = fs::remove_file(save_path);
        }
        fs::rename(tmp_path, save_path)?;

        Ok(())
    }

    fn load_save_ram_from_disk(&mut self) -> io::Result<()> {
        if !self.battery_backed_prg_ram {
            return Ok(());
        }

        let Some(save_path) = self.save_path.as_ref() else {
            return Ok(());
        };

        if !save_path.exists() {
            return Ok(());
        }

        let sav = fs::read(save_path)?;
        let to_copy = sav.len().min(0x2000);
        for (i, byte) in sav.iter().take(to_copy).enumerate() {
            self.mapper_mut().write_prg(0x6000 + i as u16, *byte);
        }

        Ok(())
    }

    /// Get a reference to the mapper
    pub fn mapper(&self) -> &dyn Mapper {
        &*self.mapper
    }

    /// Get a mutable reference to the mapper
    pub fn mapper_mut(&mut self) -> &mut dyn Mapper {
        &mut *self.mapper
    }

    /// Create a cartridge directly from components (for testing)
    #[cfg(test)]
    pub fn from_parts(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        use crate::cartridge::nrom::NROMMapper;
        let mapper = Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring));
        Self {
            mapper,
            save_path: None,
            battery_backed_prg_ram: false,
        }
    }

    #[cfg(test)]
    pub fn from_mapper_for_test(mapper: Box<dyn Mapper>) -> Self {
        Self {
            mapper,
            save_path: None,
            battery_backed_prg_ram: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_rom(
        prg_rom_banks: u8,
        chr_rom_banks: u8,
        flags6: u8,
        include_trainer: bool,
    ) -> Vec<u8> {
        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,          // iNES header
            prg_rom_banks, // PRG ROM size (16KB units)
            chr_rom_banks, // CHR ROM size (8KB units)
            flags6,        // Flags 6
            0,             // Flags 7
            0,             // Flags 8 (PRG RAM size)
            0,             // Flags 9
            0,             // Flags 10
            0,
            0,
            0,
            0,
            0, // Reserved (unused)
        ];

        // Add trainer if requested
        if include_trainer {
            rom.extend(vec![0x00; 512]);
        }

        // Add PRG ROM data
        let prg_size = prg_rom_banks as usize * 16384;
        rom.extend(vec![0xAA; prg_size]);

        // Add CHR ROM data
        let chr_size = chr_rom_banks as usize * 8192;
        rom.extend(vec![0xBB; chr_size]);

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

    #[test]
    fn test_load_from_file_parses_ines_rom() {
        // iNES Flags 6 bit 0 set => vertical mirroring.
        let rom_data = create_test_rom(1, 1, 0x01, false);
        let path = unique_temp_path("test_load_from_file.nes");
        std::fs::write(&path, &rom_data).expect("writing temp ROM should succeed");

        let cart = Cartridge::load_from_file(&path).expect("load_from_file should succeed");
        assert_eq!(cart.mapper().get_mirroring(), MirroringMode::Vertical);

        let _ = std::fs::remove_file(&path);
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
        let cart = Cartridge::load_from_file(&rom_path).expect("load_from_file should succeed");

        // Assert: PRG-RAM reads reflect loaded save.
        assert_eq!(cart.mapper().read_prg(0x6000), 0x42);
        assert_eq!(cart.mapper().read_prg(0x7FFF), 0x99);

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&sav_path);
    }

    #[test]
    fn test_save_ram_writes_to_sav_with_same_basename() {
        // Arrange a ROM loaded from disk; the cartridge should know where to save.
        let rom_data = create_test_rom(1, 1, 0x02, false);
        let rom_path = unique_temp_path("save_ram_save_test.nes");
        std::fs::write(&rom_path, &rom_data).expect("writing temp ROM should succeed");

        let mut cart = Cartridge::load_from_file(&rom_path).expect("load_from_file should succeed");
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

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&sav_path);
    }

    #[test]
    fn test_load_simple_rom() {
        let rom_data = create_test_rom(1, 1, 0, false);

        let cartridge = Cartridge::new(&rom_data).unwrap();
        // Verify mapper can read PRG ROM (16KB at $8000-$BFFF)
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        // Verify mapper can read CHR ROM (8KB at $0000-$1FFF)
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
    }

    #[test]
    fn test_load_rom_with_trainer() {
        let rom_data = create_test_rom(1, 1, 0x04, true);

        let cartridge = Cartridge::new(&rom_data).unwrap();
        // Verify mapper can read PRG ROM after skipping trainer
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        // Verify mapper can read CHR ROM
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
    }

    #[test]
    fn test_load_rom_multiple_banks() {
        let rom_data = create_test_rom(2, 4, 0, false);

        let cartridge = Cartridge::new(&rom_data).unwrap();
        // Verify 32KB PRG ROM can be read
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0xAA);
        assert_eq!(cartridge.mapper().read_prg(0xFFFF), 0xAA);
        // Verify CHR ROM can be read (only first 8KB used by NROM)
        assert_eq!(cartridge.mapper().read_chr(0x0000), 0xBB);
        assert_eq!(cartridge.mapper().read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_invalid_header() {
        let mut rom_data = vec![b'X', b'Y', b'Z', 0x1A];
        rom_data.extend(vec![0; 12]); // Rest of header

        let result = Cartridge::new(&rom_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_too_small() {
        let rom_data = create_test_rom(2, 1, 0, false);
        let truncated = &rom_data[0..100]; // Truncate to only 100 bytes

        let result = Cartridge::new(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_data() {
        let result = Cartridge::new(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_horizontal_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x00, false); // Bit 0 = 0 = Horizontal
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Horizontal
        ));
    }

    #[test]
    fn test_vertical_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x01, false); // Bit 0 = 1 = Vertical
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Vertical
        ));
    }

    #[test]
    fn test_four_screen_mirroring() {
        let rom_data = create_test_rom(1, 1, 0x08, false); // Bit 3 = 1 = Four-screen
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::FourScreen
        ));
    }

    #[test]
    fn test_four_screen_overrides_vertical() {
        let rom_data = create_test_rom(1, 1, 0x09, false); // Bit 3 and Bit 0 set
        let cartridge = Cartridge::new(&rom_data).unwrap();
        // Four-screen should take precedence
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::FourScreen
        ));
    }

    #[test]
    fn test_mirroring_bit_0_horizontal() {
        // Flags6 = 0b0000_0000: Bit 0 clear = Horizontal
        let rom_data = create_test_rom(1, 1, 0b0000_0000, false);
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Horizontal
        ));
    }

    #[test]
    fn test_mirroring_bit_0_vertical() {
        // Flags6 = 0b0000_0001: Bit 0 set = Vertical
        let rom_data = create_test_rom(1, 1, 0b0000_0001, false);
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Vertical
        ));
    }

    #[test]
    fn test_mirroring_bit_3_four_screen() {
        // Flags6 = 0b0000_1000: Bit 3 set = Four-screen
        let rom_data = create_test_rom(1, 1, 0b0000_1000, false);
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::FourScreen
        ));
    }

    #[test]
    fn test_mirroring_with_other_flags_set() {
        // Flags6 = 0b0000_0110: Bit 2 (trainer) and bit 1 set, but bit 0 clear = Horizontal
        // Lower nibble (bits 4-7) is 0, so mapper number is 0
        let rom_data = create_test_rom(1, 1, 0b0000_0110, true);
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Horizontal
        ));
    }

    #[test]
    fn test_mirroring_with_trainer_and_vertical() {
        // Flags6 = 0b0000_0101: Bit 2 (trainer) and Bit 0 (vertical) set
        let rom_data = create_test_rom(1, 1, 0b0000_0101, true);
        let cartridge = Cartridge::new(&rom_data).unwrap();
        assert!(matches!(
            cartridge.mapper().get_mirroring(),
            MirroringMode::Vertical
        ));
    }
}
