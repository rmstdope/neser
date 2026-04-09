// Test utilities for PPU tests
#[cfg(test)]
use crate::nes::cartridge::Cartridge;

#[cfg(test)]
pub struct InesRomBuilder {
    prg_rom_size: u8, // In 16KB units
    chr_rom_size: u8, // In 8KB units
    mapper: u8,
    mirroring: u8, // 0 = horizontal, 1 = vertical
    chr_rom_data: Option<Vec<u8>>,
    prg_rom_data: Option<Vec<u8>>,
}

#[cfg(test)]
#[allow(dead_code)] // Builder methods may not all be used yet but are part of the API
impl InesRomBuilder {
    pub fn new() -> Self {
        Self {
            prg_rom_size: 2, // Default: 2 * 16KB = 32KB
            chr_rom_size: 1, // Default: 1 * 8KB
            mapper: 0,       // Default: NROM
            mirroring: 0,    // Default: horizontal
            chr_rom_data: None,
            prg_rom_data: None,
        }
    }

    pub fn prg_rom_size(mut self, size: u8) -> Self {
        self.prg_rom_size = size;
        self
    }

    pub fn chr_rom_size(mut self, size: u8) -> Self {
        self.chr_rom_size = size;
        self
    }

    pub fn mapper(mut self, mapper: u8) -> Self {
        self.mapper = mapper;
        self
    }

    pub fn mirroring(mut self, mirroring: u8) -> Self {
        self.mirroring = mirroring;
        self
    }

    pub fn chr_rom_data(mut self, data: Vec<u8>) -> Self {
        self.chr_rom_data = Some(data);
        self
    }

    pub fn prg_rom_data(mut self, data: Vec<u8>) -> Self {
        self.prg_rom_data = Some(data);
        self
    }

    /// Builds the iNES ROM data as a byte vector.
    ///
    /// Creates a complete iNES format ROM with:
    /// - 16-byte iNES header (magic "NES\x1A" + size/flags)
    /// - PRG ROM data (program code, padded to specified size)
    /// - CHR ROM data (graphics patterns, padded to specified size)
    ///
    /// Returns a Vec<u8> containing the complete ROM that can be loaded by a cartridge.
    pub fn build(self) -> Vec<u8> {
        let mut ines_data = Vec::new();

        // iNES header
        ines_data.extend_from_slice(b"NES\x1A"); // Magic number
        ines_data.push(self.prg_rom_size);
        ines_data.push(self.chr_rom_size);
        ines_data.push((self.mapper << 4) | (self.mirroring & 0x01)); // Flags 6: mapper lower nibble + mirroring mode (bit 0: 0=horizontal, 1=vertical)
        ines_data.push(self.mapper & 0xF0); // Flags 7: mapper upper nibble
        ines_data.extend_from_slice(&[0; 8]); // Padding to complete 16-byte header

        // PRG ROM
        let prg_size = self.prg_rom_size as usize * 0x4000; // 16KB units
        if let Some(prg_data) = self.prg_rom_data {
            // Ensure provided data doesn't exceed declared size to avoid invalid ROM layout
            if prg_data.len() > prg_size {
                panic!(
                    "PRG ROM data ({} bytes) exceeds declared size ({} bytes)",
                    prg_data.len(),
                    prg_size
                );
            }
            ines_data.extend_from_slice(&prg_data);
            // Pad if necessary
            if prg_data.len() < prg_size {
                ines_data.resize(ines_data.len() + (prg_size - prg_data.len()), 0);
            }
        } else {
            ines_data.resize(ines_data.len() + prg_size, 0);
        }

        // CHR ROM
        let chr_size = self.chr_rom_size as usize * 0x2000; // 8KB units
        if let Some(chr_data) = self.chr_rom_data {
            // Ensure provided data doesn't exceed declared size to avoid invalid ROM layout
            if chr_data.len() > chr_size {
                panic!(
                    "CHR ROM data ({} bytes) exceeds declared size ({} bytes)",
                    chr_data.len(),
                    chr_size
                );
            }
            ines_data.extend_from_slice(&chr_data);
            // Pad if necessary
            if chr_data.len() < chr_size {
                ines_data.resize(ines_data.len() + (chr_size - chr_data.len()), 0);
            }
        } else {
            ines_data.resize(ines_data.len() + chr_size, 0);
        }

        ines_data
    }

    pub fn build_cartridge(self) -> Cartridge {
        let rom_data = self.build();
        let app_context = crate::app_context::AppContext::new();
        Cartridge::load_from_file(&rom_data, "ppu-test-rom.nes", &app_context)
            .expect("Failed to create cartridge")
    }
}

#[cfg(test)]
impl Default for InesRomBuilder {
    fn default() -> Self {
        Self::new()
    }
}
