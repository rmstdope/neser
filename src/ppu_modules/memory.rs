use crate::cartridge::MirroringMode;

/// Manages PPU memory including VRAM, palette RAM, and CHR ROM
pub struct Memory {
    /// Pattern tables (CHR ROM/RAM) - 8KB
    chr_rom: Vec<u8>,
    /// Nametables - 2KB (4 nametables with mirroring)
    ppu_ram: [u8; 2048],
    /// Palette RAM - 32 bytes
    palette: [u8; 32],
    /// Mirroring mode
    mirroring_mode: MirroringMode,
}

impl Memory {
    /// Create a new Memory instance
    pub fn new() -> Self {
        Self {
            chr_rom: vec![0; 8192],
            ppu_ram: [0; 2048],
            palette: [0; 32],
            mirroring_mode: MirroringMode::Horizontal,
        }
    }

    /// Reset memory to initial state
    pub fn reset(&mut self) {
        self.ppu_ram = [0; 2048];
        self.palette = [0; 32];
    }

    /// Load CHR ROM data
    pub fn load_chr_rom(&mut self, chr_rom: Vec<u8>) {
        self.chr_rom = chr_rom;
    }

    /// Set mirroring mode
    pub fn set_mirroring(&mut self, mirroring: MirroringMode) {
        self.mirroring_mode = mirroring;
    }

    /// Read from CHR ROM at the specified address
    pub fn read_chr(&self, addr: u16) -> u8 {
        self.chr_rom.get(addr as usize).copied().unwrap_or(0)
    }

    /// Read from nametable at the specified address (with mirroring)
    pub fn read_nametable(&self, addr: u16) -> u8 {
        let mirrored = self.mirror_vram_address(addr);
        self.ppu_ram[mirrored as usize]
    }

    /// Write to nametable at the specified address (with mirroring)
    pub fn write_nametable(&mut self, addr: u16, value: u8) {
        let mirrored = self.mirror_vram_address(addr);
        self.ppu_ram[mirrored as usize] = value;
    }

    /// Read from palette at the specified address (with mirroring)
    pub fn read_palette(&self, addr: u16) -> u8 {
        let mirrored = self.mirror_palette_address(addr);
        self.palette[mirrored]
    }

    /// Write to palette at the specified address (with mirroring)
    pub fn write_palette(&mut self, addr: u16, value: u8) {
        let mirrored = self.mirror_palette_address(addr);
        self.palette[mirrored] = value;
    }

    /// Mirror VRAM address based on nametable mirroring mode
    fn mirror_vram_address(&self, addr: u16) -> u16 {
        // Mirror down $3000-$3EFF to the range $2000-$2EFF
        // Map $2000-$2FFF to 0x0000-0x0FFF
        let vram_index = (addr & 0x2FFF) - 0x2000;
        
        if self.mirroring_mode == MirroringMode::Vertical {
            // Vertical mirroring: $2000/$2800 map to first 1KB, $2400/$2C00 map to second 1KB
            vram_index % 0x0800
        } else {
            // Horizontal mirroring: $2000/$2400 map to first 1KB, $2800/$2C00 map to second 1KB
            let table = vram_index / 0x0400;
            let offset = vram_index % 0x0400;
            let mirrored_table = match table {
                0 | 2 => 0, // Tables 0 ($2000) and 2 ($2800) map to physical table 0
                1 | 3 => 1, // Tables 1 ($2400) and 3 ($2C00) map to physical table 1
                _ => unreachable!(),
            };
            mirrored_table * 0x0400 + offset
        }
    }

    /// Mirror palette address
    /// Addresses $3F10, $3F14, $3F18, $3F1C mirror to $3F00, $3F04, $3F08, $3F0C
    fn mirror_palette_address(&self, addr: u16) -> usize {
        let offset = (addr - 0x3F00) as usize % 32;
        // Mirror addresses $10, $14, $18, $1C to $00, $04, $08, $0C
        if offset & 0x13 == 0x10 {
            offset & 0x0F
        } else {
            offset
        }
    }

    /// Get mirroring mode
    pub fn mirroring_mode(&self) -> MirroringMode {
        self.mirroring_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_new() {
        let mem = Memory::new();
        assert_eq!(mem.read_chr(0), 0);
        assert_eq!(mem.read_palette(0x3F00), 0);
    }

    #[test]
    fn test_memory_reset() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F00, 0x42);
        mem.reset();
        assert_eq!(mem.read_palette(0x3F00), 0);
    }

    #[test]
    fn test_load_chr_rom() {
        let mut mem = Memory::new();
        let chr_data = vec![0x42; 8192];
        mem.load_chr_rom(chr_data);
        assert_eq!(mem.read_chr(0), 0x42);
    }

    #[test]
    fn test_nametable_read_write() {
        let mut mem = Memory::new();
        mem.write_nametable(0x2000, 0x42);
        assert_eq!(mem.read_nametable(0x2000), 0x42);
    }

    #[test]
    fn test_palette_read_write() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F00, 0x42);
        assert_eq!(mem.read_palette(0x3F00), 0x42);
    }

    #[test]
    fn test_palette_mirroring_3f10_to_3f00() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F00, 0x42);
        assert_eq!(mem.read_palette(0x3F10), 0x42);
    }

    #[test]
    fn test_palette_mirroring_3f14_to_3f04() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F04, 0x55);
        assert_eq!(mem.read_palette(0x3F14), 0x55);
    }

    #[test]
    fn test_vertical_mirroring() {
        let mut mem = Memory::new();
        mem.set_mirroring(MirroringMode::Vertical);
        
        // Write to nametable 0
        mem.write_nametable(0x2000, 0x11);
        // Nametable 2 should mirror to nametable 0
        assert_eq!(mem.read_nametable(0x2800), 0x11);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut mem = Memory::new();
        mem.set_mirroring(MirroringMode::Horizontal);
        
        // Write to nametable 0
        mem.write_nametable(0x2000, 0x22);
        // Nametable 1 should not mirror to nametable 0
        assert_ne!(mem.read_nametable(0x2400), 0x22);
        
        // But nametable 2 should mirror to nametable 0
        assert_eq!(mem.read_nametable(0x2800), 0x22);
    }

    #[test]
    fn test_mirroring_3000_to_2000() {
        let mut mem = Memory::new();
        mem.write_nametable(0x2000, 0x33);
        // $3000-$3EFF mirrors to $2000-$2EFF
        assert_eq!(mem.read_nametable(0x3000), 0x33);
    }
}
