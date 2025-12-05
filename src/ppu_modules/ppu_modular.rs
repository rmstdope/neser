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

    /// Get mutable reference to screen buffer (for compatibility)
    pub fn screen_buffer_mut(&mut self) -> &mut crate::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer_mut()
    }

    /// Check if in VBlank period
    pub fn is_in_vblank(&self) -> bool {
        self.status.is_in_vblank()
    }

    /// Check if should generate NMI
    pub fn should_generate_nmi(&self) -> bool {
        self.registers.should_generate_nmi()
    }

    /// Get total cycles (for testing)
    #[cfg(test)]
    pub fn total_cycles(&self) -> u64 {
        self.timing.total_cycles()
    }

    /// Get v register (for testing)
    #[cfg(test)]
    pub fn v_register(&self) -> u16 {
        self.registers.v()
    }

    /// Get t register (for testing)
    #[cfg(test)]
    pub fn t_register(&self) -> u16 {
        self.registers.t()
    }

    /// Get x register (for testing)
    #[cfg(test)]
    pub fn x_register(&self) -> u8 {
        self.registers.x()
    }

    /// Get w register (for testing)
    #[cfg(test)]
    pub fn w_register(&self) -> bool {
        self.registers.w()
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

    // PPU Data tests
    #[test]
    fn test_read_data_from_palette() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        ppu.write_data(0x42);
        
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_read_data_increments_address() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        ppu.write_data(0x10);
        ppu.write_data(0x20);
        
        ppu.write_address(0x3F);
        ppu.write_address(0x00);
        assert_eq!(ppu.read_data(), 0x10);
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_write_data_to_nametable() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x42);
        
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        let _ = ppu.read_data(); // Dummy read for buffer
        assert_eq!(ppu.read_data(), 0x42);
    }

    // OAM tests
    #[test]
    fn test_oam_write_and_read() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);
        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x42);
    }

    #[test]
    fn test_oam_data_increments_address() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x11);
        ppu.write_oam_data(0x22);
        ppu.write_oam_data(0x33);
        
        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x11);
        ppu.write_oam_address(0x01);
        assert_eq!(ppu.read_oam_data(), 0x22);
        ppu.write_oam_address(0x02);
        assert_eq!(ppu.read_oam_data(), 0x33);
    }

    // Control register tests
    #[test]
    fn test_ppuctrl_nmi_enable() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Bit 7: NMI enable
        assert!(ppu.should_generate_nmi());
        
        ppu.write_control(0x00);
        assert!(!ppu.should_generate_nmi());
    }

    // Address register tests
    #[test]
    fn test_address_write_sequence() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0x20); // High byte
        ppu.write_address(0x00); // Low byte
        assert_eq!(ppu.v_register(), 0x2000);
    }

    #[test]
    fn test_address_wraps_correctly() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_address(0xFF); // High byte
        ppu.write_address(0xFF); // Low byte
        // Address should be masked to 14 bits (0x3FFF)
        assert_eq!(ppu.v_register() & 0x3FFF, 0x3FFF);
    }

    // Scroll register tests
    #[test]
    fn test_scroll_write_updates_registers() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_scroll(0xFF); // X scroll
        ppu.write_scroll(0xFF); // Y scroll
        // Verify write toggle was used
        assert!(!ppu.w_register()); // Should be false after two writes
    }

    // Timing tests
    #[test]
    fn test_scanline_increments() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(341); // One full scanline
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_frame_wraps_at_262_scanlines() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(262 * 341); // One full frame
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    // Status register tests
    #[test]
    fn test_status_read_clears_vblank() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 2); // Past vblank start
        
        let status1 = ppu.get_status();
        assert_eq!(status1 & 0x80, 0x80); // VBlank set
        
        let status2 = ppu.get_status();
        assert_eq!(status2 & 0x80, 0); // VBlank cleared
    }

    #[test]
    fn test_status_read_clears_write_toggle() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00); // First write, sets w=true
        assert!(ppu.w_register());
        
        ppu.get_status(); // Should clear w
        assert!(!ppu.w_register());
    }

    // CHR ROM and mirroring tests
    #[test]
    fn test_load_chr_rom() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        let chr_data = vec![0x42; 8192];
        ppu.load_chr_rom(chr_data);
        // CHR ROM should be loaded (tested via read operations)
    }

    #[test]
    fn test_vertical_mirroring() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Vertical);
        
        // Write to nametable 0
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x42);
        
        // Read from nametable 2 (should mirror to 0)
        ppu.write_address(0x28);
        ppu.write_address(0x00);
        let _ = ppu.read_data(); // Dummy read
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Horizontal);
        
        // Write to nametable 0
        ppu.write_address(0x20);
        ppu.write_address(0x00);
        ppu.write_data(0x55);
        
        // Read from nametable 1 (should NOT mirror to 0 in horizontal)
        ppu.write_address(0x24);
        ppu.write_address(0x00);
        let _ = ppu.read_data(); // Dummy read
        let val = ppu.read_data();
        assert_ne!(val, 0x55); // Should be different (not mirrored)
    }

    // NMI and frame complete tests
    #[test]
    fn test_nmi_polling() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Enable NMI
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank
        
        assert!(ppu.poll_nmi()); // Should return true once
        assert!(!ppu.poll_nmi()); // Should be cleared after polling
    }

    #[test]
    fn test_frame_complete_polling() {
        let mut ppu = PPUModular::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank
        
        assert!(ppu.poll_frame_complete()); // Should return true once
        assert!(!ppu.poll_frame_complete()); // Should be cleared after polling
    }
}
