use crate::cartridge::MirroringMode;
use crate::nes::TvSystem;

/// PPU Control Register ($2000) bit constants
/// Bit 7: Generate NMI at start of VBlank
/// Bit 6: PPU Master/Slave
/// Bit 5: Sprite size
/// Bit 4: Background pattern table address
/// Bit 3: Sprite pattern table address
/// Bit 2: Address increment per CPU read/write of PPUDATA
/// Bit 1-0: Base nametable address
const GENERATE_NMI: u8 = 0b1000_0000;
//const PPU_MASTER_SLAVE: u8 = 0b0100_0000;
//const SPRITE_SIZE: u8 = 0b0010_0000;
//const BG_PATTERN_TABLE_ADDR: u8 = 0b0001_0000;
//const SPRITE_PATTERN_TABLE_ADDR: u8 = 0b0000_1000;
const VRAM_ADDR_INCREMENT: u8 = 0b0000_0100;
//const BASE_NAMETABLE_ADDR: u8 = 0b0000_0011;

/// PPU Status Register ($2002) bit constants
/// Bit 7: VBlank Started
/// Bit 6: Sprite 0 Hit
/// Bit 5: Sprite Overflow
/// Bit 4-0: Unused
const SPRITE_OVERFLOW: u8 = 0b0010_0000;
const SPRITE_0_HIT: u8 = 0b0100_0000;
const VBLANK_STARTED: u8 = 0b1000_0000;

/// Number of PPU cycles (pixels) per scanline
const PIXELS_PER_SCANLINE: u16 = 341;

// Start of VBlank period
const VBLANK_START: u16 = 241;

/// NES Picture Processing Unit (PPU) emulator
pub struct PPU {
    // Data address register
    data_address: u16,
    // True if the next write to the address register is the high byte
    high_byte_next: bool,
    /// PPU data read buffer
    data_buffer: u8,
    // Control register value
    control_register: u8,
    // Mirroring
    mirroring_mode: MirroringMode,
    /// Pattern tables (CHR ROM/RAM) - 8KB
    chr_rom: Vec<u8>,
    // /// Nametables - 2KB (4 nametables with mirroring)
    ppu_ram: [u8; 2048],
    /// Palette RAM - 32 bytes
    palette: [u8; 32],
    /// OAM (Object Attribute Memory) - 256 bytes for sprite data
    oam_data: [u8; 256],
    /// OAM address register ($2003)
    oam_address: u8,
    /// Total number of PPU ticks since reset
    total_cycles: u64,
    /// TV system (NTSC or PAL)
    tv_system: TvSystem,
    /// Current scanline (0-261 for NTSC, 0-311 for PAL)
    scanline: u16,
    /// Current pixel within scanline (0-340)
    pixel: u16,
    /// VBlank flag (bit 7 of status register)
    vblank_flag: bool,
    /// Sprite 0 Hit flag (bit 6 of status register)
    sprite_0_hit: bool,
    /// Sprite Overflow flag (bit 5 of status register)
    sprite_overflow: bool,
    /// NMI enabled flag
    nmi_enabled: bool,
    // Loopy registers for scrolling
    /// v: Current VRAM address (15 bits)
    v: u16,
    /// t: Temporary VRAM address (15 bits)
    t: u16,
    /// x: Fine X scroll (3 bits)
    x: u8,
    /// w: Write toggle (1 bit) - false=first write, true=second write
    w: bool,
}

impl PPU {
    /// Create a new PPU instance
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            data_address: 0,
            high_byte_next: true,
            control_register: 0,
            mirroring_mode: MirroringMode::Horizontal,
            chr_rom: vec![0; 8192],
            ppu_ram: [0; 2048],
            palette: [0; 32],
            data_buffer: 0,
            oam_data: [0; 256],
            oam_address: 0,
            total_cycles: 0,
            tv_system,
            scanline: 0,
            pixel: 0,
            vblank_flag: false,
            sprite_0_hit: false,
            sprite_overflow: false,
            nmi_enabled: false,
            v: 0,
            t: 0,
            x: 0,
            w: false,
        }
    }

    /// Reset the PPU to its initial state
    pub fn reset(&mut self) {
        self.data_address = 0;
        self.high_byte_next = true;
        self.control_register = 0;
        self.ppu_ram = [0; 2048];
        self.palette = [0; 32];
        self.data_buffer = 0;
        self.oam_data = [0; 256];
        self.oam_address = 0;
        self.total_cycles = 0;
        self.scanline = 0;
        self.pixel = 0;
        self.vblank_flag = false;
        self.sprite_0_hit = false;
        self.sprite_overflow = false;
        self.nmi_enabled = false;
        self.v = 0;
        self.t = 0;
        self.x = 0;
        self.w = false;
    }

    /// Run the PPU for a specified number of ticks
    ///
    /// Updates the scanline and pixel position based on the number of cycles.
    /// Each scanline is 341 PPU cycles.
    /// NTSC: 262 scanlines per frame
    /// PAL: 312 scanlines per frame
    pub fn run_ppu_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;

        let total_position = self.pixel as u64 + cycles;
        self.pixel = (total_position % PIXELS_PER_SCANLINE as u64) as u16;

        let scanlines_advanced = total_position / PIXELS_PER_SCANLINE as u64;
        let scanlines_per_frame = self.tv_system.scanlines_per_frame() as u64;
        let old_scanline = self.scanline;
        self.scanline = ((self.scanline as u64 + scanlines_advanced) % scanlines_per_frame) as u16;

        // Check if we crossed into VBlank (scanline 241)
        if old_scanline < VBLANK_START && self.scanline >= VBLANK_START {
            self.vblank_flag = true;
            if self.should_generate_nmi() {
                self.nmi_enabled = true;
            }
        }

        // Clear VBlank flag when we wrap around to scanline 0
        if self.scanline < old_scanline {
            self.vblank_flag = false;
            self.nmi_enabled = false;
        }
    }

    /// Get the total number of ticks since reset
    #[cfg(test)]
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Get the current scanline
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Get the current pixel within the scanline
    #[cfg(test)]
    pub fn pixel(&self) -> u16 {
        self.pixel
    }

    /// Get the v register (current VRAM address)
    #[cfg(test)]
    pub fn v_register(&self) -> u16 {
        self.v
    }

    /// Get the t register (temporary VRAM address)
    #[cfg(test)]
    pub fn t_register(&self) -> u16 {
        self.t
    }

    /// Get the x register (fine X scroll)
    #[cfg(test)]
    pub fn x_register(&self) -> u8 {
        self.x
    }

    /// Get the w register (write toggle)
    #[cfg(test)]
    pub fn w_register(&self) -> bool {
        self.w
    }

    /// Write to the PPU scroll register ($2005)
    /// First write sets coarse X and fine X, second write sets coarse Y and fine Y
    pub fn write_scroll(&mut self, value: u8) {
        if !self.w {
            // First write: set fine X and coarse X in t register
            self.x = value & 0x07;
            self.t = (self.t & 0xFFE0) | ((value as u16) >> 3);
        } else {
            // Second write: set fine Y and coarse Y in t register
            // Fine Y goes in bits 12-14, coarse Y goes in bits 5-9
            self.t = (self.t & 0x8C1F) | (((value as u16) & 0x07) << 12) | (((value as u16) >> 3) << 5);
        }
        self.w = !self.w;
    }

    /// Write to the PPU address register ($2006)
    /// First write sets high byte, second write sets low byte, then alternates
    /// High byte writes are masked with 0x3F to limit address range
    pub fn write_address(&mut self, value: u8) {
        if self.high_byte_next {
            self.data_address = (self.data_address & 0x00FF) | (((value & 0x3F) as u16) << 8);
        } else {
            self.data_address = (self.data_address & 0xFF00) | (value as u16);
        }
        self.high_byte_next = !self.high_byte_next;
    }

    /// Increment the address by the given amount, wrapping at 0x3FFF
    fn inc_address(&mut self, amount: u16) {
        self.data_address = (self.data_address.wrapping_add(amount)) & 0x3FFF;
    }

    /// Write to the PPU control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        let old_value: u8 = self.control_register;
        self.control_register = value;
        if self.control_register & GENERATE_NMI != 0
            && old_value & GENERATE_NMI == 0
            && self.vblank_flag
        {
            // NMI enabled now
            self.nmi_enabled = true;
        }
    }

    /// Write to the OAM address register ($2003)
    pub fn write_oam_address(&mut self, value: u8) {
        self.oam_address = value;
    }

    /// Write to the OAM data register ($2004)
    /// Writes a byte to OAM at the current OAM address and increments the address
    pub fn write_oam_data(&mut self, value: u8) {
        self.oam_data[self.oam_address as usize] = value;
        self.oam_address = self.oam_address.wrapping_add(1);
    }

    /// Read from OAM data register ($2004)
    /// Reads from OAM at the current OAM address (does not increment)
    pub fn read_oam_data(&self) -> u8 {
        self.oam_data[self.oam_address as usize]
    }

    /// Get the VRAM address increment amount based on control register
    fn vram_increment(&self) -> u8 {
        if self.control_register & VRAM_ADDR_INCREMENT != 0 {
            32
        } else {
            1
        }
    }

    /// Should we generate NMI?
    pub fn should_generate_nmi(&self) -> bool {
        self.control_register & GENERATE_NMI != 0
    }

    /// Load CHR ROM data into the PPU
    pub fn load_chr_rom(&mut self, chr_rom: Vec<u8>) {
        self.chr_rom = chr_rom;
    }

    /// Set the mirroring mode
    pub fn set_mirroring(&mut self, mirroring: MirroringMode) {
        self.mirroring_mode = mirroring;
    }

    /// Read from PPU status register ($2002)
    /// Reading this register clears the VBlank flag and resets the address latch (w register)
    pub fn get_status(&mut self) -> u8 {
        let mut status = 0u8;

        if self.vblank_flag {
            status |= VBLANK_STARTED;
        }
        if self.sprite_0_hit {
            status |= SPRITE_0_HIT;
        }
        if self.sprite_overflow {
            status |= SPRITE_OVERFLOW;
        }

        // Reading status clears VBlank flag
        self.vblank_flag = false;
        // Reading status also resets the address latch (both old and new w register)
        self.high_byte_next = true;
        self.w = false;

        status
    }

    /// Read from PPU data register ($2007)
    /// Reads from PPU memory at the current address and increments the address
    /// Returns the value from the previous read (buffered) for non-palette reads
    /// Palette reads return immediately but still update the buffer
    pub fn read_data(&mut self) -> u8 {
        let addr = self.data_address;
        let result = match addr {
            0x0000..=0x1FFF => {
                // CHR ROM/RAM: buffered read
                // Return the previous buffered value
                let buffered = self.data_buffer;
                // Update buffer with current read
                self.data_buffer = self.chr_rom[addr as usize];
                buffered
            }
            0x2000..=0x3EFF => {
                // Return the previous buffered value
                let buffered = self.data_buffer;
                // Update buffer with value
                self.data_buffer = self.ppu_ram[self.mirror_vram_address(addr) as usize];
                buffered
            }
            0x3F00..=0x3FFF => {
                // Palette reads return immediately (no buffering)
                // but still update the buffer with mirrored nametable data
                let data = self.palette[(addr - 0x3F00) as usize % 32];
                // Update buffer (would be from mirrored nametable, but not implemented yet)
                // For now, just return the palette data
                data
            }
            _ => panic!("PPU address out of range: {:04X}", addr),
        };

        self.inc_address(self.vram_increment() as u16);
        result
    }

    /// Write to PPU data register ($2007)
    /// Writes a byte to PPU memory at the current address and increments the address
    pub fn write_data(&mut self, value: u8) {
        let addr = self.data_address;
        match addr {
            0x0000..=0x1FFF => {
                // CHR ROM is read-only
                panic!("Cannot write to CHR ROM at address: {:04X}", addr);
            }
            0x2000..=0x3EFF => {
                // Nametable RAM
                self.ppu_ram[self.mirror_vram_address(addr) as usize] = value;
            }
            0x3F00..=0x3FFF => {
                // Palette RAM
                self.palette[(addr - 0x3F00) as usize % 32] = value;
            }
            _ => panic!("PPU address out of range: {:04X}", addr),
        }

        self.inc_address(self.vram_increment() as u16);
    }

    /// Mirror the VRAM address based on nametable mirroring
    fn mirror_vram_address(&self, addr: u16) -> u16 {
        // Mirror down $3000-$3EFF to the range $2000-$2EFF
        // Map $2000-$2FFF to 0x0000-0x0FFF
        let vram_index = (addr & 0x2FFF) - 0x2000;
        // There are 4 nametables of 1KB each, but only 2KB of VRAM
        // Simple vertical mirroring: $2000/$2400 map to first 1KB, $2800/$2C00 map to second 1KB
        if self.mirroring_mode == MirroringMode::Vertical {
            vram_index % 0x0800
        } else {
            // Horizontal mirroring
            let table = vram_index / 0x0400;
            let offset = vram_index % 0x0400;
            let mirrored_table = match table {
                0 | 1 => 0,
                2 | 3 => 1,
                _ => unreachable!(),
            };
            mirrored_table * 0x0400 + offset
        }
    }

    /// Poll NMI
    pub fn poll_nmi(&mut self) -> bool {
        let ret = self.nmi_enabled;
        self.nmi_enabled = false;
        ret
    }

    /// Check if currently in vblank period
    pub fn is_in_vblank(&self) -> bool {
        self.vblank_flag
    }

    /// Renders the first nametable from PPU VRAM to the screen buffer.
    ///
    /// # Arguments
    ///
    /// * `screen_buffer` - Mutable reference to the screen buffer to render to
    pub fn render(&self, screen_buffer: &mut crate::screen_buffer::ScreenBuffer) {
        // NES nametable is 32x30 tiles, each tile is 8x8 pixels
        // First nametable starts at 0x2000 in PPU address space
        // Each tile is referenced by a byte that indexes into the pattern table

        // Render 32x30 tiles (256x240 pixels)
        for tile_y in 0..30 {
            for tile_x in 0..32 {
                // Get the tile index from nametable
                let nametable_index = tile_y * 32 + tile_x;
                let tile_index = self.ppu_ram[nametable_index] as usize;

                // Each tile in CHR ROM is 16 bytes (8 bytes for low bit plane, 8 bytes for high bit plane)
                let tile_addr = tile_index * 16;

                // Render the 8x8 tile
                for pixel_y in 0..8 {
                    let low_byte = self.chr_rom[tile_addr + pixel_y];
                    let high_byte = self.chr_rom[tile_addr + pixel_y + 8];

                    for pixel_x in 0..8 {
                        // Get the 2-bit color value for this pixel
                        let bit = 7 - pixel_x;
                        let low_bit = (low_byte >> bit) & 1;
                        let high_bit = (high_byte >> bit) & 1;
                        let color_index = (high_bit << 1) | low_bit;

                        // For now, use the first background palette (indices 0-3)
                        let palette_index = self.palette[color_index as usize];

                        // Convert to RGB using system palette
                        let (r, g, b) = crate::nes::Nes::lookup_system_palette(palette_index);

                        // Calculate screen position
                        let screen_x = tile_x * 8 + pixel_x;
                        let screen_y = tile_y * 8 + pixel_y;

                        screen_buffer.set_pixel(screen_x as u32, screen_y as u32, r, g, b);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppu_reset() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.reset();
    }

    #[test]
    fn test_read_data_from_palette_at_3f00() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[0] = 0x42;
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_read_data_from_palette_at_3f1f() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[31] = 0x88;
        ppu.write_address(0x3F);
        ppu.write_address(0x1F);
        assert_eq!(ppu.read_data(), 0x88);
    }

    #[test]
    fn test_read_data_from_palette_mirrors_at_3f20() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[0] = 0xAB;
        ppu.write_address(0x3F);
        ppu.write_address(0x20);
        assert_eq!(ppu.read_data(), 0xAB);
    }

    #[test]
    fn test_read_data_from_palette_at_3fff() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[31] = 0xCD;
        ppu.write_address(0x3F);
        ppu.write_address(0xFF);
        assert_eq!(ppu.read_data(), 0xCD);
    }

    #[test]
    fn test_read_data_increments_address_by_1_by_default() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[0] = 0x10;
        ppu.palette[1] = 0x20;
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x10);
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_read_data_increments_address_by_32_when_control_bit_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[0] = 0x10;
        ppu.palette[0] = 0x20; // Address 0x3F00, wraps to palette[0]
        ppu.write_control(0b0000_0100); // Set VRAM_ADDR_INCREMENT bit
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x20);
        // After increment by 32: 0x3F00 + 32 = 0x3F20, wraps to palette[0]
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_read_data_wraps_address_at_3fff() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[31] = 0xAA;
        ppu.palette[0] = 0xBB;
        ppu.write_address(0x3F);
        ppu.write_address(0xFF);
        assert_eq!(ppu.read_data(), 0xAA); // Read from 0x3FFF (palette[31])
        // After increment by 1: 0x3FFF + 1 = 0x0000, wraps around
        // But 0x0000 triggers todo!() for CHR ROM
    }

    #[test]
    fn test_read_data_palette_reads_not_buffered() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.palette[0] = 0x11;
        ppu.palette[1] = 0x22;
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        // Palette reads return immediately (not buffered)
        assert_eq!(ppu.read_data(), 0x11);
        assert_eq!(ppu.read_data(), 0x22);
    }

    #[test]
    fn test_read_data_chr_rom_first_read_returns_buffer() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.chr_rom[0x0100] = 0xAB;
        ppu.write_address(0x01);
        ppu.write_address(0x00);
        // First read from CHR ROM returns the initial buffer value (0)
        assert_eq!(ppu.read_data(), 0x00);
    }

    #[test]
    fn test_read_data_chr_rom_second_read_returns_first_value() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.chr_rom[0x0100] = 0xAB;
        ppu.chr_rom[0x0101] = 0xCD;
        ppu.write_address(0x01);
        ppu.write_address(0x00);
        // First read returns buffer (0), loads 0xAB into buffer
        assert_eq!(ppu.read_data(), 0x00);
        // Second read returns buffered 0xAB, loads 0xCD into buffer
        assert_eq!(ppu.read_data(), 0xAB);
    }

    #[test]
    fn test_read_data_chr_rom_buffered_read_sequence() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.chr_rom[0x0000] = 0x11;
        ppu.chr_rom[0x0001] = 0x22;
        ppu.chr_rom[0x0002] = 0x33;
        ppu.write_address(0x00);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // Returns buffer, loads 0x11
        assert_eq!(ppu.read_data(), 0x11); // Returns 0x11, loads 0x22
        assert_eq!(ppu.read_data(), 0x22); // Returns 0x22, loads 0x33
        assert_eq!(ppu.read_data(), 0x33); // Returns 0x33, loads next
    }

    #[test]
    fn test_read_data_chr_rom_with_increment_32() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.chr_rom[0x0000] = 0xAA;
        ppu.chr_rom[0x0020] = 0xBB;
        ppu.write_control(0b0000_0100); // Set VRAM_ADDR_INCREMENT bit (increment by 32)
        ppu.write_address(0x00);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // Returns buffer, loads 0xAA
        assert_eq!(ppu.read_data(), 0xAA); // Returns 0xAA, loads 0xBB (addr incremented by 32)
    }

    // Tests migrated from ppu_address
    #[test]
    fn test_ppu_address_write_first_byte_sets_high_byte() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x12);
        assert_eq!(ppu.data_address, 0x1200);
    }

    #[test]
    fn test_ppu_address_write_second_byte_sets_low_byte() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x12);
        ppu.write_address(0x34);
        assert_eq!(ppu.data_address, 0x1234);
    }

    #[test]
    fn test_ppu_address_write_third_byte_sets_high_byte_again() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x12);
        ppu.write_address(0x34);
        ppu.write_address(0x56);
        // 0x56 & 0x3F = 0x16
        assert_eq!(ppu.data_address, 0x1634);
    }

    #[test]
    fn test_ppu_address_write_fourth_byte_sets_low_byte_again() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x12);
        ppu.write_address(0x34);
        ppu.write_address(0x56);
        ppu.write_address(0x78);
        // 0x56 & 0x3F = 0x16
        assert_eq!(ppu.data_address, 0x1678);
    }

    #[test]
    fn test_ppu_address_write_high_byte_masked_with_3f() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0xFF);
        assert_eq!(ppu.data_address, 0x3F00);
    }

    #[test]
    fn test_ppu_address_write_high_byte_masked_third_write() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x12);
        ppu.write_address(0x34);
        ppu.write_address(0xFF);
        assert_eq!(ppu.data_address, 0x3F34);
    }

    #[test]
    fn test_ppu_address_inc_increments_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.data_address = 0x2000;
        ppu.inc_address(1);
        assert_eq!(ppu.data_address, 0x2001);
    }

    #[test]
    fn test_ppu_address_inc_increments_by_multiple() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.data_address = 0x2000;
        ppu.inc_address(32);
        assert_eq!(ppu.data_address, 0x2020);
    }

    #[test]
    fn test_ppu_address_inc_wraps_at_3fff() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.data_address = 0x3FFF;
        ppu.inc_address(1);
        assert_eq!(ppu.data_address, 0x0000);
    }

    #[test]
    fn test_ppu_address_inc_wraps_beyond_3fff() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.data_address = 0x3FFE;
        ppu.inc_address(5);
        assert_eq!(ppu.data_address, 0x0003);
    }

    // Tests migrated from ppu_control
    #[test]
    fn test_ppu_control_new() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.control_register, 0);
    }

    #[test]
    fn test_ppu_control_write() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b1010_1010);
        assert_eq!(ppu.control_register, 0b1010_1010);
    }

    #[test]
    fn test_ppu_control_write_multiple_times() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0xFF);
        assert_eq!(ppu.control_register, 0xFF);
        ppu.write_control(0x00);
        assert_eq!(ppu.control_register, 0x00);
    }

    #[test]
    fn test_ppu_control_vram_increment_returns_1_when_bit_not_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b0000_0000);
        assert_eq!(ppu.vram_increment(), 1);
    }

    #[test]
    fn test_ppu_control_vram_increment_returns_32_when_bit_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b0000_0100);
        assert_eq!(ppu.vram_increment(), 32);
    }

    #[test]
    fn test_ppu_control_vram_increment_ignores_other_bits() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b1111_1011); // All bits set except VRAM_ADDR_INCREMENT
        assert_eq!(ppu.vram_increment(), 1);
        ppu.write_control(0b0000_0100); // Only VRAM_ADDR_INCREMENT set
        assert_eq!(ppu.vram_increment(), 32);
    }

    // OAM tests
    #[test]
    fn test_oam_address_initialized_to_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.oam_address, 0);
    }

    #[test]
    fn test_oam_data_initialized_to_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        for i in 0..256 {
            assert_eq!(ppu.oam_data[i], 0);
        }
    }

    #[test]
    fn test_write_oam_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x42);
        assert_eq!(ppu.oam_address, 0x42);
    }

    #[test]
    fn test_write_oam_address_multiple_times() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x10);
        assert_eq!(ppu.oam_address, 0x10);
        ppu.write_oam_address(0xFF);
        assert_eq!(ppu.oam_address, 0xFF);
    }

    #[test]
    fn test_write_oam_data_writes_to_current_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0xAB);
        assert_eq!(ppu.oam_data[0x00], 0xAB);
    }

    #[test]
    fn test_write_oam_data_increments_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0xAB);
        assert_eq!(ppu.oam_address, 0x01);
    }

    #[test]
    fn test_write_oam_data_sequence() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x11);
        ppu.write_oam_data(0x22);
        ppu.write_oam_data(0x33);
        assert_eq!(ppu.oam_data[0x00], 0x11);
        assert_eq!(ppu.oam_data[0x01], 0x22);
        assert_eq!(ppu.oam_data[0x02], 0x33);
        assert_eq!(ppu.oam_address, 0x03);
    }

    #[test]
    fn test_write_oam_data_wraps_at_256() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0xFF);
        ppu.write_oam_data(0xAA);
        assert_eq!(ppu.oam_data[0xFF], 0xAA);
        assert_eq!(ppu.oam_address, 0x00);
        ppu.write_oam_data(0xBB);
        assert_eq!(ppu.oam_data[0x00], 0xBB);
    }

    #[test]
    fn test_get_oam_data_returns_value_at_current_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.oam_data[0x42] = 0xCD;
        ppu.write_oam_address(0x42);
        assert_eq!(ppu.read_oam_data(), 0xCD);
    }

    #[test]
    fn test_get_oam_data_does_not_increment_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.oam_data[0x10] = 0x88;
        ppu.write_oam_address(0x10);
        assert_eq!(ppu.read_oam_data(), 0x88);
        assert_eq!(ppu.oam_address, 0x10);
        assert_eq!(ppu.read_oam_data(), 0x88);
        assert_eq!(ppu.oam_address, 0x10);
    }

    #[test]
    fn test_oam_reset_clears_data() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0xFF);
        ppu.write_oam_data(0xEE);
        ppu.reset();
        assert_eq!(ppu.oam_data[0x00], 0);
        assert_eq!(ppu.oam_data[0x01], 0);
        assert_eq!(ppu.oam_address, 0);
    }

    #[test]
    fn test_write_data_increments_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x11);
        assert_eq!(ppu.data_address, 0x2001);
    }

    #[test]
    fn test_write_data_increments_by_32_when_control_bit_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b0000_0100); // Set VRAM_ADDR_INCREMENT bit
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x11);
        assert_eq!(ppu.data_address, 0x2020);
    }

    #[test]
    fn test_write_data_to_nametable() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x55);
        assert_eq!(ppu.ppu_ram[0x0000], 0x55);
    }

    #[test]
    fn test_write_data_to_nametable_with_mirroring() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Vertical);
        ppu.write_address(0x24);
        ppu.write_address(0x00);
        ppu.write_data(0x66);
        // $2400 with vertical mirroring mirrors to $0400 in the 2KB VRAM
        assert_eq!(ppu.ppu_ram[0x0400], 0x66);
    }

    #[test]
    fn test_write_data_to_palette() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        ppu.write_data(0x1A);
        assert_eq!(ppu.palette[0], 0x1A);
    }

    #[test]
    fn test_write_data_to_palette_with_mirroring() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0x20);
        ppu.write_data(0x2B);
        assert_eq!(ppu.palette[0], 0x2B);
    }

    #[test]
    fn test_write_data_wraps_address() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0xFF);
        ppu.write_data(0x99);
        assert_eq!(ppu.palette[31], 0x99);
        // Address should wrap from 0x3FFF to 0x0000
        assert_eq!(ppu.data_address, 0x0000);
    }

    #[test]
    fn test_write_then_read_data_nametable() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_address(0x20);
        ppu.write_address(0x50);
        ppu.write_data(0xEE);
        ppu.write_address(0x20);
        ppu.write_address(0x50);
        // First read returns buffer (0)
        assert_eq!(ppu.read_data(), 0x00);
        // Second read returns the written value
        assert_eq!(ppu.read_data(), 0xEE);
    }

    // Mirroring mode tests
    #[test]
    fn test_vertical_mirroring_nametable_0_and_2_same() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Vertical);
        // Write to nametable 0 ($2000)
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0xAA);
        // Read from nametable 2 ($2800) - should be the same
        ppu.write_address(0x28);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xAA); // actual value
    }

    #[test]
    fn test_vertical_mirroring_nametable_1_and_3_same() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Vertical);
        // Write to nametable 1 ($2400)
        ppu.write_address(0x24);
        ppu.write_address(0x00);
        ppu.write_data(0xBB);
        // Read from nametable 3 ($2C00) - should be the same
        ppu.write_address(0x2C);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xBB); // actual value
    }

    #[test]
    fn test_vertical_mirroring_nametable_0_and_1_different() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Vertical);
        // Write to nametable 0 ($2000) at offset 0x10
        ppu.write_address(0x20);
        ppu.write_address(0x10);
        ppu.write_data(0xAA);
        // Write to nametable 1 ($2400) at offset 0x10
        ppu.write_address(0x24);
        ppu.write_address(0x10);
        ppu.write_data(0xBB);
        // Read nametable 0 - should get 0xAA
        ppu.write_address(0x20);
        ppu.write_address(0x10);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xAA);
        // Read nametable 1 - should get 0xBB
        ppu.write_address(0x24);
        ppu.write_address(0x10);
        assert_eq!(ppu.read_data(), 0x00); // buffer (from reading ram[0x0011] which is uninitialized)
        assert_eq!(ppu.read_data(), 0xBB);
    }

    #[test]
    fn test_horizontal_mirroring_nametable_0_and_1_same() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Horizontal);
        // Write to nametable 0 ($2000)
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0xCC);
        // Read from nametable 1 ($2400) - should be the same
        ppu.write_address(0x24);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xCC); // actual value
    }

    #[test]
    fn test_horizontal_mirroring_nametable_2_and_3_same() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Horizontal);
        // Write to nametable 2 ($2800)
        ppu.write_address(0x28);
        ppu.write_address(0x00);
        ppu.write_data(0xDD);
        // Read from nametable 3 ($2C00) - should be the same
        ppu.write_address(0x2C);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xDD); // actual value
    }

    #[test]
    fn test_horizontal_mirroring_nametable_0_and_2_different() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Horizontal);
        // Write to nametable 0 ($2000) at offset 0x10
        ppu.write_address(0x20);
        ppu.write_address(0x10);
        ppu.write_data(0xCC);
        // Write to nametable 2 ($2800) at offset 0x10
        ppu.write_address(0x28);
        ppu.write_address(0x10);
        ppu.write_data(0xDD);
        // Read nametable 0 - should get 0xCC
        ppu.write_address(0x20);
        ppu.write_address(0x10);
        assert_eq!(ppu.read_data(), 0x00); // buffer
        assert_eq!(ppu.read_data(), 0xCC);
        // Read nametable 2 - should get 0xDD
        ppu.write_address(0x28);
        ppu.write_address(0x10);
        assert_eq!(ppu.read_data(), 0x00); // buffer (from reading ram[0x0011] which is uninitialized)
        assert_eq!(ppu.read_data(), 0xDD);
    }

    #[test]
    fn test_vertical_mirroring_read_write_roundtrip() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Vertical);
        // Write to all 4 nametables
        let test_values = [0x11, 0x22, 0x33, 0x44];
        let addresses = [0x2000, 0x2400, 0x2800, 0x2C00];

        for (i, &addr) in addresses.iter().enumerate() {
            ppu.write_address((addr >> 8) as u8);
            ppu.write_address((addr & 0xFF) as u8);
            ppu.write_data(test_values[i]);
        }

        // With vertical mirroring: 0==2, 1==3
        // So we should have: ram[0]=0x33 (from NT2), ram[0x400]=0x44 (from NT3)
        // because NT2 and NT3 overwrite NT0 and NT1
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.read_data(); // skip buffer
        assert_eq!(ppu.read_data(), 0x33); // NT0 mirrors NT2

        ppu.write_address(0x24);
        ppu.write_address(0x00);
        ppu.read_data(); // skip buffer
        assert_eq!(ppu.read_data(), 0x44); // NT1 mirrors NT3
    }

    #[test]
    fn test_horizontal_mirroring_read_write_roundtrip() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.set_mirroring(MirroringMode::Horizontal);
        // Write to all 4 nametables
        let test_values = [0x11, 0x22, 0x33, 0x44];
        let addresses = [0x2000, 0x2400, 0x2800, 0x2C00];

        for (i, &addr) in addresses.iter().enumerate() {
            ppu.write_address((addr >> 8) as u8);
            ppu.write_address((addr & 0xFF) as u8);
            ppu.write_data(test_values[i]);
        }

        // With horizontal mirroring: 0==1, 2==3
        // So we should have: ram[0]=0x22 (from NT1), ram[0x400]=0x44 (from NT3)
        // because NT1 and NT3 overwrite NT0 and NT2
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.read_data(); // skip buffer
        assert_eq!(ppu.read_data(), 0x22); // NT0 mirrors NT1

        ppu.write_address(0x28);
        ppu.write_address(0x00);
        ppu.read_data(); // skip buffer
        assert_eq!(ppu.read_data(), 0x44); // NT2 mirrors NT3
    }

    #[test]
    fn test_total_ticks_starts_at_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.total_cycles(), 0);
    }

    #[test]
    fn test_run_ticks_increments_counter() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(10);
        assert_eq!(ppu.total_cycles(), 10);
    }

    #[test]
    fn test_run_ticks_accumulates() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(5);
        ppu.run_ppu_cycles(3);
        ppu.run_ppu_cycles(7);
        assert_eq!(ppu.total_cycles(), 15);
    }

    #[test]
    fn test_reset_clears_total_ticks() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(100);
        ppu.reset();
        assert_eq!(ppu.total_cycles(), 0);
    }

    #[test]
    fn test_run_ticks_handles_large_values() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(1_000_000);
        ppu.run_ppu_cycles(500_000);
        assert_eq!(ppu.total_cycles(), 1_500_000);
    }

    #[test]
    fn test_ntsc_scanline_starts_at_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.scanline(), 0);
    }

    #[test]
    fn test_ntsc_pixel_starts_at_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_pal_scanline_starts_at_zero() {
        let ppu = PPU::new(TvSystem::Pal);
        assert_eq!(ppu.scanline(), 0);
    }

    #[test]
    fn test_pal_pixel_starts_at_zero() {
        let ppu = PPU::new(TvSystem::Pal);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ntsc_pixel_increments_with_cycles() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(5);
        assert_eq!(ppu.pixel(), 5);
        assert_eq!(ppu.scanline(), 0);
    }

    #[test]
    fn test_ntsc_scanline_increments_after_341_pixels() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(341);
        assert_eq!(ppu.pixel(), 0);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_ntsc_scanline_wraps_after_262_scanlines() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // 262 scanlines * 341 pixels = 89342 cycles per frame
        ppu.run_ppu_cycles(89342);
        assert_eq!(ppu.pixel(), 0);
        assert_eq!(ppu.scanline(), 0);
    }

    #[test]
    fn test_pal_scanline_increments_after_341_pixels() {
        let mut ppu = PPU::new(TvSystem::Pal);
        ppu.run_ppu_cycles(341);
        assert_eq!(ppu.pixel(), 0);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_pal_scanline_wraps_after_312_scanlines() {
        let mut ppu = PPU::new(TvSystem::Pal);
        // 312 scanlines * 341 pixels = 106392 cycles per frame
        ppu.run_ppu_cycles(106392);
        assert_eq!(ppu.pixel(), 0);
        assert_eq!(ppu.scanline(), 0);
    }

    #[test]
    fn test_ntsc_multiple_frames() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Run 2.5 frames
        ppu.run_ppu_cycles(89342 * 2 + 341 * 5 + 10);
        assert_eq!(ppu.scanline(), 5);
        assert_eq!(ppu.pixel(), 10);
    }

    #[test]
    fn test_pal_multiple_frames() {
        let mut ppu = PPU::new(TvSystem::Pal);
        // Run 1.5 frames
        ppu.run_ppu_cycles(106392 + 341 * 10 + 20);
        assert_eq!(ppu.scanline(), 10);
        assert_eq!(ppu.pixel(), 20);
    }

    #[test]
    fn test_reset_clears_scanline_and_pixel() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(5000);
        ppu.reset();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_status_vblank_flag_starts_clear() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        let status = ppu.get_status();
        assert_eq!(status & 0b1000_0000, 0);
    }

    #[test]
    fn test_status_vblank_flag_set_at_scanline_241() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Run to scanline 241 (start of VBlank)
        ppu.run_ppu_cycles(241 * 341);
        let status = ppu.get_status();
        assert_eq!(status & 0b1000_0000, 0b1000_0000);
    }

    #[test]
    fn test_status_vblank_flag_cleared_on_read() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Run to VBlank
        ppu.run_ppu_cycles(241 * 341);
        let status = ppu.get_status();
        assert_eq!(status & 0b1000_0000, 0b1000_0000);
        // Reading status should clear VBlank flag
        let status2 = ppu.get_status();
        assert_eq!(status2 & 0b1000_0000, 0);
    }

    #[test]
    fn test_status_sprite_0_hit_starts_clear() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        let status = ppu.get_status();
        assert_eq!(status & 0b0100_0000, 0);
    }

    #[test]
    fn test_status_sprite_overflow_starts_clear() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        let status = ppu.get_status();
        assert_eq!(status & 0b0010_0000, 0);
    }

    #[test]
    fn test_nmi_not_enabled_on_vblank_if_control_bit_not_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Don't set NMI bit in control register
        ppu.write_control(0x00);
        // Run to VBlank
        ppu.run_ppu_cycles(241 * 341);
        // NMI should not be enabled
        assert_eq!(ppu.poll_nmi(), false);
    }

    #[test]
    fn test_nmi_enabled_on_vblank_if_control_bit_set() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Set NMI generation bit in control register
        ppu.write_control(0b1000_0000);
        // Run to VBlank (scanline 241)
        ppu.run_ppu_cycles(241 * 341);
        // NMI should be enabled
        assert_eq!(ppu.poll_nmi(), true);
    }

    #[test]
    fn test_nmi_enabled_when_control_bit_set_during_vblank() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Don't set NMI bit initially
        ppu.write_control(0x00);
        // Run to VBlank
        ppu.run_ppu_cycles(241 * 341);
        // NMI should not be enabled yet
        assert_eq!(ppu.poll_nmi(), false);
        // Now enable NMI during VBlank
        ppu.write_control(0b1000_0000);
        // NMI should now be enabled
        assert_eq!(ppu.poll_nmi(), true);
    }

    #[test]
    fn test_nmi_not_enabled_when_control_bit_set_outside_vblank() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Set NMI bit while not in VBlank
        ppu.write_control(0b1000_0000);
        // NMI should not be enabled (we're not in VBlank)
        assert_eq!(ppu.poll_nmi(), false);
    }

    #[test]
    fn test_poll_nmi_clears_flag() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b1000_0000);
        ppu.run_ppu_cycles(241 * 341);
        // First poll returns true
        assert_eq!(ppu.poll_nmi(), true);
        // Second poll returns false (flag cleared)
        assert_eq!(ppu.poll_nmi(), false);
    }

    #[test]
    fn test_nmi_cleared_on_new_frame() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_control(0b1000_0000);
        // Run to VBlank
        ppu.run_ppu_cycles(241 * 341);
        assert_eq!(ppu.poll_nmi(), true);
        // Run to next frame (wrap around to scanline 0)
        ppu.run_ppu_cycles(21 * 341); // 262 total scanlines, so 21 more to wrap
        // NMI should be cleared on new frame
        assert_eq!(ppu.poll_nmi(), false);
    }

    #[test]
    fn test_render() {
        use crate::screen_buffer::ScreenBuffer;

        let mut ppu = PPU::new(TvSystem::Ntsc);
        let mut screen_buffer = ScreenBuffer::new();

        // Write some test data to the first nametable (0x2000-0x23FF)
        // Each nametable is 32x30 tiles = 960 bytes
        // Set a few tiles to different values
        ppu.write_address(0x20); // High byte of 0x2000
        ppu.write_address(0x00); // Low byte of 0x2000
        ppu.write_data(0x01); // First tile

        ppu.write_address(0x20); // High byte of 0x2001
        ppu.write_address(0x01); // Low byte of 0x2001
        ppu.write_data(0x02); // Second tile

        // Render the PPU to the screen buffer
        ppu.render(&mut screen_buffer);

        // For now, just verify that the function exists and can be called
        // We'll verify actual rendering in later iterations
        assert_eq!(screen_buffer.width(), 256);
        assert_eq!(screen_buffer.height(), 240);
    }

    // Tests for Loopy registers (v, t, x, w)
    #[test]
    fn test_v_register_initializes_to_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.v_register(), 0);
    }

    #[test]
    fn test_t_register_initializes_to_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.t_register(), 0);
    }

    #[test]
    fn test_x_register_initializes_to_zero() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.x_register(), 0);
    }

    #[test]
    fn test_w_register_initializes_to_false() {
        let ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.w_register(), false);
    }

    #[test]
    fn test_reset_clears_loopy_registers() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        // Manually set registers to non-zero values
        ppu.v = 0x1234;
        ppu.t = 0x5678;
        ppu.x = 0x07;
        ppu.w = true;
        
        ppu.reset();
        
        assert_eq!(ppu.v_register(), 0);
        assert_eq!(ppu.t_register(), 0);
        assert_eq!(ppu.x_register(), 0);
        assert_eq!(ppu.w_register(), false);
    }

    #[test]
    fn test_get_status_clears_w_register() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.w = true;
        ppu.get_status();
        assert_eq!(ppu.w_register(), false);
    }

    // PPUSCROLL ($2005) tests
    #[test]
    fn test_write_scroll_first_write_sets_fine_x() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_scroll(0b11111111); // All bits set
        // Fine X = data & 0x07 = 0b111 = 7
        assert_eq!(ppu.x_register(), 7);
    }

    #[test]
    fn test_write_scroll_first_write_sets_coarse_x_in_t() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_scroll(0b11111111); // All bits set
        // Coarse X = data >> 3 = 0b11111 = 31
        // t = (t & 0xFFE0) | (data >> 3)
        // t = 0x001F (coarse X in bits 0-4)
        assert_eq!(ppu.t_register() & 0x001F, 31);
    }

    #[test]
    fn test_write_scroll_first_write_toggles_w() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        assert_eq!(ppu.w_register(), false);
        ppu.write_scroll(0x12);
        assert_eq!(ppu.w_register(), true);
    }

    #[test]
    fn test_write_scroll_first_write_preserves_other_t_bits() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.t = 0x7FE0; // Set all bits except coarse X
        ppu.write_scroll(0b11111000); // coarse X = 31, fine X = 0
        // Should preserve bits 5-14, update bits 0-4
        assert_eq!(ppu.t_register(), 0x7FFF);
    }

    #[test]
    fn test_write_scroll_second_write_sets_fine_y() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00); // First write
        ppu.write_scroll(0b11111111); // Second write
        // Bits 12-14 should be set to fine Y (data & 0x07)
        assert_eq!((ppu.t_register() >> 12) & 0x07, 0x07);
    }

    #[test]
    fn test_write_scroll_second_write_sets_coarse_y_in_t() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00); // First write
        ppu.write_scroll(0b11111111); // Second write
        // Bits 5-9 should be set to coarse Y (data >> 3)
        assert_eq!((ppu.t_register() >> 5) & 0x1F, 0x1F);
    }

    #[test]
    fn test_write_scroll_second_write_toggles_w_back() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00); // First write (w becomes true)
        assert_eq!(ppu.w_register(), true);
        ppu.write_scroll(0x00); // Second write
        assert_eq!(ppu.w_register(), false); // w should toggle back to false
    }

    #[test]
    fn test_write_scroll_second_write_preserves_other_t_bits() {
        let mut ppu = PPU::new(TvSystem::Ntsc);
        ppu.t = 0x7FFF; // Set all bits
        ppu.write_scroll(0x00); // First write (clears bits 0-4)
        ppu.write_scroll(0xFF); // Second write
        // Bits 0-4 and 10-11 should be preserved
        assert_eq!(ppu.t_register() & 0x001F, 0x00); // Coarse X cleared by first write
        assert_eq!(ppu.t_register() & 0x0C00, 0x0C00); // Nametable bits preserved
    }
}
