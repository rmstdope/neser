use crate::console::TvSystem;

/// Number of PPU cycles (pixels) per scanline
pub(crate) const PIXELS_PER_SCANLINE: u16 = 341;

// Scanline constants
/// First scanline where VBlank begins (scanlines 241-260 for NTSC, 241-310 for PAL)
pub(crate) const VBLANK_START_SCANLINE: u16 = 241;
/// VBlank NMI edge latching occurs at pixel 2 of the VBlank start scanline
pub(crate) const VBLANK_NMI_LATCH_PIXEL: u16 = 2;
/// First visible scanline (0-239 are visible scanlines)
pub(crate) const FIRST_VISIBLE_SCANLINE: u16 = 0;
/// Last visible scanline + 1 (scanlines 0-239 are visible)
pub(crate) const LAST_VISIBLE_SCANLINE_PLUS_ONE: u16 = 240;
/// NTSC pre-render scanline (scanline 261)
pub(crate) const NTSC_PRERENDER_SCANLINE: u16 = 261;
/// PAL pre-render scanline (scanline 311)
pub(crate) const PAL_PRERENDER_SCANLINE: u16 = 311;

// Dot/pixel constants
/// Last dot in a scanline (dots 0-340)
pub(crate) const LAST_DOT: u16 = 340;
/// First dot/pixel in a scanline (0)
pub(crate) const FIRST_DOT: u16 = 0;
/// Dot where NTSC odd frame skip occurs on pre-render scanline
const ODD_FRAME_SKIP_DOT: u16 = 339;
/// First visible pixel (pixels 1-256 are visible)
pub(crate) const FIRST_VISIBLE_PIXEL: u16 = 1;
/// Last visible pixel (pixels 1-256 are visible)
pub(crate) const LAST_VISIBLE_PIXEL: u16 = 256;
/// Pixel where fine Y increment happens (end of visible scanline)
pub(crate) const FINE_Y_INCREMENT_PIXEL: u16 = 256;
/// Pixel where horizontal bits are copied from t to v
pub(crate) const HORIZONTAL_BITS_COPY_PIXEL: u16 = 257;
/// First pixel of sprite tile loading range (pixels 257-320)
pub(crate) const SPRITE_TILE_LOAD_START: u16 = 257;
/// Last pixel of sprite tile loading range (pixels 257-320)
pub(crate) const SPRITE_TILE_LOAD_END: u16 = 320;
/// First pixel of background pre-fetch range (pixels 321-336)
pub(crate) const BG_PREFETCH_START: u16 = 321;
/// Last pixel of background pre-fetch range (pixels 321-336)
pub(crate) const BG_PREFETCH_END: u16 = 336;
/// First pixel where background pre-fetch shift happens (329)
pub(crate) const BG_PREFETCH_SHIFT_START: u16 = 329;
/// First pixel of rendering cycle range (dots 328-336)
const RENDERING_CYCLE_START: u16 = 328;
/// Last pixel of rendering cycle range (dots 328-336)
const RENDERING_CYCLE_END: u16 = 336;
/// First dummy nametable fetch pixel
pub(crate) const DUMMY_NT_FETCH_1: u16 = 337;
/// Second dummy nametable fetch pixel
pub(crate) const DUMMY_NT_FETCH_2: u16 = 339;
/// First pixel of vertical bits copy range on pre-render scanline
pub(crate) const VERTICAL_BITS_COPY_START: u16 = 280;
/// Last pixel of vertical bits copy range on pre-render scanline
pub(crate) const VERTICAL_BITS_COPY_END: u16 = 304;

/// Manages PPU timing, including scanlines, pixels, cycles, and frame counting
pub struct Timing {
    /// Total number of PPU ticks since reset
    total_cycles: u64,
    /// TV system (NTSC or PAL)
    tv_system: TvSystem,
    /// Current scanline (0-261 for NTSC, 0-311 for PAL)
    pub scanline: u16,
    /// Current pixel within scanline (0-340, i.e., 0 to LAST_DOT)
    pub pixel: u16,
    /// Frame counter for odd/even frame tracking (used for NTSC odd frame skip)
    frame_count: u64,

    /// Rendering-enabled state delayed by 1 PPU tick.
    rendering_enabled_d1: bool,
    /// Rendering-enabled state delayed by 2 PPU ticks.
    rendering_enabled_d2: bool,
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
            rendering_enabled_d1: false,
            rendering_enabled_d2: false,
        }
    }

    /// Reset timing to initial state
    pub fn reset(&mut self) {
        self.total_cycles = 0;
        self.scanline = 0;
        self.pixel = 0;
        self.frame_count = 0;
        self.rendering_enabled_d1 = false;
        self.rendering_enabled_d2 = false;
    }

    fn rendering_enabled_for_odd_frame_skip(&mut self, rendering_enabled: bool) -> bool {
        let rendering_enabled_for_odd_skip = self.rendering_enabled_d2;
        self.rendering_enabled_d2 = self.rendering_enabled_d1;
        self.rendering_enabled_d1 = rendering_enabled;
        rendering_enabled_for_odd_skip
    }

    /// Advance timing by one PPU cycle
    /// Returns true if an odd frame skip occurred
    pub fn tick(&mut self, rendering_enabled: bool) -> bool {
        self.total_cycles += 1;

        // For the odd-frame skip decision, use a slightly delayed rendering-enable
        // signal. This matches blargg ppu_vbl_nmi 10's $2001 boundary near the end
        // of the pre-render scanline.
        let rendering_enabled_for_odd_skip =
            self.rendering_enabled_for_odd_frame_skip(rendering_enabled);

        // NTSC odd frame skip: On odd frames with rendering enabled,
        // skip from pre-render scanline dot 339 directly to scanline 0 dot 0
        let should_skip_odd_frame = self.tv_system == TvSystem::Ntsc
            && (self.frame_count & 1) == 1 // Odd frame
            && rendering_enabled_for_odd_skip
            && self.scanline == NTSC_PRERENDER_SCANLINE
            && self.pixel == ODD_FRAME_SKIP_DOT;

        if should_skip_odd_frame {
            // Skip dot 340 and go directly to scanline 0, dot 0
            self.pixel = FIRST_DOT;
            self.scanline = FIRST_VISIBLE_SCANLINE;
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

    /// Restore timing state from a save-state.
    pub fn restore_state(
        &mut self,
        scanline: u16,
        pixel: u16,
        total_cycles: u64,
        frame_count: u64,
    ) {
        self.scanline = scanline;
        self.pixel = pixel;
        self.total_cycles = total_cycles;
        self.frame_count = frame_count;
        // Note: rendering_enabled delays will be recalculated during emulation
        self.rendering_enabled_d1 = false;
        self.rendering_enabled_d2 = false;
    }

    pub fn rendering_enabled_delays(&self) -> (bool, bool) {
        (self.rendering_enabled_d1, self.rendering_enabled_d2)
    }

    pub fn set_rendering_enabled_delays(&mut self, d1: bool, d2: bool) {
        self.rendering_enabled_d1 = d1;
        self.rendering_enabled_d2 = d2;
    }

    /// Get the TV system
    pub fn tv_system(&self) -> TvSystem {
        self.tv_system
    }

    /// Check if we're currently in a rendering cycle
    /// Rendering cycles occur during visible scanlines and pre-render scanline
    /// at pixel positions 0-256 and 328-336
    #[cfg(test)]
    pub fn is_rendering_cycle(&self) -> bool {
        let is_visible_scanline = self.scanline < LAST_VISIBLE_SCANLINE_PLUS_ONE;
        let is_prerender_scanline = match self.tv_system {
            TvSystem::Ntsc => self.scanline == NTSC_PRERENDER_SCANLINE,
            TvSystem::Pal => self.scanline == PAL_PRERENDER_SCANLINE,
        };

        if is_visible_scanline || is_prerender_scanline {
            // Dots 0-256: background and sprite fetching/rendering
            // Dots 257-320: sprite pattern fetching for next scanline
            // Dots 321-336: first two tiles for next scanline
            // Dots 337-340: unknown nametable fetches
            self.pixel <= LAST_VISIBLE_PIXEL || (self.pixel >= RENDERING_CYCLE_START && self.pixel <= RENDERING_CYCLE_END)
        } else {
            false
        }
    }

    /// Check if we're currently rendering a visible pixel
    /// Visible pixels are rendered during scanlines 0-239, pixels 1-256
    #[cfg(test)]
    pub fn is_visible_pixel(&self) -> bool {
        self.scanline < LAST_VISIBLE_SCANLINE_PLUS_ONE 
            && self.pixel >= FIRST_VISIBLE_PIXEL 
            && self.pixel <= LAST_VISIBLE_PIXEL
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingDebugState {
    pub total_cycles: u64,
    pub tv_system: TvSystem,
    pub scanline: u16,
    pub pixel: u16,
    pub frame_count: u64,
    pub rendering_enabled_d1: bool,
    pub rendering_enabled_d2: bool,
}

#[cfg(test)]
impl Timing {
    pub fn debug_state(&self) -> TimingDebugState {
        TimingDebugState {
            total_cycles: self.total_cycles,
            tv_system: self.tv_system,
            scanline: self.scanline,
            pixel: self.pixel,
            frame_count: self.frame_count,
            rendering_enabled_d1: self.rendering_enabled_d1,
            rendering_enabled_d2: self.rendering_enabled_d2,
        }
    }

    pub fn set_debug_state(&mut self, state: TimingDebugState) {
        self.total_cycles = state.total_cycles;
        self.tv_system = state.tv_system;
        self.scanline = state.scanline;
        self.pixel = state.pixel;
        self.frame_count = state.frame_count;
        self.rendering_enabled_d1 = state.rendering_enabled_d1;
        self.rendering_enabled_d2 = state.rendering_enabled_d2;
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
        for _ in 0..PIXELS_PER_SCANLINE {
            timing.tick(false);
        }
        assert_eq!(timing.scanline(), 1);
        assert_eq!(timing.pixel(), 0);
    }

    #[test]
    fn test_timing_frame_wraps() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        // Advance to end of frame (262 scanlines * 341 pixels)
        for _ in 0..(262 * (PIXELS_PER_SCANLINE as usize)) {
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
        for _ in 0..(262 * (PIXELS_PER_SCANLINE as usize)) {
            timing.tick(false);
        }
        assert_eq!(timing.frame_count(), 1);

        // Advance to scanline 261, pixel 339 with rendering enabled
        for _ in 0..(261 * (PIXELS_PER_SCANLINE as usize) + 339) {
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
        timing.scanline = FIRST_VISIBLE_SCANLINE;
        timing.pixel = 100;
        assert!(timing.is_rendering_cycle());

        // Vblank scanline
        timing.scanline = VBLANK_START_SCANLINE;
        timing.pixel = 100;
        assert!(!timing.is_rendering_cycle());

        // Pre-render scanline
        timing.scanline = NTSC_PRERENDER_SCANLINE;
        timing.pixel = 100;
        assert!(timing.is_rendering_cycle());
    }

    #[test]
    fn test_is_rendering_cycle_pal_prerender_scanline() {
        let mut timing = Timing::new(TvSystem::Pal);

        // PAL pre-render scanline is 311
        timing.scanline = PAL_PRERENDER_SCANLINE;
        timing.pixel = 100;
        assert!(timing.is_rendering_cycle());

        // NTSC pre-render scanline should not be treated as rendering for PAL
        timing.scanline = NTSC_PRERENDER_SCANLINE;
        timing.pixel = 100;
        assert!(!timing.is_rendering_cycle());
    }

    #[test]
    fn test_is_visible_pixel() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // Visible pixel
        timing.scanline = 100;
        timing.pixel = 100;
        assert!(timing.is_visible_pixel());

        // Pixel 0 is not visible
        timing.pixel = FIRST_DOT;
        assert!(!timing.is_visible_pixel());

        // Vblank is not visible
        timing.scanline = VBLANK_START_SCANLINE;
        timing.pixel = 100;
        assert!(!timing.is_visible_pixel());
    }

    #[test]
    fn test_ntsc_scanline_count() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // NTSC should have exactly 262 scanlines (0-261)
        // Run through an entire frame and verify we hit scanline 0 after scanline 261
        timing.scanline = NTSC_PRERENDER_SCANLINE;
        timing.pixel = LAST_DOT;

        timing.tick(false); // Advance to next scanline
        assert_eq!(
            timing.scanline(),
            FIRST_VISIBLE_SCANLINE,
            "NTSC should wrap from scanline 261 to 0"
        );
        assert_eq!(timing.frame_count(), 1, "Frame count should increment");
    }

    #[test]
    fn test_pal_scanline_count() {
        let mut timing = Timing::new(TvSystem::Pal);

        // PAL should have exactly 312 scanlines (0-311)
        // Run through an entire frame and verify we hit scanline 0 after scanline 311
        timing.scanline = PAL_PRERENDER_SCANLINE;
        timing.pixel = LAST_DOT;

        timing.tick(false); // Advance to next scanline
        assert_eq!(
            timing.scanline(),
            FIRST_VISIBLE_SCANLINE,
            "PAL should wrap from scanline 311 to 0"
        );
        assert_eq!(timing.frame_count(), 1, "Frame count should increment");
    }

    #[test]
    fn test_dots_per_scanline() {
        let _timing = Timing::new(TvSystem::Ntsc);

        // Both NTSC and PAL use 341 dots per scanline (0-340)
        // This is already verified by PIXELS_PER_SCANLINE constant
        assert_eq!(
            PIXELS_PER_SCANLINE, 341,
            "Should have 341 pixels per scanline"
        );
    }

    #[test]
    fn test_ntsc_odd_frame_skip() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // Odd frame with rendering enabled: skip dot 340 on pre-render scanline
        // Set up: odd frame (frame_count = 1), pre-render scanline 261, dot 339
        timing.frame_count = 1; // Odd frame
        timing.scanline = NTSC_PRERENDER_SCANLINE;
        timing.pixel = ODD_FRAME_SKIP_DOT;

        // Skip decision uses a delayed rendering-enabled state.
        timing.rendering_enabled_d2 = true;

        let skipped = timing.tick(true); // rendering_enabled = true
        assert!(
            skipped,
            "Should skip dot 340 on odd NTSC frame with rendering enabled"
        );
        assert_eq!(timing.scanline(), FIRST_VISIBLE_SCANLINE, "Should jump to scanline 0");
        assert_eq!(timing.pixel(), FIRST_DOT, "Should jump to pixel 0");
        assert_eq!(timing.frame_count(), 2, "Frame count should increment");
    }

    #[test]
    fn test_ntsc_even_frame_no_skip() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // Even frame: no skip, normal 341 dots
        timing.frame_count = 0; // Even frame
        timing.scanline = NTSC_PRERENDER_SCANLINE;
        timing.pixel = ODD_FRAME_SKIP_DOT;

        timing.rendering_enabled_d2 = true;

        let skipped = timing.tick(true);
        assert!(!skipped, "Should not skip on even NTSC frame");
        assert_eq!(timing.pixel(), LAST_DOT, "Should advance to pixel 340");
    }

    #[test]
    fn test_pal_no_frame_skip() {
        let mut timing = Timing::new(TvSystem::Pal);

        // PAL never skips frames
        timing.frame_count = 1; // Odd frame
        timing.scanline = PAL_PRERENDER_SCANLINE;
        timing.pixel = ODD_FRAME_SKIP_DOT;

        let skipped = timing.tick(true);
        assert!(!skipped, "PAL should never skip frames");
        assert_eq!(timing.pixel(), LAST_DOT, "Should advance to pixel 340");
    }

    #[test]
    fn test_ntsc_cycles_per_frame() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // NTSC even frame: 262 scanlines * 341 dots = 89342 cycles
        // Run through an entire even frame
        let start_cycles = timing.total_cycles();

        // Simulate entire frame (even frame, rendering disabled to avoid skip)
        for _ in 0..262 {
            for _ in 0..PIXELS_PER_SCANLINE {
                timing.tick(false);
            }
        }

        let cycles_elapsed = timing.total_cycles() - start_cycles;
        assert_eq!(
            cycles_elapsed, 89342,
            "NTSC even frame should have 89342 cycles (262 * 341)"
        );
        assert_eq!(timing.scanline(), 0, "Should wrap back to scanline 0");
        assert_eq!(timing.pixel(), 0, "Should wrap back to pixel 0");
        assert_eq!(timing.frame_count(), 1, "Frame count should be 1");
    }

    #[test]
    fn test_ntsc_odd_frame_cycles() {
        let mut timing = Timing::new(TvSystem::Ntsc);
        timing.frame_count = 1; // Start on odd frame

        // NTSC odd frame with rendering: 262 scanlines * 341 dots - 1 (skip) = 89341 cycles
        let start_cycles = timing.total_cycles();

        // Simulate entire odd frame with rendering enabled
        for scanline in 0..262 {
            let dots = if scanline == NTSC_PRERENDER_SCANLINE { 
                // Pre-render scanline skips pixel 340 on odd frames with rendering enabled,
                // so we only loop up to pixel 339 (ODD_FRAME_SKIP_DOT)
                LAST_DOT 
            } else { 
                PIXELS_PER_SCANLINE 
            };
            for _ in 0..dots {
                timing.tick(true);
            }
        }

        let cycles_elapsed = timing.total_cycles() - start_cycles;
        assert_eq!(
            cycles_elapsed, 89341,
            "NTSC odd frame with rendering should have 89341 cycles (89342 - 1)"
        );
        assert_eq!(timing.scanline(), FIRST_VISIBLE_SCANLINE, "Should wrap back to scanline 0");
        assert_eq!(timing.pixel(), FIRST_DOT, "Should wrap back to pixel 0");
        assert_eq!(timing.frame_count(), 2, "Frame count should be 2");
    }

    #[test]
    fn test_pal_cycles_per_frame() {
        let mut timing = Timing::new(TvSystem::Pal);

        // PAL: 312 scanlines * 341 dots = 106392 cycles per frame
        let start_cycles = timing.total_cycles();

        // Simulate entire frame
        for _ in 0..312 {
            for _ in 0..PIXELS_PER_SCANLINE {
                timing.tick(false);
            }
        }

        let cycles_elapsed = timing.total_cycles() - start_cycles;
        assert_eq!(
            cycles_elapsed, 106392,
            "PAL frame should have 106392 cycles (312 * 341)"
        );
        assert_eq!(timing.scanline(), 0, "Should wrap back to scanline 0");
        assert_eq!(timing.pixel(), 0, "Should wrap back to pixel 0");
        assert_eq!(timing.frame_count(), 1, "Frame count should be 1");
    }

    #[test]
    fn test_frame_counter_wraparound() {
        let mut timing = Timing::new(TvSystem::Ntsc);

        // Verify frame counter increments properly
        assert_eq!(timing.frame_count(), 0);

        // Complete one frame
        for _ in 0..(262 * (PIXELS_PER_SCANLINE as usize)) {
            timing.tick(false);
        }
        assert_eq!(timing.frame_count(), 1);

        // Complete another frame
        for _ in 0..(262 * (PIXELS_PER_SCANLINE as usize)) {
            timing.tick(false);
        }
        assert_eq!(timing.frame_count(), 2);

        // Frame counter should continue incrementing (wraps at u64::MAX)
        timing.frame_count = u64::MAX - 1;
        for _ in 0..(262 * (PIXELS_PER_SCANLINE as usize)) {
            timing.tick(false);
        }
        assert_eq!(timing.frame_count(), u64::MAX);
    }
}
