// Test utilities for PPU tests
#[cfg(test)]
use crate::cartridge::Cartridge;

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

    pub fn build(self) -> Vec<u8> {
        let mut ines_data = Vec::new();

        // iNES header
        ines_data.extend_from_slice(b"NES\x1A"); // Magic number
        ines_data.push(self.prg_rom_size);
        ines_data.push(self.chr_rom_size);
        ines_data.push(self.mapper << 4 | self.mirroring); // Flags 6: mapper lower nibble + mirroring
        ines_data.push(self.mapper & 0xF0); // Flags 7: mapper upper nibble
        ines_data.extend_from_slice(&[0; 8]); // Padding to complete 16-byte header

        // PRG ROM
        let prg_size = self.prg_rom_size as usize * 0x4000; // 16KB units
        if let Some(prg_data) = self.prg_rom_data {
            ines_data.extend_from_slice(&prg_data);
            // Pad if necessary
            if prg_data.len() < prg_size {
                ines_data.extend_from_slice(&vec![0u8; prg_size - prg_data.len()]);
            }
        } else {
            ines_data.extend_from_slice(&vec![0u8; prg_size]);
        }

        // CHR ROM
        let chr_size = self.chr_rom_size as usize * 0x2000; // 8KB units
        if let Some(chr_data) = self.chr_rom_data {
            ines_data.extend_from_slice(&chr_data);
            // Pad if necessary
            if chr_data.len() < chr_size {
                ines_data.extend_from_slice(&vec![0u8; chr_size - chr_data.len()]);
            }
        } else {
            ines_data.extend_from_slice(&vec![0u8; chr_size]);
        }

        ines_data
    }

    pub fn build_cartridge(self) -> Cartridge {
        let rom_data = self.build();
        Cartridge::new(&rom_data).expect("Failed to create cartridge")
    }
}

#[cfg(test)]
impl Default for InesRomBuilder {
    fn default() -> Self {
        Self::new()
    }
}
