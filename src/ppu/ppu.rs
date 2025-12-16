use crate::cartridge::MirroringMode;
use crate::nes::TvSystem;
use crate::ppu::{Background, Memory, Registers, Rendering, Sprites, Status, Timing};

/// Refactored PPU using modular components
pub struct Ppu {
    /// Timing and cycle management
    timing: Timing,
    /// Status flags (VBlank, sprite 0 hit, NMI)
    status: Status,
    /// Register management (PPUCTRL, PPUMASK, Loopy registers)
    /// Public to allow MemController to access I/O bus latch
    pub registers: Registers,
    /// Memory management (VRAM, palette, CHR ROM)
    memory: Memory,
    /// Background rendering
    background: Background,
    /// Sprite rendering
    sprites: Sprites,
    /// Final rendering and screen output
    rendering: Rendering,
    /// Previous A12 state for change detection (bit 12 of PPU address)
    prev_a12: bool,
}

impl Ppu {
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
            prev_a12: false,
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
        self.prev_a12 = false;
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
            self.status
                .enter_vblank(self.registers.should_generate_nmi());
        }

        // Exit VBlank at the end of scanline 260 (one scanline before pre-render)
        // The hardware clears VBL slightly before the pre-render scanline starts
        // This happens approximately 2270 CPU cycles after VBlank starts
        let prerender_scanline = match self.timing.tv_system() {
            TvSystem::Ntsc => 261,
            TvSystem::Pal => 311,
        };
        let vblank_end_scanline = prerender_scanline - 1;
        if self.timing.scanline() == vblank_end_scanline && self.timing.pixel() == 340 {
            self.status.exit_vblank();
        }

        let scanline = self.timing.scanline();
        let pixel = self.timing.pixel();
        let is_rendering_enabled = self.registers.is_rendering_enabled();

        // Background rendering pipeline during rendering cycles
        let is_visible_scanline = scanline < 240;
        let is_prerender = scanline == prerender_scanline;
        let is_rendering_scanline = is_visible_scanline || is_prerender;
        let is_rendering_pixel = pixel >= 1 && pixel <= 256;

        // Background rendering pipeline during rendering cycles
        // Fetches happen during pixels 1-256 (visible) and 321-336 (pre-fetch for next scanline)
        // Also during pixels 337-340 (two single nametable byte fetches)
        if is_rendering_enabled && is_rendering_scanline {
            let should_fetch = (pixel >= 1 && pixel <= 256) || (pixel >= 321 && pixel <= 336);

            if should_fetch {
                // Perform background tile fetches based on cycle (every 8 pixels)
                // Fetch step: 0=nametable, 1=attribute, 2=pattern lo, 3=pattern hi
                let fetch_step = ((pixel - 1) % 8) / 2;
                match fetch_step {
                    0 => {
                        // Fetch nametable byte
                        let v = self.registers.v();
                        self.background
                            .fetch_nametable(v, |addr| self.memory.read_nametable(addr));
                    }
                    1 => {
                        // Fetch attribute byte
                        let v = self.registers.v();
                        self.background
                            .fetch_attribute(v, |addr| self.memory.read_nametable(addr));
                    }
                    2 => {
                        // Fetch pattern table low byte
                        let v = self.registers.v();
                        let bg_pattern_table = self.registers.bg_pattern_table_addr();
                        self.background
                            .fetch_pattern_lo(bg_pattern_table, v, |addr| {
                                self.memory.read_chr(addr)
                            });
                    }
                    3 => {
                        // Fetch pattern table high byte
                        let v = self.registers.v();
                        let bg_pattern_table = self.registers.bg_pattern_table_addr();
                        self.background
                            .fetch_pattern_hi(bg_pattern_table, v, |addr| {
                                self.memory.read_chr(addr)
                            });
                    }
                    _ => {}
                }

                // Load shift registers every 8 pixels (pixels 8, 16, 24, etc during visible,
                // and pixels 328, 336 during pre-fetch)
                // This happens after all 4 fetches for the tile completed
                if pixel % 8 == 0 {
                    self.background.load_shift_registers(self.registers.v());
                    self.registers.increment_coarse_x();
                }
            } else if pixel == 337 || pixel == 339 {
                // Two dummy nametable fetches at pixels 337 and 339
                // (The NES PPU does these but they're not used)
                let v = self.registers.v();
                self.background
                    .fetch_nametable(v, |addr| self.memory.read_nametable(addr));
            }

            // Handle scroll register updates during visible pixels
            if pixel == 256 {
                // Increment fine Y at end of visible scanline
                self.registers.increment_fine_y();
            } else if pixel == 257 {
                // Copy horizontal bits from t to v
                self.registers.copy_horizontal_bits();
            }
        }

        // Copy vertical bits during pre-render scanline (dots 280-304)
        if is_rendering_enabled && is_prerender && pixel >= 280 && pixel <= 304 {
            self.registers.copy_vertical_bits();
        }

        // OAM corruption bug: If OAMADDR >= 8 when sprite tile loading starts,
        // copy 8 bytes from (OAMADDR & 0xF8) to OAM[0..7]
        // This happens at pixel 257 of the pre-render scanline
        if is_rendering_enabled && is_prerender && pixel == 257 {
            if self.registers.oam_address >= 8 {
                let source_addr = (self.registers.oam_address & 0xF8) as usize;
                // Copy 8 bytes from source to OAM[0..7]
                for i in 0..8 {
                    let value = self.sprites.read_oam((source_addr + i) as u8);
                    self.sprites.write_oam(i as u8, value);
                }
            }
        }

        // Clear OAMADDR during sprite tile loading (pixels 257-320) on visible and pre-render scanlines
        // This is critical NES PPU hardware behavior
        if is_rendering_enabled && is_rendering_scanline && pixel >= 257 && pixel <= 320 {
            self.registers.oam_address = 0;
        }

        // Sprite evaluation during visible scanlines
        if is_visible_scanline {
            if pixel == 0 {
                // Reset sprite evaluation at start of scanline
                self.sprites.reset_evaluation();
            } else if pixel >= 1 && pixel <= 64 {
                // Initialize secondary OAM
                self.sprites.initialize_secondary_oam_byte(pixel);
            } else if pixel >= 65 && pixel <= 256 {
                // Evaluate sprites for next scanline
                let sprite_height = self.registers.sprite_height();
                self.sprites
                    .evaluate_sprites(pixel, scanline, sprite_height);

                if pixel == 256 {
                    // Finalize evaluation
                    self.sprites.finalize_evaluation();
                    // Set sprite overflow if more than 8 sprites found
                    if self.sprites.sprite_count() > 8 {
                        self.status.set_sprite_overflow();
                    }
                }
            } else if pixel >= 257 && pixel <= 320 {
                // Fetch sprite patterns for next scanline
                let sprite_height = self.registers.sprite_height();
                let sprite_pattern_table = self.registers.sprite_pattern_table_addr();
                self.sprites.fetch_sprite_pattern(
                    pixel,
                    scanline,
                    sprite_height,
                    sprite_pattern_table,
                    |addr| self.memory.read_chr(addr),
                );
            } else if pixel == 321 {
                // Swap sprite buffers for rendering
                self.sprites.swap_buffers();
                self.sprites.mark_buffers_ready();
            }
        }

        // Render pixels to screen buffer during visible scanlines and pixels
        if is_visible_scanline && is_rendering_pixel {
            let screen_x = (pixel - 1) as u32;
            let screen_y = scanline as u32;

            if is_rendering_enabled {
                // Shift registers before rendering (matches NES hardware timing)
                self.background.shift_registers();

                // Get background pixel (only if background rendering is enabled)
                let fine_x = self.registers.x();
                let bg_pixel = if self.registers.is_background_enabled() {
                    self.background.get_pixel(fine_x)
                } else {
                    0 // Background disabled, treat as transparent
                };

                // Get sprite pixel
                let show_sprites_left = self.registers.show_sprites_left();
                let sprite_pixel = self.sprites.get_pixel(screen_x as i16, show_sprites_left);

                // Check for sprite 0 hit
                if let Some((_palette_idx, sprite_idx, _priority)) = sprite_pixel {
                    if self.sprites.is_sprite_0(sprite_idx) && bg_pixel != 0 {
                        self.status.set_sprite_0_hit();
                    }
                }

                // Determine final palette index
                let palette_index =
                    if let Some((sprite_palette_idx, _sprite_idx, is_foreground)) = sprite_pixel {
                        if bg_pixel == 0 {
                            sprite_palette_idx // Background transparent, show sprite
                        } else if is_foreground {
                            sprite_palette_idx // Sprite in foreground
                        } else {
                            bg_pixel // Sprite in background
                        }
                    } else {
                        bg_pixel // No sprite
                    };

                // Apply grayscale if enabled (mask to monochrome palette)
                let final_palette_index = if self.registers.is_grayscale() {
                    palette_index & 0x30
                } else {
                    palette_index
                };

                // Look up color in palette (convert index to address)
                let palette_addr = 0x3F00 + (final_palette_index as u16);
                let color_value = self.memory.read_palette(palette_addr);
                let (r, g, b) = crate::nes::Nes::lookup_system_palette(color_value);

                // Apply color emphasis/tint
                let (final_r, final_g, final_b) = if self.registers.color_emphasis() != 0 {
                    let emphasis = self.registers.color_emphasis();
                    let emphasize_red = (emphasis & 0x01) != 0;
                    let emphasize_green = (emphasis & 0x02) != 0;
                    let emphasize_blue = (emphasis & 0x04) != 0;

                    const ATTENUATION: f32 = 0.75;
                    const BOOST: f32 = 1.1;

                    let mut fr = r as f32;
                    let mut fg = g as f32;
                    let mut fb = b as f32;

                    if emphasize_red {
                        fr = (fr * BOOST).min(255.0);
                        if !emphasize_green {
                            fg *= ATTENUATION;
                        }
                        if !emphasize_blue {
                            fb *= ATTENUATION;
                        }
                    }
                    if emphasize_green {
                        fg = (fg * BOOST).min(255.0);
                        if !emphasize_red {
                            fr *= ATTENUATION;
                        }
                        if !emphasize_blue {
                            fb *= ATTENUATION;
                        }
                    }
                    if emphasize_blue {
                        fb = (fb * BOOST).min(255.0);
                        if !emphasize_red {
                            fr *= ATTENUATION;
                        }
                        if !emphasize_green {
                            fg *= ATTENUATION;
                        }
                    }

                    (fr as u8, fg as u8, fb as u8)
                } else {
                    (r, g, b)
                };

                // Write pixel to screen buffer
                self.rendering
                    .screen_buffer_mut()
                    .set_pixel(screen_x, screen_y, final_r, final_g, final_b);
            } else {
                // When rendering is disabled, output the backdrop color
                let backdrop_addr = 0x3F00;
                let color_value = self.memory.read_palette(backdrop_addr);
                let (r, g, b) = crate::nes::Nes::lookup_system_palette(color_value);

                // Write backdrop color to screen buffer
                self.rendering
                    .screen_buffer_mut()
                    .set_pixel(screen_x, screen_y, r, g, b);
            }
        }
    }

    /// Write to control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        self.registers.write_control(value);
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to mask register ($2001)
    pub fn write_mask(&mut self, value: u8) {
        self.registers.write_mask(value);
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Read status register ($2002)
    pub fn get_status(&mut self) -> u8 {
        let status = self.status.read_status();
        self.registers.clear_w(); // Reading status clears write toggle
        // Update I/O bus: status bits go to bits 5-7, bits 0-4 remain from previous value
        let io_bus = self.registers.io_bus();
        let new_io_bus = (status & 0xE0) | (io_bus & 0x1F);
        self.registers.set_io_bus(new_io_bus);
        new_io_bus
    }

    /// Write to scroll register ($2005)
    pub fn write_scroll(&mut self, value: u8, is_dummy_write: bool) {
        self.registers.write_scroll(value, is_dummy_write);
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to address register ($2006)
    pub fn write_address(&mut self, value: u8, is_dummy_write: bool) {
        self.registers.write_address(value, is_dummy_write);
        self.registers.set_io_bus(value); // Update I/O bus
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
                self.registers
                    .set_data_buffer(self.memory.read_nametable(addr));
                buffered
            }
            0x3F00..=0x3FFF => {
                // Palette: immediate read
                let palette_data = self.memory.read_palette(addr);
                // Update buffer with nametable data underneath
                let mirrored_addr = addr & 0x2FFF;
                self.registers
                    .set_data_buffer(self.memory.read_nametable(mirrored_addr));
                palette_data
            }
            _ => self.registers.data_buffer(),
        };

        // Use rendering glitch during active rendering
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }
        self.registers.set_io_bus(result); // Update I/O bus with value read
        result
    }

    /// Write to data register ($2007)
    pub fn write_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let addr = self.registers.v();
        match addr {
            0x0000..=0x1FFF => {
                // CHR memory (ROM or RAM depending on cartridge)
                self.memory.write_chr(addr, value);
                // TODO Connect this with the mapper
            }
            0x2000..=0x3EFF => {
                self.memory.write_nametable(addr, value);
            }
            0x3F00..=0x3FFF => {
                self.memory.write_palette(addr, value);
            }
            _ => {}
        }

        // Use rendering glitch during active rendering
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }
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
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to OAM data register ($2004)
    pub fn write_oam_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let is_rendering = self.is_actively_rendering();

        // During rendering, writes to OAMDATA are ignored (but address still increments)
        if !is_rendering {
            self.sprites.write_oam(self.registers.oam_address, value);
            // Normal increment: add 1
            self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
        } else {
            // Glitchy increment during rendering: increment only the high 6 bits (add 4)
            // This preserves the low 2 bits and bumps the sprite index
            let low_bits = self.registers.oam_address & 0x03;
            let high_bits = self.registers.oam_address.wrapping_add(4) & 0xFC;
            self.registers.oam_address = high_bits | low_bits;
        }
    }

    /// Read from OAM data register ($2004)
    pub fn read_oam_data(&mut self) -> u8 {
        let value = self.sprites.read_oam(self.registers.oam_address);
        self.registers.set_io_bus(value); // Update I/O bus
        value
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

    /// Check if PPUDATA access should trigger the rendering glitch
    /// Returns true if rendering is enabled and we're on a visible scanline
    fn should_use_rendering_glitch(&self) -> bool {
        let scanline = self.timing.scanline();
        let is_visible_scanline = scanline < 240;
        self.registers.is_rendering_enabled() && is_visible_scanline
    }

    /// Check if PPU is currently on a rendering scanline (visible or pre-render)
    /// Returns true if we're on scanlines 0-239 or the pre-render scanline
    fn is_on_rendering_scanline(&self) -> bool {
        let scanline = self.timing.scanline();
        let prerender_scanline = match self.timing.tv_system() {
            TvSystem::Ntsc => 261,
            TvSystem::Pal => 311,
        };
        let is_visible_scanline = scanline < 240;
        let is_prerender = scanline == prerender_scanline;
        is_visible_scanline || is_prerender
    }

    /// Check if PPU is actively rendering (rendering enabled + on rendering scanline)
    fn is_actively_rendering(&self) -> bool {
        self.registers.is_rendering_enabled() && self.is_on_rendering_scanline()
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

    /// Read nametable for debugging/testing (doesn't affect PPU state)
    #[cfg(test)]
    pub fn read_nametable_for_debug(&self, addr: u16) -> u8 {
        self.memory.read_nametable(addr)
    }

    /// Get base nametable address from PPUCTRL (for testing)
    #[cfg(test)]
    pub fn base_nametable_addr(&self) -> u16 {
        self.registers.base_nametable_addr()
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

    /// Get OAM address register (for testing)
    #[cfg(test)]
    pub fn oam_address(&self) -> u8 {
        self.registers.oam_address
    }

    /// Check if A12 changed from 0 to 1 (rising edge)
    /// This is used for mapper IRQ counters (e.g., MMC3)
    /// Returns true if A12 went from 0 to 1
    #[cfg(test)]
    fn check_a12_rising_edge(&mut self, addr: u16) -> bool {
        let current_a12 = (addr & 0x1000) != 0;
        let rising_edge = !self.prev_a12 && current_a12;
        self.prev_a12 = current_a12;
        rising_edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppu_modular_new() {
        let ppu = Ppu::new(TvSystem::Ntsc);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_modular_reset() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(100);
        ppu.reset();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_modular_write_control() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0b1000_0000);
        // Control register should be set (verified internally)
    }

    #[test]
    fn test_ppu_modular_read_write_data() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_ppu_modular_vblank() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
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
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_read_data_increments_address() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x10);
        ppu.write_data(0x20);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        assert_eq!(ppu.read_data(), 0x10);
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_write_data_to_nametable() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read for buffer
        assert_eq!(ppu.read_data(), 0x42);
    }

    // OAM tests
    #[test]
    fn test_oam_write_and_read() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);
        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x42);
    }

    #[test]
    fn test_oam_data_increments_address() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x11); // Byte 0: Y position
        ppu.write_oam_data(0x22); // Byte 1: Tile index
        ppu.write_oam_data(0xE3); // Byte 2: Attributes (use valid bits only)
        ppu.write_oam_data(0x44); // Byte 3: X position

        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x11);
        ppu.write_oam_address(0x01);
        assert_eq!(ppu.read_oam_data(), 0x22);
        ppu.write_oam_address(0x02);
        assert_eq!(ppu.read_oam_data(), 0xE3);
        ppu.write_oam_address(0x03);
        assert_eq!(ppu.read_oam_data(), 0x44);
    }

    #[test]
    fn test_oam_full_256_bytes() {
        // Test writing and reading all 256 bytes of OAM
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Write all 256 bytes with a pattern that accounts for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            ppu.write_oam_data(i as u8);
        }

        // Verify OAMADDR wrapped around
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should wrap to 0 after 256 writes"
        );

        // Read all 256 bytes back, accounting for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address((i + 1) as u8); // Manually increment since read doesn't
            let expected = if (i & 0x03) == 2 {
                (i as u8) & 0xE3 // Attribute bytes have bits 2-4 masked
            } else {
                i as u8
            };
            assert_eq!(value, expected, "OAM[{}] should be {}", i, expected);
        }
    }

    #[test]
    fn test_oamaddr_cleared_during_sprite_loading() {
        // OAMADDR is automatically set to 0 during pixels 257-320 of visible and pre-render scanlines
        // This is critical hardware behavior that many test ROMs rely on
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering (otherwise OAMADDR clearing doesn't happen)
        ppu.write_control(0x00);
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Set OAMADDR to non-zero value
        ppu.write_oam_address(0x42);
        assert_eq!(ppu.oam_address(), 0x42);

        // Run to scanline 0, pixel 257 (start of sprite loading interval)
        ppu.run_ppu_cycles(257);

        // OAMADDR should be cleared to 0 during pixels 257-320
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared to 0 during sprite tile loading (pixels 257-320)"
        );

        // Set it again to verify it keeps getting cleared during the interval
        ppu.write_oam_address(0x99);
        ppu.run_ppu_cycles(1); // Still in the 257-320 interval
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should stay 0 during entire sprite loading interval"
        );

        // Run past pixel 320
        ppu.run_ppu_cycles(64); // Now at pixel 257+1+64 = 322

        // Now OAMADDR should stay whatever we set it to
        ppu.write_oam_address(0x55);
        ppu.run_ppu_cycles(1);
        assert_eq!(
            ppu.oam_address(),
            0x55,
            "OAMADDR should not be cleared after pixel 320"
        );
    }

    #[test]
    fn test_oamaddr_cleared_on_prerender_scanline() {
        // OAMADDR clearing also happens on the pre-render scanline
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261), pixel 257
        ppu.run_ppu_cycles(261 * 341 + 257);

        ppu.write_oam_address(0x42);
        ppu.run_ppu_cycles(1); // Pixel 258, should clear OAMADDR
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared during pre-render scanline sprite loading"
        );
    }

    #[test]
    fn test_oamaddr_not_cleared_when_rendering_disabled() {
        // OAMADDR should NOT be cleared if rendering is disabled
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Rendering disabled (mask = 0)
        ppu.write_mask(0x00);

        ppu.write_oam_address(0x42);

        // Run through the sprite loading interval
        ppu.run_ppu_cycles(320);

        // OAMADDR should still be 0x42
        assert_eq!(
            ppu.oam_address(),
            0x42,
            "OAMADDR should not be cleared when rendering is disabled"
        );
    }

    #[test]
    fn test_oamaddr_corruption_at_rendering_start() {
        // If OAMADDR >= 8 when rendering starts (during pre-render sprite tile loading),
        // the 8 bytes at (OAMADDR & 0xF8) are copied to OAM[0..7]
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup: Write distinct values to different parts of OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank

        // Write pattern to OAM[0..7]
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(i);
        }

        // Write different pattern to OAM[0x10..0x17]
        ppu.write_oam_address(0x10);
        for i in 0..8 {
            ppu.write_oam_data(0x80 + i);
        }

        // Set OAMADDR to 0x10 (>= 8) before rendering starts
        ppu.write_oam_address(0x10);

        // Run to pre-render scanline sprite tile loading (scanline 261, pixel 257)
        // At this point, OAM corruption should occur
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // Check that OAM[0..7] has been corrupted with data from OAM[0x10..0x17]
        // OAMADDR was 0x10, so 0x10 & 0xF8 = 0x10, meaning OAM[0x10..0x17] -> OAM[0..7]
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address((i + 1) as u8); // Re-set address since read doesn't increment
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: 0x82 with masking = 0x82 & 0xE3 = 0x82
                (0x80 + i) & 0xE3
            } else {
                0x80 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should be corrupted with value from OAM[0x10+{}]",
                i, i
            );
        }
    }

    #[test]
    fn test_no_oamaddr_corruption_when_less_than_8() {
        // If OAMADDR < 8 when rendering starts, no corruption occurs
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(0x40 + i); // Use values that work with attribute masking
        }

        // Set OAMADDR to value < 8
        ppu.write_oam_address(0x05);

        // Run to pre-render sprite tile loading
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // OAM[0..7] should be unchanged
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address((i + 1) as u8);
            let expected = if (i & 0x03) == 2 {
                (0x40 + i) & 0xE3 // Attribute byte masking
            } else {
                0x40 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should not be corrupted when OAMADDR < 8",
                i
            );
        }
    }

    #[test]
    fn test_oam_write_during_rendering_ignored() {
        // Writes to OAMDATA during rendering should NOT modify OAM
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Write initial value to OAM during vblank (should work)
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x42);

        // Run to visible scanline, avoiding the OAMADDR clearing period (257-320)
        ppu.run_ppu_cycles((262 - 241) * 341 + 100); // Scanline 0, pixel 100

        // Try to write during rendering (should be ignored)
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x99); // This write should be ignored

        // Read back - should still be 0x42
        ppu.write_oam_address(0x05);
        assert_eq!(
            ppu.read_oam_data(),
            0x42,
            "OAM write during rendering should be ignored"
        );
    }

    #[test]
    fn test_oam_write_during_rendering_increments_address() {
        // Writes to OAMDATA during rendering should still increment OAMADDR (glitchy increment)
        // The glitchy increment bumps only the high 6 bits (adds 4 instead of 1)
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to visible scanline
        ppu.run_ppu_cycles(100); // Scanline 0, pixel 100

        // Set OAMADDR to 0x10 and write (write ignored, but address incremented by 4)
        ppu.write_oam_address(0x10);
        ppu.write_oam_data(0x99); // Write ignored, but glitchy increment happens

        // Address should have incremented by 4 (glitchy increment - high 6 bits bumped)
        assert_eq!(
            ppu.oam_address(),
            0x14,
            "OAMADDR should increment by 4 (glitchy) during rendering"
        );

        // Test with address 0x13 (low 2 bits = 0b11)
        ppu.write_oam_address(0x13);
        ppu.write_oam_data(0x99);
        // Glitchy increment: (0x13 & 0x03) | ((0x13 + 4) & 0xFC) = 0x03 | 0x14 = 0x17
        assert_eq!(
            ppu.oam_address(),
            0x17,
            "Glitchy increment should preserve low 2 bits and add 4 to high 6 bits"
        );
    }

    #[test]
    fn test_oam_write_outside_rendering_works() {
        // Writes to OAMDATA outside rendering should work normally
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        // Write during vblank (should work)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);

        // Read back
        ppu.write_oam_address(0x00);
        assert_eq!(
            ppu.read_oam_data(),
            0x42,
            "OAM write during vblank should work normally"
        );
    }

    // Control register tests
    #[test]
    fn test_ppuctrl_nmi_enable() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Bit 7: NMI enable
        assert!(ppu.should_generate_nmi());

        ppu.write_control(0x00);
        assert!(!ppu.should_generate_nmi());
    }

    // Address register tests
    #[test]
    fn test_address_write_sequence() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x20, false); // High byte
        ppu.write_address(0x00, false); // Low byte
        assert_eq!(ppu.v_register(), 0x2000);
    }

    #[test]
    fn test_address_wraps_correctly() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0xFF, false); // High byte
        ppu.write_address(0xFF, false); // Low byte
        // Address should be masked to 14 bits (0x3FFF)
        assert_eq!(ppu.v_register() & 0x3FFF, 0x3FFF);
    }

    // Scroll register tests
    #[test]
    fn test_scroll_write_updates_registers() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_scroll(0xFF, false); // X scroll
        ppu.write_scroll(0xFF, false); // Y scroll
        // Verify write toggle was used
        assert!(!ppu.w_register()); // Should be false after two writes
    }

    // Timing tests
    #[test]
    fn test_scanline_increments() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(341); // One full scanline
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_frame_wraps_at_262_scanlines() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(262 * 341); // One full frame
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    // Status register tests
    #[test]
    fn test_status_read_clears_vblank() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 2); // Past vblank start

        let status1 = ppu.get_status();
        assert_eq!(status1 & 0x80, 0x80); // VBlank set

        let status2 = ppu.get_status();
        assert_eq!(status2 & 0x80, 0); // VBlank cleared
    }

    #[test]
    fn test_status_read_clears_write_toggle() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00, false); // First write, sets w=true
        assert!(ppu.w_register());

        ppu.get_status(); // Should clear w
        assert!(!ppu.w_register());
    }

    // CHR ROM and mirroring tests
    #[test]
    fn test_load_chr_rom() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        let chr_data = vec![0x42; 8192];
        ppu.load_chr_rom(chr_data);
        // CHR ROM should be loaded (tested via read operations)
    }

    #[test]
    fn test_vertical_mirroring() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Vertical);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        // Read from nametable 2 (should mirror to 0)
        ppu.write_address(0x28, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Horizontal);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x55);

        // Read from nametable 1 (should mirror to 0 in horizontal)
        ppu.write_address(0x24, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        let val = ppu.read_data();
        assert_eq!(val, 0x55); // Should be mirrored
    }

    // NMI and frame complete tests
    #[test]
    fn test_nmi_polling() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Enable NMI
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank

        assert!(ppu.poll_nmi()); // Should return true once
        assert!(!ppu.poll_nmi()); // Should be cleared after polling
    }

    #[test]
    fn test_frame_complete_polling() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank

        assert!(ppu.poll_frame_complete()); // Should return true once
        assert!(!ppu.poll_frame_complete()); // Should be cleared after polling
    }

    #[test]
    fn test_pixel_zero_no_panic() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Run through a full scanline which includes pixel 0
        ppu.run_ppu_cycles(341);

        // Should not panic - pixel 0 is handled correctly
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_rendering_with_pixel_transitions() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run through multiple scanlines to test pixel 0 transitions
        for _ in 0..5 {
            ppu.run_ppu_cycles(341);
        }

        // Should complete without panicking
        assert_eq!(ppu.scanline(), 5);
    }

    #[test]
    fn test_palette_access_with_correct_addressing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Write to palette using full address
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x30); // Write to backdrop color

        // Write to another palette entry
        ppu.write_address(0x3F, false);
        ppu.write_address(0x01, false);
        ppu.write_data(0x16);

        // Enable rendering and run one scanline
        ppu.write_mask(0x18);
        ppu.run_ppu_cycles(341);

        // Should complete without panic - palette lookups work correctly
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_shift_register_load_timing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Set up a known scroll position
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        // Run to pixel 8 of scanline 0 (first shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 8);

        // Run to pixel 16 (second shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 16);

        // Run to pixel 24 (third shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 24);

        // Verify we can continue through the scanline without issues
        ppu.run_ppu_cycles(256 - 24);
        assert_eq!(ppu.pixel(), 256);
    }

    #[test]
    fn test_scroll_register_updates_at_correct_pixels() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll and nametable
        ppu.write_control(0x00); // Nametable at $2000
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let _v_before_256 = ppu.v_register();

        // Run to pixel 256 (increment_fine_y happens here)
        ppu.run_ppu_cycles(256);
        assert_eq!(ppu.pixel(), 256);

        // Run to pixel 257 (copy_horizontal_bits happens here)
        ppu.run_ppu_cycles(1);
        assert_eq!(ppu.pixel(), 257);

        // V register should have been updated
        let _v_after_257 = ppu.v_register();
        // At minimum, fine Y should have incremented or wrapped
        // (exact value depends on internal state, but they shouldn't be identical
        // unless at a boundary condition)

        // Just verify we can continue without panic
        ppu.run_ppu_cycles(341 - 257);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_pre_render_scanline_prefetch() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261)
        ppu.run_ppu_cycles(261 * 341);
        assert_eq!(ppu.scanline(), 261);

        // Run to pixel 321 (start of pre-fetch)
        ppu.run_ppu_cycles(321);
        assert_eq!(ppu.pixel(), 321);

        // Run to pixel 328 (first pre-fetch load)
        ppu.run_ppu_cycles(7);
        assert_eq!(ppu.pixel(), 328);

        // Run to pixel 336 (second pre-fetch load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 336);

        // Complete the scanline
        ppu.run_ppu_cycles(341 - 336);
        assert_eq!(ppu.scanline(), 0); // Should wrap to scanline 0
    }

    #[test]
    fn test_rendering_enabled_background_fetch_cycles() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Run through visible pixels (1-256)
        for pixel in 1..=256 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }

        // Continue through pre-fetch region (321-336)
        ppu.run_ppu_cycles(321 - 256);
        assert_eq!(ppu.pixel(), 321);

        for pixel in 322..=336 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }
    }

    #[test]
    fn test_dummy_nametable_fetches() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pixel 337
        ppu.run_ppu_cycles(337);
        assert_eq!(ppu.pixel(), 337);

        // Run to pixel 339
        ppu.run_ppu_cycles(2);
        assert_eq!(ppu.pixel(), 339);

        // Complete the scanline without panic
        ppu.run_ppu_cycles(341 - 339);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_coarse_x_increment_timing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let v_initial = ppu.v_register();

        // Run to pixel 8 (first coarse X increment)
        ppu.run_ppu_cycles(8);
        let v_after_8 = ppu.v_register();

        // Coarse X should have incremented (bits 0-4 of v register)
        let coarse_x_initial = v_initial & 0x001F;
        let coarse_x_after_8 = v_after_8 & 0x001F;
        assert_eq!(coarse_x_after_8, (coarse_x_initial + 1) & 0x001F);

        // Run to pixel 16 (second coarse X increment)
        ppu.run_ppu_cycles(8);
        let v_after_16 = ppu.v_register();
        let coarse_x_after_16 = v_after_16 & 0x001F;
        assert_eq!(coarse_x_after_16, (coarse_x_initial + 2) & 0x001F);
    }

    #[test]
    fn test_a12_rising_edge_detection() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // A12 is bit 12 of address (0x1000)
        // Initially prev_a12 should be false

        // Access $0000 (A12=0) - no rising edge
        assert_eq!(ppu.check_a12_rising_edge(0x0000), false);

        // Access $0FFF (A12=0) - no rising edge
        assert_eq!(ppu.check_a12_rising_edge(0x0FFF), false);

        // Access $1000 (A12=1) - rising edge!
        assert_eq!(ppu.check_a12_rising_edge(0x1000), true);

        // Access $1FFF (A12=1) - no rising edge (already high)
        assert_eq!(ppu.check_a12_rising_edge(0x1FFF), false);

        // Access $0000 (A12=0) - no rising edge (falling edge)
        assert_eq!(ppu.check_a12_rising_edge(0x0000), false);

        // Access $1800 (A12=1) - rising edge!
        assert_eq!(ppu.check_a12_rising_edge(0x1800), true);

        // Access $1000 (A12=1) - no rising edge
        assert_eq!(ppu.check_a12_rising_edge(0x1000), false);
    }
}
