use crate::cartridge::MirroringMode;
use crate::nes::TvSystem;
use crate::ppu_modules::{Background, Memory, Registers, Rendering, Sprites, Status, Timing};

/// Refactored PPU using modular components
pub struct PPUModular {
    /// Timing and cycle management
    timing: Timing,
    /// Status flags (VBlank, sprite 0 hit, NMI)
    status: Status,
    /// Register management (PPUCTRL, PPUMASK, Loopy registers)
    registers: Registers,
    /// Memory management (VRAM, palette, CHR ROM)
    memory: Memory,
    /// Background rendering
    background: Background,
    /// Sprite rendering
    sprites: Sprites,
    /// Final rendering and screen output
    rendering: Rendering,
}

impl PPUModular {
    /// Create a new modular PPU instance
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            timing: Timing::new(tv_system),
            status: Status::new(),
            registers: Registers::new(),
            memory: Memory::new(),
            background: Background::new(),
            sprites: Sprites::new(),
            rendering: Rendering::new(),
        }
    }

    /// Reset the PPU to its initial state
    pub fn reset(&mut self) {
        self.timing.reset();
        self.status.reset();
        self.registers.reset();
        self.memory.reset();
        self.background.reset();
        self.sprites.reset();
    }

    /// Run the PPU for a specified number of cycles
    pub fn run_ppu_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Process a single PPU cycle
    fn tick(&mut self) {
        // Advance timing
        let _skipped = self.timing.tick(self.registers.is_rendering_enabled());

        // Clear VBlank start cycle flag from previous cycle
        self.status.clear_vblank_start_cycle();

        // Enter VBlank at scanline 241, pixel 1
        if self.timing.scanline() == 241 && self.timing.pixel() == 1 {
            self.status.enter_vblank(self.registers.should_generate_nmi());
        }

        // Exit VBlank at pre-render scanline, pixel 1
        let prerender_scanline = match self.timing.tv_system() {
            TvSystem::Ntsc => 261,
            TvSystem::Pal => 311,
        };
        if self.timing.scanline() == prerender_scanline && self.timing.pixel() == 1 {
            self.status.exit_vblank();
        }

        // TODO: Add background rendering pipeline
        // TODO: Add sprite evaluation and rendering
        // TODO: Add pixel composition
    }

    /// Write to control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        self.registers.write_control(value);
    }

    /// Write to mask register ($2001)
    pub fn write_mask(&mut self, value: u8) {
        self.registers.write_mask(value);
    }

    /// Read status register ($2002)
    pub fn get_status(&mut self) -> u8 {
        let status = self.status.read_status();
        self.registers.clear_w(); // Reading status clears write toggle
        status
    }

    /// Write to scroll register ($2005)
    pub fn write_scroll(&mut self, value: u8) {
        self.registers.write_scroll(value);
    }

    /// Write to address register ($2006)
    pub fn write_address(&mut self, value: u8) {
        self.registers.write_address(value);
    }

    /// Read from data register ($2007)
    pub fn read_data(&mut self) -> u8 {
        let addr = self.registers.v();
        let result = match addr {
            0x0000..=0x1FFF => {
                // CHR ROM: buffered read
                let buffered = self.registers.data_buffer();
                self.registers.set_data_buffer(self.memory.read_chr(addr));
                buffered
            }
            0x2000..=0x3EFF => {
                // Nametable: buffered read
                let buffered = self.registers.data_buffer();
                self.registers.set_data_buffer(self.memory.read_nametable(addr));
                buffered
            }
            0x3F00..=0x3FFF => {
                // Palette: immediate read
                let palette_data = self.memory.read_palette(addr);
                // Update buffer with nametable data underneath
                let mirrored_addr = addr & 0x2FFF;
                self.registers.set_data_buffer(self.memory.read_nametable(mirrored_addr));
                palette_data
            }
            _ => self.registers.data_buffer(),
        };
        
        self.registers.increment_vram_address();
        result
    }

    /// Write to data register ($2007)
    pub fn write_data(&mut self, value: u8) {
        let addr = self.registers.v();
        match addr {
            0x0000..=0x1FFF => {
                // CHR ROM is read-only
            }
            0x2000..=0x3EFF => {
                self.memory.write_nametable(addr, value);
            }
            0x3F00..=0x3FFF => {
                self.memory.write_palette(addr, value);
            }
            _ => {}
        }
        
        self.registers.increment_vram_address();
    }

    /// Load CHR ROM
    pub fn load_chr_rom(&mut self, chr_rom: Vec<u8>) {
        self.memory.load_chr_rom(chr_rom);
    }

    /// Set mirroring mode
    pub fn set_mirroring(&mut self, mirroring: MirroringMode) {
        self.memory.set_mirroring(mirroring);
    }

    /// Poll NMI
    pub fn poll_nmi(&mut self) -> bool {
        self.status.poll_nmi()
    }

    /// Poll frame complete
    pub fn poll_frame_complete(&mut self) -> bool {
        self.status.poll_frame_complete()
    }

    /// Get current scanline
    pub fn scanline(&self) -> u16 {
        self.timing.scanline()
    }

    /// Get current pixel
    pub fn pixel(&self) -> u16 {
        self.timing.pixel()
    }

    /// Write to OAM address register ($2003)
    pub fn write_oam_address(&mut self, value: u8) {
        self.registers.oam_address = value;
    }

    /// Write to OAM data register ($2004)
    pub fn write_oam_data(&mut self, value: u8) {
        self.sprites.write_oam(self.registers.oam_address, value);
        self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
    }

    /// Read from OAM data register ($2004)
    pub fn read_oam_data(&self) -> u8 {
        self.sprites.read_oam(self.registers.oam_address)
    }

    /// Get reference to screen buffer
    pub fn screen_buffer(&self) -> &crate::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppu_modular_new() {
        let ppu = PPUModular::new(TvSystem::Ntsc);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_modular_reset() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(100);
        ppu.reset();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_modular_write_control() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_control(0b1000_0000);
        // Control register should be set (verified internally)
    }

    #[test]
    fn test_ppu_modular_read_write_data() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        ppu.write_data(0x42);
        
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_ppu_modular_vblank() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        // Advance to VBlank (scanline 241, pixel 1)
        ppu.run_ppu_cycles(241 * 341 + 1);
        
        let status = ppu.get_status();
        // VBlank flag should be set (bit 7)
        assert_eq!(status & 0x80, 0x80);
        
        // Advance one more cycle to get past vblank_start_cycle
        ppu.run_ppu_cycles(1);
        
        // Reading status should clear VBlank flag (now that we're past vblank_start_cycle)
        let status_first_read = ppu.get_status();
        assert_eq!(status_first_read & 0x80, 0x80);
        
        // Second read should show cleared flag
        let status_second_read = ppu.get_status();
        assert_eq!(status_second_read & 0x80, 0);
    }
}
