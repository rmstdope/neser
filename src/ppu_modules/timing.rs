use crate::nes::TvSystem;

/// Number of PPU cycles (pixels) per scanline
const PIXELS_PER_SCANLINE: u16 = 341;

/// Manages PPU timing, including scanlines, pixels, cycles, and frame counting
pub struct Timing {
    /// Total number of PPU ticks since reset
    total_cycles: u64,
    /// TV system (NTSC or PAL)
    tv_system: TvSystem,
    /// Current scanline (0-261 for NTSC, 0-311 for PAL)
    pub scanline: u16,
    /// Current pixel within scanline (0-340)
    pub pixel: u16,
    /// Frame counter for odd/even frame tracking (used for NTSC odd frame skip)
    frame_count: u64,
}

impl Timing {
    /// Create a new Timing instance
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            total_cycles: 0,
            tv_system,
            scanline: 0,
            pixel: 0,
            frame_count: 0,
        }
    }

    /// Reset timing to initial state
    pub fn reset(&mut self) {
        self.total_cycles = 0;
        self.scanline = 0;
        self.pixel = 0;
        self.frame_count = 0;
    }

    /// Advance timing by one PPU cycle
    /// Returns true if an odd frame skip occurred
    pub fn tick(&mut self, rendering_enabled: bool) -> bool {
        self.total_cycles += 1;

        // NTSC odd frame skip: On odd frames with rendering enabled,
        // skip from pre-render scanline dot 339 directly to scanline 0 dot 0
        let should_skip_odd_frame = self.tv_system == TvSystem::Ntsc
            && (self.frame_count & 1) == 1 // Odd frame
            && rendering_enabled
            && self.scanline == 261 // Pre-render scanline
            && self.pixel == 339;

        if should_skip_odd_frame {
            // Skip dot 340 and go directly to scanline 0, dot 0
            self.pixel = 0;
            self.scanline = 0;
            self.frame_count += 1;
            true
        } else {
            // Normal pixel advancement
            self.pixel += 1;
            if self.pixel >= PIXELS_PER_SCANLINE {
                self.pixel = 0;
                self.scanline += 1;

                let scanlines_per_frame = self.tv_system.scanlines_per_frame();
                if self.scanline >= scanlines_per_frame {
                    self.scanline = 0;
                    self.frame_count += 1;
                }
            }
            false
        }
    }

    /// Get the total number of cycles since reset
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Get the current scanline
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Get the current pixel within the scanline
    pub fn pixel(&self) -> u16 {
        self.pixel
    }

    /// Get the frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get the TV system
    pub fn tv_system(&self) -> TvSystem {
        self.tv_system
    }

    /// Check if we're currently in a rendering cycle
    /// Rendering cycles occur during visible scanlines (0-239) and pre-render scanline (261)
    /// at pixel positions 0-256 and 328-336
    pub fn is_rendering_cycle(&self) -> bool {
        let is_visible_scanline = self.scanline < 240;
        let is_prerender_scanline = self.scanline == 261;

        if is_visible_scanline || is_prerender_scanline {
            // Dots 0-256: background and sprite fetching/rendering
            // Dots 257-320: sprite pattern fetching for next scanline
            // Dots 321-336: first two tiles for next scanline
            // Dots 337-340: unknown nametable fetches
            self.pixel <= 256 || (self.pixel >= 328 && self.pixel <= 336)
        } else {
            false
        }
    }

    /// Check if we're currently rendering a visible pixel
    /// Visible pixels are rendered during scanlines 0-239, pixels 1-256
    pub fn is_visible_pixel(&self) -> bool {
        self.scanline < 240 && self.pixel >= 1 && self.pixel <= 256
    }

    /// Get the current fetch step (0-7) within the 8-cycle pattern
    /// Returns which of the 8 fetch operations should occur this cycle
    pub fn get_fetch_step(&self) -> u8 {
        ((self.pixel - 1) % 8) as u8
    }

    /// Check if shift registers should be loaded this cycle
    /// This occurs every 8 cycles during rendering (after pattern fetch completes)
    pub fn should_load_shift_registers(&self) -> bool {
        self.pixel > 0 && (self.pixel % 8) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_new() {
        let timing = Timing::new(TvSystem::Ntsc);
        assert_eq!(timing.scanline(), 0);
        assert_eq!(timing.pixel(), 0);
        assert_eq!(timing.total_cycles(), 0);
        assert_eq!(timing.frame_count(), 0);
    }

    #[test]
    fn test_timing_reset() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        timing.tick(false);
        timing.reset();
        assert_eq!(timing.scanline(), 0);
        assert_eq!(timing.pixel(), 0);
        assert_eq!(timing.total_cycles(), 0);
    }

    #[test]
    fn test_timing_tick_increments_pixel() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        timing.tick(false);
        assert_eq!(timing.pixel(), 1);
        assert_eq!(timing.total_cycles(), 1);
    }

    #[test]
    fn test_timing_scanline_wraps() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        // Advance to end of scanline
        for _ in 0..341 {
            timing.tick(false);
        }
        assert_eq!(timing.scanline(), 1);
        assert_eq!(timing.pixel(), 0);
    }

    #[test]
    fn test_timing_frame_wraps() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        // Advance to end of frame (262 scanlines * 341 pixels)
        for _ in 0..(262 * 341) {
            timing.tick(false);
        }
        assert_eq!(timing.scanline(), 0);
        assert_eq!(timing.pixel(), 0);
        assert_eq!(timing.frame_count(), 1);
    }

    #[test]
    fn test_timing_odd_frame_skip() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        // Advance to frame 1 (odd frame)
        for _ in 0..(262 * 341) {
            timing.tick(false);
        }
        assert_eq!(timing.frame_count(), 1);
        
        // Advance to scanline 261, pixel 339 with rendering enabled
        for _ in 0..(261 * 341 + 339) {
            timing.tick(true);
        }
        
        assert_eq!(timing.scanline(), 261);
        assert_eq!(timing.pixel(), 339);
        
        // Next tick should skip to scanline 0, pixel 0
        let skipped = timing.tick(true);
        assert!(skipped);
        assert_eq!(timing.scanline(), 0);
        assert_eq!(timing.pixel(), 0);
    }

    #[test]
    fn test_is_rendering_cycle() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        
        // Visible scanline, pixel 100
        timing.scanline = 0;
        timing.pixel = 100;
        assert!(timing.is_rendering_cycle());
        
        // Vblank scanline
        timing.scanline = 241;
        timing.pixel = 100;
        assert!(!timing.is_rendering_cycle());
        
        // Pre-render scanline
        timing.scanline = 261;
        timing.pixel = 100;
        assert!(timing.is_rendering_cycle());
    }

    #[test]
    fn test_is_visible_pixel() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        
        // Visible pixel
        timing.scanline = 100;
        timing.pixel = 100;
        assert!(timing.is_visible_pixel());
        
        // Pixel 0 is not visible
        timing.pixel = 0;
        assert!(!timing.is_visible_pixel());
        
        // Vblank is not visible
        timing.scanline = 241;
        timing.pixel = 100;
        assert!(!timing.is_visible_pixel());
    }
}
