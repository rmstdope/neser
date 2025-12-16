use crate::cartridge::MirroringMode;

/// Manages PPU memory including VRAM, palette RAM, and CHR ROM
pub struct Memory {
    /// Pattern tables (CHR ROM/RAM) - 8KB
    chr_rom: Vec<u8>,
    /// Nametables - 4KB (supports all four nametables for FourScreen mode)
    ppu_ram: [u8; 4096],
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
            ppu_ram: [0; 4096],
            palette: [0; 32],
            mirroring_mode: MirroringMode::Horizontal,
        }
    }

    /// Reset memory to initial state
    pub fn reset(&mut self) {
        self.ppu_ram = [0; 4096];
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

    /// Write to CHR memory at the specified address
    /// Only works if CHR-RAM is present (not CHR-ROM)
    /// For now, we allow writes - mapper will handle ROM vs RAM distinction later
    pub fn write_chr(&mut self, addr: u16, value: u8) {
        let index = (addr & 0x1FFF) as usize;
        if index < self.chr_rom.len() {
            self.chr_rom[index] = value;
        }
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
    /// Palette RAM only stores 6 bits (0-5), bits 6-7 are ignored
    pub fn write_palette(&mut self, addr: u16, value: u8) {
        let mirrored = self.mirror_palette_address(addr);
        self.palette[mirrored] = value & 0x3F; // Only store bits 5-0
    }

    /// Mirror VRAM address based on nametable mirroring mode
    fn mirror_vram_address(&self, addr: u16) -> u16 {
        // Mirror down $3000-$3EFF to the range $2000-$2EFF
        // Map $2000-$2FFF to 0x0000-0x0FFF
        let vram_index = (addr & 0x2FFF) - 0x2000;

        match self.mirroring_mode {
            MirroringMode::Vertical => {
                // Vertical mirroring: A, A, B, B
                // $2000/$2800 map to first 1KB, $2400/$2C00 map to second 1KB
                // Use modulo 0x0800 to map tables 0,2 together and 1,3 together
                vram_index % 0x0800
            }
            MirroringMode::Horizontal => {
                // Horizontal mirroring: A, A, B, B (left-right mirrored)
                // $2000/$2400 map to A (first 1KB), $2800/$2C00 map to B (second 1KB)
                // Tables 0&1 share first 1KB, tables 2&3 share second 1KB
                let table = vram_index / 0x0400;
                let offset = vram_index % 0x0400;
                let mirrored_table = match table {
                    0 | 1 => 0, // Tables 0 ($2000) and 1 ($2400) map to physical table 0
                    2 | 3 => 1, // Tables 2 ($2800) and 3 ($2C00) map to physical table 1
                    _ => unreachable!(),
                };
                mirrored_table * 0x0400 + offset
            }
            MirroringMode::SingleScreen => {
                // SingleScreen mirroring: all nametables map to first 1KB
                vram_index % 0x0400
            }
            MirroringMode::FourScreen => {
                // FourScreen: no mirroring, direct mapping (needs 4KB VRAM)
                vram_index
            }
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
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        assert_eq!(mem.read_palette(0x3F00), 0x02);
    }

    #[test]
    fn test_palette_mirroring_3f10_to_3f00() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F00, 0x42);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        assert_eq!(mem.read_palette(0x3F10), 0x02);
    }

    #[test]
    fn test_palette_mirroring_3f14_to_3f04() {
        let mut mem = Memory::new();
        mem.write_palette(0x3F04, 0x55);
        // Palette RAM only stores 6 bits (0x55 & 0x3F = 0x15)
        assert_eq!(mem.read_palette(0x3F14), 0x15);
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
        // Nametable 1 should mirror to nametable 0 (horizontal mirroring)
        assert_eq!(mem.read_nametable(0x2400), 0x22);

        // But nametable 2 should not mirror to nametable 0
        assert_ne!(mem.read_nametable(0x2800), 0x22);
    }

    #[test]
    fn test_single_screen_mirroring() {
        let mut memory = Memory::new();
        memory.set_mirroring(MirroringMode::SingleScreen);

        // In SingleScreen mode, all four nametables map to the same 1KB
        // Write to $2000 (nametable 0)
        memory.write_nametable(0x2000, 0xAB);

        // All nametables should read the same value
        assert_eq!(memory.read_nametable(0x2000), 0xAB); // Nametable 0
        assert_eq!(memory.read_nametable(0x2400), 0xAB); // Nametable 1
        assert_eq!(memory.read_nametable(0x2800), 0xAB); // Nametable 2
        assert_eq!(memory.read_nametable(0x2C00), 0xAB); // Nametable 3

        // Write to different nametable, should affect all
        memory.write_nametable(0x2800, 0xCD);
        assert_eq!(memory.read_nametable(0x2000), 0xCD);
        assert_eq!(memory.read_nametable(0x2400), 0xCD);
        assert_eq!(memory.read_nametable(0x2800), 0xCD);
        assert_eq!(memory.read_nametable(0x2C00), 0xCD);
    }

    #[test]
    fn test_four_screen_mirroring() {
        let mut memory = Memory::new();
        memory.set_mirroring(MirroringMode::FourScreen);

        // In FourScreen mode, all four nametables are independent (no mirroring)
        // Each nametable gets its own 1KB of VRAM

        // Write different values to each nametable
        memory.write_nametable(0x2000, 0x11); // Nametable 0
        memory.write_nametable(0x2400, 0x22); // Nametable 1
        memory.write_nametable(0x2800, 0x33); // Nametable 2
        memory.write_nametable(0x2C00, 0x44); // Nametable 3

        // Each nametable should retain its own value (no mirroring)
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x33);
        assert_eq!(memory.read_nametable(0x2C00), 0x44);

        // Verify addresses within each nametable work independently
        memory.write_nametable(0x2100, 0xAA); // Middle of nametable 0
        memory.write_nametable(0x2500, 0xBB); // Middle of nametable 1
        memory.write_nametable(0x2900, 0xCC); // Middle of nametable 2
        memory.write_nametable(0x2D00, 0xDD); // Middle of nametable 3

        assert_eq!(memory.read_nametable(0x2100), 0xAA);
        assert_eq!(memory.read_nametable(0x2500), 0xBB);
        assert_eq!(memory.read_nametable(0x2900), 0xCC);
        assert_eq!(memory.read_nametable(0x2D00), 0xDD);
    }

    #[test]
    fn test_vertical_mirroring_comprehensive() {
        let mut memory = Memory::new();
        memory.set_mirroring(MirroringMode::Vertical);

        // Vertical mirroring: A, A, B, B
        // Nametable 0 ($2000) and 2 ($2800) share memory
        // Nametable 1 ($2400) and 3 ($2C00) share memory

        // Write to nametable 0, should mirror to nametable 2
        memory.write_nametable(0x2000, 0x11);
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2800), 0x11); // Mirror

        // Write to nametable 1, should mirror to nametable 3
        memory.write_nametable(0x2400, 0x22);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2C00), 0x22); // Mirror

        // Verify nametables 0 and 1 are independent
        assert_ne!(memory.read_nametable(0x2000), memory.read_nametable(0x2400));

        // Test with offset addresses
        memory.write_nametable(0x2100, 0x33);
        assert_eq!(memory.read_nametable(0x2900), 0x33); // $2100 mirrors to $2900

        memory.write_nametable(0x2500, 0x44);
        assert_eq!(memory.read_nametable(0x2D00), 0x44); // $2500 mirrors to $2D00
    }

    #[test]
    fn test_horizontal_mirroring_comprehensive() {
        let mut memory = Memory::new();
        memory.set_mirroring(MirroringMode::Horizontal);

        // Horizontal mirroring: A, A, B, B (left-right mirrored)
        // Nametable 0 ($2000) and 1 ($2400) share memory (top row)
        // Nametable 2 ($2800) and 3 ($2C00) share memory (bottom row)

        // Write to nametable 0, should mirror to nametable 1
        memory.write_nametable(0x2000, 0x11);
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x11); // Mirror

        // Write to nametable 2, should mirror to nametable 3
        memory.write_nametable(0x2800, 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x22);
        assert_eq!(memory.read_nametable(0x2C00), 0x22); // Mirror

        // Verify nametables 0 and 2 are independent
        assert_ne!(memory.read_nametable(0x2000), memory.read_nametable(0x2800));

        // Test with offset addresses
        memory.write_nametable(0x2100, 0x33);
        assert_eq!(memory.read_nametable(0x2500), 0x33); // $2100 mirrors to $2500

        memory.write_nametable(0x2900, 0x44);
        assert_eq!(memory.read_nametable(0x2D00), 0x44); // $2900 mirrors to $2D00
    }

    #[test]
    fn test_dynamic_mirroring_mode_changes() {
        let mut memory = Memory::new();

        // Start with Vertical mirroring (A, A, B, B)
        memory.set_mirroring(MirroringMode::Vertical);
        memory.write_nametable(0x2000, 0xAA);
        assert_eq!(memory.read_nametable(0x2800), 0xAA); // $2000 mirrors to $2800 in vertical

        // Switch to Horizontal mirroring (A, A, B, B)
        memory.set_mirroring(MirroringMode::Horizontal);
        memory.write_nametable(0x2000, 0xBB);
        assert_eq!(memory.read_nametable(0x2400), 0xBB); // $2000 mirrors to $2400 in horizontal
        // And $2800 mirrors to $2C00
        memory.write_nametable(0x2800, 0xDD);
        assert_eq!(memory.read_nametable(0x2C00), 0xDD);

        // Switch to SingleScreen (A, A, A, A)
        memory.set_mirroring(MirroringMode::SingleScreen);
        memory.write_nametable(0x2000, 0xCC);
        assert_eq!(memory.read_nametable(0x2400), 0xCC);
        assert_eq!(memory.read_nametable(0x2800), 0xCC);
        assert_eq!(memory.read_nametable(0x2C00), 0xCC);

        // Switch to FourScreen (A, B, C, D)
        memory.set_mirroring(MirroringMode::FourScreen);
        memory.write_nametable(0x2000, 0x11);
        memory.write_nametable(0x2400, 0x22);
        memory.write_nametable(0x2800, 0x33);
        memory.write_nametable(0x2C00, 0x44);
        // Each should be independent
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x33);
        assert_eq!(memory.read_nametable(0x2C00), 0x44);
    }

    #[test]
    fn test_mirroring_3000_to_2000() {
        let mut mem = Memory::new();
        mem.write_nametable(0x2000, 0x33);
        // $3000-$3EFF mirrors to $2000-$2EFF
        assert_eq!(mem.read_nametable(0x3000), 0x33);
    }

    #[test]
    fn test_chr_write() {
        let mut mem = Memory::new();
        // Initially should read 0
        assert_eq!(mem.read_chr(0x0000), 0x00);

        // Write to CHR memory
        mem.write_chr(0x0000, 0xAA);
        mem.write_chr(0x1000, 0xBB);
        mem.write_chr(0x1FFF, 0xCC);

        // Read back the values
        assert_eq!(mem.read_chr(0x0000), 0xAA);
        assert_eq!(mem.read_chr(0x1000), 0xBB);
        assert_eq!(mem.read_chr(0x1FFF), 0xCC);
    }
}
